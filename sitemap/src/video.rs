use crate::date::Date;
use crate::error::{Error, Result};
use crate::limit;
use crate::location::location;
use crate::writer;

/// Whether a `<video:restriction>` or `<video:platform>` list is an allowlist
/// or a denylist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Relationship {
  Allow,
  Deny,
}

impl Relationship {
  fn render(&self) -> &'static str {
    match self {
      Self::Allow => "allow",
      Self::Deny => "deny",
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Platform {
  Web,
  Mobile,
  Tv,
}

impl Platform {
  fn render(&self) -> &'static str {
    match self {
      Self::Web => "web",
      Self::Mobile => "mobile",
      Self::Tv => "tv",
    }
  }
}

/// A space-delimited list carrying an allow or deny relationship.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Restriction {
  pub relationship: Relationship,
  /// Uppercase ISO 3166 country codes.
  pub countries: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PlatformRestriction {
  pub relationship: Relationship,
  pub platforms: Vec<Platform>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Uploader {
  pub name: String,
  /// A url on the same domain as the `<loc>`.
  pub info: Option<String>,
}

/// A `<video:video>` entry.
#[derive(Debug, Clone, PartialEq)]
pub struct Video {
  thumbnail_loc: String,
  title: String,
  description: String,
  content_loc: Option<String>,
  player_loc: Option<String>,
  duration: Option<u32>,
  expiration_date: Option<Date>,
  rating: Option<f32>,
  view_count: Option<u64>,
  publication_date: Option<Date>,
  family_friendly: Option<bool>,
  restriction: Option<Restriction>,
  platform: Option<PlatformRestriction>,
  requires_subscription: Option<bool>,
  uploader: Option<Uploader>,
  live: Option<bool>,
  tags: Vec<String>,
  category: Option<String>,
}

impl Video {
  pub fn builder(
    thumbnail_loc: impl Into<String>,
    title: impl Into<String>,
    description: impl Into<String>,
  ) -> VideoBuilder {
    VideoBuilder {
      thumbnail_loc: thumbnail_loc.into(),
      title: title.into(),
      description: description.into(),
      content_loc: None,
      player_loc: None,
      duration: None,
      expiration_date: None,
      rating: None,
      view_count: None,
      publication_date: None,
      family_friendly: None,
      restriction: None,
      platform: None,
      requires_subscription: None,
      uploader: None,
      live: None,
      tags: Vec::new(),
      category: None,
    }
  }

  pub fn title(&self) -> &str {
    &self.title
  }

  pub(crate) fn write(&self, out: &mut String, depth: usize) {
    let inner = depth + 1;

    writer::open(out, depth, "video:video");
    writer::tag(out, inner, "video:thumbnail_loc", &self.thumbnail_loc);
    writer::tag(out, inner, "video:title", &self.title);
    writer::tag(out, inner, "video:description", &self.description);

    if let Some(content_loc) = &self.content_loc {
      writer::tag(out, inner, "video:content_loc", content_loc);
    }

    if let Some(player_loc) = &self.player_loc {
      writer::tag(out, inner, "video:player_loc", player_loc);
    }

    if let Some(duration) = self.duration {
      writer::tag(out, inner, "video:duration", &duration.to_string());
    }

    if let Some(expiration_date) = &self.expiration_date {
      writer::tag(out, inner, "video:expiration_date", &expiration_date.render());
    }

    if let Some(rating) = self.rating {
      writer::tag(out, inner, "video:rating", &rating.to_string());
    }

    if let Some(view_count) = self.view_count {
      writer::tag(out, inner, "video:view_count", &view_count.to_string());
    }

    if let Some(publication_date) = &self.publication_date {
      writer::tag(out, inner, "video:publication_date", &publication_date.render());
    }

    if let Some(family_friendly) = self.family_friendly {
      writer::tag(out, inner, "video:family_friendly", writer::yes_no(family_friendly));
    }

    if let Some(restriction) = &self.restriction {
      writer::tag_with(
        out,
        inner,
        "video:restriction",
        &[("relationship", restriction.relationship.render())],
        &restriction.countries.join(" "),
      );
    }

    if let Some(platform) = &self.platform {
      let platforms: Vec<&str> = platform.platforms.iter().map(Platform::render).collect();

      writer::tag_with(
        out,
        inner,
        "video:platform",
        &[("relationship", platform.relationship.render())],
        &platforms.join(" "),
      );
    }

    if let Some(requires_subscription) = self.requires_subscription {
      writer::tag(
        out,
        inner,
        "video:requires_subscription",
        writer::yes_no(requires_subscription),
      );
    }

    if let Some(uploader) = &self.uploader {
      match &uploader.info {
        Some(info) => writer::tag_with(out, inner, "video:uploader", &[("info", info)], &uploader.name),
        None => writer::tag(out, inner, "video:uploader", &uploader.name),
      }
    }

    if let Some(live) = self.live {
      writer::tag(out, inner, "video:live", writer::yes_no(live));
    }

    for tag in &self.tags {
      writer::tag(out, inner, "video:tag", tag);
    }

    if let Some(category) = &self.category {
      writer::tag(out, inner, "video:category", category);
    }

    writer::close(out, depth, "video:video");
  }
}

pub struct VideoBuilder {
  thumbnail_loc: String,
  title: String,
  description: String,
  content_loc: Option<String>,
  player_loc: Option<String>,
  duration: Option<u32>,
  expiration_date: Option<Date>,
  rating: Option<f32>,
  view_count: Option<u64>,
  publication_date: Option<Date>,
  family_friendly: Option<bool>,
  restriction: Option<Restriction>,
  platform: Option<PlatformRestriction>,
  requires_subscription: Option<bool>,
  uploader: Option<Uploader>,
  live: Option<bool>,
  tags: Vec<String>,
  category: Option<String>,
}

impl VideoBuilder {
  pub fn content_loc(mut self, content_loc: impl Into<String>) -> Self {
    self.content_loc = Some(content_loc.into());
    self
  }

  pub fn player_loc(mut self, player_loc: impl Into<String>) -> Self {
    self.player_loc = Some(player_loc.into());
    self
  }

  pub fn duration(mut self, seconds: u32) -> Self {
    self.duration = Some(seconds);
    self
  }

  pub fn expiration_date(mut self, at: impl Into<Date>) -> Self {
    self.expiration_date = Some(at.into());
    self
  }

  pub fn rating(mut self, rating: f32) -> Self {
    self.rating = Some(rating);
    self
  }

  pub fn view_count(mut self, view_count: u64) -> Self {
    self.view_count = Some(view_count);
    self
  }

  pub fn publication_date(mut self, at: impl Into<Date>) -> Self {
    self.publication_date = Some(at.into());
    self
  }

  pub fn family_friendly(mut self, family_friendly: bool) -> Self {
    self.family_friendly = Some(family_friendly);
    self
  }

  pub fn restriction(mut self, restriction: Restriction) -> Self {
    self.restriction = Some(restriction);
    self
  }

  pub fn platform(mut self, platform: PlatformRestriction) -> Self {
    self.platform = Some(platform);
    self
  }

  pub fn requires_subscription(mut self, requires_subscription: bool) -> Self {
    self.requires_subscription = Some(requires_subscription);
    self
  }

  pub fn uploader(mut self, uploader: Uploader) -> Self {
    self.uploader = Some(uploader);
    self
  }

  pub fn live(mut self, live: bool) -> Self {
    self.live = Some(live);
    self
  }

  pub fn tags(mut self, tags: Vec<String>) -> Self {
    self.tags = tags;
    self
  }

  pub fn category(mut self, category: impl Into<String>) -> Self {
    self.category = Some(category.into());
    self
  }

  pub fn build(self) -> Result<Video> {
    if self.content_loc.is_none() && self.player_loc.is_none() {
      return Err(Error::VideoWithoutSource);
    }

    if self.tags.len() > limit::MAX_VIDEO_TAGS {
      return Err(Error::TooManyVideoTags { count: self.tags.len() });
    }

    if let Some(category) = &self.category {
      let length = category.chars().count();

      if length > limit::MAX_VIDEO_CATEGORY_LEN {
        return Err(Error::VideoCategoryTooLong { length });
      }
    }

    if let Some(seconds) = self.duration
      && !(limit::MIN_VIDEO_DURATION_SECONDS..=limit::MAX_VIDEO_DURATION_SECONDS).contains(&seconds)
    {
      return Err(Error::VideoDurationOutOfRange { seconds });
    }

    if let Some(rating) = self.rating
      && !(limit::MIN_VIDEO_RATING..=limit::MAX_VIDEO_RATING).contains(&rating)
    {
      return Err(Error::VideoRatingOutOfRange { rating });
    }

    let content_loc = self.content_loc.map(location).transpose()?;
    let player_loc = self.player_loc.map(location).transpose()?;

    Ok(Video {
      thumbnail_loc: location(self.thumbnail_loc)?,
      title: self.title,
      description: self.description,
      content_loc,
      player_loc,
      duration: self.duration,
      expiration_date: self.expiration_date,
      rating: self.rating,
      view_count: self.view_count,
      publication_date: self.publication_date,
      family_friendly: self.family_friendly,
      restriction: self.restriction,
      platform: self.platform,
      requires_subscription: self.requires_subscription,
      uploader: self.uploader,
      live: self.live,
      tags: self.tags,
      category: self.category,
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn builder() -> VideoBuilder {
    Video::builder("https://example.com/thumb.jpg", "A title", "A description")
      .content_loc("https://example.com/video.mp4")
  }

  #[test]
  fn a_video_needs_a_content_or_player_location() {
    let missing = Video::builder("https://example.com/thumb.jpg", "A title", "A description").build();

    assert_eq!(missing.unwrap_err(), Error::VideoWithoutSource);
    assert!(builder().build().is_ok());
  }

  #[test]
  fn a_player_location_alone_is_enough() {
    let video = Video::builder("https://example.com/thumb.jpg", "A title", "A description")
      .player_loc("https://example.com/player")
      .build();

    assert!(video.is_ok());
  }

  #[test]
  fn duration_is_bounded_at_both_ends() {
    assert_eq!(
      builder().duration(0).build().unwrap_err(),
      Error::VideoDurationOutOfRange { seconds: 0 }
    );
    assert_eq!(
      builder().duration(28_801).build().unwrap_err(),
      Error::VideoDurationOutOfRange { seconds: 28_801 }
    );
    assert!(builder().duration(1).build().is_ok());
    assert!(builder().duration(28_800).build().is_ok());
  }

  #[test]
  fn rating_is_bounded_at_five() {
    assert_eq!(
      builder().rating(5.1).build().unwrap_err(),
      Error::VideoRatingOutOfRange { rating: 5.1 }
    );
    assert!(builder().rating(5.0).build().is_ok());
  }

  #[test]
  fn tags_are_capped_at_thirty_two() {
    let tags: Vec<String> = (0..33).map(|n| n.to_string()).collect();

    assert_eq!(
      builder().tags(tags).build().unwrap_err(),
      Error::TooManyVideoTags { count: 33 }
    );
  }

  #[test]
  fn a_category_is_capped_at_two_hundred_and_fifty_six() {
    assert_eq!(
      builder().category("a".repeat(257)).build().unwrap_err(),
      Error::VideoCategoryTooLong { length: 257 }
    );
    assert!(builder().category("a".repeat(256)).build().is_ok());
  }

  #[test]
  fn restrictions_render_as_a_space_delimited_list() {
    let mut out = String::new();

    builder()
      .restriction(Restriction {
        relationship: Relationship::Deny,
        countries: vec!["CA".to_string(), "MX".to_string()],
      })
      .build()
      .unwrap()
      .write(&mut out, 0);

    assert!(out.contains("<video:restriction relationship=\"deny\">CA MX</video:restriction>"));
  }

  #[test]
  fn an_uploader_carries_its_info_url_as_an_attribute() {
    let mut out = String::new();

    builder()
      .uploader(Uploader {
        name: "Norm".to_string(),
        info: Some("https://example.com/norm".to_string()),
      })
      .build()
      .unwrap()
      .write(&mut out, 0);

    assert!(out.contains("<video:uploader info=\"https://example.com/norm\">Norm</video:uploader>"));
  }

  #[test]
  fn booleans_render_as_yes_and_no() {
    let mut out = String::new();

    builder()
      .family_friendly(true)
      .requires_subscription(false)
      .build()
      .unwrap()
      .write(&mut out, 0);

    assert!(out.contains("<video:family_friendly>yes</video:family_friendly>"));
    assert!(out.contains("<video:requires_subscription>no</video:requires_subscription>"));
  }
}
