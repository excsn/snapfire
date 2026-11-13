//! The artifact a build emits and a runtime reads at boot. It is deliberately
//! its own format rather than serde on the runtime types, so the file can gain
//! a field without the vocabulary crate gaining a dependency.

use snapfire_fsr_core::{CacheKey, DataSourceId, ModuleId, NodeId, PlanNode, SlotName};
use snapfire_fsr_ir::ast::Consts;
use snapfire_fsr_ir::{Body, Component};

mod namespace;
pub mod sexpr;

/// Format 2 adds the `sources` table and makes actions rows. A format 1 file,
/// with bare action ids and no sources, still reads. Format 4 adds a node's
/// `error-kind` sections, which a format 3 reader refuses by name rather than
/// ignoring, so the version is what tells an older host to say so plainly.
/// Format 5 adds a component's head rows and names every component's owner,
/// a source's `store-keys` and the `reads` of islands the server does not
/// render; in an older file a component that names none is React's.
///
/// Format 6 adds `derive` rows: a store key the browser's `derive` computes,
/// its source keys and the lowered compute, which the host applies after the
/// seeds merge. A host reading format 5 would render without them, painting
/// a derived key's fallback where the browser shows its value. It also adds
/// a source's `defaults`, a second seed body whose keys a browser write
/// outranks, the `module-head` and `image` rows of generated card images, the
/// lowered `theme.ts` and a `theme-of` row per layout or page that sets a theme.
///
/// Format 7 adds the `marker` template node (markup a framework's hydration
/// reads, placed by the lowerer), the `html` template node (a value written
/// unescaped), the `place` template node (content written at the end of the
/// body), the `cell`, `module-cell`, `read-cell` and `write-cell` expressions (state a
/// render holds), the `!` entry (attributes written as given), the `svelte`
/// owner and framework and the `quot` operator, a `bigint` division. A format
/// 6 reader refuses each by name.
/// It also adds the bitwise operators `bit-and`, `bit-or`, `bit-xor`, `<<`, `>>` and `>>>`.
/// It also adds the `module-sheets` rows: the stylesheets a route segment's module links.
/// It also adds the `locales` expression, `ctx.locales`.
///
/// Raise this only for a change to a format a published release of this crate
/// already writes. A change to the format since the last publish is part of
/// the format this number already names: add it to that format's line above
/// and regenerate its golden file. Check the number in the last published
/// release before raising it.
pub const FORMAT_VERSION: u32 = 7;
/// The first format in which a component must name its owner.
pub(crate) const OWNERS_NAMED: u32 = 5;
const OLDEST_READABLE: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PlanError {
  #[error("the plan file does not parse: {0}")]
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

#[derive(Debug, Clone, PartialEq)]
pub struct Manifest {
  pub version: u32,
  pub routes: Vec<RouteEntry>,
  /// One row per data source the build knows about. A row with a body is a
  /// default the host binds unless Rust overrides the name; a row without one
  /// is a declaration Rust must answer.
  pub sources: Vec<SourceEntry>,
  /// The actions the application expects to exist. Declared so an unanswered
  /// action is a boot error rather than a 404 at request time.
  pub actions: Vec<ActionEntry>,
  /// One row per module the build lowered to a render tree. The host renders
  /// these in Rust and the browser hydrates over the output; a module without
  /// a row mounts in the browser only.
  pub components: Vec<ComponentEntry>,
  /// One row per module the build read and could not lower, with the residue
  /// that decided it. The host prints these beside the rendered modules.
  pub clients: Vec<ClientEntry>,
  /// The module-level constants bodies read by name, `<file>#<name>`, held
  /// once however many bodies read them. `Expr::Const` names one.
  pub consts: Consts,
  /// The tree a host renders, with status 404, for a path no route matches.
  /// Absent, the host answers with a plain text line.
  pub not_found: Option<Node>,
  /// One row per HTTP method a `route.ts` exports: a request the host answers
  /// with a value rather than a document.
  pub handlers: Vec<HandlerEntry>,
  /// The lowered `middleware.ts`, run before every request that is not a
  /// static file.
  pub middleware: Option<Body>,
  /// One row per `page.<slot>.tsx`: the pattern of the route it belongs to
  /// and the tree a soft navigation renders into a live layout's slot.
  pub intercepts: Vec<RouteEntry>,
  /// The exact version of each vendored framework whose server markup the
  /// renderer matches, by package: `react` to `18.3.1`. Read from the vendor
  /// manifest at build.
  pub frameworks: std::collections::BTreeMap<String, String>,
  /// The store keys an island the server does not render reads, by module:
  /// what the first wave holds its mount on while one of them is pending.
  pub reads: std::collections::BTreeMap<String, Vec<String>>,
  /// One row per store key a module-level `derive` computes in the browser,
  /// in the order the browser files declare them.
  pub derives: Vec<DeriveEntry>,
  /// Head rows for a module the build knows from a file beside it rather than
  /// from lowering it: a route directory's `opengraph-image`. Merged with a
  /// component's own rows under the same module.
  pub heads: Vec<ModuleHead>,
  /// The stylesheets a page rendering a route segment's module links: each
  /// sheet a file the module reaches imports for its effect and each Vue or
  /// Svelte component it reaches, by the path its compiled sheet would have.
  pub sheets: Vec<ModuleSheets>,
  /// One row per generated card image: an `opengraph-image.tsx` or
  /// `twitter-image.tsx` beside a page, answered at its own URL.
  pub images: Vec<ImageEntry>,
  /// The lowered `theme.ts`: the document's theme and scheme for one request, run before the
  /// page with the route's own theme as its input.
  pub theme: Option<Body>,
  /// One row per layout or page whose `export const theme` names one.
  pub themes: Vec<ThemeEntry>,
}

/// A layout's or a page's `export const theme`. Down a route's layouts the deepest one wins,
/// except that a forced theme keeps everything below it; of two forced ones the outer wins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeEntry {
  pub module: String,
  pub name: String,
  pub force: bool,
}

