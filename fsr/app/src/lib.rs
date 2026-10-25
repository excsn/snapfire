//! Binds a plan file to the implementations that answer it. An application
//! supplies as much or as little as it wants: nothing and the file decides
//! everything; or a registration per name it wants to own in Rust.

pub mod plan;
pub mod routes;

use std::future::Future;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use snapfire_fsr_core::{Data, ModuleId, Params, PlanNode};
use snapfire_fsr_ir::{body_visit, Component, Expr, Extensions, Interpreter, IrAction, IrEvaluator, IrMeta, IrPaths, IrSource, IrStore, Tmpl};
use snapfire_fsr_runtime::store::Derived;
use snapfire_fsr_runtime::{
  ActionError, ActionHandler, ActionRegistry, DataSource, DataSources, Evaluator, Evaluators,
  HandlerMatch, HandlerMatcher, LoadCache, LoadError, Matcher, MatchitMatcher, Metadata, NodeCache, Paths, Reads, RequestCtx, Runtime, Seeds, Static, SubtreeReads, TableResolver, subtree_shape,
};
use snapfire_fsr_service::{Contract, Services, Type};

pub use plan::{IntoPlan, Plan};
pub use routes::Routes;
pub use snapfire_fsr_core::ext::{self, Ambient, Catalogs, Fail, FailureKind, Reach};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum BindError {
  #[error(transparent)]
  Plan(#[from] snapfire_fsr_plan::PlanError),
  #[error("the generated image at `{0}` cannot be routed: {1}")]
  Image(String, String),
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
  #[error("the plan names react@{version}, a React whose markup this fsr does not write")]
  React { version: String },
  #[error("the plan names vue@{version}, a Vue whose markup this fsr does not write")]
  Vue { version: String },
  #[error("the plan names svelte@{version}, a Svelte whose markup this fsr does not write")]
  Svelte { version: String },
  #[error("`{loader}` exports `paths`, which reads the request; a parameter set is decided with nothing of a request behind it")]
  PathsReadRequest { loader: String },
  #[error("`{loader}` exports `paths` but its route `{pattern}` has no parameter to enumerate")]
  PathsWithoutParameter { loader: String, pattern: String },
  #[error("`{loader}` exports `paths` but sits on no route")]
  PathsOffRoute { loader: String },
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
#[non_exhaustive]
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
  /// Patterns with a parameter whose page loader's `paths` names the sets
  /// the host prerenders them for; each is also in one of the two lists above.
  pub paths: Vec<String>,
  /// Sources whose load a build can run once and a request can then skip.
  pub warmable: Vec<String>,
  /// Subtrees a build can render once and a request can then splice in, as
  /// the route's pattern and the subtree's root module.
  pub renderable: Vec<(String, String)>,
  /// Modules rendered on the server, by the lowered tree or by Rust.
  pub components: Vec<(String, Owner)>,
  /// Modules the build could not lower, so the browser renders them, each
  /// with the residue that decided it.
  pub clients: Vec<snapfire_fsr_plan::ClientEntry>,
  /// Islands a template renders, with the handlers each answers: `<module> <name>`.
  pub islands: Vec<String>,
  /// The scripts each lowered `meta` declares in its head, as the meta's
  /// name and the `src` when the build can read it as a string; `None` is a
  /// `src` computed at request time, whose origin the policy must name itself.
  pub scripts: Vec<(String, Option<String>)>,
  /// The literal `src` of every `video`, `audio`, `source` and `track` a
  /// lowered component writes, by module.
  pub media: Vec<(String, String)>,
  /// The player URL of every embed placeholder a lowered component writes, by module.
  pub embeds: Vec<(String, String)>,
  /// The literal `src` of every `img` a lowered component writes, by module.
  pub images: Vec<(String, String)>,
  /// The content of every `<style>` a lowered component writes, by module, as the server writes it; `None` for one whose content the render computes.
  pub styles: Vec<(String, Option<String>)>,

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
    for (i, client) in self.clients.iter().enumerate() {
      let label = if i == 0 && self.components.is_empty() { "rendered" } else { "" };
      writeln!(f, "{label:<9} {:<22} {:<11} {}", client.module, "client", client.at)?;
    }
    let mut stated: Vec<(&str, &str)> = Vec::new();
    for client in &self.clients {
      if stated.contains(&(client.at.as_str(), client.message.as_str())) {
        continue;
      }
      let label = if stated.is_empty() { "client" } else { "" };
      stated.push((&client.at, &client.message));
      writeln!(f, "{label:<9} {:<22} {}", client.at, client.message)?;
      if let Some(hint) = &client.hint {
        writeln!(f, "{:<9} {hint}", "")?;
      }
    }
    for (i, island) in self.islands.iter().enumerate() {
      let label = if i == 0 { "islands" } else { "" };
      writeln!(f, "{label:<9} {island:<22} {}", Owner::Rust.as_str())?;
    }
    for (i, (meta, src)) in self.scripts.iter().enumerate() {
      let label = if i == 0 { "scripts" } else { "" };
      writeln!(f, "{label:<9} {meta:<22} {}", src.as_deref().unwrap_or("computed at request time; name its origin in [document.csp] script-src"))?;
    }
    Ok(())
  }
}

