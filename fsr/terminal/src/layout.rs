use taffy::prelude::*;
use unicode_width::UnicodeWidthChar;

use crate::frame::Frame;
use crate::style::Style as CellStyle;
use crate::{Align, Border, Direction, Justify, Node, Wrap};

type Cells = Vec<(char, CellStyle)>;

/// A text leaf: its characters and how it breaks.
struct Leaf {
  cells: Cells,
  wrap: Wrap,
}

enum Kind {
  Box { border: Option<Border>, color: CellStyle, clip: (bool, bool), offset: (i32, i32), id: Option<String> },
  Text(Leaf),
}

/// A box's layout as Ink's `useBoxMetrics` reports it, in cells: `left` and `top` from its parent's corner, the client size without the border.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Metrics {
  pub width: u16,
  pub height: u16,
  pub left: i32,
  pub top: i32,
  pub client_width: u16,
  pub client_height: u16,
}

/// The cells a box with a hidden overflow lets its children draw in: columns `x1..x2` and rows `y1..y2`.
#[derive(Clone, Copy)]
struct Clip {
  x1: i64,
  x2: i64,
  y1: i64,
  y2: i64,
}

impl Clip {
  const ALL: Clip = Clip { x1: i64::MIN, x2: i64::MAX, y1: i64::MIN, y2: i64::MAX };

  fn put(&self, frame: &mut Frame, x: i64, y: i64, ch: char, style: CellStyle) {
    if x >= self.x1 && x < self.x2 && y >= self.y1 && y < self.y2 && x >= 0 && y >= 0 {
      frame.put(x as usize, y as usize, ch, style);
    }
  }
}

/// Lays `node` out across `columns` cells inside a root column that
/// stretches its children to that width as Ink's root does. Then it draws it.
/// The frame is as tall as what was drawn.
pub fn render(node: &Node, columns: u16) -> Frame {
  render_measured(node, columns).0
}

/// [`render`], with the metrics of each box that has an id.
pub fn render_measured(node: &Node, columns: u16) -> (Frame, Vec<(String, Metrics)>) {
  let mut tree: TaffyTree<usize> = TaffyTree::new();
  let mut kinds: Vec<(NodeId, Kind)> = Vec::new();
  let child = add(&mut tree, &mut kinds, node);
  let root_style = Style { flex_direction: FlexDirection::Column, size: Size { width: Dimension::length(columns as f32), height: Dimension::auto() }, ..Default::default() };
  let Ok(root) = tree.new_with_children(root_style, &[child]) else { return (Frame::new(columns as usize, 0), Vec::new()) };
  let available = Size { width: AvailableSpace::Definite(columns as f32), height: AvailableSpace::MaxContent };
  let leaves: Vec<Option<&Leaf>> = kinds.iter().map(|(_, kind)| match kind {
    Kind::Text(leaf) => Some(leaf),
    Kind::Box { .. } => None,
  }).collect();
  let measured = tree.compute_layout_with_measure(root, available, |inputs, _, context, style| {
    let leaf = context.and_then(|index| leaves.get(*index).copied().flatten());
    taffy::compute_leaf_layout(inputs, style, |_, _| 0.0, |known, avail| match leaf {
      Some(leaf) => {
        let max = known.width.map(|w| w.floor() as usize).or(match avail.width {
          AvailableSpace::Definite(w) => Some(w.floor() as usize),
          AvailableSpace::MinContent => Some(leaf.min_content()),
          AvailableSpace::MaxContent => None,
        });
        let lines = leaf.lines(max);
        let widest = lines.iter().map(|line| width(line)).max().unwrap_or(0);
        Size { width: known.width.unwrap_or(widest as f32), height: known.height.unwrap_or(lines.len() as f32) }
      }
      None => Size::ZERO,
    })
  });
  if measured.is_err() {
    return (Frame::new(columns as usize, 0), Vec::new());
  }
  let height = tree.layout(root).map_or(0.0, |layout| layout.size.height).round().max(0.0) as usize;
  let mut frame = Frame::new(columns as usize, height);
  let mut metrics = Vec::new();
  draw(&tree, root, (0.0, 0.0), Clip::ALL, &kinds, &mut frame, &mut metrics);
  (frame, metrics)
}

