//! The shapes most utilities share: a fixed list of declarations, a value looked up in the
//! theme with bare and arbitrary fallbacks, a color with an opacity modifier, a spacing scale.

use std::rc::Rc;

use crate::ast::Node;
use crate::candidate::{Candidate, UtilityValue};
use crate::design_system::DesignSystem;
use crate::theme::ThemeOptions;
use crate::utilities::{Compiled, Utilities, as_color};
use crate::utils::infer::{is_positive_integer, is_valid_spacing_multiplier};
use crate::utils::math::add_whitespace_around_math_operators;
use crate::utils::segment::segment;

pub type Handle = Rc<dyn Fn(&str, Option<&str>, &DesignSystem) -> Option<Vec<Node>>>;
pub type Bare = Rc<dyn Fn(&str, Option<&str>, &DesignSystem) -> Option<String>>;

pub fn nodes(result: Option<Vec<Node>>) -> Compiled {
  match result {
    Some(nodes) => Compiled::Nodes(nodes),
    None => Compiled::Skip,
  }
}

/// A declaration or a node built when the utility compiles.
pub enum Item {
  Decl(&'static str, String),
  Node(fn() -> Node),
}

pub fn d(property: &'static str, value: impl Into<String>) -> Item {
  Item::Decl(property, value.into())
}

pub fn static_utility(u: &mut Utilities, name: &str, items: Vec<Item>) {
  let items = Rc::new(items);
  u.static_utility(name, move |_, _| {
    Compiled::Nodes(
      items
        .iter()
        .map(|item| match item {
          Item::Decl(p, v) => crate::ast::decl(*p, v.clone()),
          Item::Node(f) => f(),
        })
        .collect(),
    )
  });
}

pub fn static_decls(u: &mut Utilities, name: &str, decls: &[(&'static str, &str)]) {
  static_utility(u, name, decls.iter().map(|(p, v)| d(p, *v)).collect());
}

#[derive(Clone, Default)]
pub enum DefaultValue {
  #[default]
  Undefined,
  Null,
  Value(String),
}

#[derive(Clone)]
pub struct Desc {
  pub supports_negative: bool,
  pub supports_fractions: bool,
  pub theme_keys: Vec<&'static str>,
  pub default_value: DefaultValue,
  pub static_values: Vec<(&'static str, Vec<Node>)>,
  pub handle_bare_value: Option<Bare>,
  pub handle_negative_bare_value: Option<Bare>,
  pub handle: Handle,
}

impl Desc {
  pub fn new(handle: impl Fn(&str, Option<&str>, &DesignSystem) -> Option<Vec<Node>> + 'static) -> Desc {
    Desc {
      supports_negative: false,
      supports_fractions: false,
      theme_keys: Vec::new(),
      default_value: DefaultValue::Undefined,
      static_values: Vec::new(),
      handle_bare_value: None,
      handle_negative_bare_value: None,
      handle: Rc::new(handle),
    }
  }

  pub fn keys(mut self, keys: &[&'static str]) -> Desc {
    self.theme_keys = keys.to_vec();
    self
  }

  pub fn negative(mut self) -> Desc {
    self.supports_negative = true;
    self
  }

  pub fn default_value(mut self, value: &str) -> Desc {
    self.default_value = DefaultValue::Value(value.to_owned());
    self
  }

  pub fn default_null(mut self) -> Desc {
    self.default_value = DefaultValue::Null;
    self
  }

  pub fn statics(mut self, values: Vec<(&'static str, Vec<Node>)>) -> Desc {
    self.static_values = values;
    self
  }

  pub fn bare(mut self, f: impl Fn(&str, Option<&str>, &DesignSystem) -> Option<String> + 'static) -> Desc {
    self.handle_bare_value = Some(Rc::new(f));
    self
  }

  pub fn negative_bare(mut self, f: impl Fn(&str, Option<&str>, &DesignSystem) -> Option<String> + 'static) -> Desc {
    self.handle_negative_bare_value = Some(Rc::new(f));
    self
  }

  /// `handleBareValue: ({ value }) => isPositiveInteger(value) ? value : null`
  pub fn bare_integer(self) -> Desc {
    self.bare(|value, _, _| {
      if is_positive_integer(value) {
        Some(value.to_owned())
      } else {
        None
      }
    })
  }
}

pub fn functional_utility(u: &mut Utilities, root: &str, desc: Desc) {
  let desc = Rc::new(desc);
  if desc.supports_negative {
    let d = desc.clone();
    u.functional(&format!("-{root}"), move |candidate, ds| {
      run_functional(&d, candidate, ds, true)
    });
  }
  let d = desc.clone();
  u.functional(root, move |candidate, ds| run_functional(&d, candidate, ds, false));
}

fn run_functional(desc: &Desc, candidate: &Candidate, ds: &DesignSystem, negative: bool) -> Compiled {
  let modifier = candidate.modifier();
  let mut value: Option<String>;
  let mut data_type: Option<String> = None;
  let keys = &desc.theme_keys;

  match candidate.value() {
    None => {
      if modifier.is_some() {
        return Compiled::Skip;
      }
      value = match &desc.default_value {
        DefaultValue::Undefined => ds.theme.resolve(None, keys, ThemeOptions::NONE),
        DefaultValue::Null => None,
        DefaultValue::Value(v) => Some(v.clone()),
      };
    }
    Some(UtilityValue::Arbitrary {
      data_type: dt,
      value: v,
    }) => {
      if modifier.is_some() {
        return Compiled::Skip;
      }
      value = Some(v.clone());
      data_type = dt.clone();
    }
    Some(UtilityValue::Named { value: v, fraction }) => {
      value = ds
        .theme
        .resolve(Some(fraction.as_deref().unwrap_or(v)), keys, ThemeOptions::NONE);

      if value.is_none()
        && desc.supports_fractions
        && let Some(fraction) = fraction
      {
        let parts = segment(fraction, b'/');
        let (lhs, rhs) = (parts[0], parts.get(1).copied().unwrap_or(""));
        if !is_positive_integer(lhs) || !is_positive_integer(rhs) {
          return Compiled::Skip;
        }
        value = Some(format!("calc({lhs} / {rhs} * 100%)"));
      }

      if value.is_none()
        && negative
        && let Some(bare) = &desc.handle_negative_bare_value
      {
        value = bare(v, fraction.as_deref(), ds);
        if !value.as_deref().is_some_and(|v| v.contains('/')) && modifier.is_some() {
          return Compiled::Skip;
        }
        if let Some(value) = value {
          return nodes((desc.handle)(&value, None, ds));
        }
      }

      if value.is_none()
        && let Some(bare) = &desc.handle_bare_value
      {
        value = bare(v, fraction.as_deref(), ds);
        if !value.as_deref().is_some_and(|v| v.contains('/')) && modifier.is_some() {
          return Compiled::Skip;
        }
      }

      if value.is_none()
        && !negative
        && !desc.static_values.is_empty()
        && modifier.is_none()
        && let Some((_, fallback)) = desc.static_values.iter().find(|(k, _)| *k == v)
      {
        return Compiled::Nodes(fallback.clone());
      }
    }
  }

  let Some(value) = value else { return Compiled::Skip };
  let value = if negative {
    add_whitespace_around_math_operators(&format!("calc({value} * -1)"))
  } else {
    value
  };
  nodes((desc.handle)(&value, data_type.as_deref(), ds))
}

/// A color utility like `bg-*`: a theme color or an arbitrary one, with `/50` opacity.
pub fn color_utility(
  u: &mut Utilities,
  root: &str,
  keys: &'static [&'static str],
  handle: impl Fn(&str, &DesignSystem) -> Option<Vec<Node>> + 'static,
) {
  u.functional(root, move |candidate, ds| {
    let Some(value) = candidate.value() else {
      return Compiled::Skip;
    };
    let value = match value {
      UtilityValue::Arbitrary { value, .. } => as_color(value, candidate.modifier(), &ds.theme),
      UtilityValue::Named { .. } => resolve_theme_color(candidate, ds, keys),
    };
    let Some(value) = value else { return Compiled::Skip };
    nodes(handle(&value, ds))
  });
}

pub fn resolve_theme_color(candidate: &Candidate, ds: &DesignSystem, keys: &[&str]) -> Option<String> {
  let Some(UtilityValue::Named { value, .. }) = candidate.value() else {
    return None;
  };
  let resolved = match value.as_str() {
    "inherit" => Some("inherit".to_owned()),
    "transparent" => Some("transparent".to_owned()),
    "current" => Some("currentcolor".to_owned()),
    other => ds.theme.resolve(Some(other), keys, ThemeOptions::NONE),
  }?;
  as_color(&resolved, candidate.modifier(), &ds.theme)
}

/// A spacing utility like `p-*`: the theme keys, then `--spacing(n)` for a bare multiple
/// of 0.25, plus `-px` and `px`.
pub fn spacing_utility(
  u: &mut Utilities,
  name: &str,
  keys: &[&'static str],
  handle: impl Fn(&str) -> Vec<Node> + 'static,
  supports_negative: bool,
  supports_fractions: bool,
  static_values: Vec<(&'static str, Vec<Node>)>,
) {
  let handle = Rc::new(handle);
  if supports_negative {
    let h = handle.clone();
    u.static_utility(&format!("-{name}-px"), move |_, _| Compiled::Nodes(h("-1px")));
  }
  let h = handle.clone();
  u.static_utility(&format!("{name}-px"), move |_, _| Compiled::Nodes(h("1px")));

  let h = handle.clone();
  let mut desc = Desc::new(move |value, _, _| Some(h(value)))
    .keys(keys)
    .default_null()
    .bare(|value, _, ds| {
      ds.theme.resolve(None, &["--spacing"], ThemeOptions::NONE)?;
      if !is_valid_spacing_multiplier(value) {
        return None;
      }
      Some(format!("--spacing({value})"))
    })
    .negative_bare(|value, _, ds| {
      ds.theme.resolve(None, &["--spacing"], ThemeOptions::NONE)?;
      if !is_valid_spacing_multiplier(value) {
        return None;
      }
      Some(format!("--spacing(-{value})"))
    })
    .statics(static_values);
  desc.supports_negative = supports_negative;
  desc.supports_fractions = supports_fractions;
  functional_utility(u, name, desc);
}
