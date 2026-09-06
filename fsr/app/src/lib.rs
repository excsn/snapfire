//! Binds a plan file to the implementations that answer it. An application
//! supplies as much or as little as it wants: nothing, and the file decides
//! everything; or a registration per name it wants to own in Rust.

pub mod plan;
pub mod routes;

use std::future::Future;
use std::collections::HashMap;
use std::sync::Arc;

use snapfire_fsr_core::{Data, ModuleId, Params, PlanNode};
use snapfire_fsr_ir::{body_visit, Component, Expr, Extensions, Interpreter, IrAction, IrEvaluator, IrMeta, IrSource, IrStore, Reach};
use snapfire_fsr_runtime::{
  ActionError, ActionHandler, ActionRegistry, DataSource, DataSources, Evaluator, Evaluators,
  HandlerMatch, HandlerMatcher, LoadError, Matcher, MatchitMatcher, Metadata, NodeCache, RequestCtx, Resolver, Runtime, TableResolver,
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
  #[error("`{owner}` calls extension `{name}`, which nothing registers; the standard library has no such member and no `extension(\"{name}\", ..)` was bound")]
  UnknownExtension { owner: String, name: String },
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
  /// Patterns one render serves for every anonymous request: their only
  /// request reads are the identity and calls that carry its token, so the
  /// host prerenders them and serves the file to visitors with no identity.
  pub prerenderable_anonymous: Vec<String>,
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
  /// The trees a soft navigation renders into a live layout's slot, by the
  /// pattern of the route they belong to.
  pub intercepts: Intercepts,
  /// The lowered components and their interpreter, for an island in server
  /// mode to step and render again; `None` when nothing was lowered.
  pub lowered: Option<Arc<IrEvaluator>>,
  /// Patterns with no parameter whose every source is lowered and reads
  /// nothing of the request, so one render serves every request.
  pub prerenderable: Vec<String>,
  /// Patterns whose only request reads are the identity, a page or layout's
  /// `identity` prop or a call through a client that carries the session's
  /// token: one anonymous render serves every anonymous request and a
  /// signed-in visitor is rendered live.
  pub prerenderable_anonymous: Vec<String>,
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

/// The intercept plans from the plan file: one per `page.<slot>.tsx`, matched
/// on the route's pattern, in file order under it.
#[derive(Default)]
pub struct Intercepts {
  matcher: MatchitMatcher,
  plans: HashMap<snapfire_fsr_runtime::EntryId, Vec<PlanNode>>,
}

impl Intercepts {
  /// Every intercept of the route `path` matches, with the matched params.
  pub fn plans_for(&self, path: &str) -> Option<(Vec<PlanNode>, Params)> {
    let matched = self.matcher.match_path(path)?;
    let plans = self.plans.get(&matched.entry)?.clone();
    Some((plans, matched.params))
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
  /// By source id: the loader module's `meta` body.
  lowered_metas: Vec<(String, snapfire_fsr_ir::Body)>,
  /// By source id: metadata a Rust host describes a segment with.
  rust_metas: Vec<(String, Arc<dyn Metadata>)>,
  lowered_stores: Vec<(String, snapfire_fsr_ir::Body)>,
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
  extensions: Extensions,
  catalogs: Option<Arc<snapfire_fsr_ir::Catalogs>>,
  bearer_services: Vec<String>,
}

impl App {
  /// Drops every cached subtree under the plan `cache_key` and says how many went.
  pub async fn invalidate(&self, plan_key: &str) -> usize {
    self.runtime.cache.invalidate(plan_key).await
  }

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
      lowered_metas: Vec::new(),
      rust_metas: Vec::new(),
      lowered_stores: Vec::new(),
      lowered_actions: Vec::new(),
      lowered_components: Vec::new(),
      contract: None,
      sources: DataSources::new(),
      claimed: Vec::new(),
      overrides: Vec::new(),
      evaluators: Evaluators::new(),
      actions: ActionRegistry::new(),
      action_claims: Vec::new(),
      extensions: Extensions::standard(),
      catalogs: None,
      bearer_services: Vec::new(),
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
    builder.lowered_metas = parsed
      .lowered_sources()
      .filter_map(|row| row.meta.clone().map(|meta| (row.id.clone(), meta)))
      .collect();
    builder.lowered_stores = parsed
      .lowered_sources()
      .filter_map(|row| row.store.clone().map(|store| (row.id.clone(), store)))
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
  /// Adds every lowered row of another plan file beside this builder's: a
  /// mounted site's routes, sources, actions, components and handlers, its
  /// ids already prefixed so nothing collides. Its middleware and not-found
  /// tree are the caller's to place.
  pub fn mount_manifest(&mut self, manifest: &str) -> Result<(), BindError> {
    let parsed = snapfire_fsr_plan::Manifest::from_json(manifest)?;
    self.routes.extend_manifest(manifest)?;
    self.declared_actions.extend(parsed.action_ids());
    self.lowered_sources.extend(parsed.lowered_sources().filter_map(|row| row.body.clone().map(|body| (row.id.clone(), body))));
    self.lowered_metas.extend(parsed.lowered_sources().filter_map(|row| row.meta.clone().map(|meta| (row.id.clone(), meta))));
    self.lowered_stores.extend(parsed.lowered_sources().filter_map(|row| row.store.clone().map(|store| (row.id.clone(), store))));
    self.lowered_actions.extend(parsed.lowered_actions().filter_map(|row| row.body.clone().map(|body| (row.id.clone(), row.input.clone(), body))));
    self.lowered_components.extend(parsed.components.iter().map(|row| (row.module.clone(), row.body.clone())));
    for row in &parsed.handlers {
      match &row.body {
        Some(body) => self.lowered_handlers.push((row.id.clone(), row.method.clone(), row.pattern.clone(), row.input.clone(), body.clone())),
        None => self.declared_handlers.push((row.id.clone(), row.method.clone(), row.pattern.clone())),
      }
    }
    Ok(())
  }

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

  /// Registers the Rust half of a native pair under `name`, `module.member`,
  /// the name its `native(..)` declaration under `ext/` gives; `reach` is
  /// what that declaration says, `render` with a browser half and `body`
  /// without. Replaces a standard member of the same name.
  pub fn extension<F>(mut self, name: impl Into<String>, reach: Reach, f: F) -> Self
  where
    F: Fn(&snapfire_fsr_ir::Ambient, &[snapfire_fsr_core::Value]) -> Result<snapfire_fsr_core::Value, snapfire_fsr_ir::Fail> + Send + Sync + 'static,
  {
    self.extensions.register(name, reach, f);
    self
  }

  /// The extensions bound so far: the standard library and every `extension`.
  pub fn extensions(&self) -> &Extensions {
    &self.extensions
  }

  /// The message catalogs `t` reads, by locale; none by default.
  pub fn catalogs(mut self, catalogs: Arc<snapfire_fsr_ir::Catalogs>) -> Self {
    self.catalogs = Some(catalogs);
    self
  }

  /// The services whose calls carry the session's token, so a body calling
  /// one depends on the identity the way a body reading `identity` does.
  pub fn bearer_services(mut self, services: impl IntoIterator<Item = String>) -> Self {
    self.bearer_services = services.into_iter().collect();
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

  /// Describes the segment whose data source is `name` once its data has
  /// loaded: the title and description the document takes. The innermost
  /// described segment on a plan wins; a lowered `meta` under the same name
  /// is replaced.
  pub fn meta(mut self, name: impl Into<String>, meta: Arc<dyn Metadata>) -> Self {
    self.rust_metas.push((name.into(), meta));
    self
  }

  /// Refuses rather than serving a plan nothing can answer: every data source
  /// the plan names must be bound, and every override must name something.
  pub fn build(mut self) -> Result<App, BindError> {
    let declared: Vec<String> = self.routes.plans().flat_map(declared_sources).collect();

    let check = |owner: &str, body: &snapfire_fsr_ir::Body| -> Result<(), BindError> {
      match unknown_extension(body, &self.extensions) {
        Some(name) => Err(BindError::UnknownExtension { owner: owner.to_owned(), name }),
        None => Ok(()),
      }
    };
    for (name, body) in self.lowered_sources.iter().chain(&self.lowered_metas).chain(&self.lowered_stores) {
      check(name, body)?;
    }
    for (id, _, body) in &self.lowered_actions {
      check(id, body)?;
    }
    for (id, _, _, _, body) in &self.lowered_handlers {
      check(id, body)?;
    }
    if let Some(body) = &self.lowered_middleware {
      check("middleware", body)?;
    }
    for (module, component) in &self.lowered_components {
      let mut found = None;
      component.visit(&mut |e| {
        if let (None, Expr::Ext { module, name, .. }) = (&found, e) {
          let key = format!("{module}.{name}");
          if !self.extensions.contains(&key) {
            found = Some(key);
          }
        }
      });
      for handler in &component.handlers {
        if found.is_none() {
          found = unknown_extension(&handler.body, &self.extensions);
        }
      }
      if let Some(name) = found {
        return Err(BindError::UnknownExtension { owner: module.clone(), name });
      }
    }
    let interpreter = Interpreter::default().with_extensions(Arc::new(self.extensions.clone())).with_catalogs(self.catalogs.clone());

    for name in &self.overrides {
      if !declared.contains(name) {
        return Err(BindError::OverridesNothing { name: name.clone() });
      }
    }

    let statics: HashMap<String, Static> = self
      .lowered_sources
      .iter()
      .map(|(name, body)| {
        let meta = self.lowered_metas.iter().find(|(m, _)| m == name).map(|(_, meta)| meta);
        (name.clone(), classify(body, meta, &self.bearer_services))
      })
      .collect();
    let fixed_sources: Vec<String> = statics.iter().filter(|(_, class)| **class == Static::Fixed).map(|(name, _)| name.clone()).collect();
    let anonymous_sources: Vec<String> = statics.iter().filter(|(_, class)| **class != Static::Dynamic).map(|(name, _)| name.clone()).collect();
    let reads: HashMap<String, Vec<String>> = self.lowered_sources.iter().map(|(name, body)| (name.clone(), snapfire_fsr_ir::body_params_read(body))).collect();
    for (name, body) in std::mem::take(&mut self.lowered_sources) {
      match self.claimed.iter().rev().find(|(claimed, _)| *claimed == name).map(|(_, o)| *o) {
        Some(Owner::RustOverride) => {}
        Some(_) => return Err(BindError::Claimed(name)),
        None => {
          self.claimed.push((name.clone(), Owner::Lowered));
          self.sources.insert(name.clone(), Arc::new(IrSource::new(name, body).with_interpreter(interpreter.clone())));
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
            None => Arc::new(IrAction::new(body).with_interpreter(interpreter.clone())),
            Some(input) => {
              let contract = self.contract.clone().ok_or_else(|| BindError::NoContract { id: id.clone(), input: input.clone() })?;
              if !contract.types.contains_key(&input) {
                return Err(BindError::UnknownInput { id: id.clone(), input });
              }
              Arc::new(CheckedInput { input, contract, inner: IrAction::new(body).with_interpreter(interpreter.clone()) })
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
      (None, Some(body)) => (Some(Arc::new(IrAction::new(body).with_interpreter(interpreter.clone()))), Some(Owner::Lowered)),
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
        None => Arc::new(IrAction::new(body).with_interpreter(interpreter.clone())),
        Some(input) => {
          let contract = self.contract.clone().ok_or_else(|| BindError::NoContract { id: id.clone(), input: input.clone() })?;
          if !contract.types.contains_key(&input) {
            return Err(BindError::UnknownInput { id: id.clone(), input });
          }
          Arc::new(CheckedInput { input, contract, inner: IrAction::new(body).with_interpreter(interpreter.clone()) })
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
    let mut intercepts = Intercepts::default();
    let mut grouped: Vec<(String, Vec<PlanNode>)> = Vec::new();
    for (pattern, plan) in self.routes.take_intercepts() {
      match grouped.iter_mut().find(|(p, _)| *p == pattern) {
        Some((_, plans)) => plans.push(plan),
        None => grouped.push((pattern, vec![plan])),
      }
    }
    for (index, (pattern, plans)) in grouped.into_iter().enumerate() {
      let entry = snapfire_fsr_runtime::EntryId(index as u32);
      intercepts
        .matcher
        .insert(&pattern, entry)
        .map_err(|e| BindError::Pattern { pattern: pattern.clone(), message: e.to_string() })?;
      intercepts.plans.insert(entry, plans);
    }
    let resolved = self.routes.resolved()?;
    let lowered = |name: &String| matches!(self.claimed.iter().rev().find(|(claimed, _)| claimed == name).map(|(_, o)| *o), Some(Owner::Lowered));
    let prerenderable: Vec<String> = resolved
      .iter()
      .filter(|(pattern, plan, _)| {
        !pattern.contains('{')
          && declared_sources(plan).iter().all(|name| fixed_sources.contains(name) && lowered(name))
          && plan_reads_request_props(plan, &self.lowered_components) == Static::Fixed
      })
      .map(|(pattern, _, _)| pattern.clone())
      .collect();
    let prerenderable_anonymous: Vec<String> = resolved
      .iter()
      .filter(|(pattern, plan, _)| {
        !pattern.contains('{')
          && !prerenderable.contains(pattern)
          && declared_sources(plan).iter().all(|name| anonymous_sources.contains(name) && lowered(name))
          && plan_reads_request_props(plan, &self.lowered_components) != Static::Dynamic
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
    let mut lowered = None;
    if !self.lowered_components.is_empty() {
      let evaluator = IrEvaluator::new(std::mem::take(&mut self.lowered_components)).with_interpreter(interpreter.clone());
      components = evaluator.modules().into_iter().map(|m| (m, Owner::Lowered)).collect();
      let evaluator = Arc::new(evaluator);
      let covers = evaluator.clone();
      lowered = Some(evaluator.clone());
      self.evaluators.register(move |m: &ModuleId| covers.covers(m), evaluator);
    }

    let mut report = Report {
      routes: resolved.iter().map(|(p, _, owner)| (p.clone(), *owner)).collect(),
      sources,
      actions,
      handlers: handler_rows,
      middleware: middleware_owner,
      prerenderable: prerenderable.clone(),
      prerenderable_anonymous: prerenderable_anonymous.clone(),
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
    for (name, meta) in std::mem::take(&mut self.lowered_metas) {
      if self.claimed.iter().any(|(claimed, owner)| *claimed == name && *owner == Owner::Lowered) {
        runtime = runtime.meta(name.clone(), Arc::new(IrMeta::new(name, meta).with_interpreter(interpreter.clone())));
      }
    }
    for (name, meta) in std::mem::take(&mut self.rust_metas) {
      runtime = runtime.meta(name, meta);
    }
    for (name, store) in std::mem::take(&mut self.lowered_stores) {
      if self.claimed.iter().any(|(claimed, owner)| *claimed == name && *owner == Owner::Lowered) {
        runtime = runtime.store(name.clone(), Arc::new(IrStore::new(name, store).with_interpreter(interpreter.clone())));
      }
    }

    Ok(App {
      matcher,
      resolver,
      lowered,
      handlers,
      middleware,
      not_found,
      intercepts,
      prerenderable,
      prerenderable_anonymous,
      runtime: runtime.build(),
      services: self.services.unwrap_or_else(|| Services::builder().build()),
      actions: self.actions,
      report,
    })
  }
}

/// The first extension `body` calls that `extensions` does not hold.
fn unknown_extension(body: &snapfire_fsr_ir::Body, extensions: &Extensions) -> Option<String> {
  let mut found = None;
  body_visit(body, &mut |e| {
    if let (None, Expr::Ext { module, name, .. }) = (&found, e) {
      let key = format!("{module}.{name}");
      if !extensions.contains(&key) {
        found = Some(key);
      }
    }
  });
  found
}

/// A lowered middleware body as the handler the edge runs it through, for a
/// host that chains a mounted site's middleware after its own.
pub fn middleware_from(body: snapfire_fsr_ir::Body) -> Arc<dyn ActionHandler> {
  Arc::new(IrAction::new(body))
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

/// True when a lowered page or layout on the plan reads the `identity` or
/// `csrf_token` prop the assembler injects, which a render for nobody cannot
/// supply.
/// How much of the request a body or a plan depends on: nothing, the
/// identity alone, or more.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Static {
  Fixed,
  Anonymous,
  Dynamic,
}

/// `Static::Fixed` when the body reads nothing of the request, `Anonymous`
/// when its only request reads are `identity` and calls through a service in
/// `bearer`, `Dynamic` otherwise. A `meta` body's input is the loader's data,
/// which is not the request.
fn classify(body: &snapfire_fsr_ir::Body, meta: Option<&snapfire_fsr_ir::Body>, bearer: &[String]) -> Static {
  fn writes_session(body: &snapfire_fsr_ir::Body) -> bool {
    body.iter().any(|stmt| match stmt {
      snapfire_fsr_ir::Stmt::SessionSet { .. } | snapfire_fsr_ir::Stmt::SessionDelete { .. } => true,
      snapfire_fsr_ir::Stmt::If { then, r#else, .. } => writes_session(then) || writes_session(r#else),
      snapfire_fsr_ir::Stmt::ForOf { body, .. } => writes_session(body),
      _ => false,
    })
  }
  fn reads(body: &snapfire_fsr_ir::Body, input_is_request: bool, bearer: &[String]) -> Static {
    let mut class = Static::Fixed;
    body_visit(body, &mut |e| {
      let read = match e {
        Expr::Param(_) | Expr::Query(_) | Expr::Session(_) | Expr::Store(_) | Expr::Now => Static::Dynamic,
        Expr::Input if input_is_request => Static::Dynamic,
        Expr::Identity(_) => Static::Anonymous,
        Expr::Call { service, .. } if bearer.contains(service) => Static::Anonymous,
        _ => Static::Fixed,
      };
      class = class.max(read);
    });
    if writes_session(body) { Static::Dynamic } else { class }
  }
  let mut class = reads(body, true, bearer);
  if let Some(meta) = meta {
    class = class.max(reads(meta, false, bearer));
  }
  class
}

/// `Static::Dynamic` when a page or layout on the plan reads its
/// `csrf_token` prop, `Anonymous` when one reads its `identity` prop and
/// none the token, `Fixed` when none reads either.
fn plan_reads_request_props(plan: &snapfire_fsr_core::PlanNode, components: &[(String, Component)]) -> Static {
  let module = plan.module.to_string();
  let mut class = Static::Fixed;
  for (name, component) in components.iter().filter(|(name, _)| *name == module) {
    if component.reads_prop("csrf_token") {
      class = Static::Dynamic;
    } else if component.reads_prop("identity") {
      class = class.max(Static::Anonymous);
    }
  }
  for (_, child) in &plan.children {
    class = class.max(plan_reads_request_props(child, components));
  }
  class
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
