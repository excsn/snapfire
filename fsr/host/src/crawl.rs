//! `[crawl]`: the `robots.txt` the host answers and the sitemap it builds by
//! asking itself for every page a crawler could reach.

use snapfire_sitemap::{limit, Alternate, Date, Error as SitemapError, Sitemap, SitemapIndex, Url, UrlSet};

use crate::config::CrawlSection;
use crate::locale::Source;
use crate::prerender::fill_pattern;
use crate::*;

/// What `[crawl]` resolved to against this host: the paths it answers, the
/// robots text and the sitemap, built on the first request that asks for it
/// and again whenever the application calls `Host::refresh_sitemap`.
pub(crate) struct Crawl {
  origin: String,
  robots: Option<(String, String)>,
  sitemap: Option<String>,
  built: tokio::sync::RwLock<Option<Arc<Result<Vec<(String, String)>, String>>>>,
  building: tokio::sync::Mutex<()>,
}

impl Crawl {
  /// `own_at` is this host's `[site] at` when it is a site, whose sitemap
  /// sits under the prefix and which serves no `robots.txt`.
  pub(crate) fn new(section: &CrawlSection, origin: &str, own_at: Option<&str>, auth: bool) -> Self {
    let sitemap = section.sitemap.as_ref().map(|path| format!("{}{path}", own_at.unwrap_or("")));
    let robots = section.robots.as_ref().filter(|_| own_at.is_none()).map(|path| {
      let mut text = String::from("User-agent: *\nAllow: /\nDisallow: /__fsr/\nDisallow: /_sf/\n");
      if auth {
        text.push_str("Disallow: /auth/\n");
      }
      for path in &section.disallow {
        text.push_str(&format!("Disallow: {path}\n"));
      }
      if let Some(sitemap) = &sitemap {
        text.push_str(&format!("Sitemap: {origin}{sitemap}\n"));
      }
      (path.clone(), text)
    });
    Self { origin: origin.to_owned(), robots, sitemap, built: tokio::sync::RwLock::new(None), building: tokio::sync::Mutex::new(()) }
  }

  /// The robots text when `path` is where it is served.
  pub(crate) fn robots(&self, path: &str) -> Option<&str> {
    self.robots.as_ref().filter(|(at, _)| at == path).map(|(_, text)| text.as_str())
  }

  /// Whether `path` is this host's sitemap or one of the files its index names.
  pub(crate) fn owns(&self, path: &str) -> bool {
    let Some(sitemap) = &self.sitemap else { return false };
    path == sitemap || part_number(sitemap, path).is_some()
  }

  pub(crate) fn sitemap_path(&self) -> Option<&str> {
    self.sitemap.as_deref()
  }

  pub(crate) fn origin(&self) -> &str {
    &self.origin
  }

  /// The files as last built, building them first when nothing has been.
  pub(crate) async fn built<F, Fut>(&self, build: F) -> Arc<Result<Vec<(String, String)>, String>>
  where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<Vec<(String, String)>, String>>,
  {
    if let Some(held) = self.built.read().await.clone() {
      return held;
    }
    let _one = self.building.lock().await;
    if let Some(held) = self.built.read().await.clone() {
      return held;
    }
    let fresh = Arc::new(build().await);
    *self.built.write().await = Some(fresh.clone());
    fresh
  }

  /// Builds the files again and swaps them in. Requests meanwhile get the
  /// files as they were; a build that fails leaves them as they were.
  pub(crate) async fn rebuild<F, Fut>(&self, build: F) -> Result<(), String>
  where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<Vec<(String, String)>, String>>,
  {
    let _one = self.building.lock().await;
    let fresh = build().await;
    let failed = fresh.as_ref().err().cloned();
    let mut held = self.built.write().await;
    match failed {
      Some(why) if held.is_some() => Err(why),
      Some(why) => {
        *held = Some(Arc::new(Err(why.clone())));
        Err(why)
      }
      None => {
        *held = Some(Arc::new(fresh));
        Ok(())
      }
    }
  }
}

