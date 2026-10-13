//! Tailwind CSS v4 in Rust. A stylesheet configures it with `@import "tailwindcss"`,
//! `@theme`, `@utility`, `@custom-variant` and `@source`; [`Compiler::build`] turns a set of
//! candidate class names into the stylesheet with their utilities in it.

mod apply;
mod ast;
mod at_import;
mod candidate;
mod compat;
mod compile;
mod css_functions;
mod css_parser;
mod design_system;
mod features;
mod optimize;
mod property_order;
mod theme;
mod utilities;
mod utils;
mod value_parser;
mod variants;
mod walk;

use indexmap::IndexSet;

pub use at_import::{LoadStylesheet, Loaded, TAILWIND_BASE, tailwind_stylesheet};
pub use features::Features;
pub use optimize::{OptimizeOptions, optimize};

use crate::ast::{
  CtxValue, Node, Polyfills, UTILITIES_SLOT, at_root, at_rule, context, ctx1, decl, rule, style_rule, to_css,
};
use crate::design_system::DesignSystem;
use crate::theme::{Theme, ThemeOptions};
use crate::utils::escape::{escape, unescape};
use crate::utils::segment::segment;
use crate::walk::{Action, walk};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
  #[error("{0}")]
  Syntax(String),
  #[error("{0}")]
  Message(String),
}

/// A `@source` path, relative to the stylesheet that named it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
  pub base: String,
  pub pattern: String,
  pub negated: bool,
}

/// Where `@tailwind utilities source(…)` points scanning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Root {
  None,
  Path { base: String, pattern: String },
}

pub struct CompileOptions<'a> {
  /// The directory the stylesheet's own imports resolve against.
  pub base: String,
  /// Resolves every `@import`. Without one only Tailwind's own stylesheets resolve; a loader
  /// that should also find them calls [`tailwind_stylesheet`].
  pub load_stylesheet: Option<Box<LoadStylesheet<'a>>>,
  pub polyfills: Polyfills,
  /// Starts the output with Tailwind's license comment.
  pub banner: bool,
  /// The stylesheet's file name. A syntax error then starts with `file:line:column`. An
  /// imported stylesheet's errors name its `path`.
  pub from: Option<String>,
}

impl Default for CompileOptions<'_> {
  fn default() -> Self {
    CompileOptions {
      base: String::new(),
      load_stylesheet: None,
      polyfills: Polyfills::ALL,
      banner: true,
      from: None,
    }
  }
}

pub use crate::ast::Polyfills as PolyfillFlags;

const THEME_SLOT: &str = "@theme";
const VERSION: &str = "4.3.2";

/// A compiled stylesheet. [`Compiler::build`] can be called again with more candidates.
pub struct Compiler {
  ds: DesignSystem,
  ast: Vec<Node>,
  features: Features,
  sources: Vec<Source>,
  root: Option<Root>,
  has_utilities: bool,
  polyfills: Polyfills,
  all_valid: IndexSet<String>,
  default_did_change: bool,
  previous_count: usize,
  compiled: Option<String>,
}

impl Compiler {
  pub fn new(css: &str, options: CompileOptions) -> Result<Compiler, Error> {
    let parsed = css_parser::parse(css, options.from.as_deref())?;
    let mut compiler = parse_css(parsed, &options)?;
    if options.banner {
      compiler.ast.insert(
        0,
        Node::Comment(format!(
          "! tailwindcss v{VERSION} | MIT License | https://tailwindcss.com "
        )),
      );
    }
    Ok(compiler)
  }

  pub fn features(&self) -> Features {
    self.features
  }

  pub fn sources(&self) -> &[Source] {
    &self.sources
  }

  pub fn root(&self) -> Option<&Root> {
    self.root.as_ref()
  }

