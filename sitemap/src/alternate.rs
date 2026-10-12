use crate::error::Result;
use crate::location::location;
use crate::writer;

/// An `<xhtml:link rel="alternate">`, naming the same page in another language
/// or region.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alternate {
  hreflang: String,
  href: String,
}

impl Alternate {
  pub fn new(hreflang: impl Into<String>, href: impl Into<String>) -> Result<Self> {
    Ok(Self {
      hreflang: hreflang.into(),
      href: location(href.into())?,
    })
  }

  pub fn hreflang(&self) -> &str {
    &self.hreflang
  }

  pub fn href(&self) -> &str {
    &self.href
  }

  pub(crate) fn write(&self, out: &mut String, depth: usize) {
    writer::empty_tag(
      out,
      depth,
      "xhtml:link",
      &[("rel", "alternate"), ("hreflang", &self.hreflang), ("href", &self.href)],
    );
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn an_alternate_is_an_empty_element() {
    let mut out = String::new();

    Alternate::new("de", "https://example.com/deutsch/")
      .unwrap()
      .write(&mut out, 0);

    assert_eq!(
      out,
      "<xhtml:link rel=\"alternate\" hreflang=\"de\" href=\"https://example.com/deutsch/\" />\n"
    );
  }
}
