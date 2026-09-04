//! Binds a plan file to the implementations that answer it. An application
//! supplies as much or as little as it wants: nothing, and the file decides
//! everything; or a registration per name it wants to own in Rust.

pub mod plan;
pub mod routes;

use std::future::Future;
use std::collections::HashMap;
use std::sync::Arc;

use snapfire_fsr_core::{Data, ModuleId, PlanNode};
use snapfire_fsr_ir::{Component, IrAction, IrEvaluator, IrSource};
use snapfire_fsr_runtime::{
  ActionError, ActionHandler, ActionRegistry, DataSource, DataSources, Evaluator, Evaluators,
  HandlerMatch, HandlerMatcher, LoadError, MatchitMatcher, NodeCache, RequestCtx, Runtime, TableResolver,
};
use snapfire_fsr_service::{Contract, Services, Type};

pub use plan::{IntoPlan, Plan};
pub use routes::Routes;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BindError {
  #[error(transparent)]
  Plan(#[from] snapfire_fsr_plan::PlanError),
  #[error("`{0}` is claimed by the plan file and by Rust; mark the Rust one as an override")]
  Claimed(String),
  #[error("action `{0}` is lowered by the plan file and bound in Rust; mark the Rust one as an override")]
  ActionClaimed(String),
  #[error("action `{id}` is marked an override but the plan lowers no such action")]
  ActionOverridesNothing { id: String },
  #[error("`{pattern}` is not a route pattern: {message}")]
  Pattern { pattern: String, message: String },
  #[error("the plan names data source `{name}`, which nothing answers")]
  Unbound { name: String },
  #[error("`{name}` is marked an override but the plan names no such data source")]
  OverridesNothing { name: String },
  #[error("`{module}` is not a module id, which is `path#export`")]
  Module { module: String },
  #[error("the plan declares action `{id}`, which nothing answers")]
  UnboundAction { id: String },
  #[error("action `{id}` names input type `{input}` but the host holds no contract; pass one with `contract`")]
  NoContract { id: String, input: String },
  #[error("action `{id}` names input type `{input}`, which the contract does not define")]
  UnknownInput { id: String, input: String },
  #[error("handler `{0}` is lowered by the plan file and bound in Rust; mark the Rust one as an override")]
  HandlerClaimed(String),
  #[error("handler `{0}` is marked an override but the plan lowers no such handler")]
  HandlerOverridesNothing(String),
  #[error("the plan declares handler `{0}`, which nothing answers")]
  UnboundHandler(String),
  #[error("the middleware is lowered by the plan file and bound in Rust; mark the Rust one as an override")]
  MiddlewareClaimed,
  #[error("the middleware is marked an override but the plan lowers none")]
  MiddlewareOverridesNothing,
}

/// Who answers a name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owner {
  PlanFile,
  Lowered,
  Rust,
  RustOverride,
}

impl Owner {
  pub fn as_str(&self) -> &'static str {
    match self {
      Self::PlanFile => "plan file",
      Self::Lowered => "lowered",
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
  pub actions: Vec<(String, Owner)>,
  /// `METHOD pattern` per handler.
  pub handlers: Vec<(String, Owner)>,
  pub middleware: Option<Owner>,
  /// Patterns one render serves for every request; the host prerenders these.
  pub prerenderable: Vec<String>,
  /// Modules rendered on the server, by the lowered tree or by Rust.
  pub components: Vec<(String, Owner)>,
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
    for (i, (action, owner)) in self.actions.iter().enumerate() {
      let label = if i == 0 { "actions" } else { "" };
      writeln!(f, "{label:<9} {action:<22} {}", owner.as_str())?;
    }
    for (i, (handler, owner)) in self.handlers.iter().enumerate() {
      let label = if i == 0 { "handlers" } else { "" };
      writeln!(f, "{label:<9} {handler:<22} {}", owner.as_str())?;
    }
    if let Some(owner) = &self.middleware {
      writeln!(f, "{:<9} {:<22} {}", "middleware", "middleware", owner.as_str())?;
    }
    for (i, (module, owner)) in self.components.iter().enumerate() {
      let label = if i == 0 { "rendered" } else { "" };
      writeln!(f, "{label:<9} {module:<22} {}", owner.as_str())?;
    }
    Ok(())
  }
}