  /// The stylesheet with the utilities for every candidate given to any build so far.
  pub fn build<'c>(&mut self, candidates: impl IntoIterator<Item = &'c str>) -> String {
    if self.features == Features::NONE {
      return self
        .compiled
        .get_or_insert_with(|| to_css(input_nodes(&self.ast)))
        .clone();
    }

    if !self.has_utilities {
      if self.compiled.is_none() {
        self.compiled = Some(to_css(&ast::optimize_ast(&self.ast, &self.ds, self.polyfills)));
      }
      return self.compiled.clone().unwrap();
    }

    let mut did_change = self.default_did_change;
    let mut added_external_variable = false;
    self.default_did_change = false;

    let previous = self.all_valid.len();
    for candidate in candidates {
      if self.ds.invalid_candidates.borrow().contains(candidate) {
        continue;
      }
      if candidate.starts_with("--") {
        let marked = self.ds.theme.mark_used_variable(candidate);
        did_change |= marked;
        added_external_variable |= marked;
      } else {
        self.all_valid.insert(candidate.to_owned());
        did_change |= self.all_valid.len() != previous;
      }
    }

    if !did_change && let Some(compiled) = &self.compiled {
      return compiled.clone();
    }

    if did_change {
      let ds = &self.ds;
      let compiled = compile::compile_candidates(self.all_valid.iter().map(String::as_str), ds, true, &mut |c| {
        ds.invalid_candidates.borrow_mut().insert(c.to_owned());
      });
      let new_nodes: Vec<Node> = compiled.nodes.into_iter().map(|(n, _)| n).collect();

      if added_external_variable || self.previous_count != new_nodes.len() || self.compiled.is_none() {
        self.previous_count = new_nodes.len();
        set_utilities(&mut self.ast, new_nodes);
        self.compiled = Some(to_css(&ast::optimize_ast(&self.ast, &self.ds, self.polyfills)));
      }
    } else if self.compiled.is_none() {
      self.compiled = Some(to_css(&ast::optimize_ast(&self.ast, &self.ds, self.polyfills)));
    }

    self.compiled.clone().unwrap()
  }

  /// The candidate in Tailwind's canonical spelling, from its first parse: `!flex` as `flex!`
  /// or `bg-[var(--x)]` as `bg-(--x)`. `None` when it does not parse.
  pub fn canonical_candidate(&self, candidate: &str) -> Option<String> {
    let parsed = self.ds.parse_candidate(candidate);
    parsed.first().map(|c| candidate::print_candidate(&self.ds, c))
  }

  /// Every candidate a build was given that is not a utility, a variant of one or a theme
  /// variable, sorted.
  pub fn invalid_candidates(&self) -> Vec<String> {
    let mut invalid: Vec<String> = self.ds.invalid_candidates.borrow().iter().cloned().collect();
    invalid.sort();
    invalid
  }

  /// The warnings Tailwind would print to the console, in order.
  pub fn warnings(&self) -> Vec<String> {
    self.ds.warnings.borrow().clone()
  }

  /// One candidate as CSS without polyfills. `None` when it is not a utility.
  pub fn candidate_css(&self, candidate: &str) -> Option<String> {
    self.ds.candidate_to_css(candidate)
  }
}

fn input_nodes(ast: &[Node]) -> &[Node] {
  match ast.iter().find(|n| matches!(n, Node::Context { .. })) {
    Some(Node::Context { nodes, .. }) => nodes,
    _ => ast,
  }
}

fn set_utilities(ast: &mut Vec<Node>, nodes: Vec<Node>) {
  let mut nodes = Some(nodes);
  walk(ast, &mut |node, _| {
    if let Node::Context {
      context,
      nodes: children,
    } = node
      && context.contains_key(UTILITIES_SLOT)
    {
      *children = nodes.take().unwrap_or_default();
      return Action::Stop;
    }
    Action::Continue
  });
}