/// `n` when `path` is `<stem>-<n>.xml` beside `sitemap`.
fn part_number(sitemap: &str, path: &str) -> Option<usize> {
  let stem = sitemap.strip_suffix(".xml").unwrap_or(sitemap);
  path.strip_prefix(stem)?.strip_prefix('-')?.strip_suffix(".xml")?.parse().ok().filter(|n| *n > 0)
}

fn part_path(sitemap: &str, n: usize) -> String {
  let stem = sitemap.strip_suffix(".xml").unwrap_or(sitemap);
  format!("{stem}-{n}.xml")
}

/// A BCP 47 tag from a configured locale, `fr_FR` as `fr-FR`.
fn hreflang(tag: &str) -> String {
  tag.replace('_', "-")
}

/// What a crawler reads off a document before the body: whether it may index
/// it and when it last changed.
#[derive(Debug, PartialEq)]
struct Reading {
  noindex: bool,
  modified: Option<Date>,
}

/// A `robots` meta or an `X-Robots-Tag` of `noindex` or `none` keeps the page
/// out. Its date is the head's `article:modified_time`, or the
/// `Last-Modified` header when the head names none.
fn read(headers: &header::HeaderMap, html: &str) -> Reading {
  let says = |value: &str| value.split(',').any(|part| matches!(part.trim().to_ascii_lowercase().as_str(), "noindex" | "none"));
  let mut noindex = headers.get_all("x-robots-tag").iter().filter_map(|v| v.to_str().ok()).any(says);
  let mut modified = None;
  let head = html.find("</head>").map_or(html, |end| &html[..end]);
  let mut rest = head;
  while let Some(start) = rest.find("<meta") {
    let tag = &rest[start..];
    let end = tag.find('>').unwrap_or(tag.len());
    let tag = &tag[..end];
    if attribute(tag, "name").is_some_and(|name| name.eq_ignore_ascii_case("robots")) && attribute(tag, "content").is_some_and(says) {
      noindex = true;
    }
    if modified.is_none() && attribute(tag, "property") == Some("article:modified_time") {
      modified = attribute(tag, "content").and_then(Date::parse);
    }
    rest = &rest[start + end..];
  }
  let modified = modified.or_else(|| headers.get(header::LAST_MODIFIED).and_then(|v| v.to_str().ok()).and_then(Date::parse_http));
  Reading { noindex, modified }
}

fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
  let at = tag.find(&format!(" {name}=\""))? + name.len() + 3;
  let value = &tag[at..];
  Some(&value[..value.find('"')?])
}

impl Host {
  /// Builds the sitemap again from what the host serves now and swaps it in,
  /// for an application that schedules its own refresh. The host never calls
  /// it. Requests meanwhile get the sitemap as it was, and a build that fails
  /// keeps it and returns why. Nothing to do without `[crawl] sitemap`.
  pub async fn refresh_sitemap(&self) -> Result<(), HostError> {
    let t = self.tables();
    let Some(crawl) = t.crawl.as_ref().filter(|crawl| crawl.sitemap_path().is_some()) else {
      return Ok(());
    };
    crawl.rebuild(|| self.build_sitemap(&t, crawl)).await.map_err(HostError::Sitemap)
  }

  /// The answer for a crawl path, `None` when `path` is none of them.
  pub(crate) async fn crawl_response(&self, t: &Tables, req: &Request<Bytes>, path: &str) -> Option<Response<Body>> {
    let crawl = t.crawl.as_ref()?;
    if req.method() != Method::GET && req.method() != Method::HEAD {
      return None;
    }
    if let Some(text) = crawl.robots(path) {
      return Some(text_response(StatusCode::OK, text.to_owned()));
    }
    if !crawl.owns(path) {
      return None;
    }
    let built = crawl.built(|| self.build_sitemap(t, crawl)).await;
    Some(match built.as_ref() {
      Ok(files) => match files.iter().find(|(at, _)| at == path) {
        Some((_, xml)) => Response::builder()
          .header(header::CONTENT_TYPE, "application/xml; charset=utf-8")
          .body(http_body_util::Full::new(Bytes::from(xml.clone())).map_err(|never| match never {}).boxed_unsync())
          .expect("an xml response"),
        None => text_response(StatusCode::NOT_FOUND, format!("no route: {path}")),
      },
      Err(why) => text_response(StatusCode::INTERNAL_SERVER_ERROR, why.clone()),
    })
  }