/// Everything a request needs, plus what the host bound to produce it.
pub struct App {
  pub matcher: MatchitMatcher,
  pub resolver: TableResolver,
  /// Route handlers: a method and a pattern answered with a value.
  pub handlers: Handlers,
  /// Runs before every request that is not a static file, with the request
  /// line as its input.
  pub middleware: Option<Arc<dyn ActionHandler>>,
  /// Rendered with status 404 for a path the matcher does not match.
  pub not_found: Option<PlanNode>,
  /// Patterns with no parameter whose every source is lowered and reads
  /// nothing of the request, so one render serves every request.
  pub prerenderable: Vec<String>,
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

/// The handlers a host dispatches: one router per method resolving to an id,
/// and the handler behind each id.
#[derive(Default)]
pub struct Handlers {
  matcher: HandlerMatcher,
  registry: ActionRegistry,
}

impl Handlers {
  pub fn match_request(&self, method: &str, path: &str) -> Option<HandlerMatch> {
    self.matcher.match_request(method, path)
  }

  pub fn dispatch(&self, id: &str, ctx: RequestCtx, input: snapfire_fsr_core::Value) -> futures_util::future::BoxFuture<'static, Result<snapfire_fsr_core::Value, ActionError>> {
    self.registry.dispatch(id, ctx, input)
  }

  pub fn ids(&self) -> Vec<String> {
    self.registry.ids()
  }

  pub fn is_empty(&self) -> bool {
    self.matcher.is_empty()
  }
}

struct FnHandler<F>(F);

impl<F, Fut> ActionHandler for FnHandler<F>
where
  F: Fn(RequestCtx, snapfire_fsr_core::Value) -> Fut + Send + Sync,
  Fut: std::future::Future<Output = Result<snapfire_fsr_core::Value, ActionError>> + Send + 'static,
{
  fn call(&self, ctx: RequestCtx, input: snapfire_fsr_core::Value) -> futures_util::future::BoxFuture<'static, Result<snapfire_fsr_core::Value, ActionError>> {
    Box::pin((self.0)(ctx, input))
  }
}

/// `METHOD pattern`, the name a handler is reported and overridden by.
fn handler_key(method: &str, pattern: &str) -> String {
  format!("{} {pattern}", method.to_ascii_uppercase())
}

pub struct AppBuilder {
  routes: Routes,
  lowered_middleware: Option<snapfire_fsr_ir::Body>,
  rust_middleware: Option<(Arc<dyn ActionHandler>, Owner)>,
  declared_handlers: Vec<(String, String, String)>,
  lowered_handlers: Vec<(String, String, String, Option<String>, snapfire_fsr_ir::Body)>,
  rust_handlers: Vec<(String, String, Arc<dyn ActionHandler>, Owner)>,
  declared_actions: Vec<String>,
  lowered_sources: Vec<(String, snapfire_fsr_ir::Body)>,
  lowered_actions: Vec<(String, Option<String>, snapfire_fsr_ir::Body)>,
  lowered_components: Vec<(String, Component)>,
  contract: Option<Arc<Contract>>,
  sources: DataSources,
  claimed: Vec<(String, Owner)>,
  overrides: Vec<String>,
  evaluators: Evaluators,
  actions: ActionRegistry,
  action_claims: Vec<(String, Owner)>,
  action_overrides: Vec<String>,
  services: Option<Arc<Services>>,
  cache: Option<Arc<dyn NodeCache>>,
}

