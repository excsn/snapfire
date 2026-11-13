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

use snapfire_fsr_core::{Value, ValueMap, ValueSeq};

use snapfire_fsr_runtime::FailureKind;

use crate::ast::{Component, Entry, ScopedStyle, ShadowMode, ShadowRoot, Stmt, Tmpl};
use crate::interp::Name;
use crate::interp::{Env, Fail, Hoists, Interpreter, Probe, Step, stringify, truthy};

mod markup;
mod react;
mod react18;
mod react19;
mod svelte;
mod vue;

pub use bake::bake_static_markup;
pub use markup::Frameworks;
pub(crate) use markup::Markup;
use react::is_custom_element;
pub use react::ReactMajor;
pub use svelte::SvelteMajor;
pub use vue::VueMajor;

mod attrs;
mod bake;
mod children;
mod prepare;
mod slots;
mod stamped;
mod tree;

use self::attrs::*;
pub(crate) use self::children::*;
use self::prepare::*;
use self::slots::*;
use self::stamped::*;
use self::tree::*;

/// Every lowered component by module id, so one may render another.
pub type Components = HashMap<String, Arc<Component>>;

const VOID: &[&str] = &["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track", "wbr"];

/// Attributes that are present or absent rather than valued under every React
/// major, React DOM's own list lowercased. React 19 adds `inert`.
const BOOLEAN: &[&str] = &["disabled", "checked", "selected", "readonly", "required", "hidden", "multiple", "open", "autofocus", "autoplay", "controls", "loop", "muted", "novalidate", "defer", "async", "default", "playsinline", "disablepictureinpicture", "disableremoteplayback", "allowfullscreen", "formnovalidate", "nomodule", "reversed", "scoped", "seamless", "itemscope"];

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
/// whether the next text under React's rules needs `<!-- -->` between them.
#[derive(Default, Clone)]
struct Out {
  html: String,
  text_open: bool,
  islands: Vec<RenderedIsland>,
  places: Vec<RenderedPlace>,
  /// The values the root component's state `let`s took.
  state: ValueMap,
}

/// A component's markup with the islands placed inside it. Each island sits
/// in `html` as `ISLAND_MARK` followed by its index in `islands` and a NUL,
/// the way a root slot sits as a `SLOT_MARK` and content written elsewhere as
/// a `PLACE_MARK` with its index in `places`; the evaluator turns each into
/// nodes.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Rendered {
  pub html: String,
  pub islands: Vec<RenderedIsland>,
  pub places: Vec<RenderedPlace>,
  /// The values the markup's hoisted expressions took, keyed as `Hoists::key`
  /// does; the island's props carry them under `$h`.
  pub hoisted: ValueMap,
  /// False when the component's own tree, which a framework renders again,
  /// reached a component the build could not lower: the markup is not the
  /// component's and the browser mounts it fresh.
  pub whole: bool,
}

/// The props key an island's hoisted values ride under.
pub const HOISTED_PROP: &str = "$h";

/// A `Tmpl::Place`'s content as rendered, for its target.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct RenderedPlace {
  pub target: String,
  pub body: Rendered,
}

/// A component rendered as an island: its module, the props it was given
/// and its own markup, which may hold islands of its own.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct RenderedIsland {
  pub module: String,
  pub props: ValueMap,
  pub when: Option<String>,
  /// `Some("server")` for an island whose events round-trip to the server.
  pub mode: Option<String>,
  /// The values the component's state `let`s took, for a server-mode island
  /// to carry to the browser as `$s`.
  pub state: ValueMap,
  /// The store keys a server-mode island reads, which the browser sends with
  /// each step and watches for a write by any other island, as `$sk`.
  pub reads: Vec<String>,
  /// The store values the island's markup was rendered from, the keys it
  /// reads that the store held, as `$sv`. A framework hydrates against them
  /// when the browser's store has moved on since; a server-mode island steps
  /// when it mounts into a store that has.
  pub rendered_store: ValueMap,
  /// The pending store keys the island reads, as `$aw`: the server held it, rendering none of its
  /// own markup. The browser mounts it once a seed has settled each.
  pub awaits: Vec<String>,
  /// The region this placement owns, keyed as `Hoists::key` keys a hoist: the
  /// enclosing component's module, the placement's id and the loop path. The
  /// browser derives the same string, which is how a re-render pairs an
  /// island with the payload that describes it.
  pub key: String,
  pub body: Rendered,
  /// Whether the server rendered the component. False for a module it has no body for, one held for a pending store key and one a body left to the browser, whose body is only the regions they offer.
  pub rendered: bool,
}

/// The props key a server-mode island's initial state rides under.
pub const STATE_PROP: &str = "$s";

/// The props key an island's region key rides under.
pub const KEY_PROP: &str = "$k";

/// The props key a server-mode island's store keys ride under.
pub const READS_PROP: &str = "$sk";

/// The props key the store values an island was rendered from ride under.
pub const RENDERED_STORE_PROP: &str = "$sv";

/// The props key a held island's pending store keys ride under.
pub const AWAITS_PROP: &str = "$aw";

/// The props key the placement writes the kinds of its typed props under: an object of prop name to a tag in [`typed`]. The server's render of the island reads the plain values; the browser mounts with each named prop wrapped in its tag, so it makes the real `Map`, `Set`, `Date` or `BigInt`.
pub const TYPES_PROP: &str = "$t";