fn parse_theme_options(params: &str) -> (ThemeOptions, Option<String>) {
  let mut options = ThemeOptions::NONE;
  let mut prefix = None;
  for option in segment(params, b' ') {
    match option {
      "reference" => options |= ThemeOptions::REFERENCE,
      "inline" => options |= ThemeOptions::INLINE,
      "default" => options |= ThemeOptions::DEFAULT,
      "static" => options |= ThemeOptions::STATIC,
      o if o.starts_with("prefix(") && o.ends_with(')') => prefix = Some(o[7..o.len() - 1].to_owned()),
      _ => {}
    }
  }
  (options, prefix)
}

fn is_quoted(path: &str) -> bool {
  let first = path.chars().next();
  let last = path.chars().last();
  match first {
    Some('"') => last == Some('"') && path.len() >= 2,
    Some('\'') => last == Some('\'') && path.len() >= 2,
    _ => false,
  }
}

enum CustomVariant {
  Selectors { style: Vec<String>, at_rules: Vec<String> },
  Body(Vec<Node>),
}

fn parse_css(parsed: Vec<Node>, options: &CompileOptions) -> Result<Compiler, Error> {
  let mut features = Features::NONE;
  let mut ast = vec![context(ctx1("base", CtxValue::Str(options.base.clone())), parsed)];

  let load = |id: &str, base: &str| -> Result<Loaded, Error> {
    match &options.load_stylesheet {
      Some(load) => load(id, base),
      None => tailwind_stylesheet(id, base)
        .ok_or_else(|| Error::Message(format!("Cannot resolve `{id}`: no `load_stylesheet` was given"))),
    }
  };
  features |= at_import::substitute_at_imports(&mut ast, &options.base, &load, 0, options.from.is_some())?;

  let mut important = false;
  let mut theme = Theme::default();
  let mut custom_variants: Vec<(String, CustomVariant)> = Vec::new();
  let mut variant_deps: Vec<(String, Vec<String>)> = Vec::new();
  let mut custom_utilities: Vec<(String, Vec<Node>)> = Vec::new();
  let mut has_theme_rule = false;
  let mut has_utilities = false;
  let mut sources = Vec::new();
  let mut inline_candidates: Vec<String> = Vec::new();
  let mut ignored_candidates: Vec<String> = Vec::new();
  let mut root: Option<Root> = None;
  let mut error: Option<Error> = None;

  macro_rules! fail {
    ($e:expr) => {{
      error = Some($e);
      return Action::Stop;
    }};
  }

  walk(&mut ast, &mut |node, visit| {
    let Node::AtRule { name, params, nodes } = node else {
      return Action::Continue;
    };
    let ctx = visit.context();
    let has_parent = visit.css_parent().is_some();

    if name == "@tailwind" && (params == "utilities" || params.starts_with("utilities")) {
      if has_utilities {
        return Action::Replace(Vec::new());
      }
      if ctx.get("reference").is_some_and(CtxValue::truthy) {
        return Action::Replace(Vec::new());
      }
      for param in segment(params, b' ') {
        if let Some(rest) = param.strip_prefix("source(") {
          let path = &rest[..rest.len().saturating_sub(1)];
          if path == "none" {
            root = Some(Root::None);
            continue;
          }
          if !is_quoted(path) {
            fail!(Error::Message("`source(…)` paths must be quoted.".to_owned()));
          }
          let base = ctx
            .get("sourceBase")
            .or_else(|| ctx.get("base"))
            .and_then(CtxValue::as_str)
            .unwrap_or("")
            .to_owned();
          root = Some(Root::Path {
            base,
            pattern: path[1..path.len() - 1].to_owned(),
          });
        }
      }
      has_utilities = true;
      features |= Features::UTILITIES;
      *node = context(ctx1(UTILITIES_SLOT, CtxValue::Bool(true)), Vec::new());
      return Action::Skip;
    }

    if name == "@utility" {
      if has_parent {
        fail!(Error::Message("`@utility` cannot be nested.".to_owned()));
      }
      if nodes.is_empty() {
        fail!(Error::Message(format!(
          "`@utility {params}` is empty. Utilities should include at least one property."
        )));
      }
      let utility_name = unescape(params);
      if !utilities::is_valid_functional_utility_name(&utility_name)
        && !utilities::is_valid_static_utility_name(&utility_name)
      {
        if !params.ends_with("-*") {
          if params.ends_with('*') {
            fail!(Error::Message(format!(
              "`@utility {params}` defines an invalid utility name. A functional utility must end in `-*`."
            )));
          } else if params.contains('*') {
            fail!(Error::Message(format!(
              "`@utility {params}` defines an invalid utility name. The dynamic portion marked by `-*` must appear once at the end."
            )));
          }
        }
        fail!(Error::Message(format!(
          "`@utility {params}` defines an invalid utility name. Utilities should be alphanumeric and start with a lowercase letter."
        )));
      }
      return Action::Continue;
    }

    if name == "@source" {
      if !nodes.is_empty() {
        fail!(Error::Message("`@source` cannot have a body.".to_owned()));
      }
      if has_parent {
        fail!(Error::Message("`@source` cannot be nested.".to_owned()));
      }
      let mut not = false;
      let mut inline = false;
      let mut path = params.as_str();
      if let Some(rest) = path.strip_prefix("not ") {
        not = true;
        path = rest;
      }
      if let Some(rest) = path.strip_prefix("inline(") {
        inline = true;
        path = rest[..rest.len().saturating_sub(1)].trim();
      }
      if !is_quoted(path) {
        fail!(Error::Message("`@source` paths must be quoted.".to_owned()));
      }
      let source = &path[1..path.len() - 1];
      if inline {
        for part in segment(source, b' ') {
          match utils::brace::expand(part) {
            Ok(expanded) => {
              let destination = if not {
                &mut ignored_candidates
              } else {
                &mut inline_candidates
              };
              destination.extend(expanded);
            }
            Err(e) => fail!(e),
          }
        }
      } else {
        let base = ctx.get("base").and_then(CtxValue::as_str).unwrap_or("").to_owned();
        sources.push(Source {
          base,
          pattern: source.to_owned(),
          negated: not,
        });
      }
      return Action::ReplaceSkip(Vec::new());
    }

    if name == "@variant" && !has_parent {
      if nodes.is_empty() {
        *name = "@custom-variant".to_owned();
      } else {
        let mut has_slot = false;
        walk::walk_ref(nodes, &mut |child, _| {
          if child.is_at_rule("@slot") {
            has_slot = true;
          }
          !has_slot
        });
        if has_slot {
          *name = "@custom-variant".to_owned();
        }
      }
    }

    if name == "@custom-variant" {
      if has_parent {
        fail!(Error::Message("`@custom-variant` cannot be nested.".to_owned()));
      }
      let parts = segment(params, b' ');
      let variant_name = parts[0].to_owned();
      let selector = parts.get(1).filter(|s| !s.is_empty()).map(|s| (*s).to_owned());
      if !variants::is_valid_variant_name(&variant_name) {
        fail!(Error::Message(format!(
          "`@custom-variant {variant_name}` defines an invalid variant name. Variants should only contain alphanumeric, dashes, or underscore characters and start with a lowercase letter or number."
        )));
      }
      if !nodes.is_empty() && selector.is_some() {
        fail!(Error::Message(format!(
          "`@custom-variant {variant_name}` cannot have both a selector and a body."
        )));
      }
      if nodes.is_empty() {
        let Some(selector) = selector else {
          fail!(Error::Message(format!(
            "`@custom-variant {variant_name}` has no selector or body."
          )));
        };
        let inner = selector.get(1..selector.len().saturating_sub(1)).unwrap_or("");
        let selectors: Vec<String> = segment(inner, b',').into_iter().map(str::to_owned).collect();
        if selectors.is_empty() || selectors.iter().any(|s| s.trim().is_empty()) {
          fail!(Error::Message(format!(
            "`@custom-variant {variant_name} ({})` selector is invalid.",
            selectors.join(",")
          )));
        }
        let mut style = Vec::new();
        let mut at_rules = Vec::new();
        for s in selectors {
          let s = s.trim().to_owned();
          if s.starts_with('@') {
            at_rules.push(s);
          } else {
            style.push(s);
          }
        }
        set_custom_variant(
          &mut custom_variants,
          &mut variant_deps,
          variant_name,
          CustomVariant::Selectors { style, at_rules },
          Vec::new(),
        );
      } else {
        let mut deps = Vec::new();
        walk::walk_ref(nodes, &mut |child, _| {
          if let Node::AtRule { name, params, .. } = child
            && name == "@variant"
          {
            deps.push(params.clone());
          }
          true
        });
        set_custom_variant(
          &mut custom_variants,
          &mut variant_deps,
          variant_name,
          CustomVariant::Body(nodes.clone()),
          deps,
        );
      }
      return Action::ReplaceSkip(Vec::new());
    }

    if name == "@media" {
      let parts: Vec<String> = segment(params, b' ').into_iter().map(str::to_owned).collect();
      let mut unknown: Vec<String> = Vec::new();
      for param in &parts {
        if let Some(rest) = param.strip_prefix("source(") {
          let path = &rest[..rest.len().saturating_sub(1)];
          let base = ctx.get("base").cloned();
          walk(nodes, &mut |child, _| {
            if let Node::AtRule { name, params, .. } = child
              && name == "@tailwind"
              && params == "utilities"
            {
              params.push_str(&format!(" source({path})"));
              let map = match &base {
                Some(base) => ctx1("sourceBase", base.clone()),
                None => Default::default(),
              };
              let wrapped = context(map, vec![child.clone()]);
              return Action::ReplaceStop(vec![wrapped]);
            }
            Action::Continue
          });
        } else if let Some(rest) = param.strip_prefix("theme(") {
          let theme_params = &rest[..rest.len().saturating_sub(1)];
          let has_reference = theme_params.contains("reference");
          let mut bad = false;
          walk(nodes, &mut |child, _| {
            if matches!(child, Node::Context { .. }) {
              return Action::Continue;
            }
            let Node::AtRule { name, params, .. } = child else {
              if has_reference {
                bad = true;
                return Action::Stop;
              }
              return Action::Continue;
            };
            if name == "@theme" {
              params.push(' ');
              params.push_str(theme_params);
              return Action::Skip;
            }
            Action::Continue
          });
          if bad {
            fail!(Error::Message(
              "Files imported with `@import \"…\" theme(reference)` must only contain `@theme` blocks.\nUse `@reference \"…\";` instead.".to_owned()
            ));
          }
        } else if let Some(rest) = param.strip_prefix("prefix(") {
          let prefix = &rest[..rest.len().saturating_sub(1)];
          walk(nodes, &mut |child, _| {
            if let Node::AtRule { name, params, .. } = child
              && name == "@theme"
            {
              params.push_str(&format!(" prefix({prefix})"));
              return Action::Skip;
            }
            Action::Continue
          });
        } else if param == "important" {
          important = true;
        } else if param == "reference" {
          let inner = std::mem::take(nodes);
          *nodes = vec![context(ctx1("reference", CtxValue::Bool(true)), inner)];
        } else {
          unknown.push(param.clone());
        }
      }
      if !unknown.is_empty() {
        *params = unknown.join(" ");
      } else if !parts.is_empty() {
        return Action::Replace(std::mem::take(nodes));
      }
      return Action::Continue;
    }

    if name == "@theme" {
      let (mut theme_options, theme_prefix) = parse_theme_options(params);
      features |= Features::AT_THEME;
      if ctx.get("reference").is_some_and(CtxValue::truthy) {
        theme_options |= ThemeOptions::REFERENCE;
      }
      if let Some(prefix) = theme_prefix {
        if prefix.is_empty() || !prefix.bytes().all(|b| b.is_ascii_lowercase()) {
          fail!(Error::Message(format!(
            "The prefix \"{prefix}\" is invalid. Prefixes must be lowercase ASCII letters (a-z) only."
          )));
        }
        theme.prefix = Some(prefix);
      }

      for child in nodes.iter() {
        match child {
          Node::AtRule { name, .. } if name == "@keyframes" => theme.add_keyframes(child.clone()),
          Node::Comment(_) => {}
          Node::Declaration { property, value, .. } if property.starts_with("--") => {
            if let Err(e) = theme.add(&unescape(property), value.as_deref().unwrap_or(""), theme_options) {
              fail!(e);
            }
          }
          other => {
            let css = to_css(&[at_rule(name.clone(), params.clone(), vec![other.clone()])]);
            let lines: Vec<&str> = css.split('\n').collect();
            let total = lines.len();
            let snippet: Vec<String> = lines
              .iter()
              .enumerate()
              .map(|(idx, line)| {
                format!(
                  "{} {line}",
                  if idx == 0 || idx >= total.saturating_sub(2) {
                    ' '
                  } else {
                    '>'
                  }
                )
              })
              .collect();
            fail!(Error::Message(format!(
              "`@theme` blocks must only contain custom properties or `@keyframes`.\n\n{}",
              snippet.join("\n")
            )));
          }
        }
      }

      if !has_theme_rule {
        has_theme_rule = true;
        return Action::ReplaceSkip(vec![style_rule(
          ":root, :host",
          vec![context(ctx1(THEME_SLOT, CtxValue::Bool(true)), Vec::new())],
        )]);
      }
      return Action::ReplaceSkip(Vec::new());
    }

    Action::Continue
  });
  if let Some(e) = error {
    return Err(e);
  }

  check_plugins(&mut ast)?;

  let names = extract_css_utilities(&mut ast, &mut custom_utilities);

  let mut ds = DesignSystem::new(theme);
  if important {
    ds.important = true;
  }
  for candidate in &ignored_candidates {
    ds.invalid_candidates.borrow_mut().insert(candidate.clone());
  }

  compat::register_legacy_utilities(&mut ds.utilities);
  ds.legacy_theme = Some(compat::LegacyTheme::default());

  for (name, _) in &custom_variants {
    ds.variants
      .static_variant(name, std::rc::Rc::new(|_, _, _| Some(())), None, None);
  }
  for name in topological_sort(&variant_deps)? {
    let Some((_, variant)) = custom_variants.iter().find(|(n, _)| *n == name) else {
      continue;
    };
    match variant {
      CustomVariant::Selectors { style, at_rules } => {
        let style = style.clone();
        let at_rules = at_rules.clone();
        let mut all: Vec<&str> = style.iter().map(String::as_str).collect();
        all.extend(at_rules.iter().map(String::as_str));
        let compounds = variants::compounds_for_selectors(&all);
        ds.variants.static_variant(
          &name,
          std::rc::Rc::new(move |r, _, _| {
            let nodes = std::mem::take(r.children_mut()?);
            let mut out = Vec::new();
            if !style.is_empty() {
              out.push(style_rule(style.join(", "), nodes.clone()));
            }
            for selector in &at_rules {
              out.push(rule(selector, nodes.clone()));
            }
            *r.children_mut()? = out;
            Some(())
          }),
          Some(compounds),
          None,
        );
      }
      CustomVariant::Body(body) => ds.variants.body_variant(&name, body.clone()),
    }
  }

  let bodies: Vec<Vec<Node>> = custom_utilities.into_iter().map(|(_, body)| body).collect();
  *ds.css_utilities.borrow_mut() = bodies;
  for (index, name) in names.0.iter().enumerate() {
    utilities::create_css_utility(&mut ds, name, index);
  }

  if has_theme_rule {
    let mut nodes = Vec::new();
    for (key, value) in ds.theme.entries() {
      if value.options.has(ThemeOptions::REFERENCE) {
        continue;
      }
      nodes.push(decl(escape(&key), value.value));
    }
    for keyframes in ds.theme.keyframes() {
      ast.push(context(
        ctx1("theme", CtxValue::Bool(true)),
        vec![at_root(vec![keyframes.clone()])],
      ));
    }
    fill_theme(&mut ast, nodes);
  }

  features |= variants::substitute_at_variant(&mut ast, &ds)?;
  for_each_utility_body(&ds, |body, ds| {
    variants::substitute_at_variant(body, ds).map(|f| features |= f)
  })?;
  features |= css_functions::substitute_functions(&mut ast, &ds)?;
  for_each_utility_body(&ds, |body, ds| {
    css_functions::substitute_functions(body, ds).map(|f| features |= f)
  })?;
  features |= apply::substitute_at_apply(&mut ast, &ds, &names.1)?;

  let mut all_valid = IndexSet::new();
  let mut default_did_change = false;
  for candidate in inline_candidates {
    if !ds.invalid_candidates.borrow().contains(&candidate) {
      all_valid.insert(candidate);
      default_did_change = true;
    }
  }

  Ok(Compiler {
    ds,
    ast,
    features,
    sources,
    root,
    has_utilities,
    polyfills: options.polyfills,
    all_valid,
    default_did_change,
    previous_count: 0,
    compiled: None,
  })
}

