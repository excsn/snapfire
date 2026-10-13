use miniz_oxide::deflate::core::{compress_to_output, create_comp_flags_from_zip_params, CompressorOxide, TDEFLFlush};

use crate::CardError;

const LEVEL: i32 = 6;

/// Segments the image data deflates in at once. Fixed rather than the
/// machine's core count, so every machine writes the same bytes.
const SEGMENTS: usize = 8;

/// Below this much image data one segment is faster than starting threads.
const PARALLEL_FROM: usize = 256 * 1024;

/// Rows between those the filter choice samples.
const SAMPLE_STRIDE: usize = 32;

#[derive(Clone, Copy, PartialEq)]
enum Filter {
  None,
  Up,
}

/// One row of premultiplied RGBA as PNG wants it: RGB when the image has no
/// transparency, else RGBA with the color divided back out.
fn convert(src: &[u8], alpha: bool, out: &mut Vec<u8>) {
  out.clear();
  if alpha {
    for p in src.chunks_exact(4) {
      let a = p[3] as u16;
      if a == 0 || a == 255 {
        out.extend_from_slice(p);
      } else {
        out.extend([0, 1, 2].map(|i| ((p[i] as u16 * 255 + a / 2) / a).min(255) as u8));
        out.push(p[3]);
      }
    }
  } else {
    for p in src.chunks_exact(4) {
      out.extend_from_slice(&p[..3]);
    }
  }
}

/// The rows `rows` of premultiplied RGBA `rgba`, `width` pixels wide, each
/// converted and filtered with its filter byte in front.
fn scanlines(rgba: &[u8], width: usize, alpha: bool, filter: Filter, rows: impl Iterator<Item = usize>) -> Vec<u8> {
  let stride = width * 4;
  let mut out = Vec::new();
  let mut line = Vec::new();
  let mut above = Vec::new();
  for r in rows {
    convert(&rgba[r * stride..(r + 1) * stride], alpha, &mut line);
    match filter {
      Filter::None => {
        out.push(0);
        out.extend_from_slice(&line);
      }
      Filter::Up => {
        out.push(2);
        if r == 0 {
          out.extend_from_slice(&line);
        } else {
          convert(&rgba[(r - 1) * stride..r * stride], alpha, &mut above);
          out.extend(line.iter().zip(&above).map(|(a, b)| a.wrapping_sub(*b)));
        }
      }
    }
  }
  out
}

/// Raw DEFLATE of `data`. A segment that is not the last ends on a sync
/// flush, so the next segment's blocks can follow it in one stream.
fn deflate(data: &[u8], last: bool, level: i32) -> Vec<u8> {
  let mut compressor = CompressorOxide::new(create_comp_flags_from_zip_params(level, -15, 0));
  let mut out = Vec::with_capacity(data.len() / 4);
  compress_to_output(&mut compressor, data, if last { TDEFLFlush::Finish } else { TDEFLFlush::Sync }, |bytes| {
    out.extend_from_slice(bytes);
    true
  });
  out
}

fn adler32(data: &[u8]) -> u32 {
  let (mut a, mut b) = (1u32, 0u32);
  for chunk in data.chunks(5552) {
    for &byte in chunk {
      a += byte as u32;
      b += a;
    }
    a %= 65521;
    b %= 65521;
  }
  (b << 16) | a
}

/// The Adler-32 of two pieces joined, from each piece's checksum and the
/// second piece's length, as zlib's `adler32_combine` computes it.
fn adler32_combine(first: u32, second: u32, second_len: usize) -> u32 {
  const BASE: u64 = 65521;
  let rem = second_len as u64 % BASE;
  let mut sum1 = (first & 0xffff) as u64;
  let mut sum2 = (rem * sum1) % BASE;
  sum1 += (second & 0xffff) as u64 + BASE - 1;
  sum2 += (first >> 16) as u64 + (second >> 16) as u64 + BASE - rem;
  sum1 %= BASE;
  sum2 %= BASE;
  ((sum2 << 16) | sum1) as u32
}

