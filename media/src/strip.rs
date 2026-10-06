//! A served original without the metadata a photo carries: GPS position,
//! camera serial, capture time, an embedded thumbnail. The container is
//! rewritten and the compressed image data is copied unchanged, so no pixel
//! moves. What changes how the file displays stays: the ICC profile and the
//! EXIF orientation, which the browser still applies. The EXIF creator and
//! copyright stay too, so a photographer's credit survives.

use std::io::Cursor;

/// The EXIF tags a stripped file keeps.
const ORIENTATION: u16 = 0x0112;
const ARTIST: u16 = 0x013b;
const COPYRIGHT: u16 = 0x8298;

/// `bytes` with every metadata block dropped but what [`Kept`] keeps, or
/// `None` for a format this does not rewrite: AVIF, SVG, BMP and ICO are
/// served as they are. An error names a JPEG, PNG, WebP or GIF whose
/// container does not hold together.
pub fn strip(bytes: &[u8]) -> Result<Option<Vec<u8>>, String> {
  if bytes.starts_with(&[0xff, 0xd8]) {
    return jpeg(bytes, &Kept::of(bytes)).map(Some);
  }
  if bytes.starts_with(PNG_SIGNATURE) {
    return png(bytes, &Kept::of(bytes)).map(Some);
  }
  if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
    return webp(bytes, &Kept::of(bytes)).map(Some);
  }
  if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
    return gif(bytes).map(Some);
  }
  Ok(None)
}

/// Whether `bytes` carries an EXIF or XMP block, in any container. What a
/// report reads for a format [`strip`] leaves alone.
pub fn carries_metadata(bytes: &[u8]) -> bool {
  exif::Reader::new().read_from_container(&mut Cursor::new(bytes)).is_ok() || contains(bytes, b"<x:xmpmeta") || contains(bytes, b"http://ns.adobe.com/xap/1.0/")
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
  haystack.windows(needle.len()).any(|window| window == needle)
}

/// What survives a strip from the source's EXIF.
struct Kept {
  orientation: u8,
  artist: Option<Vec<u8>>,
  copyright: Option<Vec<u8>>,
}

impl Kept {
  fn of(bytes: &[u8]) -> Self {
    let Ok(exif) = exif::Reader::new().read_from_container(&mut Cursor::new(bytes)) else {
      return Kept { orientation: 1, artist: None, copyright: None };
    };
    let text = |tag: exif::Tag| match exif.get_field(tag, exif::In::PRIMARY).map(|f| &f.value) {
      Some(exif::Value::Ascii(parts)) => parts.first().filter(|text| !text.is_empty()).cloned(),
      _ => None,
    };
    Kept { orientation: crate::image::orientation(bytes), artist: text(exif::Tag::Artist), copyright: text(exif::Tag::Copyright) }
  }

  /// The kept tags as a little-endian TIFF block, `None` when nothing is kept.
  fn tiff(&self) -> Option<Vec<u8>> {
    let mut entries: Vec<(u16, u16, Vec<u8>)> = Vec::new();
    if self.orientation != 1 {
      entries.push((ORIENTATION, 3, u16::from(self.orientation).to_le_bytes().to_vec()));
    }
    for (tag, text) in [(ARTIST, &self.artist), (COPYRIGHT, &self.copyright)] {
      if let Some(text) = text {
        let mut value = text.clone();
        value.push(0);
        entries.push((tag, 2, value));
      }
    }
    if entries.is_empty() {
      return None;
    }
    let mut out = b"II\x2a\x00\x08\x00\x00\x00".to_vec();
    out.extend((entries.len() as u16).to_le_bytes());
    let mut data_at = 8 + 2 + 12 * entries.len() + 4;
    let mut data = Vec::new();
    for (tag, kind, value) in &entries {
      out.extend(tag.to_le_bytes());
      out.extend(kind.to_le_bytes());
      let count = if *kind == 3 { 1u32 } else { value.len() as u32 };
      out.extend(count.to_le_bytes());
      if value.len() <= 4 {
        let mut inline = value.clone();
        inline.resize(4, 0);
        out.extend(inline);
      } else {
        out.extend((data_at as u32).to_le_bytes());
        data.extend(value);
        if value.len() % 2 == 1 {
          data.push(0);
        }
        data_at = 8 + 2 + 12 * entries.len() + 4 + data.len();
      }
    }
    out.extend(0u32.to_le_bytes());
    out.extend(data);
    Some(out)
  }
}

