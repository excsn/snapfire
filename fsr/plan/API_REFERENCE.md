# API Reference: snapfire_fsr_plan

The plan file `fsr build` writes and a host reads at boot: routes, source rows, action rows, handler rows and component rows.

## Contents

* [1. Versions](#1-versions)
  * [`FORMAT_VERSION`](#format_version)
* [2. The Manifest](#2-the-manifest)
  * [`Manifest`](#manifest)
  * [`RouteEntry`](#routeentry)
  * [`under`](#under)
  * [`renumber`](#renumber)
* [3. Nodes](#3-nodes)
  * [`Node`](#node)
  * [`Child`](#child)
* [4. Rows](#4-rows)
  * [`RowOwner`](#rowowner)
  * [`SourceEntry`](#sourceentry)
  * [`ActionEntry`](#actionentry)
  * [`HandlerEntry`](#handlerentry)
  * [`ComponentEntry`](#componententry)
  * [`HeadRow`](#headrow)
  * [`ClientEntry`](#cliententry)
  * [`ModuleHead`](#modulehead)
  * [`ModuleSheets`](#modulesheets)
  * [`ImageEntry`](#imageentry)
  * [`ThemeEntry`](#themeentry)
  * [`DeriveEntry`](#deriveentry)
* [5. The S-Expression Form](#5-the-s-expression-form)
  * [`manifest_to_sx`](#manifest_to_sx)
  * [`manifest_from_sx`](#manifest_from_sx)
  * [`manifest_from_forms`](#manifest_from_forms)
  * [`SOURCE_SECTIONS`](#source_sections)
* [6. Error Handling](#6-error-handling)
  * [`PlanError`](#planerror)
  * [`ViewError`](#viewerror)
  * [`SexprError`](#sexprerror)

## 1. Versions

### FORMAT_VERSION

* `pub const FORMAT_VERSION: u32 = 7`: the version `Manifest::new` stamps.
* A file from version 1 up to `FORMAT_VERSION` reads. Any other version is `PlanError::Version`.
* A format 1 file's bare action ids read as `rust` rows.
* A component in a file older than format 5 that names no owner reads as `Owner::React`.
* The component sections `(static)`, `(tree)` and `(vue)` read as `Owner::Fsr`, `Owner::React` and `Owner::Vue`.

## 2. The Manifest

### Manifest

The plan file in memory.

* `pub struct Manifest`. `Debug`, `Clone`, `PartialEq`, `Serialize`, `Deserialize`.

Fields. Each `Vec`, `BTreeMap` and `Option` field is absent from the file when empty or `None`, except `routes`.

* `pub version: u32`: the format version.
* `pub routes: Vec<RouteEntry>`: the routes, in file order.
* `pub sources: Vec<SourceEntry>`: one row per data source.
* `pub actions: Vec<ActionEntry>`: one row per action.
* `pub components: Vec<ComponentEntry>`: one row per module lowered to a render tree.
* `pub clients: Vec<ClientEntry>`: one row per module the build could not lower.
* `pub consts: Consts`: the module-level constants bodies read by name, keyed `<file>#<name>`. `Expr::Const` names one.
* `pub not_found: Option<Node>`: the tree a host renders with status 404 for a path no route matches.
* `pub handlers: Vec<HandlerEntry>`: one row per HTTP method a `route.ts` exports.
* `pub middleware: Option<Body>`: the lowered `middleware.ts`.
* `pub intercepts: Vec<RouteEntry>`: one entry per `page.<slot>.tsx`, under the pattern of its route. The tree a soft navigation renders into a live layout's slot.
* `pub frameworks: BTreeMap<String, String>`: the exact version of each vendored framework, by package (`react`, `react-dom`, `vue`).
* `pub reads: BTreeMap<String, Vec<String>>`: the store keys each island module the server does not render reads, by module.
* `pub derives: Vec<DeriveEntry>`: one row per store key a `derive` in the browser's files computes, in declaration order.
* `pub heads: Vec<ModuleHead>`: head rows the build knows for a module from a file beside it.
* `pub sheets: Vec<ModuleSheets>`: the stylesheets each route segment's module links.
* `pub images: Vec<ImageEntry>`: one row per generated card image per page.
* `pub theme: Option<Body>`: the lowered `theme.ts`.
* `pub themes: Vec<ThemeEntry>`: one row per layout or page that sets a theme.

Building:

* `new(routes: Vec<RouteEntry>) -> Self`: version `FORMAT_VERSION` and no rows.
* `with_sources(self, sources: Vec<SourceEntry>) -> Self`
* `with_actions(self, actions: Vec<ActionEntry>) -> Self`
* `with_components(self, components: Vec<ComponentEntry>) -> Self`
* `with_clients(self, clients: Vec<ClientEntry>) -> Self`
* `with_consts(self, consts: Consts) -> Self`
* `with_not_found(self, not_found: Option<Node>) -> Self`
* `with_handlers(self, handlers: Vec<HandlerEntry>) -> Self`
* `with_middleware(self, middleware: Option<Body>) -> Self`
* `with_intercepts(self, intercepts: Vec<RouteEntry>) -> Self`
* `with_frameworks(self, frameworks: BTreeMap<String, String>) -> Self`
* `with_heads(self, heads: Vec<ModuleHead>) -> Self`
* `with_sheets(self, sheets: Vec<ModuleSheets>) -> Self`
* `with_images(self, images: Vec<ImageEntry>) -> Self`
* `with_theme(self, theme: Option<Body>, themes: Vec<ThemeEntry>) -> Self`

Reading and writing:

* `from_text(source: &str) -> Result<Self, PlanError>`: reads a plan file, as `from_sexpr` does.
* `from_sexpr(source: &str) -> Result<Self, PlanError>`: parses a `plan.sexp`, checks the version and refuses a `lowered` source or action row with no body.
* `to_sexpr(&self) -> String`: the text of a `plan.sexp`, one form per top-level line, wrapped at 100 columns.
* `view(&self, id: &str, section: Option<&str>) -> Result<String, ViewError>`: one row printed as `to_sexpr` writes it. `id` is tried as a source id, an action id, a handler id, a component module and a derive key, in that order.
* `view` with `section` set prints one of a source row's `SOURCE_SECTIONS` alone. On any other kind of row it is `ViewError::OneBody`.
* `row_ids(&self) -> Vec<String>`: every id `view` answers: sources, actions, handlers, components and derives, each in file order.

Runtime trees:

* `routes(&self) -> Result<Vec<(String, PlanNode)>, PlanError>`: each pattern and its runtime tree, in file order.
* `routes` refuses an empty pattern, a module that is not `path#export`, a node id used twice within one route and a slot used twice on one node.
* `intercepts(&self) -> Result<Vec<(String, PlanNode)>, PlanError>`: the intercept trees in file order, checked like routes.
* `not_found(&self) -> Result<Option<PlanNode>, PlanError>`: the not-found tree, checked like a route's.

Listing:

* `sources(&self) -> Vec<String>`: every data source a route, intercept or the not-found tree names, once each, in tree order.
* `modules(&self) -> Vec<String>`: every node, fallback and error module a route, intercept or the not-found tree names, once each, in tree order. The modules of `error_kinds` are not listed.
* `action_ids(&self) -> Vec<String>`: every action row's id, in file order.
* `lowered_sources(&self) -> impl Iterator<Item = &SourceEntry>`: the source rows whose owner is `Lowered`.
* `lowered_actions(&self) -> impl Iterator<Item = &ActionEntry>`: the action rows whose owner is `Lowered`.
* `lowered_handlers(&self) -> impl Iterator<Item = &HandlerEntry>`: the handler rows whose owner is `Lowered`.

Rewriting:

* `visit_exprs_mut(&mut self, f: &mut dyn FnMut(&mut Expr))`: calls `f` on every expression in every source, action, handler, image, middleware and theme body, every component, every derive and every constant.
* `namespaced(&self, name: &str, at: &str, shell: &str) -> Manifest`: the manifest as a site's artifact.
* `namespace(&mut self, name: &str, at: &str, shell: &str)`: `namespaced` in place.
* `namespaced` moves every route, intercept, handler and image pattern under `at` with `under`.
* `namespaced` prefixes with `<name>:` every node module, source, fallback, error, cache key and `error_kinds` module. The document module `shell` is not prefixed.
* `namespaced` prefixes every row's `id`, `module` and `input`, every image's `source` and `page`, every `reads` module and every constant's name.
* Inside every body it prefixes each `call`'s service, each `const` name and each island or component placement's module.
* A value that already starts with `<name>:` is left as it is. Store keys and theme names are never prefixed.

### RouteEntry

* `pub struct RouteEntry { pub pattern: String, pub plan: Node }`. `Debug`, `Clone`, `PartialEq`, `Serialize`, `Deserialize`.

### under

* `pub fn under(at: &str, pattern: &str) -> String`: `at` with any trailing `/` removed, joined with `pattern`. `under("/billing", "/")` is `/billing` and `under("/billing/", "/x")` is `/billing/x`.

### renumber

* `pub fn renumber(node: &mut Node, next: &mut u32)`: gives every node of the tree a fresh id in tree order, starting at `*next`. It leaves `*next` one past the last id given.

## 3. Nodes

### Node

The serialized form of a `PlanNode`.

* `pub struct Node { pub id: u32, pub module: String, pub source: Option<String>, pub deferred: bool, pub fallback: Option<String>, pub error: Option<String>, pub error_kinds: Vec<(String, String)>, pub cache_key: Option<String>, pub children: Vec<Child>, pub keep: Vec<String> }`. `Debug`, `Clone`, `PartialEq`, `Serialize`, `Deserialize`.
* Each `Option` field is absent from the file when `None`, each `Vec` field when empty and `deferred` when false.
* `module`, `fallback` and `error` are module ids, `path#export`. `Manifest::routes` refuses any other spelling.
* `error_kinds` pairs a failure kind with the module that renders for it. It is `(error-kind <kind> <module>)` in the s-expression form.
* `keep` names the slots this node leaves unfilled that the browser keeps as they are when the tree arrives as a payload.
* A layout's page is the child in the slot `content`. A parallel slot under `slots/<name>/` is a further child in the slot `<name>`.
* `from_plan(plan: &PlanNode) -> Self`

### Child

* `pub struct Child { pub slot: String, pub node: Node }`. `Debug`, `Clone`, `PartialEq`, `Serialize`, `Deserialize`.

## 4. Rows

### RowOwner

Who answers a row. Serialized in snake case.

* `pub enum RowOwner { Lowered, Engine, Rust }`. `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Serialize`, `Deserialize`.
* `Lowered`: the row carries a body the host binds unless Rust overrides the name.
* `Engine`: a JavaScript engine answers the row.
* `Rust`: a declaration. The host refuses to start unless Rust answers it.
* `as_str(&self) -> &'static str`: `lowered`, `engine` or `rust`.

### SourceEntry

One data source.

* `pub struct SourceEntry { pub id: String, pub owner: RowOwner, pub module: Option<String>, pub export: Option<String>, pub reason: Option<String>, pub body: Option<Body>, pub meta: Option<Body>, pub store: Option<Body>, pub defaults: Option<Body>, pub store_keys: Option<Vec<String>>, pub paths: Option<Body> }`. `Debug`, `Clone`, `PartialEq`, `Serialize`, `Deserialize`.
* Each `Option` field is absent from the file when `None`.
* `meta`: the loader module's `meta`, describing the document from this source's data.
* `store`: the module's `store`, seeding the browser's store from this source's data.
* `defaults`: the module's `defaults`, seeding the browser's store with values that a write the browser holds outranks.
* `store_keys`: the keys `store` and `defaults` can return, inferred at build. It is the `(store-keys ...)` section.
* `paths`: the module's `paths`, naming the parameter sets a route with a parameter prerenders.
* `lowered(id: impl Into<String>, module: impl Into<String>, body: Body) -> Self`: owner `Lowered`, with the module and the body.
* `rust(id: impl Into<String>) -> Self`: owner `Rust` and no other field.
* `with_meta(self, meta: Option<Body>) -> Self`
* `with_store(self, store: Option<Body>) -> Self`
* `with_defaults(self, defaults: Option<Body>) -> Self`
* `with_paths(self, paths: Option<Body>) -> Self`
* A `Lowered` row read from a file must carry a body. `PlanError::NoBody` otherwise.

### ActionEntry

One action.

* `pub struct ActionEntry { pub id: String, pub owner: RowOwner, pub module: Option<String>, pub export: Option<String>, pub input: Option<String>, pub reason: Option<String>, pub body: Option<Body> }`. `Debug`, `Clone`, `PartialEq`, `Serialize`, `Deserialize`.
* `lowered(id: impl Into<String>, module: impl Into<String>, body: Body) -> Self`
* `rust(id: impl Into<String>) -> Self`
* `with_input(self, input: impl Into<String>) -> Self`: the contract type a call's input is checked against before the body runs.
* Deserializes from a row or from a bare string, which reads as `rust(id)`. A row with no `owner` reads as `Rust`.
* A `Lowered` row read from a file must carry a body. `PlanError::NoBody` otherwise.

### HandlerEntry

A request the host answers with a value instead of a document.

* `pub struct HandlerEntry { pub id: String, pub method: String, pub pattern: String, pub owner: RowOwner, pub module: Option<String>, pub input: Option<String>, pub reason: Option<String>, pub body: Option<Body> }`. `Debug`, `Clone`, `PartialEq`, `Serialize`, `Deserialize`.
* `method` and `pattern` are what the host matches. `id` is `<route id>.<METHOD>`.
* `input` names the type the body's input is checked against.
* A row without a body is a declaration Rust must answer.
* `lowered(id: impl Into<String>, method: impl Into<String>, pattern: impl Into<String>, module: impl Into<String>, body: Body) -> Self`
* `rust(id: impl Into<String>, method: impl Into<String>, pattern: impl Into<String>) -> Self`
* `with_input(self, input: impl Into<String>) -> Self`

### ComponentEntry

A module lowered to a render tree.

* `pub struct ComponentEntry { pub module: String, pub body: Component, pub head: Vec<HeadRow> }`. `Debug`, `Clone`, `PartialEq`, `Serialize`, `Deserialize`.
* `body` is a `snapfire_fsr_ir::Component`. `body.owner` says what renders it in the browser and is written `(owner <word>)` first among the component's sections.
* `head` is the head elements a page rendering the module carries, including those of every component it places. Absent from the file when empty.
* In the s-expression form each head row is `(head <tag> "<name>" "<value>" ...)` after the component's sections.

### HeadRow

One head element.

* `pub struct HeadRow { pub tag: String, pub attrs: Vec<(String, String)> }`: a tag and its attributes in the order written. `Debug`, `Clone`, `PartialEq`, `Eq`, `Serialize`, `Deserialize`.

### ClientEntry

A module the build read and could not lower. The browser renders it.

* `pub struct ClientEntry { pub module: String, pub at: String, pub message: String, pub hint: Option<String> }`. `Debug`, `Clone`, `PartialEq`, `Eq`, `Serialize`, `Deserialize`.
* `at` is the `file:line:column` of the construct that decided it. It is the file alone when the module did not parse.
* `message` is what the build said about it. `hint` is what it suggested, when it did.
* In the s-expression form each is `(client <module> "<at>" "<message>")`, with the hint as a fourth string when there is one.

### ModuleHead

Head rows a module carries whatever renders it, such as a route directory's `opengraph-image` and `twitter-image`.

* `pub struct ModuleHead { pub module: String, pub head: Vec<HeadRow> }`. `Debug`, `Clone`, `PartialEq`, `Eq`, `Serialize`, `Deserialize`.
* In the s-expression form each is `(module-head <module> (head <tag> "<name>" "<value>" ...) ...)`.

### ModuleSheets

The stylesheets a page rendering a layout, page, fallback or error module links.

* `pub struct ModuleSheets { pub module: String, pub sheets: Vec<String> }`. `Debug`, `Clone`, `PartialEq`, `Eq`, `Serialize`, `Deserialize`.
* Each sheet is a path under the application's build output. It is a file the module reaches and imports for its effect. It can also be the sheet compiled beside a Vue or Svelte component the module reaches, such as `Card.vue.css`.
* In the s-expression form each is `(module-sheets <module> "<path>" ...)`.

### ImageEntry

A generated card image for one page.

* `pub struct ImageEntry { pub pattern: String, pub module: String, pub source: Option<String>, pub width: u32, pub height: u32, pub content_type: String, pub page: String, pub card: String, pub hash: String, pub alt: Option<String>, pub images: Option<Body> }`. `Debug`, `Clone`, `PartialEq`, `Serialize`, `Deserialize`.
* `pattern`: the page's pattern followed by `/<stem>.png` or `/<stem>.jpg`. With `images` set it ends `/<stem>/{__image}` instead.
* `module`: the component that draws the image.
* `source`: the data source whose data are its props. That is the image's own loader, else the page's or the layout's beside the image.
* `width`, `height`, `content_type` and `alt`: the module's exports. For a listed image they are the defaults.
* `content_type`: `image/png` or `image/jpeg`.
* `page`: the page module whose head names the image.
* `card`: `og` or `twitter`.
* `hash`: the component's hash, carried in the image's URL.
* `images`: the module's `images` or `generateImageMetadata`, a body of the context returning `[{ id, alt, size, contentType }]`.
* `pub const IMAGE_PARAM: &str = "__image"`: the parameter a listed image's pattern names its id by.
* In the s-expression form each is `(image <pattern> <module> (source <id>) (size <w> <h>) (type <mime>) (page <module> <card> <hash>) (alt "<text>") (images <stmt> ...))`. `source`, `alt` and `images` are written only when set.

### ThemeEntry

A layout's or a page's `export const theme`.

* `pub struct ThemeEntry { pub module: String, pub name: String, pub force: bool }`. `Debug`, `Clone`, `PartialEq`, `Eq`, `Serialize`, `Deserialize`.
* Down a route's `content` chain the deepest row wins. A forced row keeps every level below it. Of two forced rows the outer one wins.
* `force` is written only when true.
* In the s-expression form each is `(theme-of <module> "<name>")`, with `force` after the name when set. The `theme.ts` body is `(theme <stmt> ...)`.

### DeriveEntry

A store key the browser's `derive` computes and the keys it reads.

* `pub struct DeriveEntry { pub key: String, pub sources: Vec<String>, pub body: Option<Expr>, pub at: Option<String>, pub why: Option<String> }`. `Debug`, `Clone`, `PartialEq`, `Serialize`, `Deserialize`.
* `body` is the lowered compute, which reads each source key as `Expr::Store`. A host computes the key from it after the seeds merge.
* `body` is `None` for a derive only the browser runs. `at` is then its `file:line:column` and `why` what kept it in the browser.
* A host does not render a value for a browser-only key and holds the components that read it.
* In the s-expression form each is `(derive "<key>" (sources "<key>" ...) <expr>)`. A browser-only one has `(browser "<at>" "<why>")` in place of the expression.

## 5. The S-Expression Form

The module `snapfire_fsr_plan::sexpr`. The syntax type `Sx` and the expression forms are in `snapfire_fsr_ir::sexpr`.

### manifest_to_sx

* `pub fn manifest_to_sx(manifest: &Manifest) -> Vec<Sx>`: the manifest as forms.
* The order is `(plan <version>)`, then `(framework <package> <version>)` forms, `reads`, routes, intercepts, `not-found`, `middleware`, `theme`, `theme-of` rows, source, action and handler rows, `const` forms, components, derives, `module-head` rows, `module-sheets` rows, images and clients.

### manifest_from_sx

* `pub fn manifest_from_sx(forms: &[Sx]) -> Result<Manifest, SexprError>`: the forms read back into a manifest, with no version check. `Manifest::from_sexpr` adds the version and body checks.
* A file with no `(plan <version>)` form is an error.

### manifest_from_forms

* `pub fn manifest_from_forms(forms: impl IntoIterator<Item = Result<Sx, SexprError>>) -> Result<Manifest, SexprError>`: `manifest_from_sx` over forms as `snapfire_fsr_ir::sexpr::forms` yields them, each turned into its row before the next is read.
* The first `Err` the forms yield is returned.

### SOURCE_SECTIONS

* `pub const SOURCE_SECTIONS: [&str; 5] = ["body", "meta", "store", "defaults", "paths"]`: the sections `Manifest::view` prints from a source row.

## 6. Error Handling

### PlanError

Returned by every `Manifest` reader and tree conversion.

* `pub enum PlanError`. `Debug`, `Clone`, `PartialEq`, `Eq`, `thiserror::Error`.
* `Malformed(String)`: the file did not parse, with the s-expression parser's message or a form that is not a plan form.
* `Version { found: u32 }`: a version outside 1 to `FORMAT_VERSION`.
* `NoBody { id: String }`: a `lowered` source or action row with no body.
* `Module { at: String, module: String }`: a module that is not `path#export`. `at` is the pattern followed by the slot path, with `/fallback`, `/error` or `/error.<kind>` for those fields.
* `DuplicateNode { at: String, id: u32 }`: a node id used twice within one route.
* `DuplicateSlot { at: String, slot: String }`: a slot used twice on one node.
* `EmptyPattern`: a route or intercept with an empty pattern.

### ViewError

Returned by `Manifest::view`. `Display` is the message `fsr ir view` prints.

* `pub enum ViewError`. `Debug`, `Clone`, `PartialEq`, `Eq`, `thiserror::Error`, `#[non_exhaustive]`.
* `Unknown { id: String }`: no row has that id.
* `OneBody { id: String }`: a section was named for a row that is not a source row.
* `NoSuchSection { section: String }`: the section is not one of `SOURCE_SECTIONS`.
* `Absent { id: String, section: String, present: String }`: the source row has no such section. `present` lists the sections it does carry. It reads `no sections` when there are none.

### SexprError

* `snapfire_fsr_ir::sexpr::SexprError`: returned by `manifest_from_sx`. `Display` is the message.
