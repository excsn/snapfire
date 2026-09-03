# API Reference: snapfire_fsr_runtime

The request blocks of SnapFire FSR: matching, resolution, data sources, evaluation, assembly, caching, the request context, the service seam, actions and streaming.

## Contents

* [1. Matching](#1-matching)
  * [`EntryId`](#entryid)
  * [`RouteMatch`](#routematch)
  * [`Matcher`](#matcher)
  * [`MatchitMatcher`](#matchitmatcher)
* [2. Resolution](#2-resolution)
  * [`Resolver`](#resolver)
  * [`TableResolver`](#tableresolver)
* [3. Data sources](#3-data-sources)
  * [`DataSource`](#datasource)
  * [`DataSources`](#datasources)
* [4. Evaluation](#4-evaluation)
  * [`Chunk`](#chunk)
  * [`NodeChunks`](#nodechunks)
  * [`Evaluator`](#evaluator)
  * [`NullEvaluator`](#nullevaluator)
  * [`Evaluators`](#evaluators)
* [5. The runtime](#5-the-runtime)
  * [`Runtime`](#runtime)
  * [`RuntimeBuilder`](#runtimebuilder)
* [6. Assembly](#6-assembly)
  * [`assemble`](#assemble)
  * [`Assembly`](#assembly)
  * [`PendingResolution`](#pendingresolution)
  * [`Resolved`](#resolved)
* [7. Segments](#7-segments)
  * [`SegmentKeyer`](#segmentkeyer)
  * [`DefaultKeyer`](#defaultkeyer)
  * [`SegmentInfo`](#segmentinfo)
* [8. Caching](#8-caching)
  * [`CacheEntry`](#cacheentry)
  * [`NodeCache`](#nodecache)
  * [`NoCache`](#nocache)
  * [`MemoryCache`](#memorycache)
  * [`FibreCache`](#fibrecache)
* [9. Request context](#9-request-context)
  * [`Identity`](#identity)
  * [`SessionCell`](#sessioncell)
  * [`RequestCtx`](#requestctx)
* [10. Services](#10-services)
  * [`ServiceCaller`](#servicecaller)
  * [`ServiceHandle`](#servicehandle)
* [11. Actions](#11-actions)
  * [`ActionHandler`](#actionhandler)
  * [`ActionRegistry`](#actionregistry)
* [12. Streaming](#12-streaming)
  * [`wire_stream`](#wire_stream)
  * [`html_stream`](#html_stream)
  * [`segments_to_json`](#segments_to_json)
  * [`FILL_SCRIPT`](#fill_script)
* [13. Error handling](#13-error-handling)
  * [`FailureKind`](#failurekind)
  * [`LoadError`](#loaderror)
  * [`EvalError`](#evalerror)
  * [`AssembleError`](#assembleerror)
  * [`ActionError`](#actionerror)
  * [`ServiceError`](#serviceerror)

## 1. Matching

### `EntryId`

The application's stable id for a route entry. `pub struct EntryId(pub u32)`, `Debug + Clone + Copy + PartialEq + Eq + Hash`.

### `RouteMatch`

What a matcher returns. `Debug + Clone + PartialEq`.

* `pub entry: EntryId`
* `pub params: Params` (`indexmap::IndexMap<String, String>` from `snapfire_fsr_core`)

### `Matcher`

`pub trait Matcher: Send + Sync`.

* `fn match_path(&self, path: &str) -> Option<RouteMatch>`: `None` means no route, which the caller renders as a 404.

### `MatchitMatcher`

`matchit::Router<EntryId>` behind the `Matcher` trait. `Default`.

* `pub fn new() -> Self`
* `pub fn insert(&mut self, pattern: &str, entry: EntryId) -> Result<(), matchit::InsertError>`: patterns are `matchit` patterns, so a capture is written `/dash/{section}`. Conflicting or malformed patterns fail here, at construction.
* `fn match_path(&self, path: &str) -> Option<RouteMatch>`: named captures become params, in the order `matchit` yields them.

## 2. Resolution

### `Resolver`

`pub trait Resolver: Send + Sync`.

* `fn resolve(&self, entry: EntryId, params: &Params) -> Option<PlanNode>`: `None` means the entry has no plan. Params are passed so an implementation may return a different plan per param value.

### `TableResolver`

One prebuilt plan per entry. `Default`.

* `pub fn new() -> Self`
* `pub fn insert(&mut self, entry: EntryId, plan: PlanNode)`: a second insert for the same entry replaces the first.
* `fn resolve(&self, entry: EntryId, _params: &Params) -> Option<PlanNode>`: clones the stored plan and ignores the params.

Node ids must be distinct within one plan. The assembler keys loaded data and load failures by `PlanNode::id`, so two nodes sharing an id share a loader result and a failure.

## 3. Data sources

### `DataSource`

`pub trait DataSource: Send + Sync`.

* `fn load(&self, ctx: &RequestCtx) -> BoxFuture<'static, Result<Data, LoadError>>`: the returned future is `'static`, so an implementation clones out of `ctx` rather than borrowing it. `Data` is `ValueMap`.

### `DataSources`

Registry from source id to implementation, insertion-ordered. `Default`.

* `pub fn new() -> Self`
* `pub fn insert(&mut self, id: impl Into<String>, source: Arc<dyn DataSource>)`
* `pub fn insert_fn<F, Fut>(&mut self, id: impl Into<String>, f: F)` where `F: Fn(RequestCtx) -> Fut + Send + Sync + 'static` and `Fut: Future<Output = Result<Data, LoadError>> + Send + 'static`: the closure receives a clone of the context by value.
* `pub fn get(&self, id: &DataSourceId) -> Option<&Arc<dyn DataSource>>`

A plan node naming an id that was never inserted is `AssembleError::MissingDataSource`, which fails the whole assembly. A source that returns `LoadError` degrades only its own segment.

## 4. Evaluation

### `Chunk`

One item of an evaluator's output stream. `Debug + Clone + PartialEq`.

* `Chunk::Node(Node)`: finished output.
* `Chunk::Slot(SlotName)`: the stitch point where a plan child's subtree lands. `SlotName("head")` is reserved for the head node passed to `assemble`.

There is no `Pending` variant. Holes belong to the assembler.

### `NodeChunks`

`pub type NodeChunks = BoxStream<'static, Result<Chunk, EvalError>>`.

### `Evaluator`

`pub trait Evaluator: Send + Sync`.

* `fn evaluate(&self, module: &ModuleId, props: &Data) -> NodeChunks`: pure in the arguments. The evaluator sees no plan, no tree and no request context beyond the props the assembler composed.

The props are the node's loaded data with three keys written over the top: `params` (always, a `Value::Map` of the matched params), `identity` (`{ subject, claims }`, when the session resolved one) and `csrf_token` (a `Value::Str`, when the context carries one). A loader key with one of those names is replaced.

An error module additionally receives `error`, a `Value::Str` holding the `LoadError` display string. A fallback module receives the three request keys only, never loader data.

### `NullEvaluator`

Declines to evaluate. Unit struct.

* `fn evaluate(&self, module: &ModuleId, props: &Data) -> NodeChunks`: emits one chunk, `Node::Client { module, props, children: Vec::new(), ssr: None }`, so the browser mounts the module with the same props a server evaluator would have received.

### `Evaluators`

Module-to-evaluator dispatch. `Default`, which is no rules.

* `pub fn new() -> Self`
* `pub fn register(&mut self, applies: impl Fn(&ModuleId) -> bool + Send + Sync + 'static, evaluator: Arc<dyn Evaluator>)`
* `pub fn select(&self, module: &ModuleId) -> &dyn Evaluator`: the first registered rule whose predicate returns true, in registration order. With no match it returns the built-in `NullEvaluator`, so `select` never fails.

## 5. The runtime

### `Runtime`

The per-process pipeline, shared across requests. Fields are public and readable after `build`.

* `pub sources: DataSources`
* `pub evaluators: Evaluators`
* `pub keyer: Arc<dyn SegmentKeyer>`
* `pub cache: Arc<dyn NodeCache>`
* `pub fn builder() -> RuntimeBuilder`
* `pub fn new(sources: DataSources, evaluators: Evaluators) -> Arc<Self>`: default keyer, no cache.
* `pub fn with_keyer(sources: DataSources, evaluators: Evaluators, keyer: Arc<dyn SegmentKeyer>) -> Arc<Self>`: no cache.

### `RuntimeBuilder`

Obtained from `Runtime::builder()`; it has no public constructor of its own. Every method takes and returns `self`.

* `pub fn sources(self, sources: DataSources) -> Self`
* `pub fn evaluators(self, evaluators: Evaluators) -> Self`
* `pub fn keyer(self, keyer: Arc<dyn SegmentKeyer>) -> Self`
* `pub fn cache(self, cache: Arc<dyn NodeCache>) -> Self`
* `pub fn build(self) -> Arc<Runtime>`

Defaults: `DataSources::new()`, `Evaluators::new()`, `Arc::new(DefaultKeyer)`, `Arc::new(NoCache)`.

## 6. Assembly

### `assemble`

```rust
pub async fn assemble(
  runtime: &Arc<Runtime>,
  plan: &PlanNode,
  ctx: &RequestCtx,
  head: &Node,
) -> Result<Assembly, AssembleError>
```

Turns a plan plus a request into a payload. The order is fixed: every eager data source resolves to completion, then evaluation begins.

* **Eager loads.** The walk collects `data_source` from the subtree root and every descendant, stopping at any node with `deferred` set that is not the root. The collected sources run concurrently under one `try_join_all`.
* **Load outcomes.** A missing registration aborts with `AssembleError::MissingDataSource`. A `LoadError` is recorded against its node id; the wave continues.
* **Failure degradation.** A node with a recorded failure renders its `error` module or the built-in error node when it has none. Its children are not built.
* **Head.** `Chunk::Slot(SlotName("head"))` substitutes the `head` argument and marks the subtree head-using, which propagates to ancestors through non-deferred children.
* **Slots.** Any other slot name must match a child in `PlanNode::children`; otherwise the call fails with `AssembleError::MissingSlot`.
* **Deferral.** A child with `deferred` set gets a `SlotId` from a counter starting at 1, unique per response. Its `fallback` module is evaluated with the request props alone or `Node::raw("")` when it has none. `Node::Pending { slot, fallback }` goes into the tree while a `PendingResolution` goes into `Assembly::pending`.
* **Collapse.** A node whose evaluator emitted exactly one chunk becomes that node. Otherwise it becomes `Node::Seq` and each non-deferred child segment records `path: [index]`.

Cache lookup and store happen per plan node that carries a `cache_key`. The composed key is:

```text
{cache_key}|{sorted k=v params joined by &}|ident={subject}|{fingerprint:016x}
```

* `cache_key` is `PlanNode::cache_key`, the plan's own tag.
* The params are every entry of `ctx.params`, formatted `k=v`, sorted, joined by `&`. No params means an empty field.
* `subject` is the subject from `ctx.session.identity()` when the session resolved one; it is `-` when it did not.
* The fingerprint is xxh3 over the subtree, walking the plan in tree order and hashing each node id followed by that node's `Data` fingerprint when it loaded any, rendered as 16 lowercase hex digits.

No key is composed at all, so neither `get` nor `put` runs, when the node has no `cache_key`, when the subtree contains a `deferred` descendant or when any node in the subtree has a recorded load failure. A key that was composed is always looked up, but it is written back only if the subtree did not use the head slot.

### `Assembly`

What one call produced. `Debug` (which prints `pending` as a count); not `Clone`. Both streaming functions consume it by value.

* `pub tree: Node`
* `pub pending: Vec<PendingResolution>`
* `pub segments: SegmentInfo`: the root sidecar, keyed by `runtime.keyer` from the plan root and the request params.

### `PendingResolution`

A deferred slot's eventual content.

* `pub slot: SlotId`
* `pub future: BoxFuture<'static, Resolved>`: infallible. A failed loader or evaluation inside the subtree resolves to an error node instead.

### `Resolved`

* `pub slot: SlotId`
* `pub node: Node`
* `pub pending: Vec<PendingResolution>`: nested deferral, new slots the resolution itself introduced.

Segment information produced inside a resolution is discarded; a deferred subtree's identity is the slot-addressed `SegmentInfo` already in the first response.

## 7. Segments

### `SegmentKeyer`

`pub trait SegmentKeyer: Send + Sync`.

* `fn key(&self, plan: &PlanNode, params: &Params, query: &Params) -> String`: the comparable identity of a segment across responses. Equal keys mean the client keeps the region's DOM and island state; different keys mean it is replaced from that point down. Content changes are not identity changes.

### `DefaultKeyer`

Module plus every matched param and every query pair. Unit struct.

* `fn key(&self, plan: &PlanNode, params: &Params, query: &Params) -> String`: `plan.module` displayed as `path#export`; when params or query pairs are present a `?` followed by the param `k=v` pairs sorted, then the query pairs sorted, joined by `&`, for example `page.tera#default?section=servers` or `page.tsx#default?q=wireless`.

### `SegmentInfo`

The sidecar emitted beside the payload tree. `Debug + Clone + PartialEq`.

* `pub key: String`
* `pub path: Vec<u32>`: the subtree's position relative to the parent segment's node. `[]` is the whole node, `[i]` is child `i` of a `Seq`.
* `pub slot: Option<u32>`: set for a deferred segment, which is slot-addressed and carries no path.
* `pub children: Vec<SegmentInfo>`

## 8. Caching

### `CacheEntry`

What a hit restores. `Debug + Clone + PartialEq`.

* `pub node: Node`
* `pub segments: Vec<SegmentInfo>`: the subtree's child segments, so navigation identity survives caching.

### `NodeCache`

`pub trait NodeCache: Send + Sync`. Memoizes evaluated subtrees. A hit skips evaluation entirely: no chunk stream, no engine.

* `fn get(&self, key: &str) -> BoxFuture<'_, Option<CacheEntry>>`: `key` is the composed key from [`assemble`](#assemble).
* `fn put(&self, key: String, entry: CacheEntry) -> BoxFuture<'_, ()>`
* `fn invalidate(&self, cache_key: &str) -> BoxFuture<'_, ()>`: takes the plan's `cache_key`, not a composed key. It must remove every composed key derived from it, across all params and identities.

### `NoCache`

The default. Unit struct. `get` is always `None`, `put` and `invalidate` do nothing.

### `MemoryCache`

`HashMap` behind a `parking_lot::Mutex`. Unbounded, no expiry. `Default`.

* `pub fn new() -> Self`
* `invalidate` retains every entry whose key does not start with `{cache_key}|`, which relies on the composed key's first field being the plan key.

### `FibreCache`

`fibre_cache`-backed: sharded, TinyLFU-bounded, TTL-expiring. A side index maps each plan `cache_key` to the composed keys stored under it, so invalidation is exact without iterating the cache.

* `pub fn new(cache: fibre_cache::Cache<String, CacheEntry>) -> Self`: for a cache configured by the caller; with no listener on it, the index shrinks only on `invalidate`.
* `pub fn listener() -> (Index, impl EvictionListener<String, CacheEntry> + 'static)`: an index and the listener that keeps it exact, for `CacheBuilder::eviction_listener`; `Index` is `Arc<parking_lot::Mutex<HashMap<String, HashSet<String>>>>`.
* `pub fn with_index(cache: fibre_cache::Cache<String, CacheEntry>, index: Index) -> Self`: `new` over a cache whose builder carried that listener.
* `pub fn indexed(&self, plan_key: &str) -> usize`: how many composed keys the index holds under a plan key.
* `pub fn bounded(capacity: u64, ttl: Duration) -> Self`: four shards, opportunistic maintenance on every insert (`maintenance_chance(1)`) and a timer tick of a hundredth of the TTL clamped to 10 ms and 1 s, so an entry leaves within a percent of its TTL.
* `pub fn bounded_sharded(capacity: u64, ttl: Duration, shards: usize) -> Self`: `bounded` with the shard count given; `shards` is rounded up to the next power of two by `fibre_cache`, whose own default is derived from the CPU count. Capacity is accounted across all shards, so this trades lock contention against the fixed per-shard policy and timer structures, never against usable capacity.
* Every entry is inserted with a cost of 1, so `capacity` counts entries.
* `bounded` and `bounded_sharded` panic if `fibre_cache` refuses the configuration.
* The side index holds each composed key once and drops a key when the cache evicts it, on TTL or for room, through the eviction listener `bounded` and `bounded_sharded` install; `invalidate` clears a plan key's set. Expiry follows the cache's timer tick, so a key leaves the index when the janitor drops it, within a tick of its TTL.

## 9. Request context

### `Identity`

Who the request is, resolved by the session layer before anything loads. `Debug + Clone + PartialEq`.

* `pub subject: String`
* `pub claims: ValueMap`

Application code reads it and never sees a token.

### `SessionCell`

The request's session, shared by every loader and action on it. `Clone + Default`; clones share one `Arc<Mutex<..>>`, so a mutation through any clone is visible through all of them.

* `pub fn new(data: ValueMap, identity: Option<Identity>) -> Self`: starts clean.
* `pub fn get(&self, key: &str) -> Option<Value>`
* `pub fn insert(&self, key: impl Into<String>, value: Value)`: marks dirty.
* `pub fn remove(&self, key: &str) -> Option<Value>`: marks dirty only when something was removed. Removal preserves the order of the remaining entries.
* `pub fn identity(&self) -> Option<Identity>`
* `pub fn set_identity(&self, identity: Option<Identity>)`: marks dirty.
* `pub fn clear(&self)`: drops data and identity in one dirty write.
* `pub fn is_dirty(&self) -> bool`
* `pub fn snapshot(&self) -> (ValueMap, Option<Identity>)`

Every mutator takes `&self`. Dirtiness is one-way within a request: nothing clears the flag.

### `RequestCtx`

Everything a loader or action may know about the request. `Clone + Default`. Serializable values only, plus the handle, which is callable but carries nothing readable.

* `pub params: Params`
* `pub session: SessionCell`
* `pub csrf: Option<String>`
* `pub services: ServiceHandle`
* `pub query: Params`: the decoded query string, one value per key, the last repeat winning; keys starting with `__` are dropped at the edge.
* `pub fn anonymous(params: Params) -> Self`: empty session, no CSRF token, unbound service handle. `query` is empty.
* `pub fn parse_query(raw: &str) -> Params` (free function in `ctx`, re-exported): decodes `+` and `%XX`, drops empty keys and `__`-prefixed keys.
* `pub fn identity_value(&self) -> Option<Value>`: the session identity as `Value::Map` with `subject` and `claims`, which is what reaches evaluators as the `identity` prop.

Cloning a context shares the session cell and the service handle; only `params` and `csrf` are copied.

## 10. Services

### `ServiceCaller`

`pub trait ServiceCaller: Send + Sync`. What the service layer implements.

* `fn call(&self, service: &str, method: &str, args: ValueMap) -> BoxFuture<'static, Result<Value, ServiceError>>`

### `ServiceHandle`

`ctx.services`. `Clone + Default`; the default is unbound.

* `pub fn new(caller: Arc<dyn ServiceCaller>) -> Self`
* `pub fn is_bound(&self) -> bool`
* `pub fn call(&self, service: &str, method: &str, args: ValueMap) -> BoxFuture<'static, Result<Value, ServiceError>>`: an unbound handle fails the call rather than pretending, with `FailureKind::Unavailable` and the message `no service layer is bound to this request`.

The handle is bound to the request before application code reaches it, so identity and credentials are attached to a call without being readable from the context. It exposes no accessor for the caller it holds.

## 11. Actions

### `ActionHandler`

`pub trait ActionHandler: Send + Sync`.

* `fn call(&self, ctx: RequestCtx, input: Value) -> BoxFuture<'static, Result<Value, ActionError>>`: the context arrives by value.

### `ActionRegistry`

Stable action ids to handlers, insertion-ordered. `Default`.

* `pub fn new() -> Self`
* `pub fn insert(&mut self, id: impl Into<String>, handler: Arc<dyn ActionHandler>)`
* `pub fn insert_fn<F, Fut>(&mut self, id: impl Into<String>, f: F)` where `F: Fn(RequestCtx, Value) -> Fut + Send + Sync + 'static` and `Fut: Future<Output = Result<Value, ActionError>> + Send + 'static`
* `pub fn dispatch(&self, id: &str, ctx: RequestCtx, input: Value) -> BoxFuture<'static, Result<Value, ActionError>>`: an unknown id resolves to `ActionError` with `FailureKind::NotFound` and the message ``no action `{id}` ``, never a panic. Emits a DEBUG event on target `fsr::action`.

## 12. Streaming

Both functions take the `Assembly` by value and yield `String` chunks. A resolution that introduces new pending slots has them folded into the working set, so nested deferral needs no separate call.

### `wire_stream`

```rust
pub fn wire_stream(assembly: Assembly) -> impl Stream<Item = String> + Send
```

The wire encoding of a streamed response. The first item is three newline-terminated rows in one string:

* `V {"fmt":<FORMAT_VERSION>,"enc":"json"}`
* `N <node row json>`, the tree, from `snapfire_fsr_payload::node_to_row_json`.
* `G <segment json>`, the sidecar, from [`segments_to_json`](#segments_to_json).

Then one item per resolution, `S <slot id> <node row json>\n`, in completion order rather than plan order. The stream ends when no slot is outstanding. Emits a DEBUG event on target `fsr::stream` per resolution.

### `html_stream`

```rust
pub fn html_stream(assembly: Assembly) -> impl Stream<Item = String> + Send
```

The first-response encoding. The first item is the tree serialized with each segment wrapped in `<!--sf-g:{key}-->` and `<!--/sf-g-->`, followed by the sidecar as `<script type="application/json" data-sf-segments>...</script>`, followed by [`FILL_SCRIPT`](#fill_script) when `assembly.pending` is non-empty.

Then one item per resolution:

```text
<template data-sf-fill="{slot}">{subtree}</template><script>__sfFill({slot})</script>
```

* Segment keys are escaped for the comment delimiter: `%` becomes `%25` and `-` becomes `%2D`, so a key can never contain `--`.
* `<` in the sidecar JSON is escaped to `\u003c`, so it cannot terminate its own script tag.
* One `HtmlSession` spans the whole response, so island ids (`sf-i0` upward) stay unique across chunks: a late slot continues the sequence rather than restarting it.
* Slot-addressed child segments are not recursed into while serializing the first chunk. Their DOM region is the `data-sf-slot` element `Node::Pending` produces.

### `segments_to_json`

```rust
pub fn segments_to_json(info: &SegmentInfo) -> serde_json::Value
```

The compact sidecar encoding, keys in this order: `k` the segment key, then `s` the slot id when the segment is deferred or `p` the path when it is not, then `c` the children.

### `FILL_SCRIPT`

`pub const FILL_SCRIPT: &str`. A `<script>` element defining `__sfFill(n)`, installed once ahead of the first fill. It replaces the `[data-sf-slot="n"]` element with the content of `template[data-sf-fill="n"]`, removes the template and dispatches a `sf:fill` `CustomEvent` on `document` whose `detail` is the slot number, which is how the boot runtime learns to rescan the inserted subtree.

## 13. Error handling

### `FailureKind`

The failure shapes a UI has to render, shared by actions and services so no application re-invents the mapping. `Debug + Clone + Copy + PartialEq + Eq`.

| Variant | `as_str()` | `http_status()` |
| :--- | :--- | :--- |
| `Unauthorized` | `unauthorized` | 401 |
| `NotFound` | `not_found` | 404 |
| `Invalid` | `invalid` | 400 |
| `Conflict` | `conflict` | 409 |
| `Timeout` | `timeout` | 504 |
| `Unavailable` | `unavailable` | 503 |
| `Internal` | `internal` | 500 |

* `pub fn as_str(&self) -> &'static str`
* `pub fn http_status(&self) -> u16`

### `LoadError`

A data source failed. `Debug + Clone + thiserror::Error`, displayed as ``data source {source_id}: {message}``.

* `pub source_id: String`
* `pub message: String`

Raised by a `DataSource`. In assembly it degrades one segment to its error module rather than failing the request. It disqualifies the whole enclosing subtree from being cached. Its display string is the `error` prop the error module receives. It is not a hard error unless a caller converts it into `AssembleError::Load` itself.

### `EvalError`

An evaluator failed. `Debug + Clone + thiserror::Error`, displayed as ``evaluate {module}: {message}``.

* `pub module: String`
* `pub message: String`

Reaching `assemble` it becomes `AssembleError::Eval`. Inside a deferred resolution it becomes that slot's error node instead.

### `AssembleError`

What fails a request. `Debug + thiserror::Error`.

* `MissingDataSource(String)`: ``no data source registered for `{0}` ``. A plan names a source the runtime never registered. Misconfiguration, not a runtime condition to degrade around.
* `Load(LoadError)`: transparent, via `From<LoadError>`. Not produced by `assemble`, which degrades a failed loader to the segment's error node.
* `Eval(EvalError)`: transparent, via `From<EvalError>`. An evaluator failed outside a deferred resolution.
* `MissingSlot { node: u32, slot: String }`: ``evaluator asked for slot `{slot}` and plan node {node} has no child there``.
* `SlotInFallback(String)`: ``fallback module `{0}` may not contain slots``. Raised for a slot marker in a `fallback` module or in an `error` module.

### `ActionError`

`Debug + Clone + thiserror::Error`, displayed as ``action failed ({kind}): {message}`` with the kind rendered by `as_str()`.

* `pub kind: FailureKind`
* `pub message: String`
* `pub fn new(kind: FailureKind, message: impl Into<String>) -> Self`

### `ServiceError`

`Debug + Clone + thiserror::Error`, displayed as ``{service}.{method} failed ({kind}): {message}``.

* `pub kind: FailureKind`
* `pub service: String`
* `pub method: String`
* `pub message: String`
* `pub fn new(kind: FailureKind, service: impl Into<String>, method: impl Into<String>, message: impl Into<String>) -> Self`

Because the kind is the same vocabulary, converting one into an `ActionError` or a `LoadError` needs no translation table.
