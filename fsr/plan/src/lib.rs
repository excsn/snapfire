//! The artifact a build emits and a runtime reads at boot. It is deliberately
//! its own format rather than serde on the runtime types, so the file can gain
//! a field without the vocabulary crate gaining a dependency.

use serde::{Deserialize, Serialize};
use snapfire_fsr_core::{CacheKey, DataSourceId, ModuleId, NodeId, PlanNode, SlotName};

pub const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PlanError {
  #[error("the plan file is not JSON: {0}")]
  Malformed(String),
  #[error("plan file version {found}, expected {FORMAT_VERSION}")]
  Version { found: u32 },
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
  /// The action ids the application expects to exist. Declared so an
  /// unanswered action is a boot error rather than a 404 at request time.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub actions: Vec<String>,
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
    Self { version: FORMAT_VERSION, routes, actions: Vec::new() }
  }

  pub fn with_actions(mut self, actions: Vec<String>) -> Self {
    self.actions = actions;
    self
  }

  pub fn from_json(source: &str) -> Result<Self, PlanError> {
    let manifest: Manifest =
      serde_json::from_str(source).map_err(|e| PlanError::Malformed(e.to_string()))?;
    if manifest.version != FORMAT_VERSION {
      return Err(PlanError::Version { found: manifest.version });
    }
    Ok(manifest)
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

  /// Every data source named anywhere in the file.
  pub fn sources(&self) -> Vec<String> {
    let mut out = Vec::new();
    for entry in &self.routes {
      entry.plan.sources_into(&mut out);
    }
    out
  }

  /// Every module named anywhere in the file, including fallback and error
  /// modules, which is what an evaluator has to cover.
  pub fn modules(&self) -> Vec<String> {
    let mut out = Vec::new();
    for entry in &self.routes {
      entry.plan.modules_into(&mut out);
    }
    out
  }
}
