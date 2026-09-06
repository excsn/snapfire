//! Renders a lowered component to HTML. The tree is the component's own, so
//! the serialiser is a string builder: elements, escaped text and the three
//! idioms. What the browser hydrates over is exactly this output, so it
//! follows React's server renderer byte for byte: adjacent text nodes are
//! separated by an empty comment, empty text writes nothing, a boolean
//! attribute is `name=""`, a void element closes with `/>`.

use std::collections::HashMap;
use std::sync::Arc;

use snapfire_fsr_core::{Value, ValueMap};

use crate::ast::{Component, Entry, Stmt, Tmpl};
use crate::interp::{Env, Fail, Hoists, Interpreter, stringify, truthy};

/// Every lowered component by module id, so one may render another.
pub type Components = HashMap<String, Arc<Component>>;

const VOID: &[&str] = &["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track", "wbr"];

/// Attributes that are present or absent rather than valued.
const BOOLEAN: &[&str] = &["disabled", "checked", "selected", "readonly", "required", "hidden", "multiple", "open", "autofocus", "autoplay", "controls", "loop", "muted", "novalidate", "defer", "async"];

fn escape_text(input: &str, out: &mut String) {
  for c in input.chars() {
    match c {
      '&' => out.push_str("&amp;"),
      '<' => out.push_str("&lt;"),
      '>' => out.push_str("&gt;"),
      c => out.push(c),
    }
  }
}

fn escape_attr(input: &str, out: &mut String) {
  for c in input.chars() {
    match c {
      '&' => out.push_str("&amp;"),
      '<' => out.push_str("&lt;"),
      '>' => out.push_str("&gt;"),
      '"' => out.push_str("&quot;"),
      c => out.push(c),
    }
  }
}

/// The output and whether the last thing written was text, which decides
/// whether the next text needs React's `<!-- -->` between them.
#[derive(Default)]
struct Out {
  html: String,
  text_open: bool,
  islands: Vec<RenderedIsland>,
  /// The values the root component's state `let`s took.
  state: ValueMap,
}

/// A component's markup with the islands placed inside it. Each island sits
/// in `html` as `ISLAND_MARK` followed by its index in `islands` and a NUL,
/// the way a root slot sits as a `SLOT_MARK`; the evaluator turns both into
/// nodes.
#[derive(Debug, Clone, PartialEq)]
pub struct Rendered {
  pub html: String,
  pub islands: Vec<RenderedIsland>,
  /// The values the markup's hoisted expressions took, keyed as `Hoists::key`
  /// does; the island's props carry them under `$h`.
  pub hoisted: ValueMap,
}

/// The props key an island's hoisted values ride under.
pub const HOISTED_PROP: &str = "$h";

/// A component rendered as an island: its module, the props it was given
/// and its own markup, which may hold islands of its own.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderedIsland {
  pub module: String,
  pub props: ValueMap,
  pub when: Option<String>,
  /// `Some("server")` for an island whose events round-trip to the server.
  pub mode: Option<String>,
  /// The values the component's state `let`s took, for a server-mode island
  /// to carry to the browser as `$s`.
  pub state: ValueMap,
  pub body: Rendered,
}

/// The props key a server-mode island's initial state rides under.
pub const STATE_PROP: &str = "$s";

impl RenderedIsland {
  /// The props the browser mounts the island with: its own plus `$h` when
  /// anything was hoisted and `$s` in server mode.
  pub fn mount_props(&self) -> ValueMap {
    let mut props = self.props.clone();
    if !self.body.hoisted.is_empty() {
      props.insert(HOISTED_PROP.to_owned(), Value::Map(self.body.hoisted.clone()));
    }
    if self.mode.as_deref() == Some(SERVER_MODE) {
      props.insert(STATE_PROP.to_owned(), Value::Map(self.state.clone()));
    }
    props
  }
}

/// The island mode whose events round-trip to the server.
pub const SERVER_MODE: &str = "server";

/// What `island_step` answers: the state after the handler and the island rendered from it.
#[derive(Debug, Clone, PartialEq)]
pub struct Stepped {
  pub state: ValueMap,
  pub rendered: Rendered,
}

/// The start of an island's place in the markup: `ISLAND_MARK`, the island's
/// index in decimal, then a NUL.
pub const ISLAND_MARK: &str = "\u{0}sf-island:";

impl Out {
  fn text(&mut self, text: &str) {
    if text.is_empty() {
      return;
    }
    if self.text_open {
      self.html.push_str("<!-- -->");
    }
    escape_text(text, &mut self.html);
    self.text_open = true;
  }

  fn markup(&mut self, html: &str) {
    self.html.push_str(html);
    self.text_open = false;
  }
}

/// What a root component's own `Slot` writes, since it has no caller: the
/// prefix, then the slot's name, then a NUL. The evaluator splits the markup
/// there and places the plan child of that name.
pub const SLOT_MARK: &str = "\u{0}sf-slot:";

pub fn slot_mark(name: &str) -> String {
  format!("{SLOT_MARK}{name}\u{0}")
}

/// A caller's children and the scope they read, rendered wherever the callee places its `Slot`.
struct Slot {
  children: Vec<Tmpl>,
  scope: Vec<(String, Value)>,
}

impl Interpreter {
  /// Renders `component` with `props` bound as `$props`. A `$store` prop and
  /// a `locale` prop are lifted out of the scope into the environment, where
  /// a nested component's `Expr::Store` and `Expr::Locale` still reach them.
  pub fn render(&self, component: &Component, props: &ValueMap, library: &Components) -> Result<Rendered, Fail> {
    self.render_module("", component, props, library)
  }

  /// `render` for the component under `module`, which keys its hoisted values.
  pub fn render_module(&self, module: &str, component: &Component, props: &ValueMap, library: &Components) -> Result<Rendered, Fail> {
    let mut env = self.env_for(module, props);
    let mut out = Out::default();
    let mut slots = Vec::new();
    render_component(&mut env, component, library, &mut slots, &mut out)?;
    let hoisted = env.hoists.take().map(|h| h.table).unwrap_or_default();
    Ok(Rendered { html: out.html, islands: out.islands, hoisted })
  }

  fn env_for(&self, module: &str, props: &ValueMap) -> Env {
    let mut env = Env::detached(self, vec![("$props".to_owned(), Value::Map(props.clone()))]);
    if let Some(Value::Map(store)) = props.get("$store") {
      env.store = store.clone();
    }
    if let Some(Value::Str(tag)) = props.get("locale") {
      env.ctx.locale = snapfire_fsr_runtime::Locale::new(tag.clone(), false);
    }
    env.hoists = Some(Hoists::new(module));
    env
  }

