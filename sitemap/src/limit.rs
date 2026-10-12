//! The protocol's own numbers, in one place.

/// <https://www.sitemaps.org/protocol.html>
pub const MAX_URLS: usize = 50_000;

/// <https://www.sitemaps.org/protocol.html>
pub const MAX_SITEMAPS: usize = 50_000;

/// Uncompressed. Applies to a `<urlset>` and a `<sitemapindex>` alike.
pub const MAX_DOCUMENT_BYTES: usize = 50 * 1024 * 1024;

/// A `loc` must be *under* this, not at it.
pub const MAX_LOCATION_LEN: usize = 2_048;

pub const MIN_PRIORITY: f32 = 0.0;
pub const MAX_PRIORITY: f32 = 1.0;

/// <https://developers.google.com/search/docs/crawling-indexing/sitemaps/image-sitemaps>
pub const MAX_IMAGES_PER_URL: usize = 1_000;

/// <https://developers.google.com/search/docs/crawling-indexing/sitemaps/news-sitemap>
pub const MAX_NEWS_PER_URL_SET: usize = 1_000;

/// <https://developers.google.com/search/docs/crawling-indexing/sitemaps/video-sitemaps>
pub const MAX_VIDEO_TAGS: usize = 32;
pub const MAX_VIDEO_CATEGORY_LEN: usize = 256;
pub const MIN_VIDEO_DURATION_SECONDS: u32 = 1;
pub const MAX_VIDEO_DURATION_SECONDS: u32 = 28_800;
pub const MIN_VIDEO_RATING: f32 = 0.0;
pub const MAX_VIDEO_RATING: f32 = 5.0;
