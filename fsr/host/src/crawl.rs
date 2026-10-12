//! `[crawl]`: the `robots.txt` the host answers and the sitemap it builds by
//! asking itself for every page a crawler could reach.

use snapfire_sitemap::{limit, Alternate, Error as SitemapError, Sitemap, SitemapIndex, Url, UrlSet};

use crate::config::CrawlSection;
use crate::locale::Source;
use crate::prerender::fill_pattern;
use crate::*;

/// What `[crawl]` resolved to against this host: the paths it answers, the
/// robots text and the sitemap, built on the first request that asks for it.
pub(crate) struct Crawl {
  origin: String,
  robots: Option<(String, String)>,
  sitemap: Option<String>,
  /// The sitemap path of each site in its own process, which that site
  /// answers and the shell forwards.
  remote: Vec<String>,
  built: tokio::sync::OnceCell<Result<Vec<(String, String)>, String>>,
}

impl Crawl {
  /// `own_at` is this host's `[site] at` when it is a site, whose sitemap
  /// sits under the prefix and which serves no `robots.txt`.
  pub(crate) fn new(section: &CrawlSection, origin: &str, own_at: Option<&str>, remote_at: &[String], auth: bool) -> Self {
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
    let remote = match &section.sitemap {
      Some(path) => remote_at.iter().map(|at| format!("{at}{path}")).collect(),
      None => Vec::new(),
    };
    Self { origin: origin.to_owned(), robots, sitemap, remote, built: tokio::sync::OnceCell::new() }
  }

  /// The robots text when `path` is where it is served.
  pub(crate) fn robots(&self, path: &str) -> Option<&str> {
    self.robots.as_ref().filter(|(at, _)| at == path).map(|(_, text)| text.as_str())
  }

