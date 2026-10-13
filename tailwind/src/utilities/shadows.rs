//! Text and box shadows, rings and `@container`, in Tailwind's registration order.

use crate::ast::{Node, at_root, decl};
use crate::candidate::{Candidate, Modifier, UtilityValue};
use crate::design_system::DesignSystem;
use crate::theme::{Theme, ThemeOptions};
use crate::utilities::effects::{alpha_replaced_shadow, decl_opt};
use crate::utilities::helpers::{Item, d, resolve_theme_color, static_utility};
use crate::utilities::{Compiled, Utilities, as_color, property, with_alpha};
use crate::utils::infer::{DataType, infer_data_type, is_positive_integer};

fn type_of(value: &str, hint: &Option<String>, types: &[DataType]) -> Option<String> {
  match hint {
    Some(h) => Some(h.clone()),
    None => infer_data_type(value, types).map(|t| t.name().to_owned()),
  }
}

fn shadow_alpha(modifier: Option<&Modifier>) -> Option<String> {
  match modifier {
    Some(Modifier::Arbitrary(v)) => Some(v.clone()),
    Some(Modifier::Named(v)) if is_positive_integer(v) => Some(format!("{v}%")),
    _ => None,
  }
}

fn text_shadow_properties() -> Node {
  at_root(vec![
    property("--tw-text-shadow-color", None, None),
    property("--tw-text-shadow-alpha", Some("100%"), Some("<percentage>")),
  ])
}

const BOX_SHADOW: &str = "var(--tw-inset-shadow), var(--tw-inset-ring-shadow), var(--tw-ring-offset-shadow), var(--tw-ring-shadow), var(--tw-shadow)";
const NULL_SHADOW: &str = "0 0 #0000";

fn box_shadow_properties() -> Node {
  at_root(vec![
    property("--tw-shadow", Some(NULL_SHADOW), None),
    property("--tw-shadow-color", None, None),
    property("--tw-shadow-alpha", Some("100%"), Some("<percentage>")),
    property("--tw-inset-shadow", Some(NULL_SHADOW), None),
    property("--tw-inset-shadow-color", None, None),
    property("--tw-inset-shadow-alpha", Some("100%"), Some("<percentage>")),
    property("--tw-ring-color", None, None),
    property("--tw-ring-shadow", Some(NULL_SHADOW), None),
    property("--tw-inset-ring-color", None, None),
    property("--tw-inset-ring-shadow", Some(NULL_SHADOW), None),
    property("--tw-ring-inset", None, None),
    property("--tw-ring-offset-width", Some("0px"), Some("<length>")),
    property("--tw-ring-offset-color", Some("#fff"), None),
    property("--tw-ring-offset-shadow", Some(NULL_SHADOW), None),
  ])
}

/// What `shadow` and `inset-shadow` share: a size from the theme or an arbitrary value with its
/// colors routed through `--tw-*-color`. A color sets that variable.
struct ShadowKind {
  var: &'static str,
  color_var: &'static str,
  alpha_var: &'static str,
  theme_default: &'static str,
  theme_prefix: &'static str,
  arbitrary_prefix: &'static str,
  none_value: &'static str,
}

fn box_shadow(candidate: &Candidate, ds: &DesignSystem, k: &ShadowKind) -> Compiled {
  let alpha = shadow_alpha(candidate.modifier());
  let inject = |color: &str| format!("var({}, {color})", k.color_var);
  let sized = |value: &str, prefix: &str| -> Vec<Node> {
    let mut nodes = vec![box_shadow_properties(), decl_opt(k.alpha_var, alpha.clone())];
    nodes.extend(alpha_replaced_shadow(k.var, value, alpha.as_deref(), &inject, prefix));
    nodes.push(decl("box-shadow", BOX_SHADOW));
    nodes
  };
  let colored = |value: &str| {
    vec![
      box_shadow_properties(),
      decl(k.color_var, with_alpha(value, &format!("var({})", k.alpha_var))),
    ]
  };

  let Some(value) = candidate.value() else {
    return match ds.theme.get(&[k.theme_default]) {
      Some(v) => Compiled::Nodes(sized(&v, "")),
      None => Compiled::Skip,
    };
  };

  match value {
    UtilityValue::Arbitrary { value, data_type } => {
      if type_of(value, data_type, &[DataType::Color]).as_deref() == Some("color") {
        return match as_color(value, candidate.modifier(), &ds.theme) {
          Some(v) => Compiled::Nodes(colored(&v)),
          None => Compiled::Skip,
        };
      }
      Compiled::Nodes(sized(value, k.arbitrary_prefix))
    }
    UtilityValue::Named { value: name, .. } => {
      match name.as_str() {
        "none" => {
          if candidate.modifier().is_some() {
            return Compiled::Skip;
          }
          return Compiled::Nodes(vec![
            box_shadow_properties(),
            decl(k.var, k.none_value),
            decl("box-shadow", BOX_SHADOW),
          ]);
        }
        "inherit" => {
          if candidate.modifier().is_some() {
            return Compiled::Skip;
          }
          return Compiled::Nodes(vec![box_shadow_properties(), decl(k.color_var, "inherit")]);
        }
        _ => {}
      }
      if let Some(v) = ds.theme.get(&[&format!("{}-{name}", k.theme_prefix)])
        && !v.is_empty()
      {
        return Compiled::Nodes(sized(&v, ""));
      }
      match resolve_theme_color(candidate, ds, &["--box-shadow-color", "--color"]) {
        Some(v) if !v.is_empty() => Compiled::Nodes(colored(&v)),
        _ => Compiled::Skip,
      }
    }
  }
}

