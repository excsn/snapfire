//! Mask gradients, blend modes, fill and stroke, object fit, padding, text alignment and
//! `font-*`, in Tailwind's registration order.

use std::rc::Rc;

use crate::ast::{Node, at_root, decl};
use crate::candidate::UtilityValue;
use crate::theme::ThemeOptions;
use crate::utilities::backgrounds::{StopFn, mask_properties_gradient, mask_stop_utility};
use crate::utilities::helpers::{Desc, functional_utility, resolve_theme_color, spacing_utility, static_decls};
use crate::utilities::{Compiled, Utilities, as_color, property};
use crate::utils::infer::{DataType, infer_data_type, is_positive_integer};

fn decls(property: &'static str, value: &str) -> Vec<Node> {
  vec![decl(property, value)]
}

const MASK_IMAGE: &str = "var(--tw-mask-linear), var(--tw-mask-radial), var(--tw-mask-conic)";

fn mask_properties_edge() -> Node {
  at_root(vec![
    property("--tw-mask-left", Some("linear-gradient(#fff, #fff)"), None),
    property("--tw-mask-right", Some("linear-gradient(#fff, #fff)"), None),
    property("--tw-mask-bottom", Some("linear-gradient(#fff, #fff)"), None),
    property("--tw-mask-top", Some("linear-gradient(#fff, #fff)"), None),
  ])
}

fn edge_nodes(edges: [bool; 4], var_suffix: &str, value: &str) -> Vec<Node> {
  let mut nodes = vec![
    mask_properties_gradient(),
    mask_properties_edge(),
    decl("mask-image", MASK_IMAGE),
    decl("mask-composite", "intersect"),
    decl(
      "--tw-mask-linear",
      "var(--tw-mask-left), var(--tw-mask-right), var(--tw-mask-bottom), var(--tw-mask-top)",
    ),
  ];
  for (i, edge) in ["top", "right", "bottom", "left"].iter().enumerate() {
    if !edges[i] {
      continue;
    }
    nodes.push(decl(
      format!("--tw-mask-{edge}"),
      format!("linear-gradient(to {edge}, var(--tw-mask-{edge}-from-color) var(--tw-mask-{edge}-from-position), var(--tw-mask-{edge}-to-color) var(--tw-mask-{edge}-to-position))"),
    ));
    nodes.push(at_root(vec![
      property(&format!("--tw-mask-{edge}-from-position"), Some("0%"), None),
      property(&format!("--tw-mask-{edge}-to-position"), Some("100%"), None),
      property(&format!("--tw-mask-{edge}-from-color"), Some("black"), None),
      property(&format!("--tw-mask-{edge}-to-color"), Some("transparent"), None),
    ]));
    nodes.push(decl(format!("--tw-mask-{edge}-{var_suffix}"), value));
  }
  nodes
}

fn mask_edge_utility(u: &mut Utilities, name: &str, stop: &'static str, edges: [bool; 4]) {
  mask_stop_utility(
    u,
    name,
    Rc::new(move |v| edge_nodes(edges, &format!("{stop}-color"), v)),
    Rc::new(move |v| edge_nodes(edges, &format!("{stop}-position"), v)),
  );
}

fn degrees(value: &str, negative: bool) -> Option<String> {
  if !is_positive_integer(value) {
    return None;
  }
  let n = crate::utils::js::to_number(value);
  if n == 0.0 {
    return Some("0deg".to_owned());
  }
  if n == 1.0 {
    return Some(if negative { "-1deg" } else { "1deg" }.to_owned());
  }
  Some(if negative {
    format!("calc(1deg * -{value})")
  } else {
    format!("calc(1deg * {value})")
  })
}

/// A mask gradient's stop utilities: the shared declarations, the stops variable and the
/// gradient that reads it.
#[derive(Clone, Copy)]
struct Stops {
  base: fn() -> Vec<Node>,
  stops_var: &'static str,
  stops: &'static str,
  gradient_var: &'static str,
  gradient: &'static str,
}

impl Stops {
  fn utility(self, u: &mut Utilities, root: &str, color_var: &'static str, position_var: &'static str) {
    let make = move |var: &'static str| -> StopFn {
      Rc::new(move |v| {
        let mut nodes = (self.base)();
        nodes.push(decl(self.stops_var, self.stops));
        nodes.push(decl(
          self.gradient_var,
          format!("{}(var({}))", self.gradient, self.stops_var),
        ));
        nodes.push(decl(var, v));
        nodes
      })
    };
    mask_stop_utility(u, root, make(color_var), make(position_var));
  }
}