  /// Every page an anonymous visitor is served, asked of this host through its
  /// own front door so the middleware, the loaders and the head all have
  /// their say. A page is listed when it answers 200 with a document that
  /// does not say `noindex`.
  async fn build_sitemap(&self, t: &Tables, crawl: &Crawl) -> Result<Vec<(String, String)>, String> {
    let sitemap = crawl.sitemap_path().expect("a sitemap is built only where one is served");
    let tags: Vec<String> = if t.locales.order.contains(&Source::Prefix) {
      t.locales.supported.clone()
    } else {
      vec![t.locales.default.clone()]
    };
    let mut pages: Vec<Vec<(String, String, Option<Date>)>> = Vec::new();
    for (pattern, _) in &t.report.app.routes {
      let enumerated = t.app.paths.get(pattern);
      if pattern.contains('{') && enumerated.is_none() {
        continue;
      }
      let mut by_path: Vec<(String, Vec<(String, String, Option<Date>)>)> = Vec::new();
      for tag in &tags {
        let locale = t.locales.locale(tag);
        let paths = match enumerated {
          Some(enumerated) => {
            let ctx = self.ctx(t, Incoming::anonymous(SessionCell::default()), Params::new(), Params::new(), "/", locale.clone());
            let sets = enumerated.paths(&ctx).await.map_err(|e| format!("sitemap: the paths of `{pattern}` failed: {}", e.message))?;
            sets.iter().map(|set| fill_pattern(pattern, set)).collect::<Result<Vec<_>, _>>().map_err(|e| format!("sitemap: {e}"))?
          }
          None => vec![pattern.clone()],
        };
        for path in paths {
          let served = if locale.is_default { path.clone() } else { format!("/{tag}{}", path.trim_end_matches('/')) };
          if let Some(modified) = self.indexable(crawl, &served).await {
            match by_path.iter_mut().find(|(p, _)| *p == path) {
              Some((_, variants)) => variants.push((tag.clone(), served, modified)),
              None => by_path.push((path, vec![(tag.clone(), served, modified)])),
            }
          }
        }
      }
      pages.extend(by_path.into_iter().map(|(_, variants)| variants));
    }
    let origin = crawl.origin();
    let default = &t.locales.default;
    let mut urls = Vec::new();
    for variants in &pages {
      let alternates = if tags.len() > 1 {
        let mut alternates = variants.iter().map(|(tag, served, _)| Alternate::new(hreflang(tag), format!("{origin}{served}"))).collect::<Result<Vec<_>, _>>().map_err(sitemap_error)?;
        if let Some((_, served, _)) = variants.iter().find(|(tag, _, _)| tag == default) {
          alternates.push(Alternate::new("x-default", format!("{origin}{served}")).map_err(sitemap_error)?);
        }
        alternates
      } else {
        Vec::new()
      };
      for (_, served, modified) in variants {
        let url = Url::builder(format!("{origin}{served}"));
        let url = match modified {
          Some(at) => url.lastmod(*at),
          None => url,
        };
        let url = if alternates.is_empty() { url } else { url.alternates(alternates.clone()) };
        urls.push(url.build().map_err(sitemap_error)?);
      }
    }
    let mut parts = Vec::new();
    split(urls, &mut parts)?;
    if parts.len() == 1 {
      return Ok(vec![(sitemap.to_owned(), parts.remove(0).0)]);
    }
    let mut files = Vec::new();
    let mut named = Vec::new();
    for (i, (xml, newest)) in parts.into_iter().enumerate() {
      let at = part_path(sitemap, i + 1);
      let entry = Sitemap::new(format!("{origin}{at}")).map_err(sitemap_error)?;
      named.push(match newest {
        Some(at) => entry.lastmod(at),
        None => entry,
      });
      files.push((at, xml));
    }
    files.insert(0, (sitemap.to_owned(), SitemapIndex::new(named).and_then(|index| index.to_xml()).map_err(sitemap_error)?));
    Ok(files)
  }