/// Keeps JFIF, the ICC profile and Adobe's colour transform among the
/// application segments, drops every other one and every comment, and puts
/// the kept EXIF where the source's stood. Everything from the start of scan
/// on is copied as it is.
fn jpeg(bytes: &[u8], kept: &Kept) -> Result<Vec<u8>, String> {
  let mut out = vec![0xff, 0xd8];
  let mut exif_written = false;
  let write_exif = |out: &mut Vec<u8>| {
    if let Some(tiff) = kept.tiff() {
      let length = 2 + 6 + tiff.len();
      out.extend([0xff, 0xe1]);
      out.extend((length as u16).to_be_bytes());
      out.extend(b"Exif\0\0");
      out.extend(tiff);
    }
  };
  let mut at = 2;
  loop {
    if at + 4 > bytes.len() || bytes[at] != 0xff {
      return Err(format!("a JPEG segment at byte {at} has no marker"));
    }
    let marker = bytes[at + 1];
    if marker == 0xda || marker == 0xd9 {
      if !exif_written {
        write_exif(&mut out);
      }
      out.extend(&bytes[at..]);
      return Ok(out);
    }
    let length = u16::from_be_bytes([bytes[at + 2], bytes[at + 3]]) as usize;
    let end = at + 2 + length;
    if length < 2 || end > bytes.len() {
      return Err(format!("a JPEG segment at byte {at} runs past the end"));
    }
    let payload = &bytes[at + 4..end];
    let keep = match marker {
      0xe0 => payload.starts_with(b"JFIF\0") || payload.starts_with(b"JFXX\0"),
      0xe2 => payload.starts_with(b"ICC_PROFILE\0"),
      0xee => payload.starts_with(b"Adobe"),
      0xe1 | 0xe3..=0xed | 0xef | 0xfe => false,
      _ => true,
    };
    let jfif = marker == 0xe0 && keep;
    if !jfif && !exif_written {
      write_exif(&mut out);
      exif_written = true;
    }
    if keep {
      out.extend(&bytes[at..end]);
    }
    at = end;
  }
}

const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

/// Drops the text chunks, the modification time and the EXIF chunk, and
/// writes the kept EXIF as an `eXIf` chunk ahead of the first `IDAT`.
fn png(bytes: &[u8], kept: &Kept) -> Result<Vec<u8>, String> {
  let mut out = PNG_SIGNATURE.to_vec();
  let mut at = PNG_SIGNATURE.len();
  let mut exif_written = false;
  while at < bytes.len() {
    if at + 12 > bytes.len() {
      return Err(format!("a PNG chunk at byte {at} is cut short"));
    }
    let length = u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]) as usize;
    let kind = &bytes[at + 4..at + 8];
    let end = at + 12 + length;
    if end > bytes.len() {
      return Err(format!("a PNG chunk at byte {at} runs past the end"));
    }
    if kind == b"IDAT" && !exif_written {
      if let Some(tiff) = kept.tiff() {
        out.extend(png_chunk(b"eXIf", &tiff));
      }
      exif_written = true;
    }
    if !matches!(kind, b"tEXt" | b"zTXt" | b"iTXt" | b"tIME" | b"eXIf") {
      out.extend(&bytes[at..end]);
    }
    at = end;
  }
  Ok(out)
}

fn png_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
  let mut out = (data.len() as u32).to_be_bytes().to_vec();
  out.extend(kind);
  out.extend(data);
  let mut covered = kind.to_vec();
  covered.extend(data);
  out.extend(crc32(&covered).to_be_bytes());
  out
}

fn crc32(bytes: &[u8]) -> u32 {
  let mut crc = 0xffff_ffffu32;
  for byte in bytes {
    crc ^= u32::from(*byte);
    for _ in 0..8 {
      crc = if crc & 1 == 1 { (crc >> 1) ^ 0xedb8_8320 } else { crc >> 1 };
    }
  }
  !crc
}