/// Everything a request needs, plus what the host bound to produce it.
#[non_exhaustive]
pub struct App {
  pub matcher: MatchitMatcher,
  pub resolver: TableResolver,
  /// Route handlers: a method and a pattern answered with a value.
  pub handlers: Handlers,
  /// The generated card images, each answered at its own URL.
  pub images: Images,
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
  /// Patterns whose every source is lowered and reads nothing of the request,
  /// so one render serves every request. A pattern with a parameter is here
  /// only when `paths` names its parameter sets.
  pub prerenderable: Vec<String>,
  /// Patterns whose only request reads are the identity, a page or layout's
  /// `identity` prop or a call through a client that carries the session's
  /// token: one anonymous render serves every anonymous request and a
  /// signed-in visitor is rendered live.
  pub prerenderable_anonymous: Vec<String>,
  /// By pattern: the parameter sets a route with a parameter is prerendered
  /// for, from its page loader's `paths`.
  pub paths: HashMap<String, Arc<dyn Paths>>,
  /// Every route's pattern, indexed by the entry the matcher answers with.
  pub patterns: Vec<String>,
  /// The subtrees a build can render ahead of any request, by the pattern of
  /// the route each sits on: the outermost fixed subtrees of every route that
  /// is not itself written as a document, each with a memo key and reading no
  /// store key a source outside it seeds.
  pub renderable: Vec<(String, PlanNode)>,
  /// Lowered sources reading nothing of the request beyond the identity, so
  /// one load answers every request (or every anonymous one). A build runs
  /// each once per locale and the memo answers from then on; which of the two
  /// a source is stays with the keyer, since only it composes the key.
  pub warmable: Vec<String>,
  pub runtime: Arc<Runtime>,
  pub services: Arc<Services>,
  /// The application's own Rust, by the name a body reaches it under as
  /// `ctx.native.<name>`.
  pub natives: Arc<snapfire_fsr_runtime::Natives>,
  pub actions: ActionRegistry,
  /// The handlers of every island a template renders, by module and name.
  pub islands: snapfire_fsr_runtime::IslandRegistry,
  /// The contract the plan's type names resolve through.
  pub contract: Option<Arc<Contract>>,
  /// Each lowered action's or handler's declared input type, by its id.
  pub declared_inputs: HashMap<String, String>,
  pub report: Report,
  /// What a lowered body runs on: the plan's constants, extensions and
  /// catalogs, every mounted site's included.
  interpreter: Interpreter,
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
  /// Every intercept plan, in no particular order.
  pub fn all(&self) -> impl Iterator<Item = &PlanNode> {
    self.plans.values().flatten()
  }

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

