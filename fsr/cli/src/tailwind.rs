//! The classes Tailwind generates utilities for, read from the lowered plan rather than scanned
//! from files: every `class` an element renders, each branch of a conditional one, the fixed
//! parts of a template and the class props a component is placed with. A class value the build
//! cannot list every value of is reported as `tailwind/unlisted-class`.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use snapfire_fsr_ir::{Builtin, Entry, Expr, Lit, LogicOp, Stmt, Tmpl};
use snapfire_fsr_plan::Manifest;

/// Where the build writes the classes for the bundle's stylesheets, relative to the app.
pub const CLASSES_FILE: &str = "generated/tailwind.classes";

/// The lint rule a class value the build cannot list is reported under.
pub const UNLISTED: &str = "tailwind/unlisted-class";

#[derive(Debug, Default, PartialEq)]
pub struct Classes {
  /// Each class and the modules that render it.
  pub listed: BTreeMap<String, BTreeSet<String>>,
  /// Each module with a class value the build cannot list, paired with that value as the plan spells it.
  pub unlisted: Vec<(String, String)>,
}

impl Classes {
  /// The classes file: one class per line, a tab, then the modules using it.
  pub fn file(&self) -> String {
    self.listed.iter().map(|(class, modules)| format!("{class}\t{}\n", modules.iter().cloned().collect::<Vec<_>>().join(" "))).collect()
  }
}

/// Whether any stylesheet under `styles` uses Tailwind, which is what asks for the classes file.
pub fn wanted(styles: &Path) -> bool {
  let mut stack = vec![styles.to_path_buf()];
  while let Some(dir) = stack.pop() {
    let Ok(entries) = std::fs::read_dir(&dir) else { continue };
    for entry in entries.flatten() {
      let path = entry.path();
      if path.is_dir() {
        stack.push(path);
      } else if path.extension().is_some_and(|e| e == "css") && std::fs::read_to_string(&path).is_ok_and(|css| uses_tailwind(&css)) {
        return true;
      }
    }
  }
  false
}

/// Whether a stylesheet imports Tailwind.
fn uses_tailwind(css: &str) -> bool {
  css.lines().any(|line| {
    let line = line.trim_start();
    line.starts_with("@import") && line.contains("tailwindcss")
  })
}

/// Where the build writes what each class means on a card image, relative to the app.
pub const CARDS_FILE: &str = snapfire_fsr_host::CARD_CLASSES_FILE;

/// One class as card declarations, the shape the host reads from [`CARDS_FILE`].
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CardClass {
  pub class: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub min_width: Option<f32>,
  pub declarations: Vec<(String, String)>,
}

/// What each listed class means on a card image, from the first stylesheet under `styles` that
/// imports Tailwind, in Tailwind's output order. Empty without such a stylesheet.
pub fn card_table(styles: &Path, classes: &Classes) -> Result<Vec<CardClass>, crate::BuildError> {
  let mut sheets: Vec<std::path::PathBuf> = std::fs::read_dir(styles).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "css")).collect();
  sheets.sort();
  let Some((sheet, css)) = sheets.into_iter().find_map(|p| std::fs::read_to_string(&p).ok().filter(|css| uses_tailwind(css)).map(|css| (p, css))) else { return Ok(Vec::new()) };
  let load = |id: &str, base: &str| -> Result<snapfire_tailwind::Loaded, snapfire_tailwind::Error> {
    if let Some(loaded) = snapfire_tailwind::tailwind_stylesheet(id, base) {
      return Ok(loaded);
    }
    let file = Path::new(base).join(id);
    let content = std::fs::read_to_string(&file).map_err(|e| snapfire_tailwind::Error::Message(format!("{}: {e}", file.display())))?;
    Ok(snapfire_tailwind::Loaded { path: file.to_string_lossy().into_owned(), base: file.parent().unwrap_or(Path::new(".")).to_string_lossy().into_owned(), content })
  };
  let base = sheet.parent().unwrap_or(Path::new(".")).to_string_lossy().into_owned();
  let options = snapfire_tailwind::CompileOptions { base, load_stylesheet: Some(Box::new(load)), banner: false, from: Some(sheet.to_string_lossy().into_owned()), ..Default::default() };
  let compiler = snapfire_tailwind::Compiler::new(&css, options).map_err(|e| crate::BuildError::Dev(format!("{}: {e}", sheet.display())))?;
  Ok(compiler.card_rules(classes.listed.keys().map(String::as_str)).into_iter().map(|rule| CardClass { class: rule.candidate, min_width: rule.min_width, declarations: rule.declarations }).collect())
}

