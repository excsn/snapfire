//! Renders a lowered component to HTML. The tree is the component's own, so
//! the serialiser is a string builder: elements, escaped text and the three
//! idioms. What the browser hydrates over is exactly this output, so it
//! follows React's server renderer byte for byte: adjacent text nodes are
//! separated by an empty comment, empty text writes nothing, a boolean
//! attribute is `name=""`, a void element closes with `/>`.

use std::borrow::Cow;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use snapfire_fsr_core::{Value, ValueMap};

use snapfire_fsr_runtime::FailureKind;

use crate::ast::{Component, Entry, ShadowMode, ShadowRoot, Stmt, Tmpl};
use crate::interp::{Env, Fail, Hoists, Interpreter, Probe, Step, stringify, truthy};

mod markup;
mod react;
mod react18;
mod react19;
mod vue;

pub use markup::Frameworks;
pub(crate) use markup::Markup;
use react::is_custom_element;
pub use react::ReactMajor;
pub use vue::VueMajor;

/// Every lowered component by module id, so one may render another.
pub type Components = HashMap<String, Arc<Component>>;

const VOID: &[&str] = &["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track", "wbr"];

/// Attributes that are present or absent rather than valued under every React
/// major. React 19 adds `inert`.
const BOOLEAN: &[&str] = &["disabled", "checked", "selected", "readonly", "required", "hidden", "multiple", "open", "autofocus", "autoplay", "controls", "loop", "muted", "novalidate", "defer", "async"];

fn escape_into(input: &str, out: &mut String, quotes: bool) {
  let bytes = input.as_bytes();
  let first = match quotes {
    true => match (memchr::memchr3(b'&', b'<', b'>', bytes), memchr::memchr(b'"', bytes)) {
      (Some(a), Some(b)) => Some(a.min(b)),
      (a, b) => a.or(b),
    },
    false => memchr::memchr3(b'&', b'<', b'>', bytes),
  };
  let Some(first) = first else {
    out.push_str(input);
    return;
  };
  out.push_str(&input[..first]);
  let mut last = first;
  for (offset, byte) in bytes[first..].iter().enumerate() {
    let replacement = match byte {
      b'&' => "&amp;",
      b'<' => "&lt;",
      b'>' => "&gt;",
      b'"' if quotes => "&quot;",
      _ => continue,
    };
    let i = first + offset;
    out.push_str(&input[last..i]);
    out.push_str(replacement);
    last = i + 1;
  }
  out.push_str(&input[last..]);
}
fn escape_text(input: &str, out: &mut String) {
  escape_into(input, out, false);
}

fn escape_attr(input: &str, out: &mut String) {
  escape_into(input, out, true);
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
  /// The region this placement owns, keyed as `Hoists::key` keys a hoist: the
  /// enclosing component's module, the placement's id and the loop path. The
  /// browser derives the same string, which is how a re-render pairs an
  /// island with the payload that describes it.
  pub key: String,
  pub body: Rendered,
}

/// The props key a server-mode island's initial state rides under.
pub const STATE_PROP: &str = "$s";

/// The props key an island's region key rides under.
pub const KEY_PROP: &str = "$k";

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
    if !self.key.is_empty() {
      props.insert(KEY_PROP.to_owned(), Value::str(self.key.clone()));
    }
    props
  }
}

/// The island mode whose events round-trip to the server.
pub const SERVER_MODE: &str = "server";

/// What `island_step` answers: the state after the handler, the island rendered from it and the actions the handler asked for, in order, for the host to dispatch.
#[derive(Debug, Clone, PartialEq)]
pub struct Stepped {
  pub state: ValueMap,
  pub rendered: Rendered,
  pub acts: Vec<(String, Value)>,
}

/// The handler a step runs: an index into the handlers of the component at
/// `path`, which is empty for the island's own component and otherwise the
/// address a nested component's markers carry, `c1` or `0.c2`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandlerRef {
  pub path: String,
  pub index: usize,
}

impl HandlerRef {
  pub fn own(index: usize) -> Self {
    Self { path: String::new(), index }
  }

  /// A token as an element binds it: `2` for the island's own handler,
  /// `c1/2` for one of a component inside it. `None` for anything else.
  pub fn parse(token: &str) -> Option<Self> {
    let (path, index) = token.rsplit_once('/').unwrap_or(("", token));
    if index.is_empty() || !index.bytes().all(|b| b.is_ascii_digit()) {
      return None;
    }
    Some(Self { path: path.to_owned(), index: index.parse().ok()? })
  }
}

/// The key a component's state binding takes in an island's state map: the
/// name at the island's own component, `<path>/<name>` inside it.
fn state_key(path: &str, name: &str) -> String {
  if path.is_empty() {
    name.to_owned()
  } else {
    let mut key = String::with_capacity(path.len() + name.len() + 1);
    key.push_str(path);
    key.push('/');
    key.push_str(name);
    key
  }
}

/// The start of an island's place in the markup: `ISLAND_MARK`, the island's
/// index in decimal, then a NUL.
pub const ISLAND_MARK: &str = "\u{0}sf-island:";