  pub fn allowed(&self, path: &str) -> Vec<String> {
    self.matcher.allowed(path)
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

/// The generated card images a host answers, matched by URL.
#[derive(Default)]
pub struct Images {
  matcher: MatchitMatcher,
  under_catch_all: Vec<CatchAllImage>,
  entries: Vec<snapfire_fsr_plan::ImageEntry>,
}

/// An image under a catch-all page. matchit allows nothing after `{*name}`,
/// so the page's own pattern is matched once the image's tail is off the path.
struct CatchAllImage {
  page: MatchitMatcher,
  /// `/opengraph-image.png`, or `/opengraph-image` when the module lists its images.
  tail: String,
  listed: bool,
  entry: snapfire_fsr_runtime::EntryId,
}

impl CatchAllImage {
  /// `pattern` split after its catch-all, when anything follows it.
  fn split(pattern: &str) -> Option<(&str, &str)> {
    let start = pattern.find("{*")?;
    let end = start + pattern[start..].find('}')? + 1;
    (end < pattern.len()).then(|| pattern.split_at(end))
  }

  fn match_path(&self, path: &str) -> Option<Params> {
    let (head, id) = match self.listed {
      false => (path, None),
      true => path.rsplit_once('/').filter(|(_, id)| !id.is_empty()).map(|(head, id)| (head, Some(id)))?,
    };
    let mut found = self.page.match_path(head.strip_suffix(self.tail.as_str())?)?;
    if let Some(id) = id {
      found.params.insert(snapfire_fsr_plan::IMAGE_PARAM.to_owned(), id.to_owned());
    }
    Some(found.params)
  }
}

impl Images {
  fn new(entries: Vec<snapfire_fsr_plan::ImageEntry>) -> Result<Self, BindError> {
    let mut matcher = MatchitMatcher::new();
    let mut under_catch_all = Vec::new();
    let listed_tail = format!("/{{{}}}", snapfire_fsr_plan::IMAGE_PARAM);
    for (i, entry) in entries.iter().enumerate() {
      let id = snapfire_fsr_runtime::EntryId(i as u32);
      let Some((page, tail)) = CatchAllImage::split(&entry.pattern) else {
        matcher.insert(&entry.pattern, id).map_err(|e| BindError::Image(entry.pattern.clone(), e.to_string()))?;
        continue;
      };
      let (tail, listed) = match tail.strip_suffix(listed_tail.as_str()) {
        Some(tail) => (tail, true),
        None => (tail, false),
      };
      if tail.contains('{') {
        return Err(BindError::Image(entry.pattern.clone(), "a parameter after a catch-all is not routed".to_owned()));
      }
      let mut page_matcher = MatchitMatcher::new();
      page_matcher.insert(page, id).map_err(|e| BindError::Image(entry.pattern.clone(), e.to_string()))?;
      under_catch_all.push(CatchAllImage { page: page_matcher, tail: tail.to_owned(), listed, entry: id });
    }
    Ok(Self { matcher, under_catch_all, entries })
  }

  /// The image `path` names, with the page's params.
  pub fn match_path(&self, path: &str) -> Option<(&snapfire_fsr_plan::ImageEntry, Params)> {
    if let Some(found) = self.matcher.match_path(path) {
      return Some((self.entries.get(found.entry.0 as usize)?, found.params));
    }
    self.under_catch_all.iter().find_map(|image| Some((self.entries.get(image.entry.0 as usize)?, image.match_path(path)?)))
  }

  pub fn entries(&self) -> &[snapfire_fsr_plan::ImageEntry] {
    &self.entries
  }

  pub fn is_empty(&self) -> bool {
    self.entries.is_empty()
  }
}

/// One image a card module's `images` lists.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct ListedImage {
  pub id: String,
  pub alt: Option<String>,
  pub width: u32,
  pub height: u32,
  pub content_type: String,
}

/// The images `entry`'s `images` body returns for `ctx`, each with the
/// entry's size, type and alt where it names none of its own.
pub async fn listed_images(interpreter: &Interpreter, entry: &snapfire_fsr_plan::ImageEntry, ctx: &RequestCtx) -> Result<Vec<ListedImage>, String> {
  let Some(body) = &entry.images else { return Ok(Vec::new()) };
  let outcome = interpreter.run(body, ctx, None).await.map_err(|fail| fail.message)?;
  let snapfire_fsr_core::Value::Seq(items) = &outcome.value else {
    return Err("`images` must return a list".to_owned());
  };
  let text = |v: Option<&snapfire_fsr_core::Value>| match v {
    Some(snapfire_fsr_core::Value::Str(s)) => Some(s.to_string()),
    Some(snapfire_fsr_core::Value::Int(n)) => Some(n.to_string()),
    Some(snapfire_fsr_core::Value::F64(f)) if f.fract() == 0.0 => Some(format!("{f}")),
    _ => None,
  };
  let side = |v: Option<&snapfire_fsr_core::Value>| match v {
    Some(snapfire_fsr_core::Value::Int(n)) => u32::try_from(*n).ok(),
    Some(snapfire_fsr_core::Value::F64(f)) if *f >= 1.0 && f.fract() == 0.0 => Some(*f as u32),
    _ => None,
  };
  let mut out = Vec::with_capacity(items.len());
  for item in items.iter() {
    let snapfire_fsr_core::Value::Map(fields) = item else {
      return Err("each entry of `images` is an object with an `id`".to_owned());
    };
    let id = text(fields.get("id")).filter(|id| !id.is_empty() && !id.contains('/')).ok_or("each entry of `images` has an `id`, a string or a number with no `/`")?;
    let (width, height) = match fields.get("size") {
      Some(snapfire_fsr_core::Value::Map(size)) => (side(size.get("width")).ok_or("`size.width` is a whole number")?, side(size.get("height")).ok_or("`size.height` is a whole number")?),
      _ => (entry.width, entry.height),
    };
    let content_type = match text(fields.get("contentType")) {
      Some(t) if t == "image/png" || t == "image/jpeg" => t,
      Some(t) => return Err(format!("`contentType` is `{t}`; a generated image is `image/png` or `image/jpeg`")),
      None => entry.content_type.clone(),
    };
    out.push(ListedImage { id, alt: text(fields.get("alt")).or_else(|| entry.alt.clone()), width, height, content_type });
  }
  Ok(out)
}

/// The head rows of a card module's listed images, one group per image.
struct ListedImages {
  entry: snapfire_fsr_plan::ImageEntry,
  interpreter: Interpreter,
}

impl snapfire_fsr_runtime::HeadRows for ListedImages {
  fn rows(&self, ctx: &RequestCtx) -> futures_util::future::BoxFuture<'static, Result<(String, Vec<snapfire_fsr_runtime::HeadEl>), LoadError>> {
    let entry = self.entry.clone();
    let interpreter = self.interpreter.clone();
    let ctx = ctx.clone();
    Box::pin(async move {
      let listed = listed_images(&interpreter, &entry, &ctx).await.map_err(|message| LoadError::new(entry.module.clone(), message))?;
      let (key, field, stem) = if entry.card == "twitter" { ("name", "twitter:image", "twitter-image") } else { ("property", "og:image", "opengraph-image") };
      let path = ctx.path.trim_end_matches('/');
      let row = |name: String, content: String| snapfire_fsr_runtime::HeadEl::new("meta".to_owned(), vec![(key.to_owned(), name), ("content".to_owned(), content)], None);
      let mut rows = Vec::new();
      for image in listed {
        rows.push(row(field.to_owned(), format!("{path}/{stem}/{}?v={}", image.id, entry.hash)));
        rows.push(row(format!("{field}:width"), image.width.to_string()));
        rows.push(row(format!("{field}:height"), image.height.to_string()));
        rows.push(row(format!("{field}:type"), image.content_type));
        if let Some(alt) = image.alt {
          rows.push(row(format!("{field}:alt"), alt));
        }
      }
      Ok((field.to_owned(), rows))
    })
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
  /// Registers no evaluator for the lowered components, so every one of them
  /// falls to `NullEvaluator` and the browser owns the whole render. Loaders,
  /// actions, metadata and the store still run in Rust.
  /// The application's own Rust, registered by name.
  natives: snapfire_fsr_runtime::Natives,
  islands: snapfire_fsr_runtime::IslandRegistry,
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
  lowered_stores: Vec<(String, Option<snapfire_fsr_ir::Body>, Option<snapfire_fsr_ir::Body>, Option<Vec<String>>)>,
  /// The plan's `reads`: the store keys each island the server does not render reads.
  island_reads: std::collections::BTreeMap<String, Vec<String>>,
  /// By source id: the page loader module's `paths` body.
  lowered_paths: Vec<(String, snapfire_fsr_ir::Body)>,
  lowered_actions: Vec<(String, Option<String>, snapfire_fsr_ir::Body)>,
  lowered_components: Vec<(String, Component)>,
  /// The head rows each lowered module asks for, by module id.
  lowered_heads: Vec<(String, Vec<snapfire_fsr_plan::HeadRow>)>,
  images: Vec<snapfire_fsr_plan::ImageEntry>,
  clients: Vec<snapfire_fsr_plan::ClientEntry>,
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
  loads: Option<Arc<dyn LoadCache>>,
  slot_order: Vec<String>,
  derives: Vec<snapfire_fsr_plan::DeriveEntry>,
  extensions: Extensions,
  catalogs: Option<Arc<Catalogs>>,
  files: Option<Arc<snapfire_fsr_core::ext::FileRoots>>,
  config: Arc<snapfire_fsr_core::ValueMap>,
  /// The plan's named constants, which `Expr::Const` reads.
  consts: Option<Arc<snapfire_fsr_ir::ast::Consts>>,
  bearer_services: Vec<String>,
  /// The frameworks the application vendors, whose markup the lowered components match.
  frameworks: snapfire_fsr_ir::Frameworks,
}

impl App {
  /// `body` as a handler that runs with this app's constants, extensions and
  /// catalogs, for a lowered body bound outside the plan's own tables: a
  /// mounted site's middleware or a theme.
  pub fn lowered_handler(&self, body: snapfire_fsr_ir::Body) -> Arc<dyn ActionHandler> {
    Arc::new(IrAction::new(body).with_interpreter(self.interpreter.clone()))
  }

  /// The input of the action or handler `id` read against its declared type,
  /// for an edge whose encoding carries strings only: a form body. A value
  /// that arrived typed must not be passed through here, since a string where
  /// a number is declared is a mismatch rather than a spelling.
  pub fn conform_text_input(&self, id: &str, input: &mut snapfire_fsr_core::Value) {
    let (Some(name), Some(contract)) = (self.declared_inputs.get(id), self.contract.as_ref()) else {
      return;
    };
    contract.conform_text(&Type::Named(name.clone()), input);
  }