/// The props key the placement writes the browser's source of each function prop under: an object of prop name to the module id `file#export` the browser imports that function from, each a string or an expression that yields one. The server's render of the island reads the function values; the browser mounts with each named prop as a module reference, which it resolves by importing the module.
pub const FUNCTIONS_PROP: &str = "$f";

pub use snapfire_fsr_core::value::typed;

/// `value` wrapped in the typed variant `tag` names, when its shape is the one that tag carries: a `$map` takes an object and sends its pairs, the others send the value as it is. A null prop stays null.
fn typed_value(tag: &str, value: &Value) -> Value {
  let payload = match (tag, value) {
    (_, Value::Null | Value::Undefined) => return Value::Null,
    (typed::MAP, Value::Map(map)) => Value::Seq(map.iter().map(|(k, v)| Value::seq(vec![Value::str(k.clone()), v.clone()])).collect()),
    (typed::SET, Value::Seq(_)) | (typed::DATE | typed::BIGINT, Value::Int(_) | Value::UInt(_) | Value::F64(_) | Value::F32(_)) | (typed::URL, Value::Str(_)) => value.clone(),
    _ => return value.clone(),
  };
  Value::Variant { tag: tag.to_owned(), payload: Some(Box::new(payload)) }
}

impl RenderedIsland {
  /// The props the browser mounts the island with: its own plus `$h` when
  /// anything was hoisted and `$s` in server mode.
  pub fn mount_props(&self) -> ValueMap {
    let types = match self.props.get(TYPES_PROP) {
      Some(Value::Map(types)) => Some(types),
      _ => None,
    };
    let mut props: ValueMap = self
      .props
      .iter()
      .filter(|(name, value)| handler_token(value).is_none() && *name != TYPES_PROP)
      .map(|(name, value)| match types.and_then(|t| t.get(name.as_str())) {
        Some(Value::Str(tag)) => (name.clone(), typed_value(tag.as_str(), value)),
        _ => (name.clone(), value.clone()),
      })
      .collect();
    if !self.body.hoisted.is_empty() {
      props.insert(HOISTED_PROP.to_owned(), Value::Map(self.body.hoisted.clone()));
    }
    if self.mode.as_deref() == Some(SERVER_MODE) {
      props.insert(STATE_PROP.to_owned(), Value::Map(self.state.clone()));
      if !self.reads.is_empty() {
        props.insert(READS_PROP.to_owned(), Value::seq(self.reads.iter().map(|key| Value::str(key.clone())).collect::<Vec<_>>()));
        props.insert(RENDERED_STORE_PROP.to_owned(), Value::Map(self.rendered_store.clone()));
      }
    } else if !self.rendered_store.is_empty() {
      props.insert(RENDERED_STORE_PROP.to_owned(), Value::Map(self.rendered_store.clone()));
    }
    if !self.key.is_empty() {
      props.insert(KEY_PROP.to_owned(), Value::str(self.key.clone()));
    }
    if !self.awaits.is_empty() {
      props.insert(AWAITS_PROP.to_owned(), Value::seq(self.awaits.iter().map(|key| Value::str(key.clone())).collect::<Vec<_>>()));
    }
    props
  }
}

/// The island mode whose events round-trip to the server.
pub const SERVER_MODE: &str = "server";

/// What `island_step` answers: the state after the handler, the island rendered from it and the actions the handler asked for, in order, for the host to dispatch.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Stepped {
  pub state: ValueMap,
  /// The store keys the handler wrote, with the values, for the browser to set.
  pub store: ValueMap,
  pub rendered: Rendered,
  pub acts: Vec<(String, Value)>,
  /// Each component the render entered, by the path a `HandlerRef` addresses it with and its module, root first.
  pub components: Vec<(String, String)>,
  /// Each value the render and the handler handed the host with `Builtin::Emit`, in the order they did.
  pub emitted: Vec<Emitted>,
  /// Each module cell's value as the step left it, by key.
  pub cells: ValueMap,
}

/// The handler a step runs: an index into the handlers of the component at
/// `path`, which is empty for the island's own component and otherwise the
/// address a nested component's markers carry, `c1` or `0.c2`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct HandlerRef {
  pub path: String,
  pub index: usize,
}

impl HandlerRef {
  pub fn own(index: usize) -> Self {
    Self { path: String::new(), index }
  }

