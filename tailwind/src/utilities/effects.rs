//! Typography details, filters, transitions, outlines, opacity and `text-*`, in Tailwind's
//! registration order.

use crate::ast::{Node, at_root, at_rule, decl, rule, style_rule};
use crate::candidate::{Modifier, UtilityValue};
use crate::design_system::DesignSystem;
use crate::theme::{Theme, ThemeOptions};
use crate::utilities::helpers::{
  Desc, Item, color_utility, d, functional_utility, resolve_theme_color, spacing_utility, static_decls, static_utility,
};
use crate::utilities::{Compiled, Utilities, as_color, property, replace_alpha, with_alpha};
use crate::utils::infer::{
  DataType, infer_data_type, is_positive_integer, is_valid_opacity_value, is_valid_spacing_multiplier,
};
use crate::utils::segment::segment;
use crate::utils::shadow::replace_shadow_colors;

fn decls(property: &'static str, value: &str) -> Vec<Node> {
  vec![decl(property, value)]
}

pub(super) fn decl_opt(property: &str, value: Option<String>) -> Node {
  Node::Declaration {
    property: property.to_owned(),
    value,
    important: false,
  }
}

fn type_of(value: &str, hint: &Option<String>, types: &[DataType]) -> Option<String> {
  match hint {
    Some(h) => Some(h.clone()),
    None => infer_data_type(value, types).map(|t| t.name().to_owned()),
  }
}

/// `alphaReplacedShadowProperties`: a shadow with its colors routed through `inject`. When the
/// alpha cannot be resolved statically an `@supports` copy uses relative colors.
pub(super) fn alpha_replaced_shadow(
  property: &str,
  value: &str,
  alpha: Option<&str>,
  inject: &dyn Fn(&str) -> String,
  prefix: &str,
) -> Vec<Node> {
  let mut requires_fallback = false;
  let replaced = replace_shadow_colors(value, &mut |color| {
    let Some(alpha) = alpha else { return inject(color) };
    if color.starts_with("current") {
      return inject(&with_alpha(color, alpha));
    }
    if color.starts_with("var(") || alpha.starts_with("var(") {
      requires_fallback = true;
    }
    inject(&replace_alpha(color, alpha))
  });

  let apply_prefix = |x: &str| -> String {
    if prefix.is_empty() {
      return x.to_owned();
    }
    segment(x, b',')
      .iter()
      .map(|v| format!("{} {}", prefix.trim(), v.trim()))
      .collect::<Vec<_>>()
      .join(", ")
  };

  if requires_fallback {
    let plain = replace_shadow_colors(value, &mut |c| inject(c));
    vec![
      decl(property, apply_prefix(&plain)),
      rule(
        "@supports (color: lab(from red l a b))",
        vec![decl(property, apply_prefix(&replaced))],
      ),
    ]
  } else {
    vec![decl(property, apply_prefix(&replaced))]
  }
}

/// `alphaReplacedDropShadowProperties`: the same for each `drop-shadow()` of a filter.
pub(super) fn alpha_replaced_drop_shadow(
  property: &str,
  value: &str,
  alpha: Option<&str>,
  inject: &dyn Fn(&str) -> String,
  prefix: &str,
) -> Vec<Node> {
  let mut requires_fallback = false;
  let replaced = segment(value, b',')
    .iter()
    .map(|v| {
      let shadow = replace_shadow_colors(v, &mut |color| {
        let Some(alpha) = alpha else { return inject(color) };
        if color.starts_with("current") {
          return inject(&with_alpha(color, alpha));
        }
        if color.starts_with("var(") || alpha.starts_with("var(") {
          requires_fallback = true;
        }
        inject(&replace_alpha(color, alpha))
      });
      format!("drop-shadow({shadow})")
    })
    .collect::<Vec<_>>()
    .join(" ");

  if requires_fallback {
    let plain = segment(value, b',')
      .iter()
      .map(|v| format!("drop-shadow({})", replace_shadow_colors(v, &mut |c| inject(c))))
      .collect::<Vec<_>>()
      .join(" ");
    vec![
      decl(property, format!("{prefix}{plain}")),
      rule(
        "@supports (color: lab(from red l a b))",
        vec![decl(property, format!("{prefix}{replaced}"))],
      ),
    ]
  } else {
    vec![decl(property, format!("{prefix}{replaced}"))]
  }
}

const FILTER: &str = "var(--tw-blur,) var(--tw-brightness,) var(--tw-contrast,) var(--tw-grayscale,) var(--tw-hue-rotate,) var(--tw-invert,) var(--tw-saturate,) var(--tw-sepia,) var(--tw-drop-shadow,)";
const BACKDROP: &str = "var(--tw-backdrop-blur,) var(--tw-backdrop-brightness,) var(--tw-backdrop-contrast,) var(--tw-backdrop-grayscale,) var(--tw-backdrop-hue-rotate,) var(--tw-backdrop-invert,) var(--tw-backdrop-opacity,) var(--tw-backdrop-saturate,) var(--tw-backdrop-sepia,)";

