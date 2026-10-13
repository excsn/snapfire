//! Backgrounds, gradients and masks, in Tailwind's registration order.

use std::rc::Rc;

use crate::ast::{Node, at_root, decl, rule};
use crate::candidate::{Candidate, Modifier, UtilityValue};
use crate::theme::ThemeOptions;
use crate::utilities::helpers::{Desc, functional_utility, resolve_theme_color, static_decls};
use crate::utilities::{Compiled, Utilities, as_color, property};
use crate::utils::infer::{DataType, infer_data_type, is_positive_integer, is_valid_spacing_multiplier};

fn decls(property: &'static str, value: &str) -> Vec<Node> {
  vec![decl(property, value)]
}

fn data_type(value: &str, hint: &Option<String>, types: &[DataType]) -> Option<String> {
  match hint {
    Some(h) => Some(h.clone()),
    None => infer_data_type(value, types).map(|t| t.name().to_owned()),
  }
}

fn interpolation(modifier: Option<&Modifier>) -> String {
  match modifier {
    Some(Modifier::Named(v)) => match v.as_str() {
      "longer" | "shorter" | "increasing" | "decreasing" => format!("in oklch {v} hue"),
      other => format!("in {other}"),
    },
    Some(Modifier::Arbitrary(v)) => v.clone(),
    None => "in oklab".to_owned(),
  }
}

fn bg_linear(candidate: &Candidate, negative: bool) -> Compiled {
  let Some(value) = candidate.value() else {
    return Compiled::Skip;
  };
  let value = match value {
    UtilityValue::Arbitrary { value, data_type: hint } => {
      if candidate.modifier().is_some() {
        return Compiled::Skip;
      }
      let ty = data_type(value, hint, &[DataType::Angle]);
      let value = if ty.as_deref() == Some("angle") {
        if negative {
          format!("calc({value} * -1)")
        } else {
          value.clone()
        }
      } else {
        if negative {
          return Compiled::Skip;
        }
        value.clone()
      };
      return Compiled::Nodes(vec![
        decl("--tw-gradient-position", value.clone()),
        decl(
          "background-image",
          format!("linear-gradient(var(--tw-gradient-stops,{value}))"),
        ),
      ]);
    }
    UtilityValue::Named { value, .. } => {
      let direction = match value.as_str() {
        "to-t" => Some("to top"),
        "to-tr" => Some("to top right"),
        "to-r" => Some("to right"),
        "to-br" => Some("to bottom right"),
        "to-b" => Some("to bottom"),
        "to-bl" => Some("to bottom left"),
        "to-l" => Some("to left"),
        "to-tl" => Some("to top left"),
        _ => None,
      };
      match direction {
        Some(d) if !negative => d.to_owned(),
        _ if is_positive_integer(value) => {
          if negative {
            format!("calc({value}deg * -1)")
          } else {
            format!("{value}deg")
          }
        }
        _ => return Compiled::Skip,
      }
    }
  };
  let method = interpolation(candidate.modifier());
  Compiled::Nodes(vec![
    decl("--tw-gradient-position", value.clone()),
    rule(
      "@supports (background-image: linear-gradient(in lab, red, red))",
      vec![decl("--tw-gradient-position", format!("{value} {method}"))],
    ),
    decl("background-image", "linear-gradient(var(--tw-gradient-stops))"),
  ])
}

fn bg_conic(candidate: &Candidate, negative: bool) -> Compiled {
  if let Some(UtilityValue::Arbitrary { value, .. }) = candidate.value() {
    if candidate.modifier().is_some() {
      return Compiled::Skip;
    }
    return Compiled::Nodes(vec![
      decl("--tw-gradient-position", value.clone()),
      decl(
        "background-image",
        format!("conic-gradient(var(--tw-gradient-stops,{value}))"),
      ),
    ]);
  }
  let method = interpolation(candidate.modifier());
  let Some(UtilityValue::Named { value, .. }) = candidate.value() else {
    return Compiled::Nodes(vec![
      decl("--tw-gradient-position", method),
      decl("background-image", "conic-gradient(var(--tw-gradient-stops))"),
    ]);
  };
  if !is_positive_integer(value) {
    return Compiled::Skip;
  }
  let value = if negative {
    format!("calc({value}deg * -1)")
  } else {
    format!("{value}deg")
  };
  Compiled::Nodes(vec![
    decl("--tw-gradient-position", format!("from {value} {method}")),
    decl("background-image", "conic-gradient(var(--tw-gradient-stops))"),
  ])
}