  /// Handler `index` of the component at `path`.
  pub fn at(path: impl Into<String>, index: usize) -> Self {
    Self { path: path.into(), index }
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

/// Where a `Tmpl::Place` sat in the markup: `PLACE_MARK`, its index in
/// `places` in decimal, then a NUL.
pub const PLACE_MARK: &str = "\u{0}sf-place:";

/// Where a `Tmpl::Outlet` sat in the markup: `OUTLET_MARK`, the place's name, then a NUL.
pub const OUTLET_MARK: &str = "\u{0}sf-outlet:";

impl Out {
  /// Text under `markup`'s rules: React separates two adjacent runs with an
  /// empty comment and escapes three characters; Vue joins them and escapes
  /// five; Svelte joins them and escapes two; plain markup joins them and
  /// escapes three.
  fn text(&mut self, text: &str, markup: Markup) {
    if text.is_empty() {
      return;
    }
    match markup {
      Markup::Vue(_) => vue::escape(text, &mut self.html),
      Markup::Svelte(_) => svelte::escape_text(text, &mut self.html),
      Markup::Plain => escape_text(text, &mut self.html),
      Markup::React(_) => {
        if self.text_open {
          self.html.push_str("<!-- -->");
        }
        escape_text(text, &mut self.html);
        self.text_open = true;
      }
    }
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

/// The start of a body `let` that holds a state binding's next value: the prefix, then the binding's name. It holds `[value]` when the pass asked to render again with the binding at `value` and null when it did not. While any holds one the renderer runs the body again with each such binding at its value, as React runs again a component that set its own state while rendering.
pub const RERUN_PREFIX: &str = "$rerun:";

/// The start of a body `let` that holds a state binding's value from there on: the prefix, then the binding's name. The binding's held state after the render is that value, as a ref written while rendering keeps what was written.
pub const HOLD_PREFIX: &str = "$hold:";

/// The prop a host passes a step the module cells' values in, by key, as the last step left them; a step's [`Stepped::cells`] is what it leaves them at. A terminal keeps module state this way, so a write to a module-level `let` lasts from one step to the next as it does in JavaScript.
pub const CELLS_PROP: &str = "$cells";

/// A value a body handed its host: the path of the component that emitted it, as a `HandlerRef` addresses it, its kind and the value.
#[derive(Debug, Clone, PartialEq)]
pub struct Emitted {
  pub path: String,
  pub kind: String,
  pub value: Value,
}

/// The key a deferred is held under: a promise a terminal page made is `{ "$deferred": id }`, the host holding what it settles to. A Suspense boundary reads a thrown one as a child that is not ready.
pub const DEFERRED: &str = "$deferred";

/// The key of what a function that changes its parameters answers, [`crate::ast::Builtin::WriteBack`].
pub const WRITE_BACK: &str = "$writeBack";

/// A terminal page's Suspense boundary: an element of this tag holds the content and the fallback, in that order. The content renders unless it throws a deferred, [`DEFERRED`], in which case the fallback renders in its place and the step emits [`SUSPENDED`] with the deferred.
pub const SUSPENSE_TAG: &str = "$suspense";

/// The kind of value a step emits for each deferred a Suspense boundary is waiting on.
pub const SUSPENDED: &str = "suspended";

/// The kind of value a block in a step emits for an act it issues, `{ action, input }`, with the path of the component whose body or handler ran it: a terminal page starts work while rendering this way, which a host on the web never sees, since a web render issues no act.
pub const ACT: &str = "act";

/// What a body `let`'s name asks of the renderer besides binding it.
enum Directive<'n> {
  /// [`RERUN_PREFIX`] and the state binding's name.
  Rerun(&'n str),
  /// [`HOLD_PREFIX`] and the state binding's name.
  Hold(&'n str),
}

fn directive(name: &str) -> Option<Directive<'_>> {
  if let Some(state) = name.strip_prefix(RERUN_PREFIX) {
    return Some(Directive::Rerun(state));
  }
  if let Some(state) = name.strip_prefix(HOLD_PREFIX) {
    return Some(Directive::Hold(state));
  }
  None
}

/// How many passes a body may take, React's limit for a component that keeps setting its own state while rendering.
pub const RERUN_LIMIT: usize = 25;

pub fn slot_mark(name: &str) -> String {
  format!("{SLOT_MARK}{name}\u{0}")
}

/// A caller's children and the scope they read, rendered wherever the callee places its `Slot`.
struct Slot<'a> {
  children: &'a [Tmpl],
  scope: Rc<Vec<(Name, Value)>>,
  /// The hoist module and path of the caller, which its children key under:
  /// the browser builds them in the caller's render.
  keys: Option<(String, Vec<Step>)>,
  /// An island's children, which are the server's markup inside the island
  /// rather than part of its render: see `render_children`.
  island: bool,
  /// Whether the callee placed the slot: a lowered island whose template
  /// did not carries its children apart, see `CHILDREN_HELD_OPEN`.
  placed: bool,
  /// Whether the caller's tree is one a framework renders, which its
  /// children are part of.
  framework: bool,
  /// The caller's component path, which a handler in its children addresses.
  owner_path: String,
  /// The caller's scoped style, which a `<slot>` in its children stamps by.
  scoped: Option<ScopedStyle>,
}

/// The tag of a markup value: its payload indexes the render's table of what each one captured.
pub const MARKUP_VALUE: &str = "$markup";

/// What a markup value captured where it was written: the template, the names it reads with their values there, the hoist keys and owner it renders under.
#[derive(Debug, Clone)]
pub(crate) struct Made {
  tmpl: Arc<Tmpl>,
  scope: Vec<(Name, Value)>,
  keys: Option<(String, Vec<Step>)>,
  owner_path: String,
  framework: bool,
  scoped: Option<ScopedStyle>,
}

impl Env {
  /// `tmpl` as a value: what it reads is taken now, so it renders as written wherever it is interpolated.
  pub(crate) fn make(&mut self, tmpl: &Arc<Tmpl>) -> Result<Value, Fail> {
    let mut names = Vec::new();
    tmpl.visit(&mut |e| e.free_vars(&mut names));
    let scope = names.into_iter().filter_map(|name| self.scope.iter().rev().find(|(n, _)| *n == name).map(|(_, v)| (Name::new(name), v.clone()))).collect();
    let made = Made { tmpl: Arc::clone(tmpl), scope, keys: self.hoists.as_ref().map(|h| (h.module.clone(), h.path.clone())), owner_path: self.component_path.clone(), framework: self.in_framework, scoped: self.scoped.clone() };
    let Some(table) = &mut self.made else {
      return Err(Fail::internal("markup made while the body's independent lets evaluate in parallel"));
    };
    table.push(made);
    Ok(Value::Variant { tag: MARKUP_VALUE.to_owned(), payload: Some(Box::new(Value::Int((table.len() - 1) as i128))) })
  }
}

/// The key a marker in an island's props carries in place of a markup value: the number of the region the browser adopts for it.
pub const MARKUP_PROP: &str = "$sfm";

/// The attribute of the regions an island's markup props render in, `<template>` after the island's markup and `<sf-s>` where its own render places one.
pub const MARKUP_ATTR: &str = "data-sf-m";

/// `value` with each markup value in it a marker numbering the region it renders in, the made index of each pushed to `marks`.
fn marked(value: &Value, marks: &mut Vec<usize>) -> Value {
  if let Some(index) = made_index(value) {
    let k = marks.iter().position(|&m| m == index).unwrap_or_else(|| {
      marks.push(index);
      marks.len() - 1
    });
    let mut marker = ValueMap::default();
    marker.insert(MARKUP_PROP.to_owned(), Value::Int(k as i128));
    return Value::Map(marker);
  }
  match value {
    Value::Seq(items) if items.iter().any(holds_made) => Value::seq(items.iter().map(|item| marked(item, marks)).collect::<Vec<Value>>()),
    Value::Map(map) if map.values().any(holds_made) => Value::Map(map.iter().map(|(k, v)| (k.clone(), marked(v, marks))).collect()),
    other => other.clone(),
  }
}

/// Each markup value `marks` names that the island's render did not place, in a `<template>` of its own the browser adopts as the prop it stands for.
fn write_marked(env: &mut Env, marks: &[usize], placed: &[usize], library: &Components, out: &mut Out) -> Result<(), Fail> {
  let outer = std::mem::take(&mut env.marks);
  let mut result = Ok(());
  for (k, &index) in marks.iter().enumerate().filter(|(_, index)| !placed.contains(index)) {
    out.markup(&format!("<template {MARKUP_ATTR}=\"{k}\">"));
    result = render_value(env, &made_value(index), library, out);
    out.close_tag("template");
    if result.is_err() {
      break;
    }
  }
  env.marks = outer;
  result
}

fn made_value(index: usize) -> Value {
  Value::Variant { tag: MARKUP_VALUE.to_owned(), payload: Some(Box::new(Value::Int(index as i128))) }
}

fn made_index(value: &Value) -> Option<usize> {
  match value {
    Value::Variant { tag, payload: Some(payload) } if tag == MARKUP_VALUE => match &**payload {
      Value::Int(n) => usize::try_from(*n).ok(),
      _ => None,
    },
    _ => None,
  }
}

/// Whether `value` is or holds a markup value, which only a render can write.
pub(crate) fn holds_made(value: &Value) -> bool {
  match value {
    Value::Seq(items) => items.iter().any(holds_made),
    Value::Map(map) => map.values().any(holds_made),
    other => made_index(other).is_some(),
  }
}

/// `value` interpolated: a markup value renders its template in the scope it captured, an array each item, anything else as text.
fn render_value(env: &mut Env, value: &Value, library: &Components, out: &mut Out) -> Result<(), Fail> {
  if let Some(index) = made_index(value) {
    let Some(made) = env.made.as_ref().and_then(|table| table.get(index)).cloned() else {
      return Err(Fail::internal("a markup value from another render"));
    };
    let slot = Slot { children: std::slice::from_ref(&*made.tmpl), scope: Rc::new(made.scope), keys: made.keys, island: false, placed: false, framework: made.framework, owner_path: made.owner_path, scoped: made.scoped };
    let Some(k) = env.marks.iter().position(|&m| m == index) else {
      return render_slot_content(env, slot, library, &mut Vec::new(), out);
    };
    let outer = std::mem::take(&mut env.marks);
    if !env.placed.contains(&index) {
      env.placed.push(index);
    }
    out.markup(&format!("<sf-s {MARKUP_ATTR}=\"{k}\">"));
    let result = render_slot_content(env, slot, library, &mut Vec::new(), out);
    out.close_tag("sf-s");
    env.marks = outer;
    return result;
  }
  match value {
    Value::Seq(items) if items.iter().any(holds_made) => {
      for item in items {
        render_value(env, item, library, out)?;
      }
      Ok(())
    }
    other => interpolate(other, out, env.markup),
  }
}

/// The region an island's children render in, which the mounter hands the
/// component as its `children` and never renders itself.
pub const CHILDREN_OPEN: &str = "<sf-s data-sf-children>";

/// Where a lowered React or Vue island's children go when its render did not place
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
    crate::interp::on_enough_stack(|| self.render_module_in(module, component, props, library))
  }

  fn render_module_in(&self, module: &str, component: &Component, props: &ValueMap, library: &Components) -> Result<Rendered, Fail> {
    let mut env = self.env_for(module, props);
    env.heap_live = library.values().any(|component| matches!(component.body.first(), Some(Stmt::Let { name, .. }) if name == "$heap"));
    let mut out = Out::default();
    let mut slots = Vec::new();
    render_component(&mut env, component, library, &mut slots, &mut out)?;
    let hoisted = env.hoists.take().map(|h| h.table).unwrap_or_default();
    Ok(Rendered { html: out.html, islands: out.islands, places: out.places, hoisted, whole: !env.unrendered })
  }

  /// `render_module` for an island placed with no children, as a page
  /// places one: a slot it renders writes an empty children region, where a
  /// root component's writes `slot_mark`.
  pub fn render_island(&self, module: &str, component: &Component, props: &ValueMap, library: &Components) -> Result<Rendered, Fail> {
    let mut env = self.env_for(module, props);
    env.heap_live = library.values().any(|component| matches!(component.body.first(), Some(Stmt::Let { name, .. }) if name == "$heap"));
    env.island = island_owner(module, library);
    let mut out = Out::default();
    let scope = Rc::new(env.scope.clone());
    let mut slots = vec![Slot { children: &[], scope, keys: None, island: true, placed: false, framework: false, owner_path: String::new(), scoped: None }];
    render_component(&mut env, component, library, &mut slots, &mut out)?;
    let hoisted = env.hoists.take().map(|h| h.table).unwrap_or_default();
    Ok(Rendered { html: out.html, islands: out.islands, places: out.places, hoisted, whole: !env.unrendered })
  }

  fn env_for(&self, module: &str, props: &ValueMap) -> Env {
    let mut env = Env::detached(self, component_scope(Value::Map(props.clone()), Value::Map(ValueMap::default())));
    if let Some(Value::Map(store)) = props.get("$store") {
      env.store = store.clone();
    }
    if let Some(Value::Map(cells)) = props.get(CELLS_PROP) {
      env.module_seed = cells.clone();
    }
    if let Some(Value::Seq(pending)) = props.get(snapfire_fsr_runtime::PENDING_PROP) {
      env.pending = pending.iter().filter_map(|key| match key {
        Value::Str(key) => Some(key.to_string()),
        _ => None,
      }).collect();
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
    if env.ctx.config.is_empty() {
      env.ctx.config = (**self.config()).clone();
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
    env.heap_live = library.values().any(|component| matches!(component.body.first(), Some(Stmt::Let { name, .. }) if name == "$heap"));
    env.server_mode = true;
    env.in_framework = true;
    let mut state = state.clone();
    let mut written = ValueMap::default();
    env.emitted.clear();
    let mut queue: std::collections::VecDeque<(HandlerRef, Value)> = handler.map(|handler| (handler, event.clone())).into_iter().collect();
    let mut runs = 0;
    while let Some((handler, event)) = queue.pop_front() {
      runs += 1;
      if runs > HANDLER_CALLS {
        return Err(Fail::internal(format!("handlers called one another more than {HANDLER_CALLS} times in one step")));
      }
      env.handler_calls.clear();
      let (target, own_props, path): (&Component, Option<ValueMap>, String) = if handler.path.is_empty() {
        (component, None, String::new())
      } else {
        env.state = Some(state.clone());
        env.probe = Some(Probe { path: handler.path.clone(), found: None });
        let mut scratch = Out::default();
        let mut slots = Vec::new();
        render_component(&mut env, component, library, &mut slots, &mut scratch)?;
        env.emitted.clear();
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
        env.scope.extend(component_scope(Value::Map(own), Value::Map(ValueMap::default())));
      }
      for stmt in &target.body {
        let Stmt::Let { name, expr } = stmt else { continue };
        let value = match state.get(&state_key(&path, name)) {
          Some(held) => held.clone(),
          // A `let` whose render is suspended, a Suspense boundary waiting on what it threw, is not there for the handler; one that reads it fails on the name.
          None => match env.eval_sync(expr) {
            Ok(value) => value,
            Err(fail) if fail.thrown.is_some() => continue,
            Err(fail) => return Err(fail),
          },
        };
        env.scope.push((Name::new(name), value));
      }
      env.emitted.clear();
      let mut own_state: ValueMap = if path.is_empty() { state.clone() } else { target.state.iter().filter_map(|name| state.get(&state_key(&path, name)).map(|value| (name.clone(), value.clone()))).collect() };
      for (name, _) in &target.stores {
        if let Some((_, value)) = env.scope.iter().rev().find(|(bound, _)| bound == name) {
          own_state.insert(name.clone(), value.clone());
        }
      }
      env.scope.push((Name::new_static("$state"), Value::Map(own_state)));
      env.scope.push((Name::new_static("$event"), event.clone()));
      env.store_written.clear();
      let outer_path = std::mem::replace(&mut env.component_path, path.clone());
      env.handling = true;
      let patch = env.block_sync(&body.body).and_then(crate::interp::Flow::value);
      env.handling = false;
      env.component_path = outer_path;
      let patch = patch?;
      env.scope.truncate(depth);
      written.extend(std::mem::take(&mut env.store_written));
      if let Value::Map(patch) = patch {
        for (name, value) in patch {
          if let Some(key) = target.store_key(&name) {
            env.set_store(key, Some(value.clone()));
            written.insert(key.to_owned(), value);
          } else if target.state.iter().any(|held| held.as_str() == name.as_str()) {
            state.insert(state_key(&path, &name), value);
          }
        }
      }
      queue.extend(std::mem::take(&mut env.handler_calls).into_iter().filter_map(|(token, event)| handler_ref(&token).map(|handler| (handler, event))));
    }
    env.state = Some(state);
    if !written.is_empty() {
      recompute_derived(&mut env, &written);
    }
    let mut out = Out::default();
    let mut slots = Vec::new();
    env.entered.clear();
    render_component(&mut env, component, library, &mut slots, &mut out)?;
    let hoisted = env.hoists.take().map(|h| h.table).unwrap_or_default();
    let acts = std::mem::take(&mut env.acts);
    let components = std::mem::take(&mut env.entered);
    let emitted = std::mem::take(&mut env.emitted);
    let cells = env.module_cells.iter().filter_map(|(key, index)| env.cells.as_ref().and_then(|cells| cells.get(*index)).map(|value| (key.clone(), value.clone()))).collect();
    Ok(Stepped { state: out.state, store: written, rendered: Rendered { html: out.html, islands: out.islands, places: out.places, hoisted, whole: !env.unrendered }, acts, components, emitted, cells })
  }
}

/// How deep components may nest in one render: what stops a component that renders itself without a case that stops. The stack grows to fit, [`crate::interp::on_enough_stack`], so this holds on any machine and any thread.
pub const MAX_CALLS: usize = 512;

/// A component whose body opens with a `let` of this name reads how many children its placement passed, `Children.count(children)`, empty nodes included; the placement passes the count as the prop of the same name.
pub const CHILD_COUNT: &str = "$childCount";

/// The same for `Children.toArray(children).length`, which leaves empty nodes out.
pub const CHILD_ITEMS: &str = "$childItems";

/// The same for `Children.toArray(children)` itself, each child a markup value made where the caller wrote it, the empty ones left out.
pub const CHILD_VALUES: &str = "$childValues";

/// A `Tmpl::Let` of this name around a `Tmpl::Fragment` marks the fragment as an array's items, which renders as its items and counts as each of them; any other fragment is `<>…</>`, one child.
pub const ARRAY_ITEMS: &str = "$items";

/// The scoped-style attribute of the Vue parent that placed a child, which Vue writes on the child's root element after the child's own attributes and before the child's own scope.
pub const PARENT_SCOPE_PROP: &str = "$scope";

/// The attributes a Vue parent gave a child beyond its declared props, as an object, which the child's root element takes: `class` after the root's own and anything else in place of the root's value or after its attributes.
pub const FALLTHROUGH_PROP: &str = "$attrs";

/// The tag of an element whose tag a value chose at render: its first attribute is the choice, an element name or a component reference.
pub const CHOSEN_TAG: &str = "$tag";

/// The tag of a React boundary: its `kind` attribute is `$` for a `<Suspense>` whose children rendered and `&` for a visible `<Activity>`, written as React's server writes them, `<!--$-->` and `<!--/$-->` around the children.
pub const BOUNDARY_TAG: &str = "$boundary";

/// The tag of a component reference: its payload is the component's module.
pub const COMPONENT_VALUE: &str = "$component";

/// Of `keys`, the ones `store` holds, with their values: what an island's
/// markup was rendered from.
pub fn rendered_store(store: &ValueMap, keys: &[String]) -> ValueMap {
  keys.iter().filter_map(|key| store.get(key).map(|value| (key.clone(), value.clone()))).collect()
}

pub fn store_keys(component: &Component, library: &Components) -> Vec<String> {
  fn walk(component: &Component, library: &Components, seen: &mut Vec<String>, keys: &mut std::collections::BTreeSet<String>) {
    component.visit(&mut |expr| {
      if let crate::ast::Expr::Store(key) = expr {
        keys.insert(key.clone());
      }
    });
    let mut inline = Vec::new();
    inline_modules(&component.render, &mut inline);
    for module in inline {
      if seen.contains(&module) {
        continue;
      }
      seen.push(module.clone());
      if let Some(nested) = library.get(&module) {
        walk(nested, library, seen, keys);
      }
    }
  }
  fn inline_modules(tmpl: &Tmpl, out: &mut Vec<String>) {
    match tmpl {
      Tmpl::Component { module, children, .. } => {
        out.push(module.clone());
        children.iter().for_each(|c| inline_modules(c, out));
      }
      Tmpl::Element { children, .. } | Tmpl::Fragment(children) | Tmpl::Baked { children, .. } | Tmpl::Island { children, .. } | Tmpl::Place { children, .. } => children.iter().for_each(|c| inline_modules(c, out)),
      Tmpl::If { then, r#else, .. } => {
        inline_modules(then, out);
        if let Some(e) = r#else {
          inline_modules(e, out);
        }
      }
      Tmpl::For { body, .. } => inline_modules(body, out),
      Tmpl::Let { then, .. } => inline_modules(then, out),
      Tmpl::Text(_) | Tmpl::Expr(_) | Tmpl::Html(_) | Tmpl::Slot(_) | Tmpl::Marker(_) | Tmpl::Outlet { .. } => {}
    }
  }
  let mut keys = std::collections::BTreeSet::new();
  walk(component, library, &mut Vec::new(), &mut keys);
  keys.into_iter().collect()
}

/// The style properties React DOM 19.1.0 leaves unitless, keyed by the name as written in the style object, as its `setValueForStyle` and server renderer check them. Every other number gets `px`.
const UNITLESS: &[&str] = &["animationIterationCount", "aspectRatio", "borderImageOutset", "borderImageSlice", "borderImageWidth", "boxFlex", "boxFlexGroup", "boxOrdinalGroup", "columnCount", "columns", "flex", "flexGrow", "flexPositive", "flexShrink", "flexNegative", "flexOrder", "gridArea", "gridRow", "gridRowEnd", "gridRowSpan", "gridRowStart", "gridColumn", "gridColumnEnd", "gridColumnSpan", "gridColumnStart", "fontWeight", "lineClamp", "lineHeight", "opacity", "order", "orphans", "scale", "tabSize", "widows", "zIndex", "zoom", "fillOpacity", "floodOpacity", "stopOpacity", "strokeDasharray", "strokeDashoffset", "strokeMiterlimit", "strokeOpacity", "strokeWidth", "MozAnimationIterationCount", "MozBoxFlex", "MozBoxFlexGroup", "MozLineClamp", "msAnimationIterationCount", "msFlex", "msZoom", "msFlexGrow", "msFlexNegative", "msFlexOrder", "msFlexPositive", "msFlexShrink", "msGridColumn", "msGridColumnSpan", "msGridRow", "msGridRowSpan", "WebkitAnimationIterationCount", "WebkitBoxFlex", "WebKitBoxFlexGroup", "WebkitBoxOrdinalGroup", "WebkitColumnCount", "WebkitColumns", "WebkitFlex", "WebkitFlexGrow", "WebkitFlexPositive", "WebkitFlexShrink", "WebkitLineClamp"];

/// A style property's CSS spelling: `fontSize` is `font-size` and
/// `WebkitLineClamp` is `-webkit-line-clamp`. A custom property keeps its name.
pub fn css_name(key: &str) -> String {
  if key.starts_with("--") {
    return key.to_owned();
  }
  let mut out = String::with_capacity(key.len() + 4);
  for c in key.chars() {
    if c.is_ascii_uppercase() {
      out.push('-');
      out.push(c.to_ascii_lowercase());
    } else {
      out.push(c);
    }
  }
  out
}

/// The content of every `<style>` element `component` renders, in the order
/// written, as the server writes it under the component's markup: `None` for
/// one whose content the render computes, which no build-time hash covers.
pub fn inline_styles(component: &Component) -> Vec<Option<String>> {
  fn literal(child: &Tmpl) -> bool {
    matches!(child, Tmpl::Text(_) | Tmpl::Expr(crate::Expr::Lit(crate::Lit::Str(_)) | crate::Expr::Folded(Value::Str(_))))
  }
  fn walk(tmpl: &Tmpl, owner: crate::Owner, out: &mut Vec<Option<String>>) {
    match tmpl {
      Tmpl::Element { tag, children, .. } if tag == "style" => {
        if !children.iter().all(literal) {
          out.push(None);
          return;
        }
        let alone = Component::new(owner, Vec::new(), Tmpl::Element { tag: "style".to_owned(), attrs: Vec::new(), children: children.clone() });
        let written = Interpreter::default().render(&alone, &ValueMap::default(), &Components::new()).ok().map(|rendered| rendered.html);
        out.push(written.and_then(|html| Some(html.strip_prefix("<style>")?.strip_suffix("</style>")?.to_owned())));
      }
      Tmpl::Element { children, .. } | Tmpl::Fragment(children) | Tmpl::Component { children, .. } | Tmpl::Island { children, .. } | Tmpl::Baked { children, .. } | Tmpl::Place { children, .. } => {
        children.iter().for_each(|child| walk(child, owner, out));
      }
      Tmpl::If { then, r#else, .. } => {
        walk(then, owner, out);
        if let Some(other) = r#else {
          walk(other, owner, out);
        }
      }
      Tmpl::For { body, .. } => walk(body, owner, out),
      Tmpl::Let { then, .. } => walk(then, owner, out),
      Tmpl::Text(_) | Tmpl::Expr(_) | Tmpl::Html(_) | Tmpl::Slot(_) | Tmpl::Marker(_) | Tmpl::Outlet { .. } => {}
    }
  }
  let mut out = Vec::new();
  walk(&component.render, component.owner, &mut out);
  out
}

/// Rewrites a loaded component so every element whose open tag is entirely
/// literal carries that tag as one slice. The plan is untouched: this runs
/// when a component is put into a [`Components`] library and an element it
/// cannot bake is left exactly as it was.
pub fn prepare(component: &Component) -> Component {
  let mut body = component.body.clone();
  for stmt in &mut body {
    if let Stmt::Let { expr, .. } = stmt {
      fold_expr(expr);
    }
  }
  Component { body, render: prepare_tmpl(&component.render), state: component.state.clone(), stores: component.stores.clone(), handlers: component.handlers.clone(), owner: component.owner, shadow: component.shadow, scope: component.scope.clone() }
}

/// The attribute marking an element whose inner markup is recorded as a hoisted chunk and its id.
pub const CHUNK_ATTR: &str = "$chunk";
/// The attribute beside `CHUNK_ATTR` on a chunk under a branch on state, recorded whether or not the branch is taken.
pub const UNTAKEN_ATTR: &str = "$untaken";

/// An element's `dangerouslySetInnerHTML`, holding the `__html` expression.
/// Its string is written to the document as it stands, so whoever produced it
/// answers for it; the element's children never render.
pub const RAW_ATTR: &str = "$html";

/// On a Svelte element: the flags of Svelte's `$.attributes` for one whose
/// attributes come from a spread, 1 for an SVG or MathML element, 2 to keep
/// the names' case and 4 for an `<input>`; 8 for `<svelte:element>`, whose
/// void tag `$.element` closes with `>`.
pub const SVELTE_FLAGS: &str = "$svelte";

/// On a Svelte `<option>`: whether `selected=""` follows its attributes, as
/// `$$renderer.option` writes it for an option whose value the select holds.
pub const SVELTE_SELECTED: &str = "$selected";

/// A placement of a custom element whose shadow template sits under
/// `elements/`, holding the template's module. The server writes the
/// template as the element's declarative shadow root.
pub const SHADOW_ATTR: &str = "$shadow";

/// The prefix of an attribute binding a handler to its element: `$on:click`
/// holding the handler's index. Printed as `data-sf-on="click:0"` in server
/// mode and never otherwise.
pub const HANDLER_ATTR: &str = "$on:";
/// The variant tag of a handler reference, the value `Expr::Handler` evaluates to: its payload is the token a server island's `data-sf-on` names it by, `<index>` or `<path>/<index>`.
pub const HANDLER_VALUE: &str = "$handler";

/// The handler index of a function only the browser holds, a setter or a dispatcher read as a value: no handler has it, so the value is a function that prints nothing and that no server step can run.
pub const BROWSER_HANDLER: u32 = u32::MAX;

/// A reference to handler `index` of the component rendered at `path`.
pub fn handler_value(path: &str, index: u32) -> Value {
  let token = if path.is_empty() { index.to_string() } else { format!("{path}/{index}") };
  Value::Variant { tag: HANDLER_VALUE.to_owned(), payload: Some(Box::new(Value::str(token))) }
}

/// The handler a reference's token names, `path/index` or `index` for the root; `None` for the browser's.
pub fn handler_ref(token: &str) -> Option<HandlerRef> {
  let (path, index) = token.rsplit_once('/').unwrap_or(("", token));
  let index: u32 = index.parse().ok()?;
  (index != BROWSER_HANDLER).then(|| HandlerRef::at(path, index as usize))
}

/// How many handlers one step runs, the one it was asked for and those it called through references, before it is taken for a loop.
const HANDLER_CALLS: usize = 64;

/// The token of a handler reference; `None` for any other value.
pub fn handler_token(value: &Value) -> Option<&str> {
  match value {
    Value::Variant { tag, payload: Some(payload) } if tag == HANDLER_VALUE => match &**payload {
      Value::Str(token) => Some(token.as_str()),
      _ => None,
    },
    _ => None,
  }
}
/// Bound beside `$props` in every component's scope: an object naming each slot
/// the placement's children fill, `default` (as `true`) for children that are
/// not a named wrapper and each `SLOT_CONTENT_PREFIX` wrapper whose value holds,
/// as that value.
pub const SLOTS_GIVEN: &str = "$given";

/// A `Tmpl::Let` among a placement's children named with this prefix and a slot is the caller's content for that slot: a Vue named slot or the markup a JSX caller passed as the prop of that name; `default` is the default slot's when it takes props.
pub const SLOT_CONTENT_PREFIX: &str = "$slot:";
/// A `Tmpl::Let` in a Vue template named with this prefix and a slot is that `<slot>`: its value the props it hands the caller's content, its body the fallback written when the caller gave none.
pub const SLOT_OUT_PREFIX: &str = "$slotOut:";

/// The prefix of a `{prop}` read that takes the placement's `$slot:` fill for
/// the prop before its value: `$slotRead:<prop>`, the read as its expression.
pub const SLOT_READ_PREFIX: &str = "$slotRead:";
/// The name the caller's content reads a scoped slot's props under.
pub const SLOT_PROPS: &str = "$slotProps";

/// A `Tmpl::Let` named with this prefix and a context, `$ctx:file#name`, is a provider: what it binds is the value `Expr::Context` reads anywhere beneath it, inside nested components too.
pub const CONTEXT_PREFIX: &str = "$ctx:";
/// An element's React `key`, printed as `data-sf-key` in server mode so the
/// browser's patch keeps a moved element and never otherwise.
pub const KEY_ATTR: &str = "$key";
/// Left on an element whose handler the build could not lower, holding the
/// line and the reason; an island in server mode is refused over it.
pub const UNLOWERED_ATTR: &str = "$unlowered";

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
    "autoPlay" => "autoplay",
    "playsInline" => "playsinline",
    "disablePictureInPicture" => "disablepictureinpicture",
    "disableRemotePlayback" => "disableremoteplayback",
    "allowFullScreen" => "allowfullscreen",
    "formNoValidate" => "formnovalidate",
    "noModule" => "nomodule",
    "itemScope" => "itemscope",
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

/// What React 19 writes as the `action` or `formAction` of a form whose action is a function, `actionJavaScriptURL` in `ReactFizzConfigDOM.js`.
pub const FUNCTION_FORM_ACTION: &str = "javascript:throw new Error('React form unexpectedly submitted.')";

/// What React 19 writes in place of a `javascript:` URL, so hydration agrees with the server.
const BLOCKED_JAVASCRIPT_URL: &str = "javascript:throw new Error('React has blocked a javascript: URL as a security precaution.')";

#[cfg(test)]
mod tests;

#[cfg(test)]
mod hoist_tests;

#[cfg(test)]
mod server_tests;

#[cfg(test)]
mod island_tests;

#[cfg(test)]
mod markup_tests;

#[cfg(test)]
mod bake_tests;