/// A card image a lowered component draws, answered at `pattern` with the
/// data of `source` as its props.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageEntry {
  /// The URL it answers, the page's pattern with the image's file name after it.
  pub pattern: String,
  /// The component that draws it.
  pub module: String,
  /// The page's data source, whose data are the component's props.
  pub source: Option<String>,
  pub width: u32,
  pub height: u32,
  /// `image/png` or `image/jpeg`.
  pub content_type: String,
  /// The page module whose head names the image.
  pub page: String,
  /// `og` or `twitter`: which card's tags name it.
  pub card: String,
  /// The component's hash, which the image's URL carries so a crawler fetches it again after a change.
  pub hash: String,
  pub alt: Option<String>,
  /// The module's `images` (or `generateImageMetadata`), a function of the
  /// context returning `[{ id, alt, size, contentType }]`. With one, the
  /// pattern ends in `{__image}` and each id is its own image.
  pub images: Option<snapfire_fsr_ir::Body>,
}

/// The parameter a listed image's pattern names its id by.
pub const IMAGE_PARAM: &str = "__image";

/// The head rows one module carries whatever renders it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleHead {
  pub module: String,
  pub head: Vec<HeadRow>,
}

/// The stylesheets one route segment's module links, as paths under the
/// application's build output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleSheets {
  pub module: String,
  pub sheets: Vec<String>,
}

/// A derived store key: what it is computed from and how. A row without a
/// body is one the build could not lower, with where and why: the host holds
/// every reader of the key rather than paint a value the browser replaces.
#[derive(Debug, Clone, PartialEq)]
pub struct DeriveEntry {
  pub key: String,
  pub sources: Vec<String>,
  /// The compute, reading each source as `Expr::Store`.
  pub body: Option<snapfire_fsr_ir::ast::Expr>,
  /// `file:line:column` of the `derive` that did not lower.
  pub at: Option<String>,
  pub why: Option<String>,
}

