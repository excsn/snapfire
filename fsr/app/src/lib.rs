//! Binds a plan file to the implementations that answer it. An application
//! supplies as much or as little as it wants: nothing, and the file decides
//! everything; or a registration per name it wants to own in Rust.

pub mod plan;
pub mod routes;

use std::future::Future;
use std::sync::Arc;

use snapfire_fsr_core::{Data, ModuleId};
use snapfire_fsr_runtime::{
  ActionError, ActionHandler, ActionRegistry, DataSource, DataSources, Evaluator, Evaluators,
  LoadError, MatchitMatcher, NodeCache, RequestCtx, Runtime, TableResolver,
};
use snapfire_fsr_service::Services;

pub use plan::{IntoPlan, Plan};
pub use routes::Routes;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BindError {
  #[error(transparent)]
  Plan(#[from] snapfire_fsr_plan::PlanError),
  #[error("`{0}` is claimed by the plan file and by Rust; mark the Rust one as an override")]
  Claimed(String),
  #[error("`{pattern}` is not a route pattern: {message}")]
  Pattern { pattern: String, message: String },
  #[error("the plan names data source `{name}`, which nothing answers")]
  Unbound { name: String },
  #[error("`{name}` is marked an override but the plan names no such data source")]
  OverridesNothing { name: String },
  #[error("`{module}` is not a module id, which is `path#export`")]
  Module { module: String },
}

/// Who answers a name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owner {
  PlanFile,
  Rust,
  RustOverride,
}

impl Owner {
  pub fn as_str(&self) -> &'static str {
    match self {
      Self::PlanFile => "plan file",
      Self::Rust => "rust",
      Self::RustOverride => "rust override",
    }
  }
}

/// What the host bound, in the order a reader wants it. Printed at boot so a
/// binding system does not become a guessing game.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
  pub routes: Vec<(String, Owner)>,
  pub sources: Vec<(String, Owner)>,
  pub actions: Vec<String>,
}

impl std::fmt::Display for Report {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    for (i, (pattern, owner)) in self.routes.iter().enumerate() {
      let label = if i == 0 { "routes" } else { "" };
      writeln!(f, "{label:<9} {pattern:<22} {}", owner.as_str())?;
    }
    for (i, (source, owner)) in self.sources.iter().enumerate() {
      let label = if i == 0 { "sources" } else { "" };
      writeln!(f, "{label:<9} {source:<22} {}", owner.as_str())?;
    }
    for (i, action) in self.actions.iter().enumerate() {
      let label = if i == 0 { "actions" } else { "" };
      writeln!(f, "{label:<9} {action:<22} rust")?;
    }
    Ok(())
  }
}

/// Everything a request needs, plus what the host bound to produce it.
pub struct App {
  pub matcher: MatchitMatcher,
  pub resolver: TableResolver,
  pub runtime: Arc<Runtime>,
  pub services: Arc<Services>,
  pub actions: ActionRegistry,
  pub report: Report,
}

impl std::fmt::Debug for App {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("App")
      .field("routes", &self.report.routes.len())
      .field("sources", &self.report.sources.len())
      .field("actions", &self.report.actions.len())
      .finish()
  }
}

pub struct AppBuilder {
  routes: Routes,
  sources: DataSources,
  claimed: Vec<(String, Owner)>,
  overrides: Vec<String>,
  evaluators: Evaluators,
  actions: ActionRegistry,
  services: Option<Arc<Services>>,
  cache: Option<Arc<dyn NodeCache>>,
}

impl App {
  pub fn builder(routes: Routes) -> AppBuilder {
    AppBuilder {
      routes,
      sources: DataSources::new(),
      claimed: Vec::new(),
      overrides: Vec::new(),
      evaluators: Evaluators::new(),
      actions: ActionRegistry::new(),
      services: None,
      cache: None,
    }
  }

  /// The stock entry point: a plan file and nothing else.
  pub fn from_manifest(manifest: &str) -> Result<AppBuilder, BindError> {
    Ok(Self::builder(Routes::from_manifest(manifest)?))
  }
}

impl AppBuilder {
  pub fn source<F, Fut>(mut self, name: impl Into<String>, f: F) -> Self
  where
    F: Fn(RequestCtx) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Data, LoadError>> + Send + 'static,
  {
    let name = name.into();
    self.claimed.push((name.clone(), Owner::Rust));
    self.sources.insert_fn(name, f);
    self
  }

