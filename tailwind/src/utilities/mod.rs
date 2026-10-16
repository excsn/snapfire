//! Utilities: what each class root compiles to.

mod backgrounds;
pub(crate) mod css;
mod effects;
mod helpers;
mod layout;
mod masks;
mod shadows;

pub use css::{create_css_utility, is_valid_functional_utility_name, is_valid_static_utility_name};

use indexmap::IndexMap;

use crate::ast::{Node, at_rule, decl};
use crate::candidate::{Candidate, Modifier};
use crate::design_system::DesignSystem;
use crate::theme::{Theme, ThemeOptions};
use crate::utils::infer::is_valid_opacity_value;
use crate::utils::js::{number_to_string, to_number};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
  Static,
  Functional,
}

/// What a utility made of a candidate. `Skip` lets the next utility with the same root try;
/// `Invalid` also does, unless the utility declared value types.
#[non_exhaustive]
pub enum Compiled {
  Skip,
  Invalid,
  Nodes(Vec<Node>),
}

pub type CompileFn = Box<dyn Fn(&Candidate, &DesignSystem) -> Compiled>;

#[non_exhaustive]
pub struct Utility {
  pub kind: Kind,
  pub compile: CompileFn,
  pub types: Vec<String>,
}

#[derive(Default)]
pub struct Utilities {
  map: IndexMap<String, Vec<Utility>>,
}

impl Utilities {
  pub fn static_utility(&mut self, name: &str, compile: impl Fn(&Candidate, &DesignSystem) -> Compiled + 'static) {
    self.map.entry(name.to_owned()).or_default().push(Utility {
      kind: Kind::Static,
      compile: Box::new(compile),
      types: Vec::new(),
    });
  }

  pub fn functional(&mut self, name: &str, compile: impl Fn(&Candidate, &DesignSystem) -> Compiled + 'static) {
    self.map.entry(name.to_owned()).or_default().push(Utility {
      kind: Kind::Functional,
      compile: Box::new(compile),
      types: Vec::new(),
    });
  }

  pub fn has(&self, name: &str, kind: Kind) -> bool {
    self.map.get(name).is_some_and(|fns| fns.iter().any(|u| u.kind == kind))
  }

  pub fn get(&self, name: &str) -> &[Utility] {
    self.map.get(name).map(Vec::as_slice).unwrap_or(&[])
  }
}

pub(crate) fn property(ident: &str, initial: Option<&str>, syntax: Option<&str>) -> Node {
  let mut nodes = vec![
    decl(
      "syntax",
      match syntax {
        Some(s) => format!("\"{s}\""),
        None => "\"*\"".to_owned(),
      },
    ),
    decl("inherits", "false"),
  ];
  if let Some(initial) = initial
    && !initial.is_empty()
  {
    nodes.push(decl("initial-value", initial));
  }
  at_rule("@property", ident, nodes)
}

fn alpha_percentage(alpha: &str) -> String {
  let n = to_number(alpha);
  if n.is_nan() {
    alpha.to_owned()
  } else {
    format!("{}%", number_to_string(n * 100.0))
  }
}

/// Applies an opacity to a color with `color-mix()`.
pub fn with_alpha(value: &str, alpha: &str) -> String {
  let alpha = alpha_percentage(alpha);
  if alpha == "100%" {
    return value.to_owned();
  }
  format!("color-mix(in oklab, {value} {alpha}, transparent)")
}

/// Replaces a color's opacity with relative color syntax.
pub fn replace_alpha(value: &str, alpha: &str) -> String {
  format!("oklab(from {value} l a b / {})", alpha_percentage(alpha))
}

/// A color with its opacity modifier applied. `None` when the modifier is not a valid opacity.
pub fn as_color(value: &str, modifier: Option<&Modifier>, theme: &Theme) -> Option<String> {
  let Some(modifier) = modifier else {
    return Some(value.to_owned());
  };
  match modifier {
    Modifier::Arbitrary(alpha) => Some(with_alpha(value, alpha)),
    Modifier::Named(name) => {
      if let Some(alpha) = theme.resolve(Some(name), &["--opacity"], ThemeOptions::NONE) {
        return Some(with_alpha(value, &alpha));
      }
      if !is_valid_opacity_value(name) {
        return None;
      }
      Some(with_alpha(value, &format!("{name}%")))
    }
  }
}

pub fn create_utilities(theme: &Theme) -> Utilities {
  let mut utilities = Utilities::default();
  layout::register(&mut utilities, theme);
  backgrounds::register(&mut utilities);
  masks::register(&mut utilities);
  effects::register(&mut utilities, theme);
  shadows::register(&mut utilities, theme);
  utilities
}
