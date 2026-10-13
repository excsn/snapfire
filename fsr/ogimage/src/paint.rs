use std::collections::HashMap;
use std::f32::consts::PI;

use rustybuzz::ttf_parser;
use tiny_skia::{FillRule, FilterQuality, GradientStop, LinearGradient, Mask, Paint, Path, PathBuilder, Pattern, Pixmap, Point, RadialGradient, Rect, Shader, SpreadMode, Transform};

use crate::blur::blur;
use crate::css::{BgSize, Color, Fit, Image, LenAuto, LinearDir, RadialSize, Repeat, Stop, Style, TextAlign, TransformFn};
use crate::fonts::{Faces, Raster, RasterKey};
use crate::images::Images;
use crate::layout::{Kind, Tree};
use crate::text::{Glyph, Line};

const KAPPA: f32 = 0.552_284_8;

type Radii = [(f32, f32); 4];

struct Painter<'a> {
  tree: &'a Tree,
  faces: &'a Faces<'a>,
  images: &'a Images,
  width: u32,
  height: u32,
  outlines: HashMap<(usize, u16), Option<Path>>,
  /// Opacity folded into a leaf's text color instead of a layer.
  alpha: f32,
}

pub(crate) fn paint(tree: &Tree, faces: &Faces<'_>, images: &Images, width: u32, height: u32) -> Pixmap {
  let mut canvas = Pixmap::new(width, height).expect("the card's size was checked");
  let mut painter = Painter { tree, faces, images, width, height, outlines: HashMap::new(), alpha: 1.0 };
  painter.node(&mut canvas, tree.root, Transform::identity(), None);
  canvas
}

/// Draws `layer` over `canvas` inside `area` only, since a layer is the
/// card's size and most of it is empty.
fn composite(canvas: &mut Pixmap, layer: &Pixmap, area: Option<Rect>, opacity: f32, mask: Option<&Mask>) {
  let full = Rect::from_xywh(0.0, 0.0, canvas.width() as f32, canvas.height() as f32);
  let Some(rect) = area.and_then(|a| Rect::from_ltrb(a.left().max(0.0), a.top().max(0.0), a.right().min(canvas.width() as f32), a.bottom().min(canvas.height() as f32))).or(if area.is_none() { full } else { None }) else { return };
  let paint = Paint { shader: Pattern::new(layer.as_ref(), SpreadMode::Pad, FilterQuality::Nearest, opacity, Transform::identity()), ..Paint::default() };
  canvas.fill_rect(rect, &paint, Transform::identity(), mask);
}

/// Source-over of premultiplied `src` onto `canvas` with its top left at
/// `(x, y)`, scaled by `clip`'s coverage where there is a clip.
fn over(canvas: &mut Pixmap, src: &Pixmap, x: i32, y: i32, clip: Option<&Mask>) {
  let (cw, ch) = (canvas.width() as i32, canvas.height() as i32);
  let (sw, sh) = (src.width() as i32, src.height() as i32);
  let (x0, y0) = (x.max(0), y.max(0));
  let (x1, y1) = ((x + sw).min(cw), (y + sh).min(ch));
  if x1 <= x0 || y1 <= y0 {
    return;
  }
  let s = src.data();
  let mask = clip.map(Mask::data);
  let d = canvas.data_mut();
  for py in y0..y1 {
    for px in x0..x1 {
      let si = (((py - y) * sw + (px - x)) * 4) as usize;
      let mut sa = s[si + 3] as u32;
      if sa == 0 {
        continue;
      }
      let ci = (py * cw + px) as usize;
      let cover = mask.map_or(255, |m| m[ci] as u32);
      if cover == 0 {
        continue;
      }
      let di = ci * 4;
      let scale = |v: u32| if cover == 255 { v } else { (v * cover + 127) / 255 };
      sa = scale(sa);
      let keep = 255 - sa;
      for c in 0..3 {
        d[di + c] = (scale(s[si + c] as u32) + (d[di + c] as u32 * keep + 127) / 255).min(255) as u8;
      }
      d[di + 3] = (sa + (d[di + 3] as u32 * keep + 127) / 255).min(255) as u8;
    }
  }
}

fn union(a: Option<Rect>, b: Option<Rect>) -> Option<Rect> {
  match (a, b) {
    (Some(a), Some(b)) => Rect::from_ltrb(a.left().min(b.left()), a.top().min(b.top()), a.right().max(b.right()), a.bottom().max(b.bottom())),
    (a, None) => a,
    (None, b) => b,
  }
}

fn solid(color: Color) -> Paint<'static> {
  let mut paint = Paint::default();
  paint.set_color(color.skia());
  paint.anti_alias = true;
  paint
}