impl Out {
  /// Text under `markup`'s rules: React separates two adjacent runs with an
  /// empty comment and escapes three characters; Vue joins them and escapes
  /// five.
  fn text(&mut self, text: &str, markup: Markup) {
    if text.is_empty() {
      return;
    }
    if markup.is_vue() {
      vue::escape(text, &mut self.html);
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

  fn close_tag(&mut self, tag: &str) {
    self.html.push_str("</");
    self.html.push_str(tag);
    self.html.push('>');
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
struct Slot<'a> {
  children: &'a [Tmpl],
  scope: Rc<Vec<(String, Value)>>,
  /// The hoist module and path of the caller, which its children key under:
  /// the browser builds them in the caller's render.
  keys: Option<(String, Vec<Step>)>,
  /// An island's children, which are the server's markup inside the island
  /// rather than part of its render: see `render_children`.
  island: bool,
  /// Whether the callee placed the slot: a lowered island whose template
  /// did not carries its children apart, see `CHILDREN_HELD_OPEN`.
  placed: bool,
}

/// The region an island's children render in, which the mounter hands the
/// component as its `children` and never renders itself.
pub const CHILDREN_OPEN: &str = "<sf-s data-sf-children>";

/// Where a lowered Vue island's children go when its template did not place
/// its slot this render, a closed panel for one: an inert template after the
/// island's markup, which the parser never shows, the scan never reaches and
/// the mounter reads so the slot has its content when the template opens.
pub const CHILDREN_HELD_OPEN: &str = "<template data-sf-children>";

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
    if let Some(Value::Str(path)) = props.get(snapfire_fsr_runtime::PATH_PROP) {
      env.ctx.path = path.to_string();
    }
    if let Some(Value::Str(document)) = props.get(snapfire_fsr_runtime::DOCUMENT_PROP) {
      env.ctx.document = Some(document.to_string());
    }
    env.hoists = Some(Hoists::new(module));
    env
  }

  /// One step of an island in server mode: the component's `let`s run with
  /// `state` standing in for its state bindings, `handler` runs with
  /// `$props`, `$state` and `$event` bound and the object it returns is
  /// merged into the state, then the component renders from that state with
  /// handler markers printed. `None` for `handler` renders without a step. A
  /// handler of a component rendered inside the island is found by rendering
  /// once to the path its token names, which gives the component there the
  /// props the island's render gave it; its state is the map's entries under
  /// that path. The answered state is what the render consumed, so a
  /// component the render no longer places leaves no state behind.
  pub fn island_step(&self, module: &str, component: &Component, props: &ValueMap, state: &ValueMap, handler: Option<HandlerRef>, event: &Value, library: &Components) -> Result<Stepped, Fail> {
    let mut env = self.env_for(module, props);
    env.server_mode = true;
    let mut state = state.clone();
    if let Some(handler) = handler {
      let (target, own_props, path): (&Component, Option<ValueMap>, String) = if handler.path.is_empty() {
        (component, None, String::new())
      } else {
        env.state = Some(state.clone());
        env.probe = Some(Probe { path: handler.path.clone(), found: None });
        let mut scratch = Out::default();
        let mut slots = Vec::new();
        render_component(&mut env, component, library, &mut slots, &mut scratch)?;
        env.hoists = Some(Hoists::new(module));
        let Some((found, found_props)) = env.probe.take().and_then(|p| p.found) else {
          return Err(Fail::new(FailureKind::NotFound, format!("`{module}` renders nothing at `{}`", handler.path)));
        };
        let target = library.get(&found).ok_or_else(|| Fail::internal(format!("`{found}` is not a lowered component")))?;
        (target.as_ref(), Some(found_props), handler.path.clone())
      };
      let Some(body) = target.handlers.get(handler.index) else {
        return Err(Fail::new(FailureKind::NotFound, format!("`{module}` has no handler {} at `{}`", handler.index, handler.path)));
      };
      env.state = Some(state.clone());
      let depth = env.scope.len();
      if let Some(own) = own_props {
        env.scope.push(("$props".to_owned(), Value::Map(own)));
      }
      for stmt in &target.body {
        let Stmt::Let { name, expr } = stmt else { continue };
        let value = match state.get(&state_key(&path, name)) {
          Some(held) => held.clone(),
          None => env.eval_sync(expr)?,
        };
        env.scope.push((name.clone(), value));
      }
      let own_state = if path.is_empty() { state.clone() } else { target.state.iter().filter_map(|name| state.get(&state_key(&path, name)).map(|value| (name.clone(), value.clone()))).collect() };
      env.scope.push(("$state".to_owned(), Value::Map(own_state)));
      env.scope.push(("$event".to_owned(), event.clone()));
      let patch = eval_body_sync(&mut env, &body.body)?;
      env.scope.truncate(depth);
      if let Value::Map(patch) = patch {
        for (name, value) in patch {
          if target.state.contains(&name) {
            state.insert(state_key(&path, &name), value);
          }
        }
      }
    }
    env.state = Some(state);
    let mut out = Out::default();
    let mut slots = Vec::new();
    render_component(&mut env, component, library, &mut slots, &mut out)?;
    let hoisted = env.hoists.take().map(|h| h.table).unwrap_or_default();
    let acts = std::mem::take(&mut env.acts);
    Ok(Stepped { state: out.state, rendered: Rendered { html: out.html, islands: out.islands, hoisted }, acts })
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
      Stmt::Act { action, input } => {
        let input = env.eval_sync(input)?;
        env.acts.push((action.clone(), input));
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

/// Runs `f` with the hoist path extended by `step`: the position of the
/// iteration whose values are recorded inside or the keyed placement that
/// rendered the component.
fn in_step<T>(env: &mut Env, step: Step, f: impl FnOnce(&mut Env) -> T) -> T {
  if let Some(h) = &mut env.hoists {
    h.path.push(step);
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

/// How deep components may nest in one render. Past it a component that
/// renders itself without a case that stops would overflow the thread's
/// stack, which takes the host down rather than failing the request. A small
/// recursive component takes about 14 KB of stack per level in a debug build
/// and about 2.6 KB in a release build. The limit follows the build profile
/// rather than the host's dev setting, since the frames are the profile's.
pub const MAX_CALLS: usize = if cfg!(debug_assertions) { 96 } else { 512 };

fn call(env: &mut Env, module: &str, f: impl FnOnce(&mut Env) -> Result<(), Fail>) -> Result<(), Fail> {
  if env.calls == MAX_CALLS {
    return Err(Fail::internal(format!("`{module}` is nested {MAX_CALLS} components deep, which a render refuses: a component that renders itself needs a case that renders no further")));
  }
  env.calls += 1;
  let result = f(env);
  env.calls -= 1;
  result
}

fn render_component<'a>(env: &mut Env, component: &'a Component, library: &'a Components, slots: &mut Vec<Slot<'a>>, out: &mut Out) -> Result<(), Fail> {
  let depth = env.scope.len();
  let caller = env.markup;
  env.markup = Markup::of(component.hydrated_by, env.frameworks, caller);
  let path = env.hoists.as_ref().map(|h| h.path_key()).unwrap_or_default();
  if env.probe.as_ref().is_some_and(|probe| probe.found.is_none() && probe.path == path) {
    let module = env.hoists.as_ref().map(|h| h.module.clone()).unwrap_or_default();
    let props = env.scope.iter().rev().find(|(name, _)| name == "$props").and_then(|(_, value)| match value {
      Value::Map(map) => Some(map.clone()),
      _ => None,
    });
    if let Some(probe) = &mut env.probe {
      probe.found = Some((module, props.unwrap_or_default()));
    }
  }
  for stmt in &component.body {
    let Stmt::Let { name, expr } = stmt else {
      return Err(Fail::internal("a component body is `let`s only"));
    };
    let held = if component.state.contains(name) { env.state.as_ref().and_then(|s| s.get(&state_key(&path, name))).cloned() } else { None };
    let value = match held {
      Some(held) => held,
      None => env.eval_sync(expr)?,
    };
    if component.state.contains(name) {
      out.state.insert(state_key(&path, name), value.clone());
    }
    env.scope.push((name.clone(), value));
  }
  let fragment = env.markup.is_vue() && vue_root_fragment(&component.render);
  if fragment {
    out.markup(vue::FRAGMENT_OPEN);
  }
  render(env, &component.render, library, slots, out)?;
  if fragment {
    out.markup(vue::FRAGMENT_CLOSE);
  }
  env.scope.truncate(depth);
  env.markup = caller;
  Ok(())
}

/// Whether Vue writes a component's root as a fragment: more than one root
/// node, at least one of them not text.
fn vue_root_fragment(render: &Tmpl) -> bool {
  match render {
    Tmpl::Fragment(children) => children.len() > 1 && children.iter().any(|c| !matches!(c, Tmpl::Text(_) | Tmpl::Expr(_))),
    _ => false,
  }
}

/// Whether Vue wraps a branch or a loop body in fragment anchors: anything
/// other than exactly one element.
fn vue_wraps(tmpl: &Tmpl) -> bool {
  !matches!(tmpl, Tmpl::Element { .. } | Tmpl::Baked { .. } | Tmpl::For { .. } | Tmpl::If { .. } | Tmpl::Component { .. } | Tmpl::Island { .. } | Tmpl::Slot(_))
}

/// Renders `tmpl` inside fragment anchors when Vue's rules call for them.
fn render_vue_wrapped<'a>(env: &mut Env, tmpl: &'a Tmpl, library: &'a Components, slots: &mut Vec<Slot<'a>>, out: &mut Out) -> Result<(), Fail> {
  let wrapped = env.markup.is_vue() && vue_wraps(tmpl);
  if wrapped {
    out.markup(vue::FRAGMENT_OPEN);
  }
  render(env, tmpl, library, slots, out)?;
  if wrapped {
    out.markup(vue::FRAGMENT_CLOSE);
  }
  Ok(())
}

/// Evaluates attribute or prop entries into one map in order, later entries winning; attributes are keyed by HTML spelling so a spread's `className` and a literal `class` are one key.
/// A component's or an island's props, built straight into the map the callee
/// reads. The map is the deduplication, so there is no intermediate vector and
/// no linear scan for a name already written.
fn props(env: &mut Env, entries: &[Entry]) -> Result<ValueMap, Fail> {
  let mut map = snapfire_fsr_core::Fields::with_capacity_and_hasher(entries.len(), Default::default());
  for entry in entries {
    match entry {
      Entry::Field(name, expr) => {
        let value = env.eval_sync(expr)?;
        if name != "children" {
          map.insert(name.clone(), value);
        }
      }
      Entry::Spread(expr) => match env.eval_sync(expr)? {
        Value::Map(spread) => {
          for (name, value) in spread {
            if name != "children" {
              map.insert(name, value);
            }
          }
        }
        Value::Null => {}
        other => return Err(crate::interp::type_error("spread", "an object", &other)),
      },
      Entry::Computed(key, expr) => {
        let key = stringify(&env.eval_sync(key)?)?;
        let value = env.eval_sync(expr)?;
        if key != "children" {
          map.insert(key, value);
        }
      }
      Entry::Item(_) => return Err(Fail::internal("an item entry among props")),
    }
  }
  Ok(ValueMap::from(map))
}

fn entries<'a>(env: &mut Env, entries: &'a [Entry], attrs: bool) -> Result<Vec<(Cow<'a, str>, Value)>, Fail> {
  let mut out: Vec<(Cow<'a, str>, Value)> = Vec::with_capacity(entries.len());
  let mut put = |name: Cow<'a, str>, value: Value| {
    let name = match attrs {
      true => match html_attr_name(&name) {
        html if html.len() == name.len() && html == name => name,
        html => Cow::Owned(html.to_owned()),
      },
      false => name,
    };
    if let Some(slot) = out.iter_mut().find(|(n, _)| *n == name) {
      slot.1 = value;
    } else {
      out.push((name, value));
    }
  };
  for entry in entries {
    match entry {
      Entry::Field(name, expr) => put(Cow::Borrowed(name.as_str()), env.eval_sync(expr)?),
      Entry::Spread(expr) => match env.eval_sync(expr)? {
        Value::Map(map) => {
          for (name, value) in map {
            put(Cow::Owned(name), value);
          }
        }
        Value::Null => {}
        other => return Err(crate::interp::type_error("spread", "an object", &other)),
      },
      Entry::Computed(key, expr) => {
        let key = stringify(&env.eval_sync(key)?)?;
        put(Cow::Owned(key), env.eval_sync(expr)?);
      }
      Entry::Item(_) => return Err(Fail::internal("an item entry among attributes")),
    }
  }
  Ok(out)
}

/// The larger arms are functions of their own. This recurses once per level
/// of the tree and a debug build gives every arm's locals a slot in its one
/// frame, so an arm inline here costs its stack at every level.
fn render<'a>(env: &mut Env, tmpl: &'a Tmpl, library: &'a Components, slots: &mut Vec<Slot<'a>>, out: &mut Out) -> Result<(), Fail> {
  match tmpl {
    Tmpl::Text(text) => out.text(text, env.markup),
    Tmpl::Expr(expr) => {
      let value = env.eval_sync(expr)?;
      interpolate(&value, out, env.markup)?;
    }
    Tmpl::Element { tag, attrs, children } => render_element(env, tag, attrs, children, library, slots, out)?,
    Tmpl::Baked { open, tag, children } => {
      out.markup(open);
      if let Some(tag) = tag {
        let context = enter(env, tag);
        for child in children {
          render(env, child, library, slots, out)?;
        }
        (env.in_svg, env.in_noscript) = context;
        out.close_tag(tag);
      }
    }
    Tmpl::Fragment(children) => {
      for child in children {
        render(env, child, library, slots, out)?;
      }
    }
    Tmpl::If { cond, then, r#else } => {
      let value = env.eval_sync(cond)?;
      if truthy(&value) {
        render_vue_wrapped(env, then, library, slots, out)?;
      } else if let Some(other) = r#else {
        render_vue_wrapped(env, other, library, slots, out)?;
      } else if env.markup.is_vue() {
        out.markup(vue::EMPTY);
      }
    }
    Tmpl::For { over, params, body } => render_for(env, over, params, body, library, slots, out)?,
    Tmpl::Let { name, expr, then } => {
      let value = env.eval_sync(expr)?;
      let depth = env.scope.len();
      env.scope.push((name.clone(), value));
      render(env, then, library, slots, out)?;
      env.scope.truncate(depth);
    }
    Tmpl::Component { module, props, children, id, keyed } => render_call(env, module, props, children, *id, *keyed, library, slots, out)?,
    Tmpl::Island { .. } => render_island(env, tmpl, library, slots, out)?,
    Tmpl::Slot(name) => render_slot(env, name, library, slots, out)?,
  }
  Ok(())
}

fn render_element<'a>(env: &mut Env, tag: &str, attrs: &'a [Entry], children: &'a [Tmpl], library: &'a Components, slots: &mut Vec<Slot<'a>>, out: &mut Out) -> Result<(), Fail> {
  let mut open = String::with_capacity(tag.len() + 32);
  open.push('<');
  open.push_str(tag);
  let mut bound = Vec::new();
  let mut raw: Option<String> = None;
  let markup = env.markup;
  let evaluated = entries(env, attrs, !markup.is_vue())?;
  if markup.may_hoist(tag) && !env.server_mode && !env.in_svg && !env.in_noscript && markup.hoists(tag, &evaluated) {
    return Err(hoisted_in_place(env, tag));
  }
  let shadow = match evaluated.iter().find(|(name, _)| name.as_ref() == SHADOW_ATTR) {
    Some((_, module)) => Some((stringify(module)?, evaluated.iter().filter(|(name, _)| !name.starts_with('$')).map(|(name, value)| (name.to_string(), value.clone())).collect::<ValueMap>())),
    None => None,
  };
  for (name, value) in evaluated {
    if shadow.is_some() && !is_scalar(&value) {
      continue;
    }
    if let Some(event) = name.strip_prefix(HANDLER_ATTR) {
      if env.server_mode {
        let path = env.hoists.as_ref().map(|h| h.path_key()).unwrap_or_default();
        bound.push(if path.is_empty() { format!("{event}:{}", stringify(&value)?) } else { format!("{event}:{path}/{}", stringify(&value)?) });
      }
      continue;
    }
    if name == KEY_ATTR {
      if env.server_mode {
        attribute(markup, tag, "data-sf-key", &value, &mut open)?;
      }
      continue;
    }
    if name == RAW_ATTR {
      raw = Some(match value {
        Value::Null => String::new(),
        value => stringify(&value)?,
      });
      continue;
    }
    if skipped_attr(&name) {
      continue;
    }
    attribute(markup, tag, &name, &value, &mut open)?;
  }
  if !bound.is_empty() {
    attribute(markup, tag, "data-sf-on", &Value::str(bound.join(" ")), &mut open)?;
  }
  if VOID.contains(&tag) {
    open.push_str(if markup.is_vue() { ">" } else { "/>" });
    out.markup(&open);
    return Ok(());
  }
  open.push('>');
  out.markup(&open);
  if let Some((module, props)) = shadow {
    render_shadow(env, &module, props, library, slots, out)?;
  }
  let context = enter(env, tag);
  match chunk_id(attrs) {
    Some(id) => {
      let mut inner = Out::default();
      match &raw {
        Some(html) => inner.markup(html),
        None => {
          for child in children {
            render(env, child, library, slots, &mut inner)?;
          }
        }
      }
      out.islands.extend(std::mem::take(&mut inner.islands));
      out.markup(&inner.html);
      if let Some(hoists) = &mut env.hoists {
        hoists.record(id, Value::str(inner.html));
      }
    }
    None => match &raw {
      Some(html) => out.markup(html),
      None => {
        for child in children {
          render(env, child, library, slots, out)?;
        }
      }
    },
  }
  (env.in_svg, env.in_noscript) = context;
  out.close_tag(tag);
  Ok(())
}

/// Enters `tag` for the context React 19 reads before moving `<title>`,
/// `<meta>` and `<link>` into the head. Returns the context it replaced.
fn enter(env: &mut Env, tag: &str) -> (bool, bool) {
  let held = (env.in_svg, env.in_noscript);
  match tag {
    "svg" => env.in_svg = true,
    "foreignObject" => env.in_svg = false,
    "noscript" => env.in_noscript = true,
    _ => {}
  }
  held
}

/// React 19 moves this element into the document head, so the markup the
/// server wrote and the tree the browser builds would not agree.
fn hoisted_in_place(env: &Env, tag: &str) -> Fail {
  let writer = match env.hoists.as_ref().map(|h| h.module.as_str()) {
    Some(module) if !module.is_empty() => format!("`{module}`"),
    _ => "a component".to_owned(),
  };
  Fail::internal(format!("{writer} writes `<{tag}>` in markup the browser hydrates; React 19 moves it into the document head, so it belongs in a loader's `meta` export"))
}