/// Every class the plan's components render.
pub fn collect(manifest: &Manifest) -> Classes {
  let mut classes = Classes::default();
  for component in &manifest.components {
    let mut scope = Scope { consts: &manifest.consts, names: HashMap::new() };
    for stmt in &component.body.body {
      if let Stmt::Let { name, expr } = stmt {
        scope.names.insert(name.clone(), Bound::Expr(expr.clone()));
      }
    }
    let mut found = Found::default();
    walk(&component.body.render, &mut scope, &mut found);
    for class in found.tokens {
      classes.listed.entry(class).or_default().insert(component.module.clone());
    }
    for value in found.unlisted {
      classes.unlisted.push((component.module.clone(), value));
    }
  }
  classes
}

#[derive(Clone)]
enum Bound {
  Expr(Expr),
  /// A loop parameter: one item of the expression at a time.
  Item(Expr),
}

struct Scope<'a> {
  consts: &'a snapfire_fsr_ir::ast::Consts,
  names: HashMap<String, Bound>,
}

#[derive(Default)]
struct Found {
  tokens: BTreeSet<String>,
  unlisted: Vec<String>,
}

/// What an expression in a class position can be: the whole strings it may take. `open` says
/// whether it may also take a value the build cannot see.
#[derive(Default)]
struct Values {
  strings: Vec<String>,
  open: Option<Open>,
}

#[derive(Clone, Copy, PartialEq)]
enum Open {
  /// The component's caller supplies it. The caller's classes are listed where it is placed.
  Passed,
  Dynamic,
}

impl Values {
  fn known(strings: Vec<String>) -> Self {
    Values { strings, open: None }
  }

  fn open(open: Open) -> Self {
    Values { strings: Vec::new(), open: Some(open) }
  }

  fn merge(mut self, other: Values) -> Self {
    self.strings.extend(other.strings);
    self.open = match (self.open, other.open) {
      (Some(Open::Dynamic), _) | (_, Some(Open::Dynamic)) => Some(Open::Dynamic),
      (Some(Open::Passed), _) | (_, Some(Open::Passed)) => Some(Open::Passed),
      _ => None,
    };
    self
  }
}

const DEPTH: usize = 24;
const PRODUCT: usize = 256;

fn walk(tmpl: &Tmpl, scope: &mut Scope, found: &mut Found) {
  match tmpl {
    Tmpl::Element { attrs, children, .. } => {
      for attr in attrs {
        match attr {
          Entry::Field(name, expr) if name == "class" => class_value(expr, scope, found),
          Entry::Spread(expr) => {
            if let Some(Expr::Object(entries)) = resolve(expr, scope, 0) {
              for entry in entries {
                if let Entry::Field(name, value) = entry
                  && (name == "className" || name == "class")
                {
                  class_value(&value, scope, found);
                }
              }
            }
          }
          _ => {}
        }
      }
      for child in children {
        walk(child, scope, found);
      }
    }
    Tmpl::Component { props, children, .. } | Tmpl::Island { props, children, .. } => {
      for prop in props {
        if let Entry::Field(name, expr) = prop
          && is_class_prop(name)
        {
          class_value(expr, scope, found);
        }
      }
      for child in children {
        walk(child, scope, found);
      }
    }
    Tmpl::Fragment(children) | Tmpl::Baked { children, .. } => {
      for child in children {
        walk(child, scope, found);
      }
    }
    Tmpl::If { then, r#else, .. } => {
      walk(then, scope, found);
      if let Some(other) = r#else {
        walk(other, scope, found);
      }
    }
    Tmpl::For { over, params, body } => {
      let saved: Vec<(String, Option<Bound>)> = params.iter().map(|p| (p.clone(), scope.names.get(p).cloned())).collect();
      if let Some(first) = params.first() {
        scope.names.insert(first.clone(), Bound::Item(over.clone()));
      }
      for param in params.iter().skip(1) {
        scope.names.insert(param.clone(), Bound::Expr(Expr::Lit(Lit::Null)));
      }
      walk(body, scope, found);
      restore(scope, saved);
    }
    Tmpl::Let { name, expr, then } => {
      let saved = vec![(name.clone(), scope.names.get(name).cloned())];
      scope.names.insert(name.clone(), Bound::Expr(expr.clone()));
      walk(then, scope, found);
      restore(scope, saved);
    }
    Tmpl::Text(_) | Tmpl::Expr(_) | Tmpl::Slot(_) => {}
  }
}

