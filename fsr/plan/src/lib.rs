//! The artifact a build emits and a runtime reads at boot. It is deliberately
//! its own format rather than serde on the runtime types, so the file can gain
//! a field without the vocabulary crate gaining a dependency.

use serde::{Deserialize, Deserializer, Serialize};
use snapfire_fsr_core::{CacheKey, DataSourceId, ModuleId, NodeId, PlanNode, SlotName};
use snapfire_fsr_ir::{Body, Component};

/// Format 2 adds the `sources` table and makes actions rows. A format 1 file,
/// with bare action ids and no sources, still reads.
pub const FORMAT_VERSION: u32 = 2;
const OLDEST_READABLE: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PlanError {
  #[error("the plan file is not JSON: {0}")]
  Malformed(String),
  #[error("plan file version {found}, expected {OLDEST_READABLE} to {FORMAT_VERSION}")]
  Version { found: u32 },
  #[error("source row `{id}` is `lowered` but carries no body")]
  NoBody { id: String },
  #[error("{at}: `{module}` is not a module id, which is `path#export`")]
  Module { at: String, module: String },
  #[error("{at}: node id {id} appears twice in one route")]
  DuplicateNode { at: String, id: u32 },
  #[error("{at}: slot `{slot}` appears twice on one node")]
  DuplicateSlot { at: String, slot: String },
  #[error("no route may be empty")]
  EmptyPattern,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
  pub version: u32,
  pub routes: Vec<RouteEntry>,
  /// One row per data source the build knows about. A row with a body is a
  /// default the host binds unless Rust overrides the name; a row without one
  /// is a declaration Rust must answer.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub sources: Vec<SourceEntry>,
  /// The actions the application expects to exist. Declared so an unanswered
  /// action is a boot error rather than a 404 at request time.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub actions: Vec<ActionEntry>,
  /// One row per module the build lowered to a render tree. The host renders
  /// these in Rust and the browser hydrates over the output; a module without
  /// a row mounts in the browser only.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub components: Vec<ComponentEntry>,
  /// The tree a host renders, with status 404, for a path no route matches.
  /// Absent, the host answers with a plain text line.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub not_found: Option<Node>,
  /// One row per HTTP method a `route.ts` exports: a request the host answers
  /// with a value rather than a document.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub handlers: Vec<HandlerEntry>,
  /// The lowered `middleware.ts`, run before every request that is not a
  /// static file.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub middleware: Option<Body>,
  /// One row per `page.<slot>.tsx`: the pattern of the route it belongs to
  /// and the tree a soft navigation renders into a live layout's slot.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub intercepts: Vec<RouteEntry>,
}

/// A handler row: `method` and `pattern` are what the host matches, `id` is
/// `<route id>.<METHOD>`. A row without a body is a declaration Rust answers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HandlerEntry {
  pub id: String,
  pub method: String,
  pub pattern: String,
  pub owner: RowOwner,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub module: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub input: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub reason: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub body: Option<Body>,
}

impl HandlerEntry {
  pub fn lowered(id: impl Into<String>, method: impl Into<String>, pattern: impl Into<String>, module: impl Into<String>, body: Body) -> Self {
    Self { id: id.into(), method: method.into(), pattern: pattern.into(), owner: RowOwner::Lowered, module: Some(module.into()), input: None, reason: None, body: Some(body) }
  }

  pub fn rust(id: impl Into<String>, method: impl Into<String>, pattern: impl Into<String>) -> Self {
    Self { id: id.into(), method: method.into(), pattern: pattern.into(), owner: RowOwner::Rust, module: None, input: None, reason: None, body: None }
  }

  pub fn with_input(mut self, input: impl Into<String>) -> Self {
    self.input = Some(input.into());
    self
  }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComponentEntry {
  pub module: String,
  pub body: Component,
}

/// Who a row says answers it. The host may replace `lowered` with a Rust
/// override; `rust` is a declaration and nothing else.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RowOwner {
  Lowered,
  Engine,
  Rust,
}