/// A custom element's shadow template, written as a declarative shadow root
/// at the start of the element. It renders with the element's attributes as
/// its props and under plain markup, since the parser takes the shadow root
/// and no framework sees it. The caller writes the light children after it,
/// so a `{children}` in the template is nothing: `<slot>` shows them. Nothing
/// in it is recorded among an island's hoisted values, since the island's
/// browser half never renders a shadow root.
fn render_shadow<'a>(env: &mut Env, module: &str, props: ValueMap, library: &'a Components, slots: &mut Vec<Slot<'a>>, out: &mut Out) -> Result<(), Fail> {
  let component = library.get(module).ok_or_else(|| Fail::internal(format!("`{module}` is not a lowered component")))?;
  let depth = env.scope.len();
  let outer = Rc::new(std::mem::replace(&mut env.scope, vec![("$props".to_owned(), Value::Map(props))]));
  let hoists = env.hoists.take();
  slots.push(Slot { children: &[], scope: Rc::clone(&outer), keys: None, island: false, placed: false });
  let markup = std::mem::replace(&mut env.markup, Markup::Plain);
  shadow_open(component.shadow.unwrap_or_default(), out);
  let result = in_module(env, module, |env| call(env, module, |env| render_component(env, component, library, slots, out)));
  env.markup = markup;
  env.hoists = hoists;
  slots.pop();
  env.scope = Rc::try_unwrap(outer).unwrap_or_else(|held| (*held).clone());
  env.scope.truncate(depth);
  result?;
  out.close_tag("template");
  Ok(())
}

fn shadow_open(shadow: ShadowRoot, out: &mut Out) {
  out.markup(match shadow.mode {
    ShadowMode::Open => "<template shadowrootmode=\"open\"",
    ShadowMode::Closed => "<template shadowrootmode=\"closed\"",
  });
  if shadow.delegates_focus {
    out.markup(" shadowrootdelegatesfocus");
  }
  if shadow.clonable {
    out.markup(" shadowrootclonable");
  }
  if shadow.serializable {
    out.markup(" shadowrootserializable");
  }
  out.markup(">");
}

fn render_for<'a>(env: &mut Env, over: &crate::ast::Expr, params: &[String], body: &'a Tmpl, library: &'a Components, slots: &mut Vec<Slot<'a>>, out: &mut Out) -> Result<(), Fail> {
  let items = match env.eval_sync(over)? {
    Value::Seq(items) => items,
    other => return Err(crate::interp::type_error("map", "an array", &other)),
  };
  let depth = env.scope.len();
  let list = env.markup.is_vue();
  if list {
    out.markup(vue::FRAGMENT_OPEN);
  }
  for i in 0..items.len() {
    env.scope.truncate(depth);
    for (param, value) in params.iter().zip([items[i].clone(), Value::F64(i as f64)]) {
      env.scope.push((param.clone(), value));
    }
    in_step(env, Step::Iteration(i), |env| render_vue_wrapped(env, body, library, slots, out))?;
  }
  if list {
    out.markup(vue::FRAGMENT_CLOSE);
  }
  env.scope.truncate(depth);
  Ok(())
}

#[allow(clippy::too_many_arguments)]
fn render_call<'a>(env: &mut Env, module: &str, props: &[Entry], children: &'a [Tmpl], id: u32, keyed: bool, library: &'a Components, slots: &mut Vec<Slot<'a>>, out: &mut Out) -> Result<(), Fail> {
  let component = library.get(module).ok_or_else(|| Fail::internal(format!("`{module}` is not a lowered component")))?;
  let map = self::props(env, props)?;
  let depth = env.scope.len();
  let outer = Rc::new(std::mem::replace(&mut env.scope, vec![("$props".to_owned(), Value::Map(map))]));
  let keys = env.hoists.as_ref().map(|h| (h.module.clone(), h.path.clone()));
  slots.push(Slot { children, scope: Rc::clone(&outer), keys, island: false, placed: false });
  let mut body =|env: &mut Env| in_module(env, module, |env| call(env, module, |env| render_component(env, component, library, slots, out)));
  let result = if keyed { in_step(env, Step::Placement(id), body) } else { body(env) };
  slots.pop();
  env.scope = Rc::try_unwrap(outer).unwrap_or_else(|held| (*held).clone());
  env.scope.truncate(depth);
  result
}

fn render_island<'a>(env: &mut Env, tmpl: &'a Tmpl, library: &'a Components, slots: &mut Vec<Slot<'a>>, out: &mut Out) -> Result<(), Fail> {
  let Tmpl::Island { module, props, children, when, mode, id, define } = tmpl else { unreachable!("render_island is handed an island") };
  let key = env.hoists.as_ref().map(|h| h.island_key(*id)).unwrap_or_default();
  let map = self::props(env, props)?;
  if *define {
    let mut inner = Out::default();
    for child in children {
      render(env, child, library, slots, &mut inner)?;
    }
    let index = out.islands.len();
    let body = Rendered { html: inner.html, islands: inner.islands, hoisted: ValueMap::default() };
    out.islands.push(RenderedIsland { module: module.clone(), props: ValueMap::default(), when: when.clone(), mode: None, state: ValueMap::default(), key, body });
    out.markup(&format!("{ISLAND_MARK}{index}\0"));
    return Ok(());
  }
  let keys = env.hoists.as_ref().map(|h| (h.module.clone(), h.path.clone()));
  // A component the server has no body for, a `.vue` file among them, is
  // placed with its props and its children and nothing else: the browser
  // mounts it rather than hydrating it and the page around it is whole
  // either way.
  let Some(component) = library.get(module) else {
    let mut inner = Out::default();
    if !children.is_empty() {
      render_children(env, children, keys.as_ref(), library, slots, &mut inner, CHILDREN_OPEN, "sf-s")?;
    }
    let index = out.islands.len();
    out.islands.push(RenderedIsland { module: module.clone(), props: map, when: when.clone(), mode: mode.clone(), state: ValueMap::default(), key, body: Rendered { html: inner.html, islands: inner.islands, hoisted: ValueMap::default() } });
    out.markup(&format!("{ISLAND_MARK}{index}\u{0}"));
    return Ok(());
  };
  let depth = env.scope.len();
  let outer = Rc::new(std::mem::replace(&mut env.scope, vec![("$props".to_owned(), Value::Map(map.clone()))]));
  slots.push(Slot { children, scope: Rc::clone(&outer), keys: keys.clone(), island: true, placed: false });
  let mut inner = Out::default();
  let outer_hoists = env.hoists.replace(Hoists::new(module.clone()));
  let outer_mode = std::mem::replace(&mut env.server_mode, mode.as_deref() == Some(SERVER_MODE));
  let outer_state = env.state.take();
  let result = call(env, module, |env| render_component(env, component, library, slots, &mut inner));
  let held = result.is_ok() && !children.is_empty() && component.hydrated_by == Some(crate::ast::HydratedBy::Vue) && slots.last().is_some_and(|slot| slot.island && !slot.placed);
  let result = match held {
    true => render_children(env, children, keys.as_ref(), library, slots, &mut inner, CHILDREN_HELD_OPEN, "template"),
    false => result,
  };
  env.state = outer_state;
  env.server_mode = outer_mode;
  let hoisted = std::mem::replace(&mut env.hoists, outer_hoists).map(|h| h.table).unwrap_or_default();
  slots.pop();
  env.scope = Rc::try_unwrap(outer).unwrap_or_else(|held| (*held).clone());
  env.scope.truncate(depth);
  result?;
  let index = out.islands.len();
  out.islands.push(RenderedIsland { module: module.clone(), props: map, when: when.clone(), mode: mode.clone(), state: inner.state, key, body: Rendered { html: inner.html, islands: inner.islands, hoisted } });
  out.markup(&format!("{ISLAND_MARK}{index}\u{0}"));
  Ok(())
}

fn render_slot<'a>(env: &mut Env, name: &str, library: &'a Components, slots: &mut Vec<Slot<'a>>, out: &mut Out) -> Result<(), Fail> {
  let Some(slot) = slots.pop() else {
    out.html.push_str(&slot_mark(name));
    return Ok(());
  };
  if env.markup.is_vue() {
    out.markup(vue::FRAGMENT_OPEN);
    let result = render_slot_content(env, slot, library, slots, out);
    out.markup(vue::FRAGMENT_CLOSE);
    return result;
  }
  render_slot_content(env, slot, library, slots, out)
}

fn render_slot_content<'a>(env: &mut Env, mut slot: Slot<'a>, library: &'a Components, slots: &mut Vec<Slot<'a>>, out: &mut Out) -> Result<(), Fail> {
  slot.placed = true;
  let inner = std::mem::replace(&mut env.scope, (*slot.scope).clone());
  let result = match slot.island {
    true => render_children(env, slot.children, slot.keys.as_ref(), library, slots, out, CHILDREN_OPEN, "sf-s"),
    false => {
      let callee_keys = match (&slot.keys, &mut env.hoists) {
        (Some((module, path)), Some(h)) => Some((std::mem::replace(&mut h.module, module.clone()), std::mem::replace(&mut h.path, path.clone()))),
        _ => None,
      };
      let mut result = Ok(());
      for child in slot.children {
        result = render(env, child, library, slots, out);
        if result.is_err() {
          break;
        }
      }
      if let (Some((module, path)), Some(h)) = (callee_keys, &mut env.hoists) {
        h.module = module;
        h.path = path;
      }
      result
    }
  };
  env.scope = inner;
  slots.push(slot);
  result
}