fn restore(scope: &mut Scope, saved: Vec<(String, Option<Bound>)>) {
  for (name, bound) in saved {
    match bound {
      Some(bound) => scope.names.insert(name, bound),
      None => scope.names.remove(&name),
    };
  }
}

/// `className`, `class` and any prop ending in `ClassName` or `Class`.
fn is_class_prop(name: &str) -> bool {
  name == "class" || name == "className" || name.ends_with("ClassName") || name.ends_with("Class")
}

fn class_value(expr: &Expr, scope: &Scope, found: &mut Found) {
  let values = values(expr, scope, 0);
  for string in &values.strings {
    found.tokens.extend(string.split_whitespace().map(str::to_owned));
  }
  if values.open == Some(Open::Dynamic) {
    found.unlisted.push(as_written(expr).unwrap_or_else(|| snapfire_fsr_ir::sexpr::print(&[snapfire_fsr_ir::sexpr::expr_to_sx(expr)]).trim().to_owned()));
  }
}

/// `expr` in the TypeScript it was lowered from, for a value a reader wrote as
/// a template, a field read, a ternary or an operator over those. `None` for
/// any other form, which the report prints as the plan does.
fn as_written(expr: &Expr) -> Option<String> {
  Some(match expr {
    Expr::Lit(Lit::Str(text)) => serde_json::to_string(text).ok()?,
    Expr::Lit(Lit::Int(n)) => n.to_string(),
    Expr::Lit(Lit::Float(n)) => n.to_string(),
    Expr::Lit(Lit::Bool(b)) => b.to_string(),
    Expr::Lit(Lit::Null) => "null".to_owned(),
    Expr::Var(name) => name.trim_start_matches('$').to_owned(),
    Expr::Param(name) => format!("params.{name}"),
    Expr::Query(name) => format!("query.{name}"),
    Expr::Field(target, name) => format!("{}.{name}", as_written(target)?),
    Expr::Index(target, index) => format!("{}[{}]", as_written(target)?, as_written(index)?),
    Expr::Template(parts) => {
      let mut out = String::from("`");
      for part in parts {
        match part {
          Expr::Lit(Lit::Str(text)) => out.push_str(&text.replace('\\', "\\\\").replace('`', "\\`").replace("${", "\\${")),
          other => out.push_str(&format!("${{{}}}", as_written(other)?)),
        }
      }
      out.push('`');
      out
    }
    Expr::Ternary(cond, then, otherwise) => format!("{} ? {} : {}", as_written(cond)?, as_written(then)?, as_written(otherwise)?),
    Expr::Logic(op, a, b) => format!("{} {} {}", as_written(a)?, if *op == LogicOp::And { "&&" } else { "||" }, as_written(b)?),
    Expr::Coalesce(a, b) => format!("{} ?? {}", as_written(a)?, as_written(b)?),
    Expr::Not(inner) => format!("!{}", as_written(inner)?),
    _ => return None,
  })
}

