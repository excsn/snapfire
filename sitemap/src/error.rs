use std::fmt;

use crate::limit;

/// Every way a document can violate the sitemap protocol.
///
/// These are returned rather than clamped or dropped: a search engine that
/// rejects an oversized sitemap does so silently, so construction is the only
/// place the problem can still be seen.
#[derive(Debug, Clone, PartialEq)]
pub enum Error {
  LocationEmpty,
  LocationTooLong { length: usize },
  PriorityOutOfRange { priority: f32 },
  TooManyUrls { count: usize },
  TooManySitemaps { count: usize },
  TooManyImages { count: usize },
  TooMuchNews { count: usize },
  TooManyVideoTags { count: usize },
  VideoCategoryTooLong { length: usize },
  VideoDurationOutOfRange { seconds: u32 },
  VideoRatingOutOfRange { rating: f32 },
  /// A `<video:video>` needs a `content_loc`, a `player_loc` or both.
  VideoWithoutSource,
  DocumentTooLarge { bytes: usize },
}

impl fmt::Display for Error {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::LocationEmpty => write!(f, "a location must not be empty"),
      Self::LocationTooLong { length } => write!(
        f,
        "a location must be under {} characters, got {length}",
        limit::MAX_LOCATION_LEN
      ),
      Self::PriorityOutOfRange { priority } => write!(
        f,
        "a priority must be between {} and {}, got {priority}",
        limit::MIN_PRIORITY,
        limit::MAX_PRIORITY
      ),
      Self::TooManyUrls { count } => write!(
        f,
        "a urlset holds at most {} urls, got {count}",
        limit::MAX_URLS
      ),
      Self::TooManySitemaps { count } => write!(
        f,
        "a sitemapindex holds at most {} sitemaps, got {count}",
        limit::MAX_SITEMAPS
      ),
      Self::TooManyImages { count } => write!(
        f,
        "a url holds at most {} images, got {count}",
        limit::MAX_IMAGES_PER_URL
      ),
      Self::TooMuchNews { count } => write!(
        f,
        "a urlset holds at most {} news entries, got {count}",
        limit::MAX_NEWS_PER_URL_SET
      ),
      Self::TooManyVideoTags { count } => write!(
        f,
        "a video holds at most {} tags, got {count}",
        limit::MAX_VIDEO_TAGS
      ),
      Self::VideoCategoryTooLong { length } => write!(
        f,
        "a video category must be at most {} characters, got {length}",
        limit::MAX_VIDEO_CATEGORY_LEN
      ),
      Self::VideoDurationOutOfRange { seconds } => write!(
        f,
        "a video duration must be between {} and {} seconds, got {seconds}",
        limit::MIN_VIDEO_DURATION_SECONDS,
        limit::MAX_VIDEO_DURATION_SECONDS
      ),
      Self::VideoRatingOutOfRange { rating } => write!(
        f,
        "a video rating must be between {} and {}, got {rating}",
        limit::MIN_VIDEO_RATING,
        limit::MAX_VIDEO_RATING
      ),
      Self::VideoWithoutSource => write!(f, "a video needs a content location, a player location or both"),
      Self::DocumentTooLarge { bytes } => write!(
        f,
        "a document must be under {} bytes, got {bytes}",
        limit::MAX_DOCUMENT_BYTES
      ),
    }
  }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