/// An island's children, in a region opened by `open` and closed as `tag`:
/// `CHILDREN_OPEN` where the template places them, `CHILDREN_HELD_OPEN`
/// where it did not. The browser adopts the region's markup and never renders
/// the children. Nothing they hoist is kept and an island among them keys
/// under `keys`, the caller that wrote them.
#[allow(clippy::too_many_arguments)]
fn render_children<'a>(env: &mut Env, children: &'a [Tmpl], keys: Option<&(String, Vec<Step>)>, library: &'a Components, slots: &mut Vec<Slot<'a>>, out: &mut Out, open: &str, tag: &str) -> Result<(), Fail> {
  let caller = keys.map(|(module, path)| {
    let mut hoists = Hoists::new(module.clone());
    hoists.path = path.clone();
    hoists
  });
  let held = std::mem::replace(&mut env.hoists, caller);
  out.markup(open);
  let mut result = Ok(());
  for child in children {
    result = render(env, child, library, slots, out);
    if result.is_err() {
      break;
    }
  }
  out.close_tag(tag);
  env.hoists = held;
  result
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

/// Rewrites a loaded component so every element whose open tag is entirely
/// literal carries that tag as one slice. The plan is untouched: this runs
/// when a component is put into a [`Components`] library and an element it
/// cannot bake is left exactly as it was.
pub fn prepare(component: &Component) -> Component {
  Component { body: component.body.clone(), render: prepare_tmpl(&component.render), state: component.state.clone(), handlers: component.handlers.clone(), hydrated_by: component.hydrated_by, shadow: component.shadow }
}

fn prepare_tmpl(tmpl: &Tmpl) -> Tmpl {
  match tmpl {
    Tmpl::Element { tag, attrs, children } => {
      let children: Vec<Tmpl> = children.iter().map(prepare_tmpl).collect();
      match baked_open(tag, attrs) {
        Some(open) if VOID.contains(&tag.as_str()) => Tmpl::Baked { open, tag: None, children: Vec::new() },
        Some(open) => Tmpl::Baked { open, tag: Some(tag.clone()), children },
        None => Tmpl::Element { tag: tag.clone(), attrs: attrs.clone(), children },
      }
    }
    Tmpl::Fragment(children) => Tmpl::Fragment(children.iter().map(prepare_tmpl).collect()),
    Tmpl::If { cond, then, r#else } => Tmpl::If { cond: cond.clone(), then: Box::new(prepare_tmpl(then)), r#else: r#else.as_ref().map(|other| Box::new(prepare_tmpl(other))) },
    Tmpl::For { over, params, body } => Tmpl::For { over: over.clone(), params: params.clone(), body: Box::new(prepare_tmpl(body)) },
    Tmpl::Let { name, expr, then } => Tmpl::Let { name: name.clone(), expr: expr.clone(), then: Box::new(prepare_tmpl(then)) },
    Tmpl::Component { module, props, children, id, keyed } => Tmpl::Component { module: module.clone(), props: props.clone(), children: children.iter().map(prepare_tmpl).collect(), id: *id, keyed: *keyed },
    Tmpl::Island { module, props, children, when, mode, id, define } => {
      Tmpl::Island { module: module.clone(), props: props.clone(), children: children.iter().map(prepare_tmpl).collect(), when: when.clone(), mode: mode.clone(), id: *id, define: *define }
    }
    other => other.clone(),
  }
}

/// The open tag of an element whose attributes are all literals, none of them
/// a marker the renderer answers for, that every set of markup rules prints
/// alike: a component is baked once and renders under whichever rules its
/// caller is under. `None` keeps the element on the evaluating path, which is
/// never a different answer.
fn baked_open(tag: &str, attrs: &[Entry]) -> Option<String> {
  if Markup::every().any(|markup| markup.may_hoist(tag)) || VOID.contains(&tag) {
    return None;
  }
  let mut open = String::with_capacity(tag.len() + 16);
  open.push('<');
  open.push_str(tag);
  let mut written: Vec<&str> = Vec::new();
  for entry in attrs {
    let Entry::Field(name, crate::ast::Expr::Lit(lit)) = entry else { return None };
    let name = html_attr_name(name);
    if name.starts_with('$') {
      return None;
    }
    if written.contains(&name) {
      return None;
    }
    written.push(name);
    if skipped_attr(name) {
      continue;
    }
    let value = match lit {
      crate::ast::Lit::Null => Value::Null,
      crate::ast::Lit::Bool(b) => Value::Bool(*b),
      crate::ast::Lit::Int(n) => Value::Int(*n),
      crate::ast::Lit::Float(f) => Value::F64(*f),
      crate::ast::Lit::Str(text) => Value::str(text.as_str()),
    };
    let mut agreed: Option<String> = None;
    for markup in Markup::every() {
      let mut piece = String::new();
      attribute(markup, tag, name, &value, &mut piece).ok()?;
      match &agreed {
        Some(first) if *first != piece => return None,
        Some(_) => {}
        None => agreed = Some(piece),
      }
    }
    open.push_str(&agreed.unwrap_or_default());
  }
  match VOID.contains(&tag) {
    true => open.push_str("/>"),
    false => open.push('>'),
  }
  Some(open)
}

/// Attribute keys the browser owns or that name no attribute: handlers, `key`, `ref`, `children` and a spread's `dangerouslySetInnerHTML`.
fn skipped_attr(name: &str) -> bool {
  name == "key" || name == "ref" || name == "children" || name == "dangerouslySetInnerHTML" || name.starts_with('$') || (name.len() > 2 && name.starts_with("on") && name.as_bytes()[2].is_ascii_uppercase())
}

/// The attribute marking an element whose inner markup is recorded as a hoisted chunk and its id.
pub const CHUNK_ATTR: &str = "$chunk";

/// An element's `dangerouslySetInnerHTML`, holding the `__html` expression.
/// Its string is written to the document as it stands, so whoever produced it
/// answers for it; the element's children never render.
pub const RAW_ATTR: &str = "$html";

/// A placement of a custom element whose shadow template sits under
/// `elements/`, holding the template's module. The server writes the
/// template as the element's declarative shadow root.
pub const SHADOW_ATTR: &str = "$shadow";

/// The prefix of an attribute binding a handler to its element: `$on:click`
/// holding the handler's index. Printed as `data-sf-on="click:0"` in server
/// mode and never otherwise.
pub const HANDLER_ATTR: &str = "$on:";
/// An element's React `key`, printed as `data-sf-key` in server mode so the
/// browser's patch keeps a moved element and never otherwise.
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
    // SVG presentation and reference attributes React spells in camelCase.
    // Only the ones SVG itself spells dashed belong here: `viewBox`, `refX`,
    // `markerWidth`, `preserveAspectRatio` and `gradientUnits` are camelCase
    // in SVG too and must reach the document as written.
    "strokeWidth" => "stroke-width",
    "strokeDasharray" => "stroke-dasharray",
    "strokeDashoffset" => "stroke-dashoffset",
    "strokeLinecap" => "stroke-linecap",
    "strokeLinejoin" => "stroke-linejoin",
    "strokeMiterlimit" => "stroke-miterlimit",
    "strokeOpacity" => "stroke-opacity",
    "fillOpacity" => "fill-opacity",
    "fillRule" => "fill-rule",
    "clipPath" => "clip-path",
    "clipRule" => "clip-rule",
    "stopColor" => "stop-color",
    "stopOpacity" => "stop-opacity",
    "markerStart" => "marker-start",
    "markerMid" => "marker-mid",
    "markerEnd" => "marker-end",
    "textAnchor" => "text-anchor",
    "dominantBaseline" => "dominant-baseline",
    "alignmentBaseline" => "alignment-baseline",
    "baselineShift" => "baseline-shift",
    "vectorEffect" => "vector-effect",
    "paintOrder" => "paint-order",
    "colorInterpolation" => "color-interpolation",
    "colorInterpolationFilters" => "color-interpolation-filters",
    "floodColor" => "flood-color",
    "floodOpacity" => "flood-opacity",
    "lightingColor" => "lighting-color",
    "letterSpacing" => "letter-spacing",
    "wordSpacing" => "word-spacing",
    "fontFamily" => "font-family",
    "fontSize" => "font-size",
    "fontSizeAdjust" => "font-size-adjust",
    "fontStretch" => "font-stretch",
    "fontStyle" => "font-style",
    "fontVariant" => "font-variant",
    "fontWeight" => "font-weight",
    "pointerEvents" => "pointer-events",
    "shapeRendering" => "shape-rendering",
    "textRendering" => "text-rendering",
    "imageRendering" => "image-rendering",
    "writingMode" => "writing-mode",
    "unicodeBidi" => "unicode-bidi",
    "xlinkHref" => "xlink:href",
    "xmlnsXlink" => "xmlns:xlink",
    other => other,
  }
}

/// A child expression the way React prints one: strings and numbers as text,
/// `null` and booleans as nothing, an array as its items in turn. Under
/// Vue's rules it is `toDisplayString`: a boolean is its word and an array or
/// an object its JSON.
fn interpolate(value: &Value, out: &mut Out, markup: Markup) -> Result<(), Fail> {
  if markup.is_vue() {
    out.text(&vue::display(value)?, markup);
    return Ok(());
  }
  match value {
    Value::Null | Value::Bool(_) => Ok(()),
    Value::Seq(items) => {
      for item in items {
        interpolate(item, out, markup)?;
      }
      Ok(())
    }
    other => {
      out.text(&crate::interp::scalar_str(other)?, markup);
      Ok(())
    }
  }
}

/// Whether `name` may be written into a tag. A name is printed as it stands,
/// and a `{...spread}` or a computed key takes one from a runtime value, so a
/// name carrying a space or a quote would close the attribute and start
/// another: `{"x onerror=alert(1)": 1}` spread onto an element was an event
/// handler. Anything holding a character that ends a name is dropped, the way
/// React drops one it cannot write.
fn writable_attr_name(name: &str) -> bool {
  !name.is_empty()
    && !name.chars().any(|c| {
      c.is_whitespace() || c.is_control() || matches!(c, '"' | '\'' | '>' | '<' | '/' | '=' | '&')
    })
}

/// ` name="text"`, the text escaped.
fn write_attr(name: &str, text: &str, out: &mut String) {
  out.push(' ');
  out.push_str(name);
  out.push_str("=\"");
  escape_attr(text, out);
  out.push('"');
}

