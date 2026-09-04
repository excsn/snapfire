# API Reference: snapfire_fsr_plan

The plan file: routes, source rows, action rows and component rows as a build artifact a host reads at boot.

## Contents

* [1. Versions](#1-versions)
  * [FORMAT_VERSION](#format_version)
* [2. The Manifest](#2-the-manifest)
  * [Manifest](#manifest)
  * [RouteEntry](#routeentry)
* [3. Nodes](#3-nodes)
  * [Node](#node)
  * [Child](#child)
* [4. Rows](#4-rows)
  * [RowOwner](#rowowner)
  * [SourceEntry](#sourceentry)
  * [ActionEntry](#actionentry)
  * [ComponentEntry](#componententry)
* [5. Error Handling](#5-error-handling)
  * [PlanError](#planerror)

## 1. Versions

### FORMAT_VERSION

* `pub const FORMAT_VERSION: u32 = 2`: what `Manifest::new` stamps and `to_json` writes. Format 2 adds the `sources` table and makes actions rows.
* A file from version 1 up to `FORMAT_VERSION` reads; anything else is `PlanError::Version`. A format 1 file's bare action ids read as `rust` rows.

## 2. The Manifest

### Manifest

`#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]`

* `pub struct Manifest { pub version: u32, pub routes: Vec<RouteEntry>, pub sources: Vec<SourceEntry>, pub actions: Vec<ActionEntry>, pub components: Vec<ComponentEntry>, pub not_found: Option<Node>, pub handlers: Vec<HandlerEntry> }`. `sources`, `actions`, `components` and `handlers` are absent from the file when empty; `not_found`, the tree a host renders with status 404 for a path no route matches, is absent when `None`.
* `Manifest::new(routes: Vec<RouteEntry>) -> Self`: `FORMAT_VERSION` and no rows.
* `Manifest::with_sources(self, sources: Vec<SourceEntry>) -> Self`
* `Manifest::with_actions(self, actions: Vec<ActionEntry>) -> Self`
* `Manifest::with_components(self, components: Vec<ComponentEntry>) -> Self`
* `Manifest::with_not_found(self, not_found: Option<Node>) -> Self`
* `Manifest::with_handlers(self, handlers: Vec<HandlerEntry>) -> Self`
* `Manifest::lowered_handlers(&self) -> impl Iterator<Item = &HandlerEntry>`
* `Manifest::from_json(source: &str) -> Result<Self, PlanError>`: parses, checks the version and refuses a `lowered` source or action row with no body.
* `Manifest::to_json(&self) -> String`: pretty-printed, in field order.
* `Manifest::routes(&self) -> Result<Vec<(String, PlanNode)>, PlanError>`: the runtime's trees in file order; refuses an empty pattern, a malformed module id, a node id used twice within one route and a slot used twice on one node.
* `Manifest::not_found(&self) -> Result<Option<PlanNode>, PlanError>`: the not-found tree, checked like a route's, at `not_found`.
* `Manifest::sources(&self) -> Vec<String>`: every data source any tree names, the not-found tree included, once, in tree order.
* `Manifest::modules(&self) -> Vec<String>`: every module any tree names, fallback, error and not-found modules included, once, in tree order.
* `Manifest::action_ids(&self) -> Vec<String>`
* `Manifest::lowered_sources(&self) -> impl Iterator<Item = &SourceEntry>`: the rows whose owner is `Lowered`.
* `Manifest::lowered_actions(&self) -> impl Iterator<Item = &ActionEntry>`

### RouteEntry

* `pub struct RouteEntry { pub pattern: String, pub plan: Node }`

## 3. Nodes

### Node

The serialized shape of a `PlanNode`. Every optional field is absent from the file rather than null and `deferred` is absent when false.

* `pub struct Node { pub id: u32, pub module: String, pub source: Option<String>, pub deferred: bool, pub fallback: Option<String>, pub error: Option<String>, pub cache_key: Option<String>, pub children: Vec<Child> }`
* `Node::from_plan(plan: &PlanNode) -> Self`
* `module`, `fallback` and `error` are module ids, `path#export`; `Manifest::routes` refuses any other spelling.

### Child

* `pub struct Child { pub slot: String, pub node: Node }`

## 4. Rows

### RowOwner

`#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]`, serialized in snake case.

* `Lowered`: the row carries a body the host binds unless Rust overrides the name.
* `Engine`: the row is answered by a JavaScript engine.
* `Rust`: a declaration; the host refuses to start unless Rust answers it.
* `RowOwner::as_str(&self) -> &'static str`: `lowered`, `engine` or `rust`.

### SourceEntry

* `pub struct SourceEntry { pub id: String, pub owner: RowOwner, pub module: Option<String>, pub export: Option<String>, pub reason: Option<String>, pub body: Option<Body> }`
* `SourceEntry::lowered(id, module, body: Body) -> Self`: owner `Lowered`, with the module and the body.
* `SourceEntry::rust(id) -> Self`: owner `Rust` and nothing else.
* A `Lowered` row read from a file must carry a body; `PlanError::NoBody` otherwise.

### ActionEntry

* `pub struct ActionEntry { pub id: String, pub owner: RowOwner, pub module: Option<String>, pub export: Option<String>, pub input: Option<String>, pub reason: Option<String>, pub body: Option<Body> }`
* `ActionEntry::lowered(id, module, body: Body) -> Self`
* `ActionEntry::rust(id) -> Self`
* `ActionEntry::with_input(self, input) -> Self`: the contract type a call's input is checked against before the body runs.
* Deserializes from a row or from a bare string, which reads as `rust(id)`.

### HandlerEntry

* `pub struct HandlerEntry { pub id: String, pub method: String, pub pattern: String, pub owner: RowOwner, pub module: Option<String>, pub input: Option<String>, pub reason: Option<String>, pub body: Option<Body> }`: a request the host answers with a value. `method` and `pattern` are what it matches; `id` is `<route id>.<METHOD>`; `input` names the type the body is checked against. A row without a body is a declaration Rust must answer.
* `HandlerEntry::lowered(id, method, pattern, module, body: Body) -> Self`
* `HandlerEntry::rust(id, method, pattern) -> Self`
* `HandlerEntry::with_input(self, input) -> Self`

### ComponentEntry

* `pub struct ComponentEntry { pub module: String, pub body: Component }`: a module lowered to a render tree, `snapfire_fsr_ir::Component`.

## 5. Error Handling

### PlanError

`#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]`

* `Malformed(String)`: the file is not JSON, with serde's message.
* `Version { found: u32 }`: outside 1 to `FORMAT_VERSION`.
* `NoBody { id: String }`: a `lowered` row with no body.
* `Module { at: String, module: String }`: not `path#export`; `at` is the pattern followed by the slot path, with `/fallback` or `/error` for those fields.
* `DuplicateNode { at: String, id: u32 }`: within one route.
* `DuplicateSlot { at: String, slot: String }`: on one node.
* `EmptyPattern`: a route with no pattern.