fn filter_properties() -> Node {
  at_root(vec![
    property("--tw-blur", None, None),
    property("--tw-brightness", None, None),
    property("--tw-contrast", None, None),
    property("--tw-grayscale", None, None),
    property("--tw-hue-rotate", None, None),
    property("--tw-invert", None, None),
    property("--tw-opacity", None, None),
    property("--tw-saturate", None, None),
    property("--tw-sepia", None, None),
    property("--tw-drop-shadow", None, None),
    property("--tw-drop-shadow-color", None, None),
    property("--tw-drop-shadow-alpha", Some("100%"), Some("<percentage>")),
    property("--tw-drop-shadow-size", None, None),
  ])
}

fn backdrop_filter_properties() -> Node {
  at_root(vec![
    property("--tw-backdrop-blur", None, None),
    property("--tw-backdrop-brightness", None, None),
    property("--tw-backdrop-contrast", None, None),
    property("--tw-backdrop-grayscale", None, None),
    property("--tw-backdrop-hue-rotate", None, None),
    property("--tw-backdrop-invert", None, None),
    property("--tw-backdrop-opacity", None, None),
    property("--tw-backdrop-saturate", None, None),
    property("--tw-backdrop-sepia", None, None),
  ])
}

fn filter_nodes(var: &str, value: String) -> Vec<Node> {
  vec![filter_properties(), decl(var, value), decl("filter", FILTER)]
}

fn backdrop_nodes(var: &str, value: String) -> Vec<Node> {
  vec![
    backdrop_filter_properties(),
    decl(var, value),
    decl("-webkit-backdrop-filter", BACKDROP),
    decl("backdrop-filter", BACKDROP),
  ]
}

fn percent(v: &str, _: Option<&str>, _: &DesignSystem) -> Option<String> {
  if is_positive_integer(v) {
    Some(format!("{v}%"))
  } else {
    None
  }
}

fn shadow_alpha(modifier: Option<&Modifier>) -> Option<String> {
  match modifier {
    Some(Modifier::Arbitrary(v)) => Some(v.clone()),
    Some(Modifier::Named(v)) if is_positive_integer(v) => Some(format!("{v}%")),
    _ => None,
  }
}

fn leading_modifier(modifier: &Modifier, ds: &DesignSystem) -> Result<Option<String>, ()> {
  let mut value = match modifier {
    Modifier::Arbitrary(v) => Some(v.clone()),
    Modifier::Named(v) => ds.theme.resolve(Some(v), &["--leading"], ThemeOptions::NONE),
  }
  .filter(|v| !v.is_empty());
  if value.is_none() && is_valid_spacing_multiplier(modifier.value()) {
    if ds
      .theme
      .resolve(None, &["--spacing"], ThemeOptions::NONE)
      .is_none_or(|m| m.is_empty())
    {
      return Err(());
    }
    value = Some(format!("--spacing({})", modifier.value()));
  }
  if value.is_none() && modifier.value() == "none" {
    value = Some("1".to_owned());
  }
  Ok(value)
}