/// An attribute of `tag` the way `markup`'s rules print one: `null` omits it.
/// Under React a custom element's attribute follows that major's own rules.
/// Under plain markup a custom element takes a scalar like any element and
/// refuses anything else, since nothing will set it as a property. On every
/// other element `false` omits it, `true` on a boolean attribute writes
/// `name=""`, a `style` with nothing in it is omitted and anything else is
/// stringified.
fn attribute(markup: Markup, tag: &str, name: &str, value: &Value, out: &mut String) -> Result<(), Fail> {
  if !writable_attr_name(name) {
    return Ok(());
  }
  if markup.is_vue() {
    return vue::attribute(name, value, out);
  }
  if matches!(value, Value::Null) {
    return Ok(());
  }
  if name != "style" && is_custom_element(tag) {
    match markup {
      Markup::React(major) => return major.custom_attribute(name, value, out),
      Markup::Plain if !is_scalar(value) => return Err(unset_property(tag, name, value)),
      Markup::Plain | Markup::Vue(_) => {}
    }
  }
  if matches!(value, Value::Bool(false)) {
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
  if markup.is_boolean(name) {
    if truthy(value) {
      write_attr(name, "", out);
    }
    return Ok(());
  }
  if matches!(value, Value::Bool(true)) && markup.drops_true(name) {
    return Ok(());
  }
  if matches!(value, Value::Str(text) if text.is_empty()) && markup.drops_empty(tag, name) {
    return Ok(());
  }
  write_attr(name, &crate::interp::scalar_str(value)?, out);
  Ok(())
}

fn is_scalar(value: &Value) -> bool {
  matches!(value, Value::Null | Value::Bool(_) | Value::Int(_) | Value::UInt(_) | Value::F32(_) | Value::F64(_) | Value::Str(_))
}

/// A non-scalar on a custom element in markup no framework hydrates. Nothing
/// in the browser will set it as a property, so the render names it.
fn unset_property(tag: &str, name: &str, value: &Value) -> Fail {
  Fail::internal(format!("`{name}` on `<{tag}>` holds a value of kind `{}` and nothing hydrates this markup to set it as a property; pass a string or give the element a template under `elements/`", crate::bind::kind_name(value)))
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
  fn an_svg_attribute_reaches_the_document_the_way_svg_spells_it() {
    for (jsx, printed) in [
      ("markerEnd", "marker-end"),
      ("strokeWidth", "stroke-width"),
      ("strokeDasharray", "stroke-dasharray"),
      ("fillRule", "fill-rule"),
      ("clipPath", "clip-path"),
      ("stopColor", "stop-color"),
      ("textAnchor", "text-anchor"),
      ("dominantBaseline", "dominant-baseline"),
      ("vectorEffect", "vector-effect"),
      ("paintOrder", "paint-order"),
      ("xlinkHref", "xlink:href"),
    ] {
      assert_eq!(html_attr_name(jsx), printed);
    }
    for camel in ["viewBox", "refX", "refY", "markerWidth", "markerHeight", "preserveAspectRatio", "gradientUnits", "patternUnits", "spreadMethod", "startOffset"] {
      assert_eq!(html_attr_name(camel), camel, "SVG spells this one in camelCase itself");
    }
  }

  #[test]
  fn an_svg_element_prints_its_dashed_attributes() {
    let component = Component {
      body: Vec::new(),
      render: Tmpl::Element {
        tag: "path".to_owned(),
        attrs: vec![
          Entry::Field(html_attr_name("markerEnd").to_owned(), Expr::lit_str("url(#a)")),
          Entry::Field(html_attr_name("strokeWidth").to_owned(), Expr::Lit(Lit::Int(2))),
          Entry::Field(html_attr_name("viewBox").to_owned(), Expr::lit_str("0 0 8 8")),
        ],
        children: Vec::new(),
      },
      state: Vec::new(),
      handlers: Vec::new(),
      hydrated_by: Some(crate::ast::HydratedBy::React),
      shadow: None,
    };
    let html = Interpreter::default().render(&component, &ValueMap::default(), &Components::new()).unwrap().html;
    assert_eq!(html, "<path marker-end=\"url(#a)\" stroke-width=\"2\" viewBox=\"0 0 8 8\"></path>");
  }

  #[test]
  fn markup_an_application_produced_reaches_the_document_unescaped() {
    let raw = |html: Expr, children: Vec<Tmpl>| Component {
      body: Vec::new(),
      render: Tmpl::Element { tag: "div".to_owned(), attrs: vec![Entry::Field("class".to_owned(), Expr::lit_str("md")), Entry::Field(RAW_ATTR.to_owned(), html)], children },
      state: Vec::new(),
      handlers: Vec::new(),
      hydrated_by: Some(crate::ast::HydratedBy::React),
      shadow: None,
    };
    let render = |component: &Component, props: &ValueMap| Interpreter::default().render(component, props, &Components::new()).unwrap().html;

    let component = raw(p("body"), Vec::new());
    let html = render(&component, &props(&[("body", Value::str("<p>a <b>markdown</b> blip</p>".to_owned()))]));
    assert_eq!(html, "<div class=\"md\"><p>a <b>markdown</b> blip</p></div>", "the string is written as markup, not as text");

    assert_eq!(render(&component, &props(&[("body", Value::Null)])), "<div class=\"md\"></div>", "React renders nothing for a missing __html");

    let with_children = raw(Expr::lit_str("<i>from the loader</i>"), vec![Tmpl::Text("never rendered".to_owned())]);
    assert_eq!(render(&with_children, &ValueMap::default()), "<div class=\"md\"><i>from the loader</i></div>", "the markup replaces the children the way React refuses to have both");
  }

  #[test]
  fn elements_text_and_interpolation_print_like_react() {
    let component = Component {
      body: vec![Stmt::Let { name: "n".to_owned(), expr: Expr::Length(Box::new(p("items"))) }],
      render: Tmpl::Element {
        tag: "p".to_owned(),
        attrs: vec![Entry::Field("class".to_owned(), Expr::lit_str("count")), Entry::Field("hidden".to_owned(), Expr::Lit(Lit::Bool(false))), Entry::Field("title".to_owned(), Expr::Lit(Lit::Null))],
        children: vec![Tmpl::Expr(Expr::var("n")), Tmpl::Text(" result".to_owned()), Tmpl::Expr(Expr::Ternary(Box::new(Expr::Compare(crate::ast::CompareOp::Eq, Box::new(Expr::var("n")), Box::new(Expr::Lit(Lit::Float(1.0))))), Box::new(Expr::lit_str("")), Box::new(Expr::lit_str("s")))), Tmpl::Text(" <3".to_owned())],
      }, state: Vec::new(), handlers: Vec::new(), hydrated_by: Some(crate::ast::HydratedBy::React), shadow: None
    };
    let html = (Interpreter::default().render(&component, &props(&[("items", Value::seq(vec![Value::Null, Value::Null]))]), &Components::new())).unwrap().html;
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
      }, state: Vec::new(), handlers: Vec::new(), hydrated_by: Some(crate::ast::HydratedBy::React), shadow: None
    };
    let lines = Value::seq(vec![Value::Map(props(&[("quantity", Value::Int(1))])), Value::Map(props(&[("quantity", Value::Int(3))]))]);
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
        }, state: Vec::new(), handlers: Vec::new(), hydrated_by: Some(crate::ast::HydratedBy::React), shadow: None
      }),
    );
    let page = Component {
      body: Vec::new(),
      render: Tmpl::Fragment(vec![Tmpl::Component { module: "src/ui/Stars.tsx#Stars".to_owned(), props: vec![Entry::Field("rating".to_owned(), p("product").field("rating"))], children: Vec::new(), id: 0, keyed: false }, Tmpl::Expr(p("product").field("name"))]), state: Vec::new(), handlers: Vec::new(), hydrated_by: Some(crate::ast::HydratedBy::React), shadow: None
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
          children: vec![Tmpl::Element { tag: "h1".to_owned(), attrs: Vec::new(), children: vec![Tmpl::Expr(p("title"))] }, Tmpl::Component { module: "src/ui/Card.tsx#Card".to_owned(), props: Vec::new(), children: vec![Tmpl::Slot("content".to_owned())], id: 0, keyed: false }],
        }, state: Vec::new(), handlers: Vec::new(), hydrated_by: Some(crate::ast::HydratedBy::React), shadow: None
      }),
    );
    library.insert(
      "src/ui/Card.tsx#Card".to_owned(),
      Arc::new(Component { body: Vec::new(), render: Tmpl::Element { tag: "div".to_owned(), attrs: vec![Entry::Field("class".to_owned(), Expr::lit_str("card"))], children: vec![Tmpl::Slot("content".to_owned()), Tmpl::Slot("content".to_owned())] }, state: Vec::new(), handlers: Vec::new(), hydrated_by: Some(crate::ast::HydratedBy::React), shadow: None }),
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
        id: 0,
        keyed: false,
      }, state: Vec::new(), handlers: Vec::new(), hydrated_by: Some(crate::ast::HydratedBy::React), shadow: None
    };
    let mut attrs = ValueMap::default();
    attrs.insert("className".to_owned(), Value::str("ignored"));
    attrs.insert("dataId".to_owned(), Value::Int(7));
    attrs.insert("onClick".to_owned(), Value::str("handler"));
    attrs.insert("hidden".to_owned(), Value::Bool(true));
    let items = Value::seq(vec![Value::Map(props(&[("name", Value::str("A")), ("attrs", Value::Map(attrs))])), Value::Map(props(&[("name", Value::str("B")), ("attrs", Value::Null)]))]);
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
      ]), state: Vec::new(), handlers: Vec::new(), hydrated_by: Some(crate::ast::HydratedBy::React), shadow: None
    };
    let html = (Interpreter::default().render(&component, &ValueMap::default(), &Components::new())).unwrap().html;
    assert_eq!(html, "<input value=\"a &quot;b&quot; &amp; c\" disabled=\"\" aria-hidden=\"true\"/><br/>");
  }

  #[test]
  fn a_store_read_takes_the_seed_and_falls_back_without_one() {
    let read = Expr::Coalesce(Box::new(Expr::Store("cart/count".to_owned())), Box::new(Expr::Lit(Lit::Float(0.0))));
    let inner = Component { body: vec![Stmt::Let { name: "n".to_owned(), expr: read.clone() }], render: Tmpl::Element { tag: "b".to_owned(), attrs: Vec::new(), children: vec![Tmpl::Expr(Expr::var("n"))] }, state: Vec::new(), handlers: Vec::new(), hydrated_by: Some(crate::ast::HydratedBy::React), shadow: None };
    let mut library = Components::new();
    library.insert("src/ui/Badge.tsx#Badge".to_owned(), Arc::new(inner));
    let outer = Component {
      body: vec![Stmt::Let { name: "n".to_owned(), expr: read }],
      render: Tmpl::Fragment(vec![
        Tmpl::Expr(Expr::var("n")),
        Tmpl::Component { module: "src/ui/Badge.tsx#Badge".to_owned(), props: Vec::new(), children: Vec::new(), id: 0, keyed: false },
      ]), state: Vec::new(), handlers: Vec::new(), hydrated_by: Some(crate::ast::HydratedBy::React), shadow: None
    };
    let render = |props: ValueMap| Interpreter::default().render(&outer, &props, &library).unwrap().html;
    assert_eq!(render(ValueMap::default()), "0<b>0</b>", "no seed leaves both reads on the fallback");
    let mut store = ValueMap::default();
    store.insert("cart/count".to_owned(), Value::Int(3));
    let mut props = ValueMap::default();
    props.insert("$store".to_owned(), Value::Map(store));
    assert_eq!(render(props), "3<b>3</b>", "a nested component reads the seed without a prop");
  }

  #[test]
  fn a_slot_placement_shows_its_fallback_until_the_plan_fills_it() {
    let filled = Expr::Builtin { name: Builtin::Includes, args: vec![Expr::Coalesce(Box::new(Expr::Var("$props".to_owned()).field("$slots")), Box::new(Expr::Array(Vec::new()))), Expr::lit_str("modal")] };
    let component = Component {
      body: Vec::new(),
      render: Tmpl::Element { tag: "sf-s".to_owned(), attrs: Vec::new(), children: vec![Tmpl::If { cond: filled, then: Box::new(Tmpl::Slot("modal".to_owned())), r#else: Some(Box::new(Tmpl::Text("closed".to_owned()))) }] }, state: Vec::new(), handlers: Vec::new(), hydrated_by: Some(crate::ast::HydratedBy::React), shadow: None
    };
    let render = |props: ValueMap| Interpreter::default().render(&component, &props, &Components::new()).unwrap().html;
    assert_eq!(render(ValueMap::default()), "<sf-s>closed</sf-s>", "no $slots at all shows the fallback");
    let mut props = ValueMap::default();
    props.insert("$slots".to_owned(), Value::seq(vec![Value::str("content")]));
    assert_eq!(render(props.clone()), "<sf-s>closed</sf-s>");
    props.insert("$slots".to_owned(), Value::seq(vec![Value::str("content"), Value::str("modal")]));
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
      let component = Component { body: Vec::new(), render: Tmpl::Expr(expr.clone()), state: Vec::new(), handlers: Vec::new(), hydrated_by: Some(crate::ast::HydratedBy::React), shadow: None };
      let html = (Interpreter::default().render(&component, &ValueMap::default(), &Components::new())).unwrap().html;
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
      ]), state: Vec::new(), handlers: Vec::new(), hydrated_by: Some(crate::ast::HydratedBy::React), shadow: None
    };
    let mut props = ValueMap::default();
    props.insert("total".to_owned(), Value::F64(2.5));
    props.insert("prices".to_owned(), Value::seq(vec![Value::F64(1.0), Value::F64(2.0)]));
    props.insert("taxes".to_owned(), Value::seq(vec![Value::F64(1.0), Value::F64(1.5)]));
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
      Arc::new(Component { body: Vec::new(), render: Tmpl::Expr(hoist(0, fixed(Expr::var("$props").field("cents")))), state: Vec::new(), handlers: Vec::new(), hydrated_by: Some(crate::ast::HydratedBy::React), shadow: None }),
    );
    let price = |cents: Expr| Tmpl::Component { module: "src/ui/Price.tsx#Price".to_owned(), props: vec![Entry::Field("cents".to_owned(), cents)], children: Vec::new(), id: 0, keyed: false };
    let page = Component {
      body: Vec::new(),
      render: Tmpl::Fragment(vec![
        price(Expr::Lit(Lit::Float(1.0))),
        Tmpl::For { over: Expr::var("$props").field("items"), params: vec!["it".to_owned()], body: Box::new(price(Expr::var("it"))) },
        Tmpl::Expr(hoist(0, fixed(Expr::Lit(Lit::Float(9.0))))),
      ]), state: Vec::new(), handlers: Vec::new(), hydrated_by: Some(crate::ast::HydratedBy::React), shadow: None
    };
    let mut props = ValueMap::default();
    props.insert("items".to_owned(), Value::seq(vec![Value::F64(2.0), Value::F64(3.0)]));
    let rendered = Interpreter::default().render_module("routes/index/page.tsx#default", &page, &props, &library).unwrap();
    assert_eq!(rendered.html, "1.0<!-- -->2.0<!-- -->3.0<!-- -->9.0");
    let keys: Vec<&String> = rendered.hoisted.keys().collect();
    assert_eq!(keys, ["src/ui/Price.tsx#Price|0", "src/ui/Price.tsx#Price|0@0", "src/ui/Price.tsx#Price|0@1", "routes/index/page.tsx#default|0"]);
    assert_eq!(rendered.hoisted["src/ui/Price.tsx#Price|0@1"], Value::str("3.0"));

    let twice = Component { body: Vec::new(), render: Tmpl::Fragment(vec![price(Expr::Lit(Lit::Float(1.0))), price(Expr::Lit(Lit::Float(1.0))), price(Expr::Lit(Lit::Float(2.0)))]), state: Vec::new(), handlers: Vec::new(), hydrated_by: Some(crate::ast::HydratedBy::React), shadow: None };
    let rendered = Interpreter::default().render_module("routes/index/page.tsx#default", &twice, &ValueMap::default(), &library).unwrap();
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
      }, state: Vec::new(), handlers: Vec::new(), hydrated_by: Some(crate::ast::HydratedBy::React), shadow: None
    };
    let mut props = ValueMap::default();
    props.insert("items".to_owned(), Value::seq(vec![Value::F64(1.0), Value::F64(2.0)]));
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
      Arc::new(Component { body: Vec::new(), render: Tmpl::Expr(hoist(0, fixed(Expr::var("$props").field("n")))), state: Vec::new(), handlers: Vec::new(), hydrated_by: Some(crate::ast::HydratedBy::React), shadow: None }),
    );
    let page = Component {
      body: Vec::new(),
      render: Tmpl::Fragment(vec![
        Tmpl::Expr(hoist(0, fixed(Expr::Lit(Lit::Float(1.0))))),
        Tmpl::Island { module: "src/ui/Help.tsx#Help".to_owned(), props: vec![Entry::Field("n".to_owned(), Expr::Lit(Lit::Float(2.0)))], children: Vec::new(), when: None, mode: None, id: 9, define: false },
      ]), state: Vec::new(), handlers: Vec::new(), hydrated_by: Some(crate::ast::HydratedBy::React), shadow: None
    };
    let rendered = Interpreter::default().render_module("routes/index/page.tsx#default", &page, &ValueMap::default(), &library).unwrap();
    let keys: Vec<&String> = rendered.hoisted.keys().collect();
    assert_eq!(keys, ["routes/index/page.tsx#default|0"], "the island's values are not the page's");
    assert_eq!(rendered.islands[0].body.hoisted.keys().collect::<Vec<_>>(), ["src/ui/Help.tsx#Help|0"]);
    let mount = rendered.islands[0].mount_props();
    assert_eq!(mount.get("n"), Some(&Value::F64(2.0)));
    let Some(Value::Map(table)) = mount.get(HOISTED_PROP) else { panic!("{mount:?}") };
    assert_eq!(table["src/ui/Help.tsx#Help|0"], Value::str("2.0"));
  }

  #[test]
  fn a_shadow_root_inside_an_island_records_nothing_in_the_islands_table() {
    let mut library = Components::new();
    library.insert(
      "elements/x-box.tsx#default".to_owned(),
      Arc::new(Component { body: Vec::new(), render: Tmpl::Expr(hoist(0, fixed(Expr::var("$props").field("n")))), state: Vec::new(), handlers: Vec::new(), hydrated_by: None, shadow: None }),
    );
    library.insert(
      "src/ui/Help.tsx#Help".to_owned(),
      Arc::new(Component {
        body: Vec::new(),
        render: Tmpl::Element {
          tag: "x-box".to_owned(),
          attrs: vec![Entry::Field(SHADOW_ATTR.to_owned(), Expr::lit_str("elements/x-box.tsx#default")), Entry::Field("n".to_owned(), Expr::var("$props").field("n"))],
          children: Vec::new(),
        },
        state: Vec::new(),
        handlers: Vec::new(),
        hydrated_by: Some(crate::ast::HydratedBy::React),
        shadow: None,
      }),
    );
    let page = Component {
      body: Vec::new(),
      render: Tmpl::Island { module: "src/ui/Help.tsx#Help".to_owned(), props: vec![Entry::Field("n".to_owned(), Expr::Lit(Lit::Float(2.0)))], children: Vec::new(), when: None, mode: None, id: 9, define: false },
      state: Vec::new(),
      handlers: Vec::new(),
      hydrated_by: Some(crate::ast::HydratedBy::React),
      shadow: None,
    };
    let rendered = Interpreter::default().render_module("routes/index/page.tsx#default", &page, &ValueMap::default(), &library).unwrap();
    let island = &rendered.islands[0].body;
    assert!(island.html.contains("<template shadowrootmode=\"open\">2.0</template>"), "{}", island.html);
    assert!(island.hoisted.is_empty(), "the template's hoist 0 is not Help's: {:?}", island.hoisted);
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
      hydrated_by: Some(crate::ast::HydratedBy::React),
      shadow: None,
    }
  }

  #[test]
  fn handler_markers_and_keys_print_only_in_server_mode() {
    let mut library = Components::new();
    library.insert("src/ui/Help.tsx#Help".to_owned(), Arc::new(help()));
    let island = |mode: Option<&str>| Component { body: Vec::new(), render: Tmpl::Island { module: "src/ui/Help.tsx#Help".to_owned(), props: vec![Entry::Field("id".to_owned(), Expr::Lit(Lit::Int(7)))], children: Vec::new(), when: None, mode: mode.map(str::to_owned), id: 9, define: false }, state: Vec::new(), handlers: Vec::new(), hydrated_by: Some(crate::ast::HydratedBy::React), shadow: None };
    let browser = Interpreter::default().render_module("page", &island(None), &ValueMap::default(), &library).unwrap();
    assert_eq!(browser.islands[0].body.html, "<section>order 7<button>Show</button></section>");
    assert!(browser.islands[0].mode.is_none() && !browser.islands[0].mount_props().contains_key(STATE_PROP));
    let server = Interpreter::default().render_module("page", &island(Some("server")), &ValueMap::default(), &library).unwrap();
    assert_eq!(server.islands[0].body.html, "<section>order 7<button data-sf-key=\"toggle\" data-sf-on=\"click:0\">Show</button></section>");
    assert_eq!(server.islands[0].mode.as_deref(), Some("server"));
    let props = server.islands[0].mount_props();
    assert_eq!(props.get(STATE_PROP), Some(&Value::Map(ValueMap::from_iter([("open".to_owned(), Value::Bool(false))]))), "{props:?}");
    let nodes = crate::bind::rendered_nodes(&server);
    assert_eq!(nodes[0], snapfire_fsr_core::Node::raw("<sf-s data-sf-island data-sf-region=\"page|i9\" data-sf-mode=\"server\">"));
  }

  #[test]
  fn a_step_runs_the_handler_over_the_state_and_renders_from_the_result() {
    let library = Components::new();
    let component = help();
    let mut props = ValueMap::default();
    props.insert("id".to_owned(), Value::Int(7));
    let state = ValueMap::from_iter([("open".to_owned(), Value::Bool(false))]);
    let stepped = Interpreter::default().island_step("src/ui/Help.tsx#Help", &component, &props, &state, Some(HandlerRef::own(0)), &Value::Null, &library).unwrap();
    assert_eq!(stepped.state, ValueMap::from_iter([("open".to_owned(), Value::Bool(true))]));
    assert_eq!(stepped.rendered.html, "<section>order 7<button data-sf-key=\"toggle\" data-sf-on=\"click:0\">Hide</button><ul>mail</ul></section>");
    let again = Interpreter::default().island_step("src/ui/Help.tsx#Help", &component, &props, &stepped.state, Some(HandlerRef::own(0)), &Value::Null, &library).unwrap();
    assert_eq!(again.state["open"], Value::Bool(false));
    let as_is = Interpreter::default().island_step("src/ui/Help.tsx#Help", &component, &props, &stepped.state, None, &Value::Null, &library).unwrap();
    assert_eq!(as_is.state, stepped.state, "no handler renders from the state given");
    assert!(as_is.rendered.html.contains("<ul>mail</ul>"));
    let missing = Interpreter::default().island_step("src/ui/Help.tsx#Help", &component, &props, &state, Some(HandlerRef::own(3)), &Value::Null, &library).unwrap_err();
    assert!(missing.message.contains("no handler 3"), "{}", missing.message);
    assert!(stepped.acts.is_empty() && as_is.acts.is_empty());
  }

  #[test]
  fn a_step_collects_the_actions_a_handler_calls_with_their_inputs_evaluated() {
    let library = Components::new();
    let mut component = help();
    component.handlers.push(Handler {
      event: "click".to_owned(),
      body: vec![
        Stmt::Act { action: "desk.save".to_owned(), input: Expr::Object(vec![Entry::Field("id".to_owned(), Expr::var("$props").field("id")), Entry::Field("open".to_owned(), Expr::var("open"))]) },
        Stmt::Return(Expr::Object(vec![Entry::Field("open".to_owned(), Expr::Lit(Lit::Bool(true)))])),
      ],
    });
    let mut props = ValueMap::default();
    props.insert("id".to_owned(), Value::Int(7));
    let state = ValueMap::from_iter([("open".to_owned(), Value::Bool(false))]);
    let stepped = Interpreter::default().island_step("src/ui/Help.tsx#Help", &component, &props, &state, Some(HandlerRef::own(1)), &Value::Null, &library).unwrap();
    let input = ValueMap::from_iter([("id".to_owned(), Value::Int(7)), ("open".to_owned(), Value::Bool(false))]);
    assert_eq!(stepped.acts, vec![("desk.save".to_owned(), Value::Map(input))], "the input reads the props and the state as they were when the handler ran");
    assert_eq!(stepped.state["open"], Value::Bool(true), "and the patch still applies");
    assert!(stepped.rendered.html.contains("<ul>mail</ul>"));
  }
}