/// A handler row: `method` and `pattern` are what the host matches, `id` is
/// `<route id>.<METHOD>`. A row without a body is a declaration Rust answers.
#[derive(Debug, Clone, PartialEq)]
pub struct HandlerEntry {
  pub id: String,
  pub method: String,
  pub pattern: String,
  pub owner: RowOwner,
  pub module: Option<String>,
  pub input: Option<String>,
  pub reason: Option<String>,
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

#[derive(Debug, Clone, PartialEq)]
pub struct ComponentEntry {
  pub module: String,
  pub body: Component,
  /// Head elements a page rendering this module carries, the preload of a
  /// priority image among them. The build folds in the rows of every
  /// component the module places, so a plan node's module holds the set.
  pub head: Vec<HeadRow>,
}

/// One head element a component asks for: a tag and its attributes in the
/// order written. A row is merged into the document's head by identity, so
/// two pages placing one image preload it once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadRow {
  pub tag: String,
  pub attrs: Vec<(String, String)>,
}

/// One row per module the build could not lower, so the host can say at
/// boot what the build said: the module mounts in the browser, because of
/// the residue at `at`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientEntry {
  pub module: String,
  /// `file:line:column`; the file alone when the module did not parse.
  pub at: String,
  pub message: String,
  pub hint: Option<String>,
}

/// Who a row says answers it. The host may replace `lowered` with a Rust
/// override; `rust` is a declaration and nothing else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq)]
pub struct SourceEntry {
  pub id: String,
  pub owner: RowOwner,
  pub module: Option<String>,
  pub export: Option<String>,
  pub reason: Option<String>,
  pub body: Option<Body>,
  /// The module's `meta`, describing the document from this source's data.
  pub meta: Option<Body>,
  /// The module's `store`, seeding the browser's store from this source's data.
  pub store: Option<Body>,
  /// The module's `defaults`, seeding the browser's store the way `store` does with values a write the browser holds outranks.
  pub defaults: Option<Body>,
  /// The keys `store` and `defaults` can return, inferred from their types at build: what the
  /// segment promises a first wave it is deferred from.
  pub store_keys: Option<Vec<String>>,
  /// The module's `paths`, naming the parameter sets a route with a parameter prerenders.
  pub paths: Option<Body>,
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
      defaults: None,
      store_keys: None,
      paths: None,
    }
  }

  pub fn with_meta(mut self, meta: Option<Body>) -> Self {
    self.meta = meta;
    self
  }

  pub fn with_defaults(mut self, defaults: Option<Body>) -> Self {
    self.defaults = defaults;
    self
  }

  pub fn with_store(mut self, store: Option<Body>) -> Self {
    self.store = store;
    self
  }

  pub fn with_paths(mut self, paths: Option<Body>) -> Self {
    self.paths = paths;
    self
  }

  pub fn rust(id: impl Into<String>) -> Self {
    Self { id: id.into(), owner: RowOwner::Rust, module: None, export: None, reason: None, body: None, meta: None, store: None, defaults: None, store_keys: None, paths: None }
  }
}