impl App {
  pub fn builder(routes: Routes) -> AppBuilder {
    AppBuilder {
      routes,
      lowered_middleware: None,
      rust_middleware: None,
      declared_handlers: Vec::new(),
      lowered_handlers: Vec::new(),
      rust_handlers: Vec::new(),
      declared_actions: Vec::new(),
      lowered_sources: Vec::new(),
      lowered_actions: Vec::new(),
      lowered_components: Vec::new(),
      contract: None,
      sources: DataSources::new(),
      claimed: Vec::new(),
      overrides: Vec::new(),
      evaluators: Evaluators::new(),
      actions: ActionRegistry::new(),
      action_claims: Vec::new(),
      action_overrides: Vec::new(),
      services: None,
      cache: None,
    }
  }

  /// The stock entry point: a plan file and nothing else. Every lowered row
  /// in the file is bound here as a default; Rust takes a name back with
  /// `source_override` or `action_override`.
  pub fn from_manifest(manifest: &str) -> Result<AppBuilder, BindError> {
    let parsed = snapfire_fsr_plan::Manifest::from_json(manifest)?;
    let mut builder = Self::builder(Routes::from_manifest(manifest)?);
    builder.declared_actions = parsed.action_ids();
    builder.lowered_sources = parsed
      .lowered_sources()
      .filter_map(|row| row.body.clone().map(|body| (row.id.clone(), body)))
      .collect();
    builder.lowered_actions = parsed
      .lowered_actions()
      .filter_map(|row| row.body.clone().map(|body| (row.id.clone(), row.input.clone(), body)))
      .collect();
    builder.lowered_components = parsed.components.iter().map(|row| (row.module.clone(), row.body.clone())).collect();
    builder.lowered_middleware = parsed.middleware.clone();
    for row in &parsed.handlers {
      match &row.body {
        Some(body) => builder.lowered_handlers.push((row.id.clone(), row.method.clone(), row.pattern.clone(), row.input.clone(), body.clone())),
        None => builder.declared_handlers.push((row.id.clone(), row.method.clone(), row.pattern.clone())),
      }
    }
    Ok(builder)
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
    let id = id.into();
    self.action_claims.push((id.clone(), Owner::Rust));
    self.actions.insert_fn(id, f);
    self
  }

  /// Takes an action the plan file lowered. Overriding one it did not lower
  /// is an error, since it means a rename left it dangling.
  pub fn action_override<F, Fut>(mut self, id: impl Into<String>, f: F) -> Self
  where
    F: Fn(RequestCtx, snapfire_fsr_core::Value) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<snapfire_fsr_core::Value, ActionError>> + Send + 'static,
  {
    let id = id.into();
    self.action_overrides.push(id.clone());
    self.action_claims.push((id.clone(), Owner::RustOverride));
    self.actions.insert_fn(id, f);
    self
  }

  pub fn action_impl(mut self, id: impl Into<String>, handler: Arc<dyn ActionHandler>) -> Self {
    let id = id.into();
    self.action_claims.push((id.clone(), Owner::Rust));
    self.actions.insert(id, handler);
    self
  }

  pub fn services(mut self, services: Arc<Services>) -> Self {
    self.services = Some(services);
    self
  }