#[cfg(test)]
mod island_tests {
  use super::*;
  use crate::ast::{Entry, Expr, Tmpl};
  use crate::bind::rendered_nodes;
  use snapfire_fsr_core::Node;

  #[test]
  fn a_placement_in_a_loop_keys_its_region_under_its_iteration() {
    let mut library = Components::new();
    library.insert(
      "src/ui/Body.tsx#Body".to_owned(),
      Arc::new(Component { body: Vec::new(), render: Tmpl::Expr(Expr::var("$props").field("text")), state: Vec::new(), handlers: Vec::new(), hydrated_by: Some(crate::ast::HydratedBy::React), shadow: None }),
    );
    let page = Component {
      body: Vec::new(),
      render: Tmpl::For {
        over: Expr::var("$props").field("blips"),
        params: vec!["blip".to_owned()],
        body: Box::new(Tmpl::Island { module: "src/ui/Body.tsx#Body".to_owned(), props: vec![Entry::Field("text".to_owned(), Expr::var("blip"))], children: Vec::new(), when: None, mode: None, id: 1, define: false }),
      },
      state: Vec::new(),
      handlers: Vec::new(),
      hydrated_by: Some(crate::ast::HydratedBy::React),
      shadow: None,
    };
    let mut props = ValueMap::default();
    props.insert("blips".to_owned(), Value::seq(vec![Value::str("one"), Value::str("two"), Value::str("three")]));
    let rendered = Interpreter::default().render_module("routes/w/page.tsx#default", &page, &props, &library).unwrap();
    let keys: Vec<&str> = rendered.islands.iter().map(|i| i.key.as_str()).collect();
    assert_eq!(keys, ["routes/w/page.tsx#default|i1@0", "routes/w/page.tsx#default|i1@1", "routes/w/page.tsx#default|i1@2"], "one placement in a loop is one region per iteration");
    assert_eq!(rendered.islands[1].mount_props().get(KEY_PROP), Some(&Value::str("routes/w/page.tsx#default|i1@1")), "the key rides in the props the browser mounts with");
    let nodes = rendered_nodes(&rendered);
    let marked: Vec<&str> = nodes.iter().filter_map(|n| match n {
      Node::Raw(html) if html.0.starts_with("<sf-s data-sf-island") => Some(html.0.as_str()),
      _ => None,
    }).collect();
    assert_eq!(marked, ["<sf-s data-sf-island data-sf-region=\"routes/w/page.tsx#default|i1@0\">", "<sf-s data-sf-island data-sf-region=\"routes/w/page.tsx#default|i1@1\">", "<sf-s data-sf-island data-sf-region=\"routes/w/page.tsx#default|i1@2\">"]);
  }

