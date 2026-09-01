use snapfire_fsr_core::{ModuleId, NodeId, PlanNode, SlotName};
use snapfire_fsr_plan::Manifest;
use snapfire_fsr_runtime::{EntryId, MatchitMatcher, TableResolver};

use crate::server::shell::SHELL;

/// The build artifact. Nothing here describes a route; the file does.
pub const PLAN: &str = include_str!("../../app/plan.json");

#[derive(Debug, thiserror::Error)]
pub enum RouteError {
  #[error(transparent)]
  Plan(#[from] snapfire_fsr_plan::PlanError),
  #[error("`{0}` is claimed by the plan file and by Rust; mark the Rust one as an override")]
  Claimed(String),
  #[error("`{pattern}` is not a route pattern: {message}")]
  Pattern { pattern: String, message: String },
}

/// Routes come from the plan file, from Rust, or from both. A pattern claimed
/// twice is refused rather than shadowed.
#[derive(Default)]
pub struct Routes {
  entries: Vec<(String, PlanNode)>,
  overrides: Vec<String>,
}

impl Routes {
  pub fn from_manifest(source: &str) -> Result<Self, RouteError> {
    Ok(Self { entries: Manifest::from_json(source)?.routes()?, overrides: Vec::new() })
  }

  pub fn add(mut self, pattern: impl Into<String>, plan: PlanNode) -> Self {
    self.entries.push((pattern.into(), plan));
    self
  }

  pub fn replace(mut self, pattern: impl Into<String>, plan: PlanNode) -> Self {
    let pattern = pattern.into();
    self.overrides.push(pattern.clone());
    self.entries.push((pattern, plan));
    self
  }

  pub fn patterns(&self) -> Vec<&str> {
    self.entries.iter().map(|(p, _)| p.as_str()).collect()
  }

  pub fn build(self) -> Result<(MatchitMatcher, TableResolver), RouteError> {
    let mut kept: Vec<(String, PlanNode)> = Vec::new();
    for (pattern, plan) in self.entries {
      match kept.iter().position(|(p, _)| *p == pattern) {
        Some(_) if !self.overrides.contains(&pattern) => return Err(RouteError::Claimed(pattern)),
        Some(at) => kept[at] = (pattern, plan),
        None => kept.push((pattern, plan)),
      }
    }

    let mut matcher = MatchitMatcher::new();
    let mut resolver = TableResolver::new();
    for (index, (pattern, plan)) in kept.into_iter().enumerate() {
      let entry = EntryId(index as u32);
      matcher
        .insert(&pattern, entry)
        .map_err(|e| RouteError::Pattern { pattern: pattern.clone(), message: e.to_string() })?;
      resolver.insert(entry, plan);
    }
    Ok((matcher, resolver))
  }
}

/// A route the file system convention does not describe, added in Rust beside
/// the ones the plan file carries.
pub fn about_plan() -> PlanNode {
  let content = PlanNode::new(NodeId(1), ModuleId::new("app/main.tsx", "About"));
  let mut shell = PlanNode::new(NodeId(0), ModuleId::new(SHELL, "document"));
  shell.children.push((SlotName("content".into()), content));
  shell
}