fn ring_like(
  candidate: &Candidate,
  ds: &DesignSystem,
  shadow_var: &'static str,
  color_var: &'static str,
  default_width: Option<String>,
  shadow: &dyn Fn(&str) -> String,
) -> Compiled {
  let widths = |value: &str| {
    vec![
      box_shadow_properties(),
      decl(shadow_var, shadow(value)),
      decl("box-shadow", BOX_SHADOW),
    ]
  };
  let Some(value) = candidate.value() else {
    if candidate.modifier().is_some() {
      return Compiled::Skip;
    }
    return Compiled::Nodes(widths(&default_width.unwrap_or_else(|| {
      ds.theme
        .get(&["--default-ring-width"])
        .unwrap_or_else(|| "1px".to_owned())
    })));
  };
  match value {
    UtilityValue::Arbitrary { value, data_type } => {
      match type_of(value, data_type, &[DataType::Color, DataType::Length]).as_deref() {
        Some("length") => {
          if candidate.modifier().is_some() {
            return Compiled::Skip;
          }
          Compiled::Nodes(widths(value))
        }
        _ => match as_color(value, candidate.modifier(), &ds.theme) {
          Some(v) => Compiled::Nodes(vec![decl(color_var, v)]),
          None => Compiled::Skip,
        },
      }
    }
    UtilityValue::Named { value, .. } => {
      if let Some(v) = resolve_theme_color(candidate, ds, &["--ring-color", "--color"])
        && !v.is_empty()
      {
        return Compiled::Nodes(vec![decl(color_var, v)]);
      }
      if candidate.modifier().is_some() {
        return Compiled::Skip;
      }
      let mut width = ds.theme.resolve(Some(value), &["--ring-width"], ThemeOptions::NONE);
      if width.is_none() && is_positive_integer(value) {
        width = Some(format!("{value}px"));
      }
      match width {
        Some(w) if !w.is_empty() => Compiled::Nodes(widths(&w)),
        _ => Compiled::Skip,
      }
    }
  }
}