/// A zlib stream of the image's filtered rows, each segment of rows
/// converted, filtered, deflated and checksummed on its own scoped thread.
fn zlib(rgba: &[u8], width: usize, rows: usize, alpha: bool, filter: Filter) -> Vec<u8> {
  let row = width * if alpha { 4 } else { 3 } + 1;
  let segments = if rows * row < PARALLEL_FROM { 1 } else { SEGMENTS.min(rows.max(1)) };
  let per = rows.div_ceil(segments.max(1)).max(1);
  let ranges: Vec<(usize, usize)> = (0..rows).step_by(per).map(|start| (start, (start + per).min(rows))).collect();
  let count = ranges.len();
  let segment = |i: usize, (start, end): (usize, usize)| {
    let data = scanlines(rgba, width, alpha, filter, start..end);
    (deflate(&data, i + 1 == count, LEVEL), adler32(&data), data.len())
  };
  let done: Vec<(Vec<u8>, u32, usize)> = std::thread::scope(|scope| {
    let handles: Vec<_> = ranges.iter().enumerate().skip(1).map(|(i, r)| scope.spawn(move || segment(i, *r))).collect();
    let first = match ranges.first() {
      Some(r) => segment(0, *r),
      None => (deflate(&[], true, LEVEL), 1, 0),
    };
    std::iter::once(first).chain(handles.into_iter().map(|h| h.join().expect("a deflate thread panicked"))).collect()
  });
  let mut out = Vec::with_capacity(done.iter().map(|d| d.0.len()).sum::<usize>() + 6);
  out.extend_from_slice(&[0x78, 0x9C]);
  let mut checksum = 1u32;
  for (bytes, adler, len) in done {
    out.extend_from_slice(&bytes);
    checksum = adler32_combine(checksum, adler, len);
  }
  out.extend_from_slice(&checksum.to_be_bytes());
  out
}

/// A PNG of a premultiplied RGBA image, written as RGB when every pixel is
/// opaque. The row filter is whichever of none and up deflates a sample of
/// every 32nd row smaller: flat art and gradients take none, photographs
/// take up.
pub(crate) fn png(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>, CardError> {
  let alpha = rgba.chunks_exact(4).any(|p| p[3] != 255);
  let (w, rows) = (width as usize, height as usize);
  let sample = |f| deflate(&scanlines(rgba, w, alpha, f, (0..rows).step_by(SAMPLE_STRIDE)), true, 1).len();
  let filter = if rows > SAMPLE_STRIDE && sample(Filter::Up) < sample(Filter::None) { Filter::Up } else { Filter::None };
  let stream = zlib(rgba, w, rows, alpha, filter);

  let mut out = Vec::with_capacity(stream.len() + 64);
  let mut encoder = png::Encoder::new(&mut out, width, height);
  encoder.set_color(if alpha { png::ColorType::Rgba } else { png::ColorType::Rgb });
  encoder.set_depth(png::BitDepth::Eight);
  let mut writer = encoder.write_header().map_err(|e| CardError::Encode(e.to_string()))?;
  writer.write_chunk(png::chunk::IDAT, &stream).map_err(|e| CardError::Encode(e.to_string()))?;
  writer.finish().map_err(|e| CardError::Encode(e.to_string()))?;
  Ok(out)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn adler32_matches_the_rfc_example() {
    assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
  }

  #[test]
  fn combined_checksums_equal_the_checksum_of_the_whole() {
    let data: Vec<u8> = (0..100_000u32).map(|i| (i * 7 % 256) as u8).collect();
    let (a, b) = data.split_at(31_337);
    assert_eq!(adler32_combine(adler32(a), adler32(b), b.len()), adler32(&data));
    assert_eq!(adler32_combine(1, adler32(&data), data.len()), adler32(&data));
  }

  #[test]
  fn a_segmented_png_decodes_to_its_pixels() {
    let (w, h) = (700u32, 500u32);
    let pixels: Vec<u8> = (0..w * h).flat_map(|i| [(i % 251) as u8, (i / w) as u8, 7]).collect();
    let rgba: Vec<u8> = pixels.chunks_exact(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect();
    let bytes = png(&rgba, w, h).unwrap();
    let mut reader = png::Decoder::new(std::io::Cursor::new(&bytes)).read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    reader.next_frame(&mut buf).unwrap();
    assert_eq!(buf, pixels);
  }
}