  /// Drops every cached subtree under the plan `cache_key` and says how many went.
  pub async fn invalidate(&self, plan_key: &str) -> usize {
    self.runtime.cache.invalidate(plan_key).await
  }

  pub fn builder(routes: Routes) -> AppBuilder {
    AppBuilder {
      natives: snapfire_fsr_runtime::Natives::new(),
      islands: snapfire_fsr_runtime::IslandRegistry::new(),
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
      island_reads: Default::default(),
      lowered_paths: Vec::new(),
      lowered_actions: Vec::new(),
      lowered_components: Vec::new(),
      lowered_heads: Vec::new(),
      images: Vec::new(),
      clients: Vec::new(),
      contract: None,
      sources: DataSources::new(),
      claimed: Vec::new(),
      overrides: Vec::new(),
      evaluators: Evaluators::new(),
      actions: ActionRegistry::new(),
      action_claims: Vec::new(),
      extensions: Extensions::standard(),
      catalogs: None,
      files: None,
      config: Arc::default(),
      consts: None,
      bearer_services: Vec::new(),
      action_overrides: Vec::new(),
      services: None,
      cache: None,
      loads: None,
      slot_order: Vec::new(),
      derives: Vec::new(),
      frameworks: snapfire_fsr_ir::Frameworks::default(),
    }
  }