fn gradient_stop_properties() -> Node {
  at_root(vec![
    property("--tw-gradient-position", None, None),
    property("--tw-gradient-from", Some("#0000"), Some("<color>")),
    property("--tw-gradient-via", Some("#0000"), Some("<color>")),
    property("--tw-gradient-to", Some("#0000"), Some("<color>")),
    property("--tw-gradient-stops", None, None),
    property("--tw-gradient-via-stops", None, None),
    property("--tw-gradient-from-position", Some("0%"), Some("<length-percentage>")),
    property("--tw-gradient-via-position", Some("50%"), Some("<length-percentage>")),
    property("--tw-gradient-to-position", Some("100%"), Some("<length-percentage>")),
  ])
}

pub(super) type StopFn = Rc<dyn Fn(&str) -> Vec<Node>>;

fn gradient_stop_utility(u: &mut Utilities, root: &str, color: StopFn, position: StopFn) {
  u.functional(root, move |candidate, ds| {
    let Some(value) = candidate.value() else {
      return Compiled::Skip;
    };
    match value {
      UtilityValue::Arbitrary { value, data_type: hint } => {
        match data_type(value, hint, &[DataType::Color, DataType::Length, DataType::Percentage]).as_deref() {
          Some("length") | Some("percentage") => {
            if candidate.modifier().is_some() {
              return Compiled::Skip;
            }
            Compiled::Nodes(position(value))
          }
          _ => match as_color(value, candidate.modifier(), &ds.theme) {
            Some(v) => Compiled::Nodes(color(&v)),
            None => Compiled::Skip,
          },
        }
      }
      UtilityValue::Named { value, .. } => {
        if let Some(v) = resolve_theme_color(candidate, ds, &["--background-color", "--color"])
          && !v.is_empty()
        {
          return Compiled::Nodes(color(&v));
        }
        if candidate.modifier().is_some() {
          return Compiled::Skip;
        }
        if let Some(v) = ds
          .theme
          .resolve(Some(value), &["--gradient-color-stop-positions"], ThemeOptions::NONE)
          && !v.is_empty()
        {
          return Compiled::Nodes(position(&v));
        }
        if value.ends_with('%') && is_positive_integer(&value[..value.len() - 1]) {
          return Compiled::Nodes(position(value));
        }
        Compiled::Skip
      }
    }
  });
}

pub(super) fn mask_properties_gradient() -> Node {
  at_root(vec![
    property("--tw-mask-linear", Some("linear-gradient(#fff, #fff)"), None),
    property("--tw-mask-radial", Some("linear-gradient(#fff, #fff)"), None),
    property("--tw-mask-conic", Some("linear-gradient(#fff, #fff)"), None),
  ])
}

