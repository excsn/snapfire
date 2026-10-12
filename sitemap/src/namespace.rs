use crate::error::{Error, Result};
use crate::limit;

pub(crate) const DECLARATION: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n";

pub const SITEMAP: &str = "http://www.sitemaps.org/schemas/sitemap/0.9";
pub const IMAGE: &str = "http://www.google.com/schemas/sitemap-image/1.1";
pub const VIDEO: &str = "http://www.google.com/schemas/sitemap-video/1.1";
pub const NEWS: &str = "http://www.google.com/schemas/sitemap-news/0.9";
pub const XHTML: &str = "http://www.w3.org/1999/xhtml";

/// The byte cap can only be judged once the document exists, so it is the one
/// limit checked after rendering rather than at construction.
pub(crate) fn check_size(document: String) -> Result<String> {
  if document.len() > limit::MAX_DOCUMENT_BYTES {
    return Err(Error::DocumentTooLarge { bytes: document.len() });
  }

  Ok(document)
}
