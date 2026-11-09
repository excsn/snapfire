# API Reference: snapfire_fsr

Binds a plan file and Rust registrations into an `App` or refuses with a `BindError` naming what is unanswered or claimed twice.

## Contents

* [1. The Application](#1-the-application)
  * [`App`](#app)
  * [`Intercepts`](#intercepts)
  * [`Handlers`](#handlers)
  * [`Images`](#images)
  * [`ListedImage` and `listed_images`](#listedimage-and-listed_images)
* [2. Building](#2-building)
  * [`AppBuilder`](#appbuilder)
  * [`middleware_from`](#middleware_from)
* [3. Routes and Plans](#3-routes-and-plans)
  * [`Routes`](#routes)
  * [`Plan`](#plan)
  * [`IntoPlan`](#intoplan)
* [4. The Report](#4-the-report)
  * [`Owner`](#owner)
  * [`Report`](#report)
* [5. Re-exports](#5-re-exports)
  * [`ext`](#ext)
* [6. Error Handling](#6-error-handling)
  * [`BindError`](#binderror)

## 1. The Application

### `App`

Everything a request needs, plus what was bound to produce it. `Debug`. `#[non_exhaustive]`.

Fields:

* `pub matcher: PathMatcher`
* `pub resolver: TableResolver`
* `pub handlers: Handlers`: the route handlers.
* `pub images: Images`: the generated card images.
* `pub middleware: Option<Arc<dyn ActionHandler>>`: runs before every request that is not a static file, with `{ method, path }` as its input.
* `pub not_found: Option<PlanNode>`: rendered with status 404 for a path the matcher does not match.
* `pub intercepts: Intercepts`: the plan file's intercept trees, one per `page.<slot>.tsx`, under their route's pattern.
* `pub lowered: Option<Arc<IrEvaluator>>`: the lowered components and their interpreter. `None` when nothing was lowered.
* `pub prerenderable: Vec<String>`: patterns whose every source is lowered and reads nothing of the request, in route order.
* `pub prerenderable_anonymous: Vec<String>`: the other patterns whose every source is lowered and whose only request reads are the identity, in route order.
* `pub paths: HashMap<String, Arc<dyn Paths>>`: by pattern, the `IrPaths` of every route with a parameter whose lowered page loader exports `paths`.
* `pub patterns: Vec<String>`: every route's pattern, indexed by the `EntryId` the matcher answers with.
* `pub renderable: Vec<(String, PlanNode)>`: by route pattern, the subtrees a build can render ahead of any request.
* `pub warmable: Vec<String>`: the lowered sources one load serves for every request or every anonymous one, sorted.
* `pub runtime: Arc<Runtime>`
* `pub services: Arc<Services>`
* `pub natives: Arc<snapfire_fsr_runtime::Natives>`: the application's Rust modules, by the name a body reaches as `ctx.native.<name>`.
* `pub actions: ActionRegistry`
* `pub islands: snapfire_fsr_runtime::IslandRegistry`: the handlers of every island a template renders, by module and name.
* `pub contract: Option<Arc<Contract>>`: the contract the plan's type names resolve through.
* `pub declared_inputs: HashMap<String, String>`: each lowered action's or handler's declared input type, by id.
* `pub report: Report`

Classification constraints:

* A pattern with a parameter is in `prerenderable` or `prerenderable_anonymous` only when `paths` holds it.
* An identity read is `identity`, a page or layout's `identity` prop or a call to a service named with `AppBuilder::bearer_services`. A route that reads `csrf_token` is in neither list.
* A lowered `meta` that reads only the identity classes its route the same way. One that reads more keeps its route out of both lists.
* `warmable` uses the same classification per source. A source that reads `path` or a route parameter is excluded.
* `renderable` covers routes not in `prerenderable` that have no parameter or have `paths`.
* A `renderable` subtree is an outermost node with a `cache_key`, whose `SubtreeReads` class is `Fixed`, with no deferred descendant, that reads no store key a source outside the subtree seeds.

Methods:

* `pub fn builder(routes: Routes) -> AppBuilder`: from routes alone, with no plan file.
* `pub fn from_manifest(manifest: &str) -> Result<AppBuilder, BindError>`: from the plan file's text, with every lowered row bound as the default.
  * A plan naming a `react`, `vue` or `svelte` major the renderer has no rules for is `BindError::React`, `Vue` or `Svelte`.
  * A lowered source's `meta` is registered as an `IrMeta` under the source id. Its `paths` is registered as an `IrPaths` under every route pattern whose plan names the source. Both apply only when the source binds as lowered.
* `pub async fn invalidate(&self, plan_key: &str) -> usize`: drops every cached subtree under the plan `cache_key` and returns how many.
* `pub fn conform_text_input(&self, id: &str, input: &mut Value)`: reads the input of the action or handler `id` against its declared type, for an encoding that carries strings only.
  * Does nothing when `id` has no declared input or there is no contract.
  * A value that arrived typed must not be passed through it.
* `pub fn lowered_handler(&self, body: impl Into<snapfire_fsr_ir::PreparedBody>) -> Arc<dyn ActionHandler>`: `body` as a handler on this application's constants, extensions and catalogs, every mounted site's included. It runs compiled when one of the builder's compiled bodies was written from its plan.

### `Intercepts`

The plan file's intercept trees, matched on their route's pattern. `Default`.

* `pub fn all(&self) -> impl Iterator<Item = &PlanNode>`: every intercept plan, in no particular order.
* `pub fn plans_for(&self, path: &str) -> Option<(Vec<PlanNode>, Params)>`: every intercept of the route `path` matches, in file order, with the matched params. `None` when none matches.

### `Handlers`

The route handlers a host dispatches before matching a page. `Default`.

* `pub fn match_request(&self, method: &str, path: &str) -> Option<HandlerMatch>`: the handler id and the pattern's parameters. The method is matched case-insensitively.
* `pub fn allowed(&self, path: &str) -> Vec<String>`: the methods some handler answers at `path`, sorted, with `HEAD` wherever `GET` is. Empty when no handler claims the path.
* `pub fn dispatch(&self, id: &str, ctx: RequestCtx, input: Value) -> BoxFuture<'static, Result<Value, ActionError>>`
* `pub fn ids(&self) -> Vec<String>`
* `pub fn is_empty(&self) -> bool`

### `Images`

The generated card images a host answers, from the plan's `image` rows. `Default`.

* `pub fn match_path(&self, path: &str) -> Option<(&ImageEntry, Params)>`: the image `path` names and the page's parameters. A listed image's id is under `IMAGE_PARAM`.
* `pub fn entries(&self) -> &[ImageEntry]`
* `pub fn listing(&self, pattern: &str) -> Option<&PreparedBody>`: the lowered `images` body of the entry answering `pattern`. `None` for an entry that lists no images.
* `pub fn is_empty(&self) -> bool`
* For every entry with `images`, `build` registers a `HeadRows` under the entry's page module. It writes one `og:image` or `twitter:image` group per listed image, with the URL `<path>/<stem>/<id>?v=<hash>`.

### `ListedImage` and `listed_images`

One image a card module's `images` body lists. `Debug`, `Clone`, `PartialEq`. `#[non_exhaustive]`.

* `pub struct ListedImage { pub id: String, pub alt: Option<String>, pub width: u32, pub height: u32, pub content_type: String }`
* `pub async fn listed_images(interpreter: &Interpreter, entry: &ImageEntry, listing: Option<&PreparedBody>, ctx: &RequestCtx) -> Result<Vec<ListedImage>, String>`: what `listing` returns for `ctx`. Empty when `listing` is `None`.
  * The body must return a list of objects.
  * `id` is a non-empty string or number with no `/`.
  * `size` is `{ width, height }` in whole numbers. `contentType` is `image/png` or `image/jpeg`.
  * `size`, `contentType` and `alt` fall back to the entry's.

## 2. Building

### `AppBuilder`

Every registration method takes and returns the builder.

* Evaluators are looked up in registration order.
* For one name the last claim wins. A plain claim on a lowered name is refused at `build`.

Data sources and metadata:

* `pub fn source<F, Fut>(self, name: impl Into<String>, f: F) -> Self` where `F: Fn(RequestCtx) -> Fut + Send + Sync + 'static`, `Fut: Future<Output = Result<Data, LoadError>> + Send + 'static`: claims `name` as `Rust`.
* `pub fn source_override<F, Fut>(self, name: impl Into<String>, f: F) -> Self`, same bounds: claims `name` as `RustOverride`. `build` refuses it when the plan names no such source.
* `pub fn source_impl(self, name: impl Into<String>, source: Arc<dyn DataSource>) -> Self`: claims `name` as `Rust`.
* `pub fn meta(self, name: impl Into<String>, meta: Arc<dyn Metadata>) -> Self`: the title and description of the segment whose data source is `name`, once its data has loaded. Replaces a lowered `meta` under the same name.

Actions, handlers and middleware:

* `pub fn action<F, Fut>(self, id: impl Into<String>, f: F) -> Self` where `F: Fn(RequestCtx, Value) -> Fut + Send + Sync + 'static`, `Fut: Future<Output = Result<Value, ActionError>> + Send + 'static`
* `pub fn action_override<F, Fut>(self, id: impl Into<String>, f: F) -> Self`, same bounds: `build` refuses it when the plan lowers no such action.
* `pub fn action_impl(self, id: impl Into<String>, handler: Arc<dyn ActionHandler>) -> Self`
* `pub fn handler<F, Fut>(self, method: impl Into<String>, pattern: impl Into<String>, f: F) -> Self`, same bounds as `action`: a Rust handler for `METHOD pattern`, reported under that name. `HandlerClaimed` when the plan lowers the same pair.
* `pub fn handler_override<F, Fut>(self, method: impl Into<String>, pattern: impl Into<String>, f: F) -> Self`: replaces the lowered handler for the pair. `HandlerOverridesNothing` when there is none.
* `pub fn handler_impl(self, method: impl Into<String>, pattern: impl Into<String>, handler: Arc<dyn ActionHandler>) -> Self`
* `pub fn middleware<F, Fut>(self, f: F) -> Self`, same bounds as `action`: `MiddlewareClaimed` when the plan lowers middleware.
* `pub fn middleware_override<F, Fut>(self, f: F) -> Self`: replaces the lowered middleware. `MiddlewareOverridesNothing` when there is none.
* `pub fn island_handler<F, Fut>(self, module: impl Into<String>, name: impl Into<String>, f: F) -> Self` where `F: Fn(RequestCtx, IslandEvent) -> Fut + Send + Sync + 'static`, `Fut: Future<Output = Result<Value, ActionError>> + Send + 'static`: one handler of a template island, answering with its next state. `module` is the template the placement names and `name` is the handler `data-sf-on` binds.

Rendering:

* `pub fn evaluator<P>(self, predicate: P, evaluator: Arc<dyn Evaluator>) -> Self` where `P: Fn(&ModuleId) -> bool + Send + Sync + 'static`
* `pub fn covers(&self, module: &ModuleId) -> bool`: whether an evaluator registered so far answers `module`.
* `pub fn frameworks(self, frameworks: snapfire_fsr_ir::Frameworks) -> Self`: the frameworks the application vendors. `from_manifest` reads them from the plan's `frameworks`. With none set, every lowered component is written as plain markup.
* `pub fn cache(self, cache: Arc<dyn NodeCache>) -> Self`
* `pub fn loads(self, loads: Arc<dyn LoadCache>) -> Self`: where a `warmable` source's load is kept. Without one nothing is memoized.
* `pub fn slot_order(self, slot_order: Vec<String>) -> Self`: the slot names that settle a store key two parallel slots both seed, the later winning. `[store] slot_order` in the configuration.

Services, contract and the body environment:

* `pub fn services(self, services: Arc<Services>) -> Self`: default is an empty registry.
* `pub fn contract(self, contract: Contract) -> Self`: required when any lowered action or handler names an input type.
* `pub fn bearer_services(self, services: impl IntoIterator<Item = String>) -> Self`: the services whose calls carry the session's token. A body calling one counts as reading the identity.
* `pub fn native(self, name: impl Into<String>, module: Arc<dyn snapfire_fsr_runtime::Native>) -> Self`: a Rust module a body reaches as `ctx.native.<name>.<method>()`. It takes no contract and passes through no interceptor.
* `pub fn extension<F>(self, name: impl Into<String>, reach: Reach, f: F) -> Self` where `F: Fn(&Ambient, &[Value]) -> Result<Value, Fail> + Send + Sync + 'static`: the Rust half of a native pair. `name` is `module.member` from its `native(..)` declaration under `ext/` and `reach` is what that declaration says. Replaces a standard member of the same name.
* `pub fn extensions(&self) -> &Extensions`: the extensions bound so far, the standard library included.
* `pub fn config(self, config: ValueMap) -> Self`: `[public]` from the deployment's configuration, read through `@snapfire/fsr/config`.
* `pub fn files(self, files: Arc<snapfire_fsr_core::ext::FileRoots>) -> Self`: the directories a body's `file` calls reach.
* `pub fn catalogs(self, catalogs: Arc<Catalogs>) -> Self`: the message catalogs `t` reads, by locale. None by default.
* `pub fn bodies(self, bodies: &'static [snapfire_fsr_ir::native::Native]) -> Self`: the compiled lowered bodies, the `BODIES` of a production build's `generated/bodies.rs`.
  * Each `Native` carries `plan`, the `plan_hash` of the body and constants it was written from.
  * A lowered body runs the compiled function whose `plan` matches its own. Any other runs interpreted.

Routes and plan files:

* `pub fn route(self, pattern: impl Into<String>, plan: impl IntoPlan) -> Self`
* `pub fn route_override(self, pattern: impl Into<String>, plan: impl IntoPlan) -> Self`
* `pub fn not_found(self, plan: impl IntoPlan) -> Self`: `Routes::not_found` on the builder's routes.
* `pub fn mount_manifest(&mut self, manifest: &str) -> Result<(), BindError>`: adds every lowered row of another plan file: its routes and intercepts as plan-file entries, its sources with their metas and stores, its actions, components and handlers.
  * The ids are the file's, so a mounted site's arrive prefixed.
  * Its middleware and not-found tree are not added.

Building:

* `pub fn build(self) -> Result<App, BindError>`
  * Every body is checked for unknown extensions before anything binds (`UnknownExtension`).
  * Binds every lowered source, action and handler not overridden. The lowered components bind under the IR evaluator.
  * A handler row with no body must be answered in Rust (`UnboundHandler`).
  * Every override must name something, every named source and declared action must be answered and every pattern must be one the matcher accepts.
  * A lowered `paths` that reads the request is `PathsReadRequest`. One on a route with no parameter is `PathsWithoutParameter`. One that no route names is `PathsOffRoute`.
  * A lowered action or handler with an input type checks the value against the contract before its body runs and fails as `Invalid`.
  * `Runtime::reads` gets one `SubtreeReads` per subtree of every route, intercept and not-found plan, keyed by `subtree_shape`.
  * A node's class is its source's (`Dynamic` for a Rust source, `Fixed` for none) joined with what its lowered component reads of the `identity`, `csrf_token`, `csp_nonce` and `action_failure` props.
  * A module the app cannot see into is `Dynamic` and counts as reading the token, the nonce and the failure.
  * A subtree's class is the most any node in it reads. Its store keys are those read by each node's component and every component it places, transitively.

### `middleware_from`

* `pub fn middleware_from(body: snapfire_fsr_ir::Body) -> Arc<dyn ActionHandler>`: a lowered middleware body as a handler on an interpreter with the standard extensions alone. It has no constants, catalogs or application extensions.

## 3. Routes and Plans

### `Routes`

Routes from the plan file, from Rust or both. A pattern claimed twice is refused. `Default`.

* `pub fn new() -> Self`
* `pub fn from_manifest(source: &str) -> Result<Self, BindError>`: every route in the file, owned by `PlanFile`, with the file's not-found tree and its intercepts. Their sources count as declared.
* `pub fn extend_manifest(&mut self, source: &str) -> Result<(), BindError>`: adds another plan file's routes and intercepts as plan-file entries. Its not-found tree is not added.
* `pub fn add(self, pattern: impl Into<String>, plan: impl IntoPlan) -> Self`: owned by `Rust`. A plan that fails to convert is kept as the error `build` returns.
* `pub fn replace(self, pattern: impl Into<String>, plan: impl IntoPlan) -> Self`: owned by `RustOverride`, replacing the entry with that pattern.
* `pub fn not_found(self, plan: impl IntoPlan) -> Self`: the tree for a path no route matches, replacing the plan file's. Its sources count as declared.
* `pub fn has_not_found(&self) -> bool`
* `pub fn patterns(&self) -> Vec<&str>`
* `pub fn build(self) -> Result<(PathMatcher, TableResolver), BindError>`: entry ids are positions in file order followed by Rust order.

### `Plan`

A route's plan written the way it reads. Node ids are assigned in tree order on conversion.

* `pub fn of(module: impl Into<String>) -> Self`
* `pub fn source(self, name: impl Into<String>) -> Self`
* `pub fn deferred(self) -> Self`: streams instead of blocking the first response. Pair it with `fallback`.
* `pub fn fallback(self, module: impl Into<String>) -> Self`
* `pub fn error(self, module: impl Into<String>) -> Self`: rendered in place of the node when its loader fails.
* `pub fn error_for(self, kind: impl Into<String>, module: impl Into<String>) -> Self`: rendered in place of `error` when the loader failed with `kind`, spelled as `FailureKind::as_str` spells it. One call per kind.
* `pub fn cache_key(self, key: impl Into<String>) -> Self`
* `pub fn slot(self, name: impl Into<String>, child: Plan) -> Self`
* Every module is `path#export`. Conversion fails with `BindError::Module` otherwise.

### `IntoPlan`

* `pub trait IntoPlan { fn into_plan(self) -> Result<PlanNode, BindError>; }`
* Implemented for `Plan` and for `PlanNode`, which passes through.

## 4. The Report

### `Owner`

Who answers a name. `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`.

* `PlanFile`, `Lowered`, `Rust`, `RustOverride`
* `pub fn as_str(&self) -> &'static str`: `plan file`, `lowered`, `rust`, `rust override`.

### `Report`

What was bound. `Debug`, `Clone`, `Default`, `PartialEq`, `Eq`, `Display`. `#[non_exhaustive]`.

* `pub routes: Vec<(String, Owner)>`: sorted.
* `pub sources: Vec<(String, Owner)>`: in binding order.
* `pub actions: Vec<(String, Owner)>`: in binding order.
* `pub handlers: Vec<(String, Owner)>`: by `METHOD pattern`, sorted.
* `pub middleware: Option<Owner>`
* `pub compiled: Option<Vec<String>>`: the lowered sources, actions, handlers (as `METHOD pattern`) and `middleware` that run a compiled body. `None` when the builder was given no compiled bodies.
* `pub prerenderable: Vec<String>`: `App::prerenderable`.
* `pub prerenderable_anonymous: Vec<String>`: `App::prerenderable_anonymous`.
* `pub paths: Vec<String>`: the patterns `App::paths` holds, in route order.
* `pub warmable: Vec<String>`: `App::warmable`.
* `pub renderable: Vec<(String, String)>`: `App::renderable` as the pattern and the subtree's root module.
* `pub components: Vec<(String, Owner)>`: modules rendered on the server.
* `pub clients: Vec<snapfire_fsr_plan::ClientEntry>`: the plan file's `clients` table, each module the build could not lower with the residue that decided it.
* `pub islands: Vec<String>`: template islands with their handlers, as `<module> <name>`.
* `pub scripts: Vec<(String, Option<String>)>`: every script row a lowered `meta` declares, as the meta's name and its literal `src`. `None` for a `src` computed at request time.
* `pub media: Vec<(String, String)>`: by module, the literal `src` of every `video`, `audio`, `source` and `track` a lowered component writes.
* `pub embeds: Vec<(String, String)>`: by module, each embed placeholder's player URL.
* `pub images: Vec<(String, String)>`: by module, each `img`'s literal `src`.
* `pub styles: Vec<(String, Option<String>)>`: by module, the content of every `<style>` a lowered component writes. `None` where the render computes it.

`Display` prints labelled columns in this order:

* `routes`, `sources`, `actions`, `handlers`, then `middleware` when there is one.
* `rendered`: the lowered modules, then each client module as `client` with its `at`.
* `client`: each distinct residue once with its message. Its hint follows on the next line.
* `islands`, then `scripts`. A computed `src` prints as the instruction to name its origin in `[document.csp] script-src`.
* With `compiled` set, each lowered source, action, handler and middleware row prints `lowered, compiled` or `lowered, interpreted`.

## 5. Re-exports

### `ext`

`snapfire_fsr_core::ext`, re-exported as `ext`, with `Ambient`, `Catalogs`, `Fail`, `FailureKind` and `Reach` at the crate root.

## 6. Error Handling

### `BindError`

`Debug`, `Clone`, `PartialEq`, `Eq`, `std::error::Error`. `#[non_exhaustive]`.

* `Plan(PlanError)`: the plan file did not read. `Display` is the inner error's.
* `Image(String, String)`: a generated image's pattern the matcher refused, with the reason.
* `Claimed(String)`: a source or a route pattern claimed by the plan file and by Rust without an override.
* `ActionClaimed(String)`: an action lowered by the file and bound in Rust without an override.
* `ActionOverridesNothing { id: String }`: an action override the plan has nothing for.
* `Pattern { pattern: String, message: String }`: the matcher refused the pattern.
* `Unbound { name: String }`: a source the plan names and nothing answers.
* `OverridesNothing { name: String }`: a source override the plan has nothing for.
* `Module { module: String }`: not `path#export`.
* `UnboundAction { id: String }`: an action the plan declares and nothing answers.
* `NoContract { id: String, input: String }`: a lowered action or handler names an input type and no contract was passed.
* `UnknownInput { id: String, input: String }`: the contract does not define the type.
* `UnknownExtension { owner: String, name: String }`: a body or component `owner` calls extension `name` and neither the standard library nor an `extension` call registers it.
* `HandlerClaimed(String)`: a handler lowered by the file and bound in Rust without an override. The string is `METHOD pattern`.
* `HandlerOverridesNothing(String)`: a handler override the file has nothing for.
* `UnboundHandler(String)`: a handler row with no body that no Rust handler answers.
* `MiddlewareClaimed`: middleware lowered by the file and bound in Rust without an override.
* `MiddlewareOverridesNothing`: a middleware override when the file lowers none.
* `React { version: String }`: the plan names a React major this fsr writes no markup for.
* `Vue { version: String }`: the plan names a Vue major this fsr writes no markup for.
* `Svelte { version: String }`: the plan names a Svelte major this fsr writes no markup for.
* `PathsReadRequest { loader: String }`: the source's `paths` reads `query`, `session`, `identity`, `now` or the store. Calling a bearer service counts too.
* `PathsWithoutParameter { loader: String, pattern: String }`: the source's `paths` is on a route whose pattern has no parameter.
* `PathsOffRoute { loader: String }`: the source exports `paths` but no route's plan names it.
