use tiny_skia::Pixmap;

/// Three box widths whose passes approximate a Gaussian of `sigma`.
fn boxes(sigma: f32) -> [usize; 3] {
  let ideal = (12.0 * sigma * sigma / 3.0 + 1.0).sqrt();
  let mut lower = ideal.floor() as i32;
  if lower % 2 == 0 {
    lower -= 1;
  }
  let upper = lower + 2;
  let m = ((12.0 * sigma * sigma - 3.0 * (lower * lower) as f32 - 12.0 * lower as f32 - 9.0) / (-4.0 * lower as f32 - 4.0)).round() as i32;
  let mut out = [0usize; 3];
  for (i, slot) in out.iter_mut().enumerate() {
    *slot = (if (i as i32) < m { lower } else { upper }).max(1) as usize;
  }
  out
}

/// Blurs the premultiplied pixels inside `region` (x0, y0, x1, y1), which is
/// clamped to the pixmap. Pixels outside the region count as transparent.
pub(crate) fn blur(pixmap: &mut Pixmap, sigma: f32, region: (i32, i32, i32, i32)) {
  if sigma < 0.5 {
    return;
  }
  let (w, h) = (pixmap.width() as i32, pixmap.height() as i32);
  let x0 = region.0.clamp(0, w);
  let y0 = region.1.clamp(0, h);
  let x1 = region.2.clamp(0, w);
  let y1 = region.3.clamp(0, h);
  if x1 <= x0 || y1 <= y0 {
    return;
  }
  let rw = (x1 - x0) as usize;
  let rh = (y1 - y0) as usize;
  let stride = w as usize * 4;
  let data = pixmap.data_mut();
  let mut a: Vec<f32> = vec![0.0; rw * rh * 4];
  for y in 0..rh {
    let row = (y0 as usize + y) * stride + x0 as usize * 4;
    for x in 0..rw * 4 {
      a[y * rw * 4 + x] = data[row + x] as f32;
    }
  }
  let mut b = vec![0.0f32; a.len()];
  for width in boxes(sigma) {
    let r = (width / 2) as i32;
    box_pass(&a, &mut b, rw, rh, r, true);
    box_pass(&b, &mut a, rw, rh, r, false);
  }
  for y in 0..rh {
    let row = (y0 as usize + y) * stride + x0 as usize * 4;
    for x in 0..rw * 4 {
      data[row + x] = a[y * rw * 4 + x].round().clamp(0.0, 255.0) as u8;
    }
  }
  for px in data.chunks_exact_mut(4) {
    let alpha = px[3];
    for c in &mut px[..3] {
      *c = (*c).min(alpha);
    }
  }
}

fn box_pass(src: &[f32], dst: &mut [f32], w: usize, h: usize, r: i32, horizontal: bool) {
  let (len, lines) = if horizontal { (w, h) } else { (h, w) };
  let at = |line: usize, i: usize| if horizontal { (line * w + i) * 4 } else { (i * w + line) * 4 };
  let scale = 1.0 / (2 * r + 1) as f32;
  for line in 0..lines {
    let mut sum = [0f32; 4];
    for i in -r..=r {
      if i >= 0 && (i as usize) < len {
        let p = at(line, i as usize);
        for c in 0..4 {
          sum[c] += src[p + c];
        }
      }
    }
    for i in 0..len {
      let p = at(line, i);
      for c in 0..4 {
        dst[p + c] = sum[c] * scale;
      }
      let out = i as i32 - r;
      let inn = i as i32 + r + 1;
      if out >= 0 {
        let q = at(line, out as usize);
        for c in 0..4 {
          sum[c] -= src[q + c];
        }
      }
      if (inn as usize) < len {
        let q = at(line, inn as usize);
        for c in 0..4 {
          sum[c] += src[q + c];
        }
      }
    }
  }
}