  /// `Some` with the page's date when an anonymous GET of `path` is a
  /// document a crawler may index, `None` when it is not.
  async fn indexable(&self, crawl: &Crawl, path: &str) -> Option<Option<Date>> {
    let mut req = Request::builder().method(Method::GET).uri(path).header(header::ACCEPT, "text/html");
    if let Some(host) = crawl.origin().split_once("://").map(|(_, host)| host) {
      req = req.header(header::HOST, host);
    }
    let req = req.body(Bytes::new()).ok()?;
    let response = Box::pin(self.handle_in(req)).await;
    let html = response.headers().get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).is_some_and(|v| v.starts_with("text/html"));
    if response.status() != StatusCode::OK || !html {
      return None;
    }
    let (parts, body) = response.into_parts();
    let collected = body.collect().await.ok()?;
    let reading = read(&parts.headers, &String::from_utf8_lossy(&collected.to_bytes()));
    (!reading.noindex).then_some(reading.modified)
  }
}

fn sitemap_error(e: SitemapError) -> String {
  format!("sitemap: {e}")
}

/// `urls` as one document per `limit::MAX_URLS`, halving a document that
/// comes out past the size limit, each with the newest `lastmod` it holds.
fn split(urls: Vec<Url>, parts: &mut Vec<(String, Option<Date>)>) -> Result<(), String> {
  if urls.len() > limit::MAX_URLS {
    let mut rest = urls;
    while !rest.is_empty() {
      let next = rest.split_off(rest.len().min(limit::MAX_URLS));
      split(std::mem::replace(&mut rest, next), parts)?;
    }
    return Ok(());
  }
  let count = urls.len();
  match UrlSet::new(urls.clone()).and_then(|set| set.to_xml()) {
    Ok(xml) => {
      let newest = urls.iter().filter_map(Url::lastmod).max_by_key(Date::as_instant);
      parts.push((xml, newest));
      Ok(())
    }
    Err(SitemapError::DocumentTooLarge { .. }) if count > 1 => {
      let mut first = urls;
      let second = first.split_off(count / 2);
      split(first, parts)?;
      split(second, parts)
    }
    Err(e) => Err(sitemap_error(e)),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_meta_or_a_header_saying_noindex_keeps_a_page_out() {
    let none = header::HeaderMap::new();
    assert!(read(&none, "<head><meta name=\"robots\" content=\"noindex, follow\"></head>").noindex);
    assert!(read(&none, "<head><meta name=\"ROBOTS\" content=\"none\"></head>").noindex);
    assert!(!read(&none, "<head><meta name=\"robots\" content=\"index\"></head><body><meta name=\"robots\" content=\"noindex\"></body>").noindex);
    assert!(!read(&none, "<head><meta name=\"description\" content=\"noindex\"></head>").noindex);
    let mut headers = header::HeaderMap::new();
    headers.insert("x-robots-tag", HeaderValue::from_static("noarchive, noindex"));
    assert!(read(&headers, "<head></head>").noindex);
  }

  #[test]
  fn the_head_date_wins_over_the_header_and_the_header_fills_in() {
    let mut headers = header::HeaderMap::new();
    headers.insert(header::LAST_MODIFIED, HeaderValue::from_static("Mon, 01 Jun 2026 09:30:00 GMT"));
    let dated = "<head><meta property=\"article:modified_time\" content=\"2026-07-04\"></head>";
    assert_eq!(read(&headers, dated).modified.map(|d| d.render()).as_deref(), Some("2026-07-04"));
    assert_eq!(read(&headers, "<head></head>").modified.map(|d| d.render()).as_deref(), Some("2026-06-01T09:30:00Z"));
    assert_eq!(read(&header::HeaderMap::new(), "<head><meta property=\"article:modified_time\" content=\"last week\"></head>").modified, None);
  }

  #[test]
  fn an_index_names_its_parts_beside_the_sitemap() {
    assert_eq!(part_path("/sitemap.xml", 2), "/sitemap-2.xml");
    assert_eq!(part_number("/sitemap.xml", "/sitemap-2.xml"), Some(2));
    assert_eq!(part_number("/sitemap.xml", "/sitemap-0.xml"), None);
    assert_eq!(part_number("/sitemap.xml", "/sitemap.xml"), None);
  }
}