/// `maskStopUtility`: a color, a percentage or a spacing multiple for a mask gradient stop.
pub(super) fn mask_stop_utility(u: &mut Utilities, root: &str, color: StopFn, position: StopFn) {
  u.functional(root, move |candidate, ds| {
    let Some(value) = candidate.value() else {
      return Compiled::Skip;
    };
    match value {
      UtilityValue::Arbitrary { value, data_type: hint } => {
        match data_type(value, hint, &[DataType::Length, DataType::Percentage, DataType::Color]).as_deref() {
          Some("color") => match as_color(value, candidate.modifier(), &ds.theme) {
            Some(v) => Compiled::Nodes(color(&v)),
            None => Compiled::Skip,
          },
          Some("percentage") => {
            if candidate.modifier().is_some() || !is_positive_integer(&value[..value.len().saturating_sub(1)]) {
              return Compiled::Skip;
            }
            Compiled::Nodes(position(value))
          }
          _ => {
            if candidate.modifier().is_some() {
              return Compiled::Skip;
            }
            Compiled::Nodes(position(value))
          }
        }
      }
      UtilityValue::Named { value, .. } => {
        if let Some(v) = resolve_theme_color(candidate, ds, &["--background-color", "--color"])
          && !v.is_empty()
        {
          return Compiled::Nodes(color(&v));
        }
        if candidate.modifier().is_some() {
          return Compiled::Skip;
        }
        match infer_data_type(value, &[DataType::Number, DataType::Percentage]) {
          Some(DataType::Number) => {
            if ds
              .theme
              .resolve(None, &["--spacing"], ThemeOptions::NONE)
              .is_none_or(|m| m.is_empty())
            {
              return Compiled::Skip;
            }
            if !is_valid_spacing_multiplier(value) {
              return Compiled::Skip;
            }
            Compiled::Nodes(position(&format!("--spacing({value})")))
          }
          Some(DataType::Percentage) => {
            if !is_positive_integer(&value[..value.len() - 1]) {
              return Compiled::Skip;
            }
            Compiled::Nodes(position(value))
          }
          _ => Compiled::Skip,
        }
      }
    }
  });
}