/// `@plugin` and `@config` load JavaScript, which this crate does not run. Their syntax is
/// still checked as Tailwind checks it before it would load them.
fn check_plugins(ast: &mut Vec<Node>) -> Result<(), Error> {
  let mut error: Option<Error> = None;
  let mut found = false;
  walk(ast, &mut |node, visit| {
    let Node::AtRule { name, params, nodes } = node else {
      return Action::Continue;
    };
    let has_parent = visit.css_parent().is_some();
    if name == "@plugin" {
      if has_parent {
        error = Some(Error::Message("`@plugin` cannot be nested.".to_owned()));
        return Action::Stop;
      }
      if params.chars().count() <= 2 {
        error = Some(Error::Message("`@plugin` must have a path.".to_owned()));
        return Action::Stop;
      }
      for option in nodes.iter() {
        let Node::Declaration { property, value, .. } = option else {
          error = Some(Error::Message(format!(
            "Unexpected `@plugin` option:\n\n{}\n\n`@plugin` options must be a flat list of declarations.",
            to_css(std::slice::from_ref(option))
          )));
          return Action::Stop;
        };
        let Some(value) = value else { continue };
        for part in segment(value, b',') {
          let part = part.trim();
          if part.starts_with('{') && part.ends_with('}') {
            let shown = to_css(&[Node::Declaration {
              property: property.clone(),
              value: Some(value.clone()),
              important: false,
            }]);
            error = Some(Error::Message(format!(
              "Unexpected `@plugin` option: Value of declaration `{}` is not supported.\n\nUsing an object as a plugin option is currently only supported in JavaScript configuration files.",
              shown.trim()
            )));
            return Action::Stop;
          }
        }
      }
      found = true;
      return Action::Replace(Vec::new());
    }
    if name == "@config" {
      if !nodes.is_empty() {
        error = Some(Error::Message("`@config` cannot have a body.".to_owned()));
        return Action::Stop;
      }
      if has_parent {
        error = Some(Error::Message("`@config` cannot be nested.".to_owned()));
        return Action::Stop;
      }
      found = true;
      return Action::Replace(Vec::new());
    }
    Action::Continue
  });
  if let Some(e) = error {
    return Err(e);
  }
  if found {
    return Err(Error::Message(
      "No `loadModule` function provided to `compile`".to_owned(),
    ));
  }
  Ok(())
}