impl RowOwner {
  pub fn as_str(&self) -> &'static str {
    match self {
      Self::Lowered => "lowered",
      Self::Engine => "engine",
      Self::Rust => "rust",
    }
  }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceEntry {
  pub id: String,
  pub owner: RowOwner,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub module: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub export: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub reason: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub body: Option<Body>,
  /// The module's `meta`, describing the document from this source's data.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub meta: Option<Body>,
  /// The module's `store`, seeding the browser's store from this source's data.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub store: Option<Body>,
}

impl SourceEntry {
  pub fn lowered(id: impl Into<String>, module: impl Into<String>, body: Body) -> Self {
    Self {
      id: id.into(),
      owner: RowOwner::Lowered,
      module: Some(module.into()),
      export: None,
      reason: None,
      body: Some(body),
      meta: None,
      store: None,
    }
  }

  pub fn with_meta(mut self, meta: Option<Body>) -> Self {
    self.meta = meta;
    self
  }

  pub fn with_store(mut self, store: Option<Body>) -> Self {
    self.store = store;
    self
  }

  pub fn rust(id: impl Into<String>) -> Self {
    Self { id: id.into(), owner: RowOwner::Rust, module: None, export: None, reason: None, body: None, meta: None, store: None }
  }
}

/// An action row. A format 1 file lists bare ids, which read as `rust` rows.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ActionEntry {
  pub id: String,
  pub owner: RowOwner,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub module: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub export: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub input: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub reason: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub body: Option<Body>,
}

impl ActionEntry {
  pub fn lowered(id: impl Into<String>, module: impl Into<String>, body: Body) -> Self {
    Self {
      id: id.into(),
      owner: RowOwner::Lowered,
      module: Some(module.into()),
      export: None,
      input: None,
      reason: None,
      body: Some(body),
    }
  }

  pub fn rust(id: impl Into<String>) -> Self {
    Self { id: id.into(), owner: RowOwner::Rust, module: None, export: None, input: None, reason: None, body: None }
  }

  pub fn with_input(mut self, input: impl Into<String>) -> Self {
    self.input = Some(input.into());
    self
  }
}

#[derive(Deserialize)]
struct ActionRow {
  id: String,
  #[serde(default = "rust_owner")]
  owner: RowOwner,
  #[serde(default)]
  module: Option<String>,
  #[serde(default)]
  export: Option<String>,
  #[serde(default)]
  input: Option<String>,
  #[serde(default)]
  reason: Option<String>,
  #[serde(default)]
  body: Option<Body>,
}

fn rust_owner() -> RowOwner {
  RowOwner::Rust
}

impl<'de> Deserialize<'de> for ActionEntry {
  fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
    struct Visitor;

    impl<'de> serde::de::Visitor<'de> for Visitor {
      type Value = ActionEntry;

      fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("an action id or an action row")
      }

      fn visit_str<E: serde::de::Error>(self, id: &str) -> Result<ActionEntry, E> {
        Ok(ActionEntry::rust(id))
      }

      fn visit_map<A: serde::de::MapAccess<'de>>(self, map: A) -> Result<ActionEntry, A::Error> {
        let row = ActionRow::deserialize(serde::de::value::MapAccessDeserializer::new(map))?;
        Ok(ActionEntry {
          id: row.id,
          owner: row.owner,
          module: row.module,
          export: row.export,
          input: row.input,
          reason: row.reason,
          body: row.body,
        })
      }
    }

    deserializer.deserialize_any(Visitor)
  }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouteEntry {
  pub pattern: String,
  pub plan: Node,
}

/// The serialized shape of a `PlanNode`. Optional fields are absent rather than
/// null, so a plan for a leaf route reads as three keys.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node {
  pub id: u32,
  pub module: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub source: Option<String>,
  #[serde(default, skip_serializing_if = "std::ops::Not::not")]
  pub deferred: bool,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub fallback: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub error: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub cache_key: Option<String>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub children: Vec<Child>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub keep: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Child {
  pub slot: String,
  pub node: Node,
}