/// The expression a name or a constant stands for, followed until it is something else.
fn resolve(expr: &Expr, scope: &Scope, depth: usize) -> Option<Expr> {
  if depth > DEPTH {
    return None;
  }
  match expr {
    Expr::Var(name) => match scope.names.get(name) {
      Some(Bound::Expr(bound)) => resolve(bound, scope, depth + 1).or_else(|| Some(bound.clone())),
      _ => None,
    },
    Expr::Const(name) => scope.consts.get(name).map(|c| resolve(c, scope, depth + 1).unwrap_or_else(|| c.clone())),
    other => Some(other.clone()),
  }
}

fn values(expr: &Expr, scope: &Scope, depth: usize) -> Values {
  if depth > DEPTH {
    return Values::open(Open::Dynamic);
  }
  let next = depth + 1;
  match expr {
    Expr::Lit(Lit::Str(s)) => Values::known(vec![s.clone()]),
    Expr::Lit(_) => Values::default(),
    Expr::Folded(value) => match value {
      snapfire_fsr_core::value::Value::Str(s) => Values::known(vec![s.as_str().to_owned()]),
      _ => Values::default(),
    },
    Expr::Ternary(_, then, other) => values(then, scope, next).merge(values(other, scope, next)),
    Expr::Logic(LogicOp::And, _, then) => values(then, scope, next),
    Expr::Logic(LogicOp::Or, left, right) | Expr::Coalesce(left, right) => values(left, scope, next).merge(values(right, scope, next)),
    Expr::Template(parts) => template(parts, scope, next),
    Expr::Array(entries) => entries.iter().fold(Values::default(), |acc, entry| match entry {
      Entry::Item(item) | Entry::Spread(item) => acc.merge(values(item, scope, next)),
      _ => acc,
    }),
    Expr::Object(entries) => entries.iter().fold(Values::default(), |acc, entry| match entry {
      Entry::Field(key, _) => acc.merge(Values::known(vec![key.clone()])),
      Entry::Computed(key, _) => acc.merge(values(key, scope, next)),
      _ => acc.merge(Values::open(Open::Dynamic)),
    }),
    Expr::Builtin { name: Builtin::Join | Builtin::Trim, args } => args.first().map(|a| values(a, scope, next)).unwrap_or_default(),
    Expr::Builtin { name: Builtin::Concat, args } => args.iter().fold(Values::default(), |acc, a| acc.merge(values(a, scope, next))),
    Expr::Filter(over, _) => values(over, scope, next),
    Expr::Var(name) if name == "$props" => Values::open(Open::Passed),
    Expr::Var(name) => match scope.names.get(name) {
      Some(Bound::Expr(bound)) => values(bound, scope, next),
      Some(Bound::Item(over)) => items(over, scope, next),
      None => Values::open(Open::Dynamic),
    },
    Expr::Const(name) => match scope.consts.get(name) {
      Some(value) => values(value, scope, next),
      None => Values::open(Open::Dynamic),
    },
    Expr::Field(object, field) => match object.as_ref() {
      Expr::Var(name) if name == "$props" => Values::open(Open::Passed),
      _ => match resolve(object, scope, next) {
        Some(Expr::Object(entries)) => match entries.iter().find_map(|e| match e {
          Entry::Field(key, value) if key == field => Some(value),
          _ => None,
        }) {
          Some(value) => values(value, scope, next),
          None => Values::open(Open::Dynamic),
        },
        _ => match object.as_ref() {
          Expr::Var(name) if matches!(scope.names.get(name), Some(Bound::Item(_))) => {
            let Some(Bound::Item(over)) = scope.names.get(name) else { unreachable!() };
            match resolve(over, scope, next) {
              Some(Expr::Array(rows)) => rows.iter().fold(Values::default(), |acc, row| match row {
                Entry::Item(row) => acc.merge(values(&Expr::Field(Box::new(row.clone()), field.clone()), scope, next)),
                _ => acc.merge(Values::open(Open::Dynamic)),
              }),
              _ => Values::open(Open::Dynamic),
            }
          }
          _ => Values::open(Open::Dynamic),
        },
      },
    },
    Expr::Index(object, _) => match resolve(object, scope, next) {
      Some(Expr::Object(entries)) => entries.iter().fold(Values::default(), |acc, entry| match entry {
        Entry::Field(_, value) | Entry::Computed(_, value) => acc.merge(values(value, scope, next)),
        _ => acc.merge(Values::open(Open::Dynamic)),
      }),
      Some(Expr::Array(_)) => items(object, scope, next),
      _ => Values::open(Open::Dynamic),
    },
    _ => Values::open(Open::Dynamic),
  }
}

