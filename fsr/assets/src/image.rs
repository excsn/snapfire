use crate::policy::Format;
use crate::Error;
use fast_image_resize::{FilterType, ResizeAlg, ResizeOptions, Resizer};
use image::metadata::Orientation;
use image::{DynamicImage, ImageReader, RgbaImage};
use snapfire_media::Header;
use std::io::Cursor;
use std::path::Path;

/// A decoded image, held as RGBA so every variant resizes from one buffer,
/// upright: the EXIF orientation the header carries is applied to the pixels,
/// since a variant is written without the tag.
pub struct Source {
  pixels: RgbaImage,
}

impl Source {
  pub fn open(path: &Path) -> Result<Self, Error> {
    let bytes = std::fs::read(path).map_err(|e| Error::Io(path.to_path_buf(), e))?;
    Self::from_bytes(path, &bytes)
  }

  /// `path` names the file in an error only; the bytes are what is decoded.
  pub fn from_bytes(path: &Path, bytes: &[u8]) -> Result<Self, Error> {
    let orientation = snapfire_media::image::orientation(bytes);
    let mut decoded = ImageReader::new(Cursor::new(bytes))
      .with_guessed_format()
      .map_err(|e| Error::Decode(path.to_path_buf(), e.to_string()))?
      .decode()
      .map_err(|e| Error::Decode(path.to_path_buf(), e.to_string()))?;
    if let Some(orientation) = Orientation::from_exif(orientation) {
      decoded.apply_orientation(orientation);
    }
    Ok(Self { pixels: decoded.into_rgba8() })
  }

  pub fn width(&self) -> u32 {
    self.pixels.width()
  }

  pub fn height(&self) -> u32 {
    self.pixels.height()
  }

  /// The height a variant of `width` has, keeping the aspect ratio and never
  /// collapsing to zero.
  pub fn height_at(&self, width: u32) -> u32 {
    if width >= self.width() {
      return self.height();
    }
    let scaled = f64::from(self.height()) * f64::from(width) / f64::from(self.width());
    (scaled.round() as u32).max(1)
  }

  /// One variant: resized to `width` when that is below the source's width,
  /// then encoded in `format` at `quality`, 0 to 100.
  pub fn variant(&self, width: u32, format: Format, quality: u8) -> Result<Vec<u8>, Error> {
    let resized = self.resized(width);
    encode(&resized, format, quality)
  }

  fn resized(&self, width: u32) -> RgbaImage {
    if width >= self.width() {
      return self.pixels.clone();
    }
    let height = self.height_at(width);
    let source = DynamicImage::ImageRgba8(self.pixels.clone());
    let mut target = DynamicImage::new_rgba8(width, height);
    let options = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Lanczos3));
    let mut resizer = Resizer::new();
    match resizer.resize(&source, &mut target, &options) {
      Ok(()) => target.into_rgba8(),
      Err(_) => image::imageops::resize(&self.pixels, width, height, image::imageops::FilterType::Lanczos3),
    }
  }
}

fn encode(pixels: &RgbaImage, format: Format, quality: u8) -> Result<Vec<u8>, Error> {
  let quality = f32::from(quality.min(100));
  let (width, height) = (pixels.width(), pixels.height());
  match format {
    Format::Webp => {
      let encoded = webp::Encoder::from_rgba(pixels.as_raw(), width, height).encode(quality);
      Ok(encoded.to_vec())
    }
    Format::Avif => {
      let rgba: Vec<ravif::RGBA8> =
        pixels.as_raw().chunks_exact(4).map(|px| ravif::RGBA8::new(px[0], px[1], px[2], px[3])).collect();
      let encoded = ravif::Encoder::new()
        .with_quality(quality)
        .with_alpha_quality(quality)
        .with_speed(6)
        .encode_rgba(ravif::Img::new(rgba.as_slice(), width as usize, height as usize))
        .map_err(|e| Error::Encode("avif", e.to_string()))?;
      Ok(encoded.avif_file)
    }
  }
}

/// Width and height as displayed, from the header alone with nothing decoded:
/// the stored size with the EXIF orientation applied.
pub fn dimensions(path: &Path) -> Result<(u32, u32), Error> {
  let header = Header::read(path)?;
  Ok((header.width, header.height))
}

/// Whether the file is served as it is rather than resized: an SVG scales
/// itself and an animated GIF or APNG would have every frame re-encoded.
pub fn passthrough(path: &Path) -> Result<bool, Error> {
  let ext = path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).unwrap_or_default();
  match ext.as_str() {
    "svg" => Ok(true),
    "gif" => {
      let bytes = std::fs::read(path).map_err(|e| Error::Io(path.to_path_buf(), e))?;
      Ok(gif_frames(&bytes) > 1)
    }
    "png" | "apng" => {
      let bytes = std::fs::read(path).map_err(|e| Error::Io(path.to_path_buf(), e))?;
      Ok(png_is_animated(&bytes))
    }
    _ => Ok(false),
  }
}