fn module(raw: &str, at: &str) -> Result<ModuleId, PlanError> {
  raw
    .parse()
    .map_err(|_| PlanError::Module { at: at.to_owned(), module: raw.to_owned() })
}

impl Node {
  fn to_plan(&self, at: &str, seen: &mut Vec<u32>) -> Result<PlanNode, PlanError> {
    if seen.contains(&self.id) {
      return Err(PlanError::DuplicateNode { at: at.to_owned(), id: self.id });
    }
    seen.push(self.id);

    let mut plan = PlanNode::new(NodeId(self.id), module(&self.module, at)?);
    plan.data_source = self.source.clone().map(DataSourceId);
    plan.deferred = self.deferred;
    plan.cache_key = self.cache_key.clone().map(CacheKey);
    plan.fallback = match &self.fallback {
      Some(raw) => Some(module(raw, &format!("{at}/fallback"))?),
      None => None,
    };
    plan.error = match &self.error {
      Some(raw) => Some(module(raw, &format!("{at}/error"))?),
      None => None,
    };
    plan.keep = self.keep.iter().cloned().map(SlotName).collect();

    let mut slots: Vec<&str> = Vec::new();
    for child in &self.children {
      if slots.contains(&child.slot.as_str()) {
        return Err(PlanError::DuplicateSlot { at: at.to_owned(), slot: child.slot.clone() });
      }
      slots.push(&child.slot);
      let at = format!("{at}/{}", child.slot);
      plan
        .children
        .push((SlotName(child.slot.clone()), child.node.to_plan(&at, seen)?));
    }
    Ok(plan)
  }

  pub fn from_plan(plan: &PlanNode) -> Self {
    Self {
      id: plan.id.0,
      module: plan.module.to_string(),
      source: plan.data_source.as_ref().map(|s| s.0.clone()),
      deferred: plan.deferred,
      fallback: plan.fallback.as_ref().map(ToString::to_string),
      error: plan.error.as_ref().map(ToString::to_string),
      cache_key: plan.cache_key.as_ref().map(|k| k.0.clone()),
      children: plan
        .children
        .iter()
        .map(|(slot, node)| Child { slot: slot.0.clone(), node: Node::from_plan(node) })
        .collect(),
      keep: plan.keep.iter().map(|k| k.0.clone()).collect(),
    }
  }

  /// Every data source this subtree names, in tree order. This is the list a
  /// host checks its bindings against at boot.
  fn sources_into(&self, out: &mut Vec<String>) {
    if let Some(source) = &self.source {
      if !out.contains(source) {
        out.push(source.clone());
      }
    }
    for child in &self.children {
      child.node.sources_into(out);
    }
  }

  fn modules_into(&self, out: &mut Vec<String>) {
    for module in [Some(&self.module), self.fallback.as_ref(), self.error.as_ref()].into_iter().flatten() {
      if !out.contains(module) {
        out.push(module.clone());
      }
    }
    for child in &self.children {
      child.node.modules_into(out);
    }
  }
}

impl Manifest {
  pub fn new(routes: Vec<RouteEntry>) -> Self {
    Self { version: FORMAT_VERSION, routes, sources: Vec::new(), actions: Vec::new(), components: Vec::new(), not_found: None, handlers: Vec::new(), middleware: None, intercepts: Vec::new() }
  }

  pub fn with_intercepts(mut self, intercepts: Vec<RouteEntry>) -> Self {
    self.intercepts = intercepts;
    self
  }

  /// The intercepts the way the runtime wants them, checked like routes.
  pub fn intercepts(&self) -> Result<Vec<(String, PlanNode)>, PlanError> {
    let mut out = Vec::with_capacity(self.intercepts.len());
    for entry in &self.intercepts {
      if entry.pattern.is_empty() {
        return Err(PlanError::EmptyPattern);
      }
      out.push((entry.pattern.clone(), entry.plan.to_plan(&format!("intercept {}", entry.pattern), &mut Vec::new())?));
    }
    Ok(out)
  }

