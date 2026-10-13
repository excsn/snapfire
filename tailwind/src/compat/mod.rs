//! What Tailwind keeps for stylesheets written against v3 and v4.0: utilities that were
//! renamed and `theme()` with a dotted path.

use crate::ast::decl;
use crate::candidate::UtilityValue;
use crate::design_system::DesignSystem;
use crate::theme::ThemeOptions;
use crate::utilities::{Compiled, Utilities};
use crate::utils::infer::{is_positive_integer, is_valid_spacing_multiplier};
use crate::utils::segment::segment;

mod default_theme;
pub(crate) mod theme;

pub use theme::LegacyTheme;

fn statics(utilities: &mut Utilities, name: &str, decls: &'static [(&'static str, &'static str)]) {
  utilities.static_utility(name, move |_, _| {
    Compiled::Nodes(decls.iter().map(|(p, v)| decl(*p, *v)).collect())
  });
}

pub fn register_legacy_utilities(utilities: &mut Utilities) {
  for (value, direction) in [
    ("t", "top"),
    ("tr", "top right"),
    ("r", "right"),
    ("br", "bottom right"),
    ("b", "bottom"),
    ("bl", "bottom left"),
    ("l", "left"),
    ("tl", "top left"),
  ] {
    let position = format!("to {direction} in oklab");
    utilities.static_utility(&format!("bg-gradient-to-{value}"), move |_, _| {
      Compiled::Nodes(vec![
        decl("--tw-gradient-position", position.clone()),
        decl("background-image", "linear-gradient(var(--tw-gradient-stops))"),
      ])
    });
  }

  statics(utilities, "bg-left-top", &[("background-position", "left top")]);
  statics(utilities, "bg-right-top", &[("background-position", "right top")]);
  statics(utilities, "bg-left-bottom", &[("background-position", "left bottom")]);
  statics(utilities, "bg-right-bottom", &[("background-position", "right bottom")]);
  statics(utilities, "object-left-top", &[("object-position", "left top")]);
  statics(utilities, "object-right-top", &[("object-position", "right top")]);
  statics(utilities, "object-left-bottom", &[("object-position", "left bottom")]);
  statics(utilities, "object-right-bottom", &[("object-position", "right bottom")]);

  utilities.functional("max-w-screen", |candidate, ds| {
    let Some(UtilityValue::Named { value, .. }) = candidate.value() else {
      return Compiled::Skip;
    };
    match ds.theme.resolve(Some(value), &["--breakpoint"], ThemeOptions::NONE) {
      Some(v) => Compiled::Nodes(vec![decl("max-width", v)]),
      None => Compiled::Skip,
    }
  });

  statics(utilities, "overflow-ellipsis", &[("text-overflow", "ellipsis")]);
  statics(
    utilities,
    "decoration-slice",
    &[
      ("-webkit-box-decoration-break", "slice"),
      ("box-decoration-break", "slice"),
    ],
  );
  statics(
    utilities,
    "decoration-clone",
    &[
      ("-webkit-box-decoration-break", "clone"),
      ("box-decoration-break", "clone"),
    ],
  );

  for (root, property) in [("flex-shrink", "flex-shrink"), ("flex-grow", "flex-grow")] {
    utilities.functional(root, move |candidate, _| {
      if candidate.modifier().is_some() {
        return Compiled::Skip;
      }
      match candidate.value() {
        None => Compiled::Nodes(vec![decl(property, "1")]),
        Some(UtilityValue::Arbitrary { value, .. }) => Compiled::Nodes(vec![decl(property, value.clone())]),
        Some(UtilityValue::Named { value, .. }) if is_positive_integer(value) => {
          Compiled::Nodes(vec![decl(property, value.clone())])
        }
        _ => Compiled::Skip,
      }
    });
  }

  statics(utilities, "order-none", &[("order", "0")]);
  statics(utilities, "break-words", &[("overflow-wrap", "break-word")]);

  for (name, property) in [("start", "inset-inline-start"), ("end", "inset-inline-end")] {
    let p = property;
    utilities.static_utility(&format!("{name}-auto"), move |_, _| {
      Compiled::Nodes(vec![decl(p, "auto")])
    });
    utilities.static_utility(&format!("{name}-full"), move |_, _| {
      Compiled::Nodes(vec![decl(p, "100%")])
    });
    utilities.static_utility(&format!("-{name}-full"), move |_, _| {
      Compiled::Nodes(vec![decl(p, "-100%")])
    });
    utilities.static_utility(&format!("{name}-px"), move |_, _| Compiled::Nodes(vec![decl(p, "1px")]));
    utilities.static_utility(&format!("-{name}-px"), move |_, _| {
      Compiled::Nodes(vec![decl(p, "-1px")])
    });

    for negative in [true, false] {
      let root = if negative { format!("-{name}") } else { name.to_owned() };
      utilities.functional(&root, move |candidate, ds| inset(candidate, ds, property, negative));
    }
  }
}

fn inset(
  candidate: &crate::candidate::Candidate,
  ds: &DesignSystem,
  property: &'static str,
  negative: bool,
) -> Compiled {
  let Some(value) = candidate.value() else {
    return Compiled::Skip;
  };
  match value {
    UtilityValue::Arbitrary { value, .. } => {
      if candidate.modifier().is_some() {
        return Compiled::Skip;
      }
      let v = if negative {
        format!("calc({value} * -1)")
      } else {
        value.clone()
      };
      Compiled::Nodes(vec![decl(property, v)])
    }
    UtilityValue::Named { value, fraction } => {
      let mut resolved = ds.theme.resolve(
        Some(fraction.as_deref().unwrap_or(value)),
        &["--inset", "--spacing"],
        ThemeOptions::NONE,
      );

      if resolved.is_none()
        && let Some(fraction) = fraction
      {
        let parts = segment(fraction, b'/');
        let (lhs, rhs) = (parts[0], parts.get(1).copied().unwrap_or(""));
        if !is_positive_integer(lhs) || !is_positive_integer(rhs) {
          return Compiled::Skip;
        }
        resolved = Some(format!("calc({fraction} * 100%)"));
      }

      if resolved.is_none() && negative {
        let multiplier = ds.theme.resolve(None, &["--spacing"], ThemeOptions::NONE);
        if let Some(multiplier) = multiplier
          && is_valid_spacing_multiplier(value)
        {
          return Compiled::Nodes(vec![decl(property, format!("calc({multiplier} * -{value})"))]);
        }
      }

      if resolved.is_none() {
        let multiplier = ds.theme.resolve(None, &["--spacing"], ThemeOptions::NONE);
        if let Some(multiplier) = multiplier
          && is_valid_spacing_multiplier(value)
        {
          resolved = Some(format!("calc({multiplier} * {value})"));
        }
      }

      let Some(resolved) = resolved else {
        return Compiled::Skip;
      };
      Compiled::Nodes(vec![decl(
        property,
        if negative {
          format!("calc({resolved} * -1)")
        } else {
          resolved
        },
      )])
    }
  }
}
