//! Islands whose markup a template renders and whose handlers are Rust.
//!
//! A lowered component carries its own state names and handler bodies, so the
//! host runs a step entirely from the plan. A template declares neither: it is
//! markup and nothing else. What stands in for them is this registry, one
//! named handler per module and the state the placement gave the island.

use std::future::Future;
use std::sync::Arc;

use indexmap::IndexMap;
use snapfire_fsr_core::Value;

use futures_util::future::BoxFuture;

use crate::actions::{ActionError, FailureKind};
use crate::ctx::RequestCtx;

/// The props key a server-mode island's state rides under, written by the
/// placement and read by the browser.
pub const STATE_PROP: &str = "$s";

/// The props key a keyed placement's region rides under.
pub const REGION_PROP: &str = "$k";

/// What a template renders an island from: its props without the two the
/// runtime rides on them, plus the state under `state`. The first paint and
/// every later step build it the same way, which is what makes them agree.
pub fn island_data(props: &snapfire_fsr_core::ValueMap, state: &Value) -> snapfire_fsr_core::ValueMap {
  let mut data = snapfire_fsr_core::ValueMap::default();
  for (key, value) in props {
    if key != STATE_PROP && key != REGION_PROP {
      data.insert(key.clone(), value.clone());
    }
  }
  data.insert("state".to_owned(), state.clone());
  data
}

/// What a handler is given: the props the placement passed, the state the
/// browser holds and the event that fired. All three arrive from the browser,
/// so a handler checks them the way an action checks its own input.
#[derive(Debug, Clone)]
pub struct IslandEvent {
  pub props: Value,
  pub state: Value,
  pub event: Value,
}

pub trait IslandHandler: Send + Sync {
  fn call(&self, ctx: RequestCtx, event: IslandEvent) -> BoxFuture<'static, Result<Value, ActionError>>;
}

struct FnIsland<F>(F);

impl<F, Fut> IslandHandler for FnIsland<F>
where
  F: Fn(RequestCtx, IslandEvent) -> Fut + Send + Sync,
  Fut: Future<Output = Result<Value, ActionError>> + Send + 'static,
{
  fn call(&self, ctx: RequestCtx, event: IslandEvent) -> BoxFuture<'static, Result<Value, ActionError>> {
    Box::pin((self.0)(ctx, event))
  }
}

/// The handlers of every template-rendered island, by module and name. A
/// handler answers with the island's next state; the host renders the module
/// again from it.
#[derive(Default)]
pub struct IslandRegistry {
  handlers: IndexMap<(String, String), Arc<dyn IslandHandler>>,
}

impl IslandRegistry {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn insert(&mut self, module: impl Into<String>, name: impl Into<String>, handler: Arc<dyn IslandHandler>) {
    self.handlers.insert((module.into(), name.into()), handler);
  }

  pub fn insert_fn<F, Fut>(&mut self, module: impl Into<String>, name: impl Into<String>, f: F)
  where
    F: Fn(RequestCtx, IslandEvent) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Value, ActionError>> + Send + 'static,
  {
    self.insert(module, name, Arc::new(FnIsland(f)));
  }

  /// Whether any handler is registered for the module, which is what makes it
  /// a template island rather than an unknown one.
  pub fn holds(&self, module: &str) -> bool {
    self.handlers.keys().any(|(m, _)| m == module)
  }

  /// Every module with a handler, in registration order, without duplicates.
  pub fn modules(&self) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (module, _) in self.handlers.keys() {
      if !out.contains(module) {
        out.push(module.clone());
      }
    }
    out
  }

  /// One module's handler names, in registration order.
  pub fn names(&self, module: &str) -> Vec<String> {
    self.handlers.keys().filter(|(m, _)| m == module).map(|(_, name)| name.clone()).collect()
  }

  pub fn dispatch(&self, module: &str, name: &str, ctx: RequestCtx, event: IslandEvent) -> BoxFuture<'static, Result<Value, ActionError>> {
    tracing::debug!(target: "fsr::island", module, name, "dispatch");
    match self.handlers.get(&(module.to_owned(), name.to_owned())) {
      Some(handler) => handler.call(ctx, event),
      None => {
        let message = format!("`{module}` has no handler `{name}`");
        Box::pin(async move { Err(ActionError::new(FailureKind::NotFound, message)) })
      }
    }
  }
}