  /// The contract a lowered action's input is checked against before its
  /// body runs. Required when any lowered action names an input type.
  pub fn contract(mut self, contract: Contract) -> Self {
    self.contract = Some(Arc::new(contract));
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

  /// The tree rendered, with status 404, for a path no route matches,
  /// replacing the plan file's when it has one.
  pub fn not_found(mut self, plan: impl IntoPlan) -> Self {
    self.routes = std::mem::take(&mut self.routes).not_found(plan);
    self
  }

  /// A handler written in Rust: `method` and `pattern` are what the host
  /// matches. Refused at `build` when the plan lowers the same pair.
  pub fn handler<F, Fut>(self, method: impl Into<String>, pattern: impl Into<String>, f: F) -> Self
  where
    F: Fn(RequestCtx, snapfire_fsr_core::Value) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = Result<snapfire_fsr_core::Value, ActionError>> + Send + 'static,
  {
    self.handler_impl(method, pattern, Arc::new(FnHandler(f)))
  }

  /// A Rust handler replacing a lowered one for the same method and pattern.
  pub fn handler_override<F, Fut>(mut self, method: impl Into<String>, pattern: impl Into<String>, f: F) -> Self
  where
    F: Fn(RequestCtx, snapfire_fsr_core::Value) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = Result<snapfire_fsr_core::Value, ActionError>> + Send + 'static,
  {
    self.rust_handlers.push((method.into().to_ascii_uppercase(), pattern.into(), Arc::new(FnHandler(f)), Owner::RustOverride));
    self
  }

  /// Middleware written in Rust, called with the request line as its input.
  /// Refused at `build` when the plan lowers one.
  pub fn middleware<F, Fut>(mut self, f: F) -> Self
  where
    F: Fn(RequestCtx, snapfire_fsr_core::Value) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = Result<snapfire_fsr_core::Value, ActionError>> + Send + 'static,
  {
    self.rust_middleware = Some((Arc::new(FnHandler(f)), Owner::Rust));
    self
  }

  pub fn middleware_override<F, Fut>(mut self, f: F) -> Self
  where
    F: Fn(RequestCtx, snapfire_fsr_core::Value) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = Result<snapfire_fsr_core::Value, ActionError>> + Send + 'static,
  {
    self.rust_middleware = Some((Arc::new(FnHandler(f)), Owner::RustOverride));
    self
  }

  pub fn handler_impl(mut self, method: impl Into<String>, pattern: impl Into<String>, handler: Arc<dyn ActionHandler>) -> Self {
    self.rust_handlers.push((method.into().to_ascii_uppercase(), pattern.into(), handler, Owner::Rust));
    self
  }

  pub fn route_override(mut self, pattern: impl Into<String>, plan: impl IntoPlan) -> Self {
    self.routes = self.routes.replace(pattern, plan);
    self
  }

  /// Refuses rather than serving a plan nothing can answer: every data source
  /// the plan names must be bound, and every override must name something.
  pub fn build(mut self) -> Result<App, BindError> {
    let declared: Vec<String> = self.routes.plans().flat_map(declared_sources).collect();

    for name in &self.overrides {
      if !declared.contains(name) {
        return Err(BindError::OverridesNothing { name: name.clone() });
      }
    }

    let fixed_sources: Vec<String> = self.lowered_sources.iter().filter(|(_, body)| !snapfire_fsr_ir::body_reads_request(body)).map(|(name, _)| name.clone()).collect();
    let reads: HashMap<String, Vec<String>> = self.lowered_sources.iter().map(|(name, body)| (name.clone(), snapfire_fsr_ir::body_params_read(body))).collect();
    for (name, body) in std::mem::take(&mut self.lowered_sources) {
      match self.claimed.iter().rev().find(|(claimed, _)| *claimed == name).map(|(_, o)| *o) {
        Some(Owner::RustOverride) => {}
        Some(_) => return Err(BindError::Claimed(name)),
        None => {
          self.claimed.push((name.clone(), Owner::Lowered));
          self.sources.insert(name.clone(), Arc::new(IrSource::new(name, body)));
        }
      }
    }

    for id in &self.action_overrides {
      if !self.lowered_actions.iter().any(|(lowered, _, _)| lowered == id) {
        return Err(BindError::ActionOverridesNothing { id: id.clone() });
      }
    }
    for (id, input, body) in std::mem::take(&mut self.lowered_actions) {
      match self.action_claims.iter().rev().find(|(claimed, _)| *claimed == id).map(|(_, o)| *o) {
        Some(Owner::RustOverride) => {}
        Some(_) => return Err(BindError::ActionClaimed(id)),
        None => {
          let handler: Arc<dyn ActionHandler> = match input {
            None => Arc::new(IrAction::new(body)),
            Some(input) => {
              let contract = self.contract.clone().ok_or_else(|| BindError::NoContract { id: id.clone(), input: input.clone() })?;
              if !contract.types.contains_key(&input) {
                return Err(BindError::UnknownInput { id: id.clone(), input });
              }
              Arc::new(CheckedInput { input, contract, inner: IrAction::new(body) })
            }
          };
          self.action_claims.push((id.clone(), Owner::Lowered));
          self.actions.insert(id, handler);
        }
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

    let bound_actions = self.actions.ids();
    for id in &self.declared_actions {
      if !bound_actions.contains(id) {
        return Err(BindError::UnboundAction { id: id.clone() });
      }
    }

    let (middleware, middleware_owner): (Option<Arc<dyn ActionHandler>>, Option<Owner>) = match (self.rust_middleware.take(), self.lowered_middleware.take()) {
      (Some((_, Owner::Rust)), Some(_)) => return Err(BindError::MiddlewareClaimed),
      (Some((_, Owner::RustOverride)), None) => return Err(BindError::MiddlewareOverridesNothing),
      (Some((handler, owner)), _) => (Some(handler), Some(owner)),
      (None, Some(body)) => (Some(Arc::new(IrAction::new(body))), Some(Owner::Lowered)),
      (None, None) => (None, None),
    };

    let mut handlers = Handlers::default();
    let mut handler_rows: Vec<(String, Owner)> = Vec::new();
    let mut claimed_handlers: Vec<(String, Owner)> = Vec::new();
    for (method, pattern, handler, owner) in &self.rust_handlers {
      let key = handler_key(method, pattern);
      if *owner == Owner::RustOverride && !self.lowered_handlers.iter().any(|(_, m, p, _, _)| handler_key(m, p) == key) {
        return Err(BindError::HandlerOverridesNothing(key));
      }
      handlers.matcher.insert(method, pattern, key.clone()).map_err(|e| BindError::Pattern { pattern: pattern.clone(), message: e.to_string() })?;
      handlers.registry.insert(key.clone(), handler.clone());
      claimed_handlers.push((key.clone(), *owner));
      handler_rows.push((key, *owner));
    }
    for (id, method, pattern, input, body) in std::mem::take(&mut self.lowered_handlers) {
      let key = handler_key(&method, &pattern);
      match claimed_handlers.iter().find(|(k, _)| *k == key).map(|(_, o)| *o) {
        Some(Owner::RustOverride) => continue,
        Some(_) => return Err(BindError::HandlerClaimed(key)),
        None => {}
      }
      let handler: Arc<dyn ActionHandler> = match input {
        None => Arc::new(IrAction::new(body)),
        Some(input) => {
          let contract = self.contract.clone().ok_or_else(|| BindError::NoContract { id: id.clone(), input: input.clone() })?;
          if !contract.types.contains_key(&input) {
            return Err(BindError::UnknownInput { id: id.clone(), input });
          }
          Arc::new(CheckedInput { input, contract, inner: IrAction::new(body) })
        }
      };
      handlers.matcher.insert(&method, &pattern, id.clone()).map_err(|e| BindError::Pattern { pattern: pattern.clone(), message: e.to_string() })?;
      handlers.registry.insert(id, handler);
      handler_rows.push((key, Owner::Lowered));
    }
    for (_, method, pattern) in &self.declared_handlers {
      let key = handler_key(method, pattern);
      if !claimed_handlers.iter().any(|(k, _)| *k == key) {
        return Err(BindError::UnboundHandler(key));
      }
    }
    handler_rows.sort_by(|a, b| a.0.cmp(&b.0));

    let not_found = self.routes.take_not_found();
    let resolved = self.routes.resolved()?;
    let prerenderable: Vec<String> = resolved
      .iter()
      .filter(|(pattern, plan, _)| {
        !pattern.contains('{')
          && declared_sources(plan).iter().all(|name| {
            fixed_sources.contains(name) && matches!(self.claimed.iter().rev().find(|(claimed, _)| claimed == name).map(|(_, o)| *o), Some(Owner::Lowered))
          })
      })
      .map(|(pattern, _, _)| pattern.clone())
      .collect();
    let actions = self
      .actions
      .ids()
      .into_iter()
      .map(|id| {
        let owner = self
          .action_claims
          .iter()
          .rev()
          .find(|(claimed, _)| *claimed == id)
          .map(|(_, o)| *o)
          .unwrap_or(Owner::Rust);
        (id, owner)
      })
      .collect();
    let mut components = Vec::new();
    if !self.lowered_components.is_empty() {
      let evaluator = IrEvaluator::new(std::mem::take(&mut self.lowered_components));
      components = evaluator.modules().into_iter().map(|m| (m, Owner::Lowered)).collect();
      let evaluator = Arc::new(evaluator);
      let covers = evaluator.clone();
      self.evaluators.register(move |m: &ModuleId| covers.covers(m), evaluator);
    }

    let mut report = Report {
      routes: resolved.iter().map(|(p, _, owner)| (p.clone(), *owner)).collect(),
      sources,
      actions,
      handlers: handler_rows,
      middleware: middleware_owner,
      prerenderable: prerenderable.clone(),
      components,
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

    let mut runtime = Runtime::builder().sources(self.sources).evaluators(self.evaluators).keyer(Arc::new(ReadsKeyer { reads }));
    if let Some(cache) = self.cache {
      runtime = runtime.cache(cache);
    }

    Ok(App {
      matcher,
      resolver,
      handlers,
      middleware,
      not_found,
      prerenderable,
      runtime: runtime.build(),
      services: self.services.unwrap_or_else(|| Services::builder().build()),
      actions: self.actions,
      report,
    })
  }
}

/// A lowered action whose input is checked against its declared type before
/// the body runs, so the body only ever sees a value of that shape.
struct CheckedInput {
  input: String,
  contract: Arc<Contract>,
  inner: IrAction,
}

impl ActionHandler for CheckedInput {
  fn call(&self, ctx: RequestCtx, input: snapfire_fsr_core::Value) -> futures_util::future::BoxFuture<'static, Result<snapfire_fsr_core::Value, ActionError>> {
    if let Err(e) = self.contract.check_value(&Type::Named(self.input.clone()), &input, "input") {
      let error = ActionError::new(snapfire_fsr_runtime::FailureKind::Invalid, e.to_string());
      return Box::pin(async move { Err(error) });
    }
    self.inner.call(ctx, input)
  }
}

/// Keys a node with children by its module and the route parameters its
/// source reads, so a layout survives a navigation that changes a parameter
/// it never looks at; a leaf keeps `DefaultKeyer`'s full key.
struct ReadsKeyer {
  reads: HashMap<String, Vec<String>>,
}

impl snapfire_fsr_runtime::SegmentKeyer for ReadsKeyer {
  fn key(&self, plan: &PlanNode, params: &snapfire_fsr_core::Params, query: &snapfire_fsr_core::Params) -> String {
    if plan.children.is_empty() {
      return snapfire_fsr_runtime::DefaultKeyer.key(plan, params, query);
    }
    let mut key = plan.module.to_string();
    let read = plan.data_source.as_ref().and_then(|s| self.reads.get(&s.0));
    let mut pairs: Vec<String> = params
      .iter()
      .filter(|(k, _)| read.is_some_and(|names| names.contains(k)))
      .map(|(k, v)| format!("{k}={v}"))
      .collect();
    pairs.sort_unstable();
    if !pairs.is_empty() {
      key.push('?');
      key.push_str(&pairs.join("&"));
    }
    key
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
