# API Reference: snapfire_fsr_runtime

The request pipeline of SnapFire FSR: matching, resolution, data sources, evaluation, assembly, the store, document metadata, caching, the request context, services, actions, middleware, islands and streaming.

## Contents

* [1. Matching](#1-matching)
  * [`EntryId`](#entryid)
  * [`RouteMatch`](#routematch)
  * [`Matcher`](#matcher)
  * [`PathMatcher`](#pathmatcher)
  * [`HandlerMatcher`](#handlermatcher)
  * [`HandlerMatch`](#handlermatch)
  * [`RouteConflict`](#routeconflict)
* [2. Resolution](#2-resolution)
  * [`Resolver`](#resolver)
  * [`TableResolver`](#tableresolver)
* [3. Data Sources](#3-data-sources)
  * [`DataSource`](#datasource)
  * [`DataSources`](#datasources)
  * [`LoadKeyer`](#loadkeyer)
  * [`NoLoadKey`](#noloadkey)
* [4. Evaluation](#4-evaluation)
  * [`Chunk`](#chunk)
  * [`NodeChunks`](#nodechunks)
  * [`Evaluator`](#evaluator)
  * [`NullEvaluator`](#nullevaluator)
  * [`slot_regions`](#slot_regions)
  * [`Evaluators`](#evaluators)
* [5. The Runtime](#5-the-runtime)
  * [`Runtime`](#runtime)
  * [`RuntimeBuilder`](#runtimebuilder)
  * [`Static`](#static)
  * [`SubtreeReads`](#subtreereads)
  * [`Reads`](#reads)
  * [`subtree_shape`](#subtree_shape)
* [6. Assembly](#6-assembly)
  * [`assemble`](#assemble)
  * [`assemble_under`](#assemble_under)
  * [`Origin`](#origin)
  * [`Assembly`](#assembly)
  * [`PendingResolution`](#pendingresolution)
  * [`Resolved`](#resolved)
* [7. The Store](#7-the-store)
  * [`Seeds`](#seeds)
  * [`Contribution`](#contribution)
  * [`contribution_order`](#contribution_order)
  * [`Derived`](#derived)
  * [`PENDING_PROP`](#pending_prop)
* [8. Document Metadata](#8-document-metadata)
  * [`Meta`](#meta)
  * [`HeadEl`](#headel)
  * [`Metadata`](#metadata)
  * [`HeadRows`](#headrows)
  * [`Paths`](#paths)
  * [`Head`](#head)
  * [`DocumentTheme`](#documenttheme)
  * [Head constants](#head-constants)
* [9. Segments](#9-segments)
  * [`SegmentKeyer`](#segmentkeyer)
  * [`DefaultKeyer`](#defaultkeyer)
  * [`SegmentInfo`](#segmentinfo)
* [10. Caching](#10-caching)
  * [`CacheEntry`](#cacheentry)
  * [`NodeCache`](#nodecache)
  * [`NoCache`](#nocache)
  * [`MemoryCache`](#memorycache)
  * [`FibreCache`](#fibrecache)
  * [`WarmRenders`](#warmrenders)
  * [`LoadCache`](#loadcache)
  * [`NoLoadCache`](#noloadcache)
  * [`WarmLoads`](#warmloads)
  * [`MemoryLoadCache`](#memoryloadcache)
* [11. Request Context](#11-request-context)
  * [`RequestCtx`](#requestctx)
  * [`Identity`](#identity)
  * [`SessionCell`](#sessioncell)
  * [`CsrfHandle`](#csrfhandle)
  * [`RedirectHandle`](#redirecthandle)
  * [`AfterHandle`](#afterhandle)
  * [`Address`](#address)
  * [`Locale`](#locale)
  * [Context functions](#context-functions)
* [12. Services](#12-services)
  * [`ServiceCaller`](#servicecaller)
  * [`ServiceHandle`](#servicehandle)
  * [`Native`](#native)
  * [`Natives`](#natives)
  * [`NativeHandle`](#nativehandle)
  * [`FromNativeValue`](#fromnativevalue)
  * [`CallState`](#callstate)
* [13. Actions](#13-actions)
  * [`ActionHandler`](#actionhandler)
  * [`ActionRegistry`](#actionregistry)
  * [`HttpResponse`](#httpresponse)
* [14. Middleware](#14-middleware)
  * [`NextHandle`](#nexthandle)
  * [`Target`](#target)
  * [`Passed`](#passed)
  * [`drive_middleware`](#drive_middleware)
* [15. Template Islands](#15-template-islands)
  * [`IslandEvent`](#islandevent)
  * [`IslandHandler`](#islandhandler)
  * [`IslandRegistry`](#islandregistry)
  * [`island_data`](#island_data)
* [16. Streaming](#16-streaming)
  * [`wire_stream`](#wire_stream)
  * [`html_stream`](#html_stream)
  * [`fragment_html`](#fragment_html)
  * [`meta_to_json`](#meta_to_json)
  * [`contributions_to_json`](#contributions_to_json)
  * [`segments_to_json`](#segments_to_json)
  * [`segments_from_json`](#segments_from_json)
  * [`FILL_SCRIPT`](#fill_script)
  * [`CAPTURE_SCRIPT`](#capture_script)
* [17. Reading a Payload](#17-reading-a-payload)
  * [`Row`](#row)
  * [`row_from_line`](#row_from_line)
  * [`RowReader`](#rowreader)
  * [`contributions_from_json`](#contributions_from_json)
  * [`meta_from_json`](#meta_from_json)
* [18. Error Handling](#18-error-handling)
  * [`FailureKind`](#failurekind)
  * [`LoadError`](#loaderror)
  * [`EvalError`](#evalerror)
  * [`AssembleError`](#assembleerror)
  * [`ActionError`](#actionerror)
  * [`ServiceError`](#serviceerror)

## 1. Matching

### EntryId

The application's stable id for a route entry.

* `pub struct EntryId(pub u32)`. `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Hash`.

### RouteMatch

What a matcher returns. `Debug`, `Clone`, `PartialEq`, `#[non_exhaustive]`.

* `pub struct RouteMatch { pub entry: EntryId, pub params: Params }`: `Params` is `snapfire_fsr_core`'s `IndexMap<String, String>`.
* `new(entry: EntryId, params: Params) -> Self`

### Matcher

* `pub trait Matcher: Send + Sync { fn match_path(&self, path: &str) -> Option<RouteMatch>; }`: `None` means no route.

### PathMatcher

Route patterns to entries, behind `Matcher`. `Default`.

* `new() -> Self`
* `insert(&mut self, pattern: &str, entry: EntryId) -> Result<(), RouteConflict>`: `pattern` is a path with `{name}` for one segment and `{*name}` for the rest of the path, such as `/dash/{section}`. A conflicting or malformed pattern fails here.
* `match_path` answers named captures as params, in pattern order.

### HandlerMatcher

Route patterns per HTTP method, resolving to a handler id. `Default`. Patterns are written as for `PathMatcher`.

* `new() -> Self`
* `insert(&mut self, method: &str, pattern: &str, id: impl Into<String>) -> Result<(), RouteConflict>`: the method is upper-cased.
* `match_request(&self, method: &str, path: &str) -> Option<HandlerMatch>`: a HEAD with no handler of its own is answered by the GET handler.
* `allowed(&self, path: &str) -> Vec<String>`: the methods some handler answers at `path`, sorted, with HEAD wherever GET is. Empty when no handler claims the path.
* `is_empty(&self) -> bool`

### HandlerMatch

* `pub struct HandlerMatch { pub id: String, pub params: Params }`

### RouteConflict

A pattern `insert` refused. `#[non_exhaustive]`, `Error`. `Display` prints `reason`.

* `pub pattern: String`
* `pub reason: String`

## 2. Resolution

### Resolver

* `pub trait Resolver: Send + Sync { fn resolve(&self, entry: EntryId, params: &Params) -> Option<PlanNode>; }`: `None` means the entry has no plan.

### TableResolver

One prebuilt plan per entry. `Default`.

* `new() -> Self`
* `insert(&mut self, entry: EntryId, plan: PlanNode)`: a second insert for the same entry replaces the first.
* `plans(&self) -> impl Iterator<Item = &PlanNode>`: every plan the table holds.
* `most_deferred(&self) -> u32`: the most deferred segments one document can stream. It equals the highest slot number a document fills.
* `resolve` clones the stored plan and ignores the params.
* Node ids must be distinct within one plan. Loaded data and load failures are keyed by `PlanNode::id`.

## 3. Data Sources

### DataSource

* `pub trait DataSource: Send + Sync { fn load(&self, ctx: &RequestCtx) -> BoxFuture<'static, Result<Data, LoadError>>; }`: `Data` is `ValueMap`. The future is `'static`, so it cannot borrow `ctx`.

### DataSources

Source id to implementation, in insertion order. `Default`.

* `new() -> Self`
* `insert(&mut self, id: impl Into<String>, source: Arc<dyn DataSource>)`
* `insert_fn<F, Fut>(&mut self, id: impl Into<String>, f: F)` where `F: Fn(RequestCtx) -> Fut + Send + Sync + 'static` and `Fut: Future<Output = Result<Data, LoadError>> + Send + 'static`: the closure receives a clone of the context.
* `get(&self, id: &DataSourceId) -> Option<&Arc<dyn DataSource>>`
* A plan node naming an id that was never inserted fails the assembly with `AssembleError::MissingDataSource`. A source that returns `LoadError` degrades only its own segment.

### LoadKeyer

How much of the request a source reads. A load with a key may be answered from the `LoadCache`.

* `pub trait LoadKeyer: Send + Sync { fn key(&self, source: &DataSourceId, ctx: &RequestCtx) -> Option<String>; }`: `None` means never memoize.
* The key must name everything the source reads. A key that leaves something out serves one request's data to another.

### NoLoadKey

* `pub struct NoLoadKey`: the default. `key` is always `None`.

## 4. Evaluation

### Chunk

One item of an evaluator's output stream. `Debug`, `Clone`, `PartialEq`.

* `pub enum Chunk { Node(Node), Slot(SlotName) }`
* `Node`: finished output.
* `Slot`: where a plan child's subtree lands. `SlotName("head")` is the head node passed to `assemble`.

### NodeChunks

* `pub type NodeChunks = BoxStream<'static, Result<Chunk, EvalError>>`

### Evaluator

* `pub trait Evaluator: Send + Sync`
* `fn evaluate(&self, module: &ModuleId, props: &Data) -> NodeChunks`
* `fn reads_csrf(&self, module: &ModuleId) -> bool`: whether rendering `module` can read its `csrf_token` prop. Defaults to `true`.

Props the assembler writes over a node's loaded data. A loader key with one of these names is replaced.

* `params`: always, a `Value::Map` of the matched params. Under `assemble_under`, an origin node's params come from the origin's request.
* `locale`: the locale tag, when the request has one.
* `identity`: `{ subject, claims }`, when the session resolved one and the subtree is not `Fixed`.
* `csrf_token`: a `Value::Str`. Written when the handle is bound, the subtree is `Dynamic`, the subtree reads the token or has no reads entry and the evaluator's `reads_csrf` holds. Reading the handle mints the token.
* `csp_nonce`: when the subtree is `Dynamic` and the context carries a nonce.
* `action_failure`: when the subtree is `Dynamic` and the context carries a failure.
* `$path` and `$document` (`PATH_PROP` and `DOCUMENT_PROP`): when the subtree's reads set `path` or it has no reads entry.
* `$store`: the whole store for a `Dynamic` subtree or one with no reads entry. Otherwise only the keys `SubtreeReads::store_keys` names. Absent when empty.
* `$pending`: the pending store keys the subtree reads, every pending key when its reads are unknown. Absent when empty.
* `$slots`: the node's child slot names and its `keep` names, when it has either.
* An error module receives `params` and the request props. It also receives `error` (the `LoadError` display string) and `kind` (the failure kind's name).
* A fallback module receives the request props only, never loader data.

### NullEvaluator

* `pub struct NullEvaluator`: declines to evaluate.
* `evaluate` emits one `Node::Client` carrying the module and the props, with `rendered: false` and `slot_regions(props)` as its children.

### slot_regions

* `pub fn slot_regions(props: &Data) -> Vec<Node>`: one region per name in `$slots`. `content` is a `Node::Slot` inside a bare `<sf-s>`. Any other name is inside `<sf-s data-sf-name="<name>">`.
* A name that is empty or holds characters outside ASCII letters, digits, `_` and `-` is skipped.

### Evaluators

Module-to-evaluator dispatch. `Default` holds no rules.

* `new() -> Self`
* `register(&mut self, applies: impl Fn(&ModuleId) -> bool + Send + Sync + 'static, evaluator: Arc<dyn Evaluator>)`
* `covers(&self, module: &ModuleId) -> bool`: whether a registered rule answers the module.
* `select(&self, module: &ModuleId) -> &dyn Evaluator`: the first rule whose predicate holds, in registration order. With no match, the built-in `NullEvaluator`.

## 5. The Runtime

### Runtime

The pipeline shared across requests. `#[non_exhaustive]`.

* `pub sources: DataSources`
* `pub evaluators: Evaluators`
* `pub keyer: Arc<dyn SegmentKeyer>`
* `pub cache: Arc<dyn NodeCache>`
* `pub load_keyer: Arc<dyn LoadKeyer>`
* `pub loads: Arc<dyn LoadCache>`
* `pub metas: HashMap<String, Arc<dyn Metadata>>`: by data source id.
* `pub slot_order: Vec<String>`: the application's `[store] slot_order`.
* `pub heads: HashMap<String, Vec<HeadEl>>`: by module id, the head elements a page rendering that module carries. They merge into a page's `Meta` after its segments describe it, outermost module first. `PATH_MARK` in a value is replaced by the request's path without a trailing slash.
* `pub listed: HashMap<String, Vec<Arc<dyn HeadRows>>>`: by module id, the head rows a module's listed card images make per request, applied after `heads`.
* `pub stores: HashMap<String, Arc<dyn Seeds>>`: by data source id.
* `pub reads: Reads`: by `subtree_shape`. A subtree with no entry reads everything.
* `pub derives: Vec<Derived>`: in `derive_order`.
* `builder() -> RuntimeBuilder`
* `new(sources: DataSources, evaluators: Evaluators) -> Arc<Self>`: the builder's defaults otherwise.
* `with_keyer(sources: DataSources, evaluators: Evaluators, keyer: Arc<dyn SegmentKeyer>) -> Arc<Self>`

### RuntimeBuilder

From `Runtime::builder()`. Each method takes and returns `self`.

* `sources(self, sources: DataSources) -> Self`
* `evaluators(self, evaluators: Evaluators) -> Self`
* `keyer(self, keyer: Arc<dyn SegmentKeyer>) -> Self`
* `cache(self, cache: Arc<dyn NodeCache>) -> Self`
* `load_keyer(self, keyer: Arc<dyn LoadKeyer>) -> Self`
* `loads(self, loads: Arc<dyn LoadCache>) -> Self`
* `meta(self, source_id: impl Into<String>, meta: Arc<dyn Metadata>) -> Self`
* `heads(self, heads: HashMap<String, Vec<HeadEl>>) -> Self`
* `listed(self, module: impl Into<String>, rows: Arc<dyn HeadRows>) -> Self`: adds a lister. A module can have several.
* `store(self, source_id: impl Into<String>, seeds: Arc<dyn Seeds>) -> Self`
* `slot_order(self, slot_order: Vec<String>) -> Self`
* `derives(self, derives: Vec<Derived>) -> Self`: kept in `derive_order`. Every render applies them after the contributions merge and adds them to the pending keys by `derived_pending`.
* `reads(self, reads: Reads) -> Self`
* `build(self) -> Arc<Runtime>`
* Defaults: `DataSources::new()`, `Evaluators::new()`, `DefaultKeyer`, `NoCache`, `NoLoadKey`, `NoLoadCache`, no metadata, head rows, listers, seeds or derives, an empty slot order and an empty `Reads`.

### Static

How much of the request a body or a subtree depends on.

* `pub enum Static { Fixed, Anonymous, Dynamic }`. `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `PartialOrd`, `Ord`, `Default` (`Dynamic`).
* `Fixed`: nothing. `Anonymous`: the identity alone. `Dynamic`: more.

### SubtreeReads

What a subtree reads of the request. `Debug`, `Clone`, `PartialEq`, `Eq`, `Default`, `#[non_exhaustive]`.

* `pub struct SubtreeReads { pub class: Static, pub store_keys: Vec<String>, pub path: bool, pub csrf: bool, pub nonce: bool, pub failure: bool }`
* `class`: the most any node in it reads.
* `store_keys`: the store keys any component in it reads.
* `path`: a component in it renders the request's path or the document's.
* `csrf`: a component in it reads its `csrf_token` prop.
* `nonce`: a component in it reads its `csp_nonce` prop.
* `failure`: a component in it reads its `action_failure` prop.
* Setters, each `fn(self, value) -> Self`: `class`, `store_keys`, `path`, `csrf`, `nonce` and `failure`.
* `pub const PATH_PROP: &str = "$path"` and `pub const DOCUMENT_PROP: &str = "$document"`: the props the two paths ride in.

### Reads

* `pub type Reads = HashMap<u64, SubtreeReads>`: keyed by `subtree_shape`.

### subtree_shape

* `pub fn subtree_shape(node: &PlanNode) -> u64`: xxh3 over the module names and slot names of the subtree in plan order.

## 6. Assembly

### assemble

```rust
pub async fn assemble(
  runtime: &Arc<Runtime>,
  plan: &PlanNode,
  ctx: &RequestCtx,
  head: impl Into<Head>,
) -> Result<Assembly, AssembleError>
```

`head` is a `Head`, a `Node`, a `&Node` or a `&Head`.

Loading:

* Every eager data source resolves before evaluation begins. Eager sources are those of the root and every descendant, stopping at any non-root node with `deferred` set. They run concurrently.
* A missing registration fails with `AssembleError::MissingDataSource`.
* A `LoadError` is recorded against its node id and the wave goes on.
* A node with a recorded failure renders the module its `error_kinds` names for that kind, else its `error` module, else a built-in error node. Its children are not built.
* The route's own page is the node reached from the root along `content` slots. When its loader fails, `Assembly::failed` holds the kind.

Metadata:

* After the eager wave, the innermost node whose data source has a `Metadata` registered and whose data loaded describes the document. Deferred children are excluded.
* A `describe` failure is logged on target `fsr::load` and leaves the defaults.
* The result, filled in from `head`, is `Assembly::meta`. A deferred subtree describes itself the same way in `Resolved::meta`.

Slots:

* `Chunk::Slot(SlotName("head"))` is answered with `head.node(&meta)`. A subtree that answers it is never written to the render memo.
* Any other slot name must match a child in `PlanNode::children`, else `AssembleError::MissingSlot`. A named slot the plan leaves unfilled renders nothing. An unfilled `content` slot is an error unless the node keeps it.
* A `Node::Slot` anywhere under a `Chunk::Node` is answered in place the same way.

Seeding:

* Every node whose data source has `Seeds` registered contributes what its `seed` returns as one `Contribution`. Deferred children are excluded.
* The wave renders from the merge of its contributions by `contribution_order` under `Runtime::slot_order`.
* A failing seed is logged on target `fsr::load`. The segment then contributes empty values.
* Every seeding node under a deferred child whose `Seeds::keys` is known contributes a promise for those keys. A resolution that fails writes an empty seed for each.

Deferral:

* A child with `deferred` set gets a `SlotId` from a counter starting at 1, unique per response.
* Its `fallback` module is evaluated with the request props and the wave's store. With no fallback it is `Node::raw("")`.
* `Node::Pending { slot, fallback }` goes into the tree and a `PendingResolution` into `Assembly::pending`.
* A resolution renders from the wave's contributions plus its own segments', merged by the same rule. `Resolved::contributions` carries its own.

Collapse:

* A node whose evaluator emitted exactly one chunk becomes that node. Otherwise it becomes `Node::Seq` and each non-deferred child segment records `path: [index]`.

Render memo key, composed per plan node that carries a `cache_key`:

```text
{cache_key}|{params}|ident={subject}|csrf={token}|locale={tag}|path={path}|{shape:016x}|{data:016x}|{store:016x}|pending={keys}
```

* `params`: every entry of the node's request params as `k=v`, sorted, joined by `&`.
* `subject`: the session's subject when the subtree is `Anonymous` or `Dynamic` and one resolved. `-` otherwise.
* `token`: `ctx.csrf.memo_key()` when the subtree is `Dynamic` and reads `csrf_token` or has no reads entry. `-` otherwise. A `memo_key` of `None` composes no key.
* `tag`: `ctx.locale.tag`.
* `path`: `{ctx.path}|doc={document}` when the subtree reads the path or has no reads entry, `document` being `-` when absent. `-` otherwise.
* `shape`: `subtree_shape`.
* `data`: xxh3 over the subtree's loaded data, in tree order.
* `store`: the store's fingerprint, whole for a `Dynamic` subtree or one with no reads entry, else over the keys `store_keys` names.
* `keys`: the pending keys the subtree reads, joined by `,`.
* No key is composed when the node has no `cache_key` or when the runtime's `NodeCache::keeps` is false.
* No key is composed when the subtree holds a `deferred` descendant or a recorded load failure.
* No key is composed for a `Dynamic` subtree that reads a CSP nonce or an action failure the context carries. A subtree with no reads entry counts as reading both.

### assemble_under

```rust
pub async fn assemble_under(
  runtime: &Arc<Runtime>,
  plan: &PlanNode,
  ctx: &RequestCtx,
  head: impl Into<Head>,
  origin: Origin,
) -> Result<Assembly, AssembleError>
```

* `assemble` for an intercepted render.
* The nodes `origin` names load, seed, describe, take their segment key and take their `params` prop from `origin.ctx`, the document's request.
* Every other node takes these from `ctx`, the navigation's request. Every node's `$path` and `$document` come from `ctx` too.

### Origin

* `pub struct Origin { pub ctx: RequestCtx, pub nodes: Vec<u32> }`: the document's request and the ids of the plan nodes that render under it. `#[non_exhaustive]`.
* `new(ctx: RequestCtx, nodes: Vec<u32>) -> Self`

### Assembly

What one call produced. `#[non_exhaustive]`. `Debug` prints `tree`, `segments` and `pending` as a count. Not `Clone`.

* `pub tree: Node`
* `pub pending: Vec<PendingResolution>`
* `pub segments: SegmentInfo`: the root sidecar.
* `pub meta: Meta`: the title and description the eager wave settled on, the head's defaults included.
* `pub rows: Vec<HeadEl>`: every head row the document carries under `meta`.
* `pub contributions: Vec<Contribution>`: what each eager segment seeded.
* `pub store: Data`: `contributions` merged.
* `pub locale: Locale`
* `pub entry: Option<String>`: the head's `entry`.
* `pub styles: Vec<String>`: the head's `styles`.
* `pub catalog: Option<String>`: the head's `catalog`.
* `pub theme: Option<DocumentTheme>`: the head's `theme`.
* `pub failed: Option<FailureKind>`: the kind the route's own page loader failed with. A layout or a slot failing leaves it `None`.
* `new(tree: Node, pending: Vec<PendingResolution>, segments: SegmentInfo, meta: Meta, contributions: Vec<Contribution>, store: Data, locale: Locale, entry: Option<String>, styles: Vec<String>, catalog: Option<String>, theme: Option<DocumentTheme>, failed: Option<FailureKind>) -> Self`: `rows` empty.

### PendingResolution

A deferred slot's eventual content. `#[non_exhaustive]`.

* `pub slot: SlotId`
* `pub key: String`: the deferred segment's key.
* `pub future: BoxFuture<'static, Resolved>`: never fails. A failed loader or evaluation inside resolves to the segment's error node.
* `new(slot: SlotId, key: String, future: BoxFuture<'static, Resolved>) -> Self`

### Resolved

`#[non_exhaustive]`.

* `pub slot: SlotId`
* `pub key: String`
* `pub node: Node`
* `pub segments: Vec<SegmentInfo>`: the child segments of the resolved subtree, positioned in `node`.
* `pub pending: Vec<PendingResolution>`: new slots the resolution introduced.
* `pub meta: Meta`: what the subtree said about the document. Empty when no segment in it has metadata.
* `pub rows: Vec<HeadEl>`: every head row of the document with this subtree's metadata in. Empty when it has none.
* `pub contributions: Vec<Contribution>`: what the subtree's own segments seeded.
* `new(slot: SlotId, key: String, node: Node, segments: Vec<SegmentInfo>, pending: Vec<PendingResolution>, meta: Meta, contributions: Vec<Contribution>) -> Self`: `rows` empty.

## 7. The Store

### Seeds

What a segment seeds the browser's store with. Registered under a data source id.

* `pub trait Seeds: Send + Sync`
* `fn seed(&self, ctx: &RequestCtx, data: &Data) -> BoxFuture<'static, Result<Data, LoadError>>`
* `fn defaults(&self, ctx: &RequestCtx, data: &Data) -> BoxFuture<'static, Result<Data, LoadError>>`: values laid under every segment's seed in the merge and, in the browser, under a write an island holds. Defaults to empty.
* `fn keys(&self) -> Option<Vec<String>>`: every key a seed can hold, before the data is known. Defaults to `None`. A deferred segment with no keys promises nothing.

### Contribution

What one segment seeded. `Debug`, `Clone`, `PartialEq`, `#[non_exhaustive]`.

* `pub struct Contribution { pub segment: String, pub path: Vec<String>, pub values: Data, pub defaults: Data, pub awaits: Vec<String> }`
* `segment`: the segment's key.
* `path`: the slot names from the route's root down to the segment.
* `awaits`: on a promise, the keys a deferred segment will seed. A promise has no `values`.
* `seeded(segment: impl Into<String>, path: Vec<String>, values: Data) -> Self`
* `promised(segment: impl Into<String>, path: Vec<String>, awaits: Vec<String>) -> Self`
* `with_defaults(self, defaults: Data) -> Self`

### contribution_order

* `pub fn contribution_order(a: &[String], b: &[String], slot_order: &[String]) -> Ordering`: by depth, deeper later. At one depth, by the first slot name the paths differ on.
* A slot name `slot_order` lists comes after every name it does not list. Of two listed names, the later listed one is later. Unlisted names compare as strings.
* `pub fn merge_contributions(contributions: &[Contribution], slot_order: &[String]) -> Data`: the store they merge to, each applied in `contribution_order`.
* `pub fn pending_keys(contributions: &[Contribution], slot_order: &[String]) -> BTreeSet<String>`: the keys a promise awaits and outranks every seed of. A promise whose segment has also seeded makes nothing pending.

### Derived

A store key the browser's `derive` computes. `Debug`, `#[non_exhaustive]`.

* `pub struct Derived { pub key: String, pub sources: Vec<String>, pub compute: Option<Compute> }`
* `pub type Compute = Arc<dyn Fn(&RequestCtx, &Data) -> Option<Value> + Send + Sync>`: answers `None` where the browser's compute would throw.
* `compute` is `None` for a derived key that was not translated.
* `new(key: String, sources: Vec<String>, compute: Option<Compute>) -> Self`
* `pub fn derive_order(derives: Vec<Derived>) -> Vec<Derived>`: each derived key after the derived keys it reads.
* `pub fn apply_derives(ctx: &RequestCtx, store: &mut Data, derives: &[Derived])`: computes each key in order over the merged store and writes it. A key whose compute is missing or answers `None` is removed.
* `pub fn derived_pending(pending: &mut BTreeSet<String>, derives: &[Derived])`: a derived key is pending exactly when it has no compute or a key it reads is pending.

### PENDING_PROP

* `pub const PENDING_PROP: &str = "$pending"`: the prop a node's pending store keys ride in beside `$store`. The render memo key carries them.

## 8. Document Metadata

### Meta

What a route says about itself once its data is known. `Debug`, `Clone`, `Default`, `PartialEq`, `Eq`, `#[non_exhaustive]`.

* `pub struct Meta { pub title: Option<String>, pub absolute: bool, pub description: Option<String>, pub head: Vec<HeadEl> }`
* A field left `None` keeps the document's default.
* `absolute`: `title` is written as given instead of through the head's template.
* Setters `title`, `absolute`, `description` and `head`, each `fn(self, value) -> Self`.
* `is_empty(&self) -> bool`: no title, no description and no head elements.
* `merge(&mut self, inner: Meta)`: folds an inner segment over this one.
* `merge` takes `inner`'s title with its `absolute` when the title is set. It takes `inner`'s description when set.
* In `merge`, each identity `inner` names replaces every row of that identity: its first row takes the first row's place and the rest are appended in order. An element matching nothing is appended.
* `card_text(head: &mut Vec<HeadEl>, title: &str, description: Option<&str>)`: adds `og:title` and `og:description`, each only where no element already names it and only when non-empty.

### HeadEl

One head element. `Debug`, `Clone`, `PartialEq`, `Eq`, `#[non_exhaustive]`.

* `pub struct HeadEl { pub tag: String, pub attrs: Vec<(String, String)>, pub children: Option<String> }`: attributes in the order written.
* `new(tag: String, attrs: Vec<(String, String)>, children: Option<String>) -> Self`
* `script(attrs: Vec<(String, String)>, children: Option<String>) -> Result<HeadEl, String>`: a `meta` head entry whose tag is `script`, as the document carries it.
* `script` writes `type="text/plain"`, `data-sf-script` holding `when`, `data-src` holding `src` and `data-type` holding `type`, then every other attribute as written. `when` defaults to `load`.
* `script` is `Err` for no `src`, an inline body or a `when` outside `SCRIPT_WHEN`.
* A `script` row whose `type` is a data block is written as given with its text, `</script` in the text escaped.
* `is_script(&self) -> bool`: a row `script` made.
* `is_icon(&self) -> bool`: a `link` whose `rel` includes `icon`.
* `is_empty_icon(&self) -> bool`: an icon whose `href` is `data:,`.
* `identity(&self) -> (String, String)`: what makes two entries the same element. It is the tag and the first of `rel`, `name`, `property`, `http-equiv`, `itemprop` or `id`, qualified by `sizes`, `media`, `type` and `hreflang`.
* `identity` also qualifies a `link` whose `rel` is `stylesheet`, `preconnect`, `dns-prefetch`, `preload`, `modulepreload` or `prefetch` by its `href`.
* `identity` of an `application/ld+json` script is its text. An element with none of those attributes is identified by all of its attributes.
* `render(&self, out: &mut String)`: writes the element. A void element takes no end tag and every other element is closed.
* `pub fn is_data_block(script_type: Option<&str>) -> bool`: whether a script of this `type` is data the browser never runs. False for no type, an empty type, `module`, `importmap`, `speculationrules` and the JavaScript MIME types. True for any other.
* `pub const SCRIPT_WHEN: [&str; 3] = ["load", "idle", "interaction"]`

### Metadata

Registered under a data source id.

* `pub trait Metadata: Send + Sync { fn describe(&self, ctx: &RequestCtx, data: &Data) -> BoxFuture<'static, Result<Meta, LoadError>>; }`: `data` is what that source loaded.

### HeadRows

Registered under a module id with `RuntimeBuilder::listed`.

* `pub trait HeadRows: Send + Sync { fn rows(&self, ctx: &RequestCtx) -> BoxFuture<'static, Result<(String, Vec<HeadEl>), LoadError>>; }`: the rows with the field they fill, such as `og:image`.
* Before the rows are appended, every element is removed whose `property` or `name` is the field or starts with the field and a colon.
* A failure is logged on target `fsr::load` and changes nothing.

### Paths

The parameter sets a route with a parameter is prerendered for.

* `pub trait Paths: Send + Sync { fn paths(&self, ctx: &RequestCtx) -> BoxFuture<'static, Result<Vec<Params>, LoadError>>; }`: one `Params` per path, each naming every parameter of the pattern.

### Head

The shell's head slot: everything the host puts in the head, plus the defaults a segment's `Meta` overrides. `Debug`, `Clone`, `PartialEq`, `#[non_exhaustive]`.

* `pub struct Head { pub title: String, pub template: Option<String>, pub description: Option<String>, pub rest: Node, pub entry: Option<String>, pub styles: Vec<String>, pub catalog: Option<String>, pub head: Vec<HeadEl>, pub origin: Option<String>, pub theme: Option<DocumentTheme> }`
* `template`: how a segment's title is written. `{page}` is the segment's title and `{title}` the default, each filled once. Absent, a segment's title is written as given.
* `entry`: a module the browser must load for this response's islands beyond the document's own entry. Carried as the `E` row.
* `styles`: stylesheets this response needs beyond the document's own. Carried as the `C` row.
* `catalog`: the locale's message catalog as JSON. Carried as the `D` row.
* `head`: the head elements every document carries. A segment's `meta` folds over these.
* `origin`: `scheme://host` for the deployment. A path in the `href` of a `canonical` or `alternate` link is prefixed with it.
* A path in the `content` of an `og:image`, `og:image:url`, `og:image:secure_url`, `og:url`, `og:video`, `og:audio` or `twitter:image` meta is prefixed with `origin` too.
* `theme`: what the request decided for the document's theme. Carried in the eager `H` row.
* `new(title: impl Into<String>, rest: Node) -> Self`: every other field empty.
* `title_for(&self, meta: &Meta) -> String`: the segment's title through the template unless `meta.absolute`, the default when it names none.
* `settle(&self, meta: &mut Meta)`: writes `meta`'s title through the template and clears `absolute`.
* `rows(&self, meta: &Meta) -> Vec<HeadEl>`: every head element a document under `meta` carries. That is `head` with `meta`'s folded over it, the card text and the paths made absolute.
* `rows` drops the empty icon when the head carries any other icon.
* `node(&self, meta: &Meta) -> Node`: `rest`, then `<title>` when the title is non-empty, `<meta name="description">` when there is a description, `rows` between `HEAD_OPEN` and `HEAD_CLOSE`, each of `styles` and the `entry` script.
* `From<Node>` and `From<&Node>`: an empty title. `From<&Head>`: a clone.

### DocumentTheme

`<html data-sf-theme>` and `<html data-sf-scheme>` for one request. `Debug`, `Clone`, `Default`, `PartialEq`, `Eq`, `#[non_exhaustive]`.

* `pub struct DocumentTheme { pub name: Option<String>, pub scheme: Option<String>, pub forced: bool }`
* No `name` is the root theme. No `scheme` leaves the attribute off.
* `forced` writes `<html data-sf-theme-forced>`.
* Setters `name`, `scheme` and `forced`, each `fn(self, value) -> Self`.

### Head constants

* `PATH_MARK = "{fsr:path}"`: in a module head row's value, the request's path without a trailing slash.
* `HEAD_OPEN = "<!--sf-head-->"` and `HEAD_CLOSE = "<!--/sf-head-->"`: around a document's head rows.
* `STYLE_OPEN = "<link rel=\"stylesheet\" data-sf-css=\"\" href=\""`: how a response's own stylesheets are written.
* `ENTRY_OPEN = "<script type=\"module\" src=\""`: how a response's `entry` is written.

## 9. Segments

### SegmentKeyer

* `pub trait SegmentKeyer: Send + Sync { fn key(&self, plan: &PlanNode, params: &Params, query: &Params) -> String; }`: a segment's identity across responses.
* Equal keys keep the region's DOM and island state. Different keys replace it from that point down.
* The assembler appends `Locale::key_suffix` to every key.

### DefaultKeyer

* `pub struct DefaultKeyer`: the module displayed as `path#export`, then `?` and the sorted param `k=v` pairs followed by the sorted query pairs, joined by `&`. No `?` when there are none.
* For example `page.tera#default?section=servers` or `page.tsx#default?q=wireless`.

### SegmentInfo

The sidecar emitted beside the payload tree. `Debug`, `Clone`, `PartialEq`, `#[non_exhaustive]`.

* `pub struct SegmentInfo { pub key: String, pub digest: u64, pub name: String, pub path: Vec<u32>, pub slot: Option<u32>, pub children: Vec<SegmentInfo>, pub keep: Vec<String> }`
* `digest`: the fingerprint of what the segment rendered, its child segments elided. Zero for a deferred segment.
* `name`: the slot this segment fills in its parent. Empty at the root.
* `path`: the subtree's position relative to the parent segment's node. `[]` is the whole node and `[i]` is child `i` of a `Seq` or of an island.
* `slot`: set for a deferred segment, which carries no path.
* `keep`: slots of this segment the payload leaves unfilled and the browser keeps.
* `new(key: String, digest: u64, name: String, path: Vec<u32>, slot: Option<u32>, children: Vec<SegmentInfo>, keep: Vec<String>) -> Self`
* `keep_of(plan: &PlanNode) -> Vec<String>`: the plan node's `keep` names.

## 10. Caching

### CacheEntry

What a render memo hit restores. `Debug`, `Clone`, `PartialEq`, `#[non_exhaustive]`.

* `pub struct CacheEntry { pub node: Node, pub segments: Vec<SegmentInfo>, pub digest: u64 }`
* `segments`: the subtree's child segments.
* `digest`: the subtree's own fingerprint.
* `new(node: Node, segments: Vec<SegmentInfo>, digest: u64) -> Self`

### NodeCache

Memoizes evaluated subtrees.

* `pub trait NodeCache: Send + Sync`
* `fn get(&self, key: &str) -> BoxFuture<'_, Option<CacheEntry>>`: `key` is the composed key from `assemble`.
* `fn put(&self, key: String, entry: CacheEntry) -> BoxFuture<'_, ()>`
* `fn invalidate(&self, cache_key: &str) -> BoxFuture<'_, usize>`: takes the plan's `cache_key`. It must remove every composed key made from it and answer how many went.
* `fn keeps(&self) -> bool`: whether a `put` can answer a later `get`. Defaults to `true`. A cache that keeps nothing is never given a key.

### NoCache

* `pub struct NoCache`: the default. `get` is `None`, `put` does nothing, `invalidate` answers 0 and `keeps` is false.

### MemoryCache

A `HashMap` behind a `parking_lot::Mutex`. Unbounded, no expiry. `Default`.

* `new() -> Self`
* `invalidate` removes every entry whose key starts with `{cache_key}|`.

### FibreCache

Backed by `fibre_cache`: sharded, TinyLFU-bounded and TTL-expiring. A side index maps each plan `cache_key` to the composed keys stored under it.

* `new(cache: fibre_cache::Cache<String, CacheEntry>) -> Self`: over a cache the caller built. With no listener on it, the index shrinks only on `invalidate`.
* `listener() -> (Index, impl EvictionListener<String, CacheEntry> + 'static)`: an index and the listener that keeps it in step, for `CacheBuilder::eviction_listener`.
* `Index` is `Arc<parking_lot::Mutex<HashMap<String, HashSet<String>>>>`.
* `with_index(cache: fibre_cache::Cache<String, CacheEntry>, index: Index) -> Self`: `new` over a cache whose builder carried that listener.
* `indexed(&self, plan_key: &str) -> usize`: how many composed keys the index holds under a plan key.
* `bounded(capacity: u64, ttl: Duration) -> Self`: four shards, maintenance on every insert and a timer tick of a hundredth of the TTL, clamped between 10 ms and 1 s.
* `bounded_sharded(capacity: u64, ttl: Duration, shards: usize) -> Self`: `bounded` with the shard count given. `fibre_cache` rounds `shards` up to a power of two. Capacity is accounted across all shards.
* `bounded` and `bounded_sharded` install the listener. They panic if `fibre_cache` refuses the configuration.
* Every entry is inserted with a cost of 1, so `capacity` counts entries.

### WarmRenders

A `NodeCache` holding what a build rendered, in front of a live one.

* `new(entries: HashMap<String, CacheEntry>, live: Arc<dyn NodeCache>) -> Self`
* `get` answers from the build's entries first and from `live` after. `put` and `invalidate` reach `live` alone.
* `record(&self, on: bool)`: while on, every `get` misses and every `put` lands in the build's entries.
* `replace(&self, entries: HashMap<String, CacheEntry>)`: swaps the whole map.
* `entries(&self) -> HashMap<String, CacheEntry>`: a copy.
* `len(&self) -> usize`
* `is_empty(&self) -> bool`
* `keeps` is true while it records, while it holds an entry and whenever `live` keeps.

### LoadCache

Memoizes a source's loaded data under the key its `LoadKeyer` composed.

* `pub trait LoadCache: Send + Sync`
* `fn get(&self, key: &str) -> BoxFuture<'_, Option<Data>>`
* `fn put(&self, key: String, data: Data) -> BoxFuture<'_, ()>`

### NoLoadCache

* `pub struct NoLoadCache`: the default. `get` is `None` and `put` does nothing.

### WarmLoads

A `LoadCache` holding what a build warmed. `Default`.

* `new(entries: HashMap<String, Data>) -> Self`
* `replace(&self, entries: HashMap<String, Data>)`: swaps the whole map. The only way in.
* `len(&self) -> usize`
* `is_empty(&self) -> bool`
* `put` does nothing.

### MemoryLoadCache

A `HashMap` behind a `parking_lot::Mutex`. Unbounded, no expiry. `Default`.

* `new() -> Self`
* `snapshot(&self) -> HashMap<String, Data>`

## 11. Request Context

### RequestCtx

Everything a loader or action may know about the request. `Clone`, `Default`, `#[non_exhaustive]`.

* `pub params: Params`: the matched params.
* `pub query: Params`: the decoded query string, one value per key, the last repeat winning. Keys starting with `__` are dropped.
* `pub path: String`: the path the request matched, locale prefix included and query excluded. Empty under an action and under a context nothing resolved.
* `pub document: Option<String>`: on an intercept, the path of the page the document is showing, the `x-sf-from` value with its locale prefix stripped. `None` otherwise.
* `pub address: Option<Address>`: on an intercept, the navigation's own request. `None` otherwise.
* `pub session: SessionCell`
* `pub locale: Locale`: the default `Locale` under a context nothing resolved.
* `pub locales: Vec<String>`: the locales the application serves, in `[locales]` order. Empty under a context the host did not build.
* `pub host: Option<String>`: the host the request named, already matched against `[server] hosts`. `None` when that key is unset.
* `pub origin: Option<String>`: `document.origin` as the host checked it at boot. `None` when the configuration names none.
* `pub failure: Option<Value>`: the failure of a form post that redirected back here, a map of `kind` and `message`.
* `pub config: ValueMap`: the deployment's `[public]` values.
* `pub csrf: CsrfHandle`
* `pub csp_nonce: Option<String>`: the nonce this live response's policy names. `None` for a render a file or the memo keeps.
* `pub services: ServiceHandle`
* `pub natives: NativeHandle`
* `pub redirect: RedirectHandle`
* `pub next: Option<NextHandle>`: set while a middleware runs. `None` everywhere else.
* `pub after: AfterHandle`
* `anonymous(params: Params) -> Self`: `params` and every other field empty or unbound.
* `identity_value(&self) -> Option<Value>`: the session identity as a `Value::Map` of `subject` and `claims`.
* A clone shares the session cell, the CSRF handle, the redirect cell and the after queue.

### Identity

Who the request is. `Debug`, `Clone`, `PartialEq`, `#[non_exhaustive]`.

* `pub struct Identity { pub subject: String, pub claims: ValueMap }`
* `new(subject: String, claims: ValueMap) -> Self`

### SessionCell

The request's session. `Clone`, `Default`. Clones share one cell.

* `new(data: ValueMap, identity: Option<Identity>) -> Self`: starts clean.
* `get(&self, key: &str) -> Option<Value>`
* `insert(&self, key: impl Into<String>, value: Value)`: marks dirty.
* `remove(&self, key: &str) -> Option<Value>`: marks dirty when something was removed. The remaining entries keep their order.
* `identity(&self) -> Option<Identity>`
* `set_identity(&self, identity: Option<Identity>)`: marks dirty.
* `clear(&self)`: drops data and identity in one dirty write.
* `is_dirty(&self) -> bool`
* `mark_saved(&self)`: clears the dirty flag.
* `snapshot(&self) -> (ValueMap, Option<Identity>)`
* `expiring(self, at: u64) -> Self`: sets the end, in seconds since the Unix epoch, without marking anything.
* `expires(&self) -> u64`
* `extend(&self, by: Duration)`: moves the end to `by` from now and marks the cell extended. The end moves only through `extend`.
* `is_extended(&self) -> bool`
* Every mutator takes `&self`. Nothing clears the extended flag.

### CsrfHandle

The request's CSRF token, minted on first `get` and held for the request. `Clone`, `Default` (unbound).

* `new(memo: Option<String>, mint: impl Fn() -> String + Send + Sync + 'static) -> Self`: `memo` is what the render memo keys a subtree rendering the token by, `None` when every mint differs.
* `fixed(token: impl Into<String>) -> Self`: a handle holding one token.
* `is_bound(&self) -> bool`
* `get(&self) -> Option<String>`: `None` unbound.
* `memo_key(&self) -> Option<String>`: `Some("-")` unbound, `Some(memo)` when the token is stable for the session and `None` when it is not.

### RedirectHandle

Where a successful action sends its caller. `Clone`, `Default` (empty). A clone shares the cell.

* `set(&self, to: impl Into<String>) -> Result<(), ActionError>`: takes a path on this origin, one leading `/` and not `//` or `/\`. Anything else is `ActionError` `invalid`.
* `check(to: &str) -> Result<(), ActionError>`: what `set` refuses, without setting.
* `get(&self) -> Option<String>`

### AfterHandle

Work to run once the response is written. `Clone`, `Default` (an empty queue). A clone shares the queue.

* `push(&self, work: impl Future<Output = ()> + Send + 'static)`
* `take(&self) -> Vec<BoxFuture<'static, ()>>`: everything queued, leaving the queue empty.

### Address

The request the browser navigated to on an intercepted render. `Clone`, `Debug`, `Default`, `PartialEq`, `#[non_exhaustive]`.

* `pub struct Address { pub path: String, pub params: Params, pub query: Params }`: `path` includes the locale prefix.
* Setters `path`, `params` and `query`, each `fn(self, value) -> Self`.
* `value(&self) -> Value`: the map a body reads, of `path`, `params` and `query`.

### Locale

The request's locale as the application spells it, `fr_FR` or `fr`. `Debug`, `Clone`, `PartialEq`, `Eq`, `#[non_exhaustive]`.

* `pub struct Locale { pub tag: String, pub is_default: bool }`
* `Default` is an empty tag with `is_default` true.
* `is_default`: the application's default locale, which leaves segment keys and prerender paths unmarked.
* `new(tag: impl Into<String>, is_default: bool) -> Self`
* `hyphenated(&self) -> String`: the BCP 47 spelling, `fr-FR` for `fr_FR`.
* `key_suffix(&self) -> String`: `@<tag>` for a locale that is not the default, else empty.

### Context functions

* `pub fn parse_query(raw: &str) -> Params`: decodes `+` and `%XX`, drops empty keys and `__`-prefixed keys and keeps a repeated key's last value.
* `pub fn unix_now() -> u64`: seconds since the Unix epoch.
* `pub const FAILURE_KEY: &str = "$failure"`: the session key a form post's failure waits under for the render it redirected to.

## 12. Services

### ServiceCaller

What the service layer implements.

* `pub trait ServiceCaller: Send + Sync { fn call(&self, service: &str, method: &str, args: ValueMap) -> BoxFuture<'static, Result<Value, ServiceError>>; }`

### ServiceHandle

`ctx.services`. `Clone`, `Default` (unbound).

* `new(caller: Arc<dyn ServiceCaller>) -> Self`
* `is_bound(&self) -> bool`
* `call(&self, service: &str, method: &str, args: ValueMap) -> BoxFuture<'static, Result<Value, ServiceError>>`: an unbound handle fails with `FailureKind::Unavailable` and `no service layer is bound to this request`.
* The handle has no accessor for the caller it holds.

### Native

One module of the application's own Rust, reached from a body as `ctx.native.<name>.<method>()`.

* `pub trait Native: Send + Sync`
* `fn call(&self, method: &str, args: ValueMap) -> BoxFuture<'static, Result<Value, ServiceError>>`
* `fn call_sync(&self, method: &str, args: ValueMap) -> Option<Result<Value, ServiceError>>`: a method the build read as `fn`, answered without a future. `None` for a method that is async or unknown. Defaults to `None`.
* `#[native]` from `snapfire_fsr_macros` implements both from an `impl` block. Only the methods the block declares `pub` are reachable.

### Natives

The registered modules, by the name a body calls them under. `Default`.

* `new() -> Self`
* `register(&mut self, name: impl Into<String>, module: Arc<dyn Native>)`: a name registered twice keeps the last.
* `get(&self, name: &str) -> Option<&Arc<dyn Native>>`
* `is_empty(&self) -> bool`
* `names(&self) -> Vec<&str>`: in registration order.

### NativeHandle

`ctx.natives`. `Clone`, `Default` (unbound).

* `new(natives: Arc<Natives>) -> Self`
* `is_bound(&self) -> bool`
* `call(&self, module: &str, method: &str, args: ValueMap) -> BoxFuture<'static, Result<Value, ServiceError>>`: an unbound handle fails with `FailureKind::Unavailable` and `no native modules are bound to this request`. An unknown name fails with `FailureKind::Unavailable` and ``no native module `{module}` is registered``.
* `call_sync(&self, module: &str, method: &str, args: ValueMap) -> Option<Result<Value, ServiceError>>`: `None` when nothing is registered under `module` or the method is not sync.

### FromNativeValue

What a native method's arguments and answers may be.

* `pub trait FromNativeValue: Sized`: implemented for `Value`, `Dynamic`, `ValueMap`, `String`, `bool`, every integer width, `f64`, `Option<T>` and `Vec<T>`.
* `pub trait IntoNativeValue`: the same types plus `()` and `Result<T, ServiceError>`.
* `pub fn native_arg<T: FromNativeValue>(args: &ValueMap, key: &str, method: &str) -> Result<T, ServiceError>`: one argument decoded. An absent or wrong-typed argument is an error.
* `#[derive(Record)]` from `snapfire_fsr_macros` implements both traits for a struct.

### CallState

* `pub trait CallState: Clone + Send + Sync + 'static {}`: implemented for every such type. A `#[native]` or `#[service]` type must be `Clone`. Each call clones it.

## 13. Actions

### ActionHandler

* `pub trait ActionHandler: Send + Sync { fn call(&self, ctx: RequestCtx, input: Value) -> BoxFuture<'static, Result<Value, ActionError>>; }`

### ActionRegistry

Action ids to handlers, in insertion order. `Default`.

* `new() -> Self`
* `insert(&mut self, id: impl Into<String>, handler: Arc<dyn ActionHandler>)`
* `insert_fn<F, Fut>(&mut self, id: impl Into<String>, f: F)` where `F: Fn(RequestCtx, Value) -> Fut + Send + Sync + 'static` and `Fut: Future<Output = Result<Value, ActionError>> + Send + 'static`
* `ids(&self) -> Vec<String>`: every registered id, in registration order.
* `dispatch(&self, id: &str, ctx: RequestCtx, input: Value) -> BoxFuture<'static, Result<Value, ActionError>>`: an unknown id is `ActionError` with `FailureKind::NotFound` and ``no action `{id}` ``. Emits a DEBUG event on target `fsr::action`.

### HttpResponse

A response a body answers in place of data: what `new Response(...)`, `Response.json`, `Response.redirect` and `redirect()` make. `Debug`, `Clone`, `PartialEq`.

* `pub struct HttpResponse { pub status: u16, pub headers: Vec<(String, String)>, pub body: Vec<u8>, pub downstream: Option<usize> }`: `headers` in the order given, names as written.
* `downstream`: which route response a middleware's `next` answered this stands for.
* `new(status: u16) -> Self`: no headers and an empty body.
* `header(&self, name: &str) -> Option<&str>`: matched without regard to case.
* `header_default(&mut self, name: &str, value: &str)`: sets `name` unless it is already there.
* `location(&self) -> Option<&str>`: the `location` of a 3xx response.
* `to_value(&self) -> Value`: a `Value::Variant` tagged `RESPONSE_TAG` whose payload is an object of `status`, `headers` and `body`. `headers` is an array of `[name, value]` pairs.
* `from_value(value: &Value) -> Option<Result<Self, String>>`: `None` for a value that is not one. An error for one whose payload is malformed.
* `pub const RESPONSE_TAG: &str = "$response"`
* `pub const REDIRECT_STATUSES: [u16; 5] = [301, 302, 303, 307, 308]`

## 14. Middleware

### NextHandle

`next` in a middleware's context.

* `new(next: impl Fn(Value) -> BoxFuture<'static, Result<Value, ActionError>> + Send + Sync + 'static) -> Self`
* `call(&self, request: Value) -> BoxFuture<'static, Result<Value, ActionError>>`: runs the rest of the chain for `request` and answers the route's `HttpResponse` value.

### Target

Where `next` sends a request.

* `pub struct Target { pub path: String, pub query: String }`
* `of(request: &Value, path: &str, query: &str) -> Result<Self, ActionError>`: null or undefined is the request as it came. A string or a `Request` value's `url` is a path on this origin. An absolute URL keeps its path and query. A map is read by its `path`. Anything else is `Internal`.
* `pub const REQUEST_TAG: &str = "$request"`: the tag of the `Value::Variant` `new Request(url)` makes, its payload an object holding `url`.

### Passed

How a middleware answered.

* `pub enum Passed<B> { Continue, Answer(HttpResponse), Route(HttpResponse, B) }`
* `Continue`: it returned nothing without calling `next`.
* `Answer`: a response of its own.
* `Route`: the route's response, with the status and headers the middleware left on it and the body the route answered.

### drive_middleware

* `pub async fn drive_middleware<B, Run, Serve, Fut>(ctx: RequestCtx, query: &str, run: Run, serve: Serve) -> Result<Passed<B>, ActionError>` where `Run: FnOnce(RequestCtx) -> BoxFuture<'static, Result<Value, ActionError>>`, `Serve: FnMut(Target) -> Fut` and `Fut: Future<Output = Result<(HttpResponse, B), ActionError>>`.
* Runs `run(ctx)` with `ctx.next` set and serves each `next` call with `serve`. `query` is the request's query as it came. Its path is `ctx.path`.
* Returning nothing after calling `next` answers the last route response as it came. A value that is neither a response nor nothing is `Internal`.

## 15. Template Islands

Islands whose markup a template renders and whose handlers are Rust.

### IslandEvent

What a handler is given. Every field arrives from the browser. `Debug`, `Clone`, `#[non_exhaustive]`.

* `pub struct IslandEvent { pub props: Value, pub state: Value, pub event: Value }`
* `new(props: Value, state: Value, event: Value) -> Self`

### IslandHandler

* `pub trait IslandHandler: Send + Sync { fn call(&self, ctx: RequestCtx, event: IslandEvent) -> BoxFuture<'static, Result<Value, ActionError>>; }`: answers the island's next state.

### IslandRegistry

Handlers by module and name. `Default`.

* `new() -> Self`
* `insert(&mut self, module: impl Into<String>, name: impl Into<String>, handler: Arc<dyn IslandHandler>)`
* `insert_fn<F, Fut>(&mut self, module: impl Into<String>, name: impl Into<String>, f: F)` where `F: Fn(RequestCtx, IslandEvent) -> Fut + Send + Sync + 'static` and `Fut: Future<Output = Result<Value, ActionError>> + Send + 'static`
* `holds(&self, module: &str) -> bool`: whether any handler is registered for the module.
* `modules(&self) -> Vec<String>`: every module with a handler, in registration order, once each.
* `names(&self, module: &str) -> Vec<String>`: one module's handler names, in registration order.
* `dispatch(&self, module: &str, name: &str, ctx: RequestCtx, event: IslandEvent) -> BoxFuture<'static, Result<Value, ActionError>>`: an unknown pair is `FailureKind::NotFound` with ``` `{module}` has no handler `{name}` ```. Emits a DEBUG event on target `fsr::island`.

### island_data

* `pub fn island_data(props: &ValueMap, state: &Value) -> ValueMap`: the props without `$s` and `$k`, plus `state` under `state`.
* `islands::STATE_PROP = "$s"` and `islands::REGION_PROP = "$k"`

## 16. Streaming

Both stream functions take the `Assembly` by value and yield `String` chunks. A resolution's new pending slots join the working set. The stream ends when no slot is outstanding.

### wire_stream

```rust
pub fn wire_stream(assembly: Assembly) -> impl Stream<Item = String> + Send
```

The first item is the eager wave, newline-terminated rows in this order:

* `V {"fmt":<FORMAT_VERSION>,"enc":"json"}`
* `N <node row json>`: the tree, from `snapfire_fsr_payload::node_to_row_json`.
* `H <meta json>`: the meta and `rows` as `meta_to_json` writes them, when either is not empty.
* With `assembly.theme` set, the `H` row is always written. It also carries `theme` and `scheme`, each a string or null, with `forced` as a boolean.
* `T <contributions json>`: when `assembly.contributions` is not empty.
* `L <json string>`: the locale tag, when there is one.
* `E <json string>`: the head's `entry`, when set.
* `C <json array>`: the head's `styles`, when not empty.
* `D <catalog json>`: the head's catalog, when set.
* `G <segment json>`: the sidecar, always and always last.

Then one item per resolution, in completion order:

* A `T` row when `Resolved::contributions` is not empty.
* `S <slot id> {"n": <node row json>, "g": [<segment json>...]}`, with `g` present when the resolved subtree has child segments.
* An `H` row when `Resolved::meta` is not empty.
* Each resolution emits a DEBUG event on target `fsr::stream`.

### html_stream

```rust
pub fn html_stream(assembly: Assembly) -> impl Stream<Item = String> + Send
```

The first item, in order:

* The tree with each segment wrapped in `<!--sf-g:{key}-->` and `<!--/sf-g-->`.
* The content placed elsewhere in the tree.
* The sidecar as `<script type="application/json" data-sf-segments>…</script>`.
* `<script type="application/json" data-sf-store>` holding `contributions_to_json`, when there are contributions.
* `CAPTURE_SCRIPT`.
* `FILL_SCRIPT`, when `assembly.pending` is not empty.

Then one item per resolution, in order:

* The content the resolution placed elsewhere.
* `<template data-sf-fill="{slot}">{subtree}</template>`, the subtree in its segment's delimiters with every child segment delimited inside it.
* `<script type="application/json" data-sf-fill-data="{slot}">{data}</script>`, when there is data. `data` holds `store` (the resolution's contributions), `segments` (its child segments) and `head` (its `H` row), each only when present.
* `<script>__sfFill({slot})</script>`, from `fill_call(slot)`.

Encoding:

* A segment key is escaped for the comment: `%` is `%25` and `-` is `%2D`.
* `<` in every JSON script is written `<`.
* One `HtmlSession` spans the response, so island ids (`sf-i0` upward) stay unique across chunks.
* A deferred child segment is not written into the first chunk. Its region is the `data-sf-slot` element `Node::Pending` produces.
* `pub fn fill_call(n: u32) -> String`: `__sfFill(n)`.

### fragment_html

```rust
pub async fn fragment_html(assembly: Assembly, slot: Option<&str>) -> Option<String>
```

* One segment of the route as markup with nothing around it. With `slot` `None` it is the page, the innermost segment reached through `content` slots. Otherwise it is the first segment in sidecar order whose name is `slot`.
* Every pending resolution is awaited first, nested ones included. Each is spliced over its `Node::Pending`. No fallback is written.
* The markup has no segment delimiters and no sidecar. The content it placed elsewhere follows it.
* When the route seeded the store, the markup is followed by `<script type="application/json" data-sf-store data-sf-fragment>`, `<` written `<`.
* `None` when no segment carries that name.

### meta_to_json

* `pub fn meta_to_json(meta: &Meta) -> serde_json::Value`: an object holding only what is set.
* The object's fields are `title`, `description`, `scripts` and `head`. `scripts` holds each script row's attributes as an object. `head` holds every other element as `{"tag", "attrs": [[name, value], ...], "children"?}`.

### contributions_to_json

* `pub fn contributions_to_json(contributions: &[Contribution]) -> serde_json::Value`: an array with one object per contribution.
* Each object holds `k` (the segment key), `p` (the slot path) and `v` (the values as `snapfire_fsr_payload::value_to_json` encodes a `Value::Map`).
* `d` holds the defaults when not empty. `w` holds the awaited keys when not empty.

### segments_to_json

* `pub fn segments_to_json(info: &SegmentInfo) -> serde_json::Value`: keys in order `k` the key, `d` the digest as sixteen hex digits when not zero, `n` the name when not empty, `s` the slot for a deferred segment or `p` the path otherwise, `c` the children and `keep` when not empty.

### segments_from_json

* `pub fn segments_from_json(json: &serde_json::Value) -> Result<SegmentInfo, DecodeError>`: what `segments_to_json` wrote.
* A missing `k`, a `d` that is not hex or a `p` that is not a list of numbers is a `DecodeError` naming the field. An absent `d`, `n`, `s`, `p`, `c` or `keep` is its empty value.

### FILL_SCRIPT

* `pub const FILL_SCRIPT: &str`: a `<script>` defining `__sfFill(n)`, `__sfHead(h)`, `__sfRows(w)` and `__sfStore(o)`. `FILL_SCRIPT_BODY` is its text.
* `__sfFill(n)` reads and removes `script[data-sf-fill-data="n"]` and hands its `store` to `__sfStore`.
* `__sfFill(n)` pushes the template's markup onto `window.__sfServer` and replaces the `[data-sf-slot="n"]` element with the template's content. It then removes the template.
* `__sfFill(n)` writes the data's `segments` as the children of slot `n` in the `data-sf-segments` script. It dispatches `sf:fill` on `document` with the slot number as `detail` and calls `__sfHead` with the data's `head`.
* `__sfHead(h)` replaces the rows between the head comments with `h.head` and sets `document.title` and the description meta, creating the meta when there is none. It loads each of `h.scripts` through `window.__sf.script` when defined.
* `__sfStore(o)` calls `window.__sfSeedApply(o)` when defined, else queues `o` on `window.__sfSeed`.

### CAPTURE_SCRIPT

* `pub const CAPTURE_SCRIPT: &str`: a `<script>` that sets `window.__sfServer` to `[document.body.innerHTML]`. `CAPTURE_SCRIPT_BODY` is its text.

## 17. Reading a Payload

The module `snapfire_fsr_runtime::wire` reads back what `wire_stream` wrote.

### Row

One line of a payload, by its tag. `Debug`, `Clone`, `PartialEq`.

* `pub enum Row { Version { fmt: u32, enc: String }, Tree(Node), Head(Meta), Store(Vec<Contribution>), Locale(String), Entry(String), Styles(Vec<String>), Catalog(String), Segments(SegmentInfo), Fill { slot: u32, node: Node, segments: Vec<SegmentInfo> } }`
* The tags are `V`, `N`, `H`, `T`, `L`, `E`, `C`, `D`, `G` and `S`, in variant order.
* `Catalog` keeps the JSON as written.

### row_from_line

* `pub fn row_from_line(line: &str) -> Result<Row, DecodeError>`: `<tag> <json>`, the trailing newline optional.
* A `V` row of another format, an unknown tag, a line without a tag and an `S` row whose slot is not a number are each an error naming the fault.

### RowReader

Rows out of bytes as they arrive. `Default`.

* `new() -> Self`
* `push(&mut self, bytes: &[u8]) -> Result<Vec<Row>, DecodeError>`: every row whose newline is in, whatever the chunk boundaries.
* `finish(self) -> Result<Option<Row>, DecodeError>`: a last line that ended without a newline. `None` when only whitespace is left.

### contributions_from_json

* `pub fn contributions_from_json(json: &serde_json::Value) -> Result<Vec<Contribution>, DecodeError>`: what `contributions_to_json` wrote, `d` and `w` included.

### meta_from_json

* `pub fn meta_from_json(json: &serde_json::Value) -> Result<Meta, DecodeError>`: what `meta_to_json` wrote. `scripts` and `head` both read back into `Meta::head`.

## 18. Error Handling

### FailureKind

One kind per 4xx and 5xx status the IANA HTTP status code registry assigns, 418 aside. `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `#[non_exhaustive]`. Defined in `snapfire_fsr_core::ext` and re-exported here.

| Variant | `as_str()` | `http_status()` |
| :--- | :--- | :--- |
| `Invalid` | `invalid` | 400 |
| `Unauthorized` | `unauthorized` | 401 |
| `PaymentRequired` | `payment_required` | 402 |
| `Forbidden` | `forbidden` | 403 |
| `NotFound` | `not_found` | 404 |
| `MethodNotAllowed` | `method_not_allowed` | 405 |
| `NotAcceptable` | `not_acceptable` | 406 |
| `ProxyAuthenticationRequired` | `proxy_authentication_required` | 407 |
| `RequestTimeout` | `request_timeout` | 408 |
| `Conflict` | `conflict` | 409 |
| `Gone` | `gone` | 410 |
| `LengthRequired` | `length_required` | 411 |
| `PreconditionFailed` | `precondition_failed` | 412 |
| `ContentTooLarge` | `content_too_large` | 413 |
| `UriTooLong` | `uri_too_long` | 414 |
| `UnsupportedMediaType` | `unsupported_media_type` | 415 |
| `RangeNotSatisfiable` | `range_not_satisfiable` | 416 |
| `ExpectationFailed` | `expectation_failed` | 417 |
| `MisdirectedRequest` | `misdirected_request` | 421 |
| `UnprocessableContent` | `unprocessable_content` | 422 |
| `Locked` | `locked` | 423 |
| `FailedDependency` | `failed_dependency` | 424 |
| `TooEarly` | `too_early` | 425 |
| `UpgradeRequired` | `upgrade_required` | 426 |
| `PreconditionRequired` | `precondition_required` | 428 |
| `TooManyRequests` | `too_many_requests` | 429 |
| `RequestHeaderFieldsTooLarge` | `request_header_fields_too_large` | 431 |
| `UnavailableForLegalReasons` | `unavailable_for_legal_reasons` | 451 |
| `Internal` | `internal` | 500 |
| `NotImplemented` | `not_implemented` | 501 |
| `BadGateway` | `bad_gateway` | 502 |
| `Unavailable` | `unavailable` | 503 |
| `Timeout` | `timeout` | 504 |
| `HttpVersionNotSupported` | `http_version_not_supported` | 505 |
| `VariantAlsoNegotiates` | `variant_also_negotiates` | 506 |
| `InsufficientStorage` | `insufficient_storage` | 507 |
| `LoopDetected` | `loop_detected` | 508 |
| `NotExtended` | `not_extended` | 510 |
| `NetworkAuthenticationRequired` | `network_authentication_required` | 511 |

* `pub const ALL: &'static [FailureKind]`: every kind, in status order.
* `as_str(&self) -> &'static str`
* `http_status(&self) -> u16`
* `from_name(name: &str) -> Option<FailureKind>`: the kind `as_str` spells.
* `from_status(status: u16) -> FailureKind`: the kind for a status. `Invalid` for a 4xx no kind names and `Internal` for anything else.
* `is_client_error(&self) -> bool`: a 4xx.
* `is_server_error(&self) -> bool`: a 5xx.

### LoadError

A data source failed. `Debug`, `Clone`, `thiserror::Error`, `#[non_exhaustive]`.

* `pub struct LoadError { pub source_id: String, pub message: String, pub kind: FailureKind, pub call: Option<String> }`
* `Display` is ``data source {source_id}: {message}``. With `call` set it is ``data source {source_id}: {call} failed ({kind}): {message}``.
* `kind`: the kind a body's `fail` or the failed call named, `Internal` otherwise.
* `call`: the service call that failed, `service.method`.
* `new(source_id: impl Into<String>, message: impl Into<String>) -> Self`: kind `Internal`.
* `failed(source_id: impl Into<String>, fail: Fail) -> Self`: a body's failure, keeping its kind and its call.
* `with_kind(self, kind: FailureKind) -> Self`
* In assembly a `LoadError` degrades its segment and keeps the enclosing subtree out of the render memo. Its display string is the error module's `error` prop.

### EvalError

An evaluator failed. `Debug`, `Clone`, `thiserror::Error`, `#[non_exhaustive]`.

* `pub struct EvalError { pub module: String, pub message: String }`
* `Display` is ``evaluate {module}: {message}``.
* `new(module: String, message: String) -> Self`
* In `assemble` it becomes `AssembleError::Eval`. Inside a deferred resolution it becomes that slot's error node.

### AssembleError

What fails a request. `Debug`, `thiserror::Error`, `#[non_exhaustive]`.

* `MissingDataSource(String)`: ``no data source registered for `{0}` ``.
* `Eval(EvalError)`: transparent, `From<EvalError>`. An evaluator failed outside a deferred resolution.
* `MissingSlot { node: u32, slot: String }`: ``evaluator asked for slot `{slot}` and plan node {node} has no child there``.
* `SlotInFallback(String)`: ``fallback module `{0}` may not contain slots``. Raised for a slot in a `fallback` module or an `error` module.

### ActionError

`Debug`, `Clone`, `thiserror::Error`.

* `pub struct ActionError { pub kind: FailureKind, pub message: String }`
* `Display` is ``action failed ({kind}): {message}``, the kind as `as_str()`.
* `new(kind: FailureKind, message: impl Into<String>) -> Self`

### ServiceError

`Debug`, `Clone`, `thiserror::Error`.

* `pub struct ServiceError { pub kind: FailureKind, pub service: String, pub method: String, pub message: String }`
* `Display` is ``{service}.{method} failed ({kind}): {message}``.
* `new(kind: FailureKind, service: impl Into<String>, method: impl Into<String>, message: impl Into<String>) -> Self`