  /// Takes a name the plan file already declares. Overriding a name the plan
  /// does not declare is an error, since it means a rename left it dangling.
  pub fn source_override<F, Fut>(mut self, name: impl Into<String>, f: F) -> Self
  where
    F: Fn(RequestCtx) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Data, LoadError>> + Send + 'static,
  {
    let name = name.into();
    self.overrides.push(name.clone());
    self.claimed.push((name.clone(), Owner::RustOverride));
    self.sources.insert_fn(name, f);
    self
  }

  pub fn source_impl(mut self, name: impl Into<String>, source: Arc<dyn DataSource>) -> Self {
    let name = name.into();
    self.claimed.push((name.clone(), Owner::Rust));
    self.sources.insert(name, source);
    self
  }

  pub fn evaluator<P>(mut self, predicate: P, evaluator: Arc<dyn Evaluator>) -> Self
  where
    P: Fn(&ModuleId) -> bool + Send + Sync + 'static,
  {
    self.evaluators.register(predicate, evaluator);
    self
  }

  pub fn action<F, Fut>(mut self, id: impl Into<String>, f: F) -> Self
  where
    F: Fn(RequestCtx, snapfire_fsr_core::Value) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<snapfire_fsr_core::Value, ActionError>> + Send + 'static,
  {
    self.actions.insert_fn(id, f);
    self
  }

  pub fn action_impl(mut self, id: impl Into<String>, handler: Arc<dyn ActionHandler>) -> Self {
    self.actions.insert(id, handler);
    self
  }

  pub fn services(mut self, services: Arc<Services>) -> Self {
    self.services = Some(services);
    self
  }

  pub fn cache(mut self, cache: Arc<dyn NodeCache>) -> Self {
    self.cache = Some(cache);
    self
  }

  pub fn route(mut self, pattern: impl Into<String>, plan: impl IntoPlan) -> Self {
    self.routes = self.routes.add(pattern, plan);
    self
  }

  pub fn route_override(mut self, pattern: impl Into<String>, plan: impl IntoPlan) -> Self {
    self.routes = self.routes.replace(pattern, plan);
    self
  }

  /// Refuses rather than serving a plan nothing can answer: every data source
  /// the plan names must be bound, and every override must name something.
  pub fn build(self) -> Result<App, BindError> {
    let declared: Vec<String> = self.routes.plans().flat_map(declared_sources).collect();

    for name in &self.overrides {
      if !declared.contains(name) {
        return Err(BindError::OverridesNothing { name: name.clone() });
      }
    }

    let mut sources = Vec::new();
    for name in &declared {
      if sources.iter().any(|(s, _): &(String, Owner)| s == name) {
        continue;
      }
      // The last claim wins, matching the registry it writes into.
      let owner = self
        .claimed
        .iter()
        .rev()
        .find(|(claimed, _)| claimed == name)
        .map(|(_, owner)| *owner);
      match owner {
        Some(owner) => sources.push((name.clone(), owner)),
        None => return Err(BindError::Unbound { name: name.clone() }),
      }
    }

    let resolved = self.routes.resolved()?;
    let mut report = Report {
      routes: resolved.iter().map(|(p, _, owner)| (p.clone(), *owner)).collect(),
      sources,
      actions: self.actions.ids(),
    };
    report.routes.sort_by(|a, b| a.0.cmp(&b.0));

    let mut matcher = MatchitMatcher::new();
    let mut resolver = TableResolver::new();
    for (index, (pattern, plan, _)) in resolved.into_iter().enumerate() {
      let entry = snapfire_fsr_runtime::EntryId(index as u32);
      matcher
        .insert(&pattern, entry)
        .map_err(|e| BindError::Pattern { pattern: pattern.clone(), message: e.to_string() })?;
      resolver.insert(entry, plan);
    }

    let mut runtime = Runtime::builder().sources(self.sources).evaluators(self.evaluators);
    if let Some(cache) = self.cache {
      runtime = runtime.cache(cache);
    }

    Ok(App {
      matcher,
      resolver,
      runtime: runtime.build(),
      services: self.services.unwrap_or_else(|| Services::builder().build()),
      actions: self.actions,
      report,
    })
  }
}

fn declared_sources(plan: &snapfire_fsr_core::PlanNode) -> Vec<String> {
  let mut out = Vec::new();
  fn walk(plan: &snapfire_fsr_core::PlanNode, out: &mut Vec<String>) {
    if let Some(source) = &plan.data_source {
      out.push(source.0.clone());
    }
    for (_, child) in &plan.children {
      walk(child, out);
    }
  }
  walk(plan, &mut out);
  out
}