/// Drops the `XMP ` chunk, replaces `EXIF` with the kept EXIF and sets the
/// extended header's flags to match. A simple WebP has no extended header
/// and so no metadata, and comes back as it was.
fn webp(bytes: &[u8], kept: &Kept) -> Result<Vec<u8>, String> {
  let mut chunks: Vec<([u8; 4], Vec<u8>)> = Vec::new();
  let mut at = 12;
  while at < bytes.len() {
    if at + 8 > bytes.len() {
      return Err(format!("a WebP chunk at byte {at} is cut short"));
    }
    let kind: [u8; 4] = bytes[at..at + 4].try_into().expect("four bytes");
    let size = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().expect("four bytes")) as usize;
    let end = at + 8 + size;
    if end > bytes.len() {
      return Err(format!("a WebP chunk at byte {at} runs past the end"));
    }
    chunks.push((kind, bytes[at + 8..end].to_vec()));
    at = end + size % 2;
  }
  if !chunks.iter().any(|(kind, _)| kind == b"VP8X") {
    return Ok(bytes.to_vec());
  }
  chunks.retain(|(kind, _)| kind != b"XMP " && kind != b"EXIF");
  let tiff = kept.tiff();
  if let Some(tiff) = &tiff {
    chunks.push((*b"EXIF", tiff.clone()));
  }
  for (kind, data) in chunks.iter_mut() {
    if kind == b"VP8X" && !data.is_empty() {
      data[0] &= !(0x08 | 0x04);
      if tiff.is_some() {
        data[0] |= 0x08;
      }
    }
  }
  let mut body = b"WEBP".to_vec();
  for (kind, data) in &chunks {
    body.extend(kind);
    body.extend((data.len() as u32).to_le_bytes());
    body.extend(data);
    if data.len() % 2 == 1 {
      body.push(0);
    }
  }
  let mut out = b"RIFF".to_vec();
  out.extend((body.len() as u32).to_le_bytes());
  out.extend(body);
  Ok(out)
}

/// Drops comment extensions and every application extension but the
/// looping ones, `NETSCAPE2.0` and `ANIMEXTS1.0`. GIF has no orientation.
fn gif(bytes: &[u8]) -> Result<Vec<u8>, String> {
  let short = |at: usize| format!("a GIF block at byte {at} runs past the end");
  if bytes.len() < 13 {
    return Err(short(0));
  }
  let flags = bytes[10];
  let mut at = 13 + if flags & 0x80 != 0 { 3 * (1 << ((flags & 0x07) + 1)) } else { 0 };
  if at > bytes.len() {
    return Err(short(13));
  }
  let mut out = bytes[..at].to_vec();
  let sub_blocks = |mut at: usize| -> Result<usize, String> {
    loop {
      let size = *bytes.get(at).ok_or_else(|| short(at))? as usize;
      at += 1 + size;
      if size == 0 {
        return Ok(at);
      }
      if at > bytes.len() {
        return Err(short(at));
      }
    }
  };
  loop {
    let start = at;
    match bytes.get(at).copied() {
      Some(0x3b) => {
        out.push(0x3b);
        return Ok(out);
      }
      Some(0x21) => {
        let label = *bytes.get(at + 1).ok_or_else(|| short(at))?;
        let end = sub_blocks(at + 2)?;
        let keep = match label {
          0xfe => false,
          0xff => {
            let id = bytes.get(at + 3..at + 14).ok_or_else(|| short(at))?;
            id == b"NETSCAPE2.0" || id == b"ANIMEXTS1.0"
          }
          _ => true,
        };
        if keep {
          out.extend(&bytes[start..end]);
        }
        at = end;
      }
      Some(0x2c) => {
        let packed = *bytes.get(at + 9).ok_or_else(|| short(at))?;
        let mut data = at + 10 + if packed & 0x80 != 0 { 3 * (1 << ((packed & 0x07) + 1)) } else { 0 };
        data += 1;
        let end = sub_blocks(data)?;
        out.extend(&bytes[start..end]);
        at = end;
      }
      Some(other) => return Err(format!("a GIF block at byte {at} starts with 0x{other:02x}")),
      None => return Err(short(at)),
    }
  }
}
