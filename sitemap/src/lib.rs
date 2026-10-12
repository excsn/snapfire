//! Sitemap writing with the protocol's limits enforced at construction.
//!
//! Covers `<urlset>` and `<sitemapindex>`, plus the image, video, news and
//! xhtml alternate-link extensions.
//!
//! ```
//! use snapfire_sitemap::{ChangeFrequency, Url, UrlSet};
//!
//! let urls = vec![
//!   Url::builder("https://example.com/")
//!     .changefreq(ChangeFrequency::Weekly)
//!     .priority(0.8)
//!     .build()?,
//! ];
//!
//! let xml = UrlSet::new(urls)?.to_xml()?;
//! # Ok::<(), snapfire_sitemap::Error>(())
//! ```

mod alternate;
mod date;
mod error;
mod image;
mod index;
mod location;
mod namespace;
mod news;
mod url;
mod url_set;
mod video;
mod writer;

pub mod limit;

pub use alternate::Alternate;
pub use date::Date;
pub use error::{Error, Result};
pub use image::Image;
pub use index::{Sitemap, SitemapIndex};
pub use namespace::{IMAGE, NEWS, SITEMAP, VIDEO, XHTML};
pub use news::{News, Publication};
pub use url::{ChangeFrequency, Url, UrlBuilder};
pub use url_set::UrlSet;
pub use video::{Platform, PlatformRestriction, Relationship, Restriction, Uploader, Video, VideoBuilder};