fn add(tree: &mut TaffyTree<usize>, kinds: &mut Vec<(NodeId, Kind)>, node: &Node) -> NodeId {
  match node {
    Node::Text(text) => {
      let leaf = Leaf { cells: text.styled(&CellStyle::default()), wrap: text.wrap };
      let index = kinds.len();
      let style = Style { min_size: Size { width: LengthPercentageAuto::length(0.0), height: LengthPercentageAuto::length(0.0) }, ..Default::default() };
      let id = tree.new_leaf_with_context(style, index).expect("a text leaf");
      kinds.push((id, Kind::Text(leaf)));
      id
    }
    Node::Box(boxed) => {
      let children: Vec<NodeId> = boxed.children.iter().map(|child| add(tree, kinds, child)).collect();
      let s = &boxed.style;
      let edge = if s.border.is_some() { 1.0 } else { 0.0 };
      let cells = |v: [f32; 4]| Rect { top: length(v[0]), right: length(v[1]), bottom: length(v[2]), left: length(v[3]) };
      let style = Style {
        flex_direction: match s.direction {
          Direction::Row => FlexDirection::Row,
          Direction::Column => FlexDirection::Column,
          Direction::RowReverse => FlexDirection::RowReverse,
          Direction::ColumnReverse => FlexDirection::ColumnReverse,
        },
        gap: Size { width: length(s.gap as f32), height: length(s.gap as f32) },
        padding: cells(s.padding),
        margin: Rect { top: LengthPercentageAuto::length(s.margin[0]), right: LengthPercentageAuto::length(s.margin[1]), bottom: LengthPercentageAuto::length(s.margin[2]), left: LengthPercentageAuto::length(s.margin[3]) },
        border: Rect { top: length(edge), right: length(edge), bottom: length(edge), left: length(edge) },
        size: Size { width: dimension(s.width), height: dimension(s.height) },
        flex_grow: s.grow,
        flex_shrink: s.shrink.unwrap_or(1.0),
        flex_basis: dimension(s.basis),
        // Yoga has no automatic minimum size, so a flex item shrinks below its content.
        min_size: Size { width: LengthPercentageAuto::length(0.0), height: LengthPercentageAuto::length(0.0) },
        justify_content: match s.justify {
          None => JustifyContent::NORMAL,
          Some(Justify::Start) => JustifyContent::FLEX_START,
          Some(Justify::Center) => JustifyContent::CENTER,
          Some(Justify::End) => JustifyContent::FLEX_END,
          Some(Justify::SpaceBetween) => JustifyContent::SPACE_BETWEEN,
          Some(Justify::SpaceAround) => JustifyContent::SPACE_AROUND,
          Some(Justify::SpaceEvenly) => JustifyContent::SPACE_EVENLY,
        },
        align_items: match s.align {
          None | Some(Align::Stretch) => AlignItems::STRETCH,
          Some(Align::Start) => AlignItems::FLEX_START,
          Some(Align::Center) => AlignItems::CENTER,
          Some(Align::End) => AlignItems::FLEX_END,
        },
        ..Default::default()
      };
      let id = tree.new_with_children(style, &children).expect("a box");
      let color = CellStyle { color: s.border_color, ..CellStyle::default() };
      kinds.push((id, Kind::Box { border: s.border, color, clip: (s.overflow_x, s.overflow_y), offset: s.content_offset, id: boxed.id.clone() }));
      id
    }
  }
}

fn draw(tree: &TaffyTree<usize>, id: NodeId, at: (f32, f32), clip: Clip, kinds: &[(NodeId, Kind)], frame: &mut Frame, metrics: &mut Vec<(String, Metrics)>) {
  let Ok(layout) = tree.layout(id) else { return };
  let (x, y) = (at.0 + layout.location.x, at.1 + layout.location.y);
  let (left, top) = (x.round() as i64, y.round() as i64);
  let (w, h) = (layout.size.width.round().max(0.0) as i64, layout.size.height.round().max(0.0) as i64);
  let mut inner = clip;
  let mut shift = (0.0, 0.0);
  match kinds.iter().find(|(node, _)| *node == id).map(|(_, kind)| kind) {
    Some(Kind::Text(leaf)) => {
      for (row, line) in leaf.lines(Some(w as usize)).iter().enumerate().take(h.max(1) as usize) {
        let mut column = left;
        for &(ch, style) in line {
          clip.put(frame, column, top + row as i64, ch, style);
          column += ch.width().unwrap_or(0) as i64;
        }
      }
    }
    Some(Kind::Box { border, color, clip: (cut_x, cut_y), offset, id: named }) => {
      let edge = i64::from(border.is_some());
      if let Some(name) = named {
        let cells = |n: i64| n.clamp(0, i64::from(u16::MAX)) as u16;
        let place = |n: f32| n.round() as i32;
        metrics.push((name.clone(), Metrics { width: cells(w), height: cells(h), left: place(layout.location.x), top: place(layout.location.y), client_width: cells(w - 2 * edge), client_height: cells(h - 2 * edge) }));
      }
      if let Some(border) = border.filter(|_| w >= 2 && h >= 2) {
        let [tl, tr, bl, br, top_edge, bottom_edge, left_edge, right_edge] = border.glyphs();
        for column in left + 1..left + w - 1 {
          clip.put(frame, column, top, top_edge, *color);
          clip.put(frame, column, top + h - 1, bottom_edge, *color);
        }
        for row in top + 1..top + h - 1 {
          clip.put(frame, left, row, left_edge, *color);
          clip.put(frame, left + w - 1, row, right_edge, *color);
        }
        clip.put(frame, left, top, tl, *color);
        clip.put(frame, left + w - 1, top, tr, *color);
        clip.put(frame, left, top + h - 1, bl, *color);
        clip.put(frame, left + w - 1, top + h - 1, br, *color);
      }
      if *cut_x {
        inner.x1 = inner.x1.max(left + edge);
        inner.x2 = inner.x2.min(left + w - edge);
      }
      if *cut_y {
        inner.y1 = inner.y1.max(top + edge);
        inner.y2 = inner.y2.min(top + h - edge);
      }
      shift = (offset.0 as f32, offset.1 as f32);
    }
    None => {}
  }
  if let Ok(children) = tree.children(id) {
    for child in children {
      draw(tree, child, (x - shift.0, y - shift.1), inner, kinds, frame, metrics);
    }
  }
}

