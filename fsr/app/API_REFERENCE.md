# API Reference: snapfire_fsr

The binding rule: a plan file plus Rust registrations become an `App`, or a refusal naming what is unanswered or claimed twice.

## Contents

* [1. Building](#1-building)
  * [App](#app)
  * [AppBuilder](#appbuilder)
* [2. Routes and Plans](#2-routes-and-plans)
  * [Routes](#routes)
  * [Plan](#plan)
  * [IntoPlan](#intoplan)
* [3. The Report](#3-the-report)
  * [Owner](#owner)
  * [Report](#report)
* [4. Error Handling](#4-error-handling)
  * [BindError](#binderror)

## 1. Building

### App

Everything a request needs, plus what was bound to produce it.

* `pub struct App { pub matcher: MatchitMatcher, pub resolver: TableResolver, pub handlers: Handlers, pub middleware: Option<Arc<dyn ActionHandler>>, pub not_found: Option<PlanNode>, pub prerenderable: Vec<String>, pub runtime: Arc<Runtime>, pub services: Arc<Services>, pub actions: ActionRegistry, pub report: Report }`. `not_found` is the tree a host renders, with status 404, for a path the matcher does not match. `middleware` runs before every request with `{ method, path }` as its input; what its value means is the host's reading, `snapfire_fsr_host::Preflight`. `prerenderable` lists the patterns with no parameter whose every source is lowered and reads nothing of the request, in route order.

### Handlers

The route handlers a host dispatches before matching a page.

* `Handlers::match_request(&self, method: &str, path: &str) -> Option<HandlerMatch>`: the handler id and the pattern's parameters; the method is matched case-insensitively.
* `Handlers::dispatch(&self, id: &str, ctx: RequestCtx, input: Value) -> BoxFuture<'static, Result<Value, ActionError>>`
* `Handlers::ids(&self) -> Vec<String>`; `Handlers::is_empty(&self) -> bool`.
* `App::invalidate(&self, plan_key: &str) -> usize` (async): drops every cached subtree under the plan `cache_key` and says how many went.
* `App::builder(routes: Routes) -> AppBuilder`: from routes alone, no plan file.
* `App::from_manifest(manifest: &str) -> Result<AppBuilder, BindError>`: the plan file's text; its routes, its lowered sources, actions and components and its declared actions are remembered for `build`.

### AppBuilder

Every method takes and returns the builder. Registration order is the evaluators' lookup order; for a name, the last claim wins and a plain claim on a lowered name is refused at `build`.

* `AppBuilder::source<F, Fut>(self, name, f: F) -> Self` where `F: Fn(RequestCtx) -> Fut + Send + Sync + 'static`, `Fut: Future<Output = Result<Data, LoadError>> + Send + 'static`: claims `name` as `Rust`.
* `AppBuilder::source_override<F, Fut>(self, name, f: F) -> Self`: claims `name` as `RustOverride`; `build` refuses when the plan names no such source.
* `AppBuilder::source_impl(self, name, source: Arc<dyn DataSource>) -> Self`
* `AppBuilder::action<F, Fut>(self, id, f: F) -> Self` where `F: Fn(RequestCtx, Value) -> Fut + Send + Sync + 'static`, `Fut: Future<Output = Result<Value, ActionError>> + Send + 'static`
* `AppBuilder::action_override<F, Fut>(self, id, f: F) -> Self`: `build` refuses when the plan lowers no such action.
* `AppBuilder::action_impl(self, id, handler: Arc<dyn ActionHandler>) -> Self`
* `AppBuilder::handler<F, Fut>(self, method, pattern, f: F) -> Self` with the same bounds as `action`: a Rust handler for `METHOD pattern`, reported by that name; `build` refuses it as `HandlerClaimed` when the plan lowers the same pair.
* `AppBuilder::handler_override<F, Fut>(self, method, pattern, f: F) -> Self`: replaces the lowered handler for the pair; `HandlerOverridesNothing` when there is none.
* `AppBuilder::handler_impl(self, method, pattern, handler: Arc<dyn ActionHandler>) -> Self`
* `AppBuilder::middleware<F, Fut>(self, f: F) -> Self` with the same bounds as `action`: Rust middleware; `build` refuses it as `MiddlewareClaimed` when the plan lowers one.
* `AppBuilder::middleware_override<F, Fut>(self, f: F) -> Self`: replaces the lowered middleware; `MiddlewareOverridesNothing` when there is none.
* `AppBuilder::evaluator<P>(self, predicate: P, evaluator: Arc<dyn Evaluator>) -> Self` where `P: Fn(&ModuleId) -> bool + Send + Sync + 'static`
* `AppBuilder::services(self, services: Arc<Services>) -> Self`: default is an empty registry.
* `AppBuilder::contract(self, contract: Contract) -> Self`: required when any lowered action names an input type.
* `AppBuilder::cache(self, cache: Arc<dyn NodeCache>) -> Self`
* `AppBuilder::route(self, pattern, plan: impl IntoPlan) -> Self`
* `AppBuilder::route_override(self, pattern, plan: impl IntoPlan) -> Self`
* `AppBuilder::not_found(self, plan: impl IntoPlan) -> Self`: `Routes::not_found` on the builder's routes.
* `AppBuilder::build(self) -> Result<App, BindError>`: binds every lowered source, action and handler not overridden, checks a handler row with no body is answered in Rust (`UnboundHandler`), the lowered components under the IR evaluator, checks every override names something, every named source and declared action is answered and every pattern is one the matcher accepts, then assembles the runtime and the report. A lowered action with an input type is wrapped so the value is checked against the contract before its body runs, failing as `Invalid`.

## 2. Routes and Plans

### Routes

Routes from the plan file, from Rust or both. A pattern claimed twice is refused rather than shadowed.

* `pub struct Routes`, `Default`.
* `Routes::new() -> Self`
* `Routes::from_manifest(source: &str) -> Result<Self, BindError>`: every route in the file, owned by `PlanFile`, plus the file's not-found tree when it has one.
* `Routes::add(self, pattern, plan: impl IntoPlan) -> Self`: owned by `Rust`; a plan that fails to convert is kept as the error `build` returns.
* `Routes::replace(self, pattern, plan: impl IntoPlan) -> Self`: owned by `RustOverride`, replacing the entry with that pattern.
* `Routes::not_found(self, plan: impl IntoPlan) -> Self`: the tree for a path no route matches, replacing the plan file's; its sources count as declared.
* `Routes::has_not_found(&self) -> bool`
* `Routes::patterns(&self) -> Vec<&str>`
* `Routes::build(self) -> Result<(MatchitMatcher, TableResolver), BindError>`: entry ids are positions in file order followed by Rust order.

### Plan

A route's plan written the way it reads; node ids are assigned in tree order when converted.

* `Plan::of(module) -> Self`
* `Plan::source(self, name) -> Self`
* `Plan::deferred(self) -> Self`: streams instead of blocking the first response; pair with `fallback`.
* `Plan::fallback(self, module) -> Self`
* `Plan::error(self, module) -> Self`: rendered in place of the node when its loader fails.
* `Plan::cache_key(self, key) -> Self`
* `Plan::slot(self, name, child: Plan) -> Self`
* Every module is `path#export`; conversion fails with `BindError::Module` otherwise.

### IntoPlan

* `pub trait IntoPlan { fn into_plan(self) -> Result<PlanNode, BindError>; }`
* Implemented for `Plan` and for `PlanNode`, which passes through.

## 3. The Report

### Owner

`#[derive(Debug, Clone, Copy, PartialEq, Eq)]`

* `PlanFile`, `Lowered`, `Rust`, `RustOverride`.
* `Owner::as_str(&self) -> &'static str`: `plan file`, `lowered`, `rust`, `rust override`.

### Report

`#[derive(Debug, Clone, Default, PartialEq, Eq)]`, `Display`.

* `pub struct Report { pub routes: Vec<(String, Owner)>, pub sources: Vec<(String, Owner)>, pub actions: Vec<(String, Owner)>, pub handlers: Vec<(String, Owner)>, pub middleware: Option<Owner>, pub prerenderable: Vec<String>, pub components: Vec<(String, Owner)> }`
* `routes` and `handlers` sorted, the handler name being `METHOD pattern`; the rest in binding order. `Display` prints labelled columns, `routes`, `sources`, `actions`, `handlers`, `middleware` when there is one and `rendered`.

## 4. Error Handling

### BindError

`#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]`

* `Plan(PlanError)`: the plan file did not read.
* `Claimed(String)`: a source or a route pattern claimed by the plan file and by Rust without an override.
* `ActionClaimed(String)`: an action lowered by the file and bound in Rust without an override.
* `ActionOverridesNothing { id }`, `OverridesNothing { name }`: an override the file has nothing for.
* `Pattern { pattern, message }`: the matcher refused the pattern.
* `Unbound { name }`: a source the plan names and nothing answers.
* `UnboundAction { id }`: an action the plan declares and nothing answers.
* `Module { module }`: not `path#export`.
* `NoContract { id, input }`: a lowered action names an input type and no contract was passed.
* `UnknownInput { id, input }`: the contract does not define the type.
* `HandlerClaimed(String)`: a handler lowered by the file and bound in Rust without an override; the string is `METHOD pattern`.
* `HandlerOverridesNothing(String)`: a handler override the file has nothing for.
* `UnboundHandler(String)`: a handler row with no body that no Rust handler answers.
* `MiddlewareClaimed`: middleware lowered by the file and bound in Rust without an override.
* `MiddlewareOverridesNothing`
