use std::sync::Arc;

use taffy::prelude::*;
use tiny_skia::Pixmap;

use crate::card::{Element, ImageRef};
use crate::css::{self, Align, Bad, Image, Len, LenAuto, Style};
use crate::fonts::Faces;
use crate::images::{self, Images};
use crate::text::{Lines, Paragraph, RunStyle};
use crate::{CardError, Warning};

/// The deepest a card's tree may nest.
pub(crate) const MAX_DEPTH: usize = 64;

pub(crate) enum Kind {
  Box,
  Text(Box<Paragraph>),
  Image(Arc<Pixmap>),
}

pub(crate) struct Node {
  pub style: Style,
  pub kind: Kind,
  pub children: Vec<usize>,
  pub id: NodeId,
}

pub(crate) struct Tree {
  pub nodes: Vec<Node>,
  pub taffy: TaffyTree<usize>,
  pub root: usize,
}

struct Builder<'a> {
  faces: &'a Faces<'a>,
  images: &'a Images,
  card: (f32, f32),
  root_font: f32,
  nodes: Vec<Node>,
  taffy: TaffyTree<usize>,
  warnings: &'a mut Vec<Warning>,
}

fn lp(len: Len) -> LengthPercentage {
  if len.pct != 0.0 && len.px == 0.0 { LengthPercentage::percent(len.pct / 100.0) } else { LengthPercentage::length(len.px) }
}

fn lpa(len: LenAuto) -> LengthPercentageAuto {
  match len {
    LenAuto::Auto => LengthPercentageAuto::auto(),
    LenAuto::Len(l) if l.pct != 0.0 && l.px == 0.0 => LengthPercentageAuto::percent(l.pct / 100.0),
    LenAuto::Len(l) => LengthPercentageAuto::length(l.px),
  }
}

fn dim(len: LenAuto) -> Dimension {
  match len {
    LenAuto::Auto => Dimension::auto(),
    LenAuto::Len(l) if l.pct != 0.0 && l.px == 0.0 => Dimension::percent(l.pct / 100.0),
    LenAuto::Len(l) => Dimension::length(l.px),
  }
}

fn items(a: Option<Align>) -> AlignItems {
  match a {
    None | Some(Align::Stretch) => AlignItems::STRETCH,
    Some(Align::Start) => AlignItems::START,
    Some(Align::End) => AlignItems::END,
    Some(Align::FlexStart) => AlignItems::FLEX_START,
    Some(Align::FlexEnd) => AlignItems::FLEX_END,
    Some(Align::Center) => AlignItems::CENTER,
    Some(Align::SpaceBetween | Align::SpaceAround | Align::SpaceEvenly) => AlignItems::STRETCH,
  }
}

fn content(a: Option<Align>) -> AlignContent {
  match a {
    None => AlignContent::NORMAL,
    Some(Align::Start) => AlignContent::START,
    Some(Align::End) => AlignContent::END,
    Some(Align::FlexStart) => AlignContent::FLEX_START,
    Some(Align::FlexEnd) => AlignContent::FLEX_END,
    Some(Align::Center) => AlignContent::CENTER,
    Some(Align::Stretch) => AlignContent::STRETCH,
    Some(Align::SpaceBetween) => AlignContent::SPACE_BETWEEN,
    Some(Align::SpaceAround) => AlignContent::SPACE_AROUND,
    Some(Align::SpaceEvenly) => AlignContent::SPACE_EVENLY,
  }
}

