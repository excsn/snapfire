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

* `pub struct App { pub matcher: MatchitMatcher, pub resolver: TableResolver, pub runtime: Arc<Runtime>, pub services: Arc<Services>, pub actions: ActionRegistry, pub report: Report }`
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
* `AppBuilder::evaluator<P>(self, predicate: P, evaluator: Arc<dyn Evaluator>) -> Self` where `P: Fn(&ModuleId) -> bool + Send + Sync + 'static`
* `AppBuilder::services(self, services: Arc<Services>) -> Self`: default is an empty registry.
* `AppBuilder::contract(self, contract: Contract) -> Self`: required when any lowered action names an input type.
* `AppBuilder::cache(self, cache: Arc<dyn NodeCache>) -> Self`
* `AppBuilder::route(self, pattern, plan: impl IntoPlan) -> Self`
* `AppBuilder::route_override(self, pattern, plan: impl IntoPlan) -> Self`
* `AppBuilder::build(self) -> Result<App, BindError>`: binds every lowered source and action not overridden, the lowered components under the IR evaluator, checks every override names something, every named source and declared action is answered and every pattern is one the matcher accepts, then assembles the runtime and the report. A lowered action with an input type is wrapped so the value is checked against the contract before its body runs, failing as `Invalid`.

## 2. Routes and Plans

### Routes

Routes from the plan file, from Rust or both. A pattern claimed twice is refused rather than shadowed.

* `pub struct Routes`, `Default`.
* `Routes::new() -> Self`
* `Routes::from_manifest(source: &str) -> Result<Self, BindError>`: every route in the file, owned by `PlanFile`.
* `Routes::add(self, pattern, plan: impl IntoPlan) -> Self`: owned by `Rust`; a plan that fails to convert is kept as the error `build` returns.
* `Routes::replace(self, pattern, plan: impl IntoPlan) -> Self`: owned by `RustOverride`, replacing the entry with that pattern.
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

* `pub struct Report { pub routes: Vec<(String, Owner)>, pub sources: Vec<(String, Owner)>, pub actions: Vec<(String, Owner)>, pub components: Vec<(String, Owner)> }`
* `routes` sorted by pattern; the rest in binding order. `Display` prints four labelled columns, `routes`, `sources`, `actions` and `rendered`.

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
