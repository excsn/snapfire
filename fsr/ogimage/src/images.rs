use std::{collections::HashMap, sync::Arc};

use tiny_skia::{IntSize, Pixmap};

use crate::CardError;

/// The images a card may draw, by key, decoded once when inserted. An
/// [`crate::ImageRef::Key`] or a `url(key)` in a background names one.
#[derive(Clone, Default)]
pub struct Images {
  decoded: HashMap<String, Arc<Pixmap>>,
}

impl std::fmt::Debug for Images {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_map().entries(self.decoded.iter().map(|(k, p)| (k, (p.width(), p.height())))).finish()
  }
}

impl Images {
  pub fn new() -> Images {
    Images::default()
  }

  /// Decodes PNG or JPEG bytes and keeps them under `key`, replacing an image
  /// already there.
  pub fn insert(&mut self, key: impl Into<String>, bytes: &[u8]) -> Result<(), CardError> {
    let key = key.into();
    let pixmap = decode(bytes).map_err(|reason| CardError::Image { key: key.clone(), reason })?;
    self.decoded.insert(key, Arc::new(pixmap));
    Ok(())
  }

  pub fn contains(&self, key: &str) -> bool {
    self.decoded.contains_key(key)
  }

  /// The decoded size of the image under `key`.
  pub fn size(&self, key: &str) -> Option<(u32, u32)> {
    self.decoded.get(key).map(|p| (p.width(), p.height()))
  }

  pub(crate) fn get(&self, key: &str) -> Option<Arc<Pixmap>> {
    self.decoded.get(key).cloned()
  }
}

pub(crate) fn decode(bytes: &[u8]) -> Result<Pixmap, String> {
  if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
    png_pixels(bytes)
  } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
    jpeg_pixels(bytes)
  } else if bytes.trim_ascii_start().starts_with(b"<") {
    Err("SVG images are not supported; give a PNG or a JPEG".into())
  } else {
    Err("the bytes are not a PNG or a JPEG".into())
  }
}

fn png_pixels(bytes: &[u8]) -> Result<Pixmap, String> {
  let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
  decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
  let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
  let mut buf = vec![0; reader.output_buffer_size().ok_or("the PNG is too large")?];
  let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
  let pixels = &buf[..info.buffer_size()];
  let rgba: Vec<u8> = match info.color_type {
    png::ColorType::Rgba => pixels.to_vec(),
    png::ColorType::Rgb => pixels.chunks_exact(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
    png::ColorType::GrayscaleAlpha => pixels.chunks_exact(2).flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
    png::ColorType::Grayscale => pixels.iter().flat_map(|&g| [g, g, g, 255]).collect(),
    png::ColorType::Indexed => return Err("the PNG's palette did not expand".into()),
  };
  pixmap(rgba, info.width, info.height)
}

fn jpeg_pixels(bytes: &[u8]) -> Result<Pixmap, String> {
  let options = zune_core::options::DecoderOptions::default().jpeg_set_out_colorspace(zune_core::colorspace::ColorSpace::RGBA);
  let mut decoder = zune_jpeg::JpegDecoder::new_with_options(zune_core::bytestream::ZCursor::new(bytes), options);
  let rgba = decoder.decode().map_err(|e| format!("{e:?}"))?;
  let info = decoder.info().ok_or("the JPEG has no header")?;
  pixmap(rgba, info.width as u32, info.height as u32)
}

fn pixmap(mut rgba: Vec<u8>, width: u32, height: u32) -> Result<Pixmap, String> {
  for p in rgba.chunks_exact_mut(4) {
    let a = p[3] as u16;
    if a < 255 {
      for c in &mut p[..3] {
        *c = ((*c as u16 * a + 127) / 255) as u8;
      }
    }
  }
  let size = IntSize::from_wh(width, height).ok_or("the image has no pixels")?;
  Pixmap::from_vec(rgba, size).ok_or_else(|| "the image's pixels do not match its size".into())
}