fn for_each_utility_body(
  ds: &DesignSystem,
  mut f: impl FnMut(&mut Vec<Node>, &DesignSystem) -> Result<(), Error>,
) -> Result<(), Error> {
  let count = ds.css_utilities.borrow().len();
  for index in 0..count {
    let mut body = std::mem::take(&mut ds.css_utilities.borrow_mut()[index]);
    let result = f(&mut body, ds);
    ds.css_utilities.borrow_mut()[index] = body;
    result?;
  }
  Ok(())
}

fn set_custom_variant(
  variants: &mut Vec<(String, CustomVariant)>,
  deps: &mut Vec<(String, Vec<String>)>,
  name: String,
  variant: CustomVariant,
  dependencies: Vec<String>,
) {
  match variants.iter_mut().find(|(n, _)| *n == name) {
    Some(existing) => existing.1 = variant,
    None => variants.push((name.clone(), variant)),
  }
  match deps.iter_mut().find(|(n, _)| *n == name) {
    Some(existing) => existing.1 = dependencies,
    None => deps.push((name, dependencies)),
  }
}

fn topological_sort(graph: &[(String, Vec<String>)]) -> Result<Vec<String>, Error> {
  let mut seen = IndexSet::new();
  let mut wip = IndexSet::new();
  let mut sorted = Vec::new();

  fn visit(
    node: &str,
    path: &mut Vec<String>,
    graph: &[(String, Vec<String>)],
    seen: &mut IndexSet<String>,
    wip: &mut IndexSet<String>,
    sorted: &mut Vec<String>,
  ) -> Result<(), Error> {
    let Some((_, deps)) = graph.iter().find(|(n, _)| n == node) else {
      return Ok(());
    };
    if seen.contains(node) {
      return Ok(());
    }
    if wip.contains(node) {
      let nodes: Vec<Node> = path
        .iter()
        .enumerate()
        .map(|(idx, name)| {
          at_rule(
            "@custom-variant",
            name.clone(),
            vec![at_rule(
              "@variant",
              path.get(idx + 1).cloned().unwrap_or_else(|| node.to_owned()),
              Vec::new(),
            )],
          )
        })
        .collect();
      let output = to_css(&nodes).replace(';', " { … }").replacen(
        &format!("@custom-variant {node} {{"),
        &format!("@custom-variant {node} {{ /* ← */"),
        1,
      );
      return Err(Error::Message(format!(
        "Circular dependency detected in custom variants:\n\n{output}"
      )));
    }
    wip.insert(node.to_owned());
    for dep in deps.clone() {
      path.push(node.to_owned());
      visit(&dep, path, graph, seen, wip, sorted)?;
      path.pop();
    }
    seen.insert(node.to_owned());
    wip.shift_remove(node);
    sorted.push(node.to_owned());
    Ok(())
  }

  for (node, _) in graph {
    visit(node, &mut Vec::new(), graph, &mut seen, &mut wip, &mut sorted)?;
  }
  Ok(sorted)
}