  /// The stock entry point: a plan file and nothing else. Every lowered row
  /// in the file is bound here as a default; Rust takes a name back with
  /// `source_override` or `action_override`.
  pub fn from_manifest(manifest: &str) -> Result<AppBuilder, BindError> {
    let parsed = snapfire_fsr_plan::Manifest::from_text(manifest)?;
    let mut builder = Self::builder(Routes::from_manifest(manifest)?);
    builder.add_rows(&parsed);
    builder.lowered_middleware = parsed.middleware.clone();
    if let Some(version) = parsed.frameworks.get("react") {
      builder.frameworks.react = Some(snapfire_fsr_ir::ReactMajor::of(version).ok_or_else(|| BindError::React { version: version.clone() })?);
    }
    if let Some(version) = parsed.frameworks.get("vue") {
      builder.frameworks.vue = Some(snapfire_fsr_ir::VueMajor::of(version).ok_or_else(|| BindError::Vue { version: version.clone() })?);
    }
    if let Some(version) = parsed.frameworks.get("svelte") {
      builder.frameworks.svelte = Some(snapfire_fsr_ir::SvelteMajor::of(version).ok_or_else(|| BindError::Svelte { version: version.clone() })?);
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
    let parsed = snapfire_fsr_plan::Manifest::from_text(manifest)?;
    self.routes.extend_manifest(manifest)?;
    self.add_rows(&parsed);
    Ok(())
  }

  /// Every lowered row of `parsed` beside the ones this builder holds.
  fn add_rows(&mut self, parsed: &snapfire_fsr_plan::Manifest) {
    self.declared_actions.extend(parsed.action_ids());
    self.lowered_sources.extend(parsed.lowered_sources().filter_map(|row| row.body.clone().map(|body| (row.id.clone(), body))));
    self.lowered_metas.extend(parsed.lowered_sources().filter_map(|row| row.meta.clone().map(|meta| (row.id.clone(), meta))));
    self.lowered_stores.extend(
      parsed
        .lowered_sources()
        .filter(|row| row.store.is_some() || row.defaults.is_some())
        .map(|row| (row.id.clone(), row.store.clone(), row.defaults.clone(), row.store_keys.clone())),
    );
    self.island_reads.extend(parsed.reads.clone());
    self.derives.extend(parsed.derives.iter().cloned());
    self.lowered_paths.extend(parsed.lowered_sources().filter_map(|row| row.paths.clone().map(|paths| (row.id.clone(), paths))));
    self.lowered_actions.extend(parsed.lowered_actions().filter_map(|row| row.body.clone().map(|body| (row.id.clone(), row.input.clone(), body))));
    self.lowered_components.extend(parsed.components.iter().map(|row| (row.module.clone(), row.body.clone())));
    self.lowered_heads.extend(parsed.components.iter().filter(|row| !row.head.is_empty()).map(|row| (row.module.clone(), row.head.clone())));
    self.lowered_heads.extend(parsed.heads.iter().map(|row| (row.module.clone(), row.head.clone())));
    self.images.extend(parsed.images.iter().cloned());
    self.clients.extend(parsed.clients.iter().cloned());
    if !parsed.consts.is_empty() {
      let mut merged = self.consts.as_deref().cloned().unwrap_or_default();
      merged.extend(parsed.consts.iter().map(|(k, v)| (k.clone(), v.clone())));
      self.consts = Some(Arc::new(merged));
    }
    for row in &parsed.handlers {
      match &row.body {
        Some(body) => self.lowered_handlers.push((row.id.clone(), row.method.clone(), row.pattern.clone(), row.input.clone(), body.clone())),
        None => self.declared_handlers.push((row.id.clone(), row.method.clone(), row.pattern.clone())),
      }
    }
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

  /// Registers a module of the application's own Rust under the name a body
  /// reaches it with, `ctx.native.<name>.<method>()`. Nothing crosses a wire,
  /// so it takes no contract and passes through no interceptor.
  pub fn native(mut self, name: impl Into<String>, module: Arc<dyn snapfire_fsr_runtime::Native>) -> Self {
    self.natives.register(name, module);
    self
  }

  pub fn evaluator<P>(mut self, predicate: P, evaluator: Arc<dyn Evaluator>) -> Self
  where
    P: Fn(&ModuleId) -> bool + Send + Sync + 'static,
  {
    self.evaluators.register(predicate, evaluator);
    self
  }

  /// Whether an evaluator registered so far answers `module`.
  pub fn covers(&self, module: &ModuleId) -> bool {
    self.evaluators.covers(module)
  }

  /// One handler of an island a template renders, answering with the state to
  /// render it from next. `module` is the template the placement names and
  /// `name` is what its markup binds, `data-sf-on="click:<name>"`.
  pub fn island_handler<F, Fut>(mut self, module: impl Into<String>, name: impl Into<String>, f: F) -> Self
  where
    F: Fn(RequestCtx, snapfire_fsr_runtime::IslandEvent) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<snapfire_fsr_core::Value, ActionError>> + Send + 'static,
  {
    self.islands.insert_fn(module, name, f);
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
    F: Fn(&Ambient, &[snapfire_fsr_core::Value]) -> Result<snapfire_fsr_core::Value, Fail> + Send + Sync + 'static,
  {
    self.extensions.register(name, reach, f);
    self
  }

  /// The extensions bound so far: the standard library and every `extension`.
  pub fn extensions(&self) -> &Extensions {
    &self.extensions
  }

  /// `[public]` from the deployment's configuration, which a component reads through `@snapfire/fsr/config`.
  pub fn config(mut self, config: snapfire_fsr_core::ValueMap) -> Self {
    self.config = Arc::new(config);
    self
  }

  /// The message catalogs `t` reads, by locale; none by default.
  /// The directories a body's `file` calls reach, resolved against the application; a web host always gives some, empty when the configuration names none.
  pub fn files(mut self, files: Arc<snapfire_fsr_core::ext::FileRoots>) -> Self {
    self.files = Some(files);
    self
  }

  pub fn catalogs(mut self, catalogs: Arc<Catalogs>) -> Self {
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

  /// Where a load answered once is kept, for the sources `warmable` names.
  /// Nothing is memoized without one.
  /// The frameworks the application vendors, for a host that builds its
  /// routes by hand; a plan file names its own. None unless set, where every
  /// lowered component is written as plain markup.
  pub fn frameworks(mut self, frameworks: snapfire_fsr_ir::Frameworks) -> Self {
    self.frameworks = frameworks;
    self
  }

  pub fn loads(mut self, loads: Arc<dyn LoadCache>) -> Self {
    self.loads = Some(loads);
    self
  }

  /// The slot names that settle a store key two parallel slots both seed, the later winning; `[store] slot_order`.
  pub fn slot_order(mut self, slot_order: Vec<String>) -> Self {
    self.slot_order = slot_order;
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
  /// the plan names must be bound and every override must name something.
  pub fn build(mut self) -> Result<App, BindError> {
    let declared: Vec<String> = self.routes.plans().flat_map(declared_sources).collect();

    let check = |owner: &str, body: &snapfire_fsr_ir::Body| -> Result<(), BindError> {
      match unknown_extension(body, &self.extensions) {
        Some(name) => Err(BindError::UnknownExtension { owner: owner.to_owned(), name }),
        None => Ok(()),
      }
    };
    let stores = self.lowered_stores.iter().flat_map(|(name, store, defaults, _)| store.iter().chain(defaults).map(move |body| (name, body)));
    for (name, body) in self.lowered_sources.iter().chain(&self.lowered_metas).chain(&self.lowered_paths).map(|(name, body)| (name, body)).chain(stores) {
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
    let derives = derive_order(std::mem::take(&mut self.derives));
    let interpreter = Interpreter::default()
      .with_extensions(Arc::new(self.extensions.clone()))
      .with_catalogs(self.catalogs.clone())
      .with_files(self.files.clone())
      .with_config(self.config.clone())
      .with_consts(self.consts.clone())
      .with_frameworks(self.frameworks)
      .with_island_reads(self.island_reads.clone())
      .with_derives(derives.iter().map(|row| snapfire_fsr_ir::Derive::new(row.key.clone(), row.sources.clone(), row.body.clone())).collect());

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
    let params_read: HashMap<String, Vec<String>> = self.lowered_sources.iter().map(|(name, body)| (name.clone(), snapfire_fsr_ir::body_params_read(body))).collect();
    let path_readers: HashSet<String> = self
      .lowered_sources
      .iter()
      .filter(|(_, body)| reads_path(body) || reads_param(body))
      .map(|(name, _)| name.clone())
      .collect();
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
    let mut declared_inputs: HashMap<String, String> = HashMap::new();
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
              declared_inputs.insert(id.clone(), input.clone());
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
          declared_inputs.insert(id.clone(), input.clone());
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
    let mut paths: HashMap<String, Arc<dyn Paths>> = HashMap::new();
    for (name, body) in std::mem::take(&mut self.lowered_paths) {
      if !lowered(&name) {
        continue;
      }
      if classify(&body, None, &self.bearer_services) != Static::Fixed {
        return Err(BindError::PathsReadRequest { loader: name });
      }
      let on: Vec<&String> = resolved.iter().filter(|(_, plan, _)| declared_sources(plan).contains(&name)).map(|(pattern, _, _)| pattern).collect();
      if on.is_empty() {
        return Err(BindError::PathsOffRoute { loader: name });
      }
      let enumerated: Arc<dyn Paths> = Arc::new(IrPaths::new(name.clone(), body).with_interpreter(interpreter.clone()));
      for pattern in on {
        if !pattern.contains('{') {
          return Err(BindError::PathsWithoutParameter { loader: name, pattern: pattern.clone() });
        }
        paths.insert(pattern.clone(), enumerated.clone());
      }
    }
    let warm_fixed: HashSet<String> = fixed_sources
      .iter()
      .filter(|n| lowered(n) && !path_readers.contains(*n))
      .cloned()
      .collect();
    let warm_anonymous: HashSet<String> = anonymous_sources
      .iter()
      .filter(|n| lowered(n) && !path_readers.contains(*n) && !warm_fixed.contains(*n))
      .cloned()
      .collect();
    let mut warmable: Vec<String> = warm_fixed.iter().chain(&warm_anonymous).cloned().collect();
    warmable.sort();
    let prerenderable: Vec<String> = resolved
      .iter()
      .filter(|(pattern, plan, _)| {
        (!pattern.contains('{') || paths.contains_key(pattern))
          && declared_sources(plan).iter().all(|name| fixed_sources.contains(name) && lowered(name))
          && plan_reads_request_props(plan, &self.lowered_components) == Static::Fixed
      })
      .map(|(pattern, _, _)| pattern.clone())
      .collect();
    let prerenderable_anonymous: Vec<String> = resolved
      .iter()
      .filter(|(pattern, plan, _)| {
        (!pattern.contains('{') || paths.contains_key(pattern))
          && !prerenderable.contains(pattern)
          && declared_sources(plan).iter().all(|name| anonymous_sources.contains(name) && lowered(name))
          && plan_reads_request_props(plan, &self.lowered_components) != Static::Dynamic
      })
      .map(|(pattern, _, _)| pattern.clone())
      .collect();
    let mut reads = Reads::new();
    {
      let source_class = |name: &String| if lowered(name) { statics.get(name).copied().unwrap_or(Static::Dynamic) } else { Static::Dynamic };
      let all = resolved.iter().map(|(_, plan, _)| plan).chain(intercepts.plans.values().flatten()).chain(not_found.iter());
      for plan in all {
        subtree_reads(plan, &source_class, &self.lowered_components, &self.island_reads, &mut reads);
      }
      for entry in reads.values_mut() {
        derived_sources(&mut entry.store_keys, &derives);
      }
    }
    let seeders: HashMap<String, Option<Vec<String>>> = self
      .lowered_stores
      .iter()
      .map(|(name, store, defaults, keys)| (name.clone(), IrStore::new(name.clone()).with_store(store.clone()).with_defaults(defaults.clone()).with_keys(keys.clone()).keys()))
      .collect();
    let mut renderable: Vec<(String, PlanNode)> = Vec::new();
    for (pattern, plan, _) in &resolved {
      if prerenderable.contains(pattern) || (pattern.contains('{') && !paths.contains_key(pattern)) {
        continue;
      }
      let on_route = declared_sources(plan);
      renderable_subtrees(plan, pattern, &on_route, &seeders, &reads, &mut renderable);
    }
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
    let images: Vec<(String, String)> = self.lowered_components.iter().flat_map(|(module, component)| snapfire_fsr_ir::ast::image_sources(&component.render).into_iter().map(move |src| (module.clone(), src))).collect();
    let embeds: Vec<(String, String)> = self.lowered_components.iter().flat_map(|(module, component)| snapfire_fsr_ir::ast::embed_players(&component.render).into_iter().map(move |src| (module.clone(), src))).collect();
    let media: Vec<(String, String)> = self.lowered_components.iter().flat_map(|(module, component)| snapfire_fsr_ir::ast::media_sources(&component.render).into_iter().map(move |src| (module.clone(), src))).collect();
    let styles: Vec<(String, Option<String>)> = self.lowered_components.iter().flat_map(|(module, component)| snapfire_fsr_ir::render::inline_styles(component).into_iter().map(move |css| (module.clone(), css))).collect();
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
      paths: resolved.iter().map(|(p, _, _)| p).filter(|p| paths.contains_key(*p)).cloned().collect(),
      warmable: warmable.clone(),
      renderable: renderable.iter().map(|(pattern, node)| (pattern.clone(), node.module.to_string())).collect(),
      components,
      clients: std::mem::take(&mut self.clients),
      islands: self
        .islands
        .modules()
        .into_iter()
        .flat_map(|module| self.islands.names(&module).into_iter().map(move |name| format!("{module} {name}")))
        .collect(),
      scripts: self.lowered_metas.iter().flat_map(|(name, body)| snapfire_fsr_ir::ast::head_scripts(body).into_iter().map(move |script| (name.clone(), script.src))).collect(),
      media,
      embeds,
      images,
      styles,
    };
    report.routes.sort_by(|a, b| a.0.cmp(&b.0));

    let mut matcher = MatchitMatcher::new();
    let mut resolver = TableResolver::new();
    let patterns: Vec<String> = resolved.iter().map(|(p, _, _)| p.clone()).collect();
    for (index, (pattern, plan, _)) in resolved.into_iter().enumerate() {
      let entry = snapfire_fsr_runtime::EntryId(index as u32);
      matcher
        .insert(&pattern, entry)
        .map_err(|e| BindError::Pattern { pattern: pattern.clone(), message: e.to_string() })?;
      resolver.insert(entry, plan);
    }

    let mut heads: std::collections::HashMap<String, Vec<snapfire_fsr_runtime::HeadEl>> = std::collections::HashMap::new();
    for (module, rows) in &self.lowered_heads {
      heads.entry(module.clone()).or_default().extend(rows.iter().map(|row| snapfire_fsr_runtime::HeadEl::new(row.tag.clone(), row.attrs.clone(), None)));
    }
    let mut listers: Vec<(String, Arc<dyn snapfire_fsr_runtime::HeadRows>)> = Vec::new();
    for entry in &self.images {
      if entry.images.is_some() {
        listers.push((entry.page.clone(), Arc::new(ListedImages { entry: entry.clone(), interpreter: interpreter.clone() })));
      }
    }
    let mut runtime = Runtime::builder()
      .sources(self.sources)
      .evaluators(self.evaluators)
      .reads(reads)
      .heads(heads)
      .keyer(Arc::new(ReadsKeyer { reads: params_read }))
      .slot_order(self.slot_order.clone())
      .derives(
        derives
          .iter()
          .map(|row| Derived::new(row.key.clone(), row.sources.clone(), row.body.clone().map(|body| {
              let interpreter = interpreter.clone();
              Arc::new(move |ctx: &RequestCtx, store: &Data| interpreter.compute(ctx, &body, store)) as snapfire_fsr_runtime::store::Compute
            })))
          .collect(),
      )
      .load_keyer(Arc::new(ClassKeyer {
        fixed: warm_fixed,
        anonymous: warm_anonymous,
      }));
    if let Some(cache) = self.cache {
      runtime = runtime.cache(cache);
    }
    if let Some(loads) = self.loads {
      runtime = runtime.loads(loads);
    }
    for (module, lister) in listers {
      runtime = runtime.listed(module, lister);
    }
    for (name, meta) in std::mem::take(&mut self.lowered_metas) {
      if self.claimed.iter().any(|(claimed, owner)| *claimed == name && *owner == Owner::Lowered) {
        runtime = runtime.meta(name.clone(), Arc::new(IrMeta::new(name, meta).with_interpreter(interpreter.clone())));
      }
    }
    for (name, meta) in std::mem::take(&mut self.rust_metas) {
      runtime = runtime.meta(name, meta);
    }
    for (name, store, defaults, keys) in std::mem::take(&mut self.lowered_stores) {
      if self.claimed.iter().any(|(claimed, owner)| *claimed == name && *owner == Owner::Lowered) {
        runtime = runtime.store(name.clone(), Arc::new(IrStore::new(name).with_store(store).with_defaults(defaults).with_keys(keys).with_interpreter(interpreter.clone())));
      }
    }

    let images = Images::new(std::mem::take(&mut self.images))?;
    Ok(App {
      matcher,
      resolver,
      lowered,
      handlers,
      images,
      middleware,
      not_found,
      intercepts,
      prerenderable,
      prerenderable_anonymous,
      paths,
      patterns,
      renderable,
      warmable,
      runtime: runtime.build(),
      services: self.services.unwrap_or_else(|| Services::builder().build()),
      natives: Arc::new(self.natives),
      actions: self.actions,
      islands: self.islands,
      contract: self.contract,
      declared_inputs,
      report,
      interpreter,
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

/// A lowered middleware body as a handler on an interpreter with the standard
/// extensions alone: no constants, catalogs or application extensions. A body
/// that reads any of those runs through `App::lowered_handler` instead.
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
  fn call(&self, ctx: RequestCtx, mut input: snapfire_fsr_core::Value) -> futures_util::future::BoxFuture<'static, Result<snapfire_fsr_core::Value, ActionError>> {
    let ty = Type::Named(self.input.clone());
    self.contract.conform(&ty, &mut input);
    if let Err(e) = self.contract.check_value(&ty, &input, "input") {
      let error = ActionError::new(snapfire_fsr_runtime::FailureKind::Invalid, e.to_string());
      return Box::pin(async move { Err(error) });
    }
    self.inner.call(ctx, input)
  }
}

/// A body reading `Expr::Path` or `Expr::Document`. A prerendered route has one path, so the
/// classification calls that fixed; a memo is keyed by the source rather than
/// the route and one layout source answers every path beneath it, so a
/// source reading the path is never memoized.
fn reads_path(body: &snapfire_fsr_ir::Body) -> bool {
  let mut found = false;
  body_visit(body, &mut |e| {
    if matches!(e, Expr::Path | Expr::Document) {
      found = true;
    }
  });
  found
}

/// A body reading a route parameter. A prerender of a route with `paths`
/// renders each parameter set once, so the classification calls that fixed;
/// the memo is keyed by the source alone, so such a source is never memoized.
fn reads_param(body: &snapfire_fsr_ir::Body) -> bool {
  let mut found = false;
  body_visit(body, &mut |e| {
    if matches!(e, Expr::Param(_)) {
      found = true;
    }
  });
  found
}

/// Keys a load by what its source was seen to read. A `Fixed` source reads
/// nothing of the request, so its locale alone separates two loads; an
/// `Anonymous` source reads the identity or calls through a client carrying
/// its token, so only the anonymous case is shared and a signed-in request
/// loads for itself.
struct ClassKeyer {
  fixed: HashSet<String>,
  anonymous: HashSet<String>,
}

impl snapfire_fsr_runtime::LoadKeyer for ClassKeyer {
  fn key(&self, source: &snapfire_fsr_core::DataSourceId, ctx: &RequestCtx) -> Option<String> {
    let suffix = ctx.locale.key_suffix();
    if self.fixed.contains(&source.0) {
      return Some(format!("{}{suffix}", source.0));
    }
    if self.anonymous.contains(&source.0) && ctx.session.identity().is_none() {
      return Some(format!("{}{suffix}|anon", source.0));
    }
    None
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
/// `Static::Fixed` when the body reads nothing of the request, `Anonymous`
/// when its only request reads are `identity` and calls through a service in
/// `bearer`, `Dynamic` otherwise. A route parameter is not a request read: a
/// prerender supplies it and `reads_param` keeps such a source out of the
/// memo. A `meta` body's input is the loader's data, which is not the request.
fn classify(body: &snapfire_fsr_ir::Body, meta: Option<&snapfire_fsr_ir::Body>, bearer: &[String]) -> Static {
  fn writes_session(body: &snapfire_fsr_ir::Body) -> bool {
    body.iter().any(|stmt| match stmt {
      snapfire_fsr_ir::Stmt::SessionSet { .. } | snapfire_fsr_ir::Stmt::SessionDelete { .. } | snapfire_fsr_ir::Stmt::SessionExtend { .. } => true,
      snapfire_fsr_ir::Stmt::If { then, r#else, .. } => writes_session(then) || writes_session(r#else),
      snapfire_fsr_ir::Stmt::ForOf { body, .. } => writes_session(body),
      _ => false,
    })
  }
  fn reads(body: &snapfire_fsr_ir::Body, input_is_request: bool, bearer: &[String]) -> Static {
    let mut class = Static::Fixed;
    body_visit(body, &mut |e| {
      let read = match e {
        Expr::Query(_) | Expr::ParamsAll | Expr::QueryAll | Expr::Session(_) | Expr::Store(_) | Expr::Now | Expr::Host | Expr::Address => Static::Dynamic,
        Expr::Input if input_is_request => Static::Dynamic,
        Expr::Identity(_) => Static::Anonymous,
        Expr::Call { service, .. } if bearer.contains(service) => Static::Anonymous,
        Expr::Ext { module, name, .. } if snapfire_fsr_ir::standard_reach(module, name).is_some_and(|reach| reach != snapfire_fsr_ir::Reach::Render) => Static::Dynamic,
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
  for (_, component) in components.iter().filter(|(name, _)| *name == module) {
    if component.reads_prop("csrf_token") || component.reads_prop("csp_nonce") {
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

/// Records what every subtree under `node` reads of the request, by shape,
/// and returns the root's. A node reads what its source reads and what its
/// component reads of the `identity` and `csrf_token` props; the store keys
/// its component, or a component it places, reads are listed rather than
/// classed, since the memo key carries their values whoever seeds them. A
/// module the app cannot see through reads everything.
fn subtree_reads(node: &snapfire_fsr_core::PlanNode, source_class: &dyn Fn(&String) -> Static, components: &[(String, Component)], island_reads: &BTreeMap<String, Vec<String>>, reads: &mut Reads) -> SubtreeReads {
  let mut class = match &node.data_source {
    None => Static::Fixed,
    Some(source) => source_class(&source.0),
  };
  let module = node.module.to_string();
  let mut keys = Vec::new();
  let mut path = false;
  let mut csrf = false;
  match components.iter().find(|(name, _)| *name == module) {
    None => {
      class = Static::Dynamic;
      csrf = true;
    }
    Some((_, component)) => {
      if component.reads_prop("csrf_token") {
        class = Static::Dynamic;
        csrf = true;
      } else if component.reads_prop("csp_nonce") {
        class = Static::Dynamic;
      } else if component.reads_prop("identity") {
        class = class.max(Static::Anonymous);
      }
      let mut seen = HashSet::new();
      store_keys_read(&module, components, island_reads, &mut seen, &mut keys);
      let mut seen = HashSet::new();
      path = renders_path(&module, components, &mut seen);
    }
  }
  for (_, child) in &node.children {
    let below = subtree_reads(child, source_class, components, island_reads, reads);
    class = class.max(below.class);
    keys.extend(below.store_keys);
    path = path || below.path;
    csrf = csrf || below.csrf;
  }
  keys.sort();
  keys.dedup();
  let out = SubtreeReads::default().class(class).store_keys(keys).path(path).csrf(csrf);
  reads.insert(subtree_shape(node), out.clone());
  out
}

/// The outermost subtrees under `node` a build can render ahead of a request:
/// one with a memo key, a `Fixed` class, no deferred descendant and no store
/// key read that a source outside it seeds, since the build renders it with
/// its own seeds alone and the key must match a request's.
fn renderable_subtrees(node: &PlanNode, pattern: &str, on_route: &[String], seeders: &HashMap<String, Option<Vec<String>>>, reads: &Reads, out: &mut Vec<(String, PlanNode)>) {
  fn has_deferred(node: &PlanNode) -> bool {
    node.children.iter().any(|(_, c)| c.deferred || has_deferred(c))
  }
  let entry = reads.get(&subtree_shape(node));
  if node.cache_key.is_some() && entry.is_some_and(|r| r.class == Static::Fixed) && !has_deferred(node) {
    let inside = declared_sources(node);
    let seeded_outside = entry.unwrap().store_keys.iter().any(|key| {
      on_route.iter().filter(|source| !inside.contains(source)).any(|source| match seeders.get(source) {
        None => false,
        Some(None) => true,
        Some(Some(keys)) => keys.contains(key),
      })
    });
    if !seeded_outside {
      out.push((pattern.to_owned(), node.clone()));
      return;
    }
  }
  for (_, child) in &node.children {
    renderable_subtrees(child, pattern, on_route, seeders, reads, out);
  }
}

/// The plan's derive rows, each derived source before the key it feeds.
fn derive_order(rows: Vec<snapfire_fsr_plan::DeriveEntry>) -> Vec<snapfire_fsr_plan::DeriveEntry> {
  let order = snapfire_fsr_runtime::store::derive_order(rows.iter().map(|row| Derived::new(row.key.clone(), row.sources.clone(), None)).collect());
  let mut rows = rows;
  order.iter().filter_map(|derived| rows.iter().position(|row| row.key == derived.key).map(|at| rows.remove(at))).collect()
}

/// Adds to `keys` the sources of every derived key in it, transitively: a
/// derived value is computed from them, so a render reading it reads them.
fn derived_sources(keys: &mut Vec<String>, derives: &[snapfire_fsr_plan::DeriveEntry]) {
  let mut at = 0;
  while at < keys.len() {
    if let Some(row) = derives.iter().find(|row| row.key == keys[at]) {
      for source in &row.sources {
        if !keys.contains(source) {
          keys.push(source.clone());
        }
      }
    }
    at += 1;
  }
  keys.sort();
}

/// The store keys `module`'s component reads, and every component it places
/// reads, transitively.
fn store_keys_read(module: &str, components: &[(String, Component)], island_reads: &BTreeMap<String, Vec<String>>, seen: &mut HashSet<String>, out: &mut Vec<String>) {
  if !seen.insert(module.to_owned()) {
    return;
  }
  let Some((_, component)) = components.iter().find(|(name, _)| name == module) else {
    out.extend(island_reads.get(module).into_iter().flatten().cloned());
    return;
  };
  component.visit(&mut |e| {
    if let Expr::Store(key) = e {
      out.push(key.clone());
    }
  });
  let mut placed = Vec::new();
  placed_modules(&component.render, &mut placed);
  for module in placed {
    store_keys_read(&module, components, island_reads, seen, out);
  }
}

/// Whether `module`, or a component it places, renders the path the request
/// matched. A `<Link>` does, to mark itself the page being shown.
fn renders_path(module: &str, components: &[(String, Component)], seen: &mut HashSet<String>) -> bool {
  if !seen.insert(module.to_owned()) {
    return false;
  }
  let Some((_, component)) = components.iter().find(|(name, _)| name == module) else { return false };
  let mut found = false;
  component.visit(&mut |e| {
    if matches!(e, Expr::Path | Expr::Document) {
      found = true;
    }
  });
  if found {
    return true;
  }
  let mut placed = Vec::new();
  placed_modules(&component.render, &mut placed);
  placed.iter().any(|module| renders_path(module, components, seen))
}

fn placed_modules(tmpl: &Tmpl, out: &mut Vec<String>) {
  match tmpl {
    Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Outlet { .. } | Tmpl::Html(_) | Tmpl::Expr(_) | Tmpl::Slot(_) => {}
    Tmpl::Element { children, .. } | Tmpl::Fragment(children) | Tmpl::Place { children, .. } | Tmpl::Baked { children, .. } => children.iter().for_each(|c| placed_modules(c, out)),
    Tmpl::If { then, r#else, .. } => {
      placed_modules(then, out);
      if let Some(e) = r#else {
        placed_modules(e, out);
      }
    }
    Tmpl::For { body, .. } => placed_modules(body, out),
    Tmpl::Let { then, .. } => placed_modules(then, out),
    Tmpl::Component { module, children, .. } | Tmpl::Island { module, children, .. } => {
      out.push(module.clone());
      children.iter().for_each(|c| placed_modules(c, out));
    }
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
