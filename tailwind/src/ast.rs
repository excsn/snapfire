//! The CSS tree Tailwind works on, its printer and the pass that prepares it for printing.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::design_system::DesignSystem;
use crate::theme::ThemeOptions;
use crate::utils::variables::extract_used_variables;
use crate::value_parser::{self, ValueNode};
use crate::walk::Action;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CtxValue {
  Bool(bool),
  Str(String),
}

impl CtxValue {
  pub fn truthy(&self) -> bool {
    match self {
      CtxValue::Bool(b) => *b,
      CtxValue::Str(s) => !s.is_empty(),
    }
  }

  pub fn as_str(&self) -> Option<&str> {
    match self {
      CtxValue::Str(s) => Some(s),
      CtxValue::Bool(_) => None,
    }
  }
}

pub type ContextMap = BTreeMap<String, CtxValue>;

/// The context key that marks where `@tailwind utilities` stood.
pub(crate) const UTILITIES_SLOT: &str = "@tailwind utilities";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
  Rule {
    selector: String,
    nodes: Vec<Node>,
  },
  AtRule {
    name: String,
    params: String,
    nodes: Vec<Node>,
  },
  Declaration {
    property: String,
    value: Option<String>,
    important: bool,
  },
  Comment(String),
  Context {
    context: ContextMap,
    nodes: Vec<Node>,
  },
  AtRoot {
    nodes: Vec<Node>,
  },
}

impl Node {
  pub fn children(&self) -> Option<&Vec<Node>> {
    match self {
      Node::Rule { nodes, .. } | Node::AtRule { nodes, .. } | Node::Context { nodes, .. } | Node::AtRoot { nodes } => {
        Some(nodes)
      }
      Node::Declaration { .. } | Node::Comment(_) => None,
    }
  }

  pub fn children_mut(&mut self) -> Option<&mut Vec<Node>> {
    match self {
      Node::Rule { nodes, .. } | Node::AtRule { nodes, .. } | Node::Context { nodes, .. } | Node::AtRoot { nodes } => {
        Some(nodes)
      }
      Node::Declaration { .. } | Node::Comment(_) => None,
    }
  }

  pub fn is_rule_or_at_rule(&self) -> bool {
    matches!(self, Node::Rule { .. } | Node::AtRule { .. })
  }

  pub fn is_at_rule(&self, at: &str) -> bool {
    matches!(self, Node::AtRule { name, .. } if name == at)
  }
}

pub fn style_rule(selector: impl Into<String>, nodes: Vec<Node>) -> Node {
  Node::Rule {
    selector: selector.into(),
    nodes,
  }
}

pub fn at_rule(name: impl Into<String>, params: impl Into<String>, nodes: Vec<Node>) -> Node {
  Node::AtRule {
    name: name.into(),
    params: params.into(),
    nodes,
  }
}

/// A style rule. An at-rule when `selector` starts with `@`.
pub fn rule(selector: &str, nodes: Vec<Node>) -> Node {
  if selector.starts_with('@') {
    return crate::css_parser::parse_at_rule(selector, nodes);
  }
  style_rule(selector, nodes)
}

pub fn decl(property: impl Into<String>, value: impl Into<String>) -> Node {
  Node::Declaration {
    property: property.into(),
    value: Some(value.into()),
    important: false,
  }
}

pub fn comment(value: impl Into<String>) -> Node {
  Node::Comment(value.into())
}

pub fn context(context: ContextMap, nodes: Vec<Node>) -> Node {
  Node::Context { context, nodes }
}

pub fn at_root(nodes: Vec<Node>) -> Node {
  Node::AtRoot { nodes }
}

pub fn ctx1(key: &str, value: CtxValue) -> ContextMap {
  let mut map = ContextMap::new();
  map.insert(key.to_owned(), value);
  map
}