  /// Whether `path` is a sitemap a site in its own process answers.
  pub(crate) fn is_remote(&self, path: &str) -> bool {
    self.remote.iter().any(|at| at == path)
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

  pub(crate) fn remote_paths(&self) -> &[String] {
    &self.remote
  }

  pub(crate) async fn built<F, Fut>(&self, build: F) -> &Result<Vec<(String, String)>, String>
  where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<Vec<(String, String)>, String>>,
  {
    self.built.get_or_init(build).await
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

/// Whether a document's head or headers tell a crawler not to index it.
fn noindex(headers: &header::HeaderMap, html: &str) -> bool {
  let says = |value: &str| value.split(',').any(|part| matches!(part.trim().to_ascii_lowercase().as_str(), "noindex" | "none"));
  if headers.get_all("x-robots-tag").iter().filter_map(|v| v.to_str().ok()).any(says) {
    return true;
  }
  let head = html.find("</head>").map_or(html, |end| &html[..end]);
  let mut rest = head;
  while let Some(start) = rest.find("<meta") {
    let tag = &rest[start..];
    let end = tag.find('>').unwrap_or(tag.len());
    let tag = &tag[..end];
    if attribute(tag, "name").is_some_and(|name| name.eq_ignore_ascii_case("robots")) && attribute(tag, "content").is_some_and(says) {
      return true;
    }
    rest = &rest[start + end..];
  }
  false
}

fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
  let at = tag.find(&format!(" {name}=\""))? + name.len() + 3;
  let value = &tag[at..];
  Some(&value[..value.find('"')?])
}

impl Host {
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
    Some(match built {
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
    let remote_at: Vec<&str> = t.sites.iter().filter(|site| site.remote.is_some()).map(|site| site.at.as_str()).collect();
    let mut pages: Vec<Vec<(String, String)>> = Vec::new();
    for (pattern, _) in &t.report.app.routes {
      if remote_at.iter().any(|at| pattern == at || pattern.starts_with(&format!("{at}/"))) {
        continue;
      }
      let enumerated = t.app.paths.get(pattern);
      if pattern.contains('{') && enumerated.is_none() {
        continue;
      }
      let mut by_path: Vec<(String, Vec<(String, String)>)> = Vec::new();
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
          if self.indexable(crawl, &served).await {
            match by_path.iter_mut().find(|(p, _)| *p == path) {
              Some((_, variants)) => variants.push((tag.clone(), served)),
              None => by_path.push((path, vec![(tag.clone(), served)])),
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
        let mut alternates = variants.iter().map(|(tag, served)| Alternate::new(hreflang(tag), format!("{origin}{served}"))).collect::<Result<Vec<_>, _>>().map_err(sitemap_error)?;
        if let Some((_, served)) = variants.iter().find(|(tag, _)| tag == default) {
          alternates.push(Alternate::new("x-default", format!("{origin}{served}")).map_err(sitemap_error)?);
        }
        alternates
      } else {
        Vec::new()
      };
      for (_, served) in variants {
        let url = Url::builder(format!("{origin}{served}"));
        let url = if alternates.is_empty() { url } else { url.alternates(alternates.clone()) };
        urls.push(url.build().map_err(sitemap_error)?);
      }
    }
    let mut parts = Vec::new();
    split(urls, &mut parts)?;
    if parts.len() == 1 && crawl.remote_paths().is_empty() {
      return Ok(vec![(sitemap.to_owned(), parts.remove(0))]);
    }
    let mut files = Vec::new();
    let mut named = Vec::new();
    for (i, xml) in parts.into_iter().enumerate() {
      let at = part_path(sitemap, i + 1);
      named.push(Sitemap::new(format!("{origin}{at}")).map_err(sitemap_error)?);
      files.push((at, xml));
    }
    for at in crawl.remote_paths() {
      named.push(Sitemap::new(format!("{origin}{at}")).map_err(sitemap_error)?);
    }
    files.insert(0, (sitemap.to_owned(), SitemapIndex::new(named).and_then(|index| index.to_xml()).map_err(sitemap_error)?));
    Ok(files)
  }

  /// Whether an anonymous GET of `path` is a document a crawler may index.
  async fn indexable(&self, crawl: &Crawl, path: &str) -> bool {
    let mut req = Request::builder().method(Method::GET).uri(path).header(header::ACCEPT, "text/html");
    if let Some(host) = crawl.origin().split_once("://").map(|(_, host)| host) {
      req = req.header(header::HOST, host);
    }
    let Ok(req) = req.body(Bytes::new()) else { return false };
    let response = Box::pin(self.handle_in(req)).await;
    let html = response.headers().get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).is_some_and(|v| v.starts_with("text/html"));
    if response.status() != StatusCode::OK || !html {
      return false;
    }
    let (parts, body) = response.into_parts();
    let Ok(collected) = body.collect().await else { return false };
    !noindex(&parts.headers, &String::from_utf8_lossy(&collected.to_bytes()))
  }
}

fn sitemap_error(e: SitemapError) -> String {
  format!("sitemap: {e}")
}

/// `urls` as one document per `limit::MAX_URLS`, halving a document that
/// comes out past the size limit.
fn split(urls: Vec<Url>, parts: &mut Vec<String>) -> Result<(), String> {
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
      parts.push(xml);
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
    assert!(noindex(&none, "<head><meta name=\"robots\" content=\"noindex, follow\"></head>"));
    assert!(noindex(&none, "<head><meta name=\"ROBOTS\" content=\"none\"></head>"));
    assert!(!noindex(&none, "<head><meta name=\"robots\" content=\"index\"></head><body><meta name=\"robots\" content=\"noindex\"></body>"));
    assert!(!noindex(&none, "<head><meta name=\"description\" content=\"noindex\"></head>"));
    let mut headers = header::HeaderMap::new();
    headers.insert("x-robots-tag", HeaderValue::from_static("noarchive, noindex"));
    assert!(noindex(&headers, "<head></head>"));
  }

  #[test]
  fn an_index_names_its_parts_beside_the_sitemap() {
    assert_eq!(part_path("/sitemap.xml", 2), "/sitemap-2.xml");
    assert_eq!(part_number("/sitemap.xml", "/sitemap-2.xml"), Some(2));
    assert_eq!(part_number("/sitemap.xml", "/sitemap-0.xml"), None);
    assert_eq!(part_number("/sitemap.xml", "/sitemap.xml"), None);
  }
}