/// How many image descriptors the GIF holds, counted without decoding.
fn gif_frames(bytes: &[u8]) -> usize {
  use image::AnimationDecoder;
  use image::codecs::gif::GifDecoder;
  match GifDecoder::new(Cursor::new(bytes)) {
    Ok(decoder) => decoder.into_frames().take(2).filter(|f| f.is_ok()).count(),
    Err(_) => 0,
  }
}

/// An APNG carries an `acTL` chunk before its first `IDAT`.
fn png_is_animated(bytes: &[u8]) -> bool {
  let mut at = 8;
  while at + 8 <= bytes.len() {
    let length = u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]) as usize;
    let tag = &bytes[at + 4..at + 8];
    match tag {
      b"acTL" => return true,
      b"IDAT" | b"IEND" => return false,
      _ => {}
    }
    at = at.saturating_add(12).saturating_add(length);
  }
  false
}

/// `<stem>.<hash>.<width>.<ext>`, the name a variant is served under beside
/// the hashed original.
pub fn variant_name(stem: &str, hash: &str, width: u32, format: Format) -> String {
  format!("{stem}.{hash}.{width}.{}", format.extension())
}

#[cfg(test)]
mod tests {
  use super::*;
  use image::{ImageEncoder, Rgba};

  fn gradient(width: u32, height: u32) -> Vec<u8> {
    let img = RgbaImage::from_fn(width, height, |x, y| Rgba([(x * 255 / width.max(1)) as u8, (y * 255 / height.max(1)) as u8, 128, 255]));
    let mut out = Vec::new();
    image::codecs::png::PngEncoder::new(&mut out).write_image(img.as_raw(), width, height, image::ExtendedColorType::Rgba8).unwrap();
    out
  }

  #[test]
  fn a_variant_is_resized_and_encoded_in_each_format() {
    let png = gradient(64, 40);
    let source = Source::from_bytes(Path::new("g.png"), &png).unwrap();
    assert_eq!((source.width(), source.height()), (64, 40));
    assert_eq!(source.height_at(32), 20);
    assert_eq!(source.height_at(64), 40);
    assert_eq!(source.height_at(1), 1);

    let webp = source.variant(32, Format::Webp, 80).unwrap();
    assert_eq!(&webp[8..12], b"WEBP");
    let decoded = ImageReader::new(Cursor::new(&webp)).with_guessed_format().unwrap().decode().unwrap();
    assert_eq!((decoded.width(), decoded.height()), (32, 20));

    let avif = source.variant(32, Format::Avif, 60).unwrap();
    assert_eq!(&avif[4..12], b"ftypavif");
    assert!(avif.len() > 32);

    let same = source.variant(64, Format::Webp, 80).unwrap();
    let bigger = source.variant(128, Format::Webp, 80).unwrap();
    assert_eq!(same, bigger, "a width past the source is the source");
  }

  #[test]
  fn dimensions_come_from_the_header_and_a_still_png_is_not_passed_through() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("g.png");
    std::fs::write(&path, gradient(10, 7)).unwrap();
    assert_eq!(dimensions(&path).unwrap(), (10, 7));
    assert!(!passthrough(&path).unwrap());
    let svg = dir.path().join("logo.svg");
    std::fs::write(&svg, "<svg xmlns='http://www.w3.org/2000/svg'/>").unwrap();
    assert!(passthrough(&svg).unwrap());
  }

  #[test]
  fn a_tagged_jpeg_is_decoded_upright_and_its_variants_follow() {
    let jpeg = std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tagged.jpg")).unwrap();
    let source = Source::from_bytes(Path::new("tagged.jpg"), &jpeg).unwrap();
    assert_eq!((source.width(), source.height()), (2, 4), "stored 4x2 with orientation 6");
    let top = source.pixels.get_pixel(0, 0).0;
    let bottom = source.pixels.get_pixel(0, 3).0;
    assert!(top[0] > 150 && top[2] < 100, "the stored left half, red, is on top once rotated: {top:?}");
    assert!(bottom[2] > 150 && bottom[0] < 100, "{bottom:?}");

    let webp = source.variant(2, Format::Webp, 90).unwrap();
    let decoded = ImageReader::new(Cursor::new(&webp)).with_guessed_format().unwrap().decode().unwrap();
    assert_eq!((decoded.width(), decoded.height()), (2, 4));
    assert_eq!(snapfire_media::image::orientation(&webp), 1, "a variant carries no tag");

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tagged.jpg");
    std::fs::write(&path, &jpeg).unwrap();
    assert_eq!(dimensions(&path).unwrap(), (2, 4));
  }

  #[test]
  fn an_apng_is_told_from_a_png_by_its_actl_chunk() {
    let mut still = gradient(2, 2);
    assert!(!png_is_animated(&still));
    let actl = [0u8, 0, 0, 8, b'a', b'c', b'T', b'L', 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0];
    let ihdr_end = 8 + 8 + 13 + 4;
    still.splice(ihdr_end..ihdr_end, actl);
    assert!(png_is_animated(&still));
  }

  #[test]
  fn a_variant_name_carries_hash_width_and_format() {
    assert_eq!(variant_name("hero", "0a1b2c3d", 640, Format::Avif), "hero.0a1b2c3d.640.avif");
  }
}