/// The values one item of the array `over` can take.
fn items(over: &Expr, scope: &Scope, depth: usize) -> Values {
  match resolve(over, scope, depth) {
    Some(Expr::Array(entries)) => entries.iter().fold(Values::default(), |acc, entry| match entry {
      Entry::Item(item) => acc.merge(values(item, scope, depth + 1)),
      _ => acc.merge(Values::open(Open::Dynamic)),
    }),
    _ => Values::open(Open::Dynamic),
  }
}

/// A template's whole values when every part is known, else the tokens of its fixed parts that
/// no unknown part touches. An unknown part that is not a whole token makes the value unlisted.
fn template(parts: &[Expr], scope: &Scope, depth: usize) -> Values {
  let parts: Vec<Values> = parts.iter().map(|p| values(p, scope, depth)).collect();
  if parts.iter().all(|p| p.open.is_none()) {
    let mut product = vec![String::new()];
    for part in &parts {
      let options: &[String] = if part.strings.is_empty() { &[] } else { &part.strings };
      if options.is_empty() {
        continue;
      }
      if product.len() * options.len() > PRODUCT {
        return Values::known(parts.into_iter().flat_map(|p| p.strings).collect());
      }
      product = product.iter().flat_map(|head| options.iter().map(move |tail| format!("{head}{tail}"))).collect();
    }
    return Values::known(product);
  }

  let mut out = Values::default();
  for (i, part) in parts.iter().enumerate() {
    if let Some(open) = part.open {
      let before = i.checked_sub(1).map(|j| &parts[j]);
      let after = parts.get(i + 1);
      let joined = before.is_some_and(|p| p.open.is_none() && p.strings.iter().any(|s| !s.is_empty() && !s.ends_with(char::is_whitespace))) || after.is_some_and(|p| p.open.is_none() && p.strings.iter().any(|s| !s.is_empty() && !s.starts_with(char::is_whitespace)));
      out = out.merge(Values::open(if joined { Open::Dynamic } else { open }));
      continue;
    }
    let open_before = i > 0 && parts[i - 1].open.is_some();
    let open_after = parts.get(i + 1).is_some_and(|p| p.open.is_some());
    for string in &part.strings {
      let mut tokens: Vec<&str> = string.split_whitespace().collect();
      if open_before && !string.starts_with(char::is_whitespace) && !tokens.is_empty() {
        tokens.remove(0);
      }
      if open_after && !string.ends_with(char::is_whitespace) && !tokens.is_empty() {
        tokens.pop();
      }
      out.strings.extend(tokens.into_iter().map(str::to_owned));
    }
  }
  out
}

#[cfg(test)]
mod tests {
  use super::*;

  fn lit(s: &str) -> Expr {
    Expr::Lit(Lit::Str(s.to_owned()))
  }

  fn var(s: &str) -> Expr {
    Expr::Var(s.to_owned())
  }

  fn el(class: Expr) -> Tmpl {
    Tmpl::Element { tag: "div".to_owned(), attrs: vec![Entry::Field("class".to_owned(), class)], children: Vec::new() }
  }

  fn found(tmpl: Tmpl, lets: &[(&str, Expr)]) -> (Vec<String>, Vec<String>) {
    let consts = Default::default();
    let mut scope = Scope { consts: &consts, names: lets.iter().map(|(n, e)| (n.to_string(), Bound::Expr(e.clone()))).collect() };
    let mut found = Found::default();
    walk(&tmpl, &mut scope, &mut found);
    (found.tokens.into_iter().collect(), found.unlisted)
  }

