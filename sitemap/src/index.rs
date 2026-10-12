use crate::date::Date;
use crate::error::{Error, Result};
use crate::limit;
use crate::location::location;
use crate::namespace;
use crate::writer;

/// One `<sitemap>` entry of a `<sitemapindex>`.
#[derive(Debug, Clone, PartialEq)]
pub struct Sitemap {
  loc: String,
  lastmod: Option<Date>,
}

impl Sitemap {
  pub fn new(loc: impl Into<String>) -> Result<Self> {
    Ok(Self {
      loc: location(loc.into())?,
      lastmod: None,
    })
  }

  pub fn lastmod(mut self, at: impl Into<Date>) -> Self {
    self.lastmod = Some(at.into());
    self
  }

  pub fn loc(&self) -> &str {
    &self.loc
  }

  fn write(&self, out: &mut String, depth: usize) {
    writer::open(out, depth, "sitemap");
    writer::tag(out, depth + 1, "loc", &self.loc);

    if let Some(lastmod) = &self.lastmod {
      writer::tag(out, depth + 1, "lastmod", &lastmod.render());
    }

    writer::close(out, depth, "sitemap");
  }
}

/// A `<sitemapindex>` document.
#[derive(Debug, Clone, PartialEq)]
pub struct SitemapIndex {
  sitemaps: Vec<Sitemap>,
}

impl SitemapIndex {
  pub fn new(sitemaps: Vec<Sitemap>) -> Result<Self> {
    if sitemaps.len() > limit::MAX_SITEMAPS {
      return Err(Error::TooManySitemaps { count: sitemaps.len() });
    }

    Ok(Self { sitemaps })
  }

  pub fn sitemaps(&self) -> &[Sitemap] {
    &self.sitemaps
  }

  pub fn is_empty(&self) -> bool {
    self.sitemaps.is_empty()
  }

  pub fn len(&self) -> usize {
    self.sitemaps.len()
  }

  pub fn to_xml(&self) -> Result<String> {
    let mut out = String::from(namespace::DECLARATION);

    out.push_str("<sitemapindex xmlns=\"");
    out.push_str(namespace::SITEMAP);
    out.push_str("\">\n");

    for sitemap in &self.sitemaps {
      sitemap.write(&mut out, 1);
    }

    writer::close(&mut out, 0, "sitemapindex");

    namespace::check_size(out)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use chrono::NaiveDate;

  #[test]
  fn an_index_writes_each_child_with_its_lastmod() {
    let index = SitemapIndex::new(vec![
      Sitemap::new("https://example.com/sitemap-core.xml").unwrap(),
      Sitemap::new("https://example.com/sitemap-2026.xml")
        .unwrap()
        .lastmod(NaiveDate::from_ymd_opt(2026, 6, 1).unwrap()),
    ])
    .unwrap();

    let xml = index.to_xml().unwrap();

    assert!(xml.contains("<loc>https://example.com/sitemap-core.xml</loc>"));
    assert!(xml.contains("<loc>https://example.com/sitemap-2026.xml</loc>"));
    assert_eq!(xml.matches("<lastmod>").count(), 1);
    assert!(xml.ends_with("</sitemapindex>\n"));
  }

  #[test]
  fn more_than_fifty_thousand_children_are_rejected() {
    let sitemaps: Vec<Sitemap> = (0..50_001)
      .map(|n| Sitemap::new(format!("https://example.com/{n}.xml")).unwrap())
      .collect();

    assert_eq!(
      SitemapIndex::new(sitemaps).unwrap_err(),
      Error::TooManySitemaps { count: 50_001 }
    );
  }

  #[test]
  fn an_index_loc_is_escaped() {
    let index = SitemapIndex::new(vec![Sitemap::new("https://example.com/a.xml?v=1&w=2").unwrap()]).unwrap();

    assert!(index.to_xml().unwrap().contains("a.xml?v=1&amp;w=2"));
  }
}