  /// One step of an island in server mode: the component's `let`s run with
  /// `state` standing in for its state bindings, `handler` runs with
  /// `$props`, `$state` and `$event` bound and the object it returns is
  /// merged into the state, then the component renders from that state with
  /// handler markers printed. `None` for `handler` renders without a step.
  pub fn island_step(&self, module: &str, component: &Component, props: &ValueMap, state: &ValueMap, handler: Option<usize>, event: &Value, library: &Components) -> Result<Stepped, Fail> {
    let mut env = self.env_for(module, props);
    env.server_mode = true;
    let mut state = state.clone();
    if let Some(index) = handler {
      let handler = component.handlers.get(index).ok_or_else(|| Fail::internal(format!("`{module}` has no handler {index}")))?;
      env.state = Some(state.clone());
      let depth = env.scope.len();
      for stmt in &component.body {
        let Stmt::Let { name, expr } = stmt else { continue };
        let value = match env.state.as_ref().and_then(|s| s.get(name)) {
          Some(held) => held.clone(),
          None => env.eval_sync(expr)?,
        };
        env.scope.push((name.clone(), value));
      }
      env.scope.push(("$state".to_owned(), Value::Map(state.clone())));
      env.scope.push(("$event".to_owned(), event.clone()));
      let patch = eval_body_sync(&mut env, &handler.body)?;
      env.scope.truncate(depth);
      if let Value::Map(patch) = patch {
        for (name, value) in patch {
          if component.state.contains(&name) {
            state.insert(name, value);
          }
        }
      }
    }
    env.state = Some(state.clone());
    let mut out = Out::default();
    let mut slots = Vec::new();
    render_component(&mut env, component, library, &mut slots, &mut out)?;
    let hoisted = env.hoists.take().map(|h| h.table).unwrap_or_default();
    Ok(Stepped { state, rendered: Rendered { html: out.html, islands: out.islands, hoisted } })
  }
}

/// A handler body under the sync evaluator: `let`s bind, `if` branches, the
/// first `return` answers; anything that suspends is an error.
fn eval_body_sync(env: &mut Env, body: &[Stmt]) -> Result<Value, Fail> {
  for stmt in body {
    match stmt {
      Stmt::Let { name, expr } => {
        let value = env.eval_sync(expr)?;
        env.scope.push((name.clone(), value));
      }
      Stmt::Return(expr) => return env.eval_sync(expr),
      Stmt::Expr(expr) => {
        env.eval_sync(expr)?;
      }
      Stmt::If { cond, then, r#else } => {
        let branch = if truthy(&env.eval_sync(cond)?) { then } else { r#else };
        let depth = env.scope.len();
        let value = eval_body_sync(env, branch)?;
        env.scope.truncate(depth);
        if !matches!(value, Value::Null) {
          return Ok(value);
        }
      }
      other => return Err(Fail::internal(format!("a handler cannot hold {other:?}"))),
    }
  }
  Ok(Value::Null)
}

/// Runs `f` with the hoist path extended by `index`, the position of the
/// iteration whose values are recorded inside.
fn in_iteration<T>(env: &mut Env, index: usize, f: impl FnOnce(&mut Env) -> T) -> T {
  if let Some(h) = &mut env.hoists {
    h.path.push(index);
  }
  let result = f(env);
  if let Some(h) = &mut env.hoists {
    h.path.pop();
  }
  result
}

/// Runs `f` with the hoist module set to `module`. The path carries on, so a
/// component placed from a caller's loop keys its ids below that iteration.
fn in_module<T>(env: &mut Env, module: &str, f: impl FnOnce(&mut Env) -> T) -> T {
  let outer = env.hoists.as_mut().map(|h| std::mem::replace(&mut h.module, module.to_owned()));
  let result = f(env);
  if let (Some(h), Some(module)) = (&mut env.hoists, outer) {
    h.module = module;
  }
  result
}

fn render_component(env: &mut Env, component: &Component, library: &Components, slots: &mut Vec<Slot>, out: &mut Out) -> Result<(), Fail> {
  let depth = env.scope.len();
  let overrides = env.state.take();
  for stmt in &component.body {
    let Stmt::Let { name, expr } = stmt else {
      return Err(Fail::internal("a component body is `let`s only"));
    };
    let value = match overrides.as_ref().and_then(|s| s.get(name)) {
      Some(held) => held.clone(),
      None => env.eval_sync(expr)?,
    };
    if component.state.contains(name) {
      out.state.insert(name.clone(), value.clone());
    }
    env.scope.push((name.clone(), value));
  }
  render(env, &component.render, library, slots, out)?;
  env.scope.truncate(depth);
  Ok(())
}

/// Evaluates attribute or prop entries into one map in order, later entries winning; attributes are keyed by HTML spelling so a spread's `className` and a literal `class` are one key.
fn entries(env: &mut Env, entries: &[Entry], attrs: bool) -> Result<Vec<(String, Value)>, Fail> {
  let mut out: Vec<(String, Value)> = Vec::new();
  let mut put = |name: String, value: Value| {
    let name = if attrs { html_attr_name(&name).to_owned() } else { name };
    if let Some(slot) = out.iter_mut().find(|(n, _)| *n == name) {
      slot.1 = value;
    } else {
      out.push((name, value));
    }
  };
  for entry in entries {
    match entry {
      Entry::Field(name, expr) => put(name.clone(), env.eval_sync(expr)?),
      Entry::Spread(expr) => match env.eval_sync(expr)? {
        Value::Map(map) => {
          for (name, value) in map {
            put(name, value);
          }
        }
        Value::Null => {}
        other => return Err(crate::interp::type_error("spread", "an object", &other)),
      },
      Entry::Computed(key, expr) => {
        let key = stringify(&env.eval_sync(key)?)?;
        put(key, env.eval_sync(expr)?);
      }
      Entry::Item(_) => return Err(Fail::internal("an item entry among attributes")),
    }
  }
  Ok(out)
}

fn render(env: &mut Env, tmpl: &Tmpl, library: &Components, slots: &mut Vec<Slot>, out: &mut Out) -> Result<(), Fail> {
  match tmpl {
    Tmpl::Text(text) => out.text(text),
    Tmpl::Expr(expr) => {
      let value = env.eval_sync(expr)?;
      interpolate(&value, out)?;
    }
    Tmpl::Element { tag, attrs, children } => {
      let mut open = format!("<{tag}");
      let mut bound = Vec::new();
      for (name, value) in entries(env, attrs, true)? {
        if let Some(event) = name.strip_prefix(HANDLER_ATTR) {
          if env.server_mode {
            bound.push(format!("{event}:{}", stringify(&value)?));
          }
          continue;
        }
        if name == KEY_ATTR {
          if env.server_mode {
            attribute("data-sf-key", &value, &mut open)?;
          }
          continue;
        }
        if skipped_attr(&name) {
          continue;
        }
        attribute(&name, &value, &mut open)?;
      }
      if !bound.is_empty() {
        attribute("data-sf-on", &Value::Str(bound.join(" ")), &mut open)?;
      }
      if VOID.contains(&tag.as_str()) {
        open.push_str("/>");
        out.markup(&open);
        return Ok(());
      }
      open.push('>');
      out.markup(&open);
      match chunk_id(attrs) {
        Some(id) => {
          let mut inner = Out::default();
          for child in children {
            render(env, child, library, slots, &mut inner)?;
          }
          if let Some(hoists) = &mut env.hoists {
            hoists.record(id, &Value::Str(inner.html.clone()));
          }
          out.islands.extend(inner.islands);
          out.markup(&inner.html);
        }
        None => {
          for child in children {
            render(env, child, library, slots, out)?;
          }
        }
      }
      out.markup(&format!("</{tag}>"));
    }
    Tmpl::Fragment(children) => {
      for child in children {
        render(env, child, library, slots, out)?;
      }
    }
    Tmpl::If { cond, then, r#else } => {
      let value = env.eval_sync(cond)?;
      if truthy(&value) {
        render(env, then, library, slots, out)?;
      } else if let Some(other) = r#else {
        render(env, other, library, slots, out)?;
      }
    }
    Tmpl::For { over, params, body } => {
      let items = match env.eval_sync(over)? {
        Value::Seq(items) => items,
        other => return Err(crate::interp::type_error("map", "an array", &other)),
      };
      let depth = env.scope.len();
      for (i, item) in items.into_iter().enumerate() {
        env.scope.truncate(depth);
        for (param, value) in params.iter().zip([item, Value::F64(i as f64)]) {
          env.scope.push((param.clone(), value));
        }
        in_iteration(env, i, |env| render(env, body, library, slots, out))?;
      }
      env.scope.truncate(depth);
    }
    Tmpl::Let { name, expr, then } => {
      let value = env.eval_sync(expr)?;
      let depth = env.scope.len();
      env.scope.push((name.clone(), value));
      render(env, then, library, slots, out)?;
      env.scope.truncate(depth);
    }
    Tmpl::Component { module, props, children } => {
      let component = library.get(module).cloned().ok_or_else(|| Fail::internal(format!("`{module}` is not a lowered component")))?;
      let mut map = ValueMap::new();
      for (name, value) in entries(env, props, false)? {
        if name != "children" {
          map.insert(name, value);
        }
      }
      let depth = env.scope.len();
      let outer = std::mem::replace(&mut env.scope, vec![("$props".to_owned(), Value::Map(map))]);
      slots.push(Slot { children: children.clone(), scope: outer.clone() });
      let result = in_module(env, module, |env| render_component(env, &component, library, slots, out));
      slots.pop();
      env.scope = outer;
      env.scope.truncate(depth);
      result?;
    }
    Tmpl::Island { module, props, children, when, mode } => {
      let component = library.get(module).cloned().ok_or_else(|| Fail::internal(format!("`{module}` is not a lowered component")))?;
      let mut map = ValueMap::new();
      for (name, value) in entries(env, props, false)? {
        if name != "children" {
          map.insert(name, value);
        }
      }
      let depth = env.scope.len();
      let outer = std::mem::replace(&mut env.scope, vec![("$props".to_owned(), Value::Map(map.clone()))]);
      slots.push(Slot { children: children.clone(), scope: outer.clone() });
      let mut inner = Out::default();
      let outer_hoists = env.hoists.replace(Hoists::new(module.clone()));
      let outer_mode = std::mem::replace(&mut env.server_mode, mode.as_deref() == Some(SERVER_MODE));
      let result = render_component(env, &component, library, slots, &mut inner);
      env.server_mode = outer_mode;
      let hoisted = std::mem::replace(&mut env.hoists, outer_hoists).map(|h| h.table).unwrap_or_default();
      slots.pop();
      env.scope = outer;
      env.scope.truncate(depth);
      result?;
      let index = out.islands.len();
      out.islands.push(RenderedIsland { module: module.clone(), props: map, when: when.clone(), mode: mode.clone(), state: inner.state, body: Rendered { html: inner.html, islands: inner.islands, hoisted } });
      out.markup(&format!("{ISLAND_MARK}{index}\u{0}"));
    }
    Tmpl::Slot(name) => {
      let Some(slot) = slots.pop() else {
        out.html.push_str(&slot_mark(name));
        return Ok(());
      };
      let inner = std::mem::replace(&mut env.scope, slot.scope.clone());
      let mut result = Ok(());
      for child in &slot.children {
        result = render(env, child, library, slots, out);
        if result.is_err() {
          break;
        }
      }
      env.scope = inner;
      slots.push(slot);
      result?;
    }
  }
  Ok(())
}