pub fn to_css(ast: &[Node]) -> String {
  let mut css = String::new();
  for node in ast {
    stringify(node, 0, &mut css);
  }
  css
}

fn stringify(node: &Node, depth: usize, css: &mut String) {
  let indent = "  ".repeat(depth);
  match node {
    Node::Declaration {
      property,
      value,
      important,
    } => {
      css.push_str(&indent);
      css.push_str(property);
      css.push_str(": ");
      match value {
        Some(v) => css.push_str(v),
        None => css.push_str("undefined"),
      }
      if *important {
        css.push_str(" !important");
      }
      css.push_str(";\n");
    }
    Node::Rule { selector, nodes } => {
      css.push_str(&indent);
      css.push_str(selector);
      css.push_str(" {\n");
      for child in nodes {
        stringify(child, depth + 1, css);
      }
      css.push_str(&indent);
      css.push_str("}\n");
    }
    Node::AtRule { name, params, nodes } => {
      if nodes.is_empty() {
        css.push_str(&indent);
        css.push_str(name);
        css.push(' ');
        css.push_str(params);
        css.push_str(";\n");
        return;
      }
      css.push_str(&indent);
      css.push_str(name);
      if params.is_empty() {
        css.push(' ');
      } else {
        css.push(' ');
        css.push_str(params);
        css.push(' ');
      }
      css.push_str("{\n");
      for child in nodes {
        stringify(child, depth + 1, css);
      }
      css.push_str(&indent);
      css.push_str("}\n");
    }
    Node::Comment(value) => {
      css.push_str(&indent);
      css.push_str("/*");
      css.push_str(value);
      css.push_str("*/\n");
    }
    Node::Context { .. } | Node::AtRoot { .. } => {}
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Polyfills(pub u8);

impl Polyfills {
  pub const NONE: Polyfills = Polyfills(0);
  pub const AT_PROPERTY: Polyfills = Polyfills(1);
  pub const COLOR_MIX: Polyfills = Polyfills(2);
  pub const ALL: Polyfills = Polyfills(3);

  pub fn has(self, other: Polyfills) -> bool {
    self.0 & other.0 != 0
  }
}

#[derive(Clone, Default)]
struct Ctx {
  theme: bool,
  keyframes: bool,
  supports_color_mix: bool,
}

impl Ctx {
  fn merge(&self, map: &ContextMap) -> Ctx {
    let mut next = self.clone();
    if let Some(v) = map.get("theme") {
      next.theme = v.truthy();
    }
    next
  }
}

fn extract_keyframe_names(value: &str) -> impl Iterator<Item = &str> {
  value.split(|c: char| c.is_whitespace() || c == ',')
}

/// What the first pass learns: which theme variables are referenced and by whom, plus the
/// keyframes an `animation` names.
#[derive(Default)]
struct Usage {
  dependencies: HashMap<String, HashSet<String>>,
  keyframes: HashSet<String>,
  seen_properties: HashSet<String>,
}

fn scan(node: &Node, ctx: &Ctx, depth: usize, ds: &DesignSystem, usage: &mut Usage) {
  match node {
    Node::Declaration { property, value, .. } => {
      let Some(value) = value else { return };
      if property == "--tw-sort" {
        return;
      }
      let custom = property.starts_with("--");
      if ctx.theme && custom && value == "initial" {
        return;
      }
      if value.contains("var(") {
        if ctx.theme && custom {
          for variable in extract_used_variables(value) {
            usage.dependencies.entry(variable).or_default().insert(property.clone());
          }
        } else {
          ds.track_used_variables(value);
        }
      }
      if property == "animation" {
        for name in extract_keyframe_names(value) {
          usage.keyframes.insert(name.to_owned());
        }
      }
    }
    Node::Rule { nodes, .. } => {
      for child in nodes {
        scan(child, ctx, depth + 1, ds, usage);
      }
    }
    Node::AtRule { name, params, nodes } if name == "@property" && depth == 0 => {
      if !usage.seen_properties.insert(params.clone()) {
        return;
      }
      for child in nodes {
        scan(child, ctx, depth + 1, ds, usage);
      }
    }
    Node::AtRule { name, params, nodes } => {
      let mut ctx = ctx.clone();
      if name == "@keyframes" {
        ctx.keyframes = true;
      } else if name == "@supports" && params.contains("color-mix(") {
        ctx.supports_color_mix = true;
      }
      for child in nodes {
        scan(child, &ctx, depth + 1, ds, usage);
      }
    }
    Node::AtRoot { nodes } => {
      for child in nodes {
        scan(child, ctx, 0, ds, usage);
      }
    }
    Node::Context { context, nodes } => {
      if context.get("reference").is_some_and(CtxValue::truthy) {
        return;
      }
      let ctx = ctx.merge(context);
      for child in nodes {
        scan(child, &ctx, depth, ds, usage);
      }
    }
    Node::Comment(_) => {}
  }
}

fn is_variable_used(
  variable: &str,
  ds: &DesignSystem,
  dependencies: &HashMap<String, HashSet<String>>,
  seen: &mut HashSet<String>,
) -> bool {
  if !seen.insert(variable.to_owned()) {
    return true;
  }
  let options = ds.theme.get_options(variable);
  if options.has(ThemeOptions::STATIC) || options.has(ThemeOptions::USED) {
    return true;
  }
  if let Some(deps) = dependencies.get(variable) {
    for dependency in deps {
      if is_variable_used(dependency, ds, dependencies, seen) {
        return true;
      }
    }
  }
  false
}

/// The outcome of transforming one node into its parent's list.
enum Removal {
  None,
  /// A rule lost its last theme variable: it and every at-rule above it go.
  Chain,
}

struct Transform<'a> {
  ds: &'a DesignSystem,
  polyfills: Polyfills,
  used_theme: HashMap<String, bool>,
  used_keyframes: HashSet<String>,
  seen_properties: HashSet<String>,
  at_roots: Vec<(Node, bool)>,
  fallbacks_root: Vec<Node>,
  fallbacks_universal: Vec<Node>,
}

impl Transform<'_> {
  fn theme_variable_kept(&mut self, property: &str) -> bool {
    if let Some(kept) = self.used_theme.get(property) {
      return *kept;
    }
    true
  }

  /// Transforms `node` into `parent`. Returns whether the node pushed theme variables
  /// into `parent` and whether a removal must travel upward.
  fn transform(
    &mut self,
    node: &Node,
    parent: &mut Vec<Node>,
    ctx: &Ctx,
    depth: usize,
    theme_vars: &mut usize,
    removed: &mut usize,
  ) -> Removal {
    match node {
      Node::Declaration {
        property,
        value,
        important,
      } => {
        let Some(value) = value else { return Removal::None };
        if property == "--tw-sort" {
          return Removal::None;
        }
        let custom = property.starts_with("--");
        if ctx.theme && custom {
          if value == "initial" {
            return Removal::None;
          }
          if !ctx.keyframes {
            *theme_vars += 1;
            if !self.theme_variable_kept(property) {
              *removed += 1;
              return Removal::None;
            }
          }
        }
        let decl = Node::Declaration {
          property: property.clone(),
          value: Some(value.clone()),
          important: *important,
        };
        if self.polyfills.has(Polyfills::COLOR_MIX)
          && value.contains("color-mix(")
          && !ctx.supports_color_mix
          && !ctx.keyframes
        {
          parent.extend(self.color_mix_fallback(decl));
        } else {
          parent.push(decl);
        }
        Removal::None
      }
      Node::Rule { selector, nodes } => {
        let mut children = Vec::new();
        let mut vars = 0;
        let mut gone = 0;
        for child in nodes {
          self.transform(child, &mut children, ctx, depth + 1, &mut vars, &mut gone);
        }

        dedupe_declarations(&mut children);

        let emptied_by_removal = vars > 0 && gone > 0 && children.is_empty();
        if children.is_empty() {
          if emptied_by_removal {
            return Removal::Chain;
          }
          return Removal::None;
        }
        if selector == "&" {
          parent.extend(children);
        } else {
          parent.push(Node::Rule {
            selector: selector.clone(),
            nodes: children,
          });
        }
        Removal::None
      }
      Node::AtRule { name, params, nodes } if name == "@property" && depth == 0 => {
        if !self.seen_properties.insert(params.clone()) {
          return Removal::None;
        }
        if self.polyfills.has(Polyfills::AT_PROPERTY) {
          let mut initial = None;
          let mut inherits = false;
          for prop in nodes {
            if let Node::Declaration { property, value, .. } = prop {
              if property == "initial-value" {
                initial = value.clone();
              } else if property == "inherits" {
                inherits = value.as_deref() == Some("true");
              }
            }
          }
          let fallback = decl(params.clone(), initial.unwrap_or_else(|| "initial".to_owned()));
          if inherits {
            self.fallbacks_root.push(fallback);
          } else {
            self.fallbacks_universal.push(fallback);
          }
        }
        let mut children = Vec::new();
        let mut vars = 0;
        let mut gone = 0;
        for child in nodes {
          self.transform(child, &mut children, ctx, depth + 1, &mut vars, &mut gone);
        }
        parent.push(Node::AtRule {
          name: name.clone(),
          params: params.clone(),
          nodes: children,
        });
        Removal::None
      }
      Node::AtRule { name, params, nodes } => {
        let mut ctx = ctx.clone();
        if name == "@keyframes" {
          ctx.keyframes = true;
        } else if name == "@supports" && params.contains("color-mix(") {
          ctx.supports_color_mix = true;
        }
        let mut children = Vec::new();
        let mut vars = 0;
        let mut gone = 0;
        let mut chain = false;
        for child in nodes {
          if let Removal::Chain = self.transform(child, &mut children, &ctx, depth + 1, &mut vars, &mut gone) {
            chain = true;
          }
        }
        if chain {
          return Removal::Chain;
        }
        let keep = !children.is_empty()
          || matches!(
            name.as_str(),
            "@layer" | "@charset" | "@custom-media" | "@namespace" | "@import" | "@apply"
          );
        if keep {
          parent.push(Node::AtRule {
            name: name.clone(),
            params: params.clone(),
            nodes: children,
          });
        }
        Removal::None
      }
      Node::AtRoot { nodes } => {
        for child in nodes {
          let mut out = Vec::new();
          let mut vars = 0;
          let mut gone = 0;
          let theme_keyframes = ctx.theme && child.is_at_rule("@keyframes");
          self.transform(child, &mut out, ctx, 0, &mut vars, &mut gone);
          for node in out {
            let tracked = theme_keyframes && node.is_at_rule("@keyframes");
            self.at_roots.push((node, tracked));
          }
        }
        Removal::None
      }
      Node::Context { context, nodes } => {
        if context.get("reference").is_some_and(CtxValue::truthy) {
          return Removal::None;
        }
        let ctx = ctx.merge(context);
        let mut chain = false;
        for child in nodes {
          if let Removal::Chain = self.transform(child, parent, &ctx, depth, theme_vars, removed) {
            chain = true;
          }
        }
        if chain { Removal::Chain } else { Removal::None }
      }
      Node::Comment(v) => {
        parent.push(Node::Comment(v.clone()));
        Removal::None
      }
    }
  }

  fn color_mix_fallback(&self, declaration: Node) -> Vec<Node> {
    let Node::Declaration {
      property,
      value: Some(value),
      important,
    } = &declaration
    else {
      return vec![declaration];
    };
    let mut ast = value_parser::parse(value);
    let mut requires_polyfill = false;
    let theme = &self.ds.theme;

    value_parser::walk(&mut ast, &mut |node, _| {
      let ValueNode::Function { value: fname, nodes } = node else {
        return Action::Continue;
      };
      if fname != "color-mix" {
        return Action::Continue;
      }

      let mut contains_unresolvable = false;
      let mut contains_currentcolor = false;

      value_parser::walk(nodes, &mut |inner, _| {
        if let ValueNode::Word(w) = inner
          && w.eq_ignore_ascii_case("currentcolor")
        {
          contains_currentcolor = true;
          requires_polyfill = true;
          return Action::Continue;
        }

        let mut var_node: Option<ValueNode> = Some(inner.clone());
        let mut inlined: Option<String>;
        let mut seen: HashSet<String> = HashSet::new();
        loop {
          let Some(ValueNode::Function { value: v, nodes: args }) = &var_node else {
            return Action::Continue;
          };
          if v != "var" {
            return Action::Continue;
          }
          let Some(ValueNode::Word(name)) = args.first() else {
            return Action::Continue;
          };
          let name = name.clone();
          if !seen.insert(name.clone()) {
            contains_unresolvable = true;
            return Action::Continue;
          }
          requires_polyfill = true;
          inlined = theme.resolve_value(None, &[name.as_str()]);
          let Some(color) = &inlined else {
            contains_unresolvable = true;
            return Action::Continue;
          };
          if color.eq_ignore_ascii_case("currentcolor") {
            contains_currentcolor = true;
            return Action::Continue;
          }
          if color.starts_with("var(") {
            var_node = value_parser::parse(color).into_iter().next();
          } else {
            var_node = None;
          }
          if var_node.is_none() {
            break;
          }
        }
        Action::Replace(vec![ValueNode::Word(inlined.unwrap_or_default())])
      });

      if contains_unresolvable || contains_currentcolor {
        let separator = nodes
          .iter()
          .position(|n| matches!(n, ValueNode::Separator(s) if s.trim().contains(',')));
        let Some(separator) = separator else {
          return Action::Continue;
        };
        let Some(first) = nodes.get(separator + 1).cloned() else {
          return Action::Continue;
        };
        return Action::Replace(vec![first]);
      } else if requires_polyfill
        && let Some(ValueNode::Word(space)) = nodes.get_mut(2)
        && matches!(space.as_str(), "oklab" | "oklch" | "lab" | "lch")
      {
        *space = "srgb".to_owned();
      }
      Action::Continue
    });

    if !requires_polyfill {
      return vec![declaration];
    }

    let fallback = Node::Declaration {
      property: property.clone(),
      value: Some(value_parser::to_css(&ast)),
      important: *important,
    };
    vec![
      fallback,
      rule("@supports (color: color-mix(in lab, red, red))", vec![declaration]),
    ]
  }
}

