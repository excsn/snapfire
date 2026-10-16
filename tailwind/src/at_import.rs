//! `@import` and `@reference`: inlining stylesheets, wrapped in the layer, media and
//! supports conditions the import names.

use crate::Error;
use crate::ast::{CtxValue, Node, at_rule, context, ctx1};
use crate::css_parser;
use crate::features::Features;
use crate::value_parser::{self, ValueNode};
use crate::walk::{Action, walk};

/// A stylesheet an `@import` resolved to.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Loaded {
  /// Where the stylesheet was found, for messages.
  pub path: String,
  /// The directory its own imports resolve against.
  pub base: String,
  pub content: String,
}

impl Loaded {
  pub fn new(path: String, base: String, content: String) -> Self {
    Self { path, base, content }
  }
}

/// Resolves an `@import` id against the importing stylesheet's base.
pub type LoadStylesheet<'a> = dyn Fn(&str, &str) -> Result<Loaded, Error> + 'a;

pub(crate) struct ImportParams {
  pub uri: String,
  pub layer: Option<String>,
  pub media: Option<String>,
  pub supports: Option<String>,
}

pub(crate) fn parse_import_params(params: &[ValueNode]) -> Result<Option<ImportParams>, Error> {
  let mut uri: Option<String> = None;
  let mut layer: Option<String> = None;
  let mut media: Option<String> = None;
  let mut supports: Option<String> = None;

  for (i, node) in params.iter().enumerate() {
    if matches!(node, ValueNode::Separator(_)) {
      continue;
    }
    if let ValueNode::Word(value) = node
      && uri.is_none()
    {
      if value.is_empty() {
        return Ok(None);
      }
      if !value.starts_with('"') && !value.starts_with('\'') {
        return Ok(None);
      }
      uri = Some(value.get(1..value.len().saturating_sub(1)).unwrap_or("").to_owned());
      continue;
    }
    if let ValueNode::Function { value, .. } = node
      && value.eq_ignore_ascii_case("url")
    {
      return Ok(None);
    }
    if uri.is_none() {
      return Ok(None);
    }
    if node.value().eq_ignore_ascii_case("layer") && !matches!(node, ValueNode::Separator(_)) {
      if layer.is_some() {
        return Ok(None);
      }
      if supports.is_some() {
        return Err(Error::Message(
          "`layer(…)` in an `@import` should come before any other functions or conditions".to_owned(),
        ));
      }
      layer = Some(match node {
        ValueNode::Function { nodes, .. } => value_parser::to_css(nodes),
        _ => String::new(),
      });
      continue;
    }
    if let ValueNode::Function { value, nodes } = node
      && value.eq_ignore_ascii_case("supports")
    {
      if supports.is_some() {
        return Ok(None);
      }
      supports = Some(value_parser::to_css(nodes));
      continue;
    }
    media = Some(value_parser::to_css(&params[i..]));
    break;
  }

  Ok(uri.map(|uri| ImportParams {
    uri,
    layer,
    media,
    supports,
  }))
}

fn build_import_nodes(
  imported: Vec<Node>,
  layer: Option<String>,
  media: Option<String>,
  supports: Option<String>,
) -> Vec<Node> {
  let mut root = imported;
  if let Some(layer) = layer {
    root = vec![at_rule("@layer", layer, root)];
  }
  if let Some(media) = media {
    root = vec![at_rule("@media", media, root)];
  }
  if let Some(supports) = supports {
    let params = if supports.starts_with('(') {
      supports
    } else {
      format!("({supports})")
    };
    root = vec![at_rule("@supports", params, root)];
  }
  root
}

pub(crate) fn substitute_at_imports(
  ast: &mut Vec<Node>,
  base: &str,
  load: &LoadStylesheet,
  depth: usize,
  track: bool,
) -> Result<Features, Error> {
  let mut features = Features::NONE;
  let mut error: Option<Error> = None;

  walk(ast, &mut |node, _| {
    let Node::AtRule { name, params, .. } = node else {
      return Action::Continue;
    };
    if name != "@import" && name != "@reference" {
      return Action::Continue;
    }
    let parsed = match parse_import_params(&value_parser::parse(params)) {
      Ok(Some(p)) => p,
      Ok(None) => return Action::Continue,
      Err(e) => {
        error = Some(e);
        return Action::Stop;
      }
    };
    let ImportParams {
      uri,
      layer,
      mut media,
      supports,
    } = parsed;
    if name == "@reference" {
      media = Some("reference".to_owned());
    }
    features |= Features::AT_IMPORT;

    if uri.starts_with("data:") || uri.starts_with("http://") || uri.starts_with("https://") {
      return Action::Continue;
    }

    if depth > 100 {
      error = Some(Error::Message(format!(
        "Exceeded maximum recursion depth while resolving `{uri}` in `{base}`)"
      )));
      return Action::Stop;
    }

    let result = (|| -> Result<Node, Error> {
      let loaded = load(&uri, base)?;
      let mut imported = css_parser::parse(&loaded.content, if track { Some(loaded.path.as_str()) } else { None })?;
      substitute_at_imports(&mut imported, &loaded.base, load, depth + 1, track)?;
      let nodes = build_import_nodes(
        vec![context(ctx1("base", CtxValue::Str(loaded.base.clone())), imported)],
        layer,
        media,
        supports,
      );
      Ok(context(Default::default(), nodes))
    })();

    match result {
      Ok(node) => Action::ReplaceSkip(vec![node]),
      Err(e) => {
        error = Some(e);
        Action::Stop
      }
    }
  });

  match error {
    Some(e) => Err(e),
    None => Ok(features),
  }
}

const INDEX_CSS: &str = include_str!("../css/index.css");
const THEME_CSS: &str = include_str!("../css/theme.css");
const PREFLIGHT_CSS: &str = include_str!("../css/preflight.css");
const UTILITIES_CSS: &str = include_str!("../css/utilities.css");

/// Tailwind's own stylesheets: `tailwindcss` and `tailwindcss/theme.css`,
/// `/preflight.css` and `/utilities.css`, plus the relative imports `index.css` makes.
pub fn tailwind_stylesheet(id: &str, base: &str) -> Option<Loaded> {
  let file = match id {
    "tailwindcss" | "tailwindcss/index.css" | "tailwindcss/index" => "index.css",
    "tailwindcss/theme.css" | "tailwindcss/theme" => "theme.css",
    "tailwindcss/preflight.css" | "tailwindcss/preflight" => "preflight.css",
    "tailwindcss/utilities.css" | "tailwindcss/utilities" => "utilities.css",
    "./theme.css" if base == TAILWIND_BASE => "theme.css",
    "./preflight.css" if base == TAILWIND_BASE => "preflight.css",
    "./utilities.css" if base == TAILWIND_BASE => "utilities.css",
    _ => return None,
  };
  let content = match file {
    "index.css" => INDEX_CSS,
    "theme.css" => THEME_CSS,
    "preflight.css" => PREFLIGHT_CSS,
    _ => UTILITIES_CSS,
  };
  Some(Loaded {
    path: format!("{TAILWIND_BASE}/{file}"),
    base: TAILWIND_BASE.to_owned(),
    content: content.to_owned(),
  })
}

/// The base Tailwind's own stylesheets resolve their relative imports against.
pub const TAILWIND_BASE: &str = "tailwindcss";