  #[test]
  fn lists_a_static_class_and_both_branches_of_a_conditional() {
    let tmpl = Tmpl::Fragment(vec![el(lit("p-4  font-bold")), el(Expr::Ternary(Box::new(var("open")), Box::new(lit("bg-sky-500 text-white")), Box::new(lit("bg-white"))))]);
    let (tokens, unlisted) = found(tmpl, &[]);
    assert_eq!(tokens, ["bg-sky-500", "bg-white", "font-bold", "p-4", "text-white"]);
    assert!(unlisted.is_empty());
  }

  #[test]
  fn lists_the_fixed_parts_of_a_template_and_reports_a_partial_class() {
    let whole = Expr::Template(vec![lit("card "), Expr::Field(Box::new(var("$props")), "tone".to_owned()), lit(" shadow")]);
    let partial = Expr::Template(vec![lit("rounded bg-"), Expr::Field(Box::new(var("row")), "color".to_owned()), lit("-500")]);
    let (tokens, unlisted) = found(Tmpl::Fragment(vec![el(whole), el(partial)]), &[]);
    assert_eq!(tokens, ["card", "rounded", "shadow"]);
    assert_eq!(unlisted.len(), 1);
    assert!(unlisted[0].contains("color"), "{unlisted:?}");
  }

  #[test]
  fn follows_a_let_and_a_lookup_into_a_literal_object() {
    let tones = Expr::Object(vec![Entry::Field("danger".to_owned(), lit("bg-red-600")), Entry::Field("calm".to_owned(), lit("bg-emerald-600"))]);
    let tmpl = el(Expr::Index(Box::new(var("tones")), Box::new(Expr::Field(Box::new(var("$props")), "tone".to_owned()))));
    let (tokens, unlisted) = found(tmpl, &[("tones", tones)]);
    assert_eq!(tokens, ["bg-emerald-600", "bg-red-600"]);
    assert!(unlisted.is_empty());
  }

  #[test]
  fn a_class_prop_passed_to_a_component_is_listed_where_it_is_placed() {
    let tmpl = Tmpl::Component { module: "src/ui/Card.tsx#Card".to_owned(), props: vec![Entry::Field("className".to_owned(), lit("mt-8")), Entry::Field("title".to_owned(), lit("not a class"))], children: Vec::new(), id: 0, keyed: false };
    let callee = el(Expr::Field(Box::new(var("$props")), "className".to_owned()));
    let (tokens, unlisted) = found(Tmpl::Fragment(vec![tmpl, callee]), &[]);
    assert_eq!(tokens, ["mt-8"]);
    assert!(unlisted.is_empty());
  }

  #[test]
  fn reports_a_class_read_from_data() {
    let (tokens, unlisted) = found(el(Expr::Field(Box::new(var("data")), "class".to_owned())), &[]);
    assert!(tokens.is_empty());
    assert_eq!(unlisted.len(), 1);
  }

  #[test]
  fn an_unlisted_value_is_quoted_as_it_was_written() {
    let template = Expr::Template(vec![Expr::lit_str("rounded-full px-3 status-"), Expr::Field(Box::new(var("s")), "state".to_owned())]);
    let (_, unlisted) = found(el(template), &[]);
    assert_eq!(unlisted, ["`rounded-full px-3 status-${s.state}`"]);
    let ternary = Expr::Ternary(Box::new(var("open")), Box::new(Expr::lit_str("block")), Box::new(Expr::Field(Box::new(var("data")), "tone".to_owned())));
    let (_, unlisted) = found(el(ternary), &[]);
    assert_eq!(unlisted, ["open ? \"block\" : data.tone"]);
  }

  #[test]
  fn writes_one_line_per_class() {
    let mut classes = Classes::default();
    classes.listed.entry("flex".to_owned()).or_default().extend(["routes/page.tsx#default".to_owned(), "src/ui/Header.tsx#Header".to_owned()]);
    classes.listed.entry("p-4".to_owned()).or_default().insert("routes/page.tsx#default".to_owned());
    assert_eq!(classes.file(), "flex\troutes/page.tsx#default src/ui/Header.tsx#Header\np-4\troutes/page.tsx#default\n");
  }
}