pub fn register(u: &mut Utilities) {
  static_decls(u, "bg-auto", &[("background-size", "auto")]);
  static_decls(u, "bg-cover", &[("background-size", "cover")]);
  static_decls(u, "bg-contain", &[("background-size", "contain")]);
  functional_utility(
    u,
    "bg-size",
    Desc::new(|v, _, _| {
      if v.is_empty() {
        None
      } else {
        Some(decls("background-size", v))
      }
    }),
  );

  static_decls(u, "bg-fixed", &[("background-attachment", "fixed")]);
  static_decls(u, "bg-local", &[("background-attachment", "local")]);
  static_decls(u, "bg-scroll", &[("background-attachment", "scroll")]);

  for (name, value) in [
    ("bg-top", "top"),
    ("bg-top-left", "left top"),
    ("bg-top-right", "right top"),
    ("bg-bottom", "bottom"),
    ("bg-bottom-left", "left bottom"),
    ("bg-bottom-right", "right bottom"),
    ("bg-left", "left"),
    ("bg-right", "right"),
    ("bg-center", "center"),
  ] {
    static_decls(u, name, &[("background-position", value)]);
  }
  functional_utility(
    u,
    "bg-position",
    Desc::new(|v, _, _| {
      if v.is_empty() {
        None
      } else {
        Some(decls("background-position", v))
      }
    }),
  );

  for (name, value) in [
    ("bg-repeat", "repeat"),
    ("bg-no-repeat", "no-repeat"),
    ("bg-repeat-x", "repeat-x"),
    ("bg-repeat-y", "repeat-y"),
    ("bg-repeat-round", "round"),
    ("bg-repeat-space", "space"),
  ] {
    static_decls(u, name, &[("background-repeat", value)]);
  }

  static_decls(u, "bg-none", &[("background-image", "none")]);

  u.functional("-bg-linear", |c, _| bg_linear(c, true));
  u.functional("bg-linear", |c, _| bg_linear(c, false));
  u.functional("-bg-conic", |c, _| bg_conic(c, true));
  u.functional("bg-conic", |c, _| bg_conic(c, false));

  u.functional("bg-radial", |candidate, _| match candidate.value() {
    None => {
      let method = interpolation(candidate.modifier());
      Compiled::Nodes(vec![
        decl("--tw-gradient-position", method),
        decl("background-image", "radial-gradient(var(--tw-gradient-stops))"),
      ])
    }
    Some(UtilityValue::Arbitrary { value, .. }) => {
      if candidate.modifier().is_some() {
        return Compiled::Skip;
      }
      Compiled::Nodes(vec![
        decl("--tw-gradient-position", value.clone()),
        decl(
          "background-image",
          format!("radial-gradient(var(--tw-gradient-stops,{value}))"),
        ),
      ])
    }
    _ => Compiled::Skip,
  });

  u.functional("bg", |candidate, ds| {
    let Some(value) = candidate.value() else {
      return Compiled::Skip;
    };
    match value {
      UtilityValue::Arbitrary { value, data_type: hint } => {
        let ty = data_type(
          value,
          hint,
          &[
            DataType::Image,
            DataType::Color,
            DataType::Percentage,
            DataType::Position,
            DataType::BgSize,
            DataType::Length,
            DataType::Url,
          ],
        );
        match ty.as_deref() {
          Some("percentage") | Some("position") => {
            if candidate.modifier().is_some() {
              return Compiled::Skip;
            }
            Compiled::Nodes(decls("background-position", value))
          }
          Some("bg-size") | Some("length") | Some("size") => {
            if candidate.modifier().is_some() {
              return Compiled::Skip;
            }
            Compiled::Nodes(decls("background-size", value))
          }
          Some("image") | Some("url") => {
            if candidate.modifier().is_some() {
              return Compiled::Skip;
            }
            Compiled::Nodes(decls("background-image", value))
          }
          _ => match as_color(value, candidate.modifier(), &ds.theme) {
            Some(v) => Compiled::Nodes(decls("background-color", &v)),
            None => Compiled::Skip,
          },
        }
      }
      UtilityValue::Named { value, .. } => {
        if let Some(v) = resolve_theme_color(candidate, ds, &["--background-color", "--color"])
          && !v.is_empty()
        {
          return Compiled::Nodes(decls("background-color", &v));
        }
        if candidate.modifier().is_some() {
          return Compiled::Skip;
        }
        if let Some(v) = ds
          .theme
          .resolve(Some(value), &["--background-image"], ThemeOptions::NONE)
          && !v.is_empty()
        {
          return Compiled::Nodes(decls("background-image", &v));
        }
        Compiled::Skip
      }
    }
  });

  const STOPS: &str = "var(--tw-gradient-via-stops, var(--tw-gradient-position), var(--tw-gradient-from) var(--tw-gradient-from-position), var(--tw-gradient-to) var(--tw-gradient-to-position))";
  gradient_stop_utility(
    u,
    "from",
    Rc::new(|v| {
      vec![
        gradient_stop_properties(),
        decl("--tw-sort", "--tw-gradient-from"),
        decl("--tw-gradient-from", v),
        decl("--tw-gradient-stops", STOPS),
      ]
    }),
    Rc::new(|v| vec![gradient_stop_properties(), decl("--tw-gradient-from-position", v)]),
  );
  static_decls(u, "via-none", &[("--tw-gradient-via-stops", "initial")]);
  gradient_stop_utility(
    u,
    "via",
    Rc::new(|v| {
      vec![
        gradient_stop_properties(),
        decl("--tw-sort", "--tw-gradient-via"),
        decl("--tw-gradient-via", v),
        decl(
          "--tw-gradient-via-stops",
          "var(--tw-gradient-position), var(--tw-gradient-from) var(--tw-gradient-from-position), var(--tw-gradient-via) var(--tw-gradient-via-position), var(--tw-gradient-to) var(--tw-gradient-to-position)",
        ),
        decl("--tw-gradient-stops", "var(--tw-gradient-via-stops)"),
      ]
    }),
    Rc::new(|v| vec![gradient_stop_properties(), decl("--tw-gradient-via-position", v)]),
  );
  gradient_stop_utility(
    u,
    "to",
    Rc::new(|v| {
      vec![
        gradient_stop_properties(),
        decl("--tw-sort", "--tw-gradient-to"),
        decl("--tw-gradient-to", v),
        decl("--tw-gradient-stops", STOPS),
      ]
    }),
    Rc::new(|v| vec![gradient_stop_properties(), decl("--tw-gradient-to-position", v)]),
  );

  static_decls(u, "mask-none", &[("mask-image", "none")]);

  u.functional("mask", |candidate, _| {
    let Some(UtilityValue::Arbitrary { value, data_type: hint }) = candidate.value() else {
      return Compiled::Skip;
    };
    if candidate.modifier().is_some() {
      return Compiled::Skip;
    }
    match data_type(
      value,
      hint,
      &[
        DataType::Image,
        DataType::Percentage,
        DataType::Position,
        DataType::BgSize,
        DataType::Length,
        DataType::Url,
      ],
    )
    .as_deref()
    {
      Some("percentage") | Some("position") => Compiled::Nodes(decls("mask-position", value)),
      Some("bg-size") | Some("length") | Some("size") => Compiled::Nodes(decls("mask-size", value)),
      _ => Compiled::Nodes(decls("mask-image", value)),
    }
  });

  for (name, value) in [
    ("mask-add", "add"),
    ("mask-subtract", "subtract"),
    ("mask-intersect", "intersect"),
    ("mask-exclude", "exclude"),
  ] {
    static_decls(u, name, &[("mask-composite", value)]);
  }
  for (name, value) in [
    ("mask-alpha", "alpha"),
    ("mask-luminance", "luminance"),
    ("mask-match", "match-source"),
  ] {
    static_decls(u, name, &[("mask-mode", value)]);
  }
  static_decls(u, "mask-type-alpha", &[("mask-type", "alpha")]);
  static_decls(u, "mask-type-luminance", &[("mask-type", "luminance")]);

  static_decls(u, "mask-auto", &[("mask-size", "auto")]);
  static_decls(u, "mask-cover", &[("mask-size", "cover")]);
  static_decls(u, "mask-contain", &[("mask-size", "contain")]);
  functional_utility(
    u,
    "mask-size",
    Desc::new(|v, _, _| {
      if v.is_empty() {
        None
      } else {
        Some(decls("mask-size", v))
      }
    }),
  );

  for (name, value) in [
    ("mask-top", "top"),
    ("mask-top-left", "left top"),
    ("mask-top-right", "right top"),
    ("mask-bottom", "bottom"),
    ("mask-bottom-left", "left bottom"),
    ("mask-bottom-right", "right bottom"),
    ("mask-left", "left"),
    ("mask-right", "right"),
    ("mask-center", "center"),
  ] {
    static_decls(u, name, &[("mask-position", value)]);
  }
  functional_utility(
    u,
    "mask-position",
    Desc::new(|v, _, _| {
      if v.is_empty() {
        None
      } else {
        Some(decls("mask-position", v))
      }
    }),
  );

  for (name, value) in [
    ("mask-repeat", "repeat"),
    ("mask-no-repeat", "no-repeat"),
    ("mask-repeat-x", "repeat-x"),
    ("mask-repeat-y", "repeat-y"),
    ("mask-repeat-round", "round"),
    ("mask-repeat-space", "space"),
  ] {
    static_decls(u, name, &[("mask-repeat", value)]);
  }

  for (name, value) in [
    ("mask-clip-border", "border-box"),
    ("mask-clip-padding", "padding-box"),
    ("mask-clip-content", "content-box"),
    ("mask-clip-fill", "fill-box"),
    ("mask-clip-stroke", "stroke-box"),
    ("mask-clip-view", "view-box"),
    ("mask-no-clip", "no-clip"),
  ] {
    static_decls(u, name, &[("mask-clip", value)]);
  }
  for (name, value) in [
    ("mask-origin-border", "border-box"),
    ("mask-origin-padding", "padding-box"),
    ("mask-origin-content", "content-box"),
    ("mask-origin-fill", "fill-box"),
    ("mask-origin-stroke", "stroke-box"),
    ("mask-origin-view", "view-box"),
  ] {
    static_decls(u, name, &[("mask-origin", value)]);
  }
}
