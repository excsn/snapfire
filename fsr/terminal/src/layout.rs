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
  Box { border: Option<Border>, color: CellStyle },
  Text(Leaf),
}

/// Lays `node` out across `columns` cells inside a root column that
/// stretches its children to that width as Ink's root does. Then it draws it.
/// The frame is as tall as what was drawn.
pub fn render(node: &Node, columns: u16) -> Frame {
  let mut tree: TaffyTree<usize> = TaffyTree::new();
  let mut kinds: Vec<(NodeId, Kind)> = Vec::new();
  let child = add(&mut tree, &mut kinds, node);
  let root_style = Style { flex_direction: FlexDirection::Column, size: Size { width: Dimension::length(columns as f32), height: Dimension::auto() }, ..Default::default() };
  let Ok(root) = tree.new_with_children(root_style, &[child]) else { return Frame::new(columns as usize, 0) };
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
    return Frame::new(columns as usize, 0);
  }
  let height = tree.layout(root).map_or(0.0, |layout| layout.size.height).round().max(0.0) as usize;
  let mut frame = Frame::new(columns as usize, height);
  draw(&tree, root, (0.0, 0.0), &kinds, &mut frame);
  frame
}

fn add(tree: &mut TaffyTree<usize>, kinds: &mut Vec<(NodeId, Kind)>, node: &Node) -> NodeId {
  match node {
    Node::Text(text) => {
      let leaf = Leaf { cells: text.styled(&CellStyle::default()), wrap: text.wrap };
      let index = kinds.len();
      let id = tree.new_leaf_with_context(Style::default(), index).expect("a text leaf");
      kinds.push((id, Kind::Text(leaf)));
      id
    }
    Node::Box(boxed) => {
      let children: Vec<NodeId> = boxed.children.iter().map(|child| add(tree, kinds, child)).collect();
      let s = &boxed.style;
      let edge = if s.border.is_some() { 1.0 } else { 0.0 };
      let cells = |v: [u16; 4]| Rect { top: length(v[0] as f32), right: length(v[1] as f32), bottom: length(v[2] as f32), left: length(v[3] as f32) };
      let style = Style {
        flex_direction: match s.direction {
          Direction::Row => FlexDirection::Row,
          Direction::Column => FlexDirection::Column,
        },
        gap: Size { width: length(s.gap as f32), height: length(s.gap as f32) },
        padding: cells(s.padding),
        margin: Rect { top: LengthPercentageAuto::length(s.margin[0] as f32), right: LengthPercentageAuto::length(s.margin[1] as f32), bottom: LengthPercentageAuto::length(s.margin[2] as f32), left: LengthPercentageAuto::length(s.margin[3] as f32) },
        border: Rect { top: length(edge), right: length(edge), bottom: length(edge), left: length(edge) },
        size: Size { width: s.width.map_or(Dimension::auto(), |w| Dimension::length(w as f32)), height: s.height.map_or(Dimension::auto(), |h| Dimension::length(h as f32)) },
        flex_grow: s.grow,
        flex_shrink: s.shrink.unwrap_or(1.0),
        justify_content: match s.justify {
          None => JustifyContent::NORMAL,
          Some(Justify::Start) => JustifyContent::FLEX_START,
          Some(Justify::Center) => JustifyContent::CENTER,
          Some(Justify::End) => JustifyContent::FLEX_END,
          Some(Justify::SpaceBetween) => JustifyContent::SPACE_BETWEEN,
          Some(Justify::SpaceAround) => JustifyContent::SPACE_AROUND,
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
      kinds.push((id, Kind::Box { border: s.border, color }));
      id
    }
  }
}

fn draw(tree: &TaffyTree<usize>, id: NodeId, at: (f32, f32), kinds: &[(NodeId, Kind)], frame: &mut Frame) {
  let Ok(layout) = tree.layout(id) else { return };
  let (x, y) = (at.0 + layout.location.x, at.1 + layout.location.y);
  let (left, top) = (x.round().max(0.0) as usize, y.round().max(0.0) as usize);
  let (w, h) = (layout.size.width.round().max(0.0) as usize, layout.size.height.round().max(0.0) as usize);
  match kinds.iter().find(|(node, _)| *node == id).map(|(_, kind)| kind) {
    Some(Kind::Text(leaf)) => {
      for (row, line) in leaf.lines(Some(w)).iter().enumerate().take(h.max(1)) {
        let mut column = left;
        for &(ch, style) in line {
          frame.put(column, top + row, ch, style);
          column += ch.width().unwrap_or(0);
        }
      }
    }
    Some(Kind::Box { border: Some(border), color }) if w >= 2 && h >= 2 => {
      let [tl, tr, bl, br, horizontal, vertical] = border.glyphs();
      for column in left + 1..left + w - 1 {
        frame.put(column, top, horizontal, *color);
        frame.put(column, top + h - 1, horizontal, *color);
      }
      for row in top + 1..top + h - 1 {
        frame.put(left, row, vertical, *color);
        frame.put(left + w - 1, row, vertical, *color);
      }
      frame.put(left, top, tl, *color);
      frame.put(left + w - 1, top, tr, *color);
      frame.put(left, top + h - 1, bl, *color);
      frame.put(left + w - 1, top + h - 1, br, *color);
    }
    _ => {}
  }
  if let Ok(children) = tree.children(id) {
    for child in children {
      draw(tree, child, (x, y), kinds, frame);
    }
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
  /// only when the word alone is wider and at every newline. Under
  /// [`Wrap::Truncate`] it is one line ending in `…`.
  fn lines(&self, max: Option<usize>) -> Vec<Cells> {
    let max = max.unwrap_or(usize::MAX).max(1);
    let mut out = Vec::new();
    for paragraph in self.cells.split(|(ch, _)| *ch == '\n') {
      if self.wrap == Wrap::Truncate {
        out.push(truncate(paragraph, max));
        continue;
      }
      let mut line: Cells = Vec::new();
      let mut used = 0;
      let mut space: Option<(char, CellStyle)> = None;
      let mut words: Vec<(Cells, Option<(char, CellStyle)>)> = Vec::new();
      let mut word: Cells = Vec::new();
      for &cell in paragraph {
        if cell.0 == ' ' {
          if !word.is_empty() {
            words.push((std::mem::take(&mut word), space.take()));
          }
          space = Some(cell);
        } else {
          word.push(cell);
        }
      }
      if !word.is_empty() {
        words.push((word, space.take()));
      }
      for (word, before) in words {
        let wide = width(&word);
        if !line.is_empty() && used + 1 + wide <= max {
          line.push(before.unwrap_or((' ', CellStyle::default())));
          used += 1;
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
      if !line.is_empty() || out.is_empty() && !paragraph.is_empty() {
        out.push(line);
      }
    }
    out
  }
}

fn truncate(cells: &[(char, CellStyle)], max: usize) -> Cells {
  if width(cells) <= max {
    return cells.to_vec();
  }
  let mut out = Vec::new();
  let mut used = 0;
  for &(ch, style) in cells {
    let cw = ch.width().unwrap_or(0);
    if used + cw > max - 1 {
      out.push(('…', style));
      break;
    }
    out.push((ch, style));
    used += cw;
  }
  out
}
