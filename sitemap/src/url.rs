use crate::alternate::Alternate;
use crate::date::Date;
use crate::error::{Error, Result};
use crate::image::{self, Image};
use crate::limit;
use crate::location::location;
use crate::news::News;
use crate::video::Video;
use crate::writer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeFrequency {
  Always,
  Hourly,
  Daily,
  Weekly,
  Monthly,
  Yearly,
  Never,
}

impl ChangeFrequency {
  fn render(&self) -> &'static str {
    match self {
      Self::Always => "always",
      Self::Hourly => "hourly",
      Self::Daily => "daily",
      Self::Weekly => "weekly",
      Self::Monthly => "monthly",
      Self::Yearly => "yearly",
      Self::Never => "never",
    }
  }
}

/// A `<url>` entry, with the image, video, news and alternate-link extensions
/// hanging off it.
#[derive(Debug, Clone, PartialEq)]
pub struct Url {
  loc: String,
  lastmod: Option<Date>,
  changefreq: Option<ChangeFrequency>,
  priority: Option<f32>,
  alternates: Vec<Alternate>,
  images: Vec<Image>,
  videos: Vec<Video>,
  news: Option<News>,
}

impl Url {
  pub fn builder(loc: impl Into<String>) -> UrlBuilder {
    UrlBuilder {
      loc: loc.into(),
      lastmod: None,
      changefreq: None,
      priority: None,
      alternates: Vec::new(),
      images: Vec::new(),
      videos: Vec::new(),
      news: None,
    }
  }

  pub fn loc(&self) -> &str {
    &self.loc
  }

  pub fn lastmod(&self) -> Option<Date> {
    self.lastmod
  }

  pub(crate) fn has_alternates(&self) -> bool {
    !self.alternates.is_empty()
  }

  pub(crate) fn has_images(&self) -> bool {
    !self.images.is_empty()
  }

  pub(crate) fn has_videos(&self) -> bool {
    !self.videos.is_empty()
  }

  pub(crate) fn has_news(&self) -> bool {
    self.news.is_some()
  }

  pub(crate) fn write(&self, out: &mut String, depth: usize) {
    let inner = depth + 1;

    writer::open(out, depth, "url");
    writer::tag(out, inner, "loc", &self.loc);

    for alternate in &self.alternates {
      alternate.write(out, inner);
    }

    if let Some(lastmod) = &self.lastmod {
      writer::tag(out, inner, "lastmod", &lastmod.render());
    }

    if let Some(changefreq) = &self.changefreq {
      writer::tag(out, inner, "changefreq", changefreq.render());
    }

    if let Some(priority) = self.priority {
      writer::tag(out, inner, "priority", &priority.to_string());
    }

    for image in &self.images {
      image.write(out, inner);
    }

    for video in &self.videos {
      video.write(out, inner);
    }

    if let Some(news) = &self.news {
      news.write(out, inner);
    }

    writer::close(out, depth, "url");
  }
}

pub struct UrlBuilder {
  loc: String,
  lastmod: Option<Date>,
  changefreq: Option<ChangeFrequency>,
  priority: Option<f32>,
  alternates: Vec<Alternate>,
  images: Vec<Image>,
  videos: Vec<Video>,
  news: Option<News>,
}

impl UrlBuilder {
  pub fn lastmod(mut self, at: impl Into<Date>) -> Self {
    self.lastmod = Some(at.into());
    self
  }

  pub fn changefreq(mut self, changefreq: ChangeFrequency) -> Self {
    self.changefreq = Some(changefreq);
    self
  }

  pub fn priority(mut self, priority: f32) -> Self {
    self.priority = Some(priority);
    self
  }

  pub fn alternates(mut self, alternates: Vec<Alternate>) -> Self {
    self.alternates = alternates;
    self
  }

  pub fn images(mut self, images: Vec<Image>) -> Self {
    self.images = images;
    self
  }

  pub fn videos(mut self, videos: Vec<Video>) -> Self {
    self.videos = videos;
    self
  }

  pub fn news(mut self, news: News) -> Self {
    self.news = Some(news);
    self
  }

  pub fn build(self) -> Result<Url> {
    if let Some(priority) = self.priority
      && !(limit::MIN_PRIORITY..=limit::MAX_PRIORITY).contains(&priority)
    {
      return Err(Error::PriorityOutOfRange { priority });
    }

    image::check_count(self.images.len())?;

    Ok(Url {
      loc: location(self.loc)?,
      lastmod: self.lastmod,
      changefreq: self.changefreq,
      priority: self.priority,
      alternates: self.alternates,
      images: self.images,
      videos: self.videos,
      news: self.news,
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use chrono::NaiveDate;

  #[test]
  fn a_bare_url_writes_only_its_loc() {
    let mut out = String::new();
    Url::builder("https://example.com/a")
      .build()
      .unwrap()
      .write(&mut out, 0);

    assert_eq!(out, "<url>\n  <loc>https://example.com/a</loc>\n</url>\n");
  }

  #[test]
  fn the_core_elements_keep_their_documented_order() {
    let mut out = String::new();
    Url::builder("https://example.com/a")
      .lastmod(NaiveDate::from_ymd_opt(2026, 6, 1).unwrap())
      .changefreq(ChangeFrequency::Weekly)
      .priority(0.8)
      .build()
      .unwrap()
      .write(&mut out, 0);

    let loc = out.find("<loc>").unwrap();
    let lastmod = out.find("<lastmod>").unwrap();
    let changefreq = out.find("<changefreq>").unwrap();
    let priority = out.find("<priority>").unwrap();

    assert!(loc < lastmod && lastmod < changefreq && changefreq < priority);
  }

  #[test]
  fn priority_is_bounded_at_both_ends() {
    assert_eq!(
      Url::builder("https://example.com/a")
        .priority(1.1)
        .build()
        .unwrap_err(),
      Error::PriorityOutOfRange { priority: 1.1 }
    );
    assert_eq!(
      Url::builder("https://example.com/a")
        .priority(-0.1)
        .build()
        .unwrap_err(),
      Error::PriorityOutOfRange { priority: -0.1 }
    );
    assert!(Url::builder("https://example.com/a").priority(0.0).build().is_ok());
    assert!(Url::builder("https://example.com/a").priority(1.0).build().is_ok());
  }

  #[test]
  fn images_are_capped_at_a_thousand() {
    let images: Vec<Image> = (0..1_001)
      .map(|n| Image::new(format!("https://example.com/{n}.png")).unwrap())
      .collect();

    assert_eq!(
      Url::builder("https://example.com/a").images(images).build().unwrap_err(),
      Error::TooManyImages { count: 1_001 }
    );
  }

  #[test]
  fn a_loc_is_escaped() {
    let mut out = String::new();
    Url::builder("https://example.com/a?x=1&y=2")
      .build()
      .unwrap()
      .write(&mut out, 0);

    assert!(out.contains("<loc>https://example.com/a?x=1&amp;y=2</loc>"));
  }

  #[test]
  fn alternates_sit_directly_after_the_loc() {
    let mut out = String::new();
    Url::builder("https://example.com/a")
      .alternates(vec![Alternate::new("de", "https://example.com/de/a").unwrap()])
      .lastmod(NaiveDate::from_ymd_opt(2026, 6, 1).unwrap())
      .build()
      .unwrap()
      .write(&mut out, 0);

    assert!(out.find("<xhtml:link").unwrap() < out.find("<lastmod>").unwrap());
  }
}
