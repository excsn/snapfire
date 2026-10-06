//! A served original keeps its pixels, its orientation, its colour profile
//! and its credit, and loses everything else a camera or an editor wrote.

use std::io::Cursor;

use snapfire_media::strip::{carries_metadata, strip};

fn fixture(name: &str) -> Vec<u8> {
  std::fs::read(format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

fn exif_of(bytes: &[u8]) -> Option<exif::Exif> {
  exif::Reader::new().read_from_container(&mut Cursor::new(bytes)).ok()
}

fn text(exif: &exif::Exif, tag: exif::Tag) -> Option<String> {
  match exif.get_field(tag, exif::In::PRIMARY).map(|f| &f.value) {
    Some(exif::Value::Ascii(parts)) => parts.first().map(|p| String::from_utf8_lossy(p).into_owned()),
    _ => None,
  }
}

/// A little-endian TIFF block: orientation 6, a camera make, an artist, a
/// copyright and a GPS directory holding a latitude reference.
fn camera_tiff() -> Vec<u8> {
  let entries: [(u16, u16, u32, Vec<u8>); 5] = [
    (0x010f, 2, 6, b"Phone\0".to_vec()),
    (0x0112, 3, 1, 6u16.to_le_bytes().to_vec()),
    (0x013b, 2, 4, b"Ann\0".to_vec()),
    (0x8298, 2, 9, b"(c) Ann!\0".to_vec()),
    (0x8825, 4, 1, Vec::new()),
  ];
  let ifd_end = 8 + 2 + 12 * entries.len() + 4;
  let mut data = Vec::new();
  let mut out = b"II\x2a\x00\x08\x00\x00\x00".to_vec();
  out.extend((entries.len() as u16).to_le_bytes());
  let mut gps_at_slot = 0;
  for (tag, kind, count, value) in &entries {
    out.extend(tag.to_le_bytes());
    out.extend(kind.to_le_bytes());
    out.extend(count.to_le_bytes());
    if *tag == 0x8825 {
      gps_at_slot = out.len();
      out.extend([0u8; 4]);
    } else if value.len() <= 4 {
      let mut inline = value.clone();
      inline.resize(4, 0);
      out.extend(inline);
    } else {
      out.extend(((ifd_end + data.len()) as u32).to_le_bytes());
      data.extend(value);
      if value.len() % 2 == 1 {
        data.push(0);
      }
    }
  }
  out.extend(0u32.to_le_bytes());
  out.extend(&data);
  let gps_at = out.len() as u32;
  out[gps_at_slot..gps_at_slot + 4].copy_from_slice(&gps_at.to_le_bytes());
  out.extend(1u16.to_le_bytes());
  out.extend([0x01, 0x00, 0x02, 0x00, 0x02, 0x00, 0x00, 0x00, b'N', 0, 0, 0]);
  out.extend(0u32.to_le_bytes());
  out
}

fn segment(marker: u8, payload: &[u8]) -> Vec<u8> {
  let mut out = vec![0xff, marker];
  out.extend(((payload.len() + 2) as u16).to_be_bytes());
  out.extend(payload);
  out
}

/// `tagged.jpg` with its EXIF replaced by a camera's and an editor's blocks
/// added around it: XMP, an ICC profile, IPTC and a comment.
fn camera_jpeg() -> Vec<u8> {
  let source = fixture("tagged.jpg");
  let jfif_at = 2 + 2 + u16::from_be_bytes([source[4], source[5]]) as usize;
  let mut exif = b"Exif\0\0".to_vec();
  exif.extend(camera_tiff());
  let mut out = vec![0xff, 0xd8];
  out.extend(segment(0xe1, &exif));
  out.extend(segment(0xe1, b"http://ns.adobe.com/xap/1.0/\0<x:xmpmeta>GPS 51.5</x:xmpmeta>"));
  out.extend(segment(0xe2, b"ICC_PROFILE\0\x01\x01profile-bytes"));
  out.extend(segment(0xed, b"Photoshop 3.0\0iptc-by-line"));
  out.extend(segment(0xfe, b"shot on a phone"));
  out.extend(&source[jfif_at..]);
  out
}

fn scan_of(bytes: &[u8]) -> &[u8] {
  let at = bytes.windows(2).position(|w| w == [0xff, 0xda]).expect("a start of scan");
  &bytes[at..]
}

#[test]
fn a_jpeg_keeps_its_orientation_profile_and_credit_and_loses_the_rest() {
  let source = camera_jpeg();
  assert!(exif_of(&source).unwrap().get_field(exif::Tag::GPSLatitudeRef, exif::In::PRIMARY).is_some(), "the source carries a position");
  let stripped = strip(&source).unwrap().expect("a JPEG is rewritten");
  let exif = exif_of(&stripped).expect("the kept EXIF reads back");
  assert_eq!(exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY).and_then(|f| f.value.get_uint(0)), Some(6));
  assert_eq!(text(&exif, exif::Tag::Artist).as_deref(), Some("Ann"));
  assert_eq!(text(&exif, exif::Tag::Copyright).as_deref(), Some("(c) Ann!"));
  assert!(exif.get_field(exif::Tag::Make, exif::In::PRIMARY).is_none(), "the camera goes");
  assert!(exif.get_field(exif::Tag::GPSLatitudeRef, exif::In::PRIMARY).is_none(), "the position goes");
  let has = |needle: &[u8]| stripped.windows(needle.len()).any(|w| w == needle);
  assert!(has(b"ICC_PROFILE\0") && has(b"JFIF\0"), "the colour profile and JFIF stay");
  assert!(!has(b"xmpmeta") && !has(b"Photoshop") && !has(b"shot on a phone"), "XMP, IPTC and the comment go");
  assert_eq!(scan_of(&stripped), scan_of(&source), "the image data is the source's, byte for byte");
  assert_eq!(snapfire_media::image::orientation(&stripped), 6, "the browser and the build read the same orientation");
}

#[test]
fn a_file_with_nothing_to_keep_carries_no_exif_at_all() {
  let mut source = vec![0xff, 0xd8];
  let tagged = fixture("tagged.jpg");
  let jfif_at = 2 + 2 + u16::from_be_bytes([tagged[4], tagged[5]]) as usize;
  source.extend(segment(0xe1, b"Exif\0\0II\x2a\x00\x08\x00\x00\x00\x00\x00\x00\x00\x00\x00"));
  source.extend(&tagged[jfif_at..]);
  let stripped = strip(&source).unwrap().unwrap();
  assert!(!stripped.windows(6).any(|w| w == b"Exif\0\0"), "an orientation of 1 and no credit leave nothing to write");
  assert!(!carries_metadata(&stripped));
}

#[test]
fn a_png_drops_its_text_and_keeps_its_orientation() {
  let source = fixture("tagged.png");
  let mut with_text = source[..33].to_vec();
  let mut text = b"Software".to_vec();
  text.push(0);
  text.extend(b"Phone camera");
  let chunk = |kind: &[u8], data: &[u8]| {
    let mut out = (data.len() as u32).to_be_bytes().to_vec();
    out.extend(kind);
    out.extend(data);
    let mut covered = kind.to_vec();
    covered.extend(data);
    out.extend(crc(&covered).to_be_bytes());
    out
  };
  with_text.extend(chunk(b"tEXt", &text));
  with_text.extend(&source[33..]);
  let stripped = strip(&with_text).unwrap().expect("a PNG is rewritten");
  assert!(!stripped.windows(12).any(|w| w == b"Phone camera"));
  assert_eq!(snapfire_media::image::orientation(&stripped), snapfire_media::image::orientation(&source));
  assert_ne!(snapfire_media::image::orientation(&source), 1, "the fixture is tagged");
  let idat = |bytes: &[u8]| bytes.windows(4).position(|w| w == b"IDAT").map(|at| bytes[at..].to_vec());
  assert_eq!(idat(&stripped), idat(&source), "the image data is the source's");
  let mut at = 8;
  while at < stripped.len() {
    let length = u32::from_be_bytes(stripped[at..at + 4].try_into().unwrap()) as usize;
    let covered = &stripped[at + 4..at + 8 + length];
    let written = u32::from_be_bytes(stripped[at + 8 + length..at + 12 + length].try_into().unwrap());
    assert_eq!(written, crc(covered), "every chunk's CRC holds");
    at += 12 + length;
  }
}

fn crc(bytes: &[u8]) -> u32 {
  let mut crc = 0xffff_ffffu32;
  for byte in bytes {
    crc ^= u32::from(*byte);
    for _ in 0..8 {
      crc = if crc & 1 == 1 { (crc >> 1) ^ 0xedb8_8320 } else { crc >> 1 };
    }
  }
  !crc
}

#[test]
fn a_webp_drops_its_xmp_keeps_its_orientation_and_sets_its_flags_to_match() {
  let source = fixture("tagged.webp");
  let mut body = source[12..].to_vec();
  let xmp = b"<x:xmpmeta>GPS</x:xmpmeta>";
  body.extend(b"XMP ");
  body.extend((xmp.len() as u32).to_le_bytes());
  body.extend(xmp);
  let mut with_xmp = b"RIFF".to_vec();
  with_xmp.extend(((body.len() + 4) as u32).to_le_bytes());
  with_xmp.extend(b"WEBP");
  with_xmp.extend(body);
  if let Some(at) = with_xmp.windows(4).position(|w| w == b"VP8X") {
    with_xmp[at + 8] |= 0x04;
  }
  let stripped = strip(&with_xmp).unwrap().expect("a WebP is rewritten");
  assert!(!stripped.windows(9).any(|w| w == b"xmpmeta>G"));
  assert_eq!(snapfire_media::image::orientation(&stripped), snapfire_media::image::orientation(&source));
  assert_eq!(u32::from_le_bytes(stripped[4..8].try_into().unwrap()) as usize, stripped.len() - 8, "the RIFF size is the file's");
  let flags = stripped[stripped.windows(4).position(|w| w == b"VP8X").unwrap() + 8];
  assert_eq!(flags & 0x04, 0, "no XMP flag");
  assert_eq!(flags & 0x08, 0x08, "the EXIF flag, for the orientation kept");
}

#[test]
fn a_gif_drops_its_comments_and_metadata_and_keeps_its_loop() {
  let mut gif = b"GIF89a\x01\x00\x01\x00\x00\x00\x00".to_vec();
  gif.extend(b"\x21\xff\x0bNETSCAPE2.0\x03\x01\x00\x00\x00");
  gif.extend(b"\x21\xff\x0bXMP DataXMP\x05GPS!!\x00");
  gif.extend(b"\x21\xfe\x07a phone\x00");
  gif.extend(b"\x2c\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02\x44\x01\x00");
  gif.push(0x3b);
  let stripped = strip(&gif).unwrap().expect("a GIF is rewritten");
  let has = |needle: &[u8]| stripped.windows(needle.len()).any(|w| w == needle);
  assert!(has(b"NETSCAPE2.0"), "the loop stays");
  assert!(!has(b"XMP DataXMP") && !has(b"a phone"), "XMP and the comment go");
  assert!(stripped.ends_with(b"\x2c\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02\x44\x01\x00\x3b"), "the frame is the source's");
}

#[test]
fn a_format_it_does_not_rewrite_is_left_and_a_broken_one_is_refused() {
  assert!(strip(b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>").unwrap().is_none());
  assert!(strip(b"\x00\x00\x00\x1cftypavif").unwrap().is_none(), "AVIF is reported, not rewritten");
  assert!(strip(&[0xff, 0xd8, 0xff, 0xe1, 0xff, 0xff]).is_err(), "a segment running past the end");
}