fn shaded(shader: Shader<'_>) -> Paint<'_> {
  Paint { shader, anti_alias: true, ..Paint::default() }
}

fn corner(r: (f32, f32)) -> (f32, f32) {
  if r.0 <= 0.0 || r.1 <= 0.0 { (0.0, 0.0) } else { r }
}

/// A rounded rectangle with each corner's radii, top left first, clockwise.
fn rrect(x: f32, y: f32, w: f32, h: f32, r: Radii) -> Option<Path> {
  if w <= 0.0 || h <= 0.0 {
    return None;
  }
  let [tl, tr, br, bl] = r.map(corner);
  let mut pb = PathBuilder::new();
  pb.move_to(x + tl.0, y);
  pb.line_to(x + w - tr.0, y);
  if tr.0 > 0.0 {
    pb.cubic_to(x + w - tr.0 + tr.0 * KAPPA, y, x + w, y + tr.1 - tr.1 * KAPPA, x + w, y + tr.1);
  }
  pb.line_to(x + w, y + h - br.1);
  if br.0 > 0.0 {
    pb.cubic_to(x + w, y + h - br.1 + br.1 * KAPPA, x + w - br.0 + br.0 * KAPPA, y + h, x + w - br.0, y + h);
  }
  pb.line_to(x + bl.0, y + h);
  if bl.0 > 0.0 {
    pb.cubic_to(x + bl.0 - bl.0 * KAPPA, y + h, x, y + h - bl.1 + bl.1 * KAPPA, x, y + h - bl.1);
  }
  pb.line_to(x, y + tl.1);
  if tl.0 > 0.0 {
    pb.cubic_to(x, y + tl.1 - tl.1 * KAPPA, x + tl.0 - tl.0 * KAPPA, y, x + tl.0, y);
  }
  pb.close();
  pb.finish()
}

/// The border radii at the box's size, scaled down together when two
/// corners on one side would overlap, as CSS does.
fn radii(s: &Style, w: f32, h: f32) -> Radii {
  let mut r: Radii = s.radius.map(|(x, y)| (x.resolve(w).max(0.0), y.resolve(h).max(0.0)));
  let mut f = 1.0f32;
  for (sum, side) in [(r[0].0 + r[1].0, w), (r[3].0 + r[2].0, w), (r[0].1 + r[3].1, h), (r[1].1 + r[2].1, h)] {
    if sum > side && sum > 0.0 {
      f = f.min(side / sum);
    }
  }
  if f < 1.0 {
    for c in &mut r {
      c.0 *= f;
      c.1 *= f;
    }
  }
  r
}

fn inner_radii(r: Radii, b: [f32; 4]) -> Radii {
  let [t, rt, bt, l] = b;
  [(r[0].0 - l, r[0].1 - t), (r[1].0 - rt, r[1].1 - t), (r[2].0 - rt, r[2].1 - bt), (r[3].0 - l, r[3].1 - bt)].map(|(x, y)| (x.max(0.0), y.max(0.0)))
}

fn grown(r: Radii, by: f32) -> Radii {
  r.map(|(x, y)| if x > 0.0 && y > 0.0 { ((x + by).max(0.0), (y + by).max(0.0)) } else { (0.0, 0.0) })
}

fn css_matrix(fns: &[TransformFn], w: f32, h: f32) -> Transform {
  let mut m = Transform::identity();
  for f in fns {
    let step = match *f {
      TransformFn::Translate(x, y) => Transform::from_translate(x.resolve(w), y.resolve(h)),
      TransformFn::Rotate(deg) => Transform::from_rotate(deg),
      TransformFn::Scale(x, y) => Transform::from_scale(x, y),
      TransformFn::Skew(ax, ay) => Transform::from_skew((ax * PI / 180.0).tan(), (ay * PI / 180.0).tan()),
      TransformFn::Matrix([a, b, c, d, e, f]) => Transform::from_row(a, b, c, d, e, f),
    };
    m = m.pre_concat(step);
  }
  m
}

impl<'a> Painter<'a> {
  fn mask_of(&self, path: &Path, t: Transform, clip: Option<&Mask>) -> Mask {
    let mut mask = Mask::new(self.width, self.height).expect("the card's size was checked");
    mask.fill_path(path, FillRule::Winding, true, t);
    if let Some(c) = clip {
      for (a, b) in mask.data_mut().iter_mut().zip(c.data()) {
        *a = ((*a as u16 * *b as u16 + 127) / 255) as u8;
      }
    }
    mask
  }

  fn layer(&self) -> Pixmap {
    Pixmap::new(self.width, self.height).expect("the card's size was checked")
  }

  /// A node's size and the transform from its own box to the canvas.
  fn placed_box(&self, index: usize, parent: Transform) -> Option<(Transform, f32, f32)> {
    let node = &self.tree.nodes[index];
    let s = &node.style;
    let layout = self.tree.taffy.layout(node.id).ok()?;
    let (w, h) = (layout.size.width, layout.size.height);
    let mut t = parent.pre_translate(layout.location.x, layout.location.y);
    if !s.transform.is_empty() {
      let (ox, oy) = (s.transform_origin.0.resolve(w), s.transform_origin.1.resolve(h));
      t = t.pre_translate(ox, oy).pre_concat(css_matrix(&s.transform, w, h)).pre_translate(-ox, -oy);
    }
    Some((t, w, h))
  }

  /// The canvas area a node and its descendants can draw into: each box
  /// grown by its shadows and its text's shadows.
  fn extent(&self, index: usize, parent: Transform) -> Option<Rect> {
    let node = &self.tree.nodes[index];
    if node.style.display == crate::css::Display::None {
      return None;
    }
    let (t, w, h) = self.placed_box(index, parent)?;
    let s = &node.style;
    let shadow = s.box_shadow.iter().chain(&s.text_shadow).map(|sh| sh.blur * 1.5 + sh.spread.abs() + sh.x.abs().max(sh.y.abs())).fold(0.0, f32::max);
    let grow = shadow + 2.0;
    let own = PathBuilder::from_rect(Rect::from_xywh(-grow, -grow, w + 2.0 * grow, h + 2.0 * grow)?).transform(t).map(|p| p.bounds());
    node.children.iter().fold(own, |acc, &c| union(acc, self.extent(c, t)))
  }

  fn node(&mut self, canvas: &mut Pixmap, index: usize, parent: Transform, clip: Option<&Mask>) {
    let node = &self.tree.nodes[index];
    let s = &node.style;
    if s.display == crate::css::Display::None || s.opacity <= 0.0 {
      return;
    }
    let Some((t, w, h)) = self.placed_box(index, parent) else { return };
    let bare_text = matches!(node.kind, Kind::Text(_)) && node.children.is_empty() && !s.background_color.resolve(s.color).is_visible() && s.background_images.is_empty() && s.box_shadow.is_empty() && s.text_shadow.is_empty() && (0..4).all(|i| s.border(i) == 0.0) && !s.text_decoration.underline && !s.text_decoration.line_through;
    if s.opacity < 1.0 && bare_text {
      let outer = self.alpha;
      self.alpha *= s.opacity;
      self.contents(canvas, index, t, w, h, clip);
      self.alpha = outer;
    } else if s.opacity < 1.0 {
      let mut layer = self.layer();
      self.contents(&mut layer, index, t, w, h, clip);
      let area = self.extent(index, parent);
      composite(canvas, &layer, area, s.opacity, None);
    } else {
      self.contents(canvas, index, t, w, h, clip);
    }
  }

  fn contents(&mut self, canvas: &mut Pixmap, index: usize, t: Transform, w: f32, h: f32, clip: Option<&Mask>) {
    let tree = self.tree;
    let node = &tree.nodes[index];
    let s = &node.style;
    let border = [s.border(0), s.border(1), s.border(2), s.border(3)];
    let r = radii(s, w, h);
    let rounded = r.iter().any(|c| c.0 > 0.0 && c.1 > 0.0);
    let Some(outer) = rrect(0.0, 0.0, w, h, r) else {
      self.children(canvas, index, t, clip);
      return;
    };
    let inner_r = inner_radii(r, border);
    let (px, py, pw, ph) = (border[3], border[0], w - border[1] - border[3], h - border[0] - border[2]);
    let inner = rrect(px, py, pw, ph, inner_r);

    self.outer_shadows(canvas, s, &outer, t, w, h, r, clip);

    let background = s.background_color.resolve(s.color);
    let box_mask = if rounded && !s.background_images.is_empty() { Some(self.mask_of(&outer, t, clip)) } else { None };
    if background.is_visible() {
      canvas.fill_path(&outer, &solid(background), FillRule::Winding, t, clip);
    }
    for (i, image) in s.background_images.iter().enumerate().rev() {
      self.background(canvas, s, i, image, (px, py, pw, ph), (w, h), t, box_mask.as_ref().or(clip));
    }

    if let Some(inner) = &inner {
      self.inset_shadows(canvas, s, inner, (px, py, pw, ph), inner_r, t, clip);
    }
    self.borders(canvas, s, &outer, inner.as_ref(), border, w, h, t, clip);

    let content_clip = match (&inner, s.overflow_hidden || (rounded && matches!(node.kind, Kind::Image(_)))) {
      (Some(inner), true) => Some(self.mask_of(inner, t, clip)),
      _ => None,
    };
    let clip_inside = content_clip.as_ref().or(clip);
    let layout = tree.taffy.layout(node.id).expect("laid out");
    let cx = border[3] + layout.padding.left;
    let cy = border[0] + layout.padding.top;
    let cw = (w - cx - border[1] - layout.padding.right).max(0.0);
    let ch = (h - cy - border[2] - layout.padding.bottom).max(0.0);
    match &node.kind {
      Kind::Text(_) => {
        if let Some(lines) = tree.lines(index, self.faces) {
          self.text(canvas, index, &lines.lines, cx, cy, cw, t, clip_inside);
        }
      }
      Kind::Image(pixmap) => self.image(canvas, s, pixmap, (cx, cy, cw, ch), t, clip_inside),
      Kind::Box => {}
    }
    let children_clip = if s.overflow_hidden { clip_inside } else { clip };
    self.children(canvas, index, t, children_clip);
  }

  fn children(&mut self, canvas: &mut Pixmap, index: usize, t: Transform, clip: Option<&Mask>) {
    let children = self.tree.nodes[index].children.clone();
    for child in children {
      self.node(canvas, child, t, clip);
    }
  }

  fn blurred(&self, canvas: &mut Pixmap, layer: &mut Pixmap, sigma: f32, bounds: Option<Rect>, mask: Option<&Mask>) {
    let Some(b) = bounds else { return };
    let pad = (sigma * 3.0).ceil() + 2.0;
    let region = Rect::from_ltrb((b.left() - pad).floor(), (b.top() - pad).floor(), (b.right() + pad).ceil(), (b.bottom() + pad).ceil());
    if let Some(r) = region {
      blur(layer, sigma, (r.left() as i32, r.top() as i32, r.right() as i32, r.bottom() as i32));
    }
    composite(canvas, layer, region, 1.0, mask);
  }

  #[allow(clippy::too_many_arguments)]
  fn outer_shadows(&self, canvas: &mut Pixmap, s: &Style, outer: &Path, t: Transform, w: f32, h: f32, r: Radii, clip: Option<&Mask>) {
    let shadows: Vec<_> = s.box_shadow.iter().filter(|sh| !sh.inset).collect();
    if shadows.is_empty() {
      return;
    }
    let mut outside = self.mask_of(outer, t, None);
    outside.invert();
    if let Some(c) = clip {
      for (a, b) in outside.data_mut().iter_mut().zip(c.data()) {
        *a = ((*a as u16 * *b as u16 + 127) / 255) as u8;
      }
    }
    for sh in shadows.into_iter().rev() {
      let color = sh.color.resolve(s.color);
      let Some(path) = rrect(sh.x - sh.spread, sh.y - sh.spread, w + 2.0 * sh.spread, h + 2.0 * sh.spread, grown(r, sh.spread)) else { continue };
      let mut layer = self.layer();
      layer.fill_path(&path, &solid(color), FillRule::Winding, t, None);
      let bounds = path.clone().transform(t).map(|p| p.bounds());
      self.blurred(canvas, &mut layer, sh.blur / 2.0, bounds, Some(&outside));
    }
  }

  #[allow(clippy::too_many_arguments)]
  fn inset_shadows(&self, canvas: &mut Pixmap, s: &Style, inner: &Path, area: (f32, f32, f32, f32), inner_r: Radii, t: Transform, clip: Option<&Mask>) {
    let shadows: Vec<_> = s.box_shadow.iter().filter(|sh| sh.inset).collect();
    if shadows.is_empty() {
      return;
    }
    let mask = self.mask_of(inner, t, clip);
    let (x, y, w, h) = area;
    for sh in shadows.into_iter().rev() {
      let color = sh.color.resolve(s.color);
      let pad = sh.blur + sh.spread.abs() + sh.x.abs() + sh.y.abs() + 4.0;
      let mut pb = PathBuilder::new();
      if let Some(rect) = Rect::from_xywh(x - pad, y - pad, w + 2.0 * pad, h + 2.0 * pad) {
        pb.push_rect(rect);
      }
      if let Some(hole) = rrect(x + sh.x + sh.spread, y + sh.y + sh.spread, w - 2.0 * sh.spread, h - 2.0 * sh.spread, grown(inner_r, -sh.spread)) {
        pb.push_path(&hole);
      }
      let Some(path) = pb.finish() else { continue };
      let mut layer = self.layer();
      layer.fill_path(&path, &solid(color), FillRule::EvenOdd, t, None);
      let bounds = path.clone().transform(t).map(|p| p.bounds());
      self.blurred(canvas, &mut layer, sh.blur / 2.0, bounds, Some(&mask));
    }
  }

  #[allow(clippy::too_many_arguments)]
  fn borders(&self, canvas: &mut Pixmap, s: &Style, outer: &Path, inner: Option<&Path>, b: [f32; 4], w: f32, h: f32, t: Transform, clip: Option<&Mask>) {
    if b.iter().all(|w| *w <= 0.0) {
      return;
    }
    let mut pb = PathBuilder::new();
    pb.push_path(outer);
    if let Some(inner) = inner {
      pb.push_path(inner);
    }
    let Some(ring) = pb.finish() else { return };
    let colors: Vec<Color> = s.border_color.iter().map(|c| c.resolve(s.color)).collect();
    let present: Vec<usize> = (0..4).filter(|&i| b[i] > 0.0).collect();
    if present.iter().all(|&i| colors[i] == colors[present[0]]) {
      canvas.fill_path(&ring, &solid(colors[present[0]]), FillRule::EvenOdd, t, clip);
      return;
    }
    let [bt, br, bb, bl] = b;
    let sides: [[(f32, f32); 4]; 4] = [
      [(0.0, 0.0), (w, 0.0), (w - br, bt), (bl, bt)],
      [(w, 0.0), (w, h), (w - br, h - bb), (w - br, bt)],
      [(w, h), (0.0, h), (bl, h - bb), (w - br, h - bb)],
      [(0.0, h), (0.0, 0.0), (bl, bt), (bl, h - bb)],
    ];
    for &i in &present {
      let mut pb = PathBuilder::new();
      let pts = sides[i];
      pb.move_to(pts[0].0, pts[0].1);
      for p in &pts[1..] {
        pb.line_to(p.0, p.1);
      }
      pb.close();
      let Some(side) = pb.finish() else { continue };
      let mask = self.mask_of(&side, t, clip);
      canvas.fill_path(&ring, &solid(colors[i]), FillRule::EvenOdd, t, Some(&mask));
    }
  }

  #[allow(clippy::too_many_arguments)]
  fn background(&self, canvas: &mut Pixmap, s: &Style, i: usize, image: &Image, area: (f32, f32, f32, f32), size: (f32, f32), t: Transform, mask: Option<&Mask>) {
    let (ax, ay, aw, ah) = area;
    if aw <= 0.0 || ah <= 0.0 {
      return;
    }
    let fill = Rect::from_xywh(0.0, 0.0, size.0, size.1);
    match image {
      Image::Linear { dir, stops } => {
        let (dx, dy) = match *dir {
          LinearDir::Angle(a) => ((a * PI / 180.0).sin(), -(a * PI / 180.0).cos()),
          LinearDir::Corner(sx, sy) => {
            let (x, y) = (sx * ah, sy * aw);
            let n = (x * x + y * y).sqrt();
            (x / n, y / n)
          }
        };
        let len = (aw * dx).abs() + (ah * dy).abs();
        let (cx, cy) = (ax + aw / 2.0, ay + ah / 2.0);
        let start = Point::from_xy(cx - dx * len / 2.0, cy - dy * len / 2.0);
        let end = Point::from_xy(cx + dx * len / 2.0, cy + dy * len / 2.0);
        if let (Some(shader), Some(fill)) = (LinearGradient::new(start, end, gradient_stops(stops, len, s.color), SpreadMode::Pad, Transform::identity()), fill) {
          canvas.fill_rect(fill, &shaded(shader), t, mask);
        }
      }
      Image::Radial { circle, size: rs, at, stops } => {
        let (cx, cy) = (ax + at.0.resolve(aw), ay + at.1.resolve(ah));
        let (l, r, tp, b) = (cx - ax, ax + aw - cx, cy - ay, ay + ah - cy);
        let corners = [(l, tp), (r, tp), (l, b), (r, b)].map(|(x, y): (f32, f32)| (x * x + y * y).sqrt());
        let (rx, ry) = match (*circle, *rs) {
          (true, RadialSize::ClosestSide) => (l.min(r).min(tp).min(b), 0.0),
          (true, RadialSize::FarthestSide) => (l.max(r).max(tp).max(b), 0.0),
          (true, RadialSize::ClosestCorner) => (corners.iter().copied().fold(f32::INFINITY, f32::min), 0.0),
          (true, RadialSize::FarthestCorner) => (corners.iter().copied().fold(0.0, f32::max), 0.0),
          (true, RadialSize::Explicit(x, _)) => (x.resolve(aw), 0.0),
          (false, RadialSize::ClosestSide) => (l.min(r), tp.min(b)),
          (false, RadialSize::FarthestSide) => (l.max(r), tp.max(b)),
          (false, RadialSize::ClosestCorner) => (l.min(r) * 2f32.sqrt(), tp.min(b) * 2f32.sqrt()),
          (false, RadialSize::FarthestCorner) => (l.max(r) * 2f32.sqrt(), tp.max(b) * 2f32.sqrt()),
          (false, RadialSize::Explicit(x, y)) => (x.resolve(aw), y.resolve(ah)),
        };
        let ry = if *circle { rx } else { ry };
        if rx <= 0.0 || ry <= 0.0 {
          return;
        }
        let shader = RadialGradient::new(Point::zero(), 0.0, Point::zero(), 1.0, gradient_stops(stops, rx, s.color), SpreadMode::Pad, Transform::from_row(rx, 0.0, 0.0, ry, cx, cy));
        if let (Some(shader), Some(fill)) = (shader, fill) {
          canvas.fill_rect(fill, &shaded(shader), t, mask);
        }
      }
      Image::Url(key) => {
        let Some(pixmap) = self.images.get(key) else { return };
        let (iw, ih) = (pixmap.width() as f32, pixmap.height() as f32);
        let size_value = s.background_size.get(i % s.background_size.len().max(1)).cloned().unwrap_or(BgSize::Auto);
        let (tw, th) = match size_value {
          BgSize::Auto => (iw, ih),
          BgSize::Cover => {
            let k = (aw / iw).max(ah / ih);
            (iw * k, ih * k)
          }
          BgSize::Contain => {
            let k = (aw / iw).min(ah / ih);
            (iw * k, ih * k)
          }
          BgSize::Explicit(x, y) => match (x, y) {
            (LenAuto::Auto, LenAuto::Auto) => (iw, ih),
            (LenAuto::Len(x), LenAuto::Auto) => {
              let w = x.resolve(aw);
              (w, w * ih / iw)
            }
            (LenAuto::Auto, LenAuto::Len(y)) => {
              let h = y.resolve(ah);
              (h * iw / ih, h)
            }
            (LenAuto::Len(x), LenAuto::Len(y)) => (x.resolve(aw), y.resolve(ah)),
          },
        };
        if tw <= 0.0 || th <= 0.0 {
          return;
        }
        let pos = s.background_position.get(i % s.background_position.len().max(1)).copied().unwrap_or_default();
        let (x, y) = (ax + pos.0.resolve(aw - tw), ay + pos.1.resolve(ah - th));
        let repeat = s.background_repeat.get(i % s.background_repeat.len().max(1)).copied().unwrap_or(Repeat::Repeat);
        let spread = if repeat == Repeat::NoRepeat { SpreadMode::Pad } else { SpreadMode::Repeat };
        let shader = Pattern::new((*pixmap).as_ref(), spread, FilterQuality::Bicubic, 1.0, Transform::from_row(tw / iw, 0.0, 0.0, th / ih, x, y));
        let rect = match repeat {
          Repeat::Repeat => Rect::from_xywh(0.0, 0.0, size.0, size.1),
          Repeat::NoRepeat => Rect::from_xywh(x, y, tw, th),
          Repeat::RepeatX => Rect::from_xywh(0.0, y, size.0, th),
          Repeat::RepeatY => Rect::from_xywh(x, 0.0, tw, size.1),
        };
        if let Some(rect) = rect {
          canvas.fill_rect(rect, &shaded(shader), t, mask);
        }
      }
    }
  }

  fn image(&self, canvas: &mut Pixmap, s: &Style, pixmap: &Pixmap, content: (f32, f32, f32, f32), t: Transform, clip: Option<&Mask>) {
    let (cx, cy, cw, ch) = content;
    let (iw, ih) = (pixmap.width() as f32, pixmap.height() as f32);
    if cw <= 0.0 || ch <= 0.0 || iw <= 0.0 || ih <= 0.0 {
      return;
    }
    let contain = (cw / iw).min(ch / ih);
    let (dw, dh) = match s.object_fit {
      Fit::Fill => (cw, ch),
      Fit::Contain => (iw * contain, ih * contain),
      Fit::Cover => {
        let k = (cw / iw).max(ch / ih);
        (iw * k, ih * k)
      }
      Fit::None => (iw, ih),
      Fit::ScaleDown => {
        let k = contain.min(1.0);
        (iw * k, ih * k)
      }
    };
    let dx = cx + s.object_position.0.resolve(cw - dw);
    let dy = cy + s.object_position.1.resolve(ch - dh);
    let shader = Pattern::new(pixmap.as_ref(), SpreadMode::Pad, FilterQuality::Bicubic, 1.0, Transform::from_row(dw / iw, 0.0, 0.0, dh / ih, dx, dy));
    let left = dx.max(cx);
    let top = dy.max(cy);
    let right = (dx + dw).min(cx + cw);
    let bottom = (dy + dh).min(cy + ch);
    if let Some(rect) = Rect::from_ltrb(left, top, right, bottom) {
      canvas.fill_rect(rect, &shaded(shader), t, clip);
    }
  }

  fn outline(&mut self, face: usize, id: u16) -> Option<Path> {
    let faces = self.faces;
    self
      .outlines
      .entry((face, id))
      .or_insert_with(|| {
        let mut builder = Outline(PathBuilder::new());
        faces.list.get(face)?.face.outline_glyph(ttf_parser::GlyphId(id), &mut builder)?;
        builder.0.finish()
      })
      .clone()
  }

  /// Draws a glyph whose origin lands at canvas `(x, y)` from its cached
  /// raster, made on first use at the origin's quarter-pixel offset.
  #[allow(clippy::too_many_arguments)]
  fn blit_glyph(&mut self, canvas: &mut Pixmap, g: &Glyph, size: f32, color: Color, x: f32, y: f32, clip: Option<&Mask>) {
    let (ix, iy) = (x.floor(), y.floor());
    let fx = ((x - ix) * 4.0).round() as u8;
    let fy = ((y - iy) * 4.0).round() as u8;
    let c = color.skia().to_color_u8();
    let key = RasterKey { face: self.faces.uid(g.face), id: g.id, size: size.to_bits(), color: [c.red(), c.green(), c.blue(), c.alpha()], fx, fy };
    let outline = self.outline(g.face, g.id);
    let upem = self.faces.list[g.face].face.units_per_em() as f32;
    let raster = self.faces.raster(key, || {
      let path = outline?;
      let scale = size / upem;
      let place = Transform::from_translate(fx as f32 / 4.0, fy as f32 / 4.0).pre_scale(scale, -scale);
      let bounds = path.clone().transform(place)?.bounds();
      let (left, top) = (bounds.left().floor() as i32 - 1, bounds.top().floor() as i32 - 1);
      let w = (bounds.right().ceil() as i32 + 1 - left).max(1) as u32;
      let h = (bounds.bottom().ceil() as i32 + 1 - top).max(1) as u32;
      let mut pixmap = Pixmap::new(w, h)?;
      pixmap.fill_path(&path, &solid(color), FillRule::Winding, Transform::from_translate(-left as f32, -top as f32).pre_concat(place), None);
      Some(Raster { left, top, pixmap })
    });
    if let Some(r) = raster {
      over(canvas, &r.pixmap, ix as i32 + r.left, iy as i32 + r.top, clip);
    }
  }

  /// Each glyph of a line with its x from the line's start, in visual order:
  /// a right-to-left stretch is reversed.
  fn placed(line: &Line, extra_per_space: f32) -> Vec<(Glyph, f32)> {
    let mut order: Vec<Glyph> = Vec::with_capacity(line.glyphs.len());
    let mut i = 0;
    while i < line.glyphs.len() {
      if line.glyphs[i].rtl {
        let start = i;
        while i < line.glyphs.len() && line.glyphs[i].rtl {
          i += 1;
        }
        order.extend(line.glyphs[start..i].iter().rev());
      } else {
        order.push(line.glyphs[i]);
        i += 1;
      }
    }
    let mut pen = 0.0;
    order
      .into_iter()
      .map(|g| {
        let at = pen;
        pen += g.advance + if g.space { extra_per_space } else { 0.0 };
        (g, at)
      })
      .collect()
  }

  #[allow(clippy::too_many_arguments)]
  fn text(&mut self, canvas: &mut Pixmap, index: usize, lines: &[Line], cx: f32, cy: f32, cw: f32, t: Transform, clip: Option<&Mask>) {
    let Kind::Text(paragraph) = &self.tree.nodes[index].kind else { return };
    let paragraph = paragraph.as_ref();
    let rtl = lines.first().and_then(|l| l.glyphs.first()).is_some_and(|g| g.rtl);
    let last = lines.len().saturating_sub(1);
    let mut placed_lines = Vec::with_capacity(lines.len());
    for (n, line) in lines.iter().enumerate() {
      let free = (cw - line.width).max(0.0);
      let (x0, extra) = match paragraph.align {
        TextAlign::Left => (0.0, 0.0),
        TextAlign::Right => (free, 0.0),
        TextAlign::Center => (free / 2.0, 0.0),
        TextAlign::Start => (if rtl { free } else { 0.0 }, 0.0),
        TextAlign::End => (if rtl { 0.0 } else { free }, 0.0),
        TextAlign::Justify if n < last && line.spaces > 0 => (0.0, free / line.spaces as f32),
        TextAlign::Justify => (0.0, 0.0),
      };
      placed_lines.push((line, x0, Self::placed(line, extra)));
    }

    let mut shadow_runs: Vec<usize> = (0..paragraph.runs.len()).filter(|&r| !paragraph.runs[r].shadows.is_empty()).collect();
    shadow_runs.dedup();
    for run in shadow_runs {
      for (shadow, color) in paragraph.runs[run].shadows.clone().into_iter().rev() {
        let mut layer = self.layer();
        let mut bounds: Option<Rect> = None;
        for (line, x0, glyphs) in &placed_lines {
          for (g, x) in glyphs.iter().filter(|(g, _)| g.run == run && !g.hidden) {
            let Some(path) = self.outline(g.face, g.id) else { continue };
            let scale = paragraph.runs[g.run].size / self.faces.list[g.face].face.units_per_em() as f32;
            let gt = t.pre_translate(cx + x0 + x + g.x_offset + shadow.x, cy + line.baseline - g.y_offset + shadow.y).pre_scale(scale, -scale);
            layer.fill_path(&path, &solid(color), FillRule::Winding, gt, None);
            if let Some(b) = path.clone().transform(gt).map(|p| p.bounds()) {
              bounds = Some(match bounds {
                Some(a) => Rect::from_ltrb(a.left().min(b.left()), a.top().min(b.top()), a.right().max(b.right()), a.bottom().max(b.bottom())).unwrap_or(a),
                None => b,
              });
            }
          }
        }
        self.blurred(canvas, &mut layer, shadow.blur / 2.0, bounds, clip);
      }
    }

    for (line, x0, glyphs) in &placed_lines {
      let mut decorations: Vec<(usize, usize, f32, f32)> = Vec::new();
      for (g, x) in glyphs {
        let style = &paragraph.runs[g.run];
        if !g.hidden {
          let color = Color { a: style.color.a * self.alpha, ..style.color };
          let (ox, oy) = (cx + x0 + x + g.x_offset, cy + line.baseline - g.y_offset);
          if t.sx == 1.0 && t.sy == 1.0 && t.kx == 0.0 && t.ky == 0.0 {
            self.blit_glyph(canvas, g, style.size, color, t.tx + ox, t.ty + oy, clip);
          } else if let Some(path) = self.outline(g.face, g.id) {
            let scale = style.size / self.faces.list[g.face].face.units_per_em() as f32;
            let gt = t.pre_translate(ox, oy).pre_scale(scale, -scale);
            canvas.fill_path(&path, &solid(color), FillRule::Winding, gt, clip);
          }
        }
        if style.underline || style.line_through {
          match decorations.last_mut() {
            Some(d) if d.0 == g.run && d.1 == g.face && (d.3 - (x0 + x)).abs() < 0.5 => d.3 = x0 + x + g.advance,
            _ => decorations.push((g.run, g.face, x0 + x, x0 + x + g.advance)),
          }
        }
      }
      for (run, face, start, end) in decorations {
        let style = &paragraph.runs[run];
        let open = &self.faces.list[face].face;
        let scale = style.size / open.units_per_em() as f32;
        let mut lines_to_draw = Vec::new();
        if style.underline {
          let m = open.underline_metrics();
          let pos = m.map(|m| m.position as f32).unwrap_or(-(open.units_per_em() as f32) * 0.1) * scale;
          let thick = m.map(|m| m.thickness as f32).unwrap_or(open.units_per_em() as f32 / 14.0) * scale;
          lines_to_draw.push((line.baseline - pos - thick / 2.0, thick.max(1.0)));
        }
        if style.line_through {
          let m = open.strikeout_metrics();
          let pos = m.map(|m| m.position as f32).unwrap_or(open.x_height().unwrap_or(500) as f32 / 2.0) * scale;
          let thick = m.map(|m| m.thickness as f32).unwrap_or(open.units_per_em() as f32 / 14.0) * scale;
          lines_to_draw.push((line.baseline - pos - thick / 2.0, thick.max(1.0)));
        }
        for (y, thick) in lines_to_draw {
          if let Some(rect) = Rect::from_xywh(cx + start, cy + y, end - start, thick) {
            canvas.fill_rect(rect, &solid(style.decoration_color), t, clip);
          }
        }
      }
    }
  }
}

fn gradient_stops(stops: &[Stop], length: f32, current: Color) -> Vec<GradientStop> {
  let n = stops.len();
  let mut at: Vec<Option<f32>> = stops.iter().map(|s| s.at.map(|l| if length > 0.0 { l.px / length + l.pct / 100.0 } else { l.pct / 100.0 })).collect();
  if at[0].is_none() {
    at[0] = Some(0.0);
  }
  if at[n - 1].is_none() {
    at[n - 1] = Some(1.0);
  }
  let mut max = at[0].unwrap();
  for v in at.iter_mut().flatten() {
    if *v < max {
      *v = max;
    }
    max = *v;
  }
  let mut i = 0;
  while i < n {
    if at[i].is_none() {
      let start = i - 1;
      let mut end = i;
      while at[end].is_none() {
        end += 1;
      }
      let (a, b) = (at[start].unwrap(), at[end].unwrap());
      for k in start + 1..end {
        at[k] = Some(a + (b - a) * (k - start) as f32 / (end - start) as f32);
      }
      i = end;
    }
    i += 1;
  }
  stops.iter().zip(at).map(|(s, p)| GradientStop::new(p.unwrap_or(0.0).clamp(0.0, 1.0), s.color.resolve(current).skia())).collect()
}

struct Outline(PathBuilder);

impl ttf_parser::OutlineBuilder for Outline {
  fn move_to(&mut self, x: f32, y: f32) {
    self.0.move_to(x, y);
  }
  fn line_to(&mut self, x: f32, y: f32) {
    self.0.line_to(x, y);
  }
  fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
    self.0.quad_to(x1, y1, x, y);
  }
  fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
    self.0.cubic_to(x1, y1, x2, y2, x, y);
  }
  fn close(&mut self) {
    self.0.close();
  }
}
