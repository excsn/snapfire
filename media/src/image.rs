use crate::Error;
use std::io::Cursor;
use std::path::Path;

/// What an image's header says: the size it displays at and the EXIF
/// orientation that size already accounts for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
  /// The width after the orientation is applied, which is the stored height
  /// when `orientation` is 5 to 8.
  pub width: u32,
  pub height: u32,
  /// The EXIF orientation, 1 to 8, with 1 for a file that carries none or an
  /// unreadable one. The browser rotates a served file by it; a decoder that
  /// resizes has to apply it to the pixels itself.
  pub orientation: u8,
}

impl Header {
  pub fn read(path: &Path) -> Result<Self, Error> {
    let bytes = std::fs::read(path).map_err(|e| Error::Io(path.to_path_buf(), e))?;
    Self::from_bytes(path, &bytes)
  }

  /// `path` names the file in an error only.
  pub fn from_bytes(path: &Path, bytes: &[u8]) -> Result<Self, Error> {
    let size = imagesize::blob_size(bytes).map_err(|e| Error::Image(path.to_path_buf(), e.to_string()))?;
    let (width, height) = (size.width as u32, size.height as u32);
    let orientation = orientation(bytes);
    let (width, height) = if transposes(orientation) { (height, width) } else { (width, height) };
    Ok(Header { width, height, orientation })
  }

  /// Whether the stored frame is the transpose of the displayed one.
  pub fn transposes(&self) -> bool {
    transposes(self.orientation)
  }
}

/// The EXIF orientation tag from a JPEG's APP1 segment, a PNG's `eXIf` chunk
/// or a WebP's `EXIF` chunk, 1 when there is none. A value outside 1 to 8 is
/// not an orientation and reads as 1.
pub fn orientation(bytes: &[u8]) -> u8 {
  let Ok(exif) = exif::Reader::new().read_from_container(&mut Cursor::new(bytes)) else {
    return 1;
  };
  exif
    .get_field(exif::Tag::Orientation, exif::In::PRIMARY)
    .and_then(|field| field.value.get_uint(0))
    .filter(|value| (1..=8).contains(value))
    .map(|value| value as u8)
    .unwrap_or(1)
}

fn transposes(orientation: u8) -> bool {
  (5..=8).contains(&orientation)
}

#[cfg(test)]
mod tests {
  use super::*;

  fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)).unwrap()
  }

  #[test]
  fn a_tagged_jpeg_png_and_webp_report_the_displayed_size_and_the_tag() {
    for name in ["tagged.jpg", "tagged.png", "tagged.webp"] {
      let header = Header::from_bytes(Path::new(name), &fixture(name)).unwrap();
      assert_eq!(header, Header { width: 2, height: 4, orientation: 6 }, "{name}");
      assert!(header.transposes());
      assert_eq!(imagesize::blob_size(&fixture(name)).map(|s| (s.width, s.height)).unwrap(), (4, 2), "{name} is stored as 4x2");
    }
  }

  #[test]
  fn a_file_without_a_tag_is_upright_at_its_stored_size() {
    let png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x03\0\0\0\x02\x08\x06\0\0\0";
    let header = Header::from_bytes(Path::new("dot.png"), png).unwrap();
    assert_eq!(header, Header { width: 3, height: 2, orientation: 1 });
    assert!(!header.transposes());
    assert_eq!(orientation(b"not an image"), 1);
  }

  #[test]
  fn bytes_that_are_not_an_image_are_refused_with_the_path() {
    let err = Header::from_bytes(Path::new("x.png"), b"nope").unwrap_err();
    assert!(err.to_string().contains("x.png"), "{err}");
  }

  #[test]
  fn a_tag_outside_one_to_eight_reads_as_upright() {
    let mut jpeg = fixture("tagged.jpg");
    let at = jpeg.windows(2).position(|w| w == [0x01, 0x12]).unwrap();
    jpeg[at + 8] = 0;
    jpeg[at + 9] = 9;
    assert_eq!(orientation(&jpeg), 1);
    assert_eq!(Header::from_bytes(Path::new("x.jpg"), &jpeg).unwrap().width, 4);
  }
}
