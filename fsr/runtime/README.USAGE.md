# Usage Guide: snapfire_fsr_runtime

How to wire a request through the runtime: matching a path, resolving a plan, loading its data, evaluating its modules, assembling a payload and streaming it to a browser.

## Table of Contents

* [Core Concepts](#core-concepts)
* [Quick Start](#quick-start)
  * [Rendering a page to HTML](#rendering-a-page-to-html)
  * [Streaming a deferred segment over the wire format](#streaming-a-deferred-segment-over-the-wire-format)
* [Building a Runtime](#building-a-runtime)
* [Matching a Path](#matching-a-path)
* [Resolving an Entry to a Plan](#resolving-an-entry-to-a-plan)
* [Loading Data Before Rendering](#loading-data-before-rendering)
* [Writing an Evaluator](#writing-an-evaluator)
* [Assembling the Payload](#assembling-the-payload)
* [Deferring a Segment](#deferring-a-segment)
* [Streaming the First HTML Response](#streaming-the-first-html-response)
* [Streaming a Navigation Payload](#streaming-a-navigation-payload)
* [Caching Evaluated Subtrees](#caching-evaluated-subtrees)
* [Invalidating What Changed](#invalidating-what-changed)
* [Keying Segments for Navigation](#keying-segments-for-navigation)
* [Carrying Request State](#carrying-request-state)
* [Calling a Service from a Loader](#calling-a-service-from-a-loader)
* [Registering and Dispatching Actions](#registering-and-dispatching-actions)
* [Degrading a Failed Segment](#degrading-a-failed-segment)
* [Tracing a Request](#tracing-a-request)
* [Error Handling](#error-handling)

## Core Concepts

* **Entry** (`EntryId`): the stable id a matcher hands back for a route pattern. The application picks the numbers.
* **Route match** (`RouteMatch`): an entry plus the params the path yielded, as an ordered string map.
* **Plan** (`PlanNode`, from `snapfire_fsr_core`): the tree the resolver produces. Each node names a module and holds children keyed by slot name, plus an optional data source, error module, fallback module and cache key.
* **Data source**: an async loader registered under an id, run before any evaluation begins.
* **Evaluator**: turns a module plus props into a stream of chunks. It never fetches, never suspends and never sees the tree around it.
* **Chunk**: one item of that stream, either a finished `Node` or a `Slot` marker naming where a plan child's subtree lands.
* **Slot name**: the key on a plan child. The name `head` is reserved for the head node passed to `assemble`.
* **Assembly**: what `assemble` returns, being the payload tree, the still-unresolved deferrals and the segment sidecar.
* **Pending**: the hole the assembler leaves for a deferred child, carrying the slot id and the rendered fallback. Evaluators never produce one.
* **Segment**: a region of the page with its own identity, cacheability and error boundary, one per plan node reached through a slot.
* **Segment key**: the comparable identity of a segment across two responses, produced by a `SegmentKeyer`. Same key means the DOM and island state survive a navigation.
* **Composed cache key**: the string the assembler builds from the plan's `cache_key`, the matched params, the identity subject and the subtree's data fingerprint.
* **Request context** (`RequestCtx`): everything a loader or action may know about the request. Serializable values plus a service handle, nothing else.
* **Identity**: a subject string plus claims, resolved by the session layer before anything loads. Application code never sees a token.
* **Failure kind** (`FailureKind`): the one failure vocabulary shared by actions and services, mapping to HTTP statuses at the edge.

## Quick Start

### Rendering a page to HTML

A route, a plan of two nodes, one evaluator and a rendered response. The `.tsx` child has no registered evaluator, so it falls through to `NullEvaluator` and ships as an island for the browser to mount.

```rust
use std::sync::Arc;

use futures::executor::block_on;
use futures_util::{stream, StreamExt};
use snapfire_fsr_core::{Data, ModuleId, Node, NodeId, PlanNode, SlotName};
use snapfire_fsr_runtime::{
  assemble, html_stream, Chunk, DataSources, EntryId, Evaluator, Evaluators, Matcher,
  MatchitMatcher, NodeChunks, RequestCtx, Resolver, Runtime, TableResolver,
};

const DASH: EntryId = EntryId(0);

struct Shell;

impl Evaluator for Shell {
  fn evaluate(&self, _module: &ModuleId, _props: &Data) -> NodeChunks {
    Box::pin(stream::iter([
      Ok(Chunk::Node(Node::raw("<main>"))),
      Ok(Chunk::Slot(SlotName("head".into()))),
      Ok(Chunk::Slot(SlotName("content".into()))),
      Ok(Chunk::Node(Node::raw("</main>"))),
    ]))
  }
}

fn main() {
  let mut matcher = MatchitMatcher::new();
  matcher.insert("/dash/{section}", DASH).expect("route pattern");

  let mut plan = PlanNode::new(NodeId(0), ModuleId::new("shell.tera", "default"));
  plan.children.push((
    SlotName("content".into()),
    PlanNode::new(NodeId(1), ModuleId::new("components/App.tsx", "default")),
  ));

  let mut resolver = TableResolver::new();
  resolver.insert(DASH, plan);

  let mut evaluators = Evaluators::new();
  evaluators.register(|m: &ModuleId| m.path == "shell.tera", Arc::new(Shell));
  let runtime = Runtime::new(DataSources::new(), evaluators);

  let matched = matcher.match_path("/dash/servers").expect("route matches");
  let plan = resolver.resolve(matched.entry, &matched.params).expect("entry has a plan");
  let ctx = RequestCtx::anonymous(matched.params);
  let head = Node::raw("<title>Fleet</title>");

  let assembly = block_on(assemble(&runtime, &plan, &ctx, &head)).expect("assembly");
  let html: String = block_on(html_stream(assembly).collect::<Vec<_>>()).concat();
  println!("{html}");
}
```

### Streaming a deferred segment over the wire format

The same shape with one child marked `deferred`. The first row carries the tree with a `Pending` hole and the fallback inside it; the resolution row follows when the loader completes.

```rust
use std::sync::Arc;

use futures::executor::block_on;
use futures_util::{stream, StreamExt};
use snapfire_fsr_core::{
  Data, DataSourceId, ModuleId, Node, NodeId, Params, PlanNode, SlotName, Value, ValueMap,
};
use snapfire_fsr_runtime::{
  assemble, wire_stream, Chunk, DataSources, Evaluator, Evaluators, NodeChunks, RequestCtx, Runtime,
};

struct Fixed(&'static str);

impl Evaluator for Fixed {
  fn evaluate(&self, _module: &ModuleId, props: &Data) -> NodeChunks {
    let late = match props.get("late") {
      Some(Value::Str(s)) => format!("<late>{s}</late>"),
      _ => String::new(),
    };
    Box::pin(stream::iter([Ok(Chunk::Node(Node::raw(format!("{}{late}", self.0))))]))
  }
}

struct Host;

impl Evaluator for Host {
  fn evaluate(&self, _module: &ModuleId, _props: &Data) -> NodeChunks {
    Box::pin(stream::iter([
      Ok(Chunk::Node(Node::raw("<shell>"))),
      Ok(Chunk::Slot(SlotName("chart".into()))),
      Ok(Chunk::Node(Node::raw("</shell>"))),
    ]))
  }
}

fn main() {
  let mut chart = PlanNode::new(NodeId(1), ModuleId::new("chart.tera", "default"));
  chart.deferred = true;
  chart.fallback = Some(ModuleId::new("loading.tera", "default"));
  chart.data_source = Some(DataSourceId("chart_loader".into()));

  let mut plan = PlanNode::new(NodeId(0), ModuleId::new("shell.tera", "default"));
  plan.children.push((SlotName("chart".into()), chart));

  let mut sources = DataSources::new();
  sources.insert_fn("chart_loader", |_ctx| async {
    let mut data = ValueMap::new();
    data.insert("late".to_owned(), Value::str("ready"));
    Ok(data)
  });

  let mut evaluators = Evaluators::new();
  evaluators.register(|m: &ModuleId| m.path == "shell.tera", Arc::new(Host));
  evaluators.register(|m: &ModuleId| m.path == "chart.tera", Arc::new(Fixed("<chart>")));
  evaluators.register(|m: &ModuleId| m.path == "loading.tera", Arc::new(Fixed("<skl></skl>")));
  let runtime = Runtime::new(sources, evaluators);

  let ctx = RequestCtx::anonymous(Params::new());
  let assembly = block_on(assemble(&runtime, &plan, &ctx, &Node::raw(""))).expect("assembly");
  assert_eq!(assembly.pending.len(), 1);

  for row in block_on(wire_stream(assembly).collect::<Vec<_>>()) {
    print!("{row}");
  }
}
```

The first item is the header block and the second is the resolution:

```text
V {"fmt":1,"enc":"json"}
N ["q",[["r","<shell>"],["p",1,["r","<skl></skl>"]],["r","</shell>"]]]
G {"k":"shell.tera#default","p":[],"c":[{"k":"chart.tera#default","s":1,"c":[]}]}
S 1 ["r","<chart><late>ready</late>"]
```

## Building a Runtime

`Runtime::builder()` is the general form. Every part has a default, so set only what differs.

```rust
use std::sync::Arc;
use std::time::Duration;

use snapfire_fsr_runtime::{DataSources, Evaluators, FibreCache, Runtime};

let runtime = Runtime::builder()
  .sources(sources)
  .evaluators(evaluators)
  .cache(Arc::new(FibreCache::bounded(1024, Duration::from_secs(300))))
  .build();
```

The defaults are an empty `DataSources`, an empty `Evaluators` (so every module falls to `NullEvaluator`), `DefaultKeyer` and `NoCache`. Two shorthands cover the common cases:

```rust
let runtime = Runtime::new(sources, evaluators);
let runtime = Runtime::with_keyer(sources, evaluators, Arc::new(MyKeyer));
```

`build` returns `Arc<Runtime>`, which is what `assemble` takes. Its fields stay public, so the cache and the sources remain reachable after the build:

```rust
runtime.cache.invalidate("dash_page").await;
let source = runtime.sources.get(&DataSourceId("meta_loader".into()));
```

## Matching a Path

`MatchitMatcher` wraps `matchit`, so patterns are `matchit` patterns and named captures become params.

```rust
use snapfire_fsr_runtime::{EntryId, Matcher, MatchitMatcher};

pub const DASH: EntryId = EntryId(0);
pub const SLOW: EntryId = EntryId(1);

let mut matcher = MatchitMatcher::new();
matcher.insert("/dash/{section}", DASH).expect("route pattern");
matcher.insert("/slow/{section}", SLOW).expect("route pattern");

let matched = matcher.match_path("/dash/servers").expect("route matches");
assert_eq!(matched.entry, DASH);
assert_eq!(matched.params["section"], "servers");
```

`insert` returns `matchit::InsertError` for a conflicting or malformed pattern, so route tables are validated at construction. `match_path` returns `None` for no match, which is the caller's 404.

Any other router works: implement `Matcher` and hand back the same `RouteMatch`.

```rust
struct StaticMatcher;

impl Matcher for StaticMatcher {
  fn match_path(&self, path: &str) -> Option<RouteMatch> {
    (path == "/").then(|| RouteMatch { entry: EntryId(0), params: Params::new() })
  }
}
```

## Resolving an Entry to a Plan

`TableResolver` is the minimal resolver: one prebuilt plan per entry, params ignored.

```rust
use snapfire_fsr_core::{CacheKey, DataSourceId, ModuleId, NodeId, PlanNode, SlotName};
use snapfire_fsr_runtime::{Resolver, TableResolver};

fn dash_plan() -> PlanNode {
  let mut page = PlanNode::new(NodeId(1), ModuleId::new("page.tera", "default"));
  page.data_source = Some(DataSourceId("servers_loader".into()));
  page.cache_key = Some(CacheKey("dash_page".into()));
  page.error = Some(ModuleId::new("error_section.tera", "default"));

  let mut layout = PlanNode::new(NodeId(0), ModuleId::new("layout.tera", "default"));
  layout.data_source = Some(DataSourceId("layout_loader".into()));
  layout.children.push((SlotName("content".into()), page));
  layout
}

let mut resolver = TableResolver::new();
resolver.insert(DASH, dash_plan());

let plan = resolver.resolve(matched.entry, &matched.params).expect("entry has a plan");
```

Node ids must be distinct within one plan: the assembler keys loaded data and load failures by `PlanNode::id`, so two nodes sharing an id share a loader result.

Layout conventions, route groups and interception belong to a richer resolver. `resolve` receives the params, so a resolver is free to return a different plan per param value.

## Loading Data Before Rendering

A data source is an async function of the request context, registered under the id the plan node names.

```rust
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_runtime::{DataSources, LoadError};

let mut sources = DataSources::new();

sources.insert_fn("layout_loader", |ctx| async move {
  let visits = match ctx.session.get("visits") {
    Some(Value::Int(n)) => n + 1,
    _ => 1,
  };
  ctx.session.insert("visits", Value::Int(visits));

  let mut data = ValueMap::new();
  data.insert("nav_label".to_owned(), Value::str("Snapfire FSR"));
  data.insert("visits".to_owned(), Value::Int(visits));
  Ok(data)
});
```

`assemble` collects every source in the subtree first and fires them together, so a layout loader never blocks its child. The walk stops at a deferred node: a deferred child's own source and its descendants' sources are not part of the eager wave, they run when the slot resolves.

Two failure modes, deliberately different:

```rust
sources.insert_fn("servers_loader", |_ctx| async {
  Err(LoadError { source_id: "servers_loader".into(), message: "backend down".into() })
});
```

A `LoadError` degrades that one segment to its error module and the rest of the page still renders. A plan node naming a source that was never registered is `AssembleError::MissingDataSource` and fails the whole assembly, because misconfiguration is not a runtime condition to degrade around.

For a source held elsewhere, implement the trait and insert it directly:

```rust
use futures_util::future::BoxFuture;
use snapfire_fsr_runtime::{DataSource, RequestCtx};

struct Static;

impl DataSource for Static {
  fn load(&self, _ctx: &RequestCtx) -> BoxFuture<'static, Result<Data, LoadError>> {
    Box::pin(async { Ok(ValueMap::new()) })
  }
}

sources.insert("static", Arc::new(Static));
```

## Writing an Evaluator

An evaluator receives a module id and the props for one node, and returns a stream of chunks. It has no access to the plan, the tree or the request, and it cannot produce a `Pending`: holes belong to the assembler.

```rust
use futures_util::stream;
use snapfire_fsr_core::{Data, ModuleId, Node, SlotName};
use snapfire_fsr_runtime::{Chunk, Evaluator, NodeChunks};

struct Layout;

impl Evaluator for Layout {
  fn evaluate(&self, _module: &ModuleId, props: &Data) -> NodeChunks {
    let label = match props.get("nav_label") {
      Some(Value::Str(s)) => s.clone(),
      _ => String::new(),
    };
    Box::pin(stream::iter([
      Ok(Chunk::Node(Node::raw(format!("<nav>{label}</nav>")))),
      Ok(Chunk::Slot(SlotName("head".into()))),
      Ok(Chunk::Slot(SlotName("content".into()))),
    ]))
  }
}
```

Registration is by predicate over the module id, and the first rule that matches wins, so register narrow rules before broad ones:

```rust
let mut evaluators = Evaluators::new();
evaluators.register(|m: &ModuleId| m.path.ends_with(".tera"), Arc::new(TeraEvaluator::new(templates())));
```

A module no rule matches goes to `NullEvaluator`, which emits `Node::Client { module, props, children: [], ssr: None }`: the browser mounts it and the props ride along. Registering nothing at all is therefore a working configuration, and it is the one where the server runs no JavaScript.

The props an evaluator sees are the node's loaded data with three request keys written over the top, in this order:

* `params`: a `Value::Map` of the matched params, always present.
* `identity`: `{ subject, claims }`, present when the session resolved one.
* `csrf_token`: a `Value::Str`, present when the edge supplied one.

A loader that writes a key called `params`, `identity` or `csrf_token` has it replaced.

`Chunk::Slot(SlotName("head"))` is reserved: the assembler substitutes the head node passed to `assemble` and marks the subtree as head-using, which makes it ineligible for caching. Any other slot name must match a plan child, or assembly fails with `AssembleError::MissingSlot`.

## Assembling the Payload

```rust
let assembly = assemble(&runtime, &plan, &ctx, &head_node).await?;
```

The result carries three things:

```rust
assembly.tree;      // Node, the payload tree, holes included
assembly.pending;   // Vec<PendingResolution>, one per deferred slot reached so far
assembly.segments;  // SegmentInfo, the sidecar naming every segment and where it sits
```

The head node is the application's, not the runtime's. The example app computes it from a metadata source on the route before assembling, so the title is data like any other:

```rust
let title = match source.load(&ctx).await {
  Ok(data) => match data.get("title") {
    Some(Value::Str(title)) => title.clone(),
    _ => "Snapfire FSR".to_owned(),
  },
  Err(_) => "Snapfire FSR".to_owned(),
};
let assembly = assemble(&runtime, &plan, &ctx, &head_node(&title)).await?;
```

One shape to know: a node whose evaluator emitted exactly one chunk collapses to that node rather than a one-element `Node::Seq`, and its child segments then carry an empty `path`, meaning the whole node. With two or more chunks the tree is a `Seq` and each child segment carries `path: [index]`.

## Deferring a Segment

Deferral is a plan property. Set `deferred` and give the node a `fallback` module:

```rust
let mut chart = PlanNode::new(NodeId(2), ModuleId::new("chart_section.tera", "default"));
chart.data_source = Some(DataSourceId("slow_chart_loader".into()));
chart.deferred = true;
chart.fallback = Some(ModuleId::new("chart_loading.tera", "default"));

page.children.push((SlotName("chart".into()), chart));
```

At that slot the assembler allocates a `SlotId` (numbering from 1, unique per response), renders the fallback module from the request props alone, emits `Node::Pending { slot, fallback }` in its place and pushes a `PendingResolution` onto `assembly.pending`. A node with `deferred` set and no `fallback` gets `Node::raw("")` as its fallback.

Deferral nests. A resolution runs the whole subtree, so a deferred grandchild produces a new `Pending` inside the resolved node and a new `PendingResolution` in `Resolved::pending`, which both streams fold back into their working set:

```rust
let rows = wire_stream(assembly).collect::<Vec<_>>().await;
```

Row `S 1` then carries `["p",2,...]` for the inner hole and row `S 2` follows it.

Resolutions never fail. A failed loader or a failed evaluation inside a deferred subtree resolves the slot to an error node, so the response is always complete.

A fallback module may not contain a slot marker; one is `AssembleError::SlotInFallback`. The same holds for an error module.

## Streaming the First HTML Response

`html_stream` produces the browser's first response: one chunk for the tree, then one chunk per resolution.

```rust
use futures_util::StreamExt;

let mut chunks = html_stream(assembly);
while let Some(chunk) = chunks.next().await {
  write_to_response(chunk).await?;
}
```

The first chunk is the tree with each segment wrapped in comment delimiters, the sidecar as an inert script and, when anything is pending, the fill script:

```text
<!--sf-g:shell.tera#default--><sf-i id="sf-i0" data-sf-module="components/Nav.tsx#default"></sf-i><script type="application/json" data-sf-props="sf-i0">{}</script><div data-sf-slot="1"><skl></skl></div><!--/sf-g--><script type="application/json" data-sf-segments>{"k":"shell.tera#default","p":[],"c":[{"k":"chart.tera#default","s":1,"c":[]}]}</script>
```

Segment keys are escaped before they go into a comment: `%` becomes `%25` and `-` becomes `%2D`, so a key can never contain `--` and close its own delimiter.

Every later chunk has the same shape, an inert template plus the call that moves it into place:

```text
<template data-sf-fill="{slot}">{subtree}</template><script>__sfFill({slot})</script>
```

`FILL_SCRIPT` is the definition of `__sfFill`, emitted once in the first chunk when `assembly.pending` is non-empty. It replaces the `data-sf-slot` element with the template's content and dispatches a `sf:fill` event carrying the slot number, which is how the boot runtime learns to rescan the inserted subtree for islands.

Island ids stay unique across the whole response because one `HtmlSession` spans every chunk: the initial tree takes `sf-i0` upward and a late slot continues the sequence rather than restarting it.

## Streaming a Navigation Payload

`wire_stream` is the same assembly in the encoding the browser client reads on a client-side navigation.

```rust
let stream = match mode {
  RenderMode::Html => Box::pin(html_stream(assembly)) as BoxStream<'static, String>,
  RenderMode::Payload => Box::pin(wire_stream(assembly)),
};
```

The first item is three rows in one string:

* `V {"fmt":1,"enc":"json"}`, the format version and encoding.
* `N <row json>`, the payload tree.
* `G <segment json>`, the segment sidecar.

Then one `S <slot> <row json>` row per resolution, in completion order rather than plan order. The sidecar encoding is compact: `k` is the segment key, `c` holds the children and the position is either `p`, the path, or `s`, the slot id, when the segment is deferred.

```rust
use snapfire_fsr_runtime::segments_to_json;

let json = segments_to_json(&assembly.segments);
assert_eq!(json["k"], "shell.tera#default");
```

Pick `html_stream` for a document request and `wire_stream` for a navigation or a prefetch. Both consume the `Assembly` by value, so choose before you stream.

## Caching Evaluated Subtrees

Give a plan node a `cache_key` and give the runtime a cache. A hit restores the subtree and its segment sidecar without evaluating anything.

```rust
page.cache_key = Some(CacheKey("dash_page".into()));

let runtime = Runtime::builder()
  .sources(sources)
  .evaluators(evaluators)
  .cache(Arc::new(FibreCache::bounded(1024, Duration::from_secs(300))))
  .build();
```

The key the cache actually sees is composed by the assembler, not by the plan. It is four fields joined by `|`:

```text
{plan cache_key}|{k=v pairs, sorted, joined by &}|ident={identity subject or -}|{16 hex digits of the subtree data fingerprint}
```

So a request for `/dash/servers` as `alice`, whose loader returned the fingerprint `3f2a...`, looks up:

```text
dash_page|section=servers|ident=alice|3f2a9c1d40b7e558
```

Each field closes a way of serving the wrong bytes. Params are in the key, so `/dash/servers` and `/dash/network` are separate entries. The identity subject is in the key, so one user's page is never handed to another, and an anonymous request keys on `-`. The fingerprint covers the whole subtree's loaded data, hashed over the plan node ids in tree order, so changed data is a miss rather than a stale hit.

Three things disqualify a subtree from caching:

* A `deferred` descendant anywhere below it. Slot ids are allocated per response, so a tree containing `Pending` cannot be replayed.
* A failed loader anywhere in the subtree. A degraded segment is never written, so recovery does not need an invalidation.
* Use of the `head` slot. Head content is per request, so a subtree that consumed it is evaluated every time and never written to the cache.

Which store to pick:

```rust
Arc::new(NoCache)                                          // the default: correct, never memoizes
Arc::new(MemoryCache::new())                               // unbounded, no expiry, fine for tests
Arc::new(FibreCache::bounded(1024, Duration::from_secs(300)))  // bounded and TTL-expiring
```

`FibreCache::bounded_sharded(capacity, ttl, shards)` tunes lock contention. Shards are rounded up to the next power of two and capacity is accounted across all of them, so a higher shard count trades fixed per-shard structures against contention, never against usable capacity. For a cache you configure yourself, build the `fibre_cache::Cache` and pass it:

```rust
let cache = FibreCache::new(
  fibre_cache::CacheBuilder::default()
    .capacity(64)
    .time_to_live(Duration::from_secs(60))
    .shards(2)
    .build()
    .unwrap(),
);
```

## Invalidating What Changed

Invalidation takes the plan's `cache_key`, not a composed key, and drops every entry that key produced across all params and all identities.

```rust
runtime.cache.invalidate("dash_page").await;
```

Tags are keys and revalidation is invalidation: after an action mutates the fleet, invalidating `dash_page` is what makes the next request re-evaluate. `MemoryCache` does it by dropping every entry whose key starts with `dash_page|`; `FibreCache` keeps a side index from plan key to composed keys, so it invalidates exactly without scanning. That index is populated on `put` and cleared for a plan key on `invalidate`, so it grows with the number of distinct composed keys a plan key has produced since the last invalidation.

`NodeCache` is three methods, so a Redis-backed or two-tier store is a small implementation:

```rust
impl NodeCache for MyCache {
  fn get(&self, key: &str) -> BoxFuture<'_, Option<CacheEntry>> { /* ... */ }
  fn put(&self, key: String, entry: CacheEntry) -> BoxFuture<'_, ()> { /* ... */ }
  fn invalidate(&self, cache_key: &str) -> BoxFuture<'_, ()> { /* ... */ }
}
```

A `CacheEntry` is the evaluated `Node` plus its `Vec<SegmentInfo>` sidecar. Storing only the node would lose navigation identity on a hit, so both go in together.

## Keying Segments for Navigation

The segment key decides what survives a navigation. Same key across two responses means the region's DOM and island state are kept; a different key means it is replaced from that point down. Content changes are not identity changes.

`DefaultKeyer` is the module id, plus every matched param when there is one:

```text
page.tera#default?section=servers
```

A resolver whose plans depend on fewer params pairs with a narrower keyer, so a param that changes nothing about a segment does not evict it:

```rust
use snapfire_fsr_core::{Params, PlanNode};
use snapfire_fsr_runtime::SegmentKeyer;

struct SectionKeyer;

impl SegmentKeyer for SectionKeyer {
  fn key(&self, plan: &PlanNode, params: &Params) -> String {
    match params.get("section") {
      Some(section) => format!("{}?section={section}", plan.module),
      None => plan.module.to_string(),
    }
  }
}

let runtime = Runtime::builder().keyer(Arc::new(SectionKeyer)).build();
```

The sidecar mirrors the tree. `path` locates a segment's subtree relative to its parent segment's node, where `[]` is the whole node and `[i]` is child `i` of a `Seq`. A deferred segment carries `slot: Some(id)` and no path, because its region in the DOM is the `data-sf-slot` element instead.

## Carrying Request State

`RequestCtx` is built at the HTTP edge, after the session layer has resolved identity and before a route is even matched.

```rust
use snapfire_fsr_runtime::{RequestCtx, SessionCell};

let ctx = RequestCtx {
  params: matched.params,
  session: incoming.session,
  csrf: incoming.csrf,
  services: app.services.bind(incoming.session.identity(), incoming.credentials),
};
```

For a test or a request with no session at all:

```rust
let ctx = RequestCtx::anonymous(Params::new());
```

`SessionCell` is shared by every loader and action on the request, and every mutation marks it dirty so the session layer knows to persist when the response starts:

```rust
ctx.session.insert("visits", Value::Int(4));   // dirty
ctx.session.get("visits");                      // Option<Value>
ctx.session.remove("visits");                   // dirty when something was removed
ctx.session.set_identity(Some(identity));       // dirty
ctx.session.clear();                            // logout: data and identity, one dirty write
assert!(ctx.session.is_dirty());
let (data, identity) = ctx.session.snapshot();
```

Identity is a subject plus claims, and it reaches evaluators as props rather than being read from the context by templates:

```rust
let ctx_identity = ctx.session.identity();       // Option<Identity>
let as_props = ctx.identity_value();             // Option<Value>, { subject, claims }
```

What is deliberately absent: tokens, headers, the connection, a database handle. The context holds serializable values plus one service handle, which is callable but exposes nothing readable.

## Calling a Service from a Loader

`ctx.services` is the only outward path from a loader or action.

```rust
use snapfire_fsr_core::{Value, ValueMap};

let mut args = ValueMap::new();
args.insert("section".to_owned(), Value::Str(section));

let servers = ctx.services.call("fleet", "list", args).await;
```

The handle was bound to the request before application code saw it, so identity and credentials travel with the call without being reachable from here. An unbound handle, which is what `RequestCtx::default` and `RequestCtx::anonymous` give you, fails the call rather than pretending: `FailureKind::Unavailable` with the message `no service layer is bound to this request`. Check first when that is a legitimate state:

```rust
if ctx.services.is_bound() {
  ctx.services.call("fleet", "count", ValueMap::new()).await?;
}
```

In a loader, map the failure into the loader's own error so the segment degrades rather than the page:

```rust
sources.insert_fn("servers_loader", |ctx| async move {
  let servers = ctx
    .services
    .call("fleet", "list", ValueMap::new())
    .await
    .map_err(|e| LoadError { source_id: "servers_loader".into(), message: e.message })?;

  let mut data = ValueMap::new();
  data.insert("servers".to_owned(), servers);
  Ok(data)
});
```

Implement `ServiceCaller` to bind something of your own; `ServiceHandle::new(Arc::new(caller))` produces the bound handle.

## Registering and Dispatching Actions

An action is a stable id mapped to a handler that takes the request context and one input value.

```rust
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_runtime::{ActionError, ActionRegistry, FailureKind};

let mut actions = ActionRegistry::new();

actions.insert_fn("add_server", |ctx, input| async move {
  let Value::Map(fields) = input else {
    return Err(ActionError::new(FailureKind::Invalid, "input must be a map"));
  };
  let name = match fields.get("name") {
    Some(Value::Str(name)) if !name.is_empty() => name.clone(),
    _ => return Err(ActionError::new(FailureKind::Invalid, "`name` must be a non-empty string")),
  };

  let mut args = ValueMap::new();
  args.insert("name".to_owned(), Value::Str(name));
  ctx
    .services
    .call("fleet", "add", args)
    .await
    .map_err(|e| ActionError::new(e.kind, e.message))
});
```

Dispatch by id from the HTTP edge:

```rust
let result = actions.dispatch("add_server", ctx, input).await;
```

An unknown id is not a panic and not a separate error type: it is `ActionError` with `FailureKind::NotFound` and the message ``no action `add_server` ``. Turn any of them into a status with one call:

```rust
match result {
  Ok(value) => respond_json(200, value),
  Err(e) => respond_json(e.kind.http_status(), error_body(e.kind.as_str(), &e.message)),
}
```

`FailureKind` is shared with the service layer, which is why `ActionError::new(e.kind, e.message)` above needs no translation table: a `Conflict` raised deep in a service is still a `Conflict` at the edge, and still a 409.

## Degrading a Failed Segment

Give a plan node an `error` module and a failed loader renders that module in place of the segment:

```rust
page.error = Some(ModuleId::new("error_section.tera", "default"));
```

The error module is evaluated with the request props plus one more key:

```rust
impl Evaluator for ErrorPartial {
  fn evaluate(&self, _module: &ModuleId, props: &Data) -> NodeChunks {
    let message = match props.get("error") {
      Some(Value::Str(s)) => s.clone(),
      _ => "unavailable".to_owned(),
    };
    Box::pin(stream::iter([Ok(Chunk::Node(Node::raw(format!("<oops>{message}</oops>"))))]))
  }
}
```

Without an `error` module the segment becomes the built-in node, which is `<div data-sf-error>` around the failure message:

```rust
assert!(format!("{:?}", assembly.tree).contains("data-sf-error"));
```

Either way the page is intact: the layout around the failed segment still renders, its siblings still render and `assemble` returns `Ok`. The same path serves a deferred segment, where the failure arrives as an ordinary resolution row carrying the error node.

Nothing about the failure is cached. A subtree containing a failure is not written, so the request after the backend recovers evaluates fresh rather than replaying the error.

## Tracing a Request

Four `tracing` targets, all at DEBUG except loader failures, which are WARN:

| Target | Event |
| :--- | :--- |
| `fsr::load` | a segment loader failed, with the plan node id and the error |
| `fsr::cache` | `hit` or `miss`, with the composed key |
| `fsr::stream` | a deferred slot resolved, with the slot id |
| `fsr::action` | an action was dispatched, with its id |

```rust
tracing_subscriber::fmt()
  .with_env_filter("fsr::cache=debug,fsr::stream=debug")
  .init();
```

The cache events carry the full composed key, which is the fastest way to find out why a hit you expected was a miss: compare the fingerprint field across two requests to tell a data change from a params or identity change.

## Error Handling

Six error types, and the split between them is the design. `LoadError` and a failed evaluation inside a segment degrade that segment. `AssembleError` fails the request. `ActionError` and `ServiceError` carry a `FailureKind` to the edge.

| Type | Raised by | Consequence |
| :--- | :--- | :--- |
| `LoadError` | a data source | the segment degrades to its error module |
| `EvalError` | an evaluator | `AssembleError::Eval` outside a deferral, an error node inside one |
| `AssembleError` | `assemble` | the request fails |
| `ActionError` | an action handler or `dispatch` | `kind.http_status()` |
| `ServiceError` | the service layer through `ServiceHandle::call` | usually mapped into a `LoadError` or `ActionError` |
| `FailureKind` | shared vocabulary | the HTTP status |

Matching on the assembly errors:

```rust
match assemble(&runtime, &plan, &ctx, &head).await {
  Ok(assembly) => Ok(assembly),
  Err(AssembleError::MissingDataSource(id)) => {
    Err(internal_error(format!("no data source `{id}`")))
  }
  Err(AssembleError::MissingSlot { node, slot }) => {
    Err(internal_error(format!("plan node {node} has no `{slot}` child")))
  }
  Err(AssembleError::SlotInFallback(module)) => {
    Err(internal_error(format!("`{module}` may not contain slots")))
  }
  Err(e) => Err(internal_error(e.to_string())),
}
```

And the failure kinds, which are the same seven whether they came from an action or a service:

```rust
let status = match kind {
  FailureKind::Unauthorized => 401,
  FailureKind::Invalid => 400,
  FailureKind::NotFound => 404,
  FailureKind::Conflict => 409,
  FailureKind::Unavailable => 503,
  FailureKind::Timeout => 504,
  FailureKind::Internal => 500,
};
assert_eq!(status, kind.http_status());
```