  fn island_at(module: &str, id: u32) -> Tmpl {
    Tmpl::Island { module: module.to_owned(), props: Vec::new(), children: Vec::new(), when: None, mode: None, id, define: false }
  }

  fn keys_of(rendered: &Rendered) -> Vec<&str> {
    rendered.islands.iter().map(|i| i.key.as_str()).collect()
  }

  #[test]
  fn two_placements_of_one_component_key_what_it_holds_apart() {
    let mut library = Components::new();
    library.insert("src/ui/Body.tsx#Body".to_owned(), Arc::new(Component::new(Vec::new(), Tmpl::Text("body".to_owned()))));
    library.insert("src/ui/Card.tsx#Card".to_owned(), Arc::new(Component::new(Vec::new(), island_at("src/ui/Body.tsx#Body", 0))));
    let card = |id: u32| Tmpl::Component { module: "src/ui/Card.tsx#Card".to_owned(), props: Vec::new(), children: Vec::new(), id, keyed: true };
    let page = Component::new(Vec::new(), Tmpl::Fragment(vec![card(1), card(2)]));
    let rendered = Interpreter::default().render_module("routes/w/page.tsx#default", &page, &ValueMap::default(), &library).unwrap();
    assert_eq!(keys_of(&rendered), ["src/ui/Card.tsx#Card|i0@c1", "src/ui/Card.tsx#Card|i0@c2"], "each keyed placement names itself on the path");
  }

  #[test]
  fn a_component_that_renders_itself_from_two_loops_keys_every_level_apart() {
    const THREAD: &str = "src/ui/Thread.tsx#Thread";
    let under = |field: &str, id: u32| Tmpl::For {
      over: Expr::var("$props").field(field),
      params: vec!["each".to_owned()],
      body: Box::new(Tmpl::Component { module: THREAD.to_owned(), props: vec![Entry::Spread(Expr::var("each"))], children: Vec::new(), id, keyed: true }),
    };
    let mut library = Components::new();
    library.insert("src/ui/Body.tsx#Body".to_owned(), Arc::new(Component::new(Vec::new(), Tmpl::Text("body".to_owned()))));
    library.insert(THREAD.to_owned(), Arc::new(Component::new(Vec::new(), Tmpl::Fragment(vec![island_at("src/ui/Body.tsx#Body", 0), under("replies", 1), under("asides", 2)]))));
    fn node(replies: Vec<Value>, asides: Vec<Value>) -> Value {
      let mut map = ValueMap::default();
      map.insert("replies".to_owned(), Value::seq(replies));
      map.insert("asides".to_owned(), Value::seq(asides));
      Value::Map(map)
    }
    let Value::Map(props) = node(vec![node(vec![node(vec![], vec![])], vec![])], vec![node(vec![node(vec![], vec![])], vec![])]) else { unreachable!() };
    let rendered = Interpreter::default().render_module(THREAD, &library[THREAD], &props, &library).unwrap();
    assert_eq!(
      keys_of(&rendered),
      [
        "src/ui/Thread.tsx#Thread|i0",
        "src/ui/Thread.tsx#Thread|i0@0.c1",
        "src/ui/Thread.tsx#Thread|i0@0.c1.0.c1",
        "src/ui/Thread.tsx#Thread|i0@0.c2",
        "src/ui/Thread.tsx#Thread|i0@0.c2.0.c1",
      ],
      "a reply and an aside at the same index are different placements"
    );
  }

  #[test]
  fn a_callers_children_key_under_the_caller_rather_than_the_component_placing_them() {
    let mut library = Components::new();
    library.insert("src/ui/Body.tsx#Body".to_owned(), Arc::new(Component::new(Vec::new(), Tmpl::Text("body".to_owned()))));
    library.insert("src/ui/Wrap.tsx#Wrap".to_owned(), Arc::new(Component::new(Vec::new(), Tmpl::Fragment(vec![island_at("src/ui/Body.tsx#Body", 0), Tmpl::Slot("content".to_owned())]))));
    let page = Component::new(
      Vec::new(),
      Tmpl::For {
        over: Expr::var("$props").field("rows"),
        params: vec!["row".to_owned()],
        body: Box::new(Tmpl::Component { module: "src/ui/Wrap.tsx#Wrap".to_owned(), props: Vec::new(), children: vec![island_at("src/ui/Body.tsx#Body", 2)], id: 1, keyed: true }),
      },
    );
    let mut props = ValueMap::default();
    props.insert("rows".to_owned(), Value::seq(vec![Value::str("one"), Value::str("two")]));
    let rendered = Interpreter::default().render_module("routes/w/page.tsx#default", &page, &props, &library).unwrap();
    assert_eq!(
      keys_of(&rendered),
      ["src/ui/Wrap.tsx#Wrap|i0@0.c1", "routes/w/page.tsx#default|i2@0", "src/ui/Wrap.tsx#Wrap|i0@1.c1", "routes/w/page.tsx#default|i2@1"],
      "the children were built by the page, which is where the browser computes their keys"
    );
  }

  #[test]
  fn an_islands_children_render_in_a_region_where_the_component_places_them() {
    let mut library = Components::new();
    let card = Tmpl::Element { tag: "div".to_owned(), attrs: Vec::new(), children: vec![Tmpl::Text("card".to_owned()), Tmpl::Slot("content".to_owned())] };
    library.insert("src/ui/Card.tsx#Card".to_owned(), Arc::new(Component::new(Vec::new(), card)));
    let child = Tmpl::Element { tag: "p".to_owned(), attrs: Vec::new(), children: vec![Tmpl::Expr(Expr::var("$props").field("name"))] };
    let page = Component::new(Vec::new(), Tmpl::Island { module: "src/ui/Card.tsx#Card".to_owned(), props: Vec::new(), children: vec![child], when: None, mode: None, id: 0, define: false });
    let mut props = ValueMap::default();
    props.insert("name".to_owned(), Value::str("ada"));
    let rendered = Interpreter::default().render(&page, &props, &library).unwrap();
    assert_eq!(rendered.islands[0].body.html, "<div>card<sf-s data-sf-children><p>ada</p></sf-s></div>", "the children read the page's props");
  }

  #[test]
  fn a_component_the_server_cannot_render_is_placed_with_its_children() {
    let page = Component::new(
      Vec::new(),
      Tmpl::Fragment(vec![
        Tmpl::Island { module: "src/ui/Tonight.vue#default".to_owned(), props: Vec::new(), children: vec![Tmpl::Element { tag: "p".to_owned(), attrs: Vec::new(), children: vec![Tmpl::Text("server".to_owned())] }], when: None, mode: None, id: 0, define: false },
        island_at("src/ui/Empty.vue#default", 1),
      ]),
    );
    let rendered = Interpreter::default().render(&page, &ValueMap::default(), &Components::new()).unwrap();
    assert_eq!(rendered.islands[0].body.html, "<sf-s data-sf-children><p>server</p></sf-s>");
    assert_eq!(rendered.islands[1].body.html, "");
  }

  #[test]
  fn an_island_among_an_islands_children_keys_under_the_page_that_wrote_it() {
    let mut library = Components::new();
    library.insert("src/ui/Body.tsx#Body".to_owned(), Arc::new(Component::new(Vec::new(), Tmpl::Text("body".to_owned()))));
    library.insert("src/ui/Card.tsx#Card".to_owned(), Arc::new(Component::new(Vec::new(), Tmpl::Slot("content".to_owned()))));
    let page = Component::new(Vec::new(), Tmpl::Island { module: "src/ui/Card.tsx#Card".to_owned(), props: Vec::new(), children: vec![island_at("src/ui/Body.tsx#Body", 3)], when: None, mode: None, id: 0, define: false });
    let rendered = Interpreter::default().render_module("routes/w/page.tsx#default", &page, &ValueMap::default(), &library).unwrap();
    assert_eq!(keys_of(&rendered), ["routes/w/page.tsx#default|i0"]);
    assert_eq!(keys_of(&rendered.islands[0].body), ["routes/w/page.tsx#default|i3"]);
  }

  #[test]
  fn an_island_renders_apart_and_binds_as_a_nested_client_node_in_a_region() {
    let mut library = Components::new();
    library.insert(
      "src/ui/Help.tsx#Help".to_owned(),
      Arc::new(Component { body: Vec::new(), render: Tmpl::Element { tag: "p".to_owned(), attrs: Vec::new(), children: vec![Tmpl::Text("help ".to_owned()), Tmpl::Expr(Expr::var("$props").field("id"))] }, state: Vec::new(), handlers: Vec::new(), hydrated_by: Some(crate::ast::HydratedBy::React), shadow: None }),
    );
    let page = Component {
      body: Vec::new(),
      render: Tmpl::Element {
        tag: "main".to_owned(),
        attrs: Vec::new(),
        children: vec![
          Tmpl::Text("before".to_owned()),
          Tmpl::Island { module: "src/ui/Help.tsx#Help".to_owned(), props: vec![Entry::Field("id".to_owned(), Expr::var("$props").field("id"))], children: Vec::new(), when: Some("visible".to_owned()), mode: None, id: 9, define: false },
          Tmpl::Text("after".to_owned()),
        ],
      }, state: Vec::new(), handlers: Vec::new(), hydrated_by: Some(crate::ast::HydratedBy::React), shadow: None
    };
    let mut props = ValueMap::default();
    props.insert("id".to_owned(), Value::int(7i64));
    let rendered = Interpreter::default().render(&page, &props, &library).unwrap();
    assert_eq!(rendered.html, format!("<main>before{ISLAND_MARK}0\u{0}after</main>"));
    assert_eq!(rendered.islands.len(), 1);
    assert_eq!(rendered.islands[0].body.html, "<p>help <!-- -->7</p>");
    assert_eq!(rendered.islands[0].when.as_deref(), Some("visible"));

    let nodes = rendered_nodes(&rendered);
    assert_eq!(nodes.len(), 5, "{nodes:?}");
    assert_eq!(nodes[0], Node::raw("<main>before"));
    assert_eq!(nodes[1], Node::raw("<sf-s data-sf-island data-sf-region=\"|i9\" data-sf-when=\"visible\">"));
    let Node::Client { module, props: island_props, ssr: Some(body), .. } = &nodes[2] else { panic!("{:?}", nodes[2]) };
    assert_eq!(module.to_string(), "src/ui/Help.tsx#Help");
    assert_eq!(island_props.get("id"), Some(&Value::int(7i64)));
    assert_eq!(**body, Node::raw("<p>help <!-- -->7</p>"));
    assert_eq!(nodes[3], Node::raw("</sf-s>"));
    assert_eq!(nodes[4], Node::raw("after</main>"));
  }
}

#[cfg(test)]
mod markup_tests {
  use super::*;
  use crate::ast::{Entry, Expr, Lit, Tmpl};

  fn props(entries: &[(&str, Value)]) -> ValueMap {
    entries.iter().map(|(k, v)| ((*k).to_owned(), v.clone())).collect()
  }

  fn p(name: &str) -> Expr {
    Expr::var("$props").field(name)
  }

  const BY_REACT: Option<crate::ast::HydratedBy> = Some(crate::ast::HydratedBy::React);

  fn element(tag: &str, attrs: Vec<(&str, Expr)>, children: Vec<Tmpl>) -> Tmpl {
    Tmpl::Element { tag: tag.to_owned(), attrs: attrs.into_iter().map(|(name, expr)| Entry::Field(name.to_owned(), expr)).collect(), children }
  }

  fn render_under(react: Option<ReactMajor>, hydrated_by: Option<crate::ast::HydratedBy>, render: Tmpl, props: &ValueMap, library: &Components) -> Result<String, Fail> {
    let component = Component { body: Vec::new(), render, state: Vec::new(), handlers: Vec::new(), hydrated_by, shadow: None };
    Interpreter::default().with_frameworks(Frameworks { react, vue: None }).render(&component, props, library).map(|rendered| rendered.html)
  }