/// Keeps the last of each exactly repeated declaration in a rule.
fn dedupe_declarations(nodes: &mut Vec<Node>) {
  let mut last: HashMap<(String, Option<String>, bool), usize> = HashMap::new();
  for (i, node) in nodes.iter().enumerate() {
    if let Node::Declaration {
      property,
      value,
      important,
    } = node
    {
      last.insert((property.clone(), value.clone(), *important), i);
    }
  }
  if last.len() == nodes.iter().filter(|n| matches!(n, Node::Declaration { .. })).count() {
    return;
  }
  let mut i = 0;
  nodes.retain(|node| {
    let keep = match node {
      Node::Declaration {
        property,
        value,
        important,
      } => last.get(&(property.clone(), value.clone(), *important)) == Some(&i),
      _ => true,
    };
    i += 1;
    keep
  });
}

/// Tailwind's `optimizeAst`: flattens context and at-root nodes, drops theme variables and
/// keyframes nothing uses and adds the `@property` and `color-mix()` fallbacks.
pub fn optimize_ast(ast: &[Node], ds: &DesignSystem, polyfills: Polyfills) -> Vec<Node> {
  let mut usage = Usage::default();
  let root = Ctx::default();
  for node in ast {
    scan(node, &root, 0, ds, &mut usage);
  }

  let mut used_theme = HashMap::new();
  let mut theme_vars = Vec::new();
  collect_theme_variables(ast, &root, &mut theme_vars);
  let animate = ds.theme.prefix_key("--animate-");
  for (property, value) in theme_vars {
    if used_theme.contains_key(&property) {
      continue;
    }
    let used = is_variable_used(&property, ds, &usage.dependencies, &mut HashSet::new());
    if used && property.starts_with(&animate) {
      for name in extract_keyframe_names(&value) {
        usage.keyframes.insert(name.to_owned());
      }
    }
    used_theme.insert(property, used);
  }

  let mut t = Transform {
    ds,
    polyfills,
    used_theme,
    used_keyframes: usage.keyframes,
    seen_properties: HashSet::new(),
    at_roots: Vec::new(),
    fallbacks_root: Vec::new(),
    fallbacks_universal: Vec::new(),
  };

  let mut out = Vec::new();
  for node in ast {
    let mut vars = 0;
    let mut gone = 0;
    t.transform(node, &mut out, &root, 0, &mut vars, &mut gone);
  }

  let used_keyframes = std::mem::take(&mut t.used_keyframes);
  for (node, tracked) in std::mem::take(&mut t.at_roots) {
    if tracked
      && let Node::AtRule { params, .. } = &node
      && !used_keyframes.contains(params)
    {
      continue;
    }
    out.push(node);
  }

  if polyfills.has(Polyfills::AT_PROPERTY) {
    let mut fallback_ast = Vec::new();
    if !t.fallbacks_root.is_empty() {
      fallback_ast.push(rule(":root, :host", std::mem::take(&mut t.fallbacks_root)));
    }
    if !t.fallbacks_universal.is_empty() {
      fallback_ast.push(rule(
        "*, ::before, ::after, ::backdrop",
        std::mem::take(&mut t.fallbacks_universal),
      ));
    }
    if !fallback_ast.is_empty() {
      let first = out.iter().position(|node| match node {
        Node::Comment(_) => false,
        Node::AtRule { name, .. } => name != "@charset" && name != "@import",
        _ => true,
      });
      let statement = at_rule("@layer", "properties", Vec::new());
      match first {
        Some(i) => out.insert(i, statement),
        None => out.push(statement),
      }
      out.push(rule(
        "@layer properties",
        vec![at_rule(
          "@supports",
          "((-webkit-hyphens: none) and (not (margin-trim: inline))) or ((-moz-orient: inline) and (not (color:rgb(from red r g b))))",
          fallback_ast,
        )],
      ));
    }
  }

  out
}

fn collect_theme_variables(nodes: &[Node], ctx: &Ctx, out: &mut Vec<(String, String)>) {
  for node in nodes {
    match node {
      Node::Declaration {
        property,
        value: Some(value),
        ..
      } => {
        if ctx.theme && !ctx.keyframes && property.starts_with("--") && value != "initial" && property != "--tw-sort" {
          out.push((property.clone(), value.clone()));
        }
      }
      Node::Context { context, nodes } => {
        if context.get("reference").is_some_and(CtxValue::truthy) {
          continue;
        }
        collect_theme_variables(nodes, &ctx.merge(context), out);
      }
      Node::AtRule { name, nodes, .. } if name == "@keyframes" => {
        let mut inner = ctx.clone();
        inner.keyframes = true;
        collect_theme_variables(nodes, &inner, out);
      }
      Node::Rule { nodes, .. } | Node::AtRule { nodes, .. } | Node::AtRoot { nodes } => {
        collect_theme_variables(nodes, ctx, out)
      }
      _ => {}
    }
  }
}
