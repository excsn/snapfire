//! An image or a font a module or a stylesheet names: hashed, so its URL changes
//! when its bytes do, and read for the facts a consumer wants without decoding.
//! The header read is `snapfire_media`'s, so the width and height here are
//! the ones FSR's pipeline sees for the same file.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

const IMAGES: [&str; 9] = ["png", "jpg", "jpeg", "gif", "webp", "avif", "svg", "ico", "bmp"];
const FONTS: [&str; 4] = ["woff2", "woff", "ttf", "otf"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
  Image,
  Font,
}

impl Kind {
  pub fn as_str(self) -> &'static str {
    match self {
      Kind::Image => "image",
      Kind::Font => "font",
    }
  }
}

pub fn kind(path: &Path) -> Option<Kind> {
  let ext = path.extension()?.to_str()?.to_ascii_lowercase();
  if IMAGES.contains(&ext.as_str()) {
    Some(Kind::Image)
  } else if FONTS.contains(&ext.as_str()) {
    Some(Kind::Font)
  } else {
    None
  }
}

/// One asset as the build emits it: the source it came from, the file name it
/// goes out under and what its header says. `width` and `height` are the
/// displayed size, with the EXIF orientation applied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Emitted {
  pub source: PathBuf,
  pub name: String,
  pub hash: String,
  pub kind: Kind,
  pub width: Option<u32>,
  pub height: Option<u32>,
}

impl Emitted {
  /// Where the emitted file sits under `out_dir`, mirroring the source's place under `root_dir`.
  pub fn dest(&self, root_dir: &Path, out_dir: &Path) -> Option<PathBuf> {
    let relative = self.source.strip_prefix(root_dir).ok()?;
    Some(out_dir.join(relative.with_file_name(&self.name)))
  }
}

pub fn emit(source: &Path) -> Result<Emitted> {
  let kind = kind(source).with_context(|| format!("{:?} is not an image or a font", source))?;
  let bytes = std::fs::read(source).with_context(|| format!("Failed to read {:?}", source))?;
  let hash = format!("{:08x}", xxhash_rust::xxh3::xxh3_64(&bytes) as u32);
  let stem = source.file_stem().unwrap_or_default().to_string_lossy();
  let ext = source.extension().unwrap_or_default().to_string_lossy();
  let name = format!("{stem}.{hash}.{ext}");
  let (width, height) = match kind {
    Kind::Image => match snapfire_media::Header::from_bytes(source, &bytes) {
      Ok(header) => (Some(header.width), Some(header.height)),
      Err(_) => (None, None),
    },
    Kind::Font => (None, None),
  };
  Ok(Emitted { source: source.to_path_buf(), name, hash, kind, width, height })
}

/// Whether a `url()` or a specifier names a file beside the one that wrote it,
/// as opposed to a scheme, a root-relative path, a fragment or a data URI.
pub fn is_relative(reference: &str) -> bool {
  !reference.is_empty()
    && !reference.starts_with('/')
    && !reference.starts_with('#')
    && !reference.starts_with("data:")
    && !reference.contains(':')
}

/// A reference without its query or fragment, and the fragment to put back.
pub fn split_reference(reference: &str) -> (&str, &str) {
  let (path, fragment) = match reference.find('#') {
    Some(at) => (&reference[..at], &reference[at..]),
    None => (reference, ""),
  };
  let path = match path.find('?') {
    Some(at) => &path[..at],
    None => path,
  };
  (path, fragment)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_reference_keeps_its_fragment_and_drops_its_query() {
    assert_eq!(split_reference("./a.woff2?v=3#iefix"), ("./a.woff2", "#iefix"));
    assert_eq!(split_reference("./a.png"), ("./a.png", ""));
    assert!(is_relative("./a.png"));
    assert!(is_relative("fonts/a.woff2"));
    assert!(!is_relative("/static/a.png"));
    assert!(!is_relative("https://x/a.png"));
    assert!(!is_relative("data:image/png;base64,AA"));
    assert!(!is_relative("#x"));
  }

  #[test]
  fn the_emitted_name_carries_the_hash_and_the_header_dimensions() {
    let dir = tempfile::tempdir().unwrap();
    let png = dir.path().join("dot.png");
    std::fs::write(&png, b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x03\0\0\0\x02\x08\x06\0\0\0").unwrap();
    let emitted = emit(&png).unwrap();
    assert_eq!(emitted.kind, Kind::Image);
    assert_eq!((emitted.width, emitted.height), (Some(3), Some(2)));
    assert_eq!(emitted.name, format!("dot.{}.png", emitted.hash));
    assert_eq!(emitted.hash.len(), 8);
    let again = emit(&png).unwrap();
    assert_eq!(again, emitted);
  }

  #[test]
  fn a_tagged_photo_is_emitted_at_its_displayed_size() {
    let photo = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/asset-urls/input/img/photo.jpg");
    let emitted = emit(&photo).unwrap();
    assert_eq!((emitted.width, emitted.height), (Some(160), Some(320)), "stored 320x160 with orientation 6");
  }
}
