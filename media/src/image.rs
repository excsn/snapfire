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
    if let Some((width, height)) = svg_size(bytes) {
      return Ok(Header { width, height, orientation: 1 });
    }
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

/// The size an SVG draws at, from the root element's `width` and `height`
/// when both are plain lengths or in `px`, else from its `viewBox`. An SVG
/// with neither has no size to write on an element. `None` for bytes that
/// are not an SVG at all.
pub fn svg_size(bytes: &[u8]) -> Option<(u32, u32)> {
  let head = std::str::from_utf8(&bytes[..bytes.len().min(4096)]).ok()?;
  let start = head.find("<svg")?;
  if head[..start].trim_start_matches('\u{feff}').trim().chars().any(|c| c != '<' && !c.is_whitespace() && !head[..start].contains("<?xml") && !head[..start].contains("<!--")) {
    return None;
  }
  let tag = &head[start..];
  let end = tag.find('>')?;
  let tag = &tag[..end];
  let attr = |name: &str| -> Option<&str> {
    let mut rest = tag;
    while let Some(at) = rest.find(name) {
      let before_ok = at == 0 || rest.as_bytes()[at - 1].is_ascii_whitespace();
      let after = &rest[at + name.len()..];
      let after = after.trim_start();
      if before_ok && after.starts_with('=') {
        let after = after[1..].trim_start();
        let quote = after.chars().next()?;
        if quote == '"' || quote == '\'' {
          return after[1..].split(quote).next();
        }
      }
      rest = &rest[at + name.len()..];
    }
    None
  };
  let length = |value: &str| -> Option<u32> {
    let value = value.trim().trim_end_matches("px").trim();
    let number: f64 = value.parse().ok()?;
    (number > 0.0).then(|| number.round() as u32)
  };
  if let (Some(w), Some(h)) = (attr("width").and_then(length), attr("height").and_then(length)) {
    return Some((w, h));
  }
  let view_box = attr("viewBox")?;
  let parts: Vec<f64> = view_box.split(|c: char| c == ',' || c.is_whitespace()).filter(|p| !p.is_empty()).filter_map(|p| p.parse().ok()).collect();
  if parts.len() == 4 && parts[2] > 0.0 && parts[3] > 0.0 {
    return Some((parts[2].round() as u32, parts[3].round() as u32));
  }
  None
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
  fn an_svg_is_sized_by_its_attributes_or_its_view_box_and_never_rotated() {
    let sized = br#"<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg" width="120px" height="32" viewBox="0 0 240 64"><circle r="4"/></svg>"#;
    assert_eq!(Header::from_bytes(Path::new("logo.svg"), sized).unwrap(), Header { width: 120, height: 32, orientation: 1 });
    let boxed = b"<svg viewBox=\"0 0 24 24\" xmlns=\"http://www.w3.org/2000/svg\"/>";
    assert_eq!(Header::from_bytes(Path::new("icon.svg"), boxed).unwrap(), Header { width: 24, height: 24, orientation: 1 });
    let relative = b"<svg width=\"100%\" height=\"100%\" viewBox=\"0 0 10 5\"/>";
    assert_eq!(Header::from_bytes(Path::new("fluid.svg"), relative).unwrap(), Header { width: 10, height: 5, orientation: 1 });
    let missing = b"<!-- a comment --><svg xmlns=\"http://www.w3.org/2000/svg\"><rect/></svg>";
    assert!(Header::from_bytes(Path::new("unsized.svg"), missing).is_err(), "no size to write on an element");
    assert_eq!(svg_size(b"<html><svg width=\"1\" height=\"1\"/></html>"), None, "markup that merely contains an svg is not one");
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