/// The taffy style a computed style lays out with. A length mixing `px` and
/// `%` keeps its `px` part, since taffy takes one or the other here.
fn taffy_style(s: &Style, aspect: Option<f32>) -> taffy::Style {
  let rect = |v: [LenAuto; 4]| Rect { top: lpa(v[0]), right: lpa(v[1]), bottom: lpa(v[2]), left: lpa(v[3]) };
  taffy::Style {
    display: match s.display {
      css::Display::Flex => Display::Flex,
      css::Display::None => Display::None,
    },
    position: match s.position {
      css::Position::Relative => Position::Relative,
      css::Position::Absolute => Position::Absolute,
    },
    box_sizing: if s.border_box { BoxSizing::BorderBox } else { BoxSizing::ContentBox },
    overflow: if s.overflow_hidden { taffy::Point { x: taffy::Overflow::Hidden, y: taffy::Overflow::Hidden } } else { taffy::Point { x: taffy::Overflow::Visible, y: taffy::Overflow::Visible } },
    inset: rect(s.inset),
    size: Size { width: dim(s.width), height: dim(s.height) },
    min_size: Size { width: lpa(s.min_width), height: lpa(s.min_height) },
    max_size: Size { width: lpa(s.max_width), height: lpa(s.max_height) },
    aspect_ratio: aspect,
    margin: rect(s.margin),
    padding: Rect { top: lp(s.padding[0]), right: lp(s.padding[1]), bottom: lp(s.padding[2]), left: lp(s.padding[3]) },
    border: Rect { top: length(s.border(0)), right: length(s.border(1)), bottom: length(s.border(2)), left: length(s.border(3)) },
    flex_direction: match s.direction {
      css::Direction::Row => FlexDirection::Row,
      css::Direction::RowReverse => FlexDirection::RowReverse,
      css::Direction::Column => FlexDirection::Column,
      css::Direction::ColumnReverse => FlexDirection::ColumnReverse,
    },
    flex_wrap: match s.wrap {
      css::Wrap::NoWrap => FlexWrap::NoWrap,
      css::Wrap::Wrap => FlexWrap::Wrap,
      css::Wrap::WrapReverse => FlexWrap::WrapReverse,
    },
    flex_grow: s.grow,
    flex_shrink: s.shrink,
    flex_basis: dim(s.basis),
    align_items: items(s.align_items),
    align_self: s.align_self.map(|a| items(Some(a))),
    align_content: content(s.align_content),
    justify_content: content(s.justify_content),
    gap: Size { width: lp(s.column_gap), height: lp(s.row_gap) },
    ..Default::default()
  }
}

fn label(element: &Element, index: usize) -> String {
  format!("{}({})", element.tag(), index + 1)
}

impl<'a> Builder<'a> {
  fn style(&mut self, decls: &[(String, String)], parent: &Style, path: &str) -> Result<Style, CardError> {
    let mut error = None;
    let warnings = &mut *self.warnings;
    let style = css::compute(decls, parent, self.root_font, self.card, &mut |name, value, bad| match bad {
      Bad::Unsupported(reason) => warnings.push(Warning { element: path.to_owned(), message: format!("`{name}: {value}` is skipped: {reason}") }),
      Bad::Invalid(reason) => {
        if error.is_none() {
          error = Some(CardError::Value { element: path.to_owned(), property: name.to_owned(), value: value.to_owned(), reason });
        }
      }
    });
    match error {
      Some(e) => Err(e),
      None => Ok(style),
    }
  }

  fn build(&mut self, element: &Element, parent: &Style, path: String, depth: usize) -> Result<usize, CardError> {
    if depth > MAX_DEPTH {
      return Err(CardError::Depth { element: path });
    }
    let style = self.style(element.style(), parent, &path)?;
    for image in &style.background_images {
      if let Image::Url(key) = image {
        if !self.images.contains(key) {
          return Err(CardError::MissingImage { element: path, key: key.clone() });
        }
      }
    }
    let (kind, children, aspect) = match element {
      Element::Box { children, .. } => {
        let mut built = Vec::with_capacity(children.len());
        for (i, child) in children.iter().enumerate() {
          built.push(self.build(child, &style, format!("{path} > {}", label(child, i)), depth + 1)?);
        }
        (Kind::Box, built, None)
      }
      Element::Text { runs, .. } => {
        let mut texts = Vec::with_capacity(runs.len());
        for (i, run) in runs.iter().enumerate() {
          let run_style = if run.style.is_empty() { style.clone() } else { self.style(&run.style, &style, &format!("{path} > run({})", i + 1))? };
          texts.push((run.text.as_str(), RunStyle::of(&run_style)));
        }
        let paragraph = Paragraph::build(&texts, &style, self.faces);
        if !paragraph.missing.is_empty() {
          let chars: String = paragraph.missing.iter().collect();
          let points: Vec<String> = paragraph.missing.iter().map(|c| format!("U+{:04X}", *c as u32)).collect();
          self.warnings.push(Warning { element: path.clone(), message: format!("no font covers `{chars}` ({}); add a face that does", points.join(" ")) });
        }
        (Kind::Text(Box::new(paragraph)), Vec::new(), None)
      }
      Element::Image { src, width, height, .. } => {
        let pixmap = match src {
          ImageRef::Key(key) => self.images.get(key).ok_or_else(|| CardError::MissingImage { element: path.clone(), key: key.clone() })?,
          ImageRef::Bytes(bytes) => Arc::new(images::decode(bytes).map_err(|reason| CardError::Image { key: format!("the bytes of {path}"), reason })?),
        };
        let w = width.unwrap_or(pixmap.width() as f32);
        let h = height.unwrap_or(pixmap.height() as f32);
        let aspect = (h > 0.0).then_some(w / h);
        (Kind::Image(pixmap), Vec::new(), aspect)
      }
    };
    let mut ts = taffy_style(&style, aspect);
    if let Element::Image { width: Some(w), height: Some(h), .. } = element {
      if matches!(style.width, LenAuto::Auto) {
        ts.size.width = Dimension::length(*w);
      }
      if matches!(style.height, LenAuto::Auto) {
        ts.size.height = Dimension::length(*h);
      }
    }
    let ids: Vec<NodeId> = children.iter().map(|&c| self.nodes[c].id).collect();
    let index = self.nodes.len();
    let id = if ids.is_empty() { self.taffy.new_leaf_with_context(ts, index) } else { self.taffy.new_with_children(ts, &ids) }.map_err(|e| CardError::Layout(e.to_string()))?;
    self.nodes.push(Node { style, kind, children, id });
    Ok(index)
  }
}

