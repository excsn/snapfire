//! Renders a card, a tree of boxes, text and images styled with a subset of
//! CSS, to a PNG or a JPEG. Layout is flexbox through taffy, text is shaped
//! with rustybuzz and pixels are drawn with tiny-skia.

mod blur;
mod card;
mod css;
mod encode;
mod fonts;
mod images;
mod layout;
mod paint;
mod text;

pub use card::{Card, Declarations, Element, ImageRef, Run};
pub use fonts::{FaceInfo, FontError, Fonts};
pub use images::Images;

/// The largest side a card may have, in pixels.
pub const MAX_SIDE: u32 = 8192;

/// The encoding [`render`] writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
  Png,
  /// JPEG at a quality from 1 to 100. JPEG has no transparency, so
  /// transparent pixels are drawn over white.
  Jpeg { quality: u8 },
}

impl Format {
  pub fn content_type(self) -> &'static str {
    match self {
      Format::Png => "image/png",
      Format::Jpeg { .. } => "image/jpeg",
    }
  }
}

/// Something the renderer skipped or could not draw. The card still
/// renders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
  /// The element, as the path of tags from the root with each tag's
  /// position among its siblings: `div(1) > div(2) > span(1)`.
  pub element: String,
  pub message: String,
}

impl std::fmt::Display for Warning {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}: {}", self.element, self.message)
  }
}

/// Why a card did not render.
#[derive(Debug, thiserror::Error, PartialEq)]
#[non_exhaustive]
pub enum CardError {
  #[error("a card is {width}x{height}; each side must be 1 to {MAX_SIDE} pixels")]
  Size { width: u32, height: u32 },
  #[error("{element}: `{property}: {value}` does not parse: {reason}")]
  Value { element: String, property: String, value: String, reason: String },
  #[error("{element}: no image was given for `{key}`")]
  MissingImage { element: String, key: String },
  #[error("the image `{key}` does not decode: {reason}")]
  Image { key: String, reason: String },
  #[error("{element}: the tree nests deeper than {} levels", layout::MAX_DEPTH)]
  Depth { element: String },
  #[error("layout failed: {0}")]
  Layout(String),
  #[error("encoding the card failed: {0}")]
  Encode(String),
}

/// An encoded card and what the renderer skipped on the way.
#[derive(Debug, Clone, PartialEq)]
pub struct Rendered {
  pub bytes: Vec<u8>,
  pub warnings: Vec<Warning>,
}

/// Lays out and draws `card` with `fonts` and `images`, then encodes it.
///
/// A declaration outside the subset, a value the subset leaves out and text
/// no face covers are warnings. A value that does not parse, an image key
/// with no image and bytes that do not decode are errors.
pub fn render(card: &Card, fonts: &Fonts, images: &Images, format: Format) -> Result<Rendered, CardError> {
  if card.width == 0 || card.height == 0 || card.width > MAX_SIDE || card.height > MAX_SIDE {
    return Err(CardError::Size { width: card.width, height: card.height });
  }
  let faces = fonts.open();
  let mut warnings = Vec::new();
  let tree = layout::build(&card.root, (card.width as f32, card.height as f32), &faces, images, &mut warnings)?;
  let pixmap = paint::paint(&tree, &faces, images, card.width, card.height);
  let bytes = encode(&pixmap, format)?;
  Ok(Rendered { bytes, warnings })
}

fn encode(pixmap: &tiny_skia::Pixmap, format: Format) -> Result<Vec<u8>, CardError> {
  match format {
    Format::Png => encode::png(pixmap.data(), pixmap.width(), pixmap.height()),
    Format::Jpeg { quality } => {
      let rgb: Vec<u8> = pixmap.data().chunks_exact(4).flat_map(|p| {
        let a = 255 - p[3] as u16;
        [p[0], p[1], p[2]].map(|c| (c as u16 + a).min(255) as u8)
      }).collect();
      let mut out = Vec::new();
      jpeg_encoder::Encoder::new(&mut out, quality.clamp(1, 100))
        .encode(&rgb, pixmap.width() as u16, pixmap.height() as u16, jpeg_encoder::ColorType::Rgb)
        .map_err(|e| CardError::Encode(e.to_string()))?;
      Ok(out)
    }
  }
}