/// CSS properties React leaves unitless; every other number gets `px`.
const UNITLESS: &[&str] = &["animation-iteration-count", "aspect-ratio", "border-image-outset", "border-image-slice", "border-image-width", "box-flex", "box-flex-group", "box-ordinal-group", "column-count", "columns", "flex", "flex-grow", "flex-positive", "flex-shrink", "flex-negative", "flex-order", "grid-area", "grid-row", "grid-row-end", "grid-row-span", "grid-row-start", "grid-column", "grid-column-end", "grid-column-span", "grid-column-start", "font-weight", "line-clamp", "line-height", "opacity", "order", "orphans", "scale", "tab-size", "widows", "z-index", "zoom", "fill-opacity", "flood-opacity", "stop-opacity", "stroke-dasharray", "stroke-dashoffset", "stroke-miterlimit", "stroke-opacity", "stroke-width"];

/// A style object the way React's server renderer prints it: `name:value` joined by `;`, null and empty values skipped, a number in `px` unless the property is unitless or the number is zero.
fn style_text(map: &ValueMap) -> Result<String, Fail> {
  let mut out = String::new();
  for (name, value) in map {
    let text = match value {
      Value::Null | Value::Bool(_) => continue,
      Value::Str(s) if s.trim().is_empty() => continue,
      Value::Str(s) => s.trim().to_owned(),
      Value::Int(0) => "0".to_owned(),
      Value::F64(f) if *f == 0.0 => "0".to_owned(),
      Value::Int(_) | Value::F64(_) | Value::F32(_) | Value::UInt(_) => {
        let n = stringify(value)?;
        if UNITLESS.contains(&name.as_str()) || name.starts_with("--") { n } else { format!("{n}px") }
      }
      other => stringify(other)?,
    };
    if !out.is_empty() {
      out.push(';');
    }
    out.push_str(name);
    out.push(':');
    out.push_str(&text);
  }
  Ok(out)
}

/// Attribute keys the browser owns or that name no attribute: handlers, `key`, `ref`, `children` and `dangerouslySetInnerHTML`.
fn skipped_attr(name: &str) -> bool {
  name == "key" || name == "ref" || name == "children" || name == "dangerouslySetInnerHTML" || name.starts_with('$') || (name.len() > 2 && name.starts_with("on") && name.as_bytes()[2].is_ascii_uppercase())
}

/// The attribute marking an element whose inner markup is recorded as a hoisted chunk, and its id.
pub const CHUNK_ATTR: &str = "$chunk";