  pub fn with_middleware(mut self, middleware: Option<Body>) -> Self {
    self.middleware = middleware;
    self
  }
  pub fn with_handlers(mut self, handlers: Vec<HandlerEntry>) -> Self {
    self.handlers = handlers;
    self
  }
  pub fn lowered_handlers(&self) -> impl Iterator<Item = &HandlerEntry> {
    self.handlers.iter().filter(|row| row.owner == RowOwner::Lowered)
  }
  pub fn with_not_found(mut self, not_found: Option<Node>) -> Self {
    self.not_found = not_found;
    self
  }

  pub fn with_actions(mut self, actions: Vec<ActionEntry>) -> Self {
    self.actions = actions;
    self
  }

  pub fn with_sources(mut self, sources: Vec<SourceEntry>) -> Self {
    self.sources = sources;
    self
  }

  pub fn with_components(mut self, components: Vec<ComponentEntry>) -> Self {
    self.components = components;
    self
  }

  pub fn from_json(source: &str) -> Result<Self, PlanError> {
    let manifest: Manifest =
      serde_json::from_str(source).map_err(|e| PlanError::Malformed(e.to_string()))?;
    if !(OLDEST_READABLE..=FORMAT_VERSION).contains(&manifest.version) {
      return Err(PlanError::Version { found: manifest.version });
    }
    for row in &manifest.sources {
      if row.owner == RowOwner::Lowered && row.body.is_none() {
        return Err(PlanError::NoBody { id: row.id.clone() });
      }
    }
    for row in &manifest.actions {
      if row.owner == RowOwner::Lowered && row.body.is_none() {
        return Err(PlanError::NoBody { id: row.id.clone() });
      }
    }
    Ok(manifest)
  }

  /// The source rows that carry a body, which the host binds unless Rust
  /// overrides the name.
  pub fn lowered_sources(&self) -> impl Iterator<Item = &SourceEntry> {
    self.sources.iter().filter(|row| row.owner == RowOwner::Lowered)
  }

  pub fn lowered_actions(&self) -> impl Iterator<Item = &ActionEntry> {
    self.actions.iter().filter(|row| row.owner == RowOwner::Lowered)
  }

  pub fn action_ids(&self) -> Vec<String> {
    self.actions.iter().map(|row| row.id.clone()).collect()
  }

  pub fn to_json(&self) -> String {
    serde_json::to_string_pretty(self).expect("a manifest serializes")
  }

  /// The routes as the runtime wants them: a pattern and the plan it resolves
  /// to, in file order, so entry ids are stable across boots.
  pub fn routes(&self) -> Result<Vec<(String, PlanNode)>, PlanError> {
    let mut out = Vec::with_capacity(self.routes.len());
    for entry in &self.routes {
      if entry.pattern.is_empty() {
        return Err(PlanError::EmptyPattern);
      }
      let mut seen = Vec::new();
      out.push((entry.pattern.clone(), entry.plan.to_plan(&entry.pattern, &mut seen)?));
    }
    Ok(out)
  }

  /// The not-found tree as the runtime wants it, checked like a route's.
  pub fn not_found(&self) -> Result<Option<PlanNode>, PlanError> {
    match &self.not_found {
      Some(node) => Ok(Some(node.to_plan("not_found", &mut Vec::new())?)),
      None => Ok(None),
    }
  }

  /// Every data source named anywhere in the file.
  pub fn sources(&self) -> Vec<String> {
    let mut out = Vec::new();
    for entry in self.routes.iter().chain(&self.intercepts) {
      entry.plan.sources_into(&mut out);
    }
    if let Some(node) = &self.not_found {
      node.sources_into(&mut out);
    }
    out
  }

  /// Every module named anywhere in the file, including fallback and error
  /// modules, which is what an evaluator has to cover.
  pub fn modules(&self) -> Vec<String> {
    let mut out = Vec::new();
    for entry in self.routes.iter().chain(&self.intercepts) {
      entry.plan.modules_into(&mut out);
    }
    if let Some(node) = &self.not_found {
      node.modules_into(&mut out);
    }
    out
  }
}