fn mask_properties_linear() -> Node {
  at_root(vec![
    property("--tw-mask-linear-position", Some("0deg"), None),
    property("--tw-mask-linear-from-position", Some("0%"), None),
    property("--tw-mask-linear-to-position", Some("100%"), None),
    property("--tw-mask-linear-from-color", Some("black"), None),
    property("--tw-mask-linear-to-color", Some("transparent"), None),
  ])
}

fn mask_properties_radial() -> Node {
  at_root(vec![
    property("--tw-mask-radial-from-position", Some("0%"), None),
    property("--tw-mask-radial-to-position", Some("100%"), None),
    property("--tw-mask-radial-from-color", Some("black"), None),
    property("--tw-mask-radial-to-color", Some("transparent"), None),
    property("--tw-mask-radial-shape", Some("ellipse"), None),
    property("--tw-mask-radial-size", Some("farthest-corner"), None),
    property("--tw-mask-radial-position", Some("center"), None),
  ])
}

fn mask_properties_conic() -> Node {
  at_root(vec![
    property("--tw-mask-conic-position", Some("0deg"), None),
    property("--tw-mask-conic-from-position", Some("0%"), None),
    property("--tw-mask-conic-to-position", Some("100%"), None),
    property("--tw-mask-conic-from-color", Some("black"), None),
    property("--tw-mask-conic-to-color", Some("transparent"), None),
  ])
}

fn linear_base() -> Vec<Node> {
  vec![
    mask_properties_gradient(),
    mask_properties_linear(),
    decl("mask-image", MASK_IMAGE),
    decl("mask-composite", "intersect"),
  ]
}

fn radial_base() -> Vec<Node> {
  vec![
    mask_properties_gradient(),
    mask_properties_radial(),
    decl("mask-image", MASK_IMAGE),
    decl("mask-composite", "intersect"),
  ]
}

fn conic_base() -> Vec<Node> {
  vec![
    mask_properties_gradient(),
    mask_properties_conic(),
    decl("mask-image", MASK_IMAGE),
    decl("mask-composite", "intersect"),
  ]
}