fn dimension(extent: Option<crate::Extent>) -> Dimension {
  match extent {
    None => Dimension::auto(),
    Some(crate::Extent::Cells(cells)) => Dimension::length(cells as f32),
    Some(crate::Extent::Percent(percent)) => Dimension::percent(percent / 100.0),
  }
}

fn width(cells: &[(char, CellStyle)]) -> usize {
  cells.iter().map(|(ch, _)| ch.width().unwrap_or(0)).sum()
}

impl Leaf {
  /// The widest word, what the text cannot be narrower than without breaking one.
  fn min_content(&self) -> usize {
    self.cells.split(|(ch, _)| *ch == ' ' || *ch == '\n').map(width).max().unwrap_or(0)
  }

  /// The text broken into lines no wider than `max`: at spaces, inside a word
  /// only when the word alone is wider and at every newline. Spaces are kept
  /// as written, as Ink's `wrap-ansi` with `trim: false` keeps them, except
  /// the run a line breaks at. Under
  /// [`Wrap::Truncate`] it is one line ending in `…`. An empty text is no
  /// line, and every paragraph of another is at least one, as Ink measures.
  fn lines(&self, max: Option<usize>) -> Vec<Cells> {
    let max = max.unwrap_or(usize::MAX).max(1);
    let mut out = Vec::new();
    if self.cells.is_empty() {
      return out;
    }
    for paragraph in self.cells.split(|(ch, _)| *ch == '\n') {
      let start = out.len();
      if self.wrap != Wrap::Wrap {
        out.push(truncate(paragraph, max, self.wrap));
        continue;
      }
      let mut line: Cells = Vec::new();
      let mut used = 0;
      let mut words: Vec<(Cells, Cells)> = Vec::new();
      let (mut spaces, mut word): (Cells, Cells) = (Vec::new(), Vec::new());
      for &cell in paragraph {
        if cell.0 == ' ' {
          if !word.is_empty() {
            words.push((std::mem::take(&mut spaces), std::mem::take(&mut word)));
          }
          spaces.push(cell);
        } else {
          word.push(cell);
        }
      }
      words.push((spaces, word));
      for (at, (before, word)) in words.into_iter().enumerate() {
        let wide = width(&word);
        if at == 0 || used + before.len() + wide <= max {
          let room = max.saturating_sub(used);
          used += before.len().min(room);
          line.extend(before.into_iter().take(room));
        } else if !line.is_empty() {
          out.push(std::mem::take(&mut line));
          used = 0;
        }
        for cell in word {
          let cw = cell.0.width().unwrap_or(0);
          if used + cw > max && !line.is_empty() {
            out.push(std::mem::take(&mut line));
            used = 0;
          }
          line.push(cell);
          used += cw;
        }
      }
      if !line.is_empty() || out.len() == start {
        out.push(line);
      }
    }
    out
  }
}

/// `cells` cut to `max` columns with `…` where it was cut, as `cli-truncate` cuts: at the end, at the start or in the middle, the first half before the `…`.
fn truncate(cells: &[(char, CellStyle)], max: usize, at: Wrap) -> Cells {
  if width(cells) <= max {
    return cells.to_vec();
  }
  let ellipsis = |next: Option<&(char, CellStyle)>| ('…', next.map_or(CellStyle::default(), |(_, style)| *style));
  match at {
    Wrap::TruncateStart => {
      let tail = last_columns(cells, max - 1);
      let mut out = vec![ellipsis(tail.first())];
      out.extend(tail);
      out
    }
    Wrap::TruncateMiddle => {
      let head = first_columns(cells, max / 2);
      let tail = last_columns(cells, max - max / 2 - 1);
      let mut out = head.clone();
      out.push(ellipsis(tail.first().or(head.last())));
      out.extend(tail);
      out
    }
    Wrap::Wrap | Wrap::Truncate => {
      let mut out = first_columns(cells, max - 1);
      out.push(ellipsis(cells.get(out.len())));
      out
    }
  }
}

/// The first cells of `cells` that fit in `columns`.
fn first_columns(cells: &[(char, CellStyle)], columns: usize) -> Cells {
  let mut used = 0;
  cells.iter().take_while(|(ch, _)| {
    used += ch.width().unwrap_or(0);
    used <= columns
  }).copied().collect()
}

/// The last cells of `cells` that fit in `columns`.
fn last_columns(cells: &[(char, CellStyle)], columns: usize) -> Cells {
  let mut used = 0;
  let mut out: Cells = cells.iter().rev().take_while(|(ch, _)| {
    used += ch.width().unwrap_or(0);
    used <= columns
  }).copied().collect();
  out.reverse();
  out
}