/// An action row.
#[derive(Debug, Clone, PartialEq)]
pub struct ActionEntry {
  pub id: String,
  pub owner: RowOwner,
  pub module: Option<String>,
  pub export: Option<String>,
  pub input: Option<String>,
  pub reason: Option<String>,
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

#[derive(Debug, Clone, PartialEq)]
pub struct RouteEntry {
  pub pattern: String,
  pub plan: Node,
}

/// The serialized shape of a `PlanNode`. Optional fields are absent rather than
/// null, so a plan for a leaf route reads as three keys.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
  pub id: u32,
  pub module: String,
  pub source: Option<String>,
  pub deferred: bool,
  pub fallback: Option<String>,
  pub error: Option<String>,
  /// One error module per failure kind, keyed by `FailureKind::as_str`.
  pub error_kinds: Vec<(String, String)>,
  pub cache_key: Option<String>,
  pub children: Vec<Child>,
  pub keep: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
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
    plan.error_kinds = self
      .error_kinds
      .iter()
      .map(|(kind, raw)| Ok((kind.clone(), module(raw, &format!("{at}/error.{kind}"))?)))
      .collect::<Result<_, PlanError>>()?;
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
      error_kinds: plan.error_kinds.iter().map(|(kind, m)| (kind.clone(), m.to_string())).collect(),
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
    Self { version: FORMAT_VERSION, routes, sources: Vec::new(), actions: Vec::new(), components: Vec::new(), clients: Vec::new(), consts: Consts::new(), not_found: None, handlers: Vec::new(), middleware: None, intercepts: Vec::new(), frameworks: Default::default(), reads: Default::default(), derives: Vec::new(), heads: Vec::new(), sheets: Vec::new(), images: Vec::new(), theme: None, themes: Vec::new() }
  }

  /// The constants bodies read by name.
  pub fn with_consts(mut self, consts: Consts) -> Self {
    self.consts = consts;
    self
  }

  /// The exact version of each vendored framework, by package.
  pub fn with_frameworks(mut self, frameworks: std::collections::BTreeMap<String, String>) -> Self {
    self.frameworks = frameworks;
    self
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

  pub fn with_theme(mut self, theme: Option<Body>, themes: Vec<ThemeEntry>) -> Self {
    self.theme = theme;
    self.themes = themes;
    self
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

  pub fn with_heads(mut self, heads: Vec<ModuleHead>) -> Self {
    self.heads = heads;
    self
  }

  pub fn with_sheets(mut self, sheets: Vec<ModuleSheets>) -> Self {
    self.sheets = sheets;
    self
  }

  pub fn with_images(mut self, images: Vec<ImageEntry>) -> Self {
    self.images = images;
    self
  }

  pub fn with_clients(mut self, clients: Vec<ClientEntry>) -> Self {
    self.clients = clients;
    self
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

/// Gives every node of a tree a fresh id in tree order, so a tree assembled
/// from two plans has no id twice.
pub fn renumber(node: &mut Node, next: &mut u32) {
  node.id = *next;
  *next += 1;
  for child in &mut node.children {
    renumber(&mut child.node, next);
  }
}

/// `at` joined with a pattern: `/` under `/billing` is `/billing`, `/x` is `/billing/x`.
pub fn under(at: &str, pattern: &str) -> String {
  let at = at.trim_end_matches('/');
  if pattern == "/" {
    at.to_owned()
  } else {
    format!("{at}{pattern}")
  }
}

impl Manifest {
  /// Calls `f` on every expression the plan holds, to change it: each row's bodies, every component, the constants, the middleware and the theme.
  pub fn visit_exprs_mut(&mut self, f: &mut dyn FnMut(&mut snapfire_fsr_ir::ast::Expr)) {
    use snapfire_fsr_ir::ast::body_visit_mut;
    for row in &mut self.sources {
      for body in [&mut row.body, &mut row.meta, &mut row.store, &mut row.defaults, &mut row.paths].into_iter().flatten() {
        body_visit_mut(body, f);
      }
    }
    for body in self.actions.iter_mut().filter_map(|row| row.body.as_mut()).chain(self.handlers.iter_mut().filter_map(|row| row.body.as_mut())) {
      body_visit_mut(body, f);
    }
    for body in self.images.iter_mut().filter_map(|row| row.images.as_mut()).chain(self.middleware.as_mut()).chain(self.theme.as_mut()) {
      body_visit_mut(body, f);
    }
    for row in &mut self.components {
      row.body.visit_mut(f);
    }
    for expr in self.derives.iter_mut().filter_map(|row| row.body.as_mut()).chain(self.consts.values_mut()) {
      expr.visit_mut(f);
    }
  }
}

#[cfg(test)]
mod visit_tests {
  use super::*;

  #[test]
  fn the_walk_reaches_the_expressions_of_every_kind_of_body() {
    let text = "(plan 2)
(middleware (ret (native m middleware)))
(source a lowered (body (ret (native m source))) (meta (ret (native m meta))))
(action b lowered (body (ret (native m action))))
(handler c.GET GET /c lowered (body (ret (native m handler))))
(const k (native m const))
(derive \"d\" (sources) (native m derive))
";
    let mut manifest = Manifest::from_text(text).unwrap();
    let mut reached = Vec::new();
    manifest.visit_exprs_mut(&mut |e| {
      if let snapfire_fsr_ir::ast::Expr::NativeCall { method, .. } = e {
        reached.push(method.clone());
      }
    });
    reached.sort();
    assert_eq!(reached, ["action", "const", "derive", "handler", "meta", "middleware", "source"]);
  }
}

#[cfg(test)]
mod namespace_tests {
  use super::*;

  #[test]
  fn every_id_carries_the_name_and_every_pattern_sits_under_at() {
    let manifest = Manifest::from_text(
      "(plan 2)
(route /
  (node 0 shell#document
    (slot content
      (node 1 routes/layout.tsx#default (source layout) (cache-key routes/layout.tsx#default)
        (slot content
          (node 2 routes/index/page.tsx#default (source index) (deferred)
            (fallback routes/index/loading.tsx#default) (error routes/error.tsx#default)))))))
(middleware (ret (call ledger ping)))
(source index lowered (module routes/index/page.loader.ts) (body (ret (call ledger list))))
(action index.add lowered (module routes/index/actions.ts) (input Add) (body (ret (const src/content.ts#CHAPTERS))))
(handler api.x.GET GET /api/x lowered (module routes/api/x/route.ts) (body))
(const src/content.ts#CHAPTERS (arr))
(component routes/index/page.tsx#default (owner react) (render (island src/ui/Tips.tsx#TipList 0 \"load\" nil ())))
",
    )
    .unwrap();
    let named = manifest.namespaced("billing", "/billing", "shell#document");
    let route = &named.routes[0];
    assert_eq!(route.pattern, "/billing");
    assert_eq!(route.plan.module, "shell#document");
    let layout = &route.plan.children[0].node;
    assert_eq!(layout.module, "billing:routes/layout.tsx#default");
    assert_eq!(layout.source.as_deref(), Some("billing:layout"));
    assert_eq!(layout.cache_key.as_deref(), Some("billing:routes/layout.tsx#default"));
    let page = &layout.children[0].node;
    assert_eq!(page.fallback.as_deref(), Some("billing:routes/index/loading.tsx#default"));
    assert_eq!(page.error.as_deref(), Some("billing:routes/error.tsx#default"));
    assert_eq!(named.sources[0].id, "billing:index");
    assert_eq!(named.actions[0].id, "billing:index.add");
    assert_eq!(named.actions[0].input.as_deref(), Some("billing:Add"));
    assert_eq!(named.handlers[0].id, "billing:api.x.GET");
    assert_eq!(named.handlers[0].pattern, "/billing/api/x");
    assert!(matches!(&named.components[0].body.render, snapfire_fsr_ir::ast::Tmpl::Island { module, .. } if module == "billing:src/ui/Tips.tsx#TipList"));
    assert!(named.consts.contains_key("billing:src/content.ts#CHAPTERS"));
    let text = named.to_sexpr();
    for expected in ["(call billing:ledger list)", "(call billing:ledger ping)", "(const billing:src/content.ts#CHAPTERS)"] {
      assert!(text.contains(expected), "missing {expected} in {text}");
    }
    assert!(!text.contains("(call ledger"), "{text}");
    assert_eq!(under("/billing", "/"), "/billing");
    assert_eq!(under("/billing/", "/x/{id}"), "/billing/x/{id}");
  }
}