  #[test]
  fn a_custom_element_attribute_is_written_the_way_the_react_hydrating_it_writes_it() {
    let tree = element("x-el", vec![("rows", p("rows")), ("obj", p("obj")), ("on", p("on")), ("off", p("off")), ("className", Expr::lit_str("c"))], Vec::new());
    let values = props(&[("rows", Value::seq(vec![Value::int(1i64), Value::Null, Value::int(2i64)])), ("obj", Value::Map(props(&[("a", Value::int(1i64))]))), ("on", Value::Bool(true)), ("off", Value::Bool(false))]);
    let html = |react| render_under(react, BY_REACT, tree.clone(), &values, &Components::new()).unwrap();
    assert_eq!(html(Some(ReactMajor::V18)), "<x-el rows=\"1,,2\" obj=\"[object Object]\" on=\"true\" off=\"false\" class=\"c\"></x-el>", "React 18 coerces every prop the way '' + value does");
    assert_eq!(html(Some(ReactMajor::V19)), "<x-el on=\"\" class=\"c\"></x-el>", "React 19 leaves an object to the client and writes true as present");
  }

  #[test]
  fn a_non_scalar_on_a_custom_element_nothing_hydrates_is_refused_by_name() {
    let tree = element("x-grid", vec![("rows", p("rows")), ("count", p("count"))], Vec::new());
    let values = props(&[("rows", Value::seq(vec![Value::int(1i64)])), ("count", Value::int(3i64))]);
    let fail = render_under(Some(ReactMajor::V19), None, tree.clone(), &values, &Components::new()).unwrap_err();
    assert!(fail.message.contains("`rows` on `<x-grid>`") && fail.message.contains("`array`"), "{}", fail.message);
    let scalar = props(&[("rows", Value::str("1")), ("count", Value::int(3i64))]);
    assert_eq!(render_under(None, BY_REACT, tree, &scalar, &Components::new()).unwrap(), "<x-grid rows=\"1\" count=\"3\"></x-grid>", "a React component with no React vendored is plain markup, which takes a scalar");
  }

  #[test]
  fn inert_and_an_empty_url_follow_the_major() {
    let tree = Tmpl::Fragment(vec![
      element("div", vec![("inert", Expr::Lit(Lit::Bool(true)))], Vec::new()),
      element("img", vec![("src", Expr::lit_str(""))], Vec::new()),
      element("a", vec![("href", Expr::lit_str(""))], Vec::new()),
    ]);
    let html = |react| render_under(react, BY_REACT, tree.clone(), &ValueMap::default(), &Components::new()).unwrap();
    assert_eq!(html(Some(ReactMajor::V18)), "<div></div><img src=\"\"/><a href=\"\"></a>");
    assert_eq!(html(Some(ReactMajor::V19)), "<div inert=\"\"></div><img/><a href=\"\"></a>");
    assert_eq!(html(None), "<div inert=\"true\"></div><img src=\"\"/><a href=\"\"></a>");
  }

  #[test]
  fn react_19_refuses_a_head_tag_it_would_move_out_of_hydrated_markup() {
    let title = element("title", Vec::new(), vec![Tmpl::Text("Tools".to_owned())]);
    let fail = render_under(Some(ReactMajor::V19), BY_REACT, title.clone(), &ValueMap::default(), &Components::new()).unwrap_err();
    assert!(fail.message.contains("`<title>`") && fail.message.contains("`meta` export"), "{}", fail.message);
    assert_eq!(render_under(Some(ReactMajor::V18), BY_REACT, title.clone(), &ValueMap::default(), &Components::new()).unwrap(), "<title>Tools</title>");
    assert_eq!(render_under(Some(ReactMajor::V19), None, title.clone(), &ValueMap::default(), &Components::new()).unwrap(), "<title>Tools</title>", "markup nothing hydrates keeps it in place");
    let in_svg = element("svg", Vec::new(), vec![title]);
    assert_eq!(render_under(Some(ReactMajor::V19), BY_REACT, in_svg, &ValueMap::default(), &Components::new()).unwrap(), "<svg><title>Tools</title></svg>", "an SVG title is the drawing's own");
  }

  #[test]
  fn a_bake_keeps_only_an_open_tag_every_markup_prints_alike() {
    let component = Component::new(Vec::new(), Tmpl::Fragment(vec![
      element("p", vec![("class", Expr::lit_str("a"))], Vec::new()),
      element("x-el", vec![("flag", Expr::Lit(Lit::Bool(true)))], Vec::new()),
      element("title", Vec::new(), vec![Tmpl::Text("t".to_owned())]),
    ]));
    let Tmpl::Fragment(children) = prepare(&component).render else { panic!("a fragment stays a fragment") };
    assert!(matches!(&children[0], Tmpl::Baked { open, .. } if open == "<p class=\"a\">"), "{:?}", children[0]);
    assert!(matches!(children[1], Tmpl::Element { .. }), "React 18 writes flag=\"true\" and 19 flag=\"\": {:?}", children[1]);
    assert!(matches!(children[2], Tmpl::Element { .. }), "a head tag React 19 may move stays where a render sees it: {:?}", children[2]);
  }

  #[test]
  fn an_element_template_renders_as_the_elements_declarative_shadow_root() {
    let mut library = Components::new();
    library.insert(
      "elements/x-grid.tsx#default".to_owned(),
      Arc::new(Component { body: Vec::new(), render: Tmpl::Fragment(vec![element("b", Vec::new(), vec![Tmpl::Expr(p("title"))]), Tmpl::Expr(Expr::Length(Box::new(p("rows"))))]), state: Vec::new(), handlers: Vec::new(), hydrated_by: None, shadow: None }),
    );
    let page = element("x-grid", vec![(SHADOW_ATTR, Expr::lit_str("elements/x-grid.tsx#default")), ("title", Expr::lit_str("t")), ("rows", p("rows"))], vec![Tmpl::Text("light".to_owned())]);
    let values = props(&[("rows", Value::seq(vec![Value::int(1i64), Value::int(2i64)]))]);
    for react in [None, Some(ReactMajor::V18), Some(ReactMajor::V19)] {
      assert_eq!(render_under(react, BY_REACT, page.clone(), &values, &library).unwrap(), "<x-grid title=\"t\"><template shadowrootmode=\"open\"><b>t</b>2</template>light</x-grid>", "under {react:?} the array reaches the template and never the host");
    }
  }

  #[test]
  fn an_element_template_writes_the_shadow_root_it_declares() {
    let closed = ShadowRoot { mode: ShadowMode::Closed, delegates_focus: true, clonable: true, serializable: true };
    for (shadow, open) in [
      (closed, "<template shadowrootmode=\"closed\" shadowrootdelegatesfocus shadowrootclonable shadowrootserializable>"),
      (ShadowRoot::default(), "<template shadowrootmode=\"open\">"),
    ] {
      let mut library = Components::new();
      library.insert(
        "elements/x-box.tsx#default".to_owned(),
        Arc::new(Component { body: Vec::new(), render: Tmpl::Text("in".to_owned()), state: Vec::new(), handlers: Vec::new(), hydrated_by: None, shadow: Some(shadow) }),
      );
      let page = element("x-box", vec![(SHADOW_ATTR, Expr::lit_str("elements/x-box.tsx#default"))], Vec::new());
      assert_eq!(render_under(None, BY_REACT, page, &ValueMap::default(), &library).unwrap(), format!("<x-box>{open}in</template></x-box>"));
    }
  }

  /// A component rendered inside the island keeps state of its own under its
  /// address: `<path>/<name>` in the map, `<path>/<index>` on its markers,
  /// one instance per keyed placement and per iteration.
  #[test]
  fn a_component_inside_the_island_keeps_state_under_its_address() {
    use crate::ast::Handler;
    let bump = |name: &str| Handler { event: "click".to_owned(), body: vec![Stmt::Return(Expr::Object(vec![Entry::Field(name.to_owned(), Expr::Arith(crate::ast::ArithOp::Add, Box::new(Expr::var(name)), Box::new(Expr::Lit(Lit::Int(1)))))]))] };
    let inner = Component {
      body: vec![Stmt::Let { name: "x".to_owned(), expr: Expr::var("$props").field("start") }],
      render: Tmpl::Element { tag: "i".to_owned(), attrs: vec![Entry::Field(format!("{HANDLER_ATTR}click"), Expr::Lit(Lit::Int(0)))], children: vec![Tmpl::Expr(Expr::var("x"))] },
      state: vec!["x".to_owned()],
      handlers: vec![bump("x")],
      hydrated_by: Some(crate::ast::HydratedBy::React),
      shadow: None,
    };
    let place = |id: u32, start: Expr| Tmpl::Component { module: "src/Inner.tsx#Inner".to_owned(), props: vec![Entry::Field("start".to_owned(), start)], children: Vec::new(), id, keyed: true };
    let widget = Component {
      body: vec![Stmt::Let { name: "n".to_owned(), expr: Expr::Lit(Lit::Int(1)) }],
      render: Tmpl::Element {
        tag: "div".to_owned(),
        attrs: Vec::new(),
        children: vec![
          Tmpl::Element { tag: "button".to_owned(), attrs: vec![Entry::Field(format!("{HANDLER_ATTR}click"), Expr::Lit(Lit::Int(0)))], children: vec![Tmpl::Expr(Expr::var("n"))] },
          place(1, Expr::var("n")),
          Tmpl::For { over: Expr::Array(vec![Entry::Item(Expr::Lit(Lit::Int(10))), Entry::Item(Expr::Lit(Lit::Int(20)))]), params: vec!["s".to_owned()], body: Box::new(place(2, Expr::var("s"))) },
        ],
      },
      state: vec!["n".to_owned()],
      handlers: vec![bump("n")],
      hydrated_by: Some(crate::ast::HydratedBy::React),
      shadow: None,
    };
    let mut library = Components::new();
    library.insert("src/Inner.tsx#Inner".to_owned(), Arc::new(inner));
    let props = ValueMap::default();
    let step = |state: &ValueMap, handler: Option<HandlerRef>| Interpreter::default().island_step("src/Widget.tsx#Widget", &widget, &props, state, handler, &Value::Null, &library);
    let spell = |state: &ValueMap| state.iter().map(|(k, v)| format!("{k}={}", stringify(v).unwrap())).collect::<Vec<_>>().join(" ");

    let first = step(&ValueMap::default(), None).unwrap();
    assert_eq!(first.rendered.html, "<div><button data-sf-on=\"click:0\">1</button><i data-sf-on=\"click:c1/0\">1</i><i data-sf-on=\"click:0.c2/0\">10</i><i data-sf-on=\"click:1.c2/0\">20</i></div>");
    assert_eq!(spell(&first.state), "n=1 c1/x=1 0.c2/x=10 1.c2/x=20", "every instance's state is in the map under its address");

    let second = step(&first.state, HandlerRef::parse("1.c2/0")).unwrap();
    assert_eq!(spell(&second.state), "n=1 c1/x=1 0.c2/x=10 1.c2/x=21", "the addressed instance stepped and nothing else moved");
    assert!(second.rendered.html.contains("<i data-sf-on=\"click:1.c2/0\">21</i>"), "{}", second.rendered.html);

    let third = step(&second.state, Some(HandlerRef::own(0))).unwrap();
    assert_eq!(spell(&third.state), "n=2 c1/x=1 0.c2/x=10 1.c2/x=21", "the island's own step keeps the nested state the browser carried, whatever the props now say");

    let missing = step(&third.state, HandlerRef::parse("c9/0")).unwrap_err();
    assert_eq!(missing.kind, FailureKind::NotFound, "{}", missing.message);
    let none = step(&third.state, HandlerRef::parse("c1/4")).unwrap_err();
    assert_eq!(none.kind, FailureKind::NotFound, "{}", none.message);
    assert!(HandlerRef::parse("c1/").is_none() && HandlerRef::parse("x").is_none() && HandlerRef::parse("2") == Some(HandlerRef::own(2)));
  }
}