pub fn register(u: &mut Utilities, theme: &Theme) {
  static_utility(
    u,
    "text-shadow-initial",
    vec![
      Item::Node(text_shadow_properties),
      d("--tw-text-shadow-color", "initial"),
    ],
  );

  u.functional("text-shadow", |candidate, ds| {
    let alpha = shadow_alpha(candidate.modifier());
    let inject = |color: &str| format!("var(--tw-text-shadow-color, {color})");
    let sized = |value: &str| -> Vec<Node> {
      let mut nodes = vec![
        text_shadow_properties(),
        decl_opt("--tw-text-shadow-alpha", alpha.clone()),
      ];
      nodes.extend(alpha_replaced_shadow(
        "text-shadow",
        value,
        alpha.as_deref(),
        &inject,
        "",
      ));
      nodes
    };
    let colored = |value: &str| {
      vec![
        text_shadow_properties(),
        decl(
          "--tw-text-shadow-color",
          with_alpha(value, "var(--tw-text-shadow-alpha)"),
        ),
      ]
    };

    let Some(value) = candidate.value() else {
      return match ds.theme.get(&["--text-shadow"]) {
        Some(v) => Compiled::Nodes(sized(&v)),
        None => Compiled::Skip,
      };
    };
    match value {
      UtilityValue::Arbitrary { value, data_type } => {
        if type_of(value, data_type, &[DataType::Color]).as_deref() == Some("color") {
          return match as_color(value, candidate.modifier(), &ds.theme) {
            Some(v) => Compiled::Nodes(colored(&v)),
            None => Compiled::Skip,
          };
        }
        Compiled::Nodes(sized(value))
      }
      UtilityValue::Named { value: name, .. } => {
        match name.as_str() {
          "none" => {
            if candidate.modifier().is_some() {
              return Compiled::Skip;
            }
            return Compiled::Nodes(vec![text_shadow_properties(), decl("text-shadow", "none")]);
          }
          "inherit" => {
            if candidate.modifier().is_some() {
              return Compiled::Skip;
            }
            return Compiled::Nodes(vec![
              text_shadow_properties(),
              decl("--tw-text-shadow-color", "inherit"),
            ]);
          }
          _ => {}
        }
        if let Some(v) = ds.theme.get(&[&format!("--text-shadow-{name}")])
          && !v.is_empty()
        {
          return Compiled::Nodes(sized(&v));
        }
        match resolve_theme_color(candidate, ds, &["--text-shadow-color", "--color"]) {
          Some(v) if !v.is_empty() => Compiled::Nodes(colored(&v)),
          _ => Compiled::Skip,
        }
      }
    }
  });

  static_utility(
    u,
    "shadow-initial",
    vec![Item::Node(box_shadow_properties), d("--tw-shadow-color", "initial")],
  );
  u.functional("shadow", |c, ds| {
    box_shadow(
      c,
      ds,
      &ShadowKind {
        var: "--tw-shadow",
        color_var: "--tw-shadow-color",
        alpha_var: "--tw-shadow-alpha",
        theme_default: "--shadow",
        theme_prefix: "--shadow",
        arbitrary_prefix: "",
        none_value: NULL_SHADOW,
      },
    )
  });

  static_utility(
    u,
    "inset-shadow-initial",
    vec![
      Item::Node(box_shadow_properties),
      d("--tw-inset-shadow-color", "initial"),
    ],
  );
  u.functional("inset-shadow", |c, ds| {
    box_shadow(
      c,
      ds,
      &ShadowKind {
        var: "--tw-inset-shadow",
        color_var: "--tw-inset-shadow-color",
        alpha_var: "--tw-inset-shadow-alpha",
        theme_default: "--inset-shadow",
        theme_prefix: "--inset-shadow",
        arbitrary_prefix: "inset",
        none_value: "inset 0 0 #0000",
      },
    )
  });

  static_utility(
    u,
    "ring-inset",
    vec![Item::Node(box_shadow_properties), d("--tw-ring-inset", "inset")],
  );

  let default_ring_color = theme
    .get(&["--default-ring-color"])
    .unwrap_or_else(|| "currentcolor".to_owned());
  u.functional("ring", move |c, ds| {
    let color = default_ring_color.clone();
    ring_like(c, ds, "--tw-ring-shadow", "--tw-ring-color", None, &move |value| {
      format!("var(--tw-ring-inset,) 0 0 0 calc({value} + var(--tw-ring-offset-width)) var(--tw-ring-color, {color})")
    })
  });

  u.functional("inset-ring", |c, ds| {
    ring_like(
      c,
      ds,
      "--tw-inset-ring-shadow",
      "--tw-inset-ring-color",
      Some("1px".to_owned()),
      &|value| format!("inset 0 0 0 {value} var(--tw-inset-ring-color, currentcolor)"),
    )
  });

  const RING_OFFSET: &str = "var(--tw-ring-inset,) 0 0 0 var(--tw-ring-offset-width) var(--tw-ring-offset-color)";
  u.functional("ring-offset", |candidate, ds| {
    let Some(value) = candidate.value() else {
      return Compiled::Skip;
    };
    match value {
      UtilityValue::Arbitrary { value, data_type } => {
        match type_of(value, data_type, &[DataType::Color, DataType::Length]).as_deref() {
          Some("length") => {
            if candidate.modifier().is_some() {
              return Compiled::Skip;
            }
            Compiled::Nodes(vec![
              decl("--tw-ring-offset-width", value.clone()),
              decl("--tw-ring-offset-shadow", RING_OFFSET),
            ])
          }
          _ => match as_color(value, candidate.modifier(), &ds.theme) {
            Some(v) => Compiled::Nodes(vec![decl("--tw-ring-offset-color", v)]),
            None => Compiled::Skip,
          },
        }
      }
      UtilityValue::Named { value, .. } => {
        if let Some(v) = ds
          .theme
          .resolve(Some(value), &["--ring-offset-width"], ThemeOptions::NONE)
          && !v.is_empty()
        {
          if candidate.modifier().is_some() {
            return Compiled::Skip;
          }
          return Compiled::Nodes(vec![
            decl("--tw-ring-offset-width", v),
            decl("--tw-ring-offset-shadow", RING_OFFSET),
          ]);
        }
        if is_positive_integer(value) {
          if candidate.modifier().is_some() {
            return Compiled::Skip;
          }
          return Compiled::Nodes(vec![
            decl("--tw-ring-offset-width", format!("{value}px")),
            decl("--tw-ring-offset-shadow", RING_OFFSET),
          ]);
        }
        match resolve_theme_color(candidate, ds, &["--ring-offset-color", "--color"]) {
          Some(v) if !v.is_empty() => Compiled::Nodes(vec![decl("--tw-ring-offset-color", v)]),
          _ => Compiled::Skip,
        }
      }
    }
  });

  u.functional("@container", |candidate, _| {
    let value = match candidate.value() {
      None => "inline-size".to_owned(),
      Some(UtilityValue::Arbitrary { value, .. }) => value.clone(),
      Some(UtilityValue::Named { value, .. }) if value == "normal" || value == "size" => value.clone(),
      _ => return Compiled::Skip,
    };
    match candidate.modifier() {
      Some(m) => Compiled::Nodes(vec![decl("container-type", value), decl("container-name", m.value())]),
      None => Compiled::Nodes(vec![decl("container-type", value)]),
    }
  });
}
