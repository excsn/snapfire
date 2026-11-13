//! Prebuilt markup: a literal element subtree written once when the plan is
//! built and carried as a `Tmpl::Marker`, so neither the plan nor the server
//! holds it as a tree.

use super::*;
use crate::ast::Expr;

/// The markup `text` renders to when every set of markup rules writes it
/// alike, written after markup rather than after other text.
fn static_text(text: &str) -> Option<String> {
  let mut agreed: Option<String> = None;
  for markup in Markup::every() {
    let mut out = Out::default();
    out.text(text, markup);
    match &agreed {
      Some(first) if *first != out.html => return None,
      Some(_) => {}
      None => agreed = Some(out.html),
    }
  }
  agreed
}

/// `component`'s render with its literal markup prebuilt. An element with
/// literal attributes whose open tag every set of markup rules writes alike
/// becomes that open tag, its children and its close tag, which is what the
/// renderer writes for it. Text every set of rules escapes alike becomes its
/// markup. Adjacent prebuilt pieces join into one `Tmpl::Marker`, so only
/// text the rules escape differently and markup that is not literal stay
/// nodes.
///
/// Only markup the renderer's per-render state cannot reach is prebuilt: under
/// an ordinary element or a fragment, where a component's root stamps and
/// fallthrough attributes have already been spent. Nothing is prebuilt among the
/// children a component or an island is given, which render under their
/// caller's slotted stamp.
pub fn bake_static_markup(component: &mut Component) {
  within(&mut component.render, false);
}

enum Piece {
  Markup(String),
  Text(String, String),
  Node(Tmpl),
}

fn within(tmpl: &mut Tmpl, settled: bool) {
  match tmpl {
    Tmpl::Element { tag, children, .. } => {
      let settled = settled || !tag.starts_with('$');
      bake_children(children, settled);
    }
    Tmpl::Fragment(children) => bake_children(children, true),
    Tmpl::If { then, r#else, .. } => {
      bake_one(then, settled);
      if let Some(other) = r#else {
        bake_one(other, settled);
      }
    }
    Tmpl::For { body, .. } => bake_one(body, settled),
    _ => {}
  }
}

fn bake_one(tmpl: &mut Tmpl, settled: bool) {
  if !settled {
    within(tmpl, false);
    return;
  }
  let taken = std::mem::replace(tmpl, Tmpl::Fragment(Vec::new()));
  let mut joined = join(pieces(taken));
  *tmpl = match joined.len() {
    1 => joined.pop().expect("one piece"),
    _ => Tmpl::Fragment(joined),
  };
}

fn bake_children(children: &mut Vec<Tmpl>, settled: bool) {
  if !settled {
    children.iter_mut().for_each(|child| within(child, false));
    return;
  }
  let mut all = Vec::new();
  for child in std::mem::take(children) {
    all.extend(pieces(child));
  }
  *children = join(all);
}

fn pieces(tmpl: Tmpl) -> Vec<Piece> {
  match tmpl {
    Tmpl::Text(text) => match static_text(&text) {
      Some(markup) => vec![Piece::Text(markup, text)],
      None => vec![Piece::Node(Tmpl::Text(text))],
    },
    Tmpl::Element { tag, attrs, children } if splits(&tag) && attrs_literal(&attrs) && !crate::ast::read_by_host(&tag, &attrs) => match baked_open(&tag, &attrs) {
      Some(open) => {
        let mut out = vec![Piece::Markup(open)];
        for child in children {
          out.extend(pieces(child));
        }
        out.push(Piece::Markup(format!("</{tag}>")));
        out
      }
      None => {
        let mut element = Tmpl::Element { tag, attrs, children };
        within(&mut element, true);
        vec![Piece::Node(element)]
      }
    },
    mut other => {
      within(&mut other, true);
      vec![Piece::Node(other)]
    }
  }
}

/// Whether an element can be written as its tags around its children: one
/// that changes how what is inside it renders cannot.
fn splits(tag: &str) -> bool {
  !tag.starts_with('$') && !matches!(tag, "svg" | "foreignObject" | "noscript")
}

fn attrs_literal(attrs: &[Entry]) -> bool {
  attrs.iter().all(|entry| matches!(entry, Entry::Field(name, Expr::Lit(_)) if !name.starts_with('$')))
}

/// The pieces as nodes, each run of markup one `Tmpl::Marker`. Agreed text
/// joins a run only between two markup pieces, so it never stands next to
/// other text, where React's rules would separate the two.
fn join(pieces: Vec<Piece>) -> Vec<Tmpl> {
  let mut out: Vec<Tmpl> = Vec::with_capacity(pieces.len());
  let mut run: Option<String> = None;
  let mut held_text: Option<(String, String)> = None;
  for piece in pieces {
    match piece {
      Piece::Markup(markup) => {
        if let Some((escaped, text)) = held_text.take() {
          match run.as_mut() {
            Some(run) => run.push_str(&escaped),
            None => out.push(Tmpl::Text(text)),
          }
        }
        run.get_or_insert_with(String::new).push_str(&markup);
      }
      Piece::Text(escaped, text) => {
        if let Some((_, held)) = held_text.take() {
          if let Some(run) = run.take() {
            out.push(Tmpl::Marker(run));
          }
          out.push(Tmpl::Text(held));
        }
        held_text = Some((escaped, text));
      }
      Piece::Node(node) => {
        if let Some(run) = run.take() {
          out.push(Tmpl::Marker(run));
        }
        if let Some((_, text)) = held_text.take() {
          out.push(Tmpl::Text(text));
        }
        out.push(node);
      }
    }
  }
  if let Some(run) = run.take() {
    out.push(Tmpl::Marker(run));
  }
  if let Some((_, text)) = held_text.take() {
    out.push(Tmpl::Text(text));
  }
  out
}
