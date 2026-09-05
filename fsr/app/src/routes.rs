use snapfire_fsr_core::PlanNode;
use snapfire_fsr_plan::Manifest;
use snapfire_fsr_runtime::{EntryId, MatchitMatcher, TableResolver};

use crate::{BindError, IntoPlan, Owner};

/// Routes from the plan file, from Rust, or from both. A pattern claimed twice
/// is refused rather than shadowed, so adding a route is additive by default
/// and replacing one is deliberate.
#[derive(Default)]
pub struct Routes {
  entries: Vec<(String, PlanNode, Owner)>,
  overrides: Vec<String>,
  failed: Option<BindError>,
  not_found: Option<PlanNode>,
  intercepts: Vec<(String, PlanNode)>,
}

impl Routes {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn from_manifest(source: &str) -> Result<Self, BindError> {
    let manifest = Manifest::from_json(source)?;
    let entries = manifest
      .routes()?
      .into_iter()
      .map(|(pattern, plan)| (pattern, plan, Owner::PlanFile))
      .collect();
    Ok(Self { entries, overrides: Vec::new(), failed: None, not_found: manifest.not_found()?, intercepts: manifest.intercepts()? })
  }

  /// The tree rendered, with status 404, for a path no route matches. Replaces
  /// the plan file's when it has one.
  pub fn not_found(mut self, plan: impl IntoPlan) -> Self {
    match plan.into_plan() {
      Ok(plan) => self.not_found = Some(plan),
      Err(e) => {
        self.failed.get_or_insert(e);
      }
    }
    self
  }

  pub fn has_not_found(&self) -> bool {
    self.not_found.is_some()
  }

  pub fn add(mut self, pattern: impl Into<String>, plan: impl IntoPlan) -> Self {
    match plan.into_plan() {
      Ok(plan) => self.entries.push((pattern.into(), plan, Owner::Rust)),
      Err(e) => {
        self.failed.get_or_insert(e);
      }
    }
    self
  }

  pub fn replace(mut self, pattern: impl Into<String>, plan: impl IntoPlan) -> Self {
    let pattern = pattern.into();
    match plan.into_plan() {
      Ok(plan) => {
        self.overrides.push(pattern.clone());
        self.entries.push((pattern, plan, Owner::RustOverride));
      }
      Err(e) => {
        self.failed.get_or_insert(e);
      }
    }
    self
  }

  pub fn patterns(&self) -> Vec<&str> {
    self.entries.iter().map(|(p, _, _)| p.as_str()).collect()
  }

  pub(crate) fn plans(&self) -> impl Iterator<Item = &PlanNode> {
    self.entries.iter().map(|(_, plan, _)| plan).chain(self.not_found.iter()).chain(self.intercepts.iter().map(|(_, plan)| plan))
  }

  pub(crate) fn take_not_found(&mut self) -> Option<PlanNode> {
    self.not_found.take()
  }

  pub(crate) fn take_intercepts(&mut self) -> Vec<(String, PlanNode)> {
    std::mem::take(&mut self.intercepts)
  }

  pub(crate) fn resolved(self) -> Result<Vec<(String, PlanNode, Owner)>, BindError> {
    if let Some(e) = self.failed {
      return Err(e);
    }
    let overrides = self.overrides;
    let mut kept: Vec<(String, PlanNode, Owner)> = Vec::new();
    for (pattern, plan, owner) in self.entries {
      match kept.iter().position(|(p, _, _)| *p == pattern) {
        Some(_) if !overrides.contains(&pattern) => return Err(BindError::Claimed(pattern)),
        Some(at) => kept[at] = (pattern, plan, Owner::RustOverride),
        None => kept.push((pattern, plan, owner)),
      }
    }
    Ok(kept)
  }

  pub fn build(self) -> Result<(MatchitMatcher, TableResolver), BindError> {
    let mut matcher = MatchitMatcher::new();
    let mut resolver = TableResolver::new();
    for (index, (pattern, plan, _)) in self.resolved()?.into_iter().enumerate() {
      let entry = EntryId(index as u32);
      matcher
        .insert(&pattern, entry)
        .map_err(|e| BindError::Pattern { pattern: pattern.clone(), message: e.to_string() })?;
      resolver.insert(entry, plan);
    }
    Ok((matcher, resolver))
  }
}