/// The prefix of an attribute binding a handler to its element: `$on:click`
/// holding the handler's index. Printed as `data-sf-on="click:0"` in server
/// mode and never otherwise.
pub const HANDLER_ATTR: &str = "$on:";
/// An element's React `key`, printed as `data-sf-key` in server mode so the
/// browser's patch keeps a moved element, and never otherwise.
pub const KEY_ATTR: &str = "$key";
/// Left on an element whose handler the build could not lower, holding the
/// line and the reason; an island in server mode is refused over it.
pub const UNLOWERED_ATTR: &str = "$unlowered";

fn chunk_id(attrs: &[Entry]) -> Option<u32> {
  attrs.iter().find_map(|entry| match entry {
    Entry::Field(name, crate::ast::Expr::Lit(crate::ast::Lit::Int(id))) if name == CHUNK_ATTR => Some(*id as u32),
    _ => None,
  })
}

/// React's attribute spellings to HTML's; an HTML spelling passes through.
pub fn html_attr_name(name: &str) -> &str {
  match name {
    "className" => "class",
    "htmlFor" => "for",
    "readOnly" => "readonly",
    "autoFocus" => "autofocus",
    "autoComplete" => "autocomplete",
    "tabIndex" => "tabindex",
    "defaultValue" => "value",
    "defaultChecked" => "checked",
    "maxLength" => "maxlength",
    "minLength" => "minlength",
    "colSpan" => "colspan",
    "rowSpan" => "rowspan",
    "srcSet" => "srcset",
    "noValidate" => "novalidate",
    "acceptCharset" => "accept-charset",
    "httpEquiv" => "http-equiv",
    "crossOrigin" => "crossorigin",
    "spellCheck" => "spellcheck",
    "encType" => "enctype",
    "formAction" => "formaction",
    other => other,
  }
}

/// A child expression the way React prints one: strings and numbers as text,
/// `null` and booleans as nothing, an array as its items in turn.
fn interpolate(value: &Value, out: &mut Out) -> Result<(), Fail> {
  match value {
    Value::Null | Value::Bool(_) => Ok(()),
    Value::Seq(items) => {
      for item in items {
        interpolate(item, out)?;
      }
      Ok(())
    }
    other => {
      out.text(&stringify(other)?);
      Ok(())
    }
  }
}

