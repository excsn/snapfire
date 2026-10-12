use crate::error::{Error, Result};
use crate::location::location;
use crate::writer;

/// An `<image:image>` entry.
///
/// Only `loc` still carries weight: Google dropped support for `caption`,
/// `geo_location`, `title` and `license` in 2022. They remain in the schema, so
/// they remain writable here.
#[derive(Debug, Clone, PartialEq)]
pub struct Image {
  loc: String,
  caption: Option<String>,
  geo_location: Option<String>,
  title: Option<String>,
  license: Option<String>,
}

impl Image {
  pub fn new(loc: impl Into<String>) -> Result<Self> {
    Ok(Self {
      loc: location(loc.into())?,
      caption: None,
      geo_location: None,
      title: None,
      license: None,
    })
  }

  pub fn caption(mut self, caption: impl Into<String>) -> Self {
    self.caption = Some(caption.into());
    self
  }

  pub fn geo_location(mut self, geo_location: impl Into<String>) -> Self {
    self.geo_location = Some(geo_location.into());
    self
  }

  pub fn title(mut self, title: impl Into<String>) -> Self {
    self.title = Some(title.into());
    self
  }

  pub fn license(mut self, license: impl Into<String>) -> Result<Self> {
    self.license = Some(location(license.into())?);
    Ok(self)
  }

  pub fn loc(&self) -> &str {
    &self.loc
  }

  pub(crate) fn write(&self, out: &mut String, depth: usize) {
    writer::open(out, depth, "image:image");
    writer::tag(out, depth + 1, "image:loc", &self.loc);

    if let Some(caption) = &self.caption {
      writer::tag(out, depth + 1, "image:caption", caption);
    }

    if let Some(geo_location) = &self.geo_location {
      writer::tag(out, depth + 1, "image:geo_location", geo_location);
    }

    if let Some(title) = &self.title {
      writer::tag(out, depth + 1, "image:title", title);
    }

    if let Some(license) = &self.license {
      writer::tag(out, depth + 1, "image:license", license);
    }

    writer::close(out, depth, "image:image");
  }
}

pub(crate) fn check_count(count: usize) -> Result<()> {
  if count > crate::limit::MAX_IMAGES_PER_URL {
    return Err(Error::TooManyImages { count });
  }

  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn only_loc_is_written_when_nothing_else_is_set() {
    let mut out = String::new();
    Image::new("https://example.com/a.png").unwrap().write(&mut out, 0);

    assert_eq!(
      out,
      "<image:image>\n  <image:loc>https://example.com/a.png</image:loc>\n</image:image>\n"
    );
  }

  #[test]
  fn the_legacy_fields_are_written_when_set() {
    let mut out = String::new();
    Image::new("https://example.com/a.png")
      .unwrap()
      .caption("A caption")
      .title("A title")
      .write(&mut out, 0);

    assert!(out.contains("<image:caption>A caption</image:caption>"));
    assert!(out.contains("<image:title>A title</image:title>"));
  }

  #[test]
  fn an_empty_loc_is_rejected() {
    assert_eq!(Image::new("").unwrap_err(), Error::LocationEmpty);
  }
}