pub fn register(u: &mut Utilities) {
  mask_edge_utility(u, "mask-x-from", "from", [false, true, false, true]);
  mask_edge_utility(u, "mask-x-to", "to", [false, true, false, true]);
  mask_edge_utility(u, "mask-y-from", "from", [true, false, true, false]);
  mask_edge_utility(u, "mask-y-to", "to", [true, false, true, false]);
  mask_edge_utility(u, "mask-t-from", "from", [true, false, false, false]);
  mask_edge_utility(u, "mask-t-to", "to", [true, false, false, false]);
  mask_edge_utility(u, "mask-r-from", "from", [false, true, false, false]);
  mask_edge_utility(u, "mask-r-to", "to", [false, true, false, false]);
  mask_edge_utility(u, "mask-b-from", "from", [false, false, true, false]);
  mask_edge_utility(u, "mask-b-to", "to", [false, false, true, false]);
  mask_edge_utility(u, "mask-l-from", "from", [false, false, false, true]);
  mask_edge_utility(u, "mask-l-to", "to", [false, false, false, true]);

  functional_utility(
    u,
    "mask-linear",
    Desc::new(|v, _, _| {
      let mut nodes = linear_base();
      nodes.push(decl(
        "--tw-mask-linear",
        "linear-gradient(var(--tw-mask-linear-stops, var(--tw-mask-linear-position)))",
      ));
      nodes.push(decl("--tw-mask-linear-position", v));
      Some(nodes)
    })
    .default_null()
    .negative()
    .bare(|v, _, _| degrees(v, false))
    .negative_bare(|v, _, _| degrees(v, true)),
  );

  const LINEAR_STOPS: &str = "var(--tw-mask-linear-position), var(--tw-mask-linear-from-color) var(--tw-mask-linear-from-position), var(--tw-mask-linear-to-color) var(--tw-mask-linear-to-position)";
  Stops {
    base: linear_base,
    stops_var: "--tw-mask-linear-stops",
    stops: LINEAR_STOPS,
    gradient_var: "--tw-mask-linear",
    gradient: "linear-gradient",
  }
  .utility(
    u,
    "mask-linear-from",
    "--tw-mask-linear-from-color",
    "--tw-mask-linear-from-position",
  );
  Stops {
    base: linear_base,
    stops_var: "--tw-mask-linear-stops",
    stops: LINEAR_STOPS,
    gradient_var: "--tw-mask-linear",
    gradient: "linear-gradient",
  }
  .utility(
    u,
    "mask-linear-to",
    "--tw-mask-linear-to-color",
    "--tw-mask-linear-to-position",
  );

  for (name, value) in [("mask-circle", "circle"), ("mask-ellipse", "ellipse")] {
    static_decls(u, name, &[("--tw-mask-radial-shape", value)]);
  }
  for (name, value) in [
    ("mask-radial-closest-side", "closest-side"),
    ("mask-radial-farthest-side", "farthest-side"),
    ("mask-radial-closest-corner", "closest-corner"),
    ("mask-radial-farthest-corner", "farthest-corner"),
  ] {
    static_decls(u, name, &[("--tw-mask-radial-size", value)]);
  }
  for (name, value) in [
    ("mask-radial-at-top", "top"),
    ("mask-radial-at-top-left", "top left"),
    ("mask-radial-at-top-right", "top right"),
    ("mask-radial-at-bottom", "bottom"),
    ("mask-radial-at-bottom-left", "bottom left"),
    ("mask-radial-at-bottom-right", "bottom right"),
    ("mask-radial-at-left", "left"),
    ("mask-radial-at-right", "right"),
    ("mask-radial-at-center", "center"),
  ] {
    static_decls(u, name, &[("--tw-mask-radial-position", value)]);
  }
  functional_utility(
    u,
    "mask-radial-at",
    Desc::new(|v, _, _| Some(decls("--tw-mask-radial-position", v))).default_null(),
  );

  functional_utility(
    u,
    "mask-radial",
    Desc::new(|v, _, _| {
      let mut nodes = radial_base();
      nodes.push(decl(
        "--tw-mask-radial",
        "radial-gradient(var(--tw-mask-radial-stops, var(--tw-mask-radial-size)))",
      ));
      nodes.push(decl("--tw-mask-radial-size", v));
      Some(nodes)
    })
    .default_null(),
  );

  const RADIAL_STOPS: &str = "var(--tw-mask-radial-shape) var(--tw-mask-radial-size) at var(--tw-mask-radial-position), var(--tw-mask-radial-from-color) var(--tw-mask-radial-from-position), var(--tw-mask-radial-to-color) var(--tw-mask-radial-to-position)";
  Stops {
    base: radial_base,
    stops_var: "--tw-mask-radial-stops",
    stops: RADIAL_STOPS,
    gradient_var: "--tw-mask-radial",
    gradient: "radial-gradient",
  }
  .utility(
    u,
    "mask-radial-from",
    "--tw-mask-radial-from-color",
    "--tw-mask-radial-from-position",
  );
  Stops {
    base: radial_base,
    stops_var: "--tw-mask-radial-stops",
    stops: RADIAL_STOPS,
    gradient_var: "--tw-mask-radial",
    gradient: "radial-gradient",
  }
  .utility(
    u,
    "mask-radial-to",
    "--tw-mask-radial-to-color",
    "--tw-mask-radial-to-position",
  );

  functional_utility(
    u,
    "mask-conic",
    Desc::new(|v, _, _| {
      let mut nodes = conic_base();
      nodes.push(decl(
        "--tw-mask-conic",
        "conic-gradient(var(--tw-mask-conic-stops, var(--tw-mask-conic-position)))",
      ));
      nodes.push(decl("--tw-mask-conic-position", v));
      Some(nodes)
    })
    .default_null()
    .negative()
    .bare(|v, _, _| degrees(v, false))
    .negative_bare(|v, _, _| degrees(v, true)),
  );

  const CONIC_STOPS: &str = "from var(--tw-mask-conic-position), var(--tw-mask-conic-from-color) var(--tw-mask-conic-from-position), var(--tw-mask-conic-to-color) var(--tw-mask-conic-to-position)";
  Stops {
    base: conic_base,
    stops_var: "--tw-mask-conic-stops",
    stops: CONIC_STOPS,
    gradient_var: "--tw-mask-conic",
    gradient: "conic-gradient",
  }
  .utility(
    u,
    "mask-conic-from",
    "--tw-mask-conic-from-color",
    "--tw-mask-conic-from-position",
  );
  Stops {
    base: conic_base,
    stops_var: "--tw-mask-conic-stops",
    stops: CONIC_STOPS,
    gradient_var: "--tw-mask-conic",
    gradient: "conic-gradient",
  }
  .utility(
    u,
    "mask-conic-to",
    "--tw-mask-conic-to-color",
    "--tw-mask-conic-to-position",
  );

  static_decls(
    u,
    "box-decoration-slice",
    &[
      ("-webkit-box-decoration-break", "slice"),
      ("box-decoration-break", "slice"),
    ],
  );
  static_decls(
    u,
    "box-decoration-clone",
    &[
      ("-webkit-box-decoration-break", "clone"),
      ("box-decoration-break", "clone"),
    ],
  );

  static_decls(u, "bg-clip-text", &[("background-clip", "text")]);
  static_decls(u, "bg-clip-border", &[("background-clip", "border-box")]);
  static_decls(u, "bg-clip-padding", &[("background-clip", "padding-box")]);
  static_decls(u, "bg-clip-content", &[("background-clip", "content-box")]);
  static_decls(u, "bg-origin-border", &[("background-origin", "border-box")]);
  static_decls(u, "bg-origin-padding", &[("background-origin", "padding-box")]);
  static_decls(u, "bg-origin-content", &[("background-origin", "content-box")]);

  for value in [
    "normal",
    "multiply",
    "screen",
    "overlay",
    "darken",
    "lighten",
    "color-dodge",
    "color-burn",
    "hard-light",
    "soft-light",
    "difference",
    "exclusion",
    "hue",
    "saturation",
    "color",
    "luminosity",
  ] {
    static_decls(u, &format!("bg-blend-{value}"), &[("background-blend-mode", value)]);
    static_decls(u, &format!("mix-blend-{value}"), &[("mix-blend-mode", value)]);
  }
  static_decls(u, "mix-blend-plus-darker", &[("mix-blend-mode", "plus-darker")]);
  static_decls(u, "mix-blend-plus-lighter", &[("mix-blend-mode", "plus-lighter")]);

  static_decls(u, "fill-none", &[("fill", "none")]);
  u.functional("fill", |candidate, ds| match candidate.value() {
    None => Compiled::Skip,
    Some(UtilityValue::Arbitrary { value, .. }) => match as_color(value, candidate.modifier(), &ds.theme) {
      Some(v) => Compiled::Nodes(decls("fill", &v)),
      None => Compiled::Skip,
    },
    Some(UtilityValue::Named { .. }) => match resolve_theme_color(candidate, ds, &["--fill", "--color"]) {
      Some(v) if !v.is_empty() => Compiled::Nodes(decls("fill", &v)),
      _ => Compiled::Skip,
    },
  });

  static_decls(u, "stroke-none", &[("stroke", "none")]);
  u.functional("stroke", |candidate, ds| {
    let Some(value) = candidate.value() else {
      return Compiled::Skip;
    };
    match value {
      UtilityValue::Arbitrary { value, data_type } => {
        let ty = match data_type {
          Some(t) => Some(t.clone()),
          None => infer_data_type(
            value,
            &[
              DataType::Color,
              DataType::Number,
              DataType::Length,
              DataType::Percentage,
            ],
          )
          .map(|t| t.name().to_owned()),
        };
        match ty.as_deref() {
          Some("number") | Some("length") | Some("percentage") => {
            if candidate.modifier().is_some() {
              return Compiled::Skip;
            }
            Compiled::Nodes(decls("stroke-width", value))
          }
          _ => match as_color(value, candidate.modifier(), &ds.theme) {
            Some(v) => Compiled::Nodes(decls("stroke", &v)),
            None => Compiled::Skip,
          },
        }
      }
      UtilityValue::Named { value, .. } => {
        if let Some(v) = resolve_theme_color(candidate, ds, &["--stroke", "--color"])
          && !v.is_empty()
        {
          return Compiled::Nodes(decls("stroke", &v));
        }
        if let Some(v) = ds.theme.resolve(Some(value), &["--stroke-width"], ThemeOptions::NONE)
          && !v.is_empty()
        {
          return Compiled::Nodes(decls("stroke-width", &v));
        }
        if is_positive_integer(value) {
          return Compiled::Nodes(decls("stroke-width", value));
        }
        Compiled::Skip
      }
    }
  });

  for (name, value) in [
    ("object-contain", "contain"),
    ("object-cover", "cover"),
    ("object-fill", "fill"),
    ("object-none", "none"),
    ("object-scale-down", "scale-down"),
  ] {
    static_decls(u, name, &[("object-fit", value)]);
  }
  functional_utility(
    u,
    "object",
    Desc::new(|v, _, _| Some(decls("object-position", v)))
      .keys(&["--object-position"])
      .statics(
        [
          ("top", "top"),
          ("top-left", "left top"),
          ("top-right", "right top"),
          ("bottom", "bottom"),
          ("bottom-left", "left bottom"),
          ("bottom-right", "right bottom"),
          ("left", "left"),
          ("right", "right"),
          ("center", "center"),
        ]
        .into_iter()
        .map(|(k, v)| (k, decls("object-position", v)))
        .collect(),
      ),
  );

  for (name, prop) in [
    ("p", "padding"),
    ("px", "padding-inline"),
    ("py", "padding-block"),
    ("ps", "padding-inline-start"),
    ("pe", "padding-inline-end"),
    ("pbs", "padding-block-start"),
    ("pbe", "padding-block-end"),
    ("pt", "padding-top"),
    ("pr", "padding-right"),
    ("pb", "padding-bottom"),
    ("pl", "padding-left"),
  ] {
    spacing_utility(
      u,
      name,
      &["--padding", "--spacing"],
      move |v| decls(prop, v),
      false,
      false,
      Vec::new(),
    );
  }

  for (name, value) in [
    ("left", "left"),
    ("center", "center"),
    ("right", "right"),
    ("justify", "justify"),
    ("start", "start"),
    ("end", "end"),
  ] {
    static_decls(u, &format!("text-{name}"), &[("text-align", value)]);
  }

  spacing_utility(
    u,
    "indent",
    &["--text-indent", "--spacing"],
    |v| decls("text-indent", v),
    true,
    false,
    Vec::new(),
  );

  for (name, value) in [
    ("baseline", "baseline"),
    ("top", "top"),
    ("middle", "middle"),
    ("bottom", "bottom"),
    ("text-top", "text-top"),
    ("text-bottom", "text-bottom"),
    ("sub", "sub"),
    ("super", "super"),
  ] {
    static_decls(u, &format!("align-{name}"), &[("vertical-align", value)]);
  }
  functional_utility(
    u,
    "align",
    Desc::new(|v, _, _| Some(decls("vertical-align", v))).keys(&[]),
  );

  u.functional("font", |candidate, ds| {
    let Some(value) = candidate.value() else {
      return Compiled::Skip;
    };
    if candidate.modifier().is_some() {
      return Compiled::Skip;
    }
    match value {
      UtilityValue::Arbitrary { value, data_type } => {
        let ty = match data_type {
          Some(t) => Some(t.clone()),
          None => infer_data_type(value, &[DataType::Number, DataType::GenericName, DataType::FamilyName])
            .map(|t| t.name().to_owned()),
        };
        match ty.as_deref() {
          Some("generic-name") | Some("family-name") => Compiled::Nodes(decls("font-family", value)),
          _ => Compiled::Nodes(vec![
            at_root(vec![property("--tw-font-weight", None, None)]),
            decl("--tw-font-weight", value.clone()),
            decl("font-weight", value.clone()),
          ]),
        }
      }
      UtilityValue::Named { value, .. } => {
        if let Some((families, options)) = ds.theme.resolve_with(
          value,
          &["--font"],
          &["--font-feature-settings", "--font-variation-settings"],
        ) {
          let get = |key: &str| options.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
          return Compiled::Nodes(vec![
            decl("font-family", families),
            Node::Declaration {
              property: "font-feature-settings".to_owned(),
              value: get("--font-feature-settings"),
              important: false,
            },
            Node::Declaration {
              property: "font-variation-settings".to_owned(),
              value: get("--font-variation-settings"),
              important: false,
            },
          ]);
        }
        if let Some(v) = ds.theme.resolve(Some(value), &["--font-weight"], ThemeOptions::NONE)
          && !v.is_empty()
        {
          return Compiled::Nodes(vec![
            at_root(vec![property("--tw-font-weight", None, None)]),
            decl("--tw-font-weight", v.clone()),
            decl("font-weight", v),
          ]);
        }
        Compiled::Skip
      }
    }
  });
}