/// Moves every top-level `@utility` body out of the tree into `out`, returning each
/// utility's name with and without its `-*`.
fn extract_css_utilities(
  ast: &mut Vec<Node>,
  out: &mut Vec<(String, Vec<Node>)>,
) -> (Vec<String>, apply::UtilityNames) {
  out.clear();
  walk(ast, &mut |node, visit| {
    if visit.css_parent().is_some() {
      return Action::Skip;
    }
    match node {
      Node::AtRule { name, params, nodes } if name == "@utility" => {
        out.push((params.clone(), std::mem::take(nodes)));
        Action::ReplaceSkip(Vec::new())
      }
      Node::Context { .. } => Action::Continue,
      _ => Action::Skip,
    }
  });
  let names: Vec<String> = out.iter().map(|(n, _)| n.clone()).collect();
  let roots = names
    .iter()
    .map(|n| {
      unescape(n)
        .strip_suffix("-*")
        .map(str::to_owned)
        .unwrap_or_else(|| unescape(n))
    })
    .collect();
  (names, apply::UtilityNames(roots))
}

fn fill_theme(ast: &mut Vec<Node>, nodes: Vec<Node>) {
  let mut nodes = Some(nodes);
  walk(ast, &mut |node, _| {
    if let Node::Context { context, .. } = node
      && context.contains_key(THEME_SLOT)
    {
      *node = ast::context(ctx1("theme", CtxValue::Bool(true)), nodes.take().unwrap_or_default());
      return Action::Stop;
    }
    Action::Continue
  });
}