pub(crate) fn build(root: &Element, card: (f32, f32), faces: &Faces<'_>, images: &Images, warnings: &mut Vec<Warning>) -> Result<Tree, CardError> {
  let base = Style::root();
  let root_decls = root.style();
  let root_font = {
    let probe = css::compute(&root_decls.iter().filter(|(n, _)| n.trim().eq_ignore_ascii_case("font-size")).cloned().collect::<Vec<_>>(), &base, 16.0, card, &mut |_, _, _| {});
    probe.font_size
  };
  let mut builder = Builder { faces, images, card, root_font, nodes: Vec::new(), taffy: TaffyTree::new(), warnings };
  let root_index = builder.build(root, &base, label(root, 0), 0)?;
  let mut tree = Tree { nodes: builder.nodes, taffy: builder.taffy, root: root_index };
  let root_id = tree.nodes[tree.root].id;
  let available = Size { width: AvailableSpace::Definite(card.0), height: AvailableSpace::Definite(card.1) };
  let root_style = tree.taffy.style(root_id).map_err(|e| CardError::Layout(e.to_string()))?.clone();
  let mut fixed = root_style;
  if fixed.size.width == Dimension::auto() {
    fixed.size.width = Dimension::length(card.0);
  }
  if fixed.size.height == Dimension::auto() {
    fixed.size.height = Dimension::length(card.1);
  }
  tree.taffy.set_style(root_id, fixed).map_err(|e| CardError::Layout(e.to_string()))?;
  let nodes = &tree.nodes;
  tree
    .taffy
    .compute_layout_with_measure(root_id, available, |inputs, _, context, style| {
      let kind = context.and_then(|i| nodes.get(*i)).map(|n| &n.kind);
      taffy::compute_leaf_layout(inputs, style, |_, _| 0.0, |known, avail| match kind {
        Some(Kind::Text(p)) => {
          let max = known.width.or(match avail.width {
            AvailableSpace::Definite(w) => Some(w),
            AvailableSpace::MinContent => Some(p.min_content()),
            AvailableSpace::MaxContent => None,
          });
          let lines = p.lines(max, faces);
          Size { width: known.width.unwrap_or(lines.width.ceil()), height: known.height.unwrap_or(lines.height.ceil()) }
        }
        Some(Kind::Image(px)) => {
          let (iw, ih) = (px.width() as f32, px.height() as f32);
          match (known.width, known.height) {
            (Some(w), Some(h)) => Size { width: w, height: h },
            (Some(w), None) => Size { width: w, height: if iw > 0.0 { w * ih / iw } else { 0.0 } },
            (None, Some(h)) => Size { width: if ih > 0.0 { h * iw / ih } else { 0.0 }, height: h },
            (None, None) => Size { width: iw, height: ih },
          }
        }
        _ => Size::ZERO,
      })
    })
    .map_err(|e| CardError::Layout(e.to_string()))?;
  Ok(tree)
}

impl Tree {
  /// The lines of a text node at its laid-out content width.
  pub fn lines(&self, index: usize, faces: &Faces<'_>) -> Option<Lines> {
    let node = &self.nodes[index];
    let Kind::Text(p) = &node.kind else { return None };
    let layout = self.taffy.layout(node.id).ok()?;
    let content = layout.size.width - layout.padding.left - layout.padding.right - layout.border.left - layout.border.right;
    Some(p.lines(Some(content.max(0.0) + 0.5), faces))
  }
}