/// An attribute the way React's server renderer prints one: `null`,
/// `undefined` and `false` omit it, `true` on a boolean attribute writes
/// `name=""`, a `style` with nothing in it is omitted, anything else is
/// stringified.
fn attribute(name: &str, value: &Value, out: &mut String) -> Result<(), Fail> {
  if matches!(value, Value::Null | Value::Bool(false)) {
    return Ok(());
  }
  if name == "style" {
    let css = match value {
      Value::Map(map) => style_text(map)?,
      other => stringify(other)?,
    };
    if css.is_empty() {
      return Ok(());
    }
    out.push_str(" style=\"");
    escape_attr(&css, out);
    out.push('"');
    return Ok(());
  }
  if BOOLEAN.contains(&name) {
    if truthy(value) {
      out.push(' ');
      out.push_str(name);
      out.push_str("=\"\"");
    }
    return Ok(());
  }
  out.push(' ');
  out.push_str(name);
  out.push_str("=\"");
  escape_attr(&stringify(value)?, out);
  out.push('"');
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::ast::{Builtin, Expr, Lit, Stmt};
  fn props(entries: &[(&str, Value)]) -> ValueMap {
    entries.iter().map(|(k, v)| ((*k).to_owned(), v.clone())).collect()
  }

  fn p(name: &str) -> Expr {
    Expr::var("$props").field(name)
  }

  #[test]
  fn elements_text_and_interpolation_print_like_react() {
    let component = Component {
      body: vec![Stmt::Let { name: "n".to_owned(), expr: Expr::Length(Box::new(p("items"))) }],
      render: Tmpl::Element {
        tag: "p".to_owned(),
        attrs: vec![Entry::Field("class".to_owned(), Expr::lit_str("count")), Entry::Field("hidden".to_owned(), Expr::Lit(Lit::Bool(false))), Entry::Field("title".to_owned(), Expr::Lit(Lit::Null))],
        children: vec![Tmpl::Expr(Expr::var("n")), Tmpl::Text(" result".to_owned()), Tmpl::Expr(Expr::Ternary(Box::new(Expr::Compare(crate::ast::CompareOp::Eq, Box::new(Expr::var("n")), Box::new(Expr::Lit(Lit::Float(1.0))))), Box::new(Expr::lit_str("")), Box::new(Expr::lit_str("s")))), Tmpl::Text(" <3".to_owned())],
      }, state: Vec::new(), handlers: Vec::new()
    };
    let html = (Interpreter::default().render(&component, &props(&[("items", Value::Seq(vec![Value::Null, Value::Null]))]), &Components::new())).unwrap().html;
    assert_eq!(html, "<p class=\"count\">2<!-- --> result<!-- -->s<!-- --> &lt;3</p>");
  }

  #[test]
  fn for_if_and_let_are_the_jsx_idioms() {
    let component = Component {
      body: Vec::new(),
      render: Tmpl::Element {
        tag: "ul".to_owned(),
        attrs: Vec::new(),
        children: vec![Tmpl::For {
          over: p("lines"),
          params: vec!["l".to_owned(), "i".to_owned()],
          body: Box::new(Tmpl::Let {
            name: "qty".to_owned(),
            expr: Expr::Num(Box::new(Expr::var("l").field("quantity"))),
            then: Box::new(Tmpl::Element {
              tag: "li".to_owned(),
              attrs: vec![Entry::Field("data-i".to_owned(), Expr::var("i"))],
              children: vec![Tmpl::Expr(Expr::var("qty")), Tmpl::If { cond: Expr::Compare(crate::ast::CompareOp::Gt, Box::new(Expr::var("qty")), Box::new(Expr::Lit(Lit::Float(1.0)))), then: Box::new(Tmpl::Element { tag: "b".to_owned(), attrs: Vec::new(), children: vec![Tmpl::Text("many".to_owned())] }), r#else: None }],
            }),
          }),
        }],
      }, state: Vec::new(), handlers: Vec::new()
    };
    let lines = Value::Seq(vec![Value::Map(props(&[("quantity", Value::Int(1))])), Value::Map(props(&[("quantity", Value::Int(3))]))]);
    let html = (Interpreter::default().render(&component, &props(&[("lines", lines)]), &Components::new())).unwrap().html;
    assert_eq!(html, "<ul><li data-i=\"0\">1</li><li data-i=\"1\">3<b>many</b></li></ul>");
  }

  #[test]
  fn a_component_renders_another_with_its_own_props() {
    let mut library = Components::new();
    library.insert(
      "src/ui/Stars.tsx#Stars".to_owned(),
      Arc::new(Component {
        body: vec![Stmt::Let { name: "full".to_owned(), expr: Expr::Builtin { name: Builtin::Round, args: vec![p("rating")] } }],
        render: Tmpl::Element {
          tag: "span".to_owned(),
          attrs: vec![Entry::Field("title".to_owned(), Expr::Template(vec![Expr::Builtin { name: Builtin::ToFixed, args: vec![p("rating"), Expr::Lit(Lit::Float(1.0))] }, Expr::lit_str(" out of 5")]))],
          children: vec![Tmpl::Expr(Expr::Arith(crate::ast::ArithOp::Add, Box::new(Expr::Builtin { name: Builtin::Repeat, args: vec![Expr::lit_str("★"), Expr::var("full")] }), Box::new(Expr::Builtin { name: Builtin::Repeat, args: vec![Expr::lit_str("☆"), Expr::Arith(crate::ast::ArithOp::Sub, Box::new(Expr::Lit(Lit::Float(5.0))), Box::new(Expr::var("full")))] })))],
        }, state: Vec::new(), handlers: Vec::new()
      }),
    );
    let page = Component {
      body: Vec::new(),
      render: Tmpl::Fragment(vec![Tmpl::Component { module: "src/ui/Stars.tsx#Stars".to_owned(), props: vec![Entry::Field("rating".to_owned(), p("product").field("rating"))], children: Vec::new() }, Tmpl::Expr(p("product").field("name"))]), state: Vec::new(), handlers: Vec::new()
    };
    let product = Value::Map(props(&[("rating", Value::F64(4.5)), ("name", Value::str("Filament"))]));
    let html = (Interpreter::default().render(&page, &props(&[("product", product)]), &library)).unwrap().html;
    assert_eq!(html, "<span title=\"4.5 out of 5\">★★★★★</span>Filament");
  }

  #[test]
  fn children_render_in_the_callers_scope_and_spreads_merge_in_order() {
    let mut library = Components::new();
    library.insert(
      "src/ui/Page.tsx#Page".to_owned(),
      Arc::new(Component {
        body: Vec::new(),
        render: Tmpl::Element {
          tag: "main".to_owned(),
          attrs: vec![Entry::Field("class".to_owned(), p("className"))],
          children: vec![Tmpl::Element { tag: "h1".to_owned(), attrs: Vec::new(), children: vec![Tmpl::Expr(p("title"))] }, Tmpl::Component { module: "src/ui/Card.tsx#Card".to_owned(), props: Vec::new(), children: vec![Tmpl::Slot("content".to_owned())] }],
        }, state: Vec::new(), handlers: Vec::new()
      }),
    );
    library.insert(
      "src/ui/Card.tsx#Card".to_owned(),
      Arc::new(Component { body: Vec::new(), render: Tmpl::Element { tag: "div".to_owned(), attrs: vec![Entry::Field("class".to_owned(), Expr::lit_str("card"))], children: vec![Tmpl::Slot("content".to_owned()), Tmpl::Slot("content".to_owned())] }, state: Vec::new(), handlers: Vec::new() }),
    );
    let page = Component {
      body: vec![Stmt::Let { name: "header".to_owned(), expr: Expr::Object(vec![Entry::Field("title".to_owned(), Expr::lit_str("Picks")), Entry::Field("className".to_owned(), Expr::lit_str("wrong"))]) }],
      render: Tmpl::Component {
        module: "src/ui/Page.tsx#Page".to_owned(),
        props: vec![Entry::Spread(Expr::var("header")), Entry::Field("className".to_owned(), Expr::lit_str("catalog"))],
        children: vec![Tmpl::For {
          over: p("items"),
          params: vec!["it".to_owned()],
          body: Box::new(Tmpl::Element { tag: "p".to_owned(), attrs: vec![Entry::Spread(Expr::var("it").field("attrs")), Entry::Field("class".to_owned(), Expr::lit_str("item"))], children: vec![Tmpl::Expr(Expr::var("it").field("name")), Tmpl::Text(" for ".to_owned()), Tmpl::Expr(p("title"))] }),
        }],
      }, state: Vec::new(), handlers: Vec::new()
    };
    let mut attrs = ValueMap::new();
    attrs.insert("className".to_owned(), Value::str("ignored"));
    attrs.insert("dataId".to_owned(), Value::Int(7));
    attrs.insert("onClick".to_owned(), Value::str("handler"));
    attrs.insert("hidden".to_owned(), Value::Bool(true));
    let items = Value::Seq(vec![Value::Map(props(&[("name", Value::str("A")), ("attrs", Value::Map(attrs))])), Value::Map(props(&[("name", Value::str("B")), ("attrs", Value::Null)]))]);
    let html = (Interpreter::default().render(&page, &props(&[("items", items), ("title", Value::str("outer"))]), &library)).unwrap().html;
    assert_eq!(html, "<main class=\"catalog\"><h1>Picks</h1><div class=\"card\"><p class=\"item\" dataId=\"7\" hidden=\"\">A<!-- --> for <!-- -->outer</p><p class=\"item\">B<!-- --> for <!-- -->outer</p><p class=\"item\" dataId=\"7\" hidden=\"\">A<!-- --> for <!-- -->outer</p><p class=\"item\">B<!-- --> for <!-- -->outer</p></div></main>");
  }

  #[test]
  fn void_elements_boolean_attributes_and_escaping() {
    let component = Component {
      body: Vec::new(),
      render: Tmpl::Fragment(vec![
        Tmpl::Element { tag: "input".to_owned(), attrs: vec![Entry::Field("value".to_owned(), Expr::lit_str("a \"b\" & c")), Entry::Field("disabled".to_owned(), Expr::Lit(Lit::Bool(true))), Entry::Field("aria-hidden".to_owned(), Expr::Lit(Lit::Bool(true)))], children: Vec::new() },
        Tmpl::Element { tag: "br".to_owned(), attrs: Vec::new(), children: Vec::new() },
        Tmpl::Expr(Expr::Lit(Lit::Bool(true))),
        Tmpl::Expr(Expr::Lit(Lit::Null)),
      ]), state: Vec::new(), handlers: Vec::new()
    };
    let html = (Interpreter::default().render(&component, &ValueMap::new(), &Components::new())).unwrap().html;
    assert_eq!(html, "<input value=\"a &quot;b&quot; &amp; c\" disabled=\"\" aria-hidden=\"true\"/><br/>");
  }

  #[test]
  fn a_store_read_takes_the_seed_and_falls_back_without_one() {
    let read = Expr::Coalesce(Box::new(Expr::Store("cart/count".to_owned())), Box::new(Expr::Lit(Lit::Float(0.0))));
    let inner = Component { body: vec![Stmt::Let { name: "n".to_owned(), expr: read.clone() }], render: Tmpl::Element { tag: "b".to_owned(), attrs: Vec::new(), children: vec![Tmpl::Expr(Expr::var("n"))] }, state: Vec::new(), handlers: Vec::new() };
    let mut library = Components::new();
    library.insert("src/ui/Badge.tsx#Badge".to_owned(), Arc::new(inner));
    let outer = Component {
      body: vec![Stmt::Let { name: "n".to_owned(), expr: read }],
      render: Tmpl::Fragment(vec![
        Tmpl::Expr(Expr::var("n")),
        Tmpl::Component { module: "src/ui/Badge.tsx#Badge".to_owned(), props: Vec::new(), children: Vec::new() },
      ]), state: Vec::new(), handlers: Vec::new()
    };
    let render = |props: ValueMap| Interpreter::default().render(&outer, &props, &library).unwrap().html;
    assert_eq!(render(ValueMap::new()), "0<b>0</b>", "no seed leaves both reads on the fallback");
    let mut store = ValueMap::new();
    store.insert("cart/count".to_owned(), Value::Int(3));
    let mut props = ValueMap::new();
    props.insert("$store".to_owned(), Value::Map(store));
    assert_eq!(render(props), "3<b>3</b>", "a nested component reads the seed without a prop");
  }

  #[test]
  fn a_slot_placement_shows_its_fallback_until_the_plan_fills_it() {
    let filled = Expr::Builtin { name: Builtin::Includes, args: vec![Expr::Coalesce(Box::new(Expr::Var("$props".to_owned()).field("$slots")), Box::new(Expr::Array(Vec::new()))), Expr::lit_str("modal")] };
    let component = Component {
      body: Vec::new(),
      render: Tmpl::Element { tag: "sf-s".to_owned(), attrs: Vec::new(), children: vec![Tmpl::If { cond: filled, then: Box::new(Tmpl::Slot("modal".to_owned())), r#else: Some(Box::new(Tmpl::Text("closed".to_owned()))) }] }, state: Vec::new(), handlers: Vec::new()
    };
    let render = |props: ValueMap| Interpreter::default().render(&component, &props, &Components::new()).unwrap().html;
    assert_eq!(render(ValueMap::new()), "<sf-s>closed</sf-s>", "no $slots at all shows the fallback");
    let mut props = ValueMap::new();
    props.insert("$slots".to_owned(), Value::Seq(vec![Value::str("content")]));
    assert_eq!(render(props.clone()), "<sf-s>closed</sf-s>");
    props.insert("$slots".to_owned(), Value::Seq(vec![Value::str("content"), Value::str("modal")]));
    assert_eq!(render(props), format!("<sf-s>{}</sf-s>", slot_mark("modal")));
  }

  #[test]
  fn builtins_follow_javascript() {
    let cases: Vec<(Expr, &str)> = vec![
      (Expr::Builtin { name: Builtin::ToFixed, args: vec![Expr::Lit(Lit::Float(24.0)), Expr::Lit(Lit::Float(2.0))] }, "24.00"),
      (Expr::Builtin { name: Builtin::ToFixed, args: vec![Expr::Lit(Lit::Float(2.345)), Expr::Lit(Lit::Float(2.0))] }, "2.35"),
      (Expr::Builtin { name: Builtin::Round, args: vec![Expr::Lit(Lit::Float(2.5))] }, "3"),
      (Expr::Builtin { name: Builtin::Round, args: vec![Expr::Lit(Lit::Float(-2.5))] }, "-2"),
      (Expr::Builtin { name: Builtin::LocaleNumber, args: vec![Expr::Lit(Lit::Float(1234567.0))] }, "1,234,567"),
      (Expr::Builtin { name: Builtin::LocaleNumber, args: vec![Expr::Lit(Lit::Int(1834))] }, "1,834"),
      (Expr::Builtin { name: Builtin::Join, args: vec![Expr::Array(vec![crate::ast::Entry::Item(Expr::lit_str("A1")), crate::ast::Entry::Item(Expr::lit_str("B2"))]), Expr::lit_str(", ")] }, "A1, B2"),
      (Expr::Builtin { name: Builtin::EncodeUriComponent, args: vec![Expr::lit_str("a b&c/é")] }, "a%20b%26c%2F%C3%A9"),
      (Expr::Builtin { name: Builtin::Min, args: vec![Expr::Lit(Lit::Int(12)), Expr::Lit(Lit::Float(10.0))] }, "10"),
      (Expr::Map(Box::new(Expr::Builtin { name: Builtin::Range, args: vec![Expr::Lit(Lit::Float(3.0))] }), Box::new(Expr::lambda(&["_", "i"], Expr::Arith(crate::ast::ArithOp::Add, Box::new(Expr::var("i")), Box::new(Expr::Lit(Lit::Float(1.0))))))), "1<!-- -->2<!-- -->3"),
    ];
    for (expr, expected) in cases {
      let component = Component { body: Vec::new(), render: Tmpl::Expr(expr.clone()), state: Vec::new(), handlers: Vec::new() };
      let html = (Interpreter::default().render(&component, &ValueMap::new(), &Components::new())).unwrap().html;
      assert_eq!(html, expected, "{expr:?}");
    }
  }
}

#[cfg(test)]
mod hoist_tests {
  use super::*;
  use crate::ast::{Builtin, Entry, Expr, Lit, Stmt, Tmpl};

  fn hoist(id: u32, expr: Expr) -> Expr {
    Expr::Hoist { id, expr: Box::new(expr) }
  }

  fn fixed(expr: Expr) -> Expr {
    Expr::Builtin { name: Builtin::ToFixed, args: vec![expr, Expr::Lit(Lit::Float(1.0))] }
  }

  #[test]
  fn hoisted_values_are_keyed_by_module_id_and_loop_indices() {
    let component = Component {
      body: vec![Stmt::Let { name: "total".to_owned(), expr: hoist(0, fixed(Expr::var("$props").field("total"))) }],
      render: Tmpl::Fragment(vec![
        Tmpl::Expr(Expr::var("total")),
        Tmpl::For {
          over: Expr::var("$props").field("prices"),
          params: vec!["p".to_owned()],
          body: Box::new(Tmpl::For {
            over: Expr::var("$props").field("taxes"),
            params: vec!["t".to_owned()],
            body: Box::new(Tmpl::Expr(hoist(1, fixed(Expr::Arith(crate::ast::ArithOp::Mul, Box::new(Expr::var("p")), Box::new(Expr::var("t"))))))),
          }),
        },
      ]), state: Vec::new(), handlers: Vec::new()
    };
    let mut props = ValueMap::new();
    props.insert("total".to_owned(), Value::F64(2.5));
    props.insert("prices".to_owned(), Value::Seq(vec![Value::F64(1.0), Value::F64(2.0)]));
    props.insert("taxes".to_owned(), Value::Seq(vec![Value::F64(1.0), Value::F64(1.5)]));
    let rendered = Interpreter::default().render_module("src/ui/Bill.tsx#Bill", &component, &props, &Components::new()).unwrap();
    assert_eq!(rendered.html, "2.5<!-- -->1.0<!-- -->1.5<!-- -->2.0<!-- -->3.0");
    let keys: Vec<&String> = rendered.hoisted.keys().collect();
    assert_eq!(keys, ["src/ui/Bill.tsx#Bill|0", "src/ui/Bill.tsx#Bill|1@0.0", "src/ui/Bill.tsx#Bill|1@0.1", "src/ui/Bill.tsx#Bill|1@1.0", "src/ui/Bill.tsx#Bill|1@1.1"]);
    assert_eq!(rendered.hoisted["src/ui/Bill.tsx#Bill|1@1.1"], Value::str("3.0"));
    assert!(Interpreter::default().render(&component, &props, &Components::new()).unwrap().hoisted.contains_key("|0"), "the plain render keys under an empty module");
  }

  #[test]
  fn a_nested_component_keys_by_its_own_module_below_its_callers_loops_and_a_collision_is_dropped() {
    let mut library = Components::new();
    library.insert(
      "src/ui/Price.tsx#Price".to_owned(),
      Arc::new(Component { body: Vec::new(), render: Tmpl::Expr(hoist(0, fixed(Expr::var("$props").field("cents")))), state: Vec::new(), handlers: Vec::new() }),
    );
    let price = |cents: Expr| Tmpl::Component { module: "src/ui/Price.tsx#Price".to_owned(), props: vec![Entry::Field("cents".to_owned(), cents)], children: Vec::new() };
    let page = Component {
      body: Vec::new(),
      render: Tmpl::Fragment(vec![
        price(Expr::Lit(Lit::Float(1.0))),
        Tmpl::For { over: Expr::var("$props").field("items"), params: vec!["it".to_owned()], body: Box::new(price(Expr::var("it"))) },
        Tmpl::Expr(hoist(0, fixed(Expr::Lit(Lit::Float(9.0))))),
      ]), state: Vec::new(), handlers: Vec::new()
    };
    let mut props = ValueMap::new();
    props.insert("items".to_owned(), Value::Seq(vec![Value::F64(2.0), Value::F64(3.0)]));
    let rendered = Interpreter::default().render_module("routes/index/page.tsx#default", &page, &props, &library).unwrap();
    assert_eq!(rendered.html, "1.0<!-- -->2.0<!-- -->3.0<!-- -->9.0");
    let keys: Vec<&String> = rendered.hoisted.keys().collect();
    assert_eq!(keys, ["src/ui/Price.tsx#Price|0", "src/ui/Price.tsx#Price|0@0", "src/ui/Price.tsx#Price|0@1", "routes/index/page.tsx#default|0"]);
    assert_eq!(rendered.hoisted["src/ui/Price.tsx#Price|0@1"], Value::str("3.0"));

    let twice = Component { body: Vec::new(), render: Tmpl::Fragment(vec![price(Expr::Lit(Lit::Float(1.0))), price(Expr::Lit(Lit::Float(1.0))), price(Expr::Lit(Lit::Float(2.0)))]), state: Vec::new(), handlers: Vec::new() };
    let rendered = Interpreter::default().render_module("routes/index/page.tsx#default", &twice, &ValueMap::new(), &library).unwrap();
    assert!(rendered.hoisted.is_empty(), "Price placed three times outside a loop shares one key: 1.0 twice agrees, 2.0 drops it: {:?}", rendered.hoisted);
  }

  #[test]
  fn a_chunk_records_its_inner_markup_and_the_markers_never_print() {
    let component = Component {
      body: Vec::new(),
      render: Tmpl::Element {
        tag: "ul".to_owned(),
        attrs: vec![Entry::Field("class".to_owned(), Expr::lit_str("list")), Entry::Field(CHUNK_ATTR.to_owned(), Expr::Lit(Lit::Int(4)))],
        children: vec![Tmpl::For {
          over: Expr::var("$props").field("items"),
          params: vec!["it".to_owned()],
          body: Box::new(Tmpl::Element {
            tag: "li".to_owned(),
            attrs: vec![Entry::Field("$bound".to_owned(), Expr::Lit(Lit::Bool(true)))],
            children: vec![Tmpl::Expr(hoist(1, fixed(Expr::var("it"))))],
          }),
        }],
      }, state: Vec::new(), handlers: Vec::new()
    };
    let mut props = ValueMap::new();
    props.insert("items".to_owned(), Value::Seq(vec![Value::F64(1.0), Value::F64(2.0)]));
    let rendered = Interpreter::default().render_module("routes/index/page.tsx#default", &component, &props, &Components::new()).unwrap();
    assert_eq!(rendered.html, "<ul class=\"list\"><li>1.0</li><li>2.0</li></ul>");
    assert_eq!(rendered.hoisted["routes/index/page.tsx#default|4"], Value::str("<li>1.0</li><li>2.0</li>"));
    assert_eq!(rendered.hoisted["routes/index/page.tsx#default|1@1"], Value::str("2.0"), "a value inside a chunk is still recorded, for the fallback");
  }

  #[test]
  fn an_island_carries_its_own_table_in_its_props() {
    let mut library = Components::new();
    library.insert(
      "src/ui/Help.tsx#Help".to_owned(),
      Arc::new(Component { body: Vec::new(), render: Tmpl::Expr(hoist(0, fixed(Expr::var("$props").field("n")))), state: Vec::new(), handlers: Vec::new() }),
    );
    let page = Component {
      body: Vec::new(),
      render: Tmpl::Fragment(vec![
        Tmpl::Expr(hoist(0, fixed(Expr::Lit(Lit::Float(1.0))))),
        Tmpl::Island { module: "src/ui/Help.tsx#Help".to_owned(), props: vec![Entry::Field("n".to_owned(), Expr::Lit(Lit::Float(2.0)))], children: Vec::new(), when: None, mode: None },
      ]), state: Vec::new(), handlers: Vec::new()
    };
    let rendered = Interpreter::default().render_module("routes/index/page.tsx#default", &page, &ValueMap::new(), &library).unwrap();
    let keys: Vec<&String> = rendered.hoisted.keys().collect();
    assert_eq!(keys, ["routes/index/page.tsx#default|0"], "the island's values are not the page's");
    assert_eq!(rendered.islands[0].body.hoisted.keys().collect::<Vec<_>>(), ["src/ui/Help.tsx#Help|0"]);
    let mount = rendered.islands[0].mount_props();
    assert_eq!(mount.get("n"), Some(&Value::F64(2.0)));
    let Some(Value::Map(table)) = mount.get(HOISTED_PROP) else { panic!("{mount:?}") };
    assert_eq!(table["src/ui/Help.tsx#Help|0"], Value::str("2.0"));
  }
}

#[cfg(test)]
mod server_tests {
  use super::*;
  use crate::ast::{Entry, Expr, Handler, Lit, Stmt, Tmpl};

  fn help() -> Component {
    let button = Tmpl::Element {
      tag: "button".to_owned(),
      attrs: vec![Entry::Field(format!("{HANDLER_ATTR}click"), Expr::Lit(Lit::Int(0))), Entry::Field(KEY_ATTR.to_owned(), Expr::lit_str("toggle"))],
      children: vec![Tmpl::Expr(Expr::Ternary(Box::new(Expr::var("open")), Box::new(Expr::lit_str("Hide")), Box::new(Expr::lit_str("Show"))))],
    };
    let list = Tmpl::If { cond: Expr::var("open"), then: Box::new(Tmpl::Element { tag: "ul".to_owned(), attrs: Vec::new(), children: vec![Tmpl::Text("mail".to_owned())] }), r#else: None };
    Component {
      body: vec![Stmt::Let { name: "open".to_owned(), expr: Expr::Lit(Lit::Bool(false)) }, Stmt::Let { name: "label".to_owned(), expr: Expr::Template(vec![Expr::lit_str("order "), Expr::var("$props").field("id")]) }],
      render: Tmpl::Element { tag: "section".to_owned(), attrs: Vec::new(), children: vec![Tmpl::Expr(Expr::var("label")), button, list] },
      state: vec!["open".to_owned()],
      handlers: vec![Handler { event: "click".to_owned(), body: vec![Stmt::Return(Expr::Object(vec![Entry::Field("open".to_owned(), Expr::Not(Box::new(Expr::var("open"))))]))] }],
    }
  }

  #[test]
  fn handler_markers_and_keys_print_only_in_server_mode() {
    let mut library = Components::new();
    library.insert("src/ui/Help.tsx#Help".to_owned(), Arc::new(help()));
    let island = |mode: Option<&str>| Component { body: Vec::new(), render: Tmpl::Island { module: "src/ui/Help.tsx#Help".to_owned(), props: vec![Entry::Field("id".to_owned(), Expr::Lit(Lit::Int(7)))], children: Vec::new(), when: None, mode: mode.map(str::to_owned) }, state: Vec::new(), handlers: Vec::new() };
    let browser = Interpreter::default().render_module("page", &island(None), &ValueMap::new(), &library).unwrap();
    assert_eq!(browser.islands[0].body.html, "<section>order 7<button>Show</button></section>");
    assert!(browser.islands[0].mode.is_none() && !browser.islands[0].mount_props().contains_key(STATE_PROP));
    let server = Interpreter::default().render_module("page", &island(Some("server")), &ValueMap::new(), &library).unwrap();
    assert_eq!(server.islands[0].body.html, "<section>order 7<button data-sf-key=\"toggle\" data-sf-on=\"click:0\">Show</button></section>");
    assert_eq!(server.islands[0].mode.as_deref(), Some("server"));
    let props = server.islands[0].mount_props();
    assert_eq!(props.get(STATE_PROP), Some(&Value::Map(ValueMap::from_iter([("open".to_owned(), Value::Bool(false))]))), "{props:?}");
    let nodes = crate::bind::rendered_nodes(&server);
    assert_eq!(nodes[0], snapfire_fsr_core::Node::raw("<sf-s data-sf-island data-sf-mode=\"server\">"));
  }

  #[test]
  fn a_step_runs_the_handler_over_the_state_and_renders_from_the_result() {
    let library = Components::new();
    let component = help();
    let mut props = ValueMap::new();
    props.insert("id".to_owned(), Value::Int(7));
    let state = ValueMap::from_iter([("open".to_owned(), Value::Bool(false))]);
    let stepped = Interpreter::default().island_step("src/ui/Help.tsx#Help", &component, &props, &state, Some(0), &Value::Null, &library).unwrap();
    assert_eq!(stepped.state, ValueMap::from_iter([("open".to_owned(), Value::Bool(true))]));
    assert_eq!(stepped.rendered.html, "<section>order 7<button data-sf-key=\"toggle\" data-sf-on=\"click:0\">Hide</button><ul>mail</ul></section>");
    let again = Interpreter::default().island_step("src/ui/Help.tsx#Help", &component, &props, &stepped.state, Some(0), &Value::Null, &library).unwrap();
    assert_eq!(again.state["open"], Value::Bool(false));
    let as_is = Interpreter::default().island_step("src/ui/Help.tsx#Help", &component, &props, &stepped.state, None, &Value::Null, &library).unwrap();
    assert_eq!(as_is.state, stepped.state, "no handler renders from the state given");
    assert!(as_is.rendered.html.contains("<ul>mail</ul>"));
    let missing = Interpreter::default().island_step("src/ui/Help.tsx#Help", &component, &props, &state, Some(3), &Value::Null, &library).unwrap_err();
    assert!(missing.message.contains("no handler 3"), "{}", missing.message);
  }
}

#[cfg(test)]
mod island_tests {
  use super::*;
  use crate::ast::{Entry, Expr, Tmpl};
  use crate::bind::rendered_nodes;
  use snapfire_fsr_core::Node;

  #[test]
  fn an_island_renders_apart_and_binds_as_a_nested_client_node_in_a_region() {
    let mut library = Components::new();
    library.insert(
      "src/ui/Help.tsx#Help".to_owned(),
      Arc::new(Component { body: Vec::new(), render: Tmpl::Element { tag: "p".to_owned(), attrs: Vec::new(), children: vec![Tmpl::Text("help ".to_owned()), Tmpl::Expr(Expr::var("$props").field("id"))] }, state: Vec::new(), handlers: Vec::new() }),
    );
    let page = Component {
      body: Vec::new(),
      render: Tmpl::Element {
        tag: "main".to_owned(),
        attrs: Vec::new(),
        children: vec![
          Tmpl::Text("before".to_owned()),
          Tmpl::Island { module: "src/ui/Help.tsx#Help".to_owned(), props: vec![Entry::Field("id".to_owned(), Expr::var("$props").field("id"))], children: Vec::new(), when: Some("visible".to_owned()), mode: None },
          Tmpl::Text("after".to_owned()),
        ],
      }, state: Vec::new(), handlers: Vec::new()
    };
    let mut props = ValueMap::new();
    props.insert("id".to_owned(), Value::int(7i64));
    let rendered = Interpreter::default().render(&page, &props, &library).unwrap();
    assert_eq!(rendered.html, format!("<main>before{ISLAND_MARK}0\u{0}after</main>"));
    assert_eq!(rendered.islands.len(), 1);
    assert_eq!(rendered.islands[0].body.html, "<p>help <!-- -->7</p>");
    assert_eq!(rendered.islands[0].when.as_deref(), Some("visible"));

    let nodes = rendered_nodes(&rendered);
    assert_eq!(nodes.len(), 5, "{nodes:?}");
    assert_eq!(nodes[0], Node::raw("<main>before"));
    assert_eq!(nodes[1], Node::raw("<sf-s data-sf-island data-sf-when=\"visible\">"));
    let Node::Client { module, props: island_props, ssr: Some(body), .. } = &nodes[2] else { panic!("{:?}", nodes[2]) };
    assert_eq!(module.to_string(), "src/ui/Help.tsx#Help");
    assert_eq!(island_props.get("id"), Some(&Value::int(7i64)));
    assert_eq!(**body, Node::raw("<p>help <!-- -->7</p>"));
    assert_eq!(nodes[3], Node::raw("</sf-s>"));
    assert_eq!(nodes[4], Node::raw("after</main>"));
  }
}
