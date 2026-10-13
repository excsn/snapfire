//! Utilities from accessibility through borders, in Tailwind's registration order.

use crate::ast::{Node, at_root, decl, style_rule};
use crate::candidate::UtilityValue;
use crate::theme::{Theme, ThemeOptions};
use crate::utilities::helpers::{
  Desc, Item, color_utility, d, functional_utility, resolve_theme_color, spacing_utility, static_decls, static_utility,
};
use crate::utilities::{Compiled, Utilities, as_color, property};
use crate::utils::compare::{compare_breakpoints, dimension};
use crate::utils::infer::{
  DataType, infer_data_type, is_length, is_positive_integer, is_strict_positive_integer, is_valid_spacing_multiplier,
};
use crate::utils::segment::segment;

fn decls(property: &'static str, value: &str) -> Vec<Node> {
  vec![decl(property, value)]
}

pub fn register(u: &mut Utilities, theme: &Theme) {
  static_decls(
    u,
    "sr-only",
    &[
      ("position", "absolute"),
      ("width", "1px"),
      ("height", "1px"),
      ("padding", "0"),
      ("margin", "-1px"),
      ("overflow", "hidden"),
      ("clip-path", "inset(50%)"),
      ("white-space", "nowrap"),
      ("border-width", "0"),
    ],
  );
  static_decls(
    u,
    "not-sr-only",
    &[
      ("position", "static"),
      ("width", "auto"),
      ("height", "auto"),
      ("padding", "0"),
      ("margin", "0"),
      ("overflow", "visible"),
      ("clip-path", "none"),
      ("white-space", "normal"),
    ],
  );

  static_decls(u, "pointer-events-none", &[("pointer-events", "none")]);
  static_decls(u, "pointer-events-auto", &[("pointer-events", "auto")]);

  static_decls(u, "visible", &[("visibility", "visible")]);
  static_decls(u, "invisible", &[("visibility", "hidden")]);
  static_decls(u, "collapse", &[("visibility", "collapse")]);

  static_decls(u, "static", &[("position", "static")]);
  static_decls(u, "fixed", &[("position", "fixed")]);
  static_decls(u, "absolute", &[("position", "absolute")]);
  static_decls(u, "relative", &[("position", "relative")]);
  static_decls(u, "sticky", &[("position", "sticky")]);

  for (name, prop) in [
    ("inset", "inset"),
    ("inset-x", "inset-inline"),
    ("inset-y", "inset-block"),
    ("inset-s", "inset-inline-start"),
    ("inset-e", "inset-inline-end"),
    ("inset-bs", "inset-block-start"),
    ("inset-be", "inset-block-end"),
    ("top", "top"),
    ("right", "right"),
    ("bottom", "bottom"),
    ("left", "left"),
  ] {
    static_decls(u, &format!("{name}-auto"), &[(prop, "auto")]);
    static_decls(u, &format!("{name}-full"), &[(prop, "100%")]);
    static_decls(u, &format!("-{name}-full"), &[(prop, "-100%")]);
    spacing_utility(
      u,
      name,
      &["--inset", "--spacing"],
      move |v| decls(prop, v),
      true,
      true,
      Vec::new(),
    );
  }

  static_decls(u, "isolate", &[("isolation", "isolate")]);
  static_decls(u, "isolation-auto", &[("isolation", "auto")]);

  functional_utility(
    u,
    "z",
    Desc::new(|v, _, _| Some(decls("z-index", v)))
      .negative()
      .bare_integer()
      .keys(&["--z-index"])
      .statics(vec![("auto", decls("z-index", "auto"))]),
  );

  functional_utility(
    u,
    "order",
    Desc::new(|v, _, _| Some(decls("order", v)))
      .negative()
      .bare_integer()
      .keys(&["--order"])
      .statics(vec![
        ("first", decls("order", "-9999")),
        ("last", decls("order", "9999")),
      ]),
  );

  functional_utility(
    u,
    "col",
    Desc::new(|v, _, _| Some(decls("grid-column", v)))
      .negative()
      .bare_integer()
      .keys(&["--grid-column"])
      .statics(vec![("auto", decls("grid-column", "auto"))]),
  );
  functional_utility(
    u,
    "col-span",
    Desc::new(|v, _, _| Some(vec![decl("grid-column", format!("span {v} / span {v}"))]))
      .bare_integer()
      .statics(vec![("full", decls("grid-column", "1 / -1"))]),
  );
  functional_utility(
    u,
    "col-start",
    Desc::new(|v, _, _| Some(decls("grid-column-start", v)))
      .negative()
      .bare_integer()
      .keys(&["--grid-column-start"])
      .statics(vec![("auto", decls("grid-column-start", "auto"))]),
  );
  functional_utility(
    u,
    "col-end",
    Desc::new(|v, _, _| Some(decls("grid-column-end", v)))
      .negative()
      .bare_integer()
      .keys(&["--grid-column-end"])
      .statics(vec![("auto", decls("grid-column-end", "auto"))]),
  );

  functional_utility(
    u,
    "row",
    Desc::new(|v, _, _| Some(decls("grid-row", v)))
      .negative()
      .bare_integer()
      .keys(&["--grid-row"])
      .statics(vec![("auto", decls("grid-row", "auto"))]),
  );
  functional_utility(
    u,
    "row-span",
    Desc::new(|v, _, _| Some(vec![decl("grid-row", format!("span {v} / span {v}"))]))
      .keys(&[])
      .bare_integer()
      .statics(vec![("full", decls("grid-row", "1 / -1"))]),
  );
  functional_utility(
    u,
    "row-start",
    Desc::new(|v, _, _| Some(decls("grid-row-start", v)))
      .negative()
      .bare_integer()
      .keys(&["--grid-row-start"])
      .statics(vec![("auto", decls("grid-row-start", "auto"))]),
  );
  functional_utility(
    u,
    "row-end",
    Desc::new(|v, _, _| Some(decls("grid-row-end", v)))
      .negative()
      .bare_integer()
      .keys(&["--grid-row-end"])
      .statics(vec![("auto", decls("grid-row-end", "auto"))]),
  );

  for (name, value) in [
    ("start", "inline-start"),
    ("end", "inline-end"),
    ("right", "right"),
    ("left", "left"),
    ("none", "none"),
  ] {
    static_decls(u, &format!("float-{name}"), &[("float", value)]);
  }
  for (name, value) in [
    ("start", "inline-start"),
    ("end", "inline-end"),
    ("right", "right"),
    ("left", "left"),
    ("both", "both"),
    ("none", "none"),
  ] {
    static_decls(u, &format!("clear-{name}"), &[("clear", value)]);
  }

  for (ns, prop) in [
    ("m", "margin"),
    ("mx", "margin-inline"),
    ("my", "margin-block"),
    ("ms", "margin-inline-start"),
    ("me", "margin-inline-end"),
    ("mbs", "margin-block-start"),
    ("mbe", "margin-block-end"),
    ("mt", "margin-top"),
    ("mr", "margin-right"),
    ("mb", "margin-bottom"),
    ("ml", "margin-left"),
  ] {
    static_decls(u, &format!("{ns}-auto"), &[(prop, "auto")]);
    spacing_utility(
      u,
      ns,
      &["--margin", "--spacing"],
      move |v| decls(prop, v),
      true,
      false,
      Vec::new(),
    );
  }

  static_decls(u, "box-border", &[("box-sizing", "border-box")]);
  static_decls(u, "box-content", &[("box-sizing", "content-box")]);

  functional_utility(
    u,
    "line-clamp",
    Desc::new(|v, _, _| {
      Some(vec![
        decl("overflow", "hidden"),
        decl("display", "-webkit-box"),
        decl("-webkit-box-orient", "vertical"),
        decl("-webkit-line-clamp", v),
      ])
    })
    .keys(&["--line-clamp"])
    .bare_integer()
    .statics(vec![(
      "none",
      vec![
        decl("overflow", "visible"),
        decl("display", "block"),
        decl("-webkit-box-orient", "horizontal"),
        decl("-webkit-line-clamp", "unset"),
      ],
    )]),
  );

  for (name, value) in [
    ("block", "block"),
    ("inline-block", "inline-block"),
    ("inline", "inline"),
    ("hidden", "none"),
    ("inline-flex", "inline-flex"),
    ("table", "table"),
    ("inline-table", "inline-table"),
    ("table-caption", "table-caption"),
    ("table-cell", "table-cell"),
    ("table-column", "table-column"),
    ("table-column-group", "table-column-group"),
    ("table-footer-group", "table-footer-group"),
    ("table-header-group", "table-header-group"),
    ("table-row-group", "table-row-group"),
    ("table-row", "table-row"),
    ("flow-root", "flow-root"),
    ("flex", "flex"),
    ("grid", "grid"),
    ("inline-grid", "inline-grid"),
    ("contents", "contents"),
    ("list-item", "list-item"),
  ] {
    static_decls(u, name, &[("display", value)]);
  }

  static_decls(u, "field-sizing-content", &[("field-sizing", "content")]);
  static_decls(u, "field-sizing-fixed", &[("field-sizing", "fixed")]);

  functional_utility(
    u,
    "aspect",
    Desc::new(|v, _, _| Some(decls("aspect-ratio", v)))
      .keys(&["--aspect"])
      .bare(|_, fraction, _| {
        let fraction = fraction?;
        let parts = segment(fraction, b'/');
        if !is_valid_spacing_multiplier(parts[0]) || !is_valid_spacing_multiplier(parts.get(1).copied().unwrap_or("")) {
          return None;
        }
        Some(fraction.to_owned())
      })
      .statics(vec![
        ("auto", decls("aspect-ratio", "auto")),
        ("square", decls("aspect-ratio", "1 / 1")),
      ]),
  );

  for (key, value) in [
    ("full", "100%"),
    ("svw", "100svw"),
    ("lvw", "100lvw"),
    ("dvw", "100dvw"),
    ("svh", "100svh"),
    ("lvh", "100lvh"),
    ("dvh", "100dvh"),
    ("min", "min-content"),
    ("max", "max-content"),
    ("fit", "fit-content"),
  ] {
    static_decls(
      u,
      &format!("size-{key}"),
      &[("--tw-sort", "size"), ("width", value), ("height", value)],
    );
    static_decls(u, &format!("w-{key}"), &[("width", value)]);
    static_decls(u, &format!("h-{key}"), &[("height", value)]);
    static_decls(u, &format!("min-w-{key}"), &[("min-width", value)]);
    static_decls(u, &format!("min-h-{key}"), &[("min-height", value)]);
    static_decls(u, &format!("max-w-{key}"), &[("max-width", value)]);
    static_decls(u, &format!("max-h-{key}"), &[("max-height", value)]);
  }

  static_decls(
    u,
    "size-auto",
    &[("--tw-sort", "size"), ("width", "auto"), ("height", "auto")],
  );
  static_decls(u, "w-auto", &[("width", "auto")]);
  static_decls(u, "h-auto", &[("height", "auto")]);
  static_decls(u, "min-w-auto", &[("min-width", "auto")]);
  static_decls(u, "min-h-auto", &[("min-height", "auto")]);

  static_decls(u, "h-lh", &[("height", "1lh")]);
  static_decls(u, "min-h-lh", &[("min-height", "1lh")]);
  static_decls(u, "max-h-lh", &[("max-height", "1lh")]);

  static_decls(u, "w-screen", &[("width", "100vw")]);
  static_decls(u, "min-w-screen", &[("min-width", "100vw")]);
  static_decls(u, "max-w-screen", &[("max-width", "100vw")]);
  static_decls(u, "h-screen", &[("height", "100vh")]);
  static_decls(u, "min-h-screen", &[("min-height", "100vh")]);
  static_decls(u, "max-h-screen", &[("max-height", "100vh")]);

  static_decls(u, "max-w-none", &[("max-width", "none")]);
  static_decls(u, "max-h-none", &[("max-height", "none")]);

  spacing_utility(
    u,
    "size",
    &["--size", "--spacing"],
    |v| vec![decl("--tw-sort", "size"), decl("width", v), decl("height", v)],
    false,
    true,
    Vec::new(),
  );

  for (name, keys, prop) in [
    ("w", &["--width", "--spacing", "--container"][..], "width"),
    ("min-w", &["--min-width", "--spacing", "--container"][..], "min-width"),
    ("max-w", &["--max-width", "--spacing", "--container"][..], "max-width"),
    ("h", &["--height", "--spacing"][..], "height"),
    ("min-h", &["--min-height", "--height", "--spacing"][..], "min-height"),
    ("max-h", &["--max-height", "--height", "--spacing"][..], "max-height"),
  ] {
    spacing_utility(u, name, keys, move |v| decls(prop, v), false, true, Vec::new());
  }

  for (key, value) in [
    ("full", "100%"),
    ("min", "min-content"),
    ("max", "max-content"),
    ("fit", "fit-content"),
  ] {
    static_decls(u, &format!("inline-{key}"), &[("inline-size", value)]);
    static_decls(u, &format!("block-{key}"), &[("block-size", value)]);
    static_decls(u, &format!("min-inline-{key}"), &[("min-inline-size", value)]);
    static_decls(u, &format!("min-block-{key}"), &[("min-block-size", value)]);
    static_decls(u, &format!("max-inline-{key}"), &[("max-inline-size", value)]);
    static_decls(u, &format!("max-block-{key}"), &[("max-block-size", value)]);
  }
  for (key, value) in [("svw", "100svw"), ("lvw", "100lvw"), ("dvw", "100dvw")] {
    static_decls(u, &format!("inline-{key}"), &[("inline-size", value)]);
    static_decls(u, &format!("min-inline-{key}"), &[("min-inline-size", value)]);
    static_decls(u, &format!("max-inline-{key}"), &[("max-inline-size", value)]);
  }
  for (key, value) in [("svh", "100svh"), ("lvh", "100lvh"), ("dvh", "100dvh")] {
    static_decls(u, &format!("block-{key}"), &[("block-size", value)]);
    static_decls(u, &format!("min-block-{key}"), &[("min-block-size", value)]);
    static_decls(u, &format!("max-block-{key}"), &[("max-block-size", value)]);
  }

  static_decls(u, "inline-auto", &[("inline-size", "auto")]);
  static_decls(u, "block-auto", &[("block-size", "auto")]);
  static_decls(u, "min-inline-auto", &[("min-inline-size", "auto")]);
  static_decls(u, "min-block-auto", &[("min-block-size", "auto")]);

  static_decls(u, "block-lh", &[("block-size", "1lh")]);
  static_decls(u, "min-block-lh", &[("min-block-size", "1lh")]);
  static_decls(u, "max-block-lh", &[("max-block-size", "1lh")]);

  static_decls(u, "inline-screen", &[("inline-size", "100vw")]);
  static_decls(u, "min-inline-screen", &[("min-inline-size", "100vw")]);
  static_decls(u, "max-inline-screen", &[("max-inline-size", "100vw")]);
  static_decls(u, "block-screen", &[("block-size", "100vh")]);
  static_decls(u, "min-block-screen", &[("min-block-size", "100vh")]);
  static_decls(u, "max-block-screen", &[("max-block-size", "100vh")]);

  static_decls(u, "max-inline-none", &[("max-inline-size", "none")]);
  static_decls(u, "max-block-none", &[("max-block-size", "none")]);

  for (name, keys, prop) in [
    ("inline", &["--spacing", "--container"][..], "inline-size"),
    ("min-inline", &["--spacing", "--container"][..], "min-inline-size"),
    ("max-inline", &["--spacing", "--container"][..], "max-inline-size"),
    ("block", &["--spacing"][..], "block-size"),
    ("min-block", &["--spacing"][..], "min-block-size"),
    ("max-block", &["--spacing"][..], "max-block-size"),
  ] {
    spacing_utility(u, name, keys, move |v| decls(prop, v), false, true, Vec::new());
  }

  u.static_utility("container", |_, ds| {
    let mut breakpoints: Vec<String> = ds.theme.namespace("--breakpoint").into_values().collect();
    breakpoints.sort_by(|a, z| compare_breakpoints(a, z, true));
    let mut out = vec![decl("--tw-sort", "--tw-container-component"), decl("width", "100%")];
    for breakpoint in breakpoints {
      out.push(crate::ast::at_rule(
        "@media",
        format!("(width >= {breakpoint})"),
        vec![decl("max-width", breakpoint)],
      ));
    }
    Compiled::Nodes(out)
  });

  static_decls(u, "flex-auto", &[("flex", "auto")]);
  static_decls(u, "flex-initial", &[("flex", "0 auto")]);
  static_decls(u, "flex-none", &[("flex", "none")]);

  u.functional("flex", |candidate, _| {
    let Some(value) = candidate.value() else {
      return Compiled::Skip;
    };
    match value {
      UtilityValue::Arbitrary { value, .. } => {
        if candidate.modifier().is_some() {
          return Compiled::Skip;
        }
        Compiled::Nodes(decls("flex", value))
      }
      UtilityValue::Named { value, fraction } => {
        if let Some(fraction) = fraction {
          let parts = segment(fraction, b'/');
          if !is_positive_integer(parts[0]) || !is_positive_integer(parts.get(1).copied().unwrap_or("")) {
            return Compiled::Skip;
          }
          return Compiled::Nodes(vec![decl("flex", format!("calc({fraction} * 100%)"))]);
        }
        if is_positive_integer(value) {
          if candidate.modifier().is_some() {
            return Compiled::Skip;
          }
          return Compiled::Nodes(decls("flex", value));
        }
        Compiled::Skip
      }
    }
  });

  functional_utility(
    u,
    "shrink",
    Desc::new(|v, _, _| Some(decls("flex-shrink", v)))
      .default_value("1")
      .bare_integer(),
  );
  functional_utility(
    u,
    "grow",
    Desc::new(|v, _, _| Some(decls("flex-grow", v)))
      .default_value("1")
      .bare_integer(),
  );

  static_decls(u, "basis-auto", &[("flex-basis", "auto")]);
  static_decls(u, "basis-full", &[("flex-basis", "100%")]);
  spacing_utility(
    u,
    "basis",
    &["--flex-basis", "--spacing", "--container"],
    |v| decls("flex-basis", v),
    false,
    true,
    Vec::new(),
  );

  static_decls(u, "table-auto", &[("table-layout", "auto")]);
  static_decls(u, "table-fixed", &[("table-layout", "fixed")]);
  static_decls(u, "caption-top", &[("caption-side", "top")]);
  static_decls(u, "caption-bottom", &[("caption-side", "bottom")]);
  static_decls(u, "border-collapse", &[("border-collapse", "collapse")]);
  static_decls(u, "border-separate", &[("border-collapse", "separate")]);

  fn border_spacing_properties() -> Node {
    at_root(vec![
      property("--tw-border-spacing-x", Some("0"), Some("<length>")),
      property("--tw-border-spacing-y", Some("0"), Some("<length>")),
    ])
  }
  spacing_utility(
    u,
    "border-spacing",
    &["--border-spacing", "--spacing"],
    |v| {
      vec![
        border_spacing_properties(),
        decl("--tw-border-spacing-x", v),
        decl("--tw-border-spacing-y", v),
        decl(
          "border-spacing",
          "var(--tw-border-spacing-x) var(--tw-border-spacing-y)",
        ),
      ]
    },
    false,
    false,
    Vec::new(),
  );
  spacing_utility(
    u,
    "border-spacing-x",
    &["--border-spacing", "--spacing"],
    |v| {
      vec![
        border_spacing_properties(),
        decl("--tw-border-spacing-x", v),
        decl(
          "border-spacing",
          "var(--tw-border-spacing-x) var(--tw-border-spacing-y)",
        ),
      ]
    },
    false,
    false,
    Vec::new(),
  );
  spacing_utility(
    u,
    "border-spacing-y",
    &["--border-spacing", "--spacing"],
    |v| {
      vec![
        border_spacing_properties(),
        decl("--tw-border-spacing-y", v),
        decl(
          "border-spacing",
          "var(--tw-border-spacing-x) var(--tw-border-spacing-y)",
        ),
      ]
    },
    false,
    false,
    Vec::new(),
  );

  for (root, prop, key) in [
    ("origin", "transform-origin", "--transform-origin"),
    ("perspective-origin", "perspective-origin", "--perspective-origin"),
  ] {
    let statics = [
      ("center", "center"),
      ("top", "top"),
      ("top-right", "100% 0"),
      ("right", "100%"),
      ("bottom-right", "100% 100%"),
      ("bottom", "bottom"),
      ("bottom-left", "0 100%"),
      ("left", "0"),
      ("top-left", "0 0"),
    ]
    .into_iter()
    .map(|(k, v)| (k, decls(prop, v)))
    .collect();
    let keys: &'static [&'static str] = if key == "--transform-origin" {
      &["--transform-origin"]
    } else {
      &["--perspective-origin"]
    };
    functional_utility(
      u,
      root,
      Desc::new(move |v, _, _| Some(decls(prop, v)))
        .keys(keys)
        .statics(statics),
    );
  }

  functional_utility(
    u,
    "perspective",
    Desc::new(|v, _, _| Some(decls("perspective", v)))
      .keys(&["--perspective"])
      .statics(vec![("none", decls("perspective", "none"))]),
  );

  fn translate_properties() -> Node {
    at_root(vec![
      property("--tw-translate-x", Some("0"), None),
      property("--tw-translate-y", Some("0"), None),
      property("--tw-translate-z", Some("0"), None),
    ])
  }

  static_decls(u, "translate-none", &[("translate", "none")]);
  static_utility(
    u,
    "-translate-full",
    vec![
      Item::Node(translate_properties),
      d("--tw-translate-x", "-100%"),
      d("--tw-translate-y", "-100%"),
      d("translate", "var(--tw-translate-x) var(--tw-translate-y)"),
    ],
  );
  static_utility(
    u,
    "translate-full",
    vec![
      Item::Node(translate_properties),
      d("--tw-translate-x", "100%"),
      d("--tw-translate-y", "100%"),
      d("translate", "var(--tw-translate-x) var(--tw-translate-y)"),
    ],
  );

  spacing_utility(
    u,
    "translate",
    &["--translate", "--spacing"],
    |v| {
      vec![
        translate_properties(),
        decl("--tw-translate-x", v),
        decl("--tw-translate-y", v),
        decl("translate", "var(--tw-translate-x) var(--tw-translate-y)"),
      ]
    },
    true,
    true,
    Vec::new(),
  );

  for axis in ["x", "y"] {
    let var: &'static str = if axis == "x" {
      "--tw-translate-x"
    } else {
      "--tw-translate-y"
    };
    static_utility(
      u,
      &format!("-translate-{axis}-full"),
      vec![
        Item::Node(translate_properties),
        d(var, "-100%"),
        d("translate", "var(--tw-translate-x) var(--tw-translate-y)"),
      ],
    );
    static_utility(
      u,
      &format!("translate-{axis}-full"),
      vec![
        Item::Node(translate_properties),
        d(var, "100%"),
        d("translate", "var(--tw-translate-x) var(--tw-translate-y)"),
      ],
    );
    spacing_utility(
      u,
      &format!("translate-{axis}"),
      &["--translate", "--spacing"],
      move |v| {
        vec![
          translate_properties(),
          decl(var, v),
          decl("translate", "var(--tw-translate-x) var(--tw-translate-y)"),
        ]
      },
      true,
      true,
      Vec::new(),
    );
  }

  spacing_utility(
    u,
    "translate-z",
    &["--translate", "--spacing"],
    |v| {
      vec![
        translate_properties(),
        decl("--tw-translate-z", v),
        decl(
          "translate",
          "var(--tw-translate-x) var(--tw-translate-y) var(--tw-translate-z)",
        ),
      ]
    },
    true,
    false,
    Vec::new(),
  );

  static_utility(
    u,
    "translate-3d",
    vec![
      Item::Node(translate_properties),
      d(
        "translate",
        "var(--tw-translate-x) var(--tw-translate-y) var(--tw-translate-z)",
      ),
    ],
  );

  fn scale_properties() -> Node {
    at_root(vec![
      property("--tw-scale-x", Some("1"), None),
      property("--tw-scale-y", Some("1"), None),
      property("--tw-scale-z", Some("1"), None),
    ])
  }

  static_decls(u, "scale-none", &[("scale", "none")]);
  for negative in [true, false] {
    let root = if negative { "-scale" } else { "scale" };
    u.functional(root, move |candidate, ds| {
      let Some(value) = candidate.value() else {
        return Compiled::Skip;
      };
      if candidate.modifier().is_some() {
        return Compiled::Skip;
      }
      let value = match value {
        UtilityValue::Arbitrary { value, .. } => {
          let v = if negative {
            format!("calc({value} * -1)")
          } else {
            value.clone()
          };
          return Compiled::Nodes(vec![decl("scale", v)]);
        }
        UtilityValue::Named { value, .. } => {
          let mut resolved = ds.theme.resolve(Some(value), &["--scale"], ThemeOptions::NONE);
          if resolved.as_deref().is_none_or(str::is_empty) && is_positive_integer(value) {
            resolved = Some(format!("{value}%"));
          }
          match resolved {
            Some(v) if !v.is_empty() => v,
            _ => return Compiled::Skip,
          }
        }
      };
      let value = if negative { format!("calc({value} * -1)") } else { value };
      Compiled::Nodes(vec![
        scale_properties(),
        decl("--tw-scale-x", value.clone()),
        decl("--tw-scale-y", value.clone()),
        decl("--tw-scale-z", value),
        decl("scale", "var(--tw-scale-x) var(--tw-scale-y)"),
      ])
    });
  }

  for axis in ["x", "y", "z"] {
    let var: &'static str = match axis {
      "x" => "--tw-scale-x",
      "y" => "--tw-scale-y",
      _ => "--tw-scale-z",
    };
    let scale = if axis == "z" {
      "var(--tw-scale-x) var(--tw-scale-y) var(--tw-scale-z)"
    } else {
      "var(--tw-scale-x) var(--tw-scale-y)"
    };
    functional_utility(
      u,
      &format!("scale-{axis}"),
      Desc::new(move |v, _, _| Some(vec![scale_properties(), decl(var, v), decl("scale", scale)]))
        .negative()
        .keys(&["--scale"])
        .bare(|v, _, _| {
          if is_positive_integer(v) {
            Some(format!("{v}%"))
          } else {
            None
          }
        }),
    );
  }

  static_utility(
    u,
    "scale-3d",
    vec![
      Item::Node(scale_properties),
      d("scale", "var(--tw-scale-x) var(--tw-scale-y) var(--tw-scale-z)"),
    ],
  );

  static_decls(u, "rotate-none", &[("rotate", "none")]);
  for negative in [true, false] {
    let root = if negative { "-rotate" } else { "rotate" };
    u.functional(root, move |candidate, ds| {
      let Some(value) = candidate.value() else {
        return Compiled::Skip;
      };
      if candidate.modifier().is_some() {
        return Compiled::Skip;
      }
      let value = match value {
        UtilityValue::Arbitrary { value, data_type } => {
          let ty = match data_type {
            Some(t) => Some(t.clone()),
            None => infer_data_type(value, &[DataType::Angle, DataType::Vector]).map(|t| t.name().to_owned()),
          };
          if ty.as_deref() == Some("vector") {
            return Compiled::Nodes(vec![decl("rotate", format!("{value} var(--tw-rotate)"))]);
          } else if ty.as_deref() != Some("angle") {
            return Compiled::Nodes(vec![decl(
              "rotate",
              if negative {
                format!("calc({value} * -1)")
              } else {
                value.clone()
              },
            )]);
          }
          value.clone()
        }
        UtilityValue::Named { value, .. } => {
          let mut resolved = ds.theme.resolve(Some(value), &["--rotate"], ThemeOptions::NONE);
          if resolved.as_deref().is_none_or(str::is_empty) && is_positive_integer(value) {
            resolved = Some(format!("{value}deg"));
          }
          match resolved {
            Some(v) if !v.is_empty() => v,
            _ => return Compiled::Skip,
          }
        }
      };
      Compiled::Nodes(vec![decl(
        "rotate",
        if negative { format!("calc({value} * -1)") } else { value },
      )])
    });
  }

  {
    const TRANSFORM_VALUE: &str =
      "var(--tw-rotate-x,) var(--tw-rotate-y,) var(--tw-rotate-z,) var(--tw-skew-x,) var(--tw-skew-y,)";
    fn transform_properties() -> Node {
      at_root(vec![
        property("--tw-rotate-x", None, None),
        property("--tw-rotate-y", None, None),
        property("--tw-rotate-z", None, None),
        property("--tw-skew-x", None, None),
        property("--tw-skew-y", None, None),
      ])
    }

    for axis in ["x", "y", "z"] {
      let (var, fname): (&'static str, &'static str) = match axis {
        "x" => ("--tw-rotate-x", "rotateX"),
        "y" => ("--tw-rotate-y", "rotateY"),
        _ => ("--tw-rotate-z", "rotateZ"),
      };
      functional_utility(
        u,
        &format!("rotate-{axis}"),
        Desc::new(move |v, _, _| {
          Some(vec![
            transform_properties(),
            decl(var, format!("{fname}({v})")),
            decl("transform", TRANSFORM_VALUE),
          ])
        })
        .negative()
        .keys(&["--rotate"])
        .bare(|v, _, _| {
          if is_positive_integer(v) {
            Some(format!("{v}deg"))
          } else {
            None
          }
        }),
      );
    }

    let deg = |v: &str, _: Option<&str>, _: &crate::design_system::DesignSystem| {
      if is_positive_integer(v) {
        Some(format!("{v}deg"))
      } else {
        None
      }
    };
    functional_utility(
      u,
      "skew",
      Desc::new(|v, _, _| {
        Some(vec![
          transform_properties(),
          decl("--tw-skew-x", format!("skewX({v})")),
          decl("--tw-skew-y", format!("skewY({v})")),
          decl("transform", TRANSFORM_VALUE),
        ])
      })
      .negative()
      .keys(&["--skew"])
      .bare(deg),
    );
    functional_utility(
      u,
      "skew-x",
      Desc::new(|v, _, _| {
        Some(vec![
          transform_properties(),
          decl("--tw-skew-x", format!("skewX({v})")),
          decl("transform", TRANSFORM_VALUE),
        ])
      })
      .negative()
      .keys(&["--skew"])
      .bare(deg),
    );
    functional_utility(
      u,
      "skew-y",
      Desc::new(|v, _, _| {
        Some(vec![
          transform_properties(),
          decl("--tw-skew-y", format!("skewY({v})")),
          decl("transform", TRANSFORM_VALUE),
        ])
      })
      .negative()
      .keys(&["--skew"])
      .bare(deg),
    );

    u.functional("transform", |candidate, _| {
      if candidate.modifier().is_some() {
        return Compiled::Skip;
      }
      let value = match candidate.value() {
        None => TRANSFORM_VALUE.to_owned(),
        Some(UtilityValue::Arbitrary { value, .. }) => value.clone(),
        Some(UtilityValue::Named { .. }) => return Compiled::Skip,
      };
      Compiled::Nodes(vec![transform_properties(), decl("transform", value)])
    });

    static_decls(u, "transform-cpu", &[("transform", TRANSFORM_VALUE)]);
    static_utility(
      u,
      "transform-gpu",
      vec![d("transform", format!("translateZ(0) {TRANSFORM_VALUE}"))],
    );
    static_decls(u, "transform-none", &[("transform", "none")]);
  }

  functional_utility(
    u,
    "zoom",
    Desc::new(|v, _, _| Some(decls("zoom", v))).bare(|v, _, _| {
      if is_positive_integer(v) {
        Some(format!("{v}%"))
      } else {
        None
      }
    }),
  );

  static_decls(u, "transform-flat", &[("transform-style", "flat")]);
  static_decls(u, "transform-3d", &[("transform-style", "preserve-3d")]);
  static_decls(u, "transform-content", &[("transform-box", "content-box")]);
  static_decls(u, "transform-border", &[("transform-box", "border-box")]);
  static_decls(u, "transform-fill", &[("transform-box", "fill-box")]);
  static_decls(u, "transform-stroke", &[("transform-box", "stroke-box")]);
  static_decls(u, "transform-view", &[("transform-box", "view-box")]);

  static_decls(u, "backface-visible", &[("backface-visibility", "visible")]);
  static_decls(u, "backface-hidden", &[("backface-visibility", "hidden")]);

  for value in [
    "auto",
    "default",
    "pointer",
    "wait",
    "text",
    "move",
    "help",
    "not-allowed",
    "none",
    "context-menu",
    "progress",
    "cell",
    "crosshair",
    "vertical-text",
    "alias",
    "copy",
    "no-drop",
    "grab",
    "grabbing",
    "all-scroll",
    "col-resize",
    "row-resize",
    "n-resize",
    "e-resize",
    "s-resize",
    "w-resize",
    "ne-resize",
    "nw-resize",
    "se-resize",
    "sw-resize",
    "ew-resize",
    "ns-resize",
    "nesw-resize",
    "nwse-resize",
    "zoom-in",
    "zoom-out",
  ] {
    static_decls(u, &format!("cursor-{value}"), &[("cursor", value)]);
  }
  functional_utility(
    u,
    "cursor",
    Desc::new(|v, _, _| Some(decls("cursor", v))).keys(&["--cursor"]),
  );

  for value in ["auto", "none", "manipulation"] {
    static_decls(u, &format!("touch-{value}"), &[("touch-action", value)]);
  }
  fn touch_properties() -> Node {
    at_root(vec![
      property("--tw-pan-x", None, None),
      property("--tw-pan-y", None, None),
      property("--tw-pinch-zoom", None, None),
    ])
  }
  const TOUCH: &str = "var(--tw-pan-x,) var(--tw-pan-y,) var(--tw-pinch-zoom,)";
  for value in ["x", "left", "right"] {
    static_utility(
      u,
      &format!("touch-pan-{value}"),
      vec![
        Item::Node(touch_properties),
        d("--tw-pan-x", format!("pan-{value}")),
        d("touch-action", TOUCH),
      ],
    );
  }
  for value in ["y", "up", "down"] {
    static_utility(
      u,
      &format!("touch-pan-{value}"),
      vec![
        Item::Node(touch_properties),
        d("--tw-pan-y", format!("pan-{value}")),
        d("touch-action", TOUCH),
      ],
    );
  }
  static_utility(
    u,
    "touch-pinch-zoom",
    vec![
      Item::Node(touch_properties),
      d("--tw-pinch-zoom", "pinch-zoom"),
      d("touch-action", TOUCH),
    ],
  );

  for value in ["none", "text", "all", "auto"] {
    static_decls(
      u,
      &format!("select-{value}"),
      &[("-webkit-user-select", value), ("user-select", value)],
    );
  }

  static_decls(u, "resize-none", &[("resize", "none")]);
  static_decls(u, "resize-x", &[("resize", "horizontal")]);
  static_decls(u, "resize-y", &[("resize", "vertical")]);
  static_decls(u, "resize", &[("resize", "both")]);

  static_decls(u, "snap-none", &[("scroll-snap-type", "none")]);
  fn snap_properties() -> Node {
    at_root(vec![property(
      "--tw-scroll-snap-strictness",
      Some("proximity"),
      Some("*"),
    )])
  }
  for value in ["x", "y", "both"] {
    static_utility(
      u,
      &format!("snap-{value}"),
      vec![
        Item::Node(snap_properties),
        d("scroll-snap-type", format!("{value} var(--tw-scroll-snap-strictness)")),
      ],
    );
  }
  static_utility(
    u,
    "snap-mandatory",
    vec![
      Item::Node(snap_properties),
      d("--tw-scroll-snap-strictness", "mandatory"),
    ],
  );
  static_utility(
    u,
    "snap-proximity",
    vec![
      Item::Node(snap_properties),
      d("--tw-scroll-snap-strictness", "proximity"),
    ],
  );

  static_decls(u, "snap-align-none", &[("scroll-snap-align", "none")]);
  static_decls(u, "snap-start", &[("scroll-snap-align", "start")]);
  static_decls(u, "snap-end", &[("scroll-snap-align", "end")]);
  static_decls(u, "snap-center", &[("scroll-snap-align", "center")]);
  static_decls(u, "snap-normal", &[("scroll-snap-stop", "normal")]);
  static_decls(u, "snap-always", &[("scroll-snap-stop", "always")]);

  for (ns, prop) in [
    ("scroll-m", "scroll-margin"),
    ("scroll-mx", "scroll-margin-inline"),
    ("scroll-my", "scroll-margin-block"),
    ("scroll-ms", "scroll-margin-inline-start"),
    ("scroll-me", "scroll-margin-inline-end"),
    ("scroll-mbs", "scroll-margin-block-start"),
    ("scroll-mbe", "scroll-margin-block-end"),
    ("scroll-mt", "scroll-margin-top"),
    ("scroll-mr", "scroll-margin-right"),
    ("scroll-mb", "scroll-margin-bottom"),
    ("scroll-ml", "scroll-margin-left"),
  ] {
    spacing_utility(
      u,
      ns,
      &["--scroll-margin", "--spacing"],
      move |v| decls(prop, v),
      true,
      false,
      Vec::new(),
    );
  }
  for (ns, prop) in [
    ("scroll-p", "scroll-padding"),
    ("scroll-px", "scroll-padding-inline"),
    ("scroll-py", "scroll-padding-block"),
    ("scroll-ps", "scroll-padding-inline-start"),
    ("scroll-pe", "scroll-padding-inline-end"),
    ("scroll-pbs", "scroll-padding-block-start"),
    ("scroll-pbe", "scroll-padding-block-end"),
    ("scroll-pt", "scroll-padding-top"),
    ("scroll-pr", "scroll-padding-right"),
    ("scroll-pb", "scroll-padding-bottom"),
    ("scroll-pl", "scroll-padding-left"),
  ] {
    spacing_utility(
      u,
      ns,
      &["--scroll-padding", "--spacing"],
      move |v| decls(prop, v),
      false,
      false,
      Vec::new(),
    );
  }

  static_decls(u, "list-inside", &[("list-style-position", "inside")]);
  static_decls(u, "list-outside", &[("list-style-position", "outside")]);
  functional_utility(
    u,
    "list",
    Desc::new(|v, _, _| Some(decls("list-style-type", v)))
      .keys(&["--list-style-type"])
      .statics(vec![
        ("none", decls("list-style-type", "none")),
        ("disc", decls("list-style-type", "disc")),
        ("decimal", decls("list-style-type", "decimal")),
      ]),
  );
  functional_utility(
    u,
    "list-image",
    Desc::new(|v, _, _| Some(decls("list-style-image", v)))
      .keys(&["--list-style-image"])
      .statics(vec![("none", decls("list-style-image", "none"))]),
  );

  static_decls(u, "appearance-none", &[("appearance", "none")]);
  static_decls(u, "appearance-auto", &[("appearance", "auto")]);

  static_decls(u, "scheme-normal", &[("color-scheme", "normal")]);
  static_decls(u, "scheme-dark", &[("color-scheme", "dark")]);
  static_decls(u, "scheme-light", &[("color-scheme", "light")]);
  static_decls(u, "scheme-light-dark", &[("color-scheme", "light dark")]);
  static_decls(u, "scheme-only-dark", &[("color-scheme", "only dark")]);
  static_decls(u, "scheme-only-light", &[("color-scheme", "only light")]);

  functional_utility(
    u,
    "columns",
    Desc::new(|v, _, _| Some(decls("columns", v)))
      .keys(&["--columns", "--container"])
      .bare_integer()
      .statics(vec![("auto", decls("columns", "auto"))]),
  );

  for value in ["auto", "avoid", "all", "avoid-page", "page", "left", "right", "column"] {
    static_decls(u, &format!("break-before-{value}"), &[("break-before", value)]);
  }
  for value in ["auto", "avoid", "avoid-page", "avoid-column"] {
    static_decls(u, &format!("break-inside-{value}"), &[("break-inside", value)]);
  }
  for value in ["auto", "avoid", "all", "avoid-page", "page", "left", "right", "column"] {
    static_decls(u, &format!("break-after-{value}"), &[("break-after", value)]);
  }

  static_decls(u, "grid-flow-row", &[("grid-auto-flow", "row")]);
  static_decls(u, "grid-flow-col", &[("grid-auto-flow", "column")]);
  static_decls(u, "grid-flow-dense", &[("grid-auto-flow", "dense")]);
  static_decls(u, "grid-flow-row-dense", &[("grid-auto-flow", "row dense")]);
  static_decls(u, "grid-flow-col-dense", &[("grid-auto-flow", "column dense")]);

  for (root, prop, key) in [
    ("auto-cols", "grid-auto-columns", "--grid-auto-columns"),
    ("auto-rows", "grid-auto-rows", "--grid-auto-rows"),
  ] {
    let keys: &'static [&'static str] = if key == "--grid-auto-columns" {
      &["--grid-auto-columns"]
    } else {
      &["--grid-auto-rows"]
    };
    functional_utility(
      u,
      root,
      Desc::new(move |v, _, _| Some(decls(prop, v)))
        .keys(keys)
        .bare(|v, _, ds| {
          ds.theme.resolve(None, &["--spacing"], ThemeOptions::NONE)?;
          if !is_valid_spacing_multiplier(v) {
            return None;
          }
          Some(format!("--spacing({v})"))
        })
        .statics(vec![
          ("auto", decls(prop, "auto")),
          ("min", decls(prop, "min-content")),
          ("max", decls(prop, "max-content")),
          ("fr", decls(prop, "minmax(0, 1fr)")),
        ]),
    );
  }

  for (root, prop, key) in [
    ("grid-cols", "grid-template-columns", "--grid-template-columns"),
    ("grid-rows", "grid-template-rows", "--grid-template-rows"),
  ] {
    let keys: &'static [&'static str] = if key == "--grid-template-columns" {
      &["--grid-template-columns"]
    } else {
      &["--grid-template-rows"]
    };
    functional_utility(
      u,
      root,
      Desc::new(move |v, _, _| Some(decls(prop, v)))
        .keys(keys)
        .bare(|v, _, _| {
          if is_strict_positive_integer(v) {
            Some(format!("repeat({v}, minmax(0, 1fr))"))
          } else {
            None
          }
        })
        .statics(vec![("none", decls(prop, "none")), ("subgrid", decls(prop, "subgrid"))]),
    );
  }

  static_decls(u, "flex-row", &[("flex-direction", "row")]);
  static_decls(u, "flex-row-reverse", &[("flex-direction", "row-reverse")]);
  static_decls(u, "flex-col", &[("flex-direction", "column")]);
  static_decls(u, "flex-col-reverse", &[("flex-direction", "column-reverse")]);
  static_decls(u, "flex-wrap", &[("flex-wrap", "wrap")]);
  static_decls(u, "flex-nowrap", &[("flex-wrap", "nowrap")]);
  static_decls(u, "flex-wrap-reverse", &[("flex-wrap", "wrap-reverse")]);

  for (name, value) in [
    ("center", "center"),
    ("start", "start"),
    ("end", "end"),
    ("center-safe", "safe center"),
    ("end-safe", "safe end"),
    ("between", "space-between"),
    ("around", "space-around"),
    ("evenly", "space-evenly"),
    ("baseline", "baseline"),
    ("stretch", "stretch"),
  ] {
    static_decls(u, &format!("place-content-{name}"), &[("place-content", value)]);
  }
  for (name, value) in [
    ("center", "center"),
    ("start", "start"),
    ("end", "end"),
    ("center-safe", "safe center"),
    ("end-safe", "safe end"),
    ("baseline", "baseline"),
    ("stretch", "stretch"),
  ] {
    static_decls(u, &format!("place-items-{name}"), &[("place-items", value)]);
  }
  for (name, value) in [
    ("normal", "normal"),
    ("center", "center"),
    ("start", "flex-start"),
    ("end", "flex-end"),
    ("center-safe", "safe center"),
    ("end-safe", "safe flex-end"),
    ("between", "space-between"),
    ("around", "space-around"),
    ("evenly", "space-evenly"),
    ("baseline", "baseline"),
    ("stretch", "stretch"),
  ] {
    static_decls(u, &format!("content-{name}"), &[("align-content", value)]);
  }
  for (name, value) in [
    ("center", "center"),
    ("start", "flex-start"),
    ("end", "flex-end"),
    ("center-safe", "safe center"),
    ("end-safe", "safe flex-end"),
    ("baseline", "baseline"),
    ("baseline-last", "last baseline"),
    ("stretch", "stretch"),
  ] {
    static_decls(u, &format!("items-{name}"), &[("align-items", value)]);
  }
  for (name, value) in [
    ("normal", "normal"),
    ("center", "center"),
    ("start", "flex-start"),
    ("end", "flex-end"),
    ("center-safe", "safe center"),
    ("end-safe", "safe flex-end"),
    ("between", "space-between"),
    ("around", "space-around"),
    ("evenly", "space-evenly"),
    ("baseline", "baseline"),
    ("stretch", "stretch"),
  ] {
    static_decls(u, &format!("justify-{name}"), &[("justify-content", value)]);
  }
  for (name, value) in [
    ("normal", "normal"),
    ("center", "center"),
    ("start", "start"),
    ("end", "end"),
    ("center-safe", "safe center"),
    ("end-safe", "safe end"),
    ("stretch", "stretch"),
  ] {
    static_decls(u, &format!("justify-items-{name}"), &[("justify-items", value)]);
  }

  spacing_utility(
    u,
    "gap",
    &["--gap", "--spacing"],
    |v| decls("gap", v),
    false,
    false,
    Vec::new(),
  );
  spacing_utility(
    u,
    "gap-x",
    &["--gap", "--spacing"],
    |v| decls("column-gap", v),
    false,
    false,
    Vec::new(),
  );
  spacing_utility(
    u,
    "gap-y",
    &["--gap", "--spacing"],
    |v| decls("row-gap", v),
    false,
    false,
    Vec::new(),
  );

  fn is_zero(value: &str) -> bool {
    if value == "--spacing(0)" || value == "--spacing(-0)" {
      return true;
    }
    matches!(dimension(value), Some((n, unit)) if n == 0.0 && (unit.is_none() || is_length(value)))
  }

  for (axis, sort, start, end) in [
    ("x", "row-gap", "margin-inline-start", "margin-inline-end"),
    ("y", "column-gap", "margin-block-start", "margin-block-end"),
  ] {
    let reverse: &'static str = if axis == "x" {
      "--tw-space-x-reverse"
    } else {
      "--tw-space-y-reverse"
    };
    spacing_utility(
      u,
      &format!("space-{axis}"),
      &["--space", "--spacing"],
      move |v| {
        let zero = is_zero(v);
        vec![
          at_root(vec![property(reverse, Some("0"), None)]),
          style_rule(
            ":where(& > :not(:last-child))",
            vec![
              decl("--tw-sort", sort),
              decl(reverse, "0"),
              decl(
                start,
                if zero {
                  "0".to_owned()
                } else {
                  format!("calc({v} * var({reverse}))")
                },
              ),
              decl(
                end,
                if zero {
                  "0".to_owned()
                } else {
                  format!("calc({v} * calc(1 - var({reverse})))")
                },
              ),
            ],
          ),
        ]
      },
      true,
      false,
      Vec::new(),
    );
  }

  u.static_utility("space-x-reverse", |_, _| {
    Compiled::Nodes(vec![
      at_root(vec![property("--tw-space-x-reverse", Some("0"), None)]),
      style_rule(
        ":where(& > :not(:last-child))",
        vec![decl("--tw-sort", "row-gap"), decl("--tw-space-x-reverse", "1")],
      ),
    ])
  });
  u.static_utility("space-y-reverse", |_, _| {
    Compiled::Nodes(vec![
      at_root(vec![property("--tw-space-y-reverse", Some("0"), None)]),
      style_rule(
        ":where(& > :not(:last-child))",
        vec![decl("--tw-sort", "column-gap"), decl("--tw-space-y-reverse", "1")],
      ),
    ])
  });

  static_decls(u, "accent-auto", &[("accent-color", "auto")]);
  color_utility(u, "accent", &["--accent-color", "--color"], |v, _| {
    Some(decls("accent-color", v))
  });
  color_utility(u, "caret", &["--caret-color", "--color"], |v, _| {
    Some(decls("caret-color", v))
  });
  color_utility(u, "divide", &["--divide-color", "--border-color", "--color"], |v, _| {
    Some(vec![style_rule(
      ":where(& > :not(:last-child))",
      vec![decl("--tw-sort", "divide-color"), decl("border-color", v)],
    )])
  });

  for (name, value) in [
    ("auto", "auto"),
    ("start", "start"),
    ("end", "end"),
    ("center", "center"),
    ("end-safe", "safe end"),
    ("center-safe", "safe center"),
    ("stretch", "stretch"),
  ] {
    static_decls(u, &format!("place-self-{name}"), &[("place-self", value)]);
  }
  for (name, value) in [
    ("auto", "auto"),
    ("start", "flex-start"),
    ("end", "flex-end"),
    ("center", "center"),
    ("end-safe", "safe flex-end"),
    ("center-safe", "safe center"),
    ("stretch", "stretch"),
    ("baseline", "baseline"),
    ("baseline-last", "last baseline"),
  ] {
    static_decls(u, &format!("self-{name}"), &[("align-self", value)]);
  }
  for (name, value) in [
    ("auto", "auto"),
    ("start", "flex-start"),
    ("end", "flex-end"),
    ("center", "center"),
    ("end-safe", "safe flex-end"),
    ("center-safe", "safe center"),
    ("stretch", "stretch"),
  ] {
    static_decls(u, &format!("justify-self-{name}"), &[("justify-self", value)]);
  }

  for value in ["auto", "hidden", "clip", "visible", "scroll"] {
    static_decls(u, &format!("overflow-{value}"), &[("overflow", value)]);
    static_decls(u, &format!("overflow-x-{value}"), &[("overflow-x", value)]);
    static_decls(u, &format!("overflow-y-{value}"), &[("overflow-y", value)]);
  }
  for value in ["auto", "contain", "none"] {
    static_decls(u, &format!("overscroll-{value}"), &[("overscroll-behavior", value)]);
    static_decls(u, &format!("overscroll-x-{value}"), &[("overscroll-behavior-x", value)]);
    static_decls(u, &format!("overscroll-y-{value}"), &[("overscroll-behavior-y", value)]);
  }

  static_decls(u, "scroll-auto", &[("scroll-behavior", "auto")]);
  static_decls(u, "scroll-smooth", &[("scroll-behavior", "smooth")]);
  static_decls(u, "scrollbar-auto", &[("scrollbar-width", "auto")]);
  static_decls(u, "scrollbar-thin", &[("scrollbar-width", "thin")]);
  static_decls(u, "scrollbar-none", &[("scrollbar-width", "none")]);

  fn scrollbar_color_properties() -> Node {
    at_root(vec![
      property("--tw-scrollbar-thumb", Some("#0000"), Some("<color>")),
      property("--tw-scrollbar-track", Some("#0000"), Some("<color>")),
    ])
  }
  for (root, var) in [
    ("scrollbar-thumb", "--tw-scrollbar-thumb"),
    ("scrollbar-track", "--tw-scrollbar-track"),
  ] {
    color_utility(u, root, &["--color"], move |v, _| {
      Some(vec![
        scrollbar_color_properties(),
        decl(var, v),
        decl("scrollbar-color", "var(--tw-scrollbar-thumb) var(--tw-scrollbar-track)"),
      ])
    });
  }

  static_decls(u, "scrollbar-gutter-auto", &[("scrollbar-gutter", "auto")]);
  static_decls(u, "scrollbar-gutter-stable", &[("scrollbar-gutter", "stable")]);
  static_decls(u, "scrollbar-gutter-both", &[("scrollbar-gutter", "stable both-edges")]);

  static_decls(
    u,
    "truncate",
    &[
      ("overflow", "hidden"),
      ("text-overflow", "ellipsis"),
      ("white-space", "nowrap"),
    ],
  );
  static_decls(u, "text-ellipsis", &[("text-overflow", "ellipsis")]);
  static_decls(u, "text-clip", &[("text-overflow", "clip")]);

  for value in ["none", "manual", "auto"] {
    static_decls(
      u,
      &format!("hyphens-{value}"),
      &[("-webkit-hyphens", value), ("hyphens", value)],
    );
  }

  for (name, value) in [
    ("normal", "normal"),
    ("nowrap", "nowrap"),
    ("pre", "pre"),
    ("pre-line", "pre-line"),
    ("pre-wrap", "pre-wrap"),
    ("break-spaces", "break-spaces"),
  ] {
    static_decls(u, &format!("whitespace-{name}"), &[("white-space", value)]);
  }

  functional_utility(u, "tab", Desc::new(|v, _, _| Some(decls("tab-size", v))).bare_integer());

  static_decls(u, "text-wrap", &[("text-wrap", "wrap")]);
  static_decls(u, "text-nowrap", &[("text-wrap", "nowrap")]);
  static_decls(u, "text-balance", &[("text-wrap", "balance")]);
  static_decls(u, "text-pretty", &[("text-wrap", "pretty")]);
  static_decls(
    u,
    "break-normal",
    &[("overflow-wrap", "normal"), ("word-break", "normal")],
  );
  static_decls(u, "break-all", &[("word-break", "break-all")]);
  static_decls(u, "break-keep", &[("word-break", "keep-all")]);
  static_decls(u, "wrap-anywhere", &[("overflow-wrap", "anywhere")]);
  static_decls(u, "wrap-break-word", &[("overflow-wrap", "break-word")]);
  static_decls(u, "wrap-normal", &[("overflow-wrap", "normal")]);

  for (root, props) in [
    ("rounded", &["border-radius"][..]),
    (
      "rounded-s",
      &["border-start-start-radius", "border-end-start-radius"][..],
    ),
    ("rounded-e", &["border-start-end-radius", "border-end-end-radius"][..]),
    ("rounded-t", &["border-top-left-radius", "border-top-right-radius"][..]),
    (
      "rounded-r",
      &["border-top-right-radius", "border-bottom-right-radius"][..],
    ),
    (
      "rounded-b",
      &["border-bottom-right-radius", "border-bottom-left-radius"][..],
    ),
    (
      "rounded-l",
      &["border-top-left-radius", "border-bottom-left-radius"][..],
    ),
    ("rounded-ss", &["border-start-start-radius"][..]),
    ("rounded-se", &["border-start-end-radius"][..]),
    ("rounded-ee", &["border-end-end-radius"][..]),
    ("rounded-es", &["border-end-start-radius"][..]),
    ("rounded-tl", &["border-top-left-radius"][..]),
    ("rounded-tr", &["border-top-right-radius"][..]),
    ("rounded-br", &["border-bottom-right-radius"][..]),
    ("rounded-bl", &["border-bottom-left-radius"][..]),
  ] {
    let all = |value: &str| -> Vec<Node> { props.iter().map(|p| decl(*p, value)).collect() };
    let statics = vec![("none", all("0")), ("full", all("calc(infinity * 1px)"))];
    functional_utility(
      u,
      root,
      Desc::new(move |v, _, _| Some(props.iter().map(|p| decl(*p, v)).collect()))
        .keys(&["--radius"])
        .statics(statics),
    );
  }

  for style in ["solid", "dashed", "dotted", "double", "hidden", "none"] {
    static_decls(
      u,
      &format!("border-{style}"),
      &[("--tw-border-style", style), ("border-style", style)],
    );
  }

  fn border_properties() -> Node {
    at_root(vec![property("--tw-border-style", Some("solid"), None)])
  }

  for (root, style, width, color) in [
    ("border", "border-style", "border-width", "border-color"),
    (
      "border-x",
      "border-inline-style",
      "border-inline-width",
      "border-inline-color",
    ),
    (
      "border-y",
      "border-block-style",
      "border-block-width",
      "border-block-color",
    ),
    (
      "border-s",
      "border-inline-start-style",
      "border-inline-start-width",
      "border-inline-start-color",
    ),
    (
      "border-e",
      "border-inline-end-style",
      "border-inline-end-width",
      "border-inline-end-color",
    ),
    (
      "border-bs",
      "border-block-start-style",
      "border-block-start-width",
      "border-block-start-color",
    ),
    (
      "border-be",
      "border-block-end-style",
      "border-block-end-width",
      "border-block-end-color",
    ),
    ("border-t", "border-top-style", "border-top-width", "border-top-color"),
    (
      "border-r",
      "border-right-style",
      "border-right-width",
      "border-right-color",
    ),
    (
      "border-b",
      "border-bottom-style",
      "border-bottom-width",
      "border-bottom-color",
    ),
    (
      "border-l",
      "border-left-style",
      "border-left-width",
      "border-left-color",
    ),
  ] {
    let widths = move |value: &str| {
      vec![
        border_properties(),
        decl(style, "var(--tw-border-style)"),
        decl(width, value),
      ]
    };
    u.functional(root, move |candidate, ds| {
      let Some(value) = candidate.value() else {
        if candidate.modifier().is_some() {
          return Compiled::Skip;
        }
        let value = ds
          .theme
          .get(&["--default-border-width"])
          .unwrap_or_else(|| "1px".to_owned());
        return Compiled::Nodes(widths(&value));
      };
      match value {
        UtilityValue::Arbitrary { value, data_type } => {
          let ty = match data_type {
            Some(t) => Some(t.clone()),
            None => infer_data_type(value, &[DataType::Color, DataType::LineWidth, DataType::Length])
              .map(|t| t.name().to_owned()),
          };
          match ty.as_deref() {
            Some("line-width") | Some("length") => {
              if candidate.modifier().is_some() {
                return Compiled::Skip;
              }
              Compiled::Nodes(widths(value))
            }
            _ => match as_color(value, candidate.modifier(), &ds.theme) {
              Some(v) => Compiled::Nodes(decls(color, &v)),
              None => Compiled::Skip,
            },
          }
        }
        UtilityValue::Named { value, .. } => {
          if let Some(v) = resolve_theme_color(candidate, ds, &["--border-color", "--color"])
            && !v.is_empty()
          {
            return Compiled::Nodes(decls(color, &v));
          }
          if candidate.modifier().is_some() {
            return Compiled::Skip;
          }
          if let Some(v) = ds.theme.resolve(Some(value), &["--border-width"], ThemeOptions::NONE)
            && !v.is_empty()
          {
            return Compiled::Nodes(widths(&v));
          }
          if is_positive_integer(value) {
            return Compiled::Nodes(widths(&format!("{value}px")));
          }
          Compiled::Skip
        }
      }
    });
  }

  let default_border = theme
    .get(&["--default-border-width"])
    .unwrap_or_else(|| "1px".to_owned());
  for (axis, sort) in [("x", "divide-x-width"), ("y", "divide-y-width")] {
    let reverse: &'static str = if axis == "x" {
      "--tw-divide-x-reverse"
    } else {
      "--tw-divide-y-reverse"
    };
    functional_utility(
      u,
      &format!("divide-{axis}"),
      Desc::new(move |v, _, _| {
        let body = if reverse == "--tw-divide-x-reverse" {
          vec![
            decl("--tw-sort", sort),
            border_properties(),
            decl(reverse, "0"),
            decl("border-inline-style", "var(--tw-border-style)"),
            decl("border-inline-start-width", format!("calc({v} * var({reverse}))")),
            decl(
              "border-inline-end-width",
              format!("calc({v} * calc(1 - var({reverse})))"),
            ),
          ]
        } else {
          vec![
            decl("--tw-sort", sort),
            border_properties(),
            decl(reverse, "0"),
            decl("border-bottom-style", "var(--tw-border-style)"),
            decl("border-top-style", "var(--tw-border-style)"),
            decl("border-top-width", format!("calc({v} * var({reverse}))")),
            decl("border-bottom-width", format!("calc({v} * calc(1 - var({reverse})))")),
          ]
        };
        Some(vec![
          at_root(vec![property(reverse, Some("0"), None)]),
          style_rule(":where(& > :not(:last-child))", body),
        ])
      })
      .default_value(&default_border)
      .keys(&["--divide-width", "--border-width"])
      .bare(|v, _, _| {
        if is_positive_integer(v) {
          Some(format!("{v}px"))
        } else {
          None
        }
      }),
    );
  }

  for axis in ["x", "y"] {
    let reverse: &'static str = if axis == "x" {
      "--tw-divide-x-reverse"
    } else {
      "--tw-divide-y-reverse"
    };
    u.static_utility(&format!("divide-{axis}-reverse"), move |_, _| {
      Compiled::Nodes(vec![
        at_root(vec![property(reverse, Some("0"), None)]),
        style_rule(":where(& > :not(:last-child))", vec![decl(reverse, "1")]),
      ])
    });
  }

  for value in ["solid", "dashed", "dotted", "double", "none"] {
    u.static_utility(&format!("divide-{value}"), move |_, _| {
      Compiled::Nodes(vec![style_rule(
        ":where(& > :not(:last-child))",
        vec![
          decl("--tw-sort", "divide-style"),
          decl("--tw-border-style", value),
          decl("border-style", value),
        ],
      )])
    });
  }
}
