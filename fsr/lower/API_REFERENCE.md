# API Reference: snapfire_fsr_lower

Turns the TypeScript an FSR application runs on the server into the IR trees `snapfire_fsr_ir` runs. It names the line where a file stops being translatable.

## Contents

* [1. Lowering Functions](#1-lowering-functions)
  * [lower_loader](#lower_loader)
  * [lower_actions](#lower_actions)
  * [lower_handlers](#lower_handlers)
  * [lower_middleware](#lower_middleware)
  * [lower_theme](#lower_theme)
  * [lower_meta_with](#lower_meta_with)
  * [lower_store_with](#lower_store_with)
  * [lower_defaults_with](#lower_defaults_with)
  * [lower_paths_with](#lower_paths_with)
  * [SessionDefaults](#sessiondefaults)
  * [LoweredAction](#loweredaction)
  * [LoweredHandler](#loweredhandler)
  * [mdx_component_map](#mdx_component_map)
  * [MappedValue](#mappedvalue)
  * [declares_unexported](#declares_unexported)
  * [resolve_specifier](#resolve_specifier)
  * [Origin](#origin)
  * [OriginSegment](#originsegment)
  * [Crate Constants](#crate-constants)
* [2. Schemas](#2-schemas)
  * [read_schema](#read_schema)
  * [read_session_defaults](#read_session_defaults)
  * [builtin_types](#builtin_types)
  * [SchemaType](#schematype)
* [3. Component Sets](#3-component-sets)
  * [ComponentSet](#componentset)
  * [HeadRow](#headrow)
  * [LoweredDerive](#loweredderive)
  * [ThemeExport](#themeexport)
  * [ImageExports](#imageexports)
  * [Extracted](#extracted)
  * [ForeignImport](#foreignimport)
  * [Transform](#transform)
  * [Transformed](#transformed)
  * [Override](#override)
  * [Component Module Items](#component-module-items)
* [4. Assets](#4-assets)
  * [AssetResolver](#assetresolver)
  * [ImageRequest](#imagerequest)
  * [ImageFacts](#imagefacts)
  * [NoAssets](#noassets)
  * [Kind](#kind)
  * [Asset Functions and Constants](#asset-functions-and-constants)
* [5. Vue Descriptions](#5-vue-descriptions)
  * [vue::Template](#vuetemplate)
  * [vue::Node](#vuenode)
  * [vue::Prop](#vueprop)
  * [vue::props_declaration](#vueprops_declaration)
  * [Vue Constants](#vue-constants)
* [6. Svelte Descriptions](#6-svelte-descriptions)
  * [svelte::Template](#sveltetemplate)
  * [svelte::Fragment](#sveltefragment)
  * [svelte::Node](#sveltenode)
  * [svelte::Attr, AttrValue and Part](#svelteattr-attrvalue-and-part)
  * [svelte::Code, Declarator and SlotGroup](#sveltecode-declarator-and-slotgroup)
  * [Svelte Constants](#svelte-constants)
* [7. Hoisting](#7-hoisting)
  * [hoist::Rewrite](#hoistrewrite)
  * [hoist::ChunkSite](#hoistchunksite)
  * [hoist::Hook](#hoisthook)
  * [hoist::StatementLoop](#hoiststatementloop)
  * [hoist::Chunk](#hoistchunk)
  * [hoist::Candidates](#hoistcandidates)
  * [Hoist Functions](#hoist-functions)
  * [Hoist Constants](#hoist-constants)
* [8. Meanings and Sides](#8-meanings-and-sides)
  * [meaning::Side](#meaningside)
  * [meaning::ImportBinding](#meaningimportbinding)
  * [Meaning Functions](#meaning-functions)
  * [Meaning Constants](#meaning-constants)
* [9. Terminal Pages](#9-terminal-pages)
  * [Terminal Constants](#terminal-constants)
  * [terminal::primitive_ref](#terminalprimitive_ref)
* [10. Route Extraction](#10-route-extraction)
  * [extract::Extraction](#extractextraction)
  * [extract::Kept](#extractkept)
* [11. The Recognised Language](#11-the-recognised-language)
  * [Modules](#modules)
  * [Names and Bindings](#names-and-bindings)
  * [FSR Components](#fsr-components)
  * [Statements](#statements)
  * [Component Bodies](#component-bodies)
  * [Markup Values](#markup-values)
  * [Expressions](#expressions)
  * [Builtins](#builtins)
  * [Dates, Encoding and Intl](#dates-encoding-and-intl)
  * [Collections and Kinds](#collections-and-kinds)
  * [Extensions](#extensions)
  * [Hoisted Calls and Subtrees](#hoisted-calls-and-subtrees)
  * [Function Props](#function-props)
  * [Handlers](#handlers)
  * [Terminal Pages](#terminal-pages)
  * [Lambdas](#lambdas)
  * [Vue Files](#vue-files)
  * [Svelte Files](#svelte-files)
  * [Transformed Files](#transformed-files)
  * [Schema Types](#schema-types)
* [12. Test Files](#12-test-files)
  * [lower_tests](#lower_tests)
  * [TestFile](#testfile)
  * [Block](#block)
  * [TestCase](#testcase)
  * [Mode](#mode)
  * [Each](#each)
  * [Step](#step)
  * [Assertion](#assertion)
  * [Subject](#subject)
  * [Matcher](#matcher)
  * [Pattern](#pattern)
  * [Mock](#mock)
  * [Answer](#answer)
  * [Binding](#binding)
  * [Target](#target)
* [13. Error Handling](#13-error-handling)
  * [LowerError](#lowererror)
  * [Residue](#residue)
  * [Placement](#placement)

## 1. Lowering Functions

`file` is the path relative to the app. It is used in diagnostics and to resolve imports. Each free function lowers through a `ComponentSet` with `source` provided under `file`.

### lower_loader

* `pub fn lower_loader(file: &str, source: &str) -> Result<Body, LowerError>`: the exported `load` of a loader module.
* `pub fn lower_loader_with(file: &str, source: &str, defaults: &SessionDefaults) -> Result<Body, LowerError>`: as `lower_loader`, with every read of a session key that has a default lowered to `Coalesce(Session(key), default)`.
* `load` is a function declaration or an arrow with a block body.

### lower_actions

* `pub fn lower_actions(file: &str, source: &str) -> Result<Vec<LoweredAction>, LowerError>`: every `export const <name> = action(<function>)` or `action<T>(<function>)`, in file order.
* `pub fn lower_actions_with(file: &str, source: &str, defaults: &SessionDefaults) -> Result<Vec<LoweredAction>, LowerError>`: as `lower_actions`, with session defaults.
* The function is an arrow with a block body, an arrow with an expression body or a function expression. An expression body lowers to a `Return` of the expression.
* A type or a plain value export is skipped. An `action(...)` of anything but a function is a `Residue` naming the export.
* The first `Residue` in any action fails the whole call.

### lower_handlers

* `pub fn lower_handlers(file: &str, source: &str) -> Result<Vec<LoweredHandler>, LowerError>`: every export of a `route.ts` named in `HANDLER_METHODS`, in file order.
* `pub fn lower_handlers_with(file: &str, source: &str, defaults: &SessionDefaults) -> Result<Vec<LoweredHandler>, LowerError>`: as `lower_handlers`, with session defaults.
* A method exported as a plain function reads the request body as `input` unchecked. One exported as `action<T>(...)` has `input` checked against `T`.
* A method export that is neither a function nor an `action(...)` is a `Residue`.

### lower_middleware

* `pub fn lower_middleware(file: &str, source: &str) -> Result<Body, LowerError>`: the exported `middleware` of `middleware.ts`.
* `pub fn lower_middleware_with(file: &str, source: &str, defaults: &SessionDefaults) -> Result<Body, LowerError>`: as `lower_middleware`, with session defaults.
* The input is the request line as `request`, with `method` and `path`.
* The body returns nothing to continue. To act it returns a map naming `redirect`, `rewrite`, `status`, `body` or `headers`.

### lower_theme

* `pub fn lower_theme(file: &str, source: &str) -> Result<Body, LowerError>`: the exported `theme` function.
* `ctx.theme`, the route's own theme as `{ name, forced }`, lowers to `Expr::Input`. The rest of the context binds as a loader's does.
* No `theme` export is `LowerError::MissingExport`. There is no `_with` form.

### lower_meta_with

* `pub fn lower_meta_with(file: &str, source: &str, defaults: &SessionDefaults) -> Result<Option<Body>, LowerError>`: the loader module's exported `meta`, returning the document's `title` and `description`.
* `meta` is an object literal or a function of `{ data }`, with `data` the loader's result as `Expr::Input`.
* The function is a block body or an arrow whose expression body becomes its `Return`.
* `None` when the module exports no `meta`. A `meta` that is not a function or an object literal is a `Residue`.

### lower_store_with

* `pub fn lower_store_with(file: &str, source: &str, defaults: &SessionDefaults) -> Result<Option<Body>, LowerError>`: the loader module's exported `store`, a function of `{ data }` returning the store keys the route seeds.
* Same shape as `meta`. `None` when the module exports no `store`.

### lower_defaults_with

* `pub fn lower_defaults_with(file: &str, source: &str, defaults: &SessionDefaults) -> Result<Option<Body>, LowerError>`: the loader module's exported `defaults`, a function of `{ data }` returning store keys the route seeds as defaults.
* A value an island wrote in the browser outranks a seeded default. `None` when the module exports no `defaults`.

### lower_paths_with

* `pub fn lower_paths_with(file: &str, source: &str, defaults: &SessionDefaults) -> Result<Option<Body>, LowerError>`: the page loader's exported `paths`, returning one parameter object per path the route prerenders.
* `paths` is a function of the context or of nothing. The context binds as a loader's does and `data` is not bound.
* `None` when the module exports no `paths`. A `paths` that is not a function is a `Residue`.

### SessionDefaults

* `pub type SessionDefaults = Vec<(String, Expr)>`: one literal expression per session key, from `export const defaults` in the session schema.

### LoweredAction

* `pub struct LoweredAction { pub export: String, pub input: Option<String>, pub body: Body }`. `Debug`, `Clone`, `PartialEq`, `#[non_exhaustive]`.
* `input` is the type named by the parameter's `ActionCtx<T>` annotation or by `action<T>(...)`, when `T` is a plain type reference. Otherwise `None`.

### LoweredHandler

* `pub struct LoweredHandler { pub method: String, pub input: Option<String>, pub body: Body }`. `Debug`, `Clone`, `PartialEq`, `#[non_exhaustive]`.
* `method` is one of `HANDLER_METHODS`. `input` is `T` when the export is an `action<T>`.

### mdx_component_map

* `pub fn mdx_component_map(file: &str, source: &str) -> Result<Vec<(String, MappedValue)>, LowerError>`: the keys of the object `useMDXComponents` returns in `mdx-components.tsx`, in order.
* A spread of the components it is given is left out.
* A missing `useMDXComponents` function is a `Residue`. So is one returning anything but an object literal.

### MappedValue

* `pub enum MappedValue { Name(String), Expr(Range<usize>) }`. `Debug`, `Clone`, `PartialEq`, `Eq`.
* `Name` is a component by its name in the file. `Expr` is any other expression by its byte range in `source`.

### declares_unexported

* `pub fn declares_unexported(file: &str, source: &str, name: &str) -> bool`: whether `source` declares `name` at module level without exporting it. False when `source` does not parse.

### resolve_specifier

* `pub fn resolve_specifier(from: &str, specifier: &str) -> Option<String>`: `specifier` as a path relative to the app, with an alias in `GENERATED` or `ALIASES` expanded or a relative specifier joined to `from`'s directory.
* `None` for a bare specifier.

### Origin

Where a module written from a file in another language came from.

* `pub struct Origin { pub source: String, pub segments: Vec<OriginSegment> }`. `Debug`, `Clone`, `#[non_exhaustive]`.
* `Origin::new(source: String, segments: Vec<OriginSegment>) -> Origin`
* `position(&self, offset: usize) -> (usize, usize)`: the one-based line and column in `source` of byte `offset` of the module.
* Every residue, parse error and placement position in such a module is reported through it.

### OriginSegment

* `pub struct OriginSegment { pub generated: usize, pub len: usize, pub original: usize, pub verbatim: bool }`. `Debug`, `Clone`, `Copy`, `#[non_exhaustive]`.
* `OriginSegment::new(generated: usize, len: usize, original: usize, verbatim: bool) -> OriginSegment`
* A run of `len` bytes from `generated`. A verbatim run maps byte for byte from `original`. Any other run maps whole to `original`.

### Crate Constants

* `pub const ALIASES: &[(&str, &str)]`: `@app/`, `@routes/`, `@src/`, `@schemas/`, `@generated/` and `@ext/`, each with the app directory it stands for (`""`, `routes/`, `src/`, `schemas/`, `generated/`, `ext/`).
* `pub const GENERATED: &[(&str, &str)]`: `("@snapfire/fsr/head", "generated/head")`, the exact specifiers mapped to a generated module.
* `pub const HEAD_MODULE: &str = "generated/head.ts"`: a module under `generated/` a body may call into.
* `pub const CONTENT_MODULE: &str = "generated/content.ts"`: the module `fsr` writes from `[mdx.collections]`, the other module under `generated/` a body may read.
* `pub const STORE_SPECIFIER: &str = "@snapfire/fsr/client/store"`: where `key` comes from.
* `pub const EXT_DIR: &str = "ext"`: the directory under the app whose modules are extensions.
* `pub const HANDLER_METHODS: [&str; 5]`: `GET`, `POST`, `PUT`, `PATCH`, `DELETE`.
* `pub const TERMINAL_COLUMNS: &str = "$terminal.columns"` and `pub const TERMINAL_ROWS: &str = "$terminal.rows"`: the store keys a terminal sets to its size.
* `pub const TERMINAL_FOCUS: &str = "$terminal.focus"`: the store key a terminal sets to the focused id. Null when nothing has focus.
* `pub const TERMINAL_METRICS: &str = "$terminal.metrics"`: the store key a terminal sets to each measured box, by ref id.
* `pub const REF_ID: &str = "$id"`: the field a terminal `useRef` holds its id in beside `current`.
* `COMPONENTS_MODULE` and `TEMPLATE_MODULE` are re-exported from `meaning`.

## 2. Schemas

### read_schema

* `pub fn read_schema(file: &str, source: &str) -> Result<Vec<SchemaType>, LowerError>`: every `export interface` and `export type X = "a" | "b"`, in file order.
* Declarations that are not exported and every other statement are skipped.

### read_session_defaults

* `pub fn read_session_defaults(file: &str, source: &str) -> Result<SessionDefaults, LowerError>`: `export const defaults = { key: literal, ... }`, one entry per key.
* Each value lowers as an expression with no context. Anything beyond literals and object and array literals is a `Residue`.
* Empty when the module declares no `defaults`.

### builtin_types

* `pub fn builtin_types() -> Vec<SchemaType>`: what every contract holds before a schema is read.
* One record, `UPLOAD` (`pub const UPLOAD: &str = "Upload"`), with `filename` and `content_type` as `Str`, `size` as `I64` and `bytes` as `Bytes`.
* The host builds one `Upload` per file part of a `multipart/form-data` body. An application declaring its own `Upload` is refused.

### SchemaType

* `pub struct SchemaType { pub name: String, pub def: TypeDef }`, with `TypeDef` from `snapfire_fsr_service`.

## 3. Component Sets

### ComponentSet

The parsed files, lowered components and import resolution of one application. `component::ComponentSet`, `#[non_exhaustive]`.

Construction:

* `ComponentSet::new(app: &Path) -> ComponentSet`
* `with_defaults(self, defaults: SessionDefaults) -> ComponentSet`: the session defaults every body lowers with.
* `with_assets(self, resolver: Rc<dyn AssetResolver>, rewrite_images: bool) -> ComponentSet`: where an imported image, font or media file is looked up and whether a plain `<img>` of one is rewritten. Default `NoAssets` and `true`.
* `with_transform(self, transform: Rc<Transform>) -> ComponentSet`: what writes the module for an `.mdx` or `.md` file.
* `with_elements(self, elements: HashMap<String, String>) -> ComponentSet`: the custom elements whose shadow template the build lowers, by tag.
* `for_terminal(self) -> ComponentSet`: lowers for `fsr dev --terminal`. A render may read the clock and the `Terminal`, `Later` and `Process` reaches are allowed.
* `provide(self, file: impl Into<String>, source: impl Into<String>) -> ComponentSet`: a module the set reads from `source` rather than from disk.

Plugin descriptions:

* `describe(&mut self, file: impl Into<String>, described: Described)`: what the framework's plugin said `file` is. A placement of it then lowers through that framework's front end.
* `described(&self) -> impl Iterator<Item = (&str, &Described)>`: every file the plugin described.
* `undescribed(&mut self, file: impl Into<String>, why: impl Into<String>)`: the plugin refused to describe `file`. A placement of it is foreign with `why` as its residue.
* `script_only(&mut self, file: impl Into<String>, script: snapfire_compiler_wire::Script)`: the script of a file the plugin would not describe, read only for its imports.
* `is_described(&self, module: &str) -> bool`

Components:

* `lower(&mut self, module: &str) -> Result<(), LowerError>`: lowers `path#export` and everything it renders into `components`. A module already lowered is not read again.
* A module met while it is still being lowered renders itself. It lowers to a `Tmpl::Component` naming it.
* When the outermost `lower` returns, a component on a cycle is pure when every component on the cycle is.
* A TSX component's `owner` is `Owner::React` when it has state, handlers, an element whose handler did not lower or a context provider. Otherwise it is `Owner::Fsr`.
* A component it renders that does not lower is recorded in `browser_only` and does not fail it.
* `lower_route(&mut self, module: &str) -> Result<Option<Extracted>, LowerError>`: `lower` for a route module. When the residue sits in the module's own body, the statement or element holding it moves into an island beside the page and the page lowers again. `Ok(Some(_))` names the split. Any other error is returned as `lower` gives it.
* `extract_route(&mut self, module: &str) -> Result<Option<Extracted>, String>`: splits a page or a layout whose default export holds state, handlers or effects into composition and an island beside it, before lowering. `Ok(None)` when nothing needs moving. `Err` names why the module cannot be split.
* The island is the smallest JSX subtree holding every use of the state, handlers and effects outside a callback. It is the whole returned tree when an early return chooses between trees.
* The hooks, handlers and functions the island needs move with it. Every value it reads from the page becomes a prop.
* After a split, the set reads both files from what the split wrote.
* A module cannot be split when its state reaches one of the layout's slot props. It stays a root.
* `settle_fills(&mut self)`: run once every component is lowered. Turns each `render::SLOT_READ_PREFIX` read of a prop that no placement fills back into a plain `Tmpl::Expr`.
* `settle_proofs(&mut self)`: run once every component is lowered. Clears `proven` on the chunks of a component a `foreign` placement mounts. Clears it on every chunk when any component holds a `<Mount>`.

Bodies:

* `lower_loader(&mut self, file: &str) -> Result<Body, LowerError>`
* `lower_meta(&mut self, file: &str) -> Result<Option<Body>, LowerError>`
* `lower_store(&mut self, file: &str) -> Result<Option<Body>, LowerError>`
* `lower_defaults(&mut self, file: &str) -> Result<Option<Body>, LowerError>`
* `lower_paths(&mut self, file: &str) -> Result<Option<Body>, LowerError>`
* `lower_actions(&mut self, file: &str) -> Result<Vec<LoweredAction>, LowerError>`
* `lower_handlers(&mut self, file: &str) -> Result<Vec<LoweredHandler>, LowerError>`
* `lower_middleware(&mut self, file: &str) -> Result<Body, LowerError>`
* `lower_theme(&mut self, file: &str) -> Result<Body, LowerError>`
* Each body lowerer follows the module-level names the body calls through the same resolution a component uses. A name that cannot be followed is the residue the free functions give.
* `lower_images(&mut self, file: &str) -> Result<Option<Body>, LowerError>`: a card image module's `images` export, else its `generateImageMetadata`, a function of the context. `None` when it has neither.
* `image_exports(&mut self, file: &str) -> Result<ImageExports, LowerError>`: a card image module's literal `size`, `alt` and `contentType` exports. A non-literal one is a `Residue`.
* `theme_export(&mut self, file: &str) -> Result<Option<ThemeExport>, LowerError>`: a layout's or a page's `export const theme`. `None` when the module exports no `theme`. Anything but a name or `{ name, force }` is a `Residue`.
* `lower_extensions(&mut self, file: &str) -> Result<Vec<(String, String)>, LowerError>`: every export of a module under `ext/`, as `(file#export, kind)` with kind `lowered`, `native render` or `native body`. An export that does not lower is `LowerError::Extension`.
* `lower_derives(&mut self, file: &str) -> Result<Vec<LoweredDerive>, LowerError>`: every call in `file` of the store's `derive`, under any spelling.
* A `derive` key and each source are a literal or a `key()` followed through an import. One that is neither is `LowerError::Residue`.
* A `derive` that is a statement at module level lowers its compute, an arrow or a function with at most one parameter. A call of that parameter is `Expr::Store` of its argument. The compute lowers under render-path rules.
* A `derive` inside a function has no body and carries `at` and `why`. So does one whose compute does not lower.
* `store_reads(&mut self, file: &str) -> Vec<String>`: the keys a module that does not lower passes to `useStore`, as a literal or a `key()` followed through an import. Empty when one of them is neither.

Files:

* `writes_jsx(&mut self, file: &str) -> bool`: whether `file` writes a JSX element or fragment anywhere. False when it does not parse.
* `imports_value_from(&self, module: &str, source: &str) -> bool`: whether `module`'s file imports a value from `source`.
* `local_reach(&mut self, file: &str) -> Vec<String>`: `file` and every local file it reaches through value imports and re-exports, in the order first reached. A file that does not parse is listed and not followed.
* `local_imports(&mut self, file: &str) -> Vec<String>`: the local script files and described foreign components `file` imports a value from or re-exports, in source order. Imported assets are left out. Empty when it does not parse.
* `host_import(&mut self, file: &str) -> Option<u32>`: the line of the first import or re-export in `file` from `@snapfire/fsr/host` or a module under it.
* `sheets(&mut self, file: &str) -> Vec<String>`: the stylesheets `file` brings, in source order: each local one imported for its effect and, for each Vue or Svelte component it imports, the sheet beside it (`Card.vue.css`).
* `foreign_imports(&mut self, file: &str) -> Vec<ForeignImport>`: the value imports in `file` of components in a plugin's language, namespace imports left out. Empty when `file` does not parse.
* `draws_in_terminal(&mut self, file: &str) -> bool`: whether `file` or a local file it reaches imports a value from Ink's API.
* `rewritten(&self) -> Vec<(String, String)>`: every file with a `Rewrite`, with `hoist::apply` over its source. Also every file a hoisted function or an unexported function was referenced from, with `export const __sfF<offset> = <function>;` and `export { name };` appended.
* `function_modules(&self) -> Vec<String>`: every file the browser imports a function prop from.

Fields:

* `heads: HashMap<String, Vec<HeadRow>>`: the head rows each lowered module asks for, its own and those of every component it places, by module id.
* `consts: Consts`: the module-level constants the plan names.
* `components: Vec<(String, Component)>`: every lowered component, by module id.
* `layouts: Vec<String>`: modules that are layouts. A layout's `children` is placed inside `<sf-s>`.
* `slots: Vec<(String, Vec<String>)>`: per layout module, the slots its `slots/` directory declares.
* `rewrites: Vec<hoist::Rewrite>`: one per component with a surviving hoist.
* `pure: HashMap<String, bool>`: per lowered module, whether it has no state, handler, island, slot or ambient read and every component it renders is pure.
* `natives: Vec<(String, Reach)>`: every native pair declared so far, as `module.member` and reach.
* `remaining: Vec<(String, String)>`: per lowered module, `file:line:column` of each render-path call candidate that was not hoisted and sits under no hoisted call.
* `foreign: Vec<String>`: components in a language the build does not read, as `file#export`. Each is placed as an island the server writes empty.
* `foreign_residue: Vec<(String, Residue)>`: each described component that did not lower, with why. It stays foreign and the page lowers on.
* `browser_only: Vec<(String, Residue)>`: components that did not lower, each with its residue, placed by a component that did. Each is placed as a React island.
* `browser_texts: Vec<(String, String)>`: per lowered module, each value only the browser knows, as `file:line:column: reason`.
* `mounted: Vec<String>`: components in a plugin's language that an `Owner::React` component renders inline, as `file#export`.

### HeadRow

* `pub struct HeadRow { pub tag: String, pub attrs: Vec<(String, String)> }`: one element a module asks for in the document head.
* A priority `Picture` of an imported image asks for one `link rel="preload" as="image"` with its first format's `srcset` and its `sizes`.

### LoweredDerive

* `pub struct LoweredDerive { pub key: String, pub sources: Vec<String>, pub body: Option<Expr>, pub at: Option<String>, pub why: Option<String> }`
* `at` is the call's `file:line:column`. `at` and `why` are set when `body` is `None`.

### ThemeExport

* `pub struct ThemeExport { pub name: String, pub force: bool }`. `#[non_exhaustive]`.
* `ThemeExport::new(name: String, force: bool) -> ThemeExport`
* A string or a template with no expression is a name. `{ name, force }` with a literal boolean sets `force`.
* `as`, `satisfies` and parentheses are looked through.

### ImageExports

* `pub struct ImageExports { pub width: Option<u32>, pub height: Option<u32>, pub alt: Option<String>, pub content_type: Option<String> }`
* `size` is `{ width, height }` with whole positive numbers. `alt` and `contentType` are strings or templates with no expression.
* `as`, `satisfies` and parentheses are looked through.

### Extracted

A route module split by `extract_route` or `lower_route`. `#[non_exhaustive]`.

* `pub struct Extracted { pub island: String, pub file: String, pub source: String, pub page: (String, String), pub holds: Vec<String>, pub started: Option<Residue> }`
* `island` is the island's module id, `<page stem>.island0.tsx#default`. `file` and `source` are the island's.
* `page` is the page's file and its source as composition. `holds` is the names the island took from the page's body.
* `started` is the residue in the page as written that made the split.
* Neither source is written to the app. The build puts both in the bundle overlay.

### ForeignImport

* `pub struct ForeignImport { pub text: String, pub bindings: Vec<(String, String)> }`: the import declaration as written and each binding as the local name and the module id.

### Transform

* `pub type Transform = dyn Fn(&str, &str, &[Override]) -> Result<Transformed, String>`: takes the file's path, its text and the elements to place as other components.
* Without a transform, loading a transformed file is `LowerError::Parse` naming the `mdx` feature.
* An `Err` from the transform is `LowerError::Parse` with that message.

### Transformed

* `pub struct Transformed { pub source: String, pub origin: Option<Origin> }`. `#[non_exhaustive]`.
* `Transformed::new(source: String, origin: Option<Origin>) -> Transformed`

### Override

* `pub struct Override { pub key: String, pub specifier: String, pub export: String }`: one element and the component placed for it, `export` of `specifier` relative to the file.

### Component Module Items

* `pub const TRANSFORMED: &[&str]`: `mdx` and `md`.
* `pub fn is_transformed(file: &str) -> bool`: for a path, a `file#export` id or a variant id `file~<hash>`. A transformed file is never foreign.
* `pub fn is_foreign(module: &str) -> bool`: whether a file or a `file#export` module is in a language the build does not read.
* `pub fn files_under(dir: &Path) -> Vec<String>`: every file under `dir`, relative and `/`-separated. A directory named `node_modules` or starting with `.` is left out.
* `pub const TEMPLATE_SOURCES: &[&str]`: `@snapfire/fsr/template`, `@snapfire/fsr/client/react`, `@snapfire/fsr/client/vue` and `@snapfire/fsr/client/jsx-runtime`, the modules `Island`, `island`, `Slot` and `Link` are read from.
* `pub const CLIENT_SOURCE: &str = "@snapfire/fsr/client"`: where `action` comes from.
* `pub const RAW_ESCAPE: &str = "data-sf-raw"`: the attribute that keeps an `<img>` out of the image rewrite. It is stripped from the markup.

## 4. Assets

### AssetResolver

Where an imported image, font or media file is looked up. `assets::AssetResolver`. Every `path` is relative to the app and normalised.

* `fn image(&self, path: &str, request: &ImageRequest) -> Option<ImageFacts>`: the image's served URL, size and `<source>` rows. The resolver records `request` for the build. `None` when the file is not under the app.
* `fn font(&self, path: &str) -> Option<String>`: a font file's served URL.
* `fn media(&self, path: &str) -> Option<String>`: a video, audio or caption file's served URL. Default `None`.
* `fn embed_still(&self, provider: &str, id: &str) -> Option<ImageFacts>`: the still an `<Embed>` shows before it plays. Default `youtube_still(provider, id)`.
* `fn computed_still(&self, provider: &str) -> bool`: whether an `<Embed>` with a computed id shows the provider's still. Default `provider == "youtube"`.
* `fn source(&self, name: &str) -> Option<String>`: a named remote source's URL template, with `{src}` and `{width}`.
* `fn widths(&self) -> Vec<u32>`: the policy's widths, for a remote image whose own width is unknown.

### ImageRequest

* `pub struct ImageRequest { pub widths: Option<Vec<u32>>, pub quality: Option<BTreeMap<String, u8>> }`: a per-image override. `Debug`, `Clone`, `Default`, `PartialEq`, `Eq`, `#[non_exhaustive]`.

### ImageFacts

* `pub struct ImageFacts { pub src: String, pub width: u32, pub height: u32, pub sources: Vec<(String, String)> }`. `#[non_exhaustive]`.
* `ImageFacts::new(src: String, width: u32, height: u32, sources: Vec<(String, String)>) -> ImageFacts`
* `src` is the hashed original's URL. `sources` is `(mime, srcset)` per format, empty for an image served as it is.

### NoAssets

* `pub struct NoAssets`: a resolver that knows nothing. An asset import stays unbound.

### Kind

* `pub enum Kind { Image, Font, Media }`
* `Image`: `png`, `jpg`, `jpeg`, `gif`, `webp`, `avif`, `svg`, `ico`, `bmp`.
* `Font`: `woff2`, `woff`, `ttf`, `otf`.
* `Media`: `mp4`, `m4v`, `webm`, `ogv`, `mov`, `mp3`, `m4a`, `aac`, `oga`, `ogg`, `opus`, `wav`, `vtt`.

### Asset Functions and Constants

* `pub fn kind(specifier: &str) -> Option<Kind>`: what a relative specifier names by its extension.
* `pub fn default_sizes(width: u32) -> String`: `(max-width: {width}px) 100vw, {width}px`.
* `pub const EMBED_PLAYERS: [(&str, &str, &str); 2]`: each provider's player as the provider, the URL before the id and the URL after it. `youtube`: `https://www.youtube-nocookie.com/embed/<id>?autoplay=1`. `vimeo`: `https://player.vimeo.com/video/<id>?autoplay=1`.
* `pub fn embed_provider(url: &str) -> Option<&'static str>`: the provider whose player `url` starts with.
* `pub const YOUTUBE_STILL: (&str, &str, u32, u32)`: `https://i.ytimg.com/vi/`, `/hqdefault.jpg`, `480`, `360`.
* `pub fn youtube_still(provider: &str, id: &str) -> Option<ImageFacts>`: YouTube's still for `id`. `None` for any other provider.

## 5. Vue Descriptions

The template tree `snapfirec-vue`'s `describeNode` writes, read from `Described::template` with serde.

### vue::Template

* `pub struct Template { pub children: Vec<Node> }`

### vue::Node

* `pub struct Node { pub node: String, pub tag: String, pub kind: u8, pub props: Vec<Prop>, pub children: Vec<Node>, pub content: String, pub other: u32, pub line: usize, pub column: usize }`
* `node` is `element` (with `tag`, `kind`, `props` and `children`), `text` or `interpolation` (with `content`) or `other`.
* `kind` is Vue's `ElementTypes`: element 0, component 1, slot 2, template 3.
* `other` is Vue's `NodeTypes` number of a node the plugin passed through unread.

### vue::Prop

* `pub struct Prop { pub prop: String, pub name: String, pub value: Option<String>, pub arg: Option<String>, pub arg_static: bool, pub exp: Option<String>, pub modifiers: Vec<String>, pub line: usize, pub column: usize, pub exp_line: usize, pub exp_column: usize }`
* `prop` is `attribute` (with `name` and `value`) or `directive` (with `name`, `arg`, `argStatic`, `exp` and `modifiers`).
* `exp_line` and `exp_column` locate a directive's expression. Serialized as `expLine` and `expColumn`.

### vue::props_declaration

* `pub fn props_declaration(file: &str, described: &Described) -> Option<String>`: a declaration module typing the component as `(props: T & { children?: unknown }) => any`.
* `T` is the `defineProps<T>()` of a `<script setup lang="ts">`, each `withDefaults` key made optional, after the script's interfaces, type aliases and type-only imports.
* `None` for a JavaScript script or props declared any other way.
* `pub fn props_declaration_path(file: &str) -> String`: `generated/vue/<file without .vue>.d.vue.ts`.

### Vue Constants

* `pub const VUE_CLIENT: &str = "@snapfire/fsr/client/vue"`: where `useStore` is imported from.
* `pub const PROPS_DECLARATION_DIR: &str = "generated/vue"`

## 6. Svelte Descriptions

The template tree `snapfirec-svelte` writes, read from `Described::template` with serde. Every node carries `at`, its line and column in the file.

### svelte::Template

* `pub struct Template { pub runes: bool, pub scoping: Option<String>, pub instance: Option<(u32, u32)>, pub module: Option<(u32, u32)>, pub fragment: Fragment }`
* `scoping` is `read` when the plugin read which elements Svelte's CSS scopes and `unread` when it could not.

### svelte::Fragment

* `pub struct Fragment { pub hoisted: Vec<Node>, pub nodes: Vec<Node>, pub standalone: bool, pub text_first: bool }`

### svelte::Node

* `pub enum Node`, tagged by `type`: `Text`, `Comment`, `Expr`, `Html`, `Const`, `Render`, `Element`, `Component`, `Slot`, `If`, `Each`, `Snippet`, `Key`, `Declare`, `Await`, `Special`, `Other`.
* `Element { name, svg, mathml, scoped, attrs, fragment, at }`: `scoped` is the `svelte-` class Svelte's CSS gives the element.
* `Component { name, dynamic, svg, attrs, fragment, slots, at }`: `slots` groups the children by their static `slot` attribute, as `build_inline_component` does.
* `Declare { items, at }`: `{let x = ...}` or `{const x = ...}` in the template.
* `Special { kind, name, scoped, attrs, this, fragment, slots, at }`: a `svelte:` element or `<title>`. `this` is the expression `svelte:element` and `svelte:component` take.
* `Other { kind, at }`: a node the plugin passed through unread.

### svelte::Attr, AttrValue and Part

* `pub enum Attr { Attr { name, value: AttrValue, at }, Spread { code, at }, Attach { code, at }, Directive { kind, name, modifiers, expression: Option<Code>, value: Option<AttrValue>, at } }`
* `pub enum AttrValue { Present(bool), Parts(Vec<Part>) }`
* `pub enum Part { Text { text: String }, Code(Code) }`

### svelte::Code, Declarator and SlotGroup

* `pub struct Code { pub code: String, pub at: (usize, usize) }`
* `pub struct Declarator { pub id: Code, pub init: Option<Code> }`
* `pub struct SlotGroup { pub name: String, pub fragment: Fragment }`

### Svelte Constants

* `pub const SVELTE_CLIENT: &str = "@snapfire/fsr/client/svelte"`: where `useStore`, `useLocale`, `useLinkStatus`, `link`, `picture`, `embed`, `island` and `mount` are imported from.

## 7. Hoisting

### hoist::Rewrite

The rewrite of one component's source for the bundle. `#[non_exhaustive]`.

* `pub struct Rewrite { pub file: String, pub module: String, pub hook: Hook, pub sites: Vec<(u32, Range<usize>)>, pub chunks: Vec<ChunkSite>, pub islands: Vec<(u32, usize)>, pub placements: Vec<(u32, Range<usize>, bool)>, pub ids: Vec<(u32, Range<usize>)>, pub loops: Vec<Range<usize>>, pub statement_loops: Vec<StatementLoop>, pub browser: Vec<Range<usize>> }`
* `sites`: the surviving value candidates, as id and call range.
* `chunks`: one `ChunkSite` per kept subtree.
* `islands`: the island placements, as id and the offset the reader call is spliced at.
* `placements`: the keyed placements, as id, element range and whether the element sits among JSX children.
* `ids`: the `useId` calls, as id and call range.
* `loops`: the byte ranges of the JSX `.map` callbacks the survivors sit in.
* `statement_loops`: the `for...of` or counted `for` statements the survivors sit in.
* `browser`: the expressions only the browser knows. Each is null through hydration and computed once the component has mounted.
* A placement is keyed when the component it places keys a hoist, a chunk or a region below it and it sits inside no kept chunk. `Tmpl::Component.keyed` is set on it. A component met in its own cycle counts as keyed.

### hoist::ChunkSite

* `pub struct ChunkSite { pub id: u32, pub range: Range<usize>, pub open: Range<usize>, pub as_child: bool, pub proven: bool }`. `#[non_exhaustive]`.
* `range` is the element, `open` its opening tag. `as_child` is whether it sits among JSX children.

### hoist::Hook

* `pub enum Hook { Block { after: usize }, Expression(Range<usize>) }`. `#[non_exhaustive]`.
* `Block` is after a block body's `{`. `Expression` is around an arrow's expression body, which becomes a block returning it.

### hoist::StatementLoop

* `pub struct StatementLoop { pub range: Range<usize>, pub body: Range<usize>, pub pushes: Vec<Range<usize>> }`
* `StatementLoop::new(range: Range<usize>, body: Range<usize>, pushes: Vec<Range<usize>>) -> StatementLoop`
* `pushes` is the ranges of the arguments its body's `push` calls give the arrays it writes.

### hoist::Chunk

* `pub struct Chunk { pub id: u32, pub proven: bool }`. `#[non_exhaustive]`.
* `proven` is false for a chunk among the children a component or an island is given.

### hoist::Candidates

The hoist candidates of one component as the lowerer meets them. `Default`, `#[non_exhaustive]`.

* Fields: `sites`, `chunk_sites`, `island_sites`, `id_sites`, `open_loops`, `statement_loops`.
* `remaining(&self, kept: &[u32], chunks: &[Chunk]) -> Vec<Range<usize>>`
* `rewrite(self, values: &[u32], chunks: &[Chunk], islands: &[u32], keyed: &[u32], file: &str, module: &str, hook: Hook) -> Option<Rewrite>`

### Hoist Functions

* `pub fn decide(component: &mut Component, state: &[String]) -> Vec<u32>`: unwraps every value candidate that is not props only. Returns the ids kept.
* `pub fn chunks(component: &mut Component, state: &[String], pure: &HashMap<String, bool>) -> Vec<Chunk>`: keeps the outermost subtree candidates that are static and do work. Removes every other marker.
* `pub fn rechunk(component: &mut Component, state: &[String], pure: &HashMap<String, bool>) -> Vec<Chunk>`: `chunks` again, with the `UNTAKEN_ATTR` marks of the first pass cleared.
* `pub fn cover(component: &mut Component, chunks: &[Chunk]) -> Vec<u32>`: unwraps every value hoist among a proven chunk's children. Returns their ids.
* `pub fn static_tree(t: &Tmpl, pure: &HashMap<String, bool>) -> bool`: the static test with no state.
* `pub fn placed_with_state(component: &Component, state: &[String]) -> Vec<String>`: the modules `component` places with a prop that reads `state` or anything the request or the browser can change.
* `pub fn apply(source: &str, rewrites: &[&Rewrite]) -> String`: `source` with every rewrite applied, spliced from the end. All rewrites are for that one file.
* `apply` prepends `IMPORT` and binds `const __sfh = __sfUseHoisted("<module>")` at the hook.
* `apply` replaces each call with `__sfh.r(<id>, () => (<call>))`.
* `apply` wraps each loop callback as `__sfh.l(<callback>)`.
* `apply` wraps each statement loop as `{ let __sfI<at> = 0; <head> { __sfh.e(__sfI<at>++); try { <body> } finally { __sfh.x(); } } }`, with each `push` argument as `__sfh.w(<argument>)`.
* `apply` wraps each keyed placement as `__sfh.p(<id>, <element>)` and replaces each `useId()` call with `__sfh.id(<id>)`.
* `apply` writes a proven chunk as `__sfh.c(<id>, (__sfHtml) => <tag attrs dangerouslySetInnerHTML={__sfHtml} />)`. The element and every edit inside it are gone.
* `apply` writes an unproven chunk as `__sfh.c(<id>, (__sfHtml) => <tag attrs dangerouslySetInnerHTML={__sfHtml} />, () => (<element>))`.
* A chunk among JSX children is braced.

### Hoist Constants

* `pub const BOUND_ATTR: &str = "$bound"`: marks an element with a handler, a `ref` or a spread.
* `pub const CHUNK_ATTR: &str = "$chunk"`: marks a subtree candidate with its id.
* `pub const UNTAKEN_ATTR: &str = "$untaken"`: marks a kept chunk under an `If` whose condition reads state.
* `pub const PROPS: &str = "$props"`: the name a component reads its props through.
* `pub const IMPORT: &str`: `import { useHoisted as __sfUseHoisted, useBrowser as __sfUseBrowser } from "@snapfire/fsr/client/react";` and a newline.

## 8. Meanings and Sides

### meaning::Side

* `pub enum Side { Client, Host, Terminal }`
* `root(self) -> &'static str`: `CLIENT_ROOT`, `HOST_ROOT` or `TERMINAL_ROOT`.
* `holds(self, module: &str, reach: Reach) -> bool`: whether a standard member of `module` with `reach` is under this side.
* `Client` holds the `render` members. `Host` holds the `render` and `body` members. `Terminal` holds `system` and `clipboard`, with `system.spawn`.

### meaning::ImportBinding

* `pub struct ImportBinding { pub local: String, pub source: String, pub imported: Option<String> }`: a name bound to a module's export through an awaited `import()`.
* `imported` is `default` for a default import and `None` when the local is the whole module.

### Meaning Functions

* `pub fn side_modules(side: Side) -> Vec<(&'static str, Vec<&'static str>)>`: each standard module under `side` with its members, from `snapfire_fsr_ir::STANDARD` by reach.
* `pub fn reference() -> String`: the Markdown table of every platform form the build reads and every standard member with its paths and where it runs.
* `pub fn awaited_imports(stmt: &oxc_ast::ast::Statement<'_>) -> Vec<ImportBinding>`: the bindings `stmt` makes through awaited dynamic imports, including `await Promise.all([import(...), ...])`. Empty for any other statement.
* `pub fn terminal_primitives() -> impl Iterator<Item = (&'static str, &'static [Signature])>`: each terminal primitive's name with its signatures.

### Meaning Constants

* `pub const CLIENT_ROOT: &str = "@snapfire/fsr/client"`
* `pub const HOST_ROOT: &str = "@snapfire/fsr/host"`
* `pub const TERMINAL_ROOT: &str = "@snapfire/fsr/terminal"`
* `pub const PAGE_LOCALE_MODULE: &str = "@snapfire/fsr/client/intl"`
* `pub const COMPONENTS_MODULE: &str = "@snapfire/fsr/components"`: exports `Link`, `Picture`, `Mount`, `Embed` and `Place` in every framework.
* `pub const TEMPLATE_MODULE: &str = "@snapfire/fsr/template"`: `createContext`, `use` and the template dialect's types.
* `pub const CONFIG_MODULE: &str = "@snapfire/fsr/config"`: the deployment's `[public]` values as `config` and named exports.

## 9. Terminal Pages

### Terminal Constants

* `pub const INK_FILE: &str = "$fsr/terminal.tsx"`: the name `INK_SOURCE` is provided under.
* `pub const INK_SOURCE: &str`: Ink's hooks written over the terminal primitives, `lower/terminal/ink.tsx`.
* `pub const INK_MODULE: &str = "@snapfire/fsr/terminal/ink"`
* `pub const COMPANIONS: [(&str, &str, &str); 4]`: `(specifier, file, source)` for `@snapfire/fsr/terminal/spinners`, `/spinner`, `/text-input` and `/big-text`, provided under `$fsr/terminal/`.
* `pub const REPLACED: [(&str, &str); 5]`: `ink`, `cli-spinners`, `ink-spinner`, `ink-text-input` and `ink-big-text`, each with FSR's module to import instead.
* `pub const FOCUSABLE: &str = "focusable"`: the emitted key for `{ id, autoFocus, isActive }`, one per `useFocus`, in render order.
* `pub const CURSOR: &str = "cursor"`: the emitted key for `{ x, y }`, null to hide the cursor. The last one a step emits holds.
* `pub const EFFECT: &str = "effect"`: the emitted key for `{ event, deps }` of an effect whose dependencies change.
* `pub const PRIMITIVE: &str = "$primitive"`: the key a terminal primitive used as a value is held under.

### terminal::primitive_ref

* `pub fn primitive_ref(expr: &Expr) -> Option<String>`: the primitive a `{ "$primitive": name }` value refers to.

## 10. Route Extraction

### extract::Extraction

* `pub struct Extraction { pub page: String, pub island_file: String, pub island: String, pub holds: Vec<String> }`: a page with its browser half moved out. `#[non_exhaustive]`.
* `page` is the page's source as composition. `island_file` sits beside the page's file.

### extract::Kept

* `pub struct Kept(pub String)`: why a page cannot be split.

## 11. The Recognised Language

What lowers. Anything not listed is a `Residue`.

### Modules

* The context is the body's first parameter: an identifier or an object pattern.
* Pattern keys are `params`, `query`, `session`, `services`, `identity`, `input` or `now`, each optionally renamed with `key: local`. Any other key, a nested pattern or a rest element is residue.
* Imports, type declarations and non-action exports are ignored.
* A default import of a relative specifier naming an image binds to `{ src, width, height, sources }` through the set's `AssetResolver`. A font or a media file binds to its URL. An import the resolver does not know stays unbound.
* A `.json` file the application imports is its value as the default export.
* A module that binds one name twice at top level is residue at the second binding, naming the line of the first.
* Two `var`s of one name, overload signatures and `declare` statements bind a name once. Types, enums and namespaces are not counted.
* `export default tree(...)` with `tree` from `@snapfire/fsr/client/react` is `LowerError::Retired`.
* A component export that is `export default name` of an app import lowers as the component it names. So does `export { x as name } from "..."` of an app module.
* Such an export from a package is residue naming the package.
* A component export made by a call the build cannot read (`observer(...)`, `dynamic(...)`, `const { Link } = createNavigation(...)`) or by a tagged template is residue naming the call.

### Names and Bindings

* A name is read by what it is bound to. Each file's module-level names and exports resolve once through imports, namespace and default imports, re-exports, `export *` and module-level `const` aliases.
* Library meanings recognised by binding: React's hooks and `createContext`, the client's `useStore`, `useLocale` and `useHoisted`, the store's `key`, the dialect's `createContext` and `use`, the placements, island aliases, the retired `tree`, the server's `action` and `fail`, the client's `action`, `t`, `native`, the standard member modules, Vue's composition functions, the generated client's `actions` and image or font imports.
* An aliased, namespaced, re-exported or `const`-aliased spelling of any of these lowers as the bare import does.
* The application's own names resolve to the file and name that declare them. A custom hook is a hook by the name it is declared under.
* A name bound inside a function is a local. It is never a hook, placement or context. A tag naming one is residue.
* A name nothing binds, such as `Date` or `Intl`, is a JavaScript global.

### FSR Components

`Link`, `Picture`, `Place`, `Embed` and `Mount` are read from `COMPONENTS_MODULE`, `@snapfire/fsr/client/react` or `@snapfire/fsr/template`.

* `<Picture>` lowers to a `<picture>` with a `<source type srcset sizes>` per format and the hashed original as its `<img>`. An image served as it is lowers to the `<img>` alone.
* The `<img>` carries `width`, `height`, `loading` (`lazy`, `eager` under `priority`), `decoding="async"` and `fetchpriority="high"` under `priority`.
* `src` may be an imported image, its `.src` or an entry of an object of imported images indexed by a value (`PHOTOS[key]`). An indexed entry lowers to a `Tmpl::If` per entry and renders nothing when none matches.
* With `source="<name>"` and any other `src`, the named template writes the `srcset` per policy width and `sizes` defaults to `100vw`. Without `source`, it is an `<img src>` as written.
* `sizes` defaults to `default_sizes`. `widths` is an array of positive numbers and `quality` a number or `{ avif, webp }`, both written out.
* A literal relative `src` is residue naming the import form.
* With `rewrite_images`, an `<img>` whose `src` is `<import>.src` lowers as a `Picture`. With `data-sf-raw` it stays a plain `<img>` of the hashed original, with `width` and `height` filled unless written.
* `<Place name as ...attrs>` lowers to `<div id="name">` (the `as` tag when given) with the other attributes and a `Tmpl::Outlet` for `name`.
* `<Place>`'s `name` is written out in letters, digits, `-` and `_`. A missing or computed `name`, an `id`, a computed `as` and children are refused.
* `<Embed provider id title>` lowers to a `div` carrying `data-sf-embed`, the player URL and `data-sf-embed-title`, holding a `button` with the still, the title and a play mark.
* `<Embed>`'s `provider` is `youtube` or `vimeo` and `id` a string, both written out. `title` is required. `style` and a spread are refused.
* `<Link>` lowers to an `<a>` carrying `data-sf-link` and an `aria-current` computed against `Expr::Path`.
* `<Link current="document">` computes `aria-current` against `Expr::Document` and adds `data-sf-current="document"`. `current="url"` is the default.
* `<Link>`'s `match` and `current` are written out.
* `<Island when="visible">` with `Island` from `@snapfire/fsr/client/react` places its one component child as `Tmpl::Island`.
* A module-level `const Lazy = island(Chart, { when })` makes `<Lazy ... />` the same.
* `when` is written out as `"load"`, `"visible"` or `"idle"`. `<Island>` around an element, with another attribute or with more than one child is residue.
* `<Island mode="server">` and `island(C, { mode: "server" })` set `Tmpl::Island.mode`. `"browser"` is the default. Any other value is residue.
* A tag meaning a context (`<X>`) or its provider (`<X.Provider>`, `<P>` after `const P = X.Provider`, `<Ns.X>`) lowers to a context `Tmpl::Let`. `X` is made by `createContext(...)` from `react` or `@snapfire/fsr/template`.
* A React context makes the component's owner `Owner::React`. A `<Y.Provider>` whose `Y` is no context is residue. `X.Consumer` is residue.
* Ink's `Box`, `Text`, `Spacer` and `Newline` from `INK_MODULE` lower to `ink-box`, `ink-text`, `ink-spacer` and `ink-newline` elements with their props as kebab-case attributes.
* On an Ink element `key` is dropped, `ref={r}` is written as `ink-ref` and a spread stays a spread.
* Ink's `Static` is a component in `INK_FILE`: a column `Box` over its items with `style` spread onto it.
* In a `.svelte` file, `<Link>`, `<Picture>`, `<Mount>`, `<Embed>` and `<Place>` write what the Svelte components the build supplies write. A directive on one is residue. So is an attribute mixing text and expressions.
* In a `.svelte` file, `<a {...link({ ... })}>`, `picture({ ... })` and `embed({ ... })` from `SVELTE_CLIENT` lower the same way when their options are an object literal with keys written out.

### Statements

* `const x = e` or `let x = e`. Several declarators in one statement are each their own `Let`.
* `let x;` with no value starts as `null`. Any other uninitialised binding is residue.
* A destructuring binds `e` once under a temporary (`$d1`). Each name reads a part of it: an object pattern's fields (renamed or nested) or an array pattern's items with holes.
* A rest in a pattern is `Builtin::Omit` of an object or `Builtin::Slice` of an array.
* A default in a pattern is `Coalesce`. It applies to null as well as to a missing part.
* `for (const x of e) body`, with `x` a name or a pattern.
* `for (let i = a; i < b; i++)`, with `<=`, `++i` or `i += s`, is a `Let` of the start and a `ForOf` over `Builtin::Range` of the count.
* The count is `ceil((b - a) / s)` under `<` and `floor((b - a) / s) + 1` under `<=`.
* A step that is not a positive literal is tested at run time. A step that is not positive is refused by `Range`.
* A counter the loop body writes is residue. So is a bound or a step reading what the loop writes.
* `while (test)` on a terminal page is a `ForOf` over 10,000 passes that leaves at the first false test. Elsewhere it is residue.
* `break` and `continue` without a label are `Stmt::Break` and `Stmt::Continue`.
* `if (c) fail("kind", msg)`, bare or in a one-statement block with no `else`, is a guard.
* The guard's kind is a string literal. Its message is any expression.
* `fail("kind", msg)` as a bare statement is a guard whose condition is `true`.
* Any other `if` is a conditional whose branches are blocks or single statements.
* `return e` or `return`.
* `x = e`, every compound assignment, `x++`, `x--`, `x.field = e`, `x.push(...)` and `x.unshift(...)` as a statement on a local `x` are `Stmt::Set`.
* A `Set` updates the nearest binding. A write inside a branch or a loop outlives it.
* A field write rebinds `x` to a copy with that field. A push rebinds it to a copy with the items.
* A write at a computed key or index rebinds `x` to a copy with that entry.
* `delete x.field` or `delete x.a[key]` as a statement rebinds `x` to a copy without that key, through `Builtin::Omit`.
* A local bound to an element of a local array (`rows[i]`, `rows.at(i)`) or to a field path of a local is an alias. A write or `delete` through it rewrites the array or the root as well.
* A write through an alias after the array or the root was itself written is residue. A name holding the same object by any other route does not see the write.
* `switch (d) { ... }` binds `d` under a temporary and is an `if` chain comparing each `case` with `===`.
* Cases with no statements share the next one's. An arm ends with a `break`, a `return` or the end of the last arm.
* A `switch` arm that falls into the next, a `break` before the end of its arm and a `default` before the last `case` are residue.
* `session.key = e`, `session.key.sub = e` and `session.key[e] = e`, also through `ctx.session`.
* `delete session.key[e]`, `delete session.key?.[e]` and `delete session.key.sub`.
* `session.extend(e)` or `ctx.session.extend(e)` as a statement is `Stmt::SessionExtend`, `e` the seconds. It lowers in an action, a route handler or middleware with exactly one argument.
* `try { } catch (e) { } finally { }` is `Stmt::Try`. The `catch` and the `finally` are each optional. A `catch` parameter may be a pattern.
* `throw e` is a `Throw` builtin over the message and the value thrown.
* `redirect(to)` from `@snapfire/fsr` is a `RedirectTo` builtin in an action or a route handler. As a statement it is a `Return` of it. In a loader or middleware it is residue.
* `after(fn)` from `@snapfire/fsr` is `Expr::After` of a lambda with no parameters. `fn` is written in place, takes no parameters and does not write or extend the session.
* `new Response(body, init)`, `Response.json(value, init)` and `Response.redirect(url, status)` are the `Response`, `ResponseJson` and `ResponseRedirect` builtins.
* `new Uint8Array(source)` is `Uint8Array`.
* In middleware, `next(request?)` and `ctx.next(request?)` are `Expr::Next`. `new Request(url)` is `RequestNew`.
* `response.status` and `response.ok` are `ResponseMember`. `response.headers.get(name)` and `.has(name)` are `ResponseHeaderGet`.
* `response.headers.set`, `.append` and `.delete` as statements on a local rebind it through `ResponseHeaderSet`. They fail at run time on anything but a `Response`.
* A function declared in a body is a `Let` of its lambda before the body's first statement. It is a `Fix` when it calls itself.
* A body function that calls a function declared after it in the same body is residue.
* Any other expression statement is `Stmt::Expr`.
* `for...in`, `for await`, classes, labels and bare blocks are residue.

### Component Bodies

* Before the tree, a component body holds `const`s, `let`s, inner functions, effect hooks, blocks, writes to its own locals and `if` and `switch` with no `return` inside.
* A write rebinds its name. After an `if` the value is a `Ternary` over the branches. After a `switch` it is a chain of them.
* A write to a prop's name works the same way. A write to state is residue.
* A call to one of the component's own setters during render rebinds `render::RERUN_PREFIX` and the state to `[value]`. The body ends with a `Let` of that name for each state a setter wrote.
* A setter called after a branching `return` is residue.
* `if (c) return t;` with no `else` is an early return: the tree is an `If` choosing `t` when `c` holds.
* A `let` or a write after an early return is computed where no early return fired. A hook after one is residue.
* Any other `if`, `switch` or block that returns makes the rest of the body a tree. Each `return` is a leaf and a declaration or write is a `Tmpl::Let` over what follows.
* A path that ends without a `return` renders nothing. A hook or function declared there is residue.
* `useState`, `useStore`, `useMemo`, `useRef`, `useCallback`, `useLocale` and effect hooks lower. `useState(x)` reads as `x`, `useMemo(f)` as `f()` and `useRef(x)` as `{ current: x }`.
* Event handlers, inner functions, effects and `useState` setters are dropped from the render.
* `useReducer(reducer, initial, init?)` is state starting at `initial` or `init(initial)`. Its `dispatch` is a handler.
* `useContext(c)` and `use(c)` are `Coalesce(Expr::Context(id), default)` wherever an expression stands. `id` is the `file#name` that calls `createContext` and `default` its argument.
* `use` of anything that is not a context is residue.
* `<c.Provider value={v}>` is a `Tmpl::Let` named `render::CONTEXT_PREFIX` and the id, around its children.
* A custom hook is inlined from the file that declares it. Every name it binds is renamed `<hook>$<call>$<name>`.
* What a custom hook returns binds at the call site, part by part for an object or array literal. A returned setter or function becomes a handler there.
* A hook the build cannot follow to a declaration is residue.
* `useLinkStatus()` from FSR's React, Vue or Svelte adapter is `{ pending: false }` held as state.
* `useId()` is `let name = Expr::HoistId(n)`, numbered with the component's hoist candidates. The server writes `_S_<module>|<n>[@<path>]_`.
* `useId` inside a custom hook is residue.
* `xs?.map((x) => <jsx>)` is a `Tmpl::If` that writes nothing when `xs` is null or undefined and the `Tmpl::For` otherwise.
* A `.map` callback with a block body lowers as a tree. It can declare and write its own locals and return from inside a branch.
* A loop in a component is a fold: each outer local the loop body writes is an `Expr::Reduce` over the items. Several names fold together as fields of one object.
* In a component fold, `if (...) continue;` skips the rest of a pass. A `return` in a helper's loop gives the first value returned.
* In a component fold, `break`, any other `continue`, `for await` and every other loop are residue.
* A fold whose pass only appends markup to an array places what the array held before followed by a `Tmpl::For` over the items.
* A call in the markup to a function the component or its module declares, whose body returns markup on some path, is inlined where it is called.
* Each argument of an inlined markup call is bound to its parameter by a `Tmpl::Let`. A spread argument is residue.
* A markup function that calls itself is a `Tmpl::Component` placing `file#name`. One taking a single object pattern gets the argument as its props. Any other gets each argument as the prop named by its position (`"0"`, `"1"`).
* A markup function declared inside the component that reaches itself is residue.
* A capitalised tag naming a local is a `Tmpl::If` per tag string the local can hold. A list the build cannot complete is residue. So is an entry that is not a lowercase element name.
* `Children.count(children)` and `Children.toArray(children).length` over the component's own children read `render::CHILD_COUNT`. `Children.map`, `forEach`, `only` and `toArray` read `render::CHILD_ITEMS`.
* An object literal in a component body whose every field is a function and that does not lower as a value is a table of handlers. Each field lowers as a handler of the component.
* A module-level `let` a function of its file writes is state. So is a module-level collection a function changes in place.
* Such a module state is bound in a component to the store key `file#name`, seeded with what the module's initialisation leaves it at.
* A constant of a module with such state is read by replaying the module's top-level `let` and `const` bindings in order.
* A render calling a helper that writes module state runs the write on the request's copy. A loader or action reading module state is residue.
* A name a script function writes may be held in a cell. It is `Expr::Cell` per instance. One in Svelte's `<script module>` is `Expr::ModuleCell` keyed `file#name`.
* A cell read is `ReadCell`. An assignment or update to it is `WriteCell`.
* Where cells are held, a statement that writes a cell or makes a call becomes a `Let` of an `Expr::Block` before the rest.
* A helper's body or a lambda's block lowers to one expression: a `return` in a branch is a `Ternary`, a write rebinds its name and a path with no `return` is `null`.
* A helper that writes a module-level `let` keeps its statements as an `Expr::Block` that runs in the caller's scope.
* `const { default: Name, meta } = await import(\`prefix${key}suffix\`)` makes every file under the directory `prefix` ends in a choice. Directories named `node_modules` or starting with `.` are left out.
* `<Name />` from such an import lowers to a `Tmpl::If` chain comparing the key, as a string, with each choice. A key naming none renders nothing.
* Each other destructured name is a `let` of the chosen file's export of that name, `null` for none.
* An `async` component is accepted when every `await` in it is of an `import()`.

### Markup Values

* JSX in a `const` or `let`, a write, an attribute or an object field is lowered into `Expr::Markup`.
* A local holding markup is substituted where it renders. A branch becomes a `Tmpl::If`.
* A placement's attribute holding markup becomes a `Tmpl::Let` among its children named `render::SLOT_CONTENT_PREFIX` and the prop.
* In the component that takes it, each `{props.name}` is a `Tmpl::Let` named `render::SLOT_READ_PREFIX` and the prop.
* An array of markup (returned, written in place, held in a local, passed as a prop, built with `push` in a branch, `concat` or kept with `filter(Boolean)`) is a `Tmpl::Fragment` of its items.
* Markup arrays carry a `render::ARRAY_ITEMS` let around their fragment.
* Markup in data renders where it is read. Markup in an element's attribute is residue. So is markup read as a value.
* Markup passed as a prop to a component that renders in the browser crosses as a region the server renders and the island adopts.
* `createElement(type, props, ...children)` bound to React's is read as the JSX it stands for.
* In `createElement`, a lowercase string tag is an element. An uppercase identifier or `A.B` is a component. `null` or `undefined` props are none.
* A `createElement` with a tag held in a variable, props that are neither an object literal nor an identifier or a computed or method field is residue.

### Expressions

* Parentheses, `await`, `as`, `!` non-null, `satisfies`, type assertions and optional chaining on a member are transparent.
* Reads: `params.x`, `query.x`, `session.x`, `identity.x`, `identity.x.y`, `input` and `now`, also under `ctx.`.
* A session read whose key has a default is `Coalesce(Session, default)`.
* A context root used whole is residue. So is a computed key on a root.
* Strings and booleans lower as themselves. Numbers are `Lit::Float` and bigints `Lit::Int`. `null` and `undefined` are `Lit::Null`.
* A regular expression is an object holding its source under `jsregex::PATTERN` and its flags under `jsregex::FLAGS`.
* A regular expression `jsregex::compiled` refuses is residue: the `y` flag, a nested unbounded quantifier, a pattern past its length cap or one JavaScript would not compile.
* Template literals. Object literals with shorthand, `key: value`, `[computed]: value` and spread entries. Array literals with spread.
* A method in an object literal is a function value. A getter or a setter is residue.
* A `function` expression lowers as an arrow does. A generator is residue.
* Operators: `+ - * / % **`, `& | ^ << >> >>>`, `=== !== == != < <= > >=`, `&& || ??`, `!`, `~`, unary minus and `?:`.
* `~x` is `x ^ -1`. A bitwise operator with a `bigint` side is residue.
* `k in x` is `Builtin::HasKey`, an own key of an object or an index of an array.
* `Array.isArray(x)` and `x instanceof Array` are `Builtin::IsArray`.
* `typeof x` is `Builtin::TypeOf`. It answers `"undefined"` for null as well as for undefined.
* `typeof x` is written out as `"bigint"` or `"object"` where `x` has a known kind.
* `x instanceof Map`, `Set` or `Date` is a constant where the kind of `x` is known and residue where it is not.
* `x instanceof Error` and `instanceof` any error constructor are `Builtin::IsError` with the constructor's name.
* `instanceof` of another class is residue.
* `a.b` and `a["b"]` are field reads. `a[e]` is an index. `a.length` is `Expr::Length`.
* `a?.b.m(x)` is the call as written, `null` when an optional link's object is null or undefined. `f?.()` is residue.
* `String(e)`, `Number(e)` and `BigInt(e)`. `x.toString()` is `String(x)`.
* `Object.entries(e)`, `Object.keys(e)` and `Object.values(e)`.
* `await services.<s>.<m>(args)` and `ctx.services.<s>.<m>(args)`, with `args` absent or one object literal. Argument values that are `null` at run time are omitted.
* `x++`, `x--`, `++x` and `--x` on a local inside an expression are lifted before the statement when it reaches them unconditionally. Under a condition, a short-circuit or a function they are residue.
* A module function that calls itself lowers to `Fix { name, f }`. A call nested `interp::MAX_APPLIES` deep fails.
* A function's name where a callback goes, `items.map(sum)`, is that function value.
* `object.name(args)` on an object literal whose field holds a function applies that field. `object[key](args)` applies the field the key names.
* `f(a, ...rest)` into a function the module or the component declares is `ApplyOf`. So is one into a function value in scope. A spread into any other call is residue.
* `console.log(a, ...rest)` is `LogOf` over `[a, ...rest]`.
* `new Error(message)`, `Error(message)` and the other error constructors are `{ name, message }`.
* Any other call, `this`, comma expressions, tagged templates, `yield` and assignment inside an expression are residue. A call to a name that is not a builtin names it in the message.

### Builtins

* Each is an `Expr::Builtin` with the receiver as its first argument.
* `Math.round`, `floor`, `ceil`, `abs`, `min`, `max`, `pow`, `sqrt`, `trunc` and `sign`.
* `Math.hypot`, `imul`, `clz32`, `fround` and `f16round` are `Builtin::Math` with their name, beside the transcendental functions and `tanh`.
* `Math.random()` is `crypto.random` scaled to `[0, 1)` in a body and in a terminal page. On a web render path or in a module-level constant it is a value the browser fills in, since a component may read the constant.
* `Number.MAX_SAFE_INTEGER`, `MIN_SAFE_INTEGER`, `EPSILON`, `MAX_VALUE`, `MIN_VALUE`, `POSITIVE_INFINITY`, `NEGATIVE_INFINITY` and `NaN` lower to their values.
* `isFinite(x)` is `Number.isFinite(Number(x))`. `parseFloat` and `Number.parseFloat` are `Builtin::ParseFloat`.
* `Object.assign({...}, ...sources)` onto an object written in place is a spread of every argument. Onto anything else it is residue naming `{ ...target, ...source }`.
* `Object.freeze(x)` is `x`. `Object.hasOwn(o, key)` and `o.hasOwnProperty(key)` are `Builtin::HasKey`, true for an array's or a string's `length` and indices. `o.propertyIsEnumerable(key)` is `HasKey` without an array's or a string's `length`. `Object.is(a, b)` is `Builtin::SameValue`.
* `Object.groupBy(items, f)` is a `reduce` into an object keyed by `String(f(item, index))`. `Map.groupBy(items, f)` is the same into a `Map` keyed by the value itself.
* `Object.getOwnPropertyNames(o)` and `Reflect.ownKeys(o)` are the keys, with an array's indices and `length`. `Object.getOwnPropertyDescriptor(o, key)` is a writable, enumerable and configurable value, since a body's objects carry no accessors. `Object.create(null)` is `{}`; another prototype is residue.
* `Object.getPrototypeOf(x)` and `C.prototype` of a built-in constructor are a tagged value naming the constructor, so `Object.getPrototypeOf(x) === Array.prototype` compares them. `C.prototype.isPrototypeOf(x)` is `x instanceof C`, with `Object` any object. `isPrototypeOf` of anything else is `false`, since nothing makes an object with a prototype.
* `Object.seal(x)` and `Object.preventExtensions(x)` are `x`. `Object.isFrozen`, `isSealed` and `isExtensible` answer for a primitive, an object or array written in place and a value `Object.freeze`, `seal` or `preventExtensions` makes in place. Of anything else they are residue.
* `Reflect.get`, `has`, `set`, `deleteProperty`, `apply` and `construct` are the read, `in`, write, `delete`, call and `new` they stand for. `Reflect.getPrototypeOf` and its kin are the `Object` function of the same name.
* `Symbol.for(key)` is a tagged value holding its key, so two of one key are equal and `typeof` is `"symbol"`. `Symbol.keyFor(s)` reads the key back. `JSON.stringify` leaves a symbol out of an object.
* `JSON.rawJSON(text)` is a tagged value `JSON.stringify` writes as `text`. `JSON.isRawJSON(x)` tests the tag.
* `BigInt.asUintN(bits, n)` is `n` modulo 2 to the `bits`. `BigInt.asIntN` reads the top bit as the sign.
* `String.raw(strings, ...subs)` joins each raw string with the substitution after it.
* `Array.of(...items)` is the array. `String.fromCharCode` and `String.fromCodePoint` are `Builtin::FromCharCode` and `FromCodePoint`. `Date.UTC(...)` is `Builtin::DateUtc`.
* `Promise.reject(e)` where no handler resumes later throws `e`, since a body holds no promise values.
* `Promise.allSettled(list)` settles each element of a list written in place as `{ status, value }` or `{ status, reason }`. Over a `map`, it settles each call of the map's function.
* `Promise.try(f, ...args)` is `f(...args)`. `Promise.any(list)` is the first item that does not throw, else an `AggregateError` holding each reason. `Promise.race(list)` is the first item, the others still made. `Array.fromAsync(items, f?)` is `Array.from`.
* `Promise.withResolvers()` is `{ promise, resolve, reject }` with the promise an object holding its state, which `resolve` and `reject` overwrite. Each `await` in a body that makes one is `Builtin::Settle`, which answers the value or throws the reason. A promise read before anything settles it is an error, since a body does not wait.
* `Math.min(...items)` and `Math.max(...items)` are `Builtin::MinOf` and `Builtin::MaxOf` over one array. A spread into `Math`, `String.fromCharCode`, `fromCodePoint`, `concat`, `slice`, `at`, `indexOf`, `lastIndexOf`, `charAt`, `charCodeAt`, `codePointAt`, `padStart`, `padEnd` or `substring` is `Builtin::Spread` of that builtin over the list the arguments make. A spread into another builtin is residue.
* A function written where it is called, `((x) => x + 1)(1)`, is an `Expr::Apply` of its lambda.
* `f.call(this, ...args)` and `f.apply(this, args)` of a function value are an apply of `f`, and `f.bind(this, ...args)` a lambda that applies `f` to the bound arguments and its own. `this` is not read.
* `Array.from({ length })`. `Array.from(x)` copies an array and reads a `Map`'s entries.
* `encodeURIComponent(e)`, `encodeURI(e)`, `decodeURI(e)`, `JSON.stringify(e, replacer, indent)` and `JSON.parse(s)`. A function replacer is called with each key and value, the top value under `""`. An array replacer keeps the fields of each object it names, in its order.
* String and array methods: `toFixed`, `repeat`, `join`, `trim`, `toUpperCase`, `toLowerCase`, `includes`, `startsWith`, `endsWith`, `split`, `replace`, `slice`, `at`, `indexOf`, `concat`, `padStart`, `padEnd` and `substring`.
* `substr`, `isWellFormed`, `toWellFormed`, `normalize`, `toPrecision` and `toExponential` are the builtins of those names.
* `toSpliced` and `with` are `Builtin::ToSpliced` and `ArrayWith`. `fill` and `copyWithin` lower over an array the expression builds and over a local the body holds. `new Array(n).fill(v, start, end)` fills that span.
* `s.anchor(name)`, `big`, `blink`, `bold`, `fixed`, `fontcolor(color)`, `fontsize(size)`, `italics`, `link(url)`, `small`, `strike`, `sub` and `sup` are `Expr::Template` of the element around `String(s)`, a `"` in the attribute written `&quot;`.
* A `Set`'s `union`, `intersection`, `difference` and `symmetricDifference` are a `Set` in JavaScript's order, `intersection` walking the smaller set. `isSubsetOf`, `isSupersetOf` and `isDisjointFrom` are booleans. The argument is a `Set` or a `Map`'s keys.
* `a.localeCompare()` compares with `"undefined"`.
* `e.entries()`, `e.keys()` and `e.values()` of an array or a `Set` are `Builtin::Iterate`.
* A standard method no arm lowers is residue at build rather than a call of a field the value does not have.
* A write to a value made in place, `[3, 1, 2].pop()`, `new Map().set(k, v)` or `new Date(0).setUTCHours(1)`, takes the path a write to a local takes. `Map.set` and `Set.add` answer the collection, so `.size` reads after them. `delete` answers whether the key was there and `clear` answers `undefined`.
* `re.compile(pattern, flags)` overwrites a local or literal regular expression. A regular expression's `source`, `flags`, `global`, `ignoreCase`, `multiline`, `dotAll`, `unicode`, `unicodeSets`, `sticky` and `hasIndices` read from its pattern and flags. `JSON.stringify` writes it as `{}`.
* `d.toTemporalInstant()` is `Temporal.Instant.fromEpochMilliseconds(d.getTime())`.
* A `Set`'s `entries()` pairs each item with itself.
* `new WeakMap(pairs)` and `new WeakSet(items)` are a `Map` and a `Set` keyed by object identity, with `get`, `has`, `set`, `add` and `delete`. A key that is not an object throws a `TypeError`, as JavaScript does. A body runs to its end without a collection, so nothing they hold goes. `new WeakRef(x).deref()` is `x`. A `FinalizationRegistry`'s `register` and `unregister` change nothing, since its callback never runs within a body.
* An expression statement in a function, `new WeakSet(items);` or a call for its effect, is evaluated where it stands for what it does and what it throws.
* `new Promise(executor)` runs the executor at once with `resolve` and `reject`, the promise the object `Promise.withResolvers()` makes. A throw from the executor rejects it.
* `new Error(message, { cause })` and its kin carry `cause`. `new AggregateError(errors, message)` carries `errors` and `new SuppressedError(error, suppressed, message)` carries both. `instanceof Error` holds for each.
* `new Object()` is `{}` and `new Object(x)` is `x` when it is an object. `new Array(n)` is `n` empty slots, a `RangeError` for a length that is not a whole number below 2 to the 32. `new Array(a, b, ...)` is the items.
* The typed arrays other than `Uint8Array` are arrays of numbers, each element converted as its type stores it: `Builtin::ElementOf` wraps, clamps or rounds it. `new ArrayBuffer(n)` and `new SharedArrayBuffer(n)` are `n` zero bytes and `byteLength` reads their length. `new DataView(buffer)` is that buffer's bytes. Its `getX(offset, little)` is `Builtin::ViewGet` and its `setX(offset, value, little)` writes the local through `Builtin::ViewSet`. A typed array made over a buffer copies it, so a write through one view is not seen through another.
* `values()`, `keys()` and `entries()` are the arrays they would yield, so the iterator helpers `map`, `filter`, `flatMap`, `reduce`, `some`, `every` and `find` lower as the array methods. `take(n)`, `drop(n)` and `toArray()` over one, and `Iterator.from(x)`, are a slice or a copy.
* `String`, `Number`, `BigInt`, `parseFloat`, `encodeURIComponent` and `decodeURIComponent` passed as a callback are the conversion. `parseInt` passed to `map` takes the index as its radix, as JavaScript does.
* `new Number(n)`, `new Boolean(b)` and `new String(s)` are wrapper objects: `typeof` reads `"object"`, they are always truthy and every other read is of the primitive they wrap.
* A string's indices count UTF-16 code units.
* `startsWith` and `endsWith` take exactly one argument. `replace` takes exactly two.
* `split` takes a separator and an optional limit. `split("")` fails at run time on a character outside the Basic Multilingual Plane.
* `e.map(f)`, `filter`, `find`, `findIndex`, `some`, `every`, `flatMap` and `reduce(f, init)`. Each callback receives the item, the index and the array.
* `findLast`, `findLastIndex` and `reduceRight` run the forward method over the reversed items, with each callback handed the item's index in the original order.
* `e.filter(Boolean)` is `e.filter((x) => !!x)`.
* `clsx` (named or default import) and the default import of `classnames` are `Builtin::ClassNames` over their arguments as one array.
* `e.toSorted(f)` and `e.sort(f)` are `Expr::Sort` with `f` or `null`. `e.toReversed()` and `e.reverse()` are `Builtin::Reverse`.
* As an expression, `sort` and `reverse` lower only over an array the expression builds. Otherwise they are residue naming `[...items].sort()`.
* As a statement on a local, `sort`, `reverse`, `pop` and `shift` rewrite the local.
* On a local the body holds, `splice` is `Builtin::Splice` and answers the items it removed. `getOrInsert(k, v)` and `getOrInsertComputed(k, f)` of a `Map` write the key when it is missing and answer its value.
* `let x;` binds `undefined`.
* `re.test(s)`, `re.exec(s)`, `s.match(re)`, `s.matchAll(re)` and `s.search(re)` are `Builtin::RegexTest`, `Exec`, `Match`, `MatchAll` and `Search`. A string given to `match` or `search` is read as a regular expression, as JavaScript reads it.
* A regular expression carries `lastIndex`. `re.exec(s)` and `re.test(s)` on a local are `Builtin::ExecFrom` and `TestFrom`, which read and move it for `g` and `y` as JavaScript does. A literal's starts at 0. With `y` a match must start at `lastIndex`; `split` ignores `y`.
* `s.replace(re, to)` and `s.split(re)` take a regular expression. `s.replace(pattern, f)` with an arrow is `Expr::ReplaceWith`.
* `s.replaceAll(from, to)` is `Builtin::ReplaceAll` for a string and `Replace` for a regular expression, which must carry `g`.
* `structuredClone(x)` is `x`.

### Dates, Encoding and Intl

* `new Date(x)` and `Date.parse(s)` are `Builtin::DateMs`, from a number or an ISO 8601 string.
* `new Date()` and `Date.now()` read the request's clock in a body. On a render path they are residue.
* `new Date(year, month, ...)` is `Builtin::DateUtc` moved by `time.localOffset` in a body and in a terminal page. A time a change to daylight saving skips or repeats takes the offset from before the change. On a web render path it is a value the browser fills in.
* A date's `getTime()`, `valueOf()`, `toISOString()`, `toJSON()`, `toUTCString()` and `getUTC*()` lower. `setTime` and the `setUTC*` setters rebind a local `Date`, a part past its range carrying into the next.
* The local-time getters, `getYear`, `getTimezoneOffset`, `toDateString` and the local setters read `time.localOffset` in a body and in a terminal page. On a web render path they are a value the browser fills in.
* `d.toTimeString()` is `intl.timeString` in a body and in a terminal page. A `Date` as text, `d.toString()`, `String(d)`, `` `${d}` ``, `"" + d` and `d == text`, is `intl.dateTimeString` there. On a web render path both are a value the browser fills in.
* `performance.now()` is `Num` over `time.monotonic`, under the same render-path rule as `Date.now()`.
* `new Temporal.<type>(...)` is `temporal.new`, and `Temporal.<type>.from(...)`, `compare(...)` and the `Instant` epoch constructors are `temporal.static`. A field read of a Temporal value is `temporal.get` and a method call `temporal.call`. A value is Temporal when one of these made it, a method that answers a Temporal value did, or its type annotation names `Temporal.<type>`.
* `Temporal.Now.<name>()` is `temporal.now` in a body and in a terminal page. On a web render path it is a value the browser fills in.
* `crypto.randomUUID()` is `crypto.uuid`, under the same rule.
* `d.toLocaleTimeString(locales?)` is `intl.localTime` in a body and in a terminal page, the locale written in place or computed. On a web render path it is a value the browser fills in. With options it is `intl.date`.
* `d.toLocaleDateString(locales?)` and `d.toLocaleString(locales?)` with no options are `intl.localDate` in a body and in a terminal page: the date in CLDR's `yMd` pattern, then for `toLocaleString` the time, joined by the locale's short date-time pattern. On a web render path they are a value the browser fills in.
* `s.toLocaleUpperCase(locales)` and `s.toLocaleLowerCase(locales)` are `intl.upper` and `intl.lower` everywhere. A locale the code computes falls back to `intl.systemLocale` in a body and in a terminal page and to `intl.localeTag` on a web render path when it is `undefined` at run time. With no locale they read `intl.systemLocale` in a body and in a terminal page. On a web render path that is a value the browser fills in.
* `btoa(s)` and `atob(s)` are `encoding.btoa` and `encoding.atob`.
* `new TextEncoder().encode(s)` and `new TextDecoder().decode(bytes)` are `encoding.encode` and `encoding.decode`, with bytes as an array of numbers.
* A `TextDecoder` with a label other than UTF-8 is residue. So is one with options.
* `new Intl.NumberFormat(locale, options).format(n)` and `e.toLocaleString(locale, options)` are `intl.number`, with `style` `decimal` or `percent`, the fraction digits, `minimumIntegerDigits`, `useGrouping` and `signDisplay`.
* `Intl.NumberFormat` with `style: "currency"` is `intl.currency`, with `currencyDisplay` as `display` (default `symbol`) and the fraction digits.
* `new Intl.DateTimeFormat(locale, options).format(d)` and `d.toLocaleDateString(locale, options)` are `intl.date`.
* `intl.date` takes a `dateStyle` or a `timeStyle` of `short` or `medium`.
* `intl.date` also takes the parts `weekday`, `year`, `month` and `day` with a text month or weekday (`long`, `short`, `narrow`) and a numeric year and day.
* `timeZone: "UTC"` is allowed. On a web render path it is required beside a `timeStyle`. In a body and in a terminal page a date with no `timeZone` is formatted in this machine's zone, as node formats it, the instant moved by `time.localOffset`.
* `new Intl.PluralRules(locale, { type }).select(n)` is `intl.plural`.
* `new Intl.Collator(locale, { sensitivity, numeric }).compare(a, b)` and `a.localeCompare(b, locale, options)` are `intl.compare`. `collator.compare` passed as a function is a lambda over it.
* `new Intl.RelativeTimeFormat(locale, { numeric, style }).format(value, unit)` is `intl.relative`.
* `new Intl.ListFormat(locale, { type, style }).format(items)` is `intl.list`.
* The locale is `undefined` for the request's or an expression.
* A formatter kept in a `const`, local or module-level, binds its name to null and its method lowers as the inline form.
* Each formatter call is a hoist candidate in a component.
* A numeric month, a date with neither a style nor a part, an option a formatter does not read and a string option outside its values are residue.

### Collections and Kinds

* `new Map(pairs)` and `Object.fromEntries(pairs)` are `Builtin::FromEntries`, an object keyed by `String(key)`.
* `new Set(items)` is `Builtin::Unique`.
* `new URLSearchParams(x)` is `Builtin::FormEncode`. Its `.get(k)` is `Builtin::FormGet`.
* A name bound to a `new Map`, `new Set` or `new URLSearchParams`, module-level included, has that kind.
* A `Map` or `Set` name's `.size`, `.get(k)`, `.has(k)`, `.keys()`, `.values()`, `.entries()`, a spread of it and `for...of` over it read the collection.
* In a body, `m.set(k, v)`, `m.delete(k)` and `s.add(x)` on a local are writes.
* `new` of anything else is residue.
* `new Date(...)`, a bigint literal and `BigInt(x)` give their kinds the same way.
* A prop has a kind where the props parameter's type names `Map<K, V>`, `ReadonlyMap`, `Set<T>`, `ReadonlySet`, `Date`, `URLSearchParams` or `bigint`, in place or through a `type` or `interface` the file declares.
* Without a type, a prop has the kind every placement in the first file to place the component passes. The type wins where both say something.
* A placement passing a known kind that differs from the kind the component read is residue at that placement.
* Each placement carries the kinds it knows under `render::TYPES_PROP`, `$t`.

### Extensions

* `X.m(args)` where `X` is a standard module imported from a side lowers to `Expr::Ext { module: X, name: m, args }`. A member `standard_reach` does not know is residue naming it.
* `@snapfire/fsr/client`, `@snapfire/fsr/host` and `@snapfire/fsr/terminal` export each module as an object. `<root>/<module>` exports its members.
* A member a side lacks is refused with where the member is. `native` is at `@snapfire/fsr`.
* A member whose `Member.platform` is set is under no side. Asking for one is refused with the platform code to write.
* A member whose reach is `terminal`, `later` or `process` is residue on a web page, with `Reach::needs` as the reason.
* Source needing `later` (an async handler, `new Promise`, a `setTimeout` that settles a promise, `p.then`) or `process` (`spawn`, `system.*`, `file.*`, `suspendTerminal`) is residue on a web page.
* A `body` member on a component's render path is `LowerError::Reach`. The set never downgrades it.
* A `body` member inside a handler, a body, middleware or a helper lowered on its own is allowed. The refusal follows an inlined helper to the render-path call, naming both.
* A `render` member is a hoist candidate.
* `export const f = native("module.member", g?)` at module level, with `native` from `@snapfire/fsr`, declares a native pair. The name is a string literal of two non-empty parts, else `LowerError::Extension`.
* A native pair's reach is `render` with `g` and `body` without. `f(args)` lowers to `Expr::Ext`.
* Every export of a module under `EXT_DIR` must lower, else `LowerError::Extension`.
* A name imported from one of Node's modules is residue naming FSR's members. So is the `process` global.

### Hoisted Calls and Subtrees

* In a component, a call to a module-level helper or a `render` extension call is wrapped as `Expr::Hoist` when its inputs are props only.
* A call's inputs are props only when no free variable reaches a `useState`, `useStore` or `useRef` binding or a name computed from one. It must also read no store key, request or service.
* A call inside a lambda body, under another hoist or with a state input stays a plain call.
* A `Tmpl::For` whose `over` reads state taints its parameters.
* Every element with children is a subtree candidate, marked `CHUNK_ATTR` with its id.
* A kept chunk is static and does work. A chunk inside a loop over state is never kept.
* A subtree is static when every expression is props only, no element carries `BOUND_ATTR`, it is not `sf-s`, it holds no island or slot and every component it renders is pure.
* A subtree does work when it holds an interpolation, a branch, a loop, a binding, a component or a non-literal attribute.
* A kept chunk under an `If` whose condition reads state carries `UNTAKEN_ATTR`.

### Function Props

* A function passed as a prop to a component that renders in the browser reaches it as a module reference in a `render::FUNCTIONS_PROP` object, `$f`, naming `file#export` per prop.
* An arrow or a `function` written in place that reads only its own names, module-level bindings and globals is hoisted into an export of its file named `__sfF<byte offset>`.
* A function the file declares is named by its own export, which the rewrite adds when missing. An imported function is named by its file and export.
* A conditional between two function props is an expression choosing between their ids.
* A function the server cannot lower is a `render::BROWSER_HANDLER` value with its reference.
* A function prop that reads a prop, a local or a function of the component is residue. So is a function held in a local.
* A module id carries no site prefix.
* A component in a plugin's language rendered inline by an `Owner::React` component lowers to an empty `<sf-s data-sf-island>`. Given children, it is `LowerError::ForeignChildren`.
* In an `Owner::Fsr` component such a component is residue that says to place it inside `<Island>`.
* A placement of a transformed file's default export with `components={{ key: Name }}` lowers the file again as `file~<hash>#default` with those elements as `Override`s. Any other `components` value is residue.

### Handlers

* Every `on*` attribute of an element lowers as a handler when it can: an arrow, a function declared in the component, a `const` holding one, `useCallback` or `() => void f()`.
* A handler body holds `const`s, state setter calls and action calls.
* `setX(expr)` or `setX((prev) => expr)` sets state, `prev` reading `X`.
* `save(input)` or `void save(input)`, where `save` is a module-level `const save = action("id")` with `action` from `CLIENT_SOURCE`, is `Stmt::Act { action: id, input }`. The input is `{}` when the call has no argument.
* `e.preventDefault()` and `e.stopPropagation()` are dropped. The event parameter reads as `$event`.
* The handler returns an object of the state it set, empty when it only called actions.
* An `if`, with `else` or `else if`, holds the same statements. A key set under a branch is `Ternary(reach, value, prior)`.
* An action called under a branch is `Stmt::If { cond: reach, then: [Act], else: [] }`.
* A `const` declared under a branch is hoisted as a `Let` under its own name, `rest$1`, with value `Ternary(reach, init, Null)`.
* A bare `return` in a branch makes the statements after the `if` reachable only where that branch did not run.
* A handler that sets no state and calls no action, loops or calls anything else leaves `$unlowered` on the element with the line and the reason.
* A function-valued prop on a component placement lowers into the placing component's handlers. The prop is `Expr::Handler(index)`.
* A state setter passed by name, `onChange={setQuery}`, is a handler that sets that state to `$event`.
* A handler's own `let` it writes later is a cell for the step. A branch condition that reads a cell is evaluated once.
* A module-level `let` or collection a called function writes is read back into the handler's patch.
* A handler prop that does not lower leaves `$unlowered` with the line and the reason among the placement's props. The prop is left out.
* An `on*` attribute naming a function the component was handed is `$on:<event>` holding that expression.
* `Component.state` lists the `useState` and `useStore` bindings. `Component.stores` lists the `useStore` ones with their keys. `Component.handlers` lists the handlers, indexed by the `$on:` attributes.
* On a web page, `setInterval(callback, ms)` or `setTimeout(callback, ms)` started by a `useEffect` or `useLayoutEffect` with empty dependencies lowers as a handler named `interval:<ms>` or `timeout:<ms>`. `ms` is a number written out or a module constant.
* An effect with dependencies, a timer whose callback does not lower and anything else an effect does are dropped.

### Terminal Pages

* An effect timer starts from a `mount` handler as the act `$timer`. Its callback reads the state of the render that ran the effect. An updater applies to the current state.
* Ink's `useInput(handler)` from `INK_MODULE` lowers as the component's `input` handler, its parameters reading `Field(Var("$event"), "input")` and `Field(Var("$event"), "key")`.
* In a mount effect, a statement that is not a declaration, a timer or a call to one of the effect's functions lowers as a `mount` handler of its own.
* In a mount effect, `system.spawn(command, args, options)` lowers to a `mount` handler holding the act `$spawn` `{ id, command, args, options }`.
* `sub.on("error" | "exit" | "close", f)`, `sub.stdout.on("data", f)` and `sub.stderr.on("data", f)` lower to handlers named `spawn:<id>:<event>`. `exit` and `close` bind `code` and `signal`.
* `sub.stdin.write(text)` and `sub.stdin.end()` lower to the act `$stdin` `{ id, text }` or `{ id, end: true }`. `$spawn` then carries `stdin: true`.
* An effect whose dependencies change lowers as a handler named `effect:<n>` and emits `EFFECT` with the dependencies' values.
* A changing-dependency effect that sets no state and calls no action is left out. So is one that does not lower and reads `document`, `window`, `navigator`, `location`, `history`, `localStorage` or `sessionStorage`.
* In a handler, `setInterval(f, ms)` or `setTimeout(f, ms)` with `f` an arrow in place is `$timer` `{ id, event, carry, ms, repeats }`. Its value is `{ "$timer": id }` (`snapfire_fsr_ir::host::TIMER`).
* `clearInterval` or `clearTimeout` of a timer is `$clearTimer`.
* An arrow returned by a changing-dependency effect is `$cleanup` `{ effect, event, carry }`.
* A module-level `let` a function writes is `ModuleCell { key, init }`. A step starts from `render::CELLS_PROP` and returns `render::Stepped::cells`.
* A handler's `let` that a listener or continuation writes is a `ModuleCell` keyed by a `crypto.random` value made per run.
* A module-level `new Map()` or `new Set()` a function changes with `set`, `delete`, `add` or `clear` is a cell.
* A module-level constant that reads the machine, a file, the clock, an extension member past `Render` reach or `now` is a `ModuleCell` made once per session.
* `child.on(event, f)` or `child.stdout.on("data", f)` on a spawned process is `$listen` with `f` a continuation.
* `new Promise(executor)` makes a deferred `{ "$deferred": id }`, with `resolve` and `reject` bound to `snapfire_fsr_ir::host::SETTLES` values. A call to one is `$settle`.
* `setTimeout(resolve, ms)` is a `$timer` that settles.
* A declaration whose value issues an act or draws randomness is bound once, where it is declared.
* An async arrow called where it is written runs to its first `await`, which issues `$then`. The rest is a handler named `resume:<file>@<offset>`, with the names it reads carried in the act.
* An `async` module function returns a deferred its statements settle. A returned promise is adopted.
* An awaited rejection inside a `try` runs the `catch`, then the `finally`, then what follows the `try`.
* In an async module function the `await` is a statement, a `const`'s value or an assignment `x = await f()`.
* A `for...of` over an array, a counted `for` or a `while` whose body awaits lowers its head to a continuation `resume:<file>@<offset>`, run through `$run` with every variable in scope carried.
* `for (const x of xs)` in such a loop reads as `const $items = xs; let $i = 0` with the test `$i < $items.length`.
* A `continue` or `return` at the top of an awaiting loop body ends that pass. So does one ending an `if` there.
* `break` inside an awaiting loop, a deeper `continue` or `return` and a `for...of` over a `Map` are residue.
* `suspendTerminal(f)` from `useApp` lowers `f` as an async function, then `$suspend` `{ until, settles }`.
* In a block, `spawn(...)` issues `$spawn` and is `{ "$process": id }`.
* `useStdout().stdout` is the terminal's output with `columns` and `rows`. `stdout.on("resize", f)` and `off` are `$listen` and `$unlisten`.
* `String.raw` is a template of its raw pieces. `throw` in a block is `Builtin::Throw`.
* `Suspense` lowers to `render::SUSPENSE_TAG` with its content and its fallback.
* A name imported from `INK_MODULE` that `INK_FILE` exports reads that file's export: `Static`, `useWindowSize`, `useStdout`, `useStderr`, `useIsScreenReaderEnabled`, `useFocus`, `useFocusManager`, `useCursor`, `useBoxMetrics` and `useApp`.
* `emit(kind, value)` from `@snapfire/fsr/terminal/core` lowers to `Builtin::Emit`. A step returns every emitted value as `render::Stepped::emitted`, in order.
* `action("id")` is an act reference `{ "$act": id }`.
* `useApp().exit` is the `$exit` act. `suspendTerminal` is `{ "$primitive": "suspendTerminal" }`.
* `ms`, `stripAnsi`, `stringWidth`, `wrapAnsi` and `bigText` from `@snapfire/fsr/terminal` lower to `terminal.ms`, `terminal.stripAnsi`, `terminal.stringWidth`, `terminal.wrapAnsi` and `terminal.bigText`.
* A `terminal` member is refused outside a terminal page.
* `system.*` and `clipboard.*` lower at `Process` reach. `file.*` and the host's `path` members come from `@snapfire/fsr/host`.
* `crypto.randomUUID()`, `crypto.random` and `crypto.digest(algorithm, encoding, ...parts)` lower as standard members.
* A module `const` holding `new Scope()` from `@snapfire/fsr/terminal` is a module cell starting undefined. `scope.get()` is `ReadCell` and `scope.set(v)` is `WriteCell`.
* `scope.run(v, fn, ...args)` sets the cell to `v`, applies `fn`, restores the previous value and answers the call's result. On a web page a scope is residue.
* An import of a module in `COMPANIONS` resolves to the TypeScript module the build provides under `$fsr/terminal/`.
* A module importing or re-exporting from a package in `REPLACED` is `LowerError::Residue` at that line, with a hint naming FSR's module.

### Lambdas

* An arrow in a builtin position. A parameter is an identifier, an identifier with a default or a pattern.
* A pattern parameter is named `$<index>` and each name it declares reads a part of it.
* A rest parameter, last, is named `...xs` and takes every argument past the named ones as an array. A rest that is not a plain name or not last is residue.
* The body is one expression or a block lowered as a helper's is.
* An arrow anywhere else is residue.

### Vue Files

A described `.vue` module lowers to a `Component` with `owner` `Owner::Vue` and no hoisting. The export is `default`.

* The script block is parsed as TypeScript at its line in the file. Each template expression is parsed at its line and column.
* `defineProps`, bound whole or destructured, is `$props`. `withDefaults` adds a `let $props` spreading the defaults under the placement's props.
* `ref` and `shallowRef` are a `let` and a state binding, read as the value in the template and through `.value` in the script.
* `computed` of an arrow is a `let`. `reactive`, `shallowReactive` and `readonly` bind their value.
* `useStore(key, initial)` from `VUE_CLIENT` is `let name = <store key> ?? initial`, read through `.value`.
* A `const` with any other value is a `let`.
* A function declaration or a `const` holding a function is a handler body an `@event` can name or call.
* `defineEmits`, lifecycle hooks, `watch`, `nextTick`, `defineExpose`, `defineOptions` and `defineSlots` are passed over. Interfaces and type aliases are nothing.
* Script residue: an export, a class, an enum, `toRef`, `toRefs`, `customRef`, `useTemplateRef`, `useId`, `useAttrs`, `useSlots`, a writable `computed`, a plain `<script>` and any other statement.
* The template root is the one child or a `Fragment`.
* `v-if`, `v-else-if` and `v-else` chain into `If`, with whitespace between branches dropped.
* `v-for="(item, index) in items"` is `For`. A `v-if` on the same element lowers outside the loop.
* A `<template>` with `v-if` or `v-for` is its one element or a `Fragment`.
* A `<slot />` with no name, props or fallback is `Slot("content")`. Any other `<slot>` is a `Tmpl::Let` named `render::SLOT_OUT_PREFIX` and its name, holding its props and its fallback.
* A static attribute is a literal. A valueless one is `true`.
* A static `class` beside `:class` merges into `[dynamic, static]`. `:class` alone becomes a one-element array.
* A static `style` is parsed into an object. `:style`, `v-show` and a static `style` merge into one `style` array.
* A bound boolean attribute (Vue's `isBooleanAttr`) is `true` when truthy or the empty string and null otherwise.
* Any other bound attribute is null when null and `String(value)` otherwise. Vue's four camelCase attribute spellings are mapped.
* `@event` with a written event name is a `Handler` and a `HANDLER_ATTR` field. When it does not lower it is an `UNLOWERED_ATTR` field with the line and the reason.
* A Vue handler is a setup function by name or called with arguments, an arrow, a `function` or a statement over `$event`.
* A Vue handler holds `const`s, `if` and `else`, a bare `return`, assignments and `++` or `--` on a `ref` or a `useStore` binding, calls to setup functions and calls to actions.
* `.prevent` and `.stop` are allowed. Any other modifier, a dynamic event name, an object `v-on` and a recursive call leave the reason.
* `:key`, `ref`, `v-once`, `v-memo` and `v-cloak` are dropped. `v-html` is `RAW_ATTR`. `v-text` replaces the children.
* A scoped style's `data-v-` id is a `true` attribute on every element and the component's `scope` as `ScopedStyle`, `slotted` when a rule is `:slotted()`.
* `:[name]="value"` is `Entry::Computed` on an element, a `.vue` child's props and a `<slot>`'s props.
* `v-bind` of an object on an element is an `Entry::Spread`.
* Template residue: a slot named by an expression, `.prop`, `:is`, `value` on a `<textarea>`, `v-slot`, `v-pre`, a custom directive and a component tag other than a `.vue` child.
* Template residue: a `<template>` with other directives, `v-else` with no `v-if`, a `v-for` alias that is a pattern or a third alias and a node type passed through unread.
* `v-model` on anything but an input, a textarea, a select or a `.vue` child is residue. An input whose `type` is bound is residue.
* `v-model` writes `value` on an input, `checked` on a checkbox (`Builtin::Checked`) and on a radio (`Builtin::LooseMatch`), a textarea's content and `selected` on each `<option>` with a value.
* `defineModel()` and `defineModel("name", { default })` bind a ref reading `$props.modelValue` or `$props.name`.
* Server anchors are `Tmpl::Marker` nodes: `<!--[-->` and `<!--]-->` around a multi-node root, every `v-for` list, each list item and branch that is not one element and every `<slot>`.
* `<!---->` is the branch of a `v-if` chain that takes none.
* `<Teleport to="body">` writes `<!--teleport start--><!--teleport end-->` in place and its children in a `Tmpl::Place` for `body`, between `<!--teleport start anchor-->` and `<!--teleport anchor-->`.
* `<Teleport to="#name">` targets the place a `<Place name="name">` declares. An element of the component with `id` written as `name` gets a `Tmpl::Outlet { hydrated: true }`.
* `<Teleport disabled>` keeps the children between the in-place comments.
* A `to` that is neither `body` nor `#name` is residue. `defer` is residue. So is `:defer` bound to anything but `false`.
* `provide(key, value)` wraps the template in a context `Tmpl::Let`. `inject(key, default)` is `Coalesce(Expr::Context(id), default)`.
* A string context key is `vue:<key>`. A key bound to a name is `vue:<file>#<name>` of its declaration. An `inject` with a default factory is residue.
* A tag naming a `.vue` file the script imports (`<Card>`, `<card-item>` as `CardItem`) is a `Tmpl::Component`.
* A `.vue` child's static and bound attributes are props by camelCase name. `v-bind` of an object is a spread. Its children are its default slot.
* An attribute the child does not declare as a prop is residue. So is a child that does not lower.
* A scoped parent adds `render::PARENT_SCOPE_PROP` with its scope attribute. A child in a slot's content takes the slot owner's scope. Under `:slotted()` every content element carries `<id>-s`.
* An `@event` arrow on a `.vue` child passes as the prop `on<Event>`. A ref or `data` entry it writes is an `Expr::Cell`.
* In the child, `emit("event", ...args)` through `defineEmits` and `this.$emit(...)` in `created` call that prop. A `created` holding anything else is residue.
* On a `.vue` child, each `<template #name="params">` is a `Tmpl::Let` named `render::SLOT_CONTENT_PREFIX` and the slot, with the params bound to `render::SLOT_PROPS`. The slot's fallback is written when the caller gave none or the `v-if` fails.

### Svelte Files

A described `.svelte` module lowers to a `Component` with `owner` `Owner::Svelte`. The export is `default`. Runes mode and Svelte 4 legacy mode both lower, a file at a time.

* `$state`, `$derived`, `$derived.by` and `$props` bind as `let`s. So do Svelte 4's `export let` and `$:`.
* `$props.id()` counts on a render cell, one per page render.
* `useStore(key, initial)` reads the store.
* `$effect`, `$effect.pre`, `$effect.root`, `$inspect`, `onMount`, `onDestroy`, `beforeUpdate`, `afterUpdate` and `tick` are passed over.
* A class whose fields, constructor writes and one-`return` getters lower is read. A class that extends another is residue.
* Markers follow Svelte's server visitors at v5.57.2: `<!--[-->` and `<!--]-->` around blocks, `<!--[!-->` for an else branch, `<!---->` for an empty anchor and `{@html}` between two of them.
* `{#if}`, `{#each}` with index, key and `{:else}`, `{#key}`, `{#await}`, `{#snippet}`, `{@render}` of a prop or a declared snippet, `{@const}`, `{@html}` and `<slot>` with `$$slots` lower.
* `<svelte:element this>`, `<svelte:component this>`, `<svelte:self>`, `<svelte:fragment>` and `<svelte:boundary>` render.
* `<svelte:window>`, `<svelte:body>` and `<svelte:document>` write nothing.
* `<svelte:head>` and `<title>` are residue. Head content goes in a loader's `meta` export.
* Any other statement or node is residue naming it.

### Transformed Files

* A file in another language a `Transform` turns into TSX, `.mdx` and `.md` among them, lowers the module written in its place.
* Positions in it are reported through its `Origin`.

### Schema Types

* An interface becomes `TypeDef::Record`. `extends`, type parameters, methods, index signatures and computed keys are residue.
* `string` is `Str`, `number` is `F64`, `bigint` is `I64` and `boolean` is `Bool`. `null` and `undefined` are `Null`.
* `T[]` and `Array<T>` are `List`. `Record<string, T>` is `Map`.
* `Uint8Array` is `Bytes`. The other typed arrays are `Array(kind)`.
* A bare name is `Named`.
* `T | null`, `T | undefined` and a `?` field are `Optional(T)`.
* A union of two real types, an inline object type, a literal outside a named union and a generic reference are residue.
* A type alias is a union of string literals and becomes `TypeDef::Union` of unit variants.

## 12. Test Files

`snapfire_fsr_lower::testing`: reads a `*.test.ts` into the cases `fsr test` replays through the interpreter.

### lower_tests

* `pub fn lower_tests(file: &str, source: &str) -> Result<TestFile, LowerError>`: the test file at `file`, relative to the app.
* The file holds imports, `const` fixtures, mock functions, `let` bindings, hooks, `describe` blocks and tests. Anything else is a `LowerError::Residue` naming its line.
* A test imports the loader, its `meta` or `store`, an action, a route handler or the middleware by path or alias. Helpers come from `@snapfire/fsr/testing`.
* An `only` anywhere in the file gives every case it does not cover `Mode::Skip`.

### TestFile

* `pub struct TestFile { pub file: String, pub blocks: Vec<Block>, pub tests: Vec<TestCase> }`. `#[non_exhaustive]`.
* `chain(&self, block: usize) -> Vec<usize>`: the blocks around `block`, outermost first. Block 0 is the file.
* `full_name(&self, case: &TestCase) -> String`: the case's name under its blocks' names, joined with ` > `.

### Block

* `pub struct Block { pub name: String, pub parent: Option<usize>, pub each: Option<Each>, pub before_all: Vec<(usize, Step)>, pub after_all: Vec<(usize, Step)>, pub before_each: Vec<(usize, Step)>, pub after_each: Vec<(usize, Step)> }`. `Default`, `#[non_exhaustive]`.
* A `describe` or the file itself, each hook as its steps with their lines.
* A mock function declared at the top of a file or a block is a `Step::Fn` in its `before_each`.

### TestCase

* `pub struct TestCase { pub name: String, pub line: usize, pub block: usize, pub mode: Mode, pub each: Option<Each>, pub steps: Vec<(usize, Step)> }`. `#[non_exhaustive]`.
* Under an `each`, `name` is the pattern each row fills.

### Mode

* `pub enum Mode { Run, Skip, Todo }`

### Each

* `pub struct Each { pub table: Expr, pub params: Vec<Binding> }`. `#[non_exhaustive]`.
* `table` is evaluated when the file runs. An array row is spread over `params`. Any other row binds the one parameter.
* A template table lowers to an array of objects keyed by its headings.

### Step

* `pub enum Step { Mock { name: String, mock: Mock }, Run { binding: Option<Binding>, target: Target, ctx: String, input: Option<Expr> }, Let { name: String, value: Expr }, Assert(Assertion), Fn { name: String, answer: Option<Answer> }, Answer { name: String, answer: Answer, once: bool }, Clear { name: String, reset: bool } }`
* `Mock` is a `ctx(...)` bound or assigned to a name.
* `Run`'s `input` is what a `meta({ data })` names, run in place of the ctx's own.
* `Let` is a local `const` or an assignment to a `let`.
* `Fn` is `fn(impl)`, `vi.fn(impl)` or `jest.fn(impl)`.
* `Answer` is `mockReturnValue`, `mockResolvedValue`, `mockRejectedValue`, `mockImplementation` and their `Once` forms.
* `Clear` is `mockClear` or `mockReset`. An empty name is every mock function, as `vi.clearAllMocks()` asks.

### Assertion

* `pub enum Assertion { Ok(Expr), Equal(Expr, Expr), Rejects { target: Target, ctx: String, input: Option<Expr>, kind: Option<String> }, Expect { subject: Subject, not: bool, matcher: Matcher, message: Option<String> } }`
* `Ok`, `Equal` and `Rejects` are `assert.ok`, `assert.equal` and `assert.rejects`.
* `assert.match(actual, pattern, message?)` lowers to an `Expect` with `Matcher::Match`.
* `Expect` is `expect(subject, message?)` with `.not` and a matcher. The message is a string literal.

### Subject

* `pub enum Subject { Value(Expr), Settled { target: Target, ctx: String, input: Option<Expr>, rejects: bool }, Mock(String) }`
* A subject under `.resolves` or `.rejects` is `Settled`, a run settled when the step runs. Its failure reads as `{ kind, message }`.
* A call matcher reads a mock function by name as `Mock`.

### Matcher

* `pub enum Matcher { Be(Expr), Equal(Expr), StrictEqual(Expr), Truthy, Falsy, Null, Undefined, Defined, NaN, GreaterThan(Expr), GreaterThanOrEqual(Expr), LessThan(Expr), LessThanOrEqual(Expr), CloseTo(Expr, Option<Expr>), Contain(Expr), ContainEqual(Expr), Length(Expr), Property(Expr, Option<Expr>), Match(Pattern), MatchObject(Expr), TypeOf(Expr), OneOf(Expr), Throw(Option<Pattern>), Called, CalledOnce, CalledTimes(Expr), CalledWith(Vec<Expr>), CalledExactlyOnceWith(Vec<Expr>), LastCalledWith(Vec<Expr>), NthCalledWith(Expr, Vec<Expr>), Returned, ReturnedTimes(Expr), ReturnedWith(Expr), LastReturnedWith(Expr) }`
* `name(&self) -> &'static str`: the name a test writes, `toBe` for `Be`.
* `takes_expected(&self) -> bool`
* `reads_calls(&self) -> bool`
* A matcher's older names lower to the same variant: `toBeCalled` is `Called` and `toThrowError` is `Throw`.
* `expect.any`, `anything`, `objectContaining`, `arrayContaining`, `stringContaining`, `stringMatching` and `closeTo`, with their `expect.not` forms, lower to objects whose `EXPECT_MARK` field names the helper.
* `pub const EXPECT_MARK: &str = "$sf.expect"`
* A regular expression literal is taken only by `toMatch`, `toThrow` and `stringMatching`.
* A matcher for markup is refused.

### Pattern

* `pub enum Pattern { Text(Expr), Regex { source: String, flags: String } }`

### Mock

* `pub struct Mock { pub session: Vec<(String, Expr)>, pub services: Vec<(String, String, Expr)>, pub mock_fns: Vec<(String, String, String)>, pub input: Option<Expr>, pub params: Vec<(String, Expr)>, pub query: Vec<(String, Expr)>, pub identity: Option<Expr>, pub locale: Option<Expr>, pub locales: Option<Expr>, pub path: Option<Expr>, pub host: Option<Expr>, pub origin: Option<Expr>, pub config: Vec<(String, Expr)> }`. `Debug`, `Clone`, `PartialEq`, `Default`, `#[non_exhaustive]`.
* The `ctx({ ... })` literal, each part an expression the runner evaluates.
* `services` is `(service, method, lambda)`. A value that is not a function is a lambda of no parameters.
* `mock_fns` is `(service, method, name)` for a method a mock function or a `let` answers.

### Answer

* `pub enum Answer { Returns(Expr), Calls(Expr), Fails(Expr) }`
* `Returns` answers a value whatever the arguments. `Calls` applies a lambda to them.
* `Fails` fails the call with the kind its value names when that is `{ kind, message }`.

### Binding

* `pub enum Binding { Name(String), Fields(Vec<(String, String)>) }`. `Debug`, `Clone`, `PartialEq`, `Eq`.

### Target

* `pub enum Target { Loader { file: String }, Meta { file: String }, Store { file: String }, Paths { file: String }, Action { file: String, export: String }, Handler { file: String, export: String }, Middleware { file: String } }`. Each path is relative to the app.
* `Meta` and `Store` run over the data rather than a ctx.
* `Paths` comes from `paths` imported from a `page.loader`. A `paths()` call with no argument runs against the ctx bound above it.

## 13. Error Handling

### LowerError

`#[non_exhaustive]`. `Debug`, `Clone`, `PartialEq`, `Eq`, `Error`.

* `Parse { file: String, message: String }`: displays as `{file}: {message}`. `message` holds the parser's line, column and message.
* `MissingExport { file: String, export: String }`: displays as ``{file}: no exported `{export}` ``.
* `Residue(Residue)`: displays as the residue.
* `Reach(Residue)`: a `body` extension on a component's render path. Displays as the residue followed by `; it runs on the server only, and a component's render path runs in the browser too`.
* `Extension(Residue)`: an export under `ext/` that does not lower or a `native` declaration the build cannot read. Displays as the residue followed by `; every export under ext/ is an extension and must lower`.
* `Retired(Residue)`: a spelling FSR no longer takes, with what to write instead in the message. Displays as the residue.
* `ForeignChildren(Residue)`: an `Owner::React` component gives children to a component another framework mounts. Displays as the residue.

### Residue

* `pub struct Residue { pub file: String, pub line: usize, pub column: usize, pub message: String, pub hint: Option<String>, pub via: Vec<Placement> }`. `Debug`, `Clone`, `PartialEq`, `Eq`, `Error`, `#[non_exhaustive]`.
* `Display` is `{file}:{line}:{column}: {message}`, then an indented `reached through <chain>` when `via` is not empty, then the indented `hint` when there is one.
* `line` and `column` are one-based and point at the construct, not at the statement that contains it.
* `hint` names the rewrite that does the same thing in the IR.
* `via` is the chain of placements from the module the build asked for down to this residue's file, outermost first. Empty when they are the same file.
* `placed_at(self, at: Placement) -> Residue`: inserts `at` at the front of `via`.
* `chain(&self) -> String`: `via` as one line, outermost placement first, joined with `, `.

### Placement

* `pub struct Placement { pub file: String, pub line: usize, pub column: usize, pub tag: String }`. `Debug`, `Clone`, `PartialEq`, `Eq`, `#[non_exhaustive]`.
* The file holding a `<Tag />`, the one-based position of the tag and the name as written.
* `Display` is `<{tag}> {file}:{line}:{column}`.
