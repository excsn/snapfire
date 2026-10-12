use crate::error::{Error, Result};
use crate::limit;
use crate::news;
use crate::namespace;
use crate::url::Url;
use crate::writer;

/// A `<urlset>` document.
#[derive(Debug, Clone, PartialEq)]
pub struct UrlSet {
  urls: Vec<Url>,
}

impl UrlSet {
  pub fn new(urls: Vec<Url>) -> Result<Self> {
    if urls.len() > limit::MAX_URLS {
      return Err(Error::TooManyUrls { count: urls.len() });
    }

    news::check_count(urls.iter().filter(|url| url.has_news()).count())?;

    Ok(Self { urls })
  }

  pub fn urls(&self) -> &[Url] {
    &self.urls
  }

  pub fn is_empty(&self) -> bool {
    self.urls.is_empty()
  }

  pub fn len(&self) -> usize {
    self.urls.len()
  }

  /// Only the namespaces the document actually uses are declared, so a plain
  /// sitemap stays a plain sitemap.
  pub fn to_xml(&self) -> Result<String> {
    let mut out = String::from(namespace::DECLARATION);

    out.push_str("<urlset xmlns=\"");
    out.push_str(namespace::SITEMAP);
    out.push('"');

    if self.urls.iter().any(Url::has_alternates) {
      push_namespace(&mut out, "xhtml", namespace::XHTML);
    }

    if self.urls.iter().any(Url::has_images) {
      push_namespace(&mut out, "image", namespace::IMAGE);
    }

    if self.urls.iter().any(Url::has_videos) {
      push_namespace(&mut out, "video", namespace::VIDEO);
    }

    if self.urls.iter().any(Url::has_news) {
      push_namespace(&mut out, "news", namespace::NEWS);
    }

    out.push_str(">\n");

    for url in &self.urls {
      url.write(&mut out, 1);
    }

    writer::close(&mut out, 0, "urlset");

    namespace::check_size(out)
  }
}

fn push_namespace(out: &mut String, prefix: &str, uri: &str) {
  out.push_str(" xmlns:");
  out.push_str(prefix);
  out.push_str("=\"");
  out.push_str(uri);
  out.push('"');
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::alternate::Alternate;
  use crate::image::Image;
  use crate::news::{News, Publication};
  use crate::url::ChangeFrequency;
  use crate::video::Video;

  use chrono::NaiveDate;

  fn url(loc: &str) -> Url {
    Url::builder(loc).build().unwrap()
  }

  #[test]
  fn a_plain_document_declares_only_the_sitemap_namespace() {
    let xml = UrlSet::new(vec![url("https://example.com/a")]).unwrap().to_xml().unwrap();

    assert!(xml.contains("<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">"));
    assert!(!xml.contains("xmlns:image"));
    assert!(!xml.contains("xmlns:video"));
    assert!(!xml.contains("xmlns:news"));
    assert!(!xml.contains("xmlns:xhtml"));
  }

  #[test]
  fn each_extension_pulls_in_its_own_namespace() {
    let with_image = Url::builder("https://example.com/a")
      .images(vec![Image::new("https://example.com/a.png").unwrap()])
      .build()
      .unwrap();
    let with_video = Url::builder("https://example.com/b")
      .videos(vec![
        Video::builder("https://example.com/t.jpg", "T", "D")
          .content_loc("https://example.com/v.mp4")
          .build()
          .unwrap(),
      ])
      .build()
      .unwrap();
    let with_news = Url::builder("https://example.com/c")
      .news(News::new(
        Publication {
          name: "N".to_string(),
          language: "en".to_string(),
        },
        NaiveDate::from_ymd_opt(2026, 6, 1).unwrap(),
        "T",
      ))
      .build()
      .unwrap();
    let with_alternate = Url::builder("https://example.com/d")
      .alternates(vec![Alternate::new("de", "https://example.com/de/d").unwrap()])
      .build()
      .unwrap();

    let xml = UrlSet::new(vec![with_image, with_video, with_news, with_alternate])
      .unwrap()
      .to_xml()
      .unwrap();

    assert!(xml.contains("xmlns:image=\"http://www.google.com/schemas/sitemap-image/1.1\""));
    assert!(xml.contains("xmlns:video=\"http://www.google.com/schemas/sitemap-video/1.1\""));
    assert!(xml.contains("xmlns:news=\"http://www.google.com/schemas/sitemap-news/0.9\""));
    assert!(xml.contains("xmlns:xhtml=\"http://www.w3.org/1999/xhtml\""));
  }

  #[test]
  fn a_document_over_fifty_thousand_urls_is_rejected() {
    let urls: Vec<Url> = (0..50_001).map(|n| url(&format!("https://example.com/{n}"))).collect();

    assert_eq!(
      UrlSet::new(urls).unwrap_err(),
      Error::TooManyUrls { count: 50_001 }
    );
  }

  #[test]
  fn more_than_a_thousand_news_entries_are_rejected() {
    let urls: Vec<Url> = (0..1_001)
      .map(|n| {
        Url::builder(format!("https://example.com/{n}"))
          .news(News::new(
            Publication {
              name: "N".to_string(),
              language: "en".to_string(),
            },
            NaiveDate::from_ymd_opt(2026, 6, 1).unwrap(),
            "T",
          ))
          .build()
          .unwrap()
      })
      .collect();

    assert_eq!(UrlSet::new(urls).unwrap_err(), Error::TooMuchNews { count: 1_001 });
  }

  #[test]
  fn an_empty_document_is_still_well_formed() {
    let xml = UrlSet::new(vec![]).unwrap().to_xml().unwrap();

    assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"));
    assert!(xml.ends_with("</urlset>\n"));
  }

  #[test]
  fn a_full_document_round_trips_its_pieces() {
    let entry = Url::builder("https://example.com/a")
      .lastmod(NaiveDate::from_ymd_opt(2026, 6, 1).unwrap())
      .changefreq(ChangeFrequency::Monthly)
      .priority(0.7)
      .build()
      .unwrap();

    let xml = UrlSet::new(vec![entry]).unwrap().to_xml().unwrap();

    assert!(xml.contains("<lastmod>2026-06-01</lastmod>"));
    assert!(xml.contains("<changefreq>monthly</changefreq>"));
    assert!(xml.contains("<priority>0.7</priority>"));
  }
}