pub fn register(u: &mut Utilities, theme: &Theme) {
  functional_utility(
    u,
    "font-features",
    Desc::new(|v, _, _| Some(decls("font-feature-settings", v))).keys(&[]),
  );

  static_decls(u, "uppercase", &[("text-transform", "uppercase")]);
  static_decls(u, "lowercase", &[("text-transform", "lowercase")]);
  static_decls(u, "capitalize", &[("text-transform", "capitalize")]);
  static_decls(u, "normal-case", &[("text-transform", "none")]);
  static_decls(u, "italic", &[("font-style", "italic")]);
  static_decls(u, "not-italic", &[("font-style", "normal")]);
  static_decls(u, "underline", &[("text-decoration-line", "underline")]);
  static_decls(u, "overline", &[("text-decoration-line", "overline")]);
  static_decls(u, "line-through", &[("text-decoration-line", "line-through")]);
  static_decls(u, "no-underline", &[("text-decoration-line", "none")]);

  for value in [
    "normal",
    "ultra-condensed",
    "extra-condensed",
    "condensed",
    "semi-condensed",
    "semi-expanded",
    "expanded",
    "extra-expanded",
    "ultra-expanded",
  ] {
    static_decls(u, &format!("font-stretch-{value}"), &[("font-stretch", value)]);
  }
  functional_utility(
    u,
    "font-stretch",
    Desc::new(|v, _, _| Some(decls("font-stretch", v))).bare(|v, _, _| {
      let number = v.strip_suffix('%')?;
      let n = crate::utils::js::to_number(number);
      if !crate::utils::js::is_integer(n) || n < 0.0 {
        return None;
      }
      if !(50.0..=200.0).contains(&n) {
        return None;
      }
      Some(v.to_owned())
    }),
  );

  color_utility(u, "placeholder", &["--placeholder-color", "--color"], |v, _| {
    Some(vec![style_rule(
      "&::placeholder",
      vec![decl("--tw-sort", "placeholder-color"), decl("color", v)],
    )])
  });

  for value in ["solid", "double", "dotted", "dashed", "wavy"] {
    static_decls(u, &format!("decoration-{value}"), &[("text-decoration-style", value)]);
  }
  static_decls(u, "decoration-auto", &[("text-decoration-thickness", "auto")]);
  static_decls(u, "decoration-from-font", &[("text-decoration-thickness", "from-font")]);

  u.functional("decoration", |candidate, ds| {
    let Some(value) = candidate.value() else {
      return Compiled::Skip;
    };
    match value {
      UtilityValue::Arbitrary { value, data_type } => match type_of(
        value,
        data_type,
        &[DataType::Color, DataType::Length, DataType::Percentage],
      )
      .as_deref()
      {
        Some("length") | Some("percentage") => {
          if candidate.modifier().is_some() {
            return Compiled::Skip;
          }
          Compiled::Nodes(decls("text-decoration-thickness", value))
        }
        _ => match as_color(value, candidate.modifier(), &ds.theme) {
          Some(v) => Compiled::Nodes(decls("text-decoration-color", &v)),
          None => Compiled::Skip,
        },
      },
      UtilityValue::Named { value, .. } => {
        if let Some(v) = ds
          .theme
          .resolve(Some(value), &["--text-decoration-thickness"], ThemeOptions::NONE)
          && !v.is_empty()
        {
          if candidate.modifier().is_some() {
            return Compiled::Skip;
          }
          return Compiled::Nodes(decls("text-decoration-thickness", &v));
        }
        if is_positive_integer(value) {
          if candidate.modifier().is_some() {
            return Compiled::Skip;
          }
          return Compiled::Nodes(vec![decl("text-decoration-thickness", format!("{value}px"))]);
        }
        match resolve_theme_color(candidate, ds, &["--text-decoration-color", "--color"]) {
          Some(v) if !v.is_empty() => Compiled::Nodes(decls("text-decoration-color", &v)),
          _ => Compiled::Skip,
        }
      }
    }
  });

  functional_utility(
    u,
    "animate",
    Desc::new(|v, _, _| Some(decls("animation", v)))
      .keys(&["--animate"])
      .statics(vec![("none", decls("animation", "none"))]),
  );

  u.functional("filter", |candidate, _| {
    if candidate.modifier().is_some() {
      return Compiled::Skip;
    }
    match candidate.value() {
      None => Compiled::Nodes(vec![filter_properties(), decl("filter", FILTER)]),
      Some(UtilityValue::Arbitrary { value, .. }) => Compiled::Nodes(decls("filter", value)),
      Some(UtilityValue::Named { value, .. }) if value == "none" => Compiled::Nodes(decls("filter", "none")),
      _ => Compiled::Skip,
    }
  });

  u.functional("backdrop-filter", |candidate, _| {
    if candidate.modifier().is_some() {
      return Compiled::Skip;
    }
    match candidate.value() {
      None => Compiled::Nodes(vec![
        backdrop_filter_properties(),
        decl("-webkit-backdrop-filter", BACKDROP),
        decl("backdrop-filter", BACKDROP),
      ]),
      Some(UtilityValue::Arbitrary { value, .. }) => Compiled::Nodes(vec![
        decl("-webkit-backdrop-filter", value.clone()),
        decl("backdrop-filter", value.clone()),
      ]),
      Some(UtilityValue::Named { value, .. }) if value == "none" => Compiled::Nodes(vec![
        decl("-webkit-backdrop-filter", "none"),
        decl("backdrop-filter", "none"),
      ]),
      _ => Compiled::Skip,
    }
  });

  functional_utility(
    u,
    "blur",
    Desc::new(|v, _, _| Some(filter_nodes("--tw-blur", format!("blur({v})"))))
      .keys(&["--blur"])
      .statics(vec![("none", filter_nodes("--tw-blur", " ".to_owned()))]),
  );
  functional_utility(
    u,
    "backdrop-blur",
    Desc::new(|v, _, _| Some(backdrop_nodes("--tw-backdrop-blur", format!("blur({v})"))))
      .keys(&["--backdrop-blur", "--blur"])
      .statics(vec![("none", backdrop_nodes("--tw-backdrop-blur", " ".to_owned()))]),
  );

  functional_utility(
    u,
    "brightness",
    Desc::new(|v, _, _| Some(filter_nodes("--tw-brightness", format!("brightness({v})"))))
      .keys(&["--brightness"])
      .bare(percent),
  );
  functional_utility(
    u,
    "backdrop-brightness",
    Desc::new(|v, _, _| Some(backdrop_nodes("--tw-backdrop-brightness", format!("brightness({v})"))))
      .keys(&["--backdrop-brightness", "--brightness"])
      .bare(percent),
  );
  functional_utility(
    u,
    "contrast",
    Desc::new(|v, _, _| Some(filter_nodes("--tw-contrast", format!("contrast({v})"))))
      .keys(&["--contrast"])
      .bare(percent),
  );
  functional_utility(
    u,
    "backdrop-contrast",
    Desc::new(|v, _, _| Some(backdrop_nodes("--tw-backdrop-contrast", format!("contrast({v})"))))
      .keys(&["--backdrop-contrast", "--contrast"])
      .bare(percent),
  );
  functional_utility(
    u,
    "grayscale",
    Desc::new(|v, _, _| Some(filter_nodes("--tw-grayscale", format!("grayscale({v})"))))
      .keys(&["--grayscale"])
      .bare(percent)
      .default_value("100%"),
  );
  functional_utility(
    u,
    "backdrop-grayscale",
    Desc::new(|v, _, _| Some(backdrop_nodes("--tw-backdrop-grayscale", format!("grayscale({v})"))))
      .keys(&["--backdrop-grayscale", "--grayscale"])
      .bare(percent)
      .default_value("100%"),
  );
  let deg = |v: &str, _: Option<&str>, _: &DesignSystem| {
    if is_positive_integer(v) {
      Some(format!("{v}deg"))
    } else {
      None
    }
  };
  functional_utility(
    u,
    "hue-rotate",
    Desc::new(|v, _, _| Some(filter_nodes("--tw-hue-rotate", format!("hue-rotate({v})"))))
      .negative()
      .keys(&["--hue-rotate"])
      .bare(deg),
  );
  functional_utility(
    u,
    "backdrop-hue-rotate",
    Desc::new(|v, _, _| Some(backdrop_nodes("--tw-backdrop-hue-rotate", format!("hue-rotate({v})"))))
      .negative()
      .keys(&["--backdrop-hue-rotate", "--hue-rotate"])
      .bare(deg),
  );
  functional_utility(
    u,
    "invert",
    Desc::new(|v, _, _| Some(filter_nodes("--tw-invert", format!("invert({v})"))))
      .keys(&["--invert"])
      .bare(percent)
      .default_value("100%"),
  );
  functional_utility(
    u,
    "backdrop-invert",
    Desc::new(|v, _, _| Some(backdrop_nodes("--tw-backdrop-invert", format!("invert({v})"))))
      .keys(&["--backdrop-invert", "--invert"])
      .bare(percent)
      .default_value("100%"),
  );
  functional_utility(
    u,
    "saturate",
    Desc::new(|v, _, _| Some(filter_nodes("--tw-saturate", format!("saturate({v})"))))
      .keys(&["--saturate"])
      .bare(percent),
  );
  functional_utility(
    u,
    "backdrop-saturate",
    Desc::new(|v, _, _| Some(backdrop_nodes("--tw-backdrop-saturate", format!("saturate({v})"))))
      .keys(&["--backdrop-saturate", "--saturate"])
      .bare(percent),
  );
  functional_utility(
    u,
    "sepia",
    Desc::new(|v, _, _| Some(filter_nodes("--tw-sepia", format!("sepia({v})"))))
      .keys(&["--sepia"])
      .bare(percent)
      .default_value("100%"),
  );
  functional_utility(
    u,
    "backdrop-sepia",
    Desc::new(|v, _, _| Some(backdrop_nodes("--tw-backdrop-sepia", format!("sepia({v})"))))
      .keys(&["--backdrop-sepia", "--sepia"])
      .bare(percent)
      .default_value("100%"),
  );

  static_utility(
    u,
    "drop-shadow-none",
    vec![
      Item::Node(filter_properties),
      d("--tw-drop-shadow", " "),
      d("filter", FILTER),
    ],
  );

  u.functional("drop-shadow", |candidate, ds| {
    let alpha = shadow_alpha(candidate.modifier());
    let inject = |color: &str| format!("var(--tw-drop-shadow-color, {color})");
    let drop_shadows = |resolved: &str| {
      segment(resolved, b',')
        .iter()
        .map(|v| format!("drop-shadow({v})"))
        .collect::<Vec<_>>()
        .join(" ")
    };

    let Some(value) = candidate.value() else {
      let (Some(value), Some(resolved)) = (
        ds.theme.get(&["--drop-shadow"]),
        ds.theme.resolve(None, &["--drop-shadow"], ThemeOptions::NONE),
      ) else {
        return Compiled::Skip;
      };
      let mut nodes = vec![filter_properties(), decl_opt("--tw-drop-shadow-alpha", alpha.clone())];
      nodes.extend(alpha_replaced_drop_shadow(
        "--tw-drop-shadow-size",
        &value,
        alpha.as_deref(),
        &inject,
        "",
      ));
      nodes.push(decl("--tw-drop-shadow", drop_shadows(&resolved)));
      nodes.push(decl("filter", FILTER));
      return Compiled::Nodes(nodes);
    };

    match value {
      UtilityValue::Arbitrary { value, data_type } => {
        if type_of(value, data_type, &[DataType::Color]).as_deref() == Some("color") {
          let Some(color) = as_color(value, candidate.modifier(), &ds.theme) else {
            return Compiled::Skip;
          };
          return Compiled::Nodes(vec![
            filter_properties(),
            decl(
              "--tw-drop-shadow-color",
              with_alpha(&color, "var(--tw-drop-shadow-alpha)"),
            ),
            decl("--tw-drop-shadow", "var(--tw-drop-shadow-size)"),
          ]);
        }
        if candidate.modifier().is_some() && alpha.is_none() {
          return Compiled::Skip;
        }
        let mut nodes = vec![filter_properties(), decl_opt("--tw-drop-shadow-alpha", alpha.clone())];
        nodes.extend(alpha_replaced_drop_shadow(
          "--tw-drop-shadow-size",
          value,
          alpha.as_deref(),
          &inject,
          "",
        ));
        nodes.push(decl("--tw-drop-shadow", "var(--tw-drop-shadow-size)"));
        nodes.push(decl("filter", FILTER));
        Compiled::Nodes(nodes)
      }
      UtilityValue::Named { value: name, .. } => {
        let raw = ds.theme.get(&[&format!("--drop-shadow-{name}")]);
        let resolved = ds.theme.resolve(Some(name), &["--drop-shadow"], ThemeOptions::NONE);
        if let (Some(raw), Some(resolved)) = (raw.filter(|v| !v.is_empty()), resolved.filter(|v| !v.is_empty())) {
          if candidate.modifier().is_some() && alpha.is_none() {
            return Compiled::Skip;
          }
          let mut nodes = vec![filter_properties(), decl_opt("--tw-drop-shadow-alpha", alpha.clone())];
          nodes.extend(alpha_replaced_drop_shadow(
            "--tw-drop-shadow-size",
            &raw,
            alpha.as_deref(),
            &inject,
            "",
          ));
          if alpha.is_some() {
            nodes.push(decl("--tw-drop-shadow", "var(--tw-drop-shadow-size)"));
          } else {
            nodes.push(decl("--tw-drop-shadow", drop_shadows(&resolved)));
          }
          nodes.push(decl("filter", FILTER));
          return Compiled::Nodes(nodes);
        }

        match resolve_theme_color(candidate, ds, &["--drop-shadow-color", "--color"]) {
          Some(v) if v == "inherit" => Compiled::Nodes(vec![
            filter_properties(),
            decl("--tw-drop-shadow-color", "inherit"),
            decl("--tw-drop-shadow", "var(--tw-drop-shadow-size)"),
          ]),
          Some(v) if !v.is_empty() => Compiled::Nodes(vec![
            filter_properties(),
            decl("--tw-drop-shadow-color", with_alpha(&v, "var(--tw-drop-shadow-alpha)")),
            decl("--tw-drop-shadow", "var(--tw-drop-shadow-size)"),
          ]),
          _ => Compiled::Skip,
        }
      }
    }
  });

  functional_utility(
    u,
    "backdrop-opacity",
    Desc::new(|v, _, _| Some(backdrop_nodes("--tw-backdrop-opacity", format!("opacity({v})"))))
      .keys(&["--backdrop-opacity", "--opacity"])
      .bare(|v, _, _| {
        if is_valid_opacity_value(v) {
          Some(format!("{v}%"))
        } else {
          None
        }
      }),
  );

  {
    let timing = format!(
      "var(--tw-ease, {})",
      theme
        .resolve(None, &["--default-transition-timing-function"], ThemeOptions::NONE)
        .unwrap_or_else(|| "ease".to_owned())
    );
    let duration = format!(
      "var(--tw-duration, {})",
      theme
        .resolve(None, &["--default-transition-duration"], ThemeOptions::NONE)
        .unwrap_or_else(|| "0s".to_owned())
    );
    let with = |prop: &str| {
      vec![
        decl("transition-property", prop),
        decl("transition-timing-function", timing.clone()),
        decl("transition-duration", duration.clone()),
      ]
    };
    let statics = vec![
      ("none", decls("transition-property", "none")),
      ("all", with("all")),
      (
        "colors",
        with(
          "color, background-color, border-color, outline-color, text-decoration-color, fill, stroke, --tw-gradient-from, --tw-gradient-via, --tw-gradient-to",
        ),
      ),
      ("opacity", with("opacity")),
      ("shadow", with("box-shadow")),
      ("transform", with("transform, translate, scale, rotate")),
    ];
    let (t, du) = (timing.clone(), duration.clone());
    functional_utility(
      u,
      "transition",
      Desc::new(move |v, _, _| Some(vec![decl("transition-property", v), decl("transition-timing-function", t.clone()), decl("transition-duration", du.clone())]))
        .default_value(
          "color, background-color, border-color, outline-color, text-decoration-color, fill, stroke, --tw-gradient-from, --tw-gradient-via, --tw-gradient-to, opacity, box-shadow, transform, translate, scale, rotate, filter, -webkit-backdrop-filter, backdrop-filter, display, content-visibility, overlay, pointer-events",
        )
        .keys(&["--transition-property"])
        .statics(statics),
    );
  }

  static_decls(u, "transition-discrete", &[("transition-behavior", "allow-discrete")]);
  static_decls(u, "transition-normal", &[("transition-behavior", "normal")]);

  functional_utility(
    u,
    "delay",
    Desc::new(|v, _, _| Some(decls("transition-delay", v)))
      .bare(|v, _, _| {
        if is_positive_integer(v) {
          Some(format!("{v}ms"))
        } else {
          None
        }
      })
      .keys(&["--transition-delay"]),
  );

  fn duration_property() -> Node {
    at_root(vec![property("--tw-duration", None, None)])
  }
  static_utility(
    u,
    "duration-initial",
    vec![Item::Node(duration_property), d("--tw-duration", "initial")],
  );
  u.functional("duration", |candidate, ds| {
    if candidate.modifier().is_some() {
      return Compiled::Skip;
    }
    let Some(value) = candidate.value() else {
      return Compiled::Skip;
    };
    let value = match value {
      UtilityValue::Arbitrary { value, .. } => Some(value.clone()),
      UtilityValue::Named { value, fraction } => {
        let mut resolved = ds.theme.resolve(
          Some(fraction.as_deref().unwrap_or(value)),
          &["--transition-duration"],
          ThemeOptions::NONE,
        );
        if resolved.is_none() && is_positive_integer(value) {
          resolved = Some(format!("{value}ms"));
        }
        resolved
      }
    };
    let Some(value) = value else { return Compiled::Skip };
    Compiled::Nodes(vec![
      duration_property(),
      decl("--tw-duration", value.clone()),
      decl("transition-duration", value),
    ])
  });

  fn ease_property() -> Node {
    at_root(vec![property("--tw-ease", None, None)])
  }
  functional_utility(
    u,
    "ease",
    Desc::new(|v, _, _| {
      Some(vec![
        ease_property(),
        decl("--tw-ease", v),
        decl("transition-timing-function", v),
      ])
    })
    .keys(&["--ease"])
    .statics(vec![
      ("initial", vec![ease_property(), decl("--tw-ease", "initial")]),
      (
        "linear",
        vec![
          ease_property(),
          decl("--tw-ease", "linear"),
          decl("transition-timing-function", "linear"),
        ],
      ),
    ]),
  );

  static_decls(u, "will-change-auto", &[("will-change", "auto")]);
  static_decls(u, "will-change-scroll", &[("will-change", "scroll-position")]);
  static_decls(u, "will-change-contents", &[("will-change", "contents")]);
  static_decls(u, "will-change-transform", &[("will-change", "transform")]);
  functional_utility(
    u,
    "will-change",
    Desc::new(|v, _, _| Some(decls("will-change", v))).keys(&[]),
  );

  static_decls(u, "content-none", &[("--tw-content", "none"), ("content", "none")]);
  functional_utility(
    u,
    "content",
    Desc::new(|v, _, _| {
      Some(vec![
        at_root(vec![property("--tw-content", Some("\"\""), None)]),
        decl("--tw-content", v),
        decl("content", "var(--tw-content)"),
      ])
    })
    .keys(&["--content"]),
  );

  {
    const CONTAIN: &str =
      "var(--tw-contain-size,) var(--tw-contain-layout,) var(--tw-contain-paint,) var(--tw-contain-style,)";
    fn contain_properties() -> Node {
      at_root(vec![
        property("--tw-contain-size", None, None),
        property("--tw-contain-layout", None, None),
        property("--tw-contain-paint", None, None),
        property("--tw-contain-style", None, None),
      ])
    }
    static_decls(u, "contain-none", &[("contain", "none")]);
    static_decls(u, "contain-content", &[("contain", "content")]);
    static_decls(u, "contain-strict", &[("contain", "strict")]);
    for (name, var, value) in [
      ("contain-size", "--tw-contain-size", "size"),
      ("contain-inline-size", "--tw-contain-size", "inline-size"),
      ("contain-layout", "--tw-contain-layout", "layout"),
      ("contain-paint", "--tw-contain-paint", "paint"),
      ("contain-style", "--tw-contain-style", "style"),
    ] {
      static_utility(
        u,
        name,
        vec![Item::Node(contain_properties), d(var, value), d("contain", CONTAIN)],
      );
    }
    functional_utility(u, "contain", Desc::new(|v, _, _| Some(decls("contain", v))).keys(&[]));
  }

  static_decls(u, "forced-color-adjust-none", &[("forced-color-adjust", "none")]);
  static_decls(u, "forced-color-adjust-auto", &[("forced-color-adjust", "auto")]);

  spacing_utility(
    u,
    "leading",
    &["--leading", "--spacing"],
    |v| {
      vec![
        at_root(vec![property("--tw-leading", None, None)]),
        decl("--tw-leading", v),
        decl("line-height", v),
      ]
    },
    false,
    false,
    vec![(
      "none",
      vec![
        at_root(vec![property("--tw-leading", None, None)]),
        decl("--tw-leading", "1"),
        decl("line-height", "1"),
      ],
    )],
  );

  functional_utility(
    u,
    "tracking",
    Desc::new(|v, _, _| {
      Some(vec![
        at_root(vec![property("--tw-tracking", None, None)]),
        decl("--tw-tracking", v),
        decl("letter-spacing", v),
      ])
    })
    .negative()
    .keys(&["--tracking"]),
  );

  static_decls(
    u,
    "antialiased",
    &[
      ("-webkit-font-smoothing", "antialiased"),
      ("-moz-osx-font-smoothing", "grayscale"),
    ],
  );
  static_decls(
    u,
    "subpixel-antialiased",
    &[("-webkit-font-smoothing", "auto"), ("-moz-osx-font-smoothing", "auto")],
  );

  {
    const NUMERIC: &str = "var(--tw-ordinal,) var(--tw-slashed-zero,) var(--tw-numeric-figure,) var(--tw-numeric-spacing,) var(--tw-numeric-fraction,)";
    fn numeric_properties() -> Node {
      at_root(vec![
        property("--tw-ordinal", None, None),
        property("--tw-slashed-zero", None, None),
        property("--tw-numeric-figure", None, None),
        property("--tw-numeric-spacing", None, None),
        property("--tw-numeric-fraction", None, None),
      ])
    }
    static_decls(u, "normal-nums", &[("font-variant-numeric", "normal")]);
    for (name, var, value) in [
      ("ordinal", "--tw-ordinal", "ordinal"),
      ("slashed-zero", "--tw-slashed-zero", "slashed-zero"),
      ("lining-nums", "--tw-numeric-figure", "lining-nums"),
      ("oldstyle-nums", "--tw-numeric-figure", "oldstyle-nums"),
      ("proportional-nums", "--tw-numeric-spacing", "proportional-nums"),
      ("tabular-nums", "--tw-numeric-spacing", "tabular-nums"),
      ("diagonal-fractions", "--tw-numeric-fraction", "diagonal-fractions"),
      ("stacked-fractions", "--tw-numeric-fraction", "stacked-fractions"),
    ] {
      static_utility(
        u,
        name,
        vec![
          Item::Node(numeric_properties),
          d(var, value),
          d("font-variant-numeric", NUMERIC),
        ],
      );
    }
  }

  {
    fn outline_properties() -> Node {
      at_root(vec![property("--tw-outline-style", Some("solid"), None)])
    }
    u.static_utility("outline-hidden", |_, _| {
      Compiled::Nodes(vec![
        decl("--tw-outline-style", "none"),
        decl("outline-style", "none"),
        at_rule(
          "@media",
          "(forced-colors: active)",
          vec![decl("outline", "2px solid transparent"), decl("outline-offset", "2px")],
        ),
      ])
    });
    for style in ["none", "solid", "dashed", "dotted", "double"] {
      static_decls(
        u,
        &format!("outline-{style}"),
        &[("--tw-outline-style", style), ("outline-style", style)],
      );
    }

    let width = |value: &str| {
      vec![
        outline_properties(),
        decl("outline-style", "var(--tw-outline-style)"),
        decl("outline-width", value),
      ]
    };
    u.functional("outline", move |candidate, ds| {
      let Some(value) = candidate.value() else {
        if candidate.modifier().is_some() {
          return Compiled::Skip;
        }
        let value = ds
          .theme
          .get(&["--default-outline-width"])
          .unwrap_or_else(|| "1px".to_owned());
        return Compiled::Nodes(width(&value));
      };
      match value {
        UtilityValue::Arbitrary { value, data_type } => match type_of(
          value,
          data_type,
          &[
            DataType::Color,
            DataType::Length,
            DataType::Number,
            DataType::Percentage,
          ],
        )
        .as_deref()
        {
          Some("length") | Some("number") | Some("percentage") => {
            if candidate.modifier().is_some() {
              return Compiled::Skip;
            }
            Compiled::Nodes(width(value))
          }
          _ => match as_color(value, candidate.modifier(), &ds.theme) {
            Some(v) => Compiled::Nodes(decls("outline-color", &v)),
            None => Compiled::Skip,
          },
        },
        UtilityValue::Named { value, .. } => {
          if let Some(v) = resolve_theme_color(candidate, ds, &["--outline-color", "--color"])
            && !v.is_empty()
          {
            return Compiled::Nodes(decls("outline-color", &v));
          }
          if candidate.modifier().is_some() {
            return Compiled::Skip;
          }
          if let Some(v) = ds.theme.resolve(Some(value), &["--outline-width"], ThemeOptions::NONE)
            && !v.is_empty()
          {
            return Compiled::Nodes(width(&v));
          }
          if is_positive_integer(value) {
            return Compiled::Nodes(width(&format!("{value}px")));
          }
          Compiled::Skip
        }
      }
    });

    functional_utility(
      u,
      "outline-offset",
      Desc::new(|v, _, _| Some(decls("outline-offset", v)))
        .negative()
        .keys(&["--outline-offset"])
        .bare(|v, _, _| {
          if is_positive_integer(v) {
            Some(format!("{v}px"))
          } else {
            None
          }
        }),
    );
  }

  functional_utility(
    u,
    "opacity",
    Desc::new(|v, _, _| Some(decls("opacity", v)))
      .keys(&["--opacity"])
      .bare(|v, _, _| {
        if is_valid_opacity_value(v) {
          Some(format!("{v}%"))
        } else {
          None
        }
      }),
  );

  functional_utility(
    u,
    "underline-offset",
    Desc::new(|v, _, _| Some(decls("text-underline-offset", v)))
      .negative()
      .keys(&["--text-underline-offset"])
      .bare(|v, _, _| {
        if is_positive_integer(v) {
          Some(format!("{v}px"))
        } else {
          None
        }
      })
      .statics(vec![("auto", decls("text-underline-offset", "auto"))]),
  );

  u.functional("text", |candidate, ds| {
    let Some(value) = candidate.value() else {
      return Compiled::Skip;
    };
    match value {
      UtilityValue::Arbitrary { value, data_type } => {
        match type_of(
          value,
          data_type,
          &[
            DataType::Color,
            DataType::Length,
            DataType::Percentage,
            DataType::AbsoluteSize,
            DataType::RelativeSize,
          ],
        )
        .as_deref()
        {
          Some("size") | Some("length") | Some("percentage") | Some("absolute-size") | Some("relative-size") => {
            if let Some(modifier) = candidate.modifier() {
              return match leading_modifier(modifier, ds) {
                Ok(Some(leading)) => {
                  Compiled::Nodes(vec![decl("font-size", value.clone()), decl("line-height", leading)])
                }
                _ => Compiled::Invalid,
              };
            }
            Compiled::Nodes(decls("font-size", value))
          }
          _ => match as_color(value, candidate.modifier(), &ds.theme) {
            Some(v) => Compiled::Nodes(decls("color", &v)),
            None => Compiled::Skip,
          },
        }
      }
      UtilityValue::Named { value, .. } => {
        if let Some(v) = resolve_theme_color(candidate, ds, &["--text-color", "--color"])
          && !v.is_empty()
        {
          return Compiled::Nodes(decls("color", &v));
        }
        let Some((font_size, options)) = ds.theme.resolve_with(
          value,
          &["--text"],
          &["--line-height", "--letter-spacing", "--font-weight"],
        ) else {
          return Compiled::Skip;
        };
        if let Some(modifier) = candidate.modifier() {
          return match leading_modifier(modifier, ds) {
            Ok(Some(leading)) => Compiled::Nodes(vec![decl("font-size", font_size), decl("line-height", leading)]),
            _ => Compiled::Invalid,
          };
        }
        let get = |key: &str| {
          options
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .filter(|v| !v.is_empty())
        };
        Compiled::Nodes(vec![
          decl("font-size", font_size),
          decl_opt(
            "line-height",
            get("--line-height").map(|v| format!("var(--tw-leading, {v})")),
          ),
          decl_opt(
            "letter-spacing",
            get("--letter-spacing").map(|v| format!("var(--tw-tracking, {v})")),
          ),
          decl_opt(
            "font-weight",
            get("--font-weight").map(|v| format!("var(--tw-font-weight, {v})")),
          ),
        ])
      }
    }
  });
}
