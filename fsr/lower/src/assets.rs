//! What the lowerer asks about an image, a font or a media file a component imports. The
//! build answers from the files; the lowerer only writes what it is told.

use std::collections::BTreeMap;

/// Where the build looks an asset up. `path` is relative to the app and
/// normalised, `src/img/hero.png`.
pub trait AssetResolver {
  /// An image's served URL, its intrinsic size and the `<source>` rows its
  /// variants make. `request` carries a per-image override, which the
  /// resolver records so the build generates what the markup names. `None`
  /// when the file is not under the app.
  fn image(&self, path: &str, request: &ImageRequest) -> Option<ImageFacts>;
  /// A font file's served URL.
  fn font(&self, path: &str) -> Option<String>;
  /// A video, an audio file or a caption track's served URL: the file as it
  /// is, under its content hash.
  fn media(&self, _path: &str) -> Option<String> {
    None
  }
  /// The still an `<Embed>` shows for `id` before it plays: the provider's
  /// own image, or one this application serves when `[media] embed_stills`
  /// is `local`. YouTube's is named by the id alone; Vimeo's needs a lookup a
  /// resolver makes. `None` when there is none, and the placeholder shows the
  /// title alone.
  fn embed_still(&self, provider: &str, id: &str) -> Option<ImageFacts> {
    youtube_still(provider, id)
  }
  /// The URL template of a named remote source, `{src}` and `{width}` in it.
  fn source(&self, name: &str) -> Option<String>;
  /// The policy's widths, for a remote image whose own width is unknown.
  fn widths(&self) -> Vec<u32>;
}

/// A per-image override of the policy, both halves optional.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct ImageRequest {
  pub widths: Option<Vec<u32>>,
  pub quality: Option<BTreeMap<String, u8>>,
}

/// One image as the markup names it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ImageFacts {
  /// The hashed original's URL, what the `<img>` carries.
  pub src: String,
  pub width: u32,
  pub height: u32,
  /// `(mime, srcset)` per format, in the policy's order. Empty for an image
  /// served as it is, an SVG or an animation, which gets no `<picture>`.
  pub sources: Vec<(String, String)>,
}

impl ImageFacts {
  pub fn new(src: String, width: u32, height: u32, sources: Vec<(String, String)>) -> Self {
    Self { src, width, height, sources }
  }
}

/// YouTube's still: the URL before the id, the URL after it, the width and the height.
pub const YOUTUBE_STILL: (&str, &str, u32, u32) = ("https://i.ytimg.com/vi/", "/hqdefault.jpg", 480, 360);

/// YouTube's still for `id`, which its CDN serves at a URL the id names.
pub fn youtube_still(provider: &str, id: &str) -> Option<ImageFacts> {
  let (before, after, width, height) = YOUTUBE_STILL;
  (provider == "youtube").then(|| ImageFacts { src: format!("{before}{id}{after}"), width, height, sources: Vec::new() })
}

/// A resolver that knows nothing, so an asset import stays unbound.
pub struct NoAssets;

impl AssetResolver for NoAssets {
  fn image(&self, _: &str, _: &ImageRequest) -> Option<ImageFacts> {
    None
  }
  fn font(&self, _: &str) -> Option<String> {
    None
  }
  fn source(&self, _: &str) -> Option<String> {
    None
  }
  fn widths(&self) -> Vec<u32> {
    Vec::new()
  }
}

const IMAGES: [&str; 9] = ["png", "jpg", "jpeg", "gif", "webp", "avif", "svg", "ico", "bmp"];
const FONTS: [&str; 4] = ["woff2", "woff", "ttf", "otf"];
/// What a `<video>`, an `<audio>` or a `<track>` plays: the containers browsers decode and WebVTT captions.
const MEDIA: [&str; 13] = ["mp4", "m4v", "webm", "ogv", "mov", "mp3", "m4a", "aac", "oga", "ogg", "opus", "wav", "vtt"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
  Image,
  Font,
  Media,
}

/// What a relative specifier names by its extension, if an asset.
pub fn kind(specifier: &str) -> Option<Kind> {
  let path = specifier.split(['?', '#']).next().unwrap_or(specifier);
  let ext = path.rsplit('.').next()?.to_ascii_lowercase();
  if path.rsplit('/').next().is_some_and(|name| !name.contains('.')) {
    return None;
  }
  if IMAGES.contains(&ext.as_str()) {
    Some(Kind::Image)
  } else if FONTS.contains(&ext.as_str()) {
    Some(Kind::Font)
  } else if MEDIA.contains(&ext.as_str()) {
    Some(Kind::Media)
  } else {
    None
  }
}

/// The `sizes` an image gets when the author writes none: full width on a
/// screen narrower than the image, its own width otherwise.
pub fn default_sizes(width: u32) -> String {
  format!("(max-width: {width}px) 100vw, {width}px")
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn an_extension_says_what_an_import_names() {
    assert_eq!(kind("../img/hero.png"), Some(Kind::Image));
    assert_eq!(kind("./a.woff2?v=1"), Some(Kind::Font));
    assert_eq!(kind("./Card"), None);
    assert_eq!(kind("./Card.tsx"), None);
    assert_eq!(kind("./data.json"), None);
  }

  #[test]
  fn the_default_sizes_names_the_intrinsic_width() {
    assert_eq!(default_sizes(1600), "(max-width: 1600px) 100vw, 1600px");
  }
}
