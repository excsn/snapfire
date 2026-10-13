use unicode_linebreak::BreakOpportunity;

use crate::css::{Color, Decoration, LineHeight, Shadow, Style, TextAlign, Transform, WhiteSpace, WordBreak};
use crate::fonts::Faces;

/// What one run draws with, resolved from its computed style.
#[derive(Debug, Clone)]
pub(crate) struct RunStyle {
  pub families: Vec<String>,
  pub size: f32,
  pub weight: u16,
  pub italic: bool,
  pub color: Color,
  pub letter_spacing: f32,
  pub transform: Transform,
  pub line_height: LineHeight,
  pub underline: bool,
  pub line_through: bool,
  pub decoration_color: Color,
  pub shadows: Vec<(Shadow, Color)>,
}

impl RunStyle {
  pub fn of(style: &Style) -> RunStyle {
    let Decoration { underline, line_through, color } = style.text_decoration;
    RunStyle {
      families: style.font_family.clone(),
      size: style.font_size,
      weight: style.font_weight,
      italic: style.italic,
      color: style.color,
      letter_spacing: style.letter_spacing,
      transform: style.text_transform,
      line_height: style.line_height,
      underline,
      line_through,
      decoration_color: color.map(|c| c.resolve(style.color)).unwrap_or(style.color),
      shadows: style.text_shadow.iter().map(|s| (*s, s.color.resolve(style.color))).collect(),
    }
  }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Glyph {
  pub id: u16,
  pub face: usize,
  pub run: usize,
  pub cluster: usize,
  pub advance: f32,
  pub x_offset: f32,
  pub y_offset: f32,
  pub space: bool,
  pub hidden: bool,
  /// Whether the glyph's segment runs right to left. Glyphs are kept in
  /// logical order and a right-to-left stretch is reversed when drawn.
  pub rtl: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct Line {
  pub glyphs: Vec<Glyph>,
  pub width: f32,
  pub baseline: f32,
  /// Spaces in the line a justified line widens.
  pub spaces: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct Lines {
  pub lines: Vec<Line>,
  pub width: f32,
  pub height: f32,
}

/// One text element's paragraph, shaped once and broken into lines for
/// whatever width layout offers.
#[derive(Debug, Clone)]
pub(crate) struct Paragraph {
  pub text: String,
  pub runs: Vec<RunStyle>,
  pub glyphs: Vec<Glyph>,
  breaks: Vec<(usize, bool)>,
  wraps: bool,
  word_break: WordBreak,
  pub align: TextAlign,
  clamp: Option<u32>,
  ellipsis: bool,
  strut: RunStyle,
  strut_face: Option<usize>,
  /// Characters no face covers, in the order met.
  pub missing: Vec<char>,
}

fn transform_text(text: &str, how: Transform, word_start: &mut bool) -> String {
  match how {
    Transform::None => text.to_owned(),
    Transform::Upper => text.to_uppercase(),
    Transform::Lower => text.to_lowercase(),
    Transform::Capitalize => {
      let mut out = String::with_capacity(text.len());
      for c in text.chars() {
        if *word_start && c.is_alphanumeric() {
          out.extend(c.to_uppercase());
        } else {
          out.push(c);
        }
        *word_start = c.is_whitespace() || (*word_start && !c.is_alphanumeric());
      }
      out
    }
  }
}

impl Paragraph {
  pub fn build(texts: &[(&str, RunStyle)], block: &Style, faces: &Faces<'_>) -> Paragraph {
    let collapse = matches!(block.white_space, WhiteSpace::Normal | WhiteSpace::NoWrap);
    let mut text = String::new();
    let mut spans: Vec<(usize, usize)> = Vec::new();
    let mut word_start = true;
    for (raw, style) in texts {
      let start = text.len();
      let transformed = transform_text(raw, style.transform, &mut word_start);
      for c in transformed.chars() {
        if collapse {
          if c.is_whitespace() {
            if !text.is_empty() && !text.ends_with(' ') {
              text.push(' ');
            }
          } else {
            text.push(c);
          }
        } else if c == '\t' {
          text.push_str("        ");
        } else if c == '\r' {
          continue;
        } else {
          text.push(c);
        }
      }
      spans.push((start, text.len()));
    }
    if collapse {
      while text.ends_with(' ') {
        text.pop();
      }
      for span in &mut spans {
        span.0 = span.0.min(text.len());
        span.1 = span.1.min(text.len());
      }
    }
    let runs: Vec<RunStyle> = texts.iter().map(|(_, s)| s.clone()).collect();
    let strut = RunStyle::of(block);
    let strut_face = faces.select(&strut.families, strut.weight, strut.italic, 'x').or_else(|| (!faces.list.is_empty()).then_some(0));

    let mut missing = Vec::new();
    let mut segments: Vec<(usize, usize, usize, usize)> = Vec::new();
    for (run, &(start, end)) in spans.iter().enumerate() {
      let style = &runs[run];
      let mut previous: Option<usize> = None;
      for (offset, c) in text[start..end].char_indices() {
        let at = start + offset;
        let script = unicode_script::UnicodeScript::script(&c);
        let shared = c.is_whitespace() || c.is_control() || matches!(script, unicode_script::Script::Common | unicode_script::Script::Inherited);
        let face = match previous {
          Some(p) if shared && faces.covers(p, c) => Some(p),
          _ => faces.select(&style.families, style.weight, style.italic, c),
        };
        let face = match face {
          Some(f) => f,
          None => {
            if !c.is_whitespace() && !c.is_control() && !missing.contains(&c) {
              missing.push(c);
            }
            previous.or(strut_face).unwrap_or(0)
          }
        };
        previous = Some(face);
        match segments.last_mut() {
          Some(last) if last.2 == run && last.3 == face && last.1 == at => last.1 = at + c.len_utf8(),
          _ => segments.push((at, at + c.len_utf8(), run, face)),
        }
      }
    }

    let mut glyphs = Vec::new();
    for &(start, end, run, face) in &segments {
      let Some(open) = faces.list.get(face) else { continue };
      let style = &runs[run];
      let mut buffer = rustybuzz::UnicodeBuffer::new();
      buffer.push_str(&text[start..end]);
      buffer.guess_segment_properties();
      let rtl = buffer.direction() == rustybuzz::Direction::RightToLeft;
      let shaped = rustybuzz::shape(&open.face, &[], buffer);
      let scale = style.size / open.face.units_per_em() as f32;
      let mut part: Vec<Glyph> = shaped
        .glyph_infos()
        .iter()
        .zip(shaped.glyph_positions())
        .map(|(info, pos)| {
          let cluster = start + info.cluster as usize;
          let c = text[cluster..].chars().next().unwrap_or(' ');
          Glyph {
            id: info.glyph_id as u16,
            face,
            run,
            cluster,
            advance: if c.is_control() { 0.0 } else { pos.x_advance as f32 * scale },
            x_offset: pos.x_offset as f32 * scale,
            y_offset: pos.y_offset as f32 * scale,
            space: c == ' ' || c == '\u{a0}' || c == '\t',
            hidden: c.is_control() || info.glyph_id == 0,
            rtl,
          }
        })
        .collect();
      if rtl {
        part.reverse();
      }
      glyphs.extend(part);
    }
    glyphs.sort_by_key(|g| g.cluster);
    for i in 0..glyphs.len() {
      let last_of_cluster = glyphs.get(i + 1).is_none_or(|n| n.cluster != glyphs[i].cluster);
      if last_of_cluster && !glyphs[i].hidden {
        glyphs[i].advance += runs[glyphs[i].run].letter_spacing;
      }
    }

    let wraps = matches!(block.white_space, WhiteSpace::Normal | WhiteSpace::PreWrap);
    let mut breaks: Vec<(usize, bool)> = unicode_linebreak::linebreaks(&text)
      .filter_map(|(at, kind)| {
        let mandatory = kind == BreakOpportunity::Mandatory;
        (mandatory || wraps).then_some((at, mandatory && (at < text.len() || text.ends_with('\n'))))
      })
      .collect();
    if wraps && block.word_break == WordBreak::BreakAll {
      breaks.extend(text.char_indices().skip(1).map(|(i, _)| (i, false)));
      breaks.sort_by_key(|b| (b.0, !b.1));
      breaks.dedup_by_key(|b| b.0);
    }
    if breaks.last().is_none_or(|b| b.0 != text.len()) {
      breaks.push((text.len(), false));
    }

    Paragraph { text, runs, glyphs, breaks, wraps, word_break: block.word_break, align: block.text_align, clamp: block.line_clamp, ellipsis: block.ellipsis, strut, strut_face, missing }
  }

  /// The widest piece no break opportunity splits, which is the narrowest
  /// the paragraph can be laid out.
  pub fn min_content(&self) -> f32 {
    let mut widest = 0f32;
    let mut g = 0;
    for &(at, _) in &self.breaks {
      let mut w = 0.0;
      let mut trailing = 0.0;
      while g < self.glyphs.len() && self.glyphs[g].cluster < at {
        let glyph = &self.glyphs[g];
        w += glyph.advance;
        trailing = if glyph.space { trailing + glyph.advance } else { 0.0 };
        g += 1;
      }
      widest = widest.max(w - trailing);
    }
    if self.word_break == WordBreak::BreakWord {
      return self.glyphs.iter().map(|g| g.advance).fold(0.0, f32::max).min(widest);
    }
    widest
  }

  pub fn lines(&self, max: Option<f32>, faces: &Faces<'_>) -> Lines {
    let limit = max.filter(|m| m.is_finite()).unwrap_or(f32::INFINITY);
    let mut spans: Vec<(usize, usize)> = Vec::new();
    let mut line_start = 0;
    let mut line_w = 0f32;
    let mut g = 0;
    let n = self.glyphs.len();
    for &(at, mandatory) in &self.breaks {
      let piece_start = g;
      let mut w = 0.0;
      let mut trailing = 0.0;
      while g < n && self.glyphs[g].cluster < at {
        let glyph = &self.glyphs[g];
        w += glyph.advance;
        trailing = if glyph.space { trailing + glyph.advance } else { 0.0 };
        g += 1;
      }
      if self.wraps && piece_start > line_start && line_w + w - trailing > limit {
        spans.push((line_start, piece_start));
        line_start = piece_start;
        line_w = 0.0;
      }
      if self.wraps && self.word_break == WordBreak::BreakWord && piece_start == line_start && w - trailing > limit {
        let mut run_w = 0.0;
        let mut i = piece_start;
        while i < g {
          let cluster = self.glyphs[i].cluster;
          let mut cw = 0.0;
          let mut j = i;
          while j < g && self.glyphs[j].cluster == cluster {
            cw += self.glyphs[j].advance;
            j += 1;
          }
          if run_w + cw > limit && i > line_start {
            spans.push((line_start, i));
            line_start = i;
            run_w = 0.0;
          }
          run_w += cw;
          i = j;
        }
        line_w = run_w;
      } else {
        line_w += w;
      }
      if mandatory {
        spans.push((line_start, g));
        line_start = g;
        line_w = 0.0;
      }
    }
    if line_start < n || (spans.is_empty() && !self.text.is_empty()) {
      spans.push((line_start, n));
    }

    let clamp = self.clamp.map(|c| c as usize);
    let clamped = clamp.is_some_and(|c| spans.len() > c);
    if let Some(c) = clamp {
      spans.truncate(c);
    }
    let last = spans.len().saturating_sub(1);
    let mut lines = Vec::with_capacity(spans.len());
    let mut top = 0f32;
    for (index, &(s, e)) in spans.iter().enumerate() {
      let mut glyphs: Vec<Glyph> = self.glyphs[s..e].to_vec();
      while glyphs.last().is_some_and(|g| g.space || g.hidden) {
        glyphs.pop();
      }
      let mut width: f32 = glyphs.iter().map(|g| g.advance).sum();
      let overflows = width > limit + 0.01;
      if (clamped && index == last) || (self.ellipsis && overflows) {
        width = self.ellipsize(&mut glyphs, limit, faces);
      }
      let (above, below) = self.metrics(&glyphs, faces);
      let spaces = glyphs.iter().filter(|g| g.space).count();
      lines.push(Line { glyphs, width, baseline: top + above, spaces });
      top += above + below;
    }
    let width = lines.iter().map(|l| l.width).fold(0.0, f32::max);
    Lines { lines, width, height: top }
  }

  /// Cuts the line until the ellipsis fits after it, then appends it: `…`,
  /// else `...` when the last glyph's face has no `…`. Returns the new width.
  fn ellipsize(&self, glyphs: &mut Vec<Glyph>, limit: f32, faces: &Faces<'_>) -> f32 {
    let (run, face) = glyphs.last().map(|g| (g.run, g.face)).or_else(|| self.glyphs.first().map(|g| (g.run, g.face))).or(self.strut_face.map(|f| (0, f))).unwrap_or((0, 0));
    let Some(open) = faces.list.get(face) else {
      return glyphs.iter().map(|g| g.advance).sum();
    };
    let style = self.runs.get(run).unwrap_or(&self.strut);
    let mark = if faces.covers(face, '…') { "…" } else { "..." };
    let mut buffer = rustybuzz::UnicodeBuffer::new();
    buffer.push_str(mark);
    let shaped = rustybuzz::shape(&open.face, &[], buffer);
    let scale = style.size / open.face.units_per_em() as f32;
    let cluster = glyphs.last().map(|g| g.cluster + 1).unwrap_or(0);
    let tail: Vec<Glyph> = shaped.glyph_infos().iter().zip(shaped.glyph_positions()).map(|(i, p)| Glyph { id: i.glyph_id as u16, face, run, cluster, advance: p.x_advance as f32 * scale, x_offset: p.x_offset as f32 * scale, y_offset: p.y_offset as f32 * scale, space: false, hidden: false, rtl: false }).collect();
    let tail_w: f32 = tail.iter().map(|g| g.advance).sum();
    let mut width: f32 = glyphs.iter().map(|g| g.advance).sum();
    while !glyphs.is_empty() && width + tail_w > limit {
      let cluster = glyphs.last().unwrap().cluster;
      while glyphs.last().is_some_and(|g| g.cluster == cluster) {
        width -= glyphs.pop().unwrap().advance;
      }
      while glyphs.last().is_some_and(|g| g.space) {
        width -= glyphs.pop().unwrap().advance;
      }
    }
    glyphs.extend(tail);
    width + tail_w
  }

  /// The space a line takes above and below its baseline: each run's ascent
  /// and descent plus half the leading its line height adds.
  fn metrics(&self, glyphs: &[Glyph], faces: &Faces<'_>) -> (f32, f32) {
    let mut above = 0f32;
    let mut below = 0f32;
    let mut seen: Vec<(usize, usize)> = Vec::new();
    let mut measure = |face: usize, style: &RunStyle| {
      let Some(open) = faces.list.get(face) else { return };
      let upem = open.face.units_per_em() as f32;
      let scale = style.size / upem;
      let ascent = open.face.ascender() as f32 * scale;
      let descent = -(open.face.descender() as f32) * scale;
      let gap = open.face.line_gap() as f32 * scale;
      let height = match style.line_height {
        LineHeight::Normal => ascent + descent + gap,
        LineHeight::Factor(f) => f * style.size,
        LineHeight::Px(px) => px,
      };
      let half = (height - ascent - descent) / 2.0;
      above = above.max(ascent + half);
      below = below.max(descent + half);
    };
    for g in glyphs {
      if !seen.contains(&(g.face, g.run)) {
        seen.push((g.face, g.run));
        measure(g.face, &self.runs[g.run]);
      }
    }
    if seen.is_empty() {
      if let Some(face) = self.strut_face {
        measure(face, &self.strut);
      }
    }
    (above, below)
  }
}
