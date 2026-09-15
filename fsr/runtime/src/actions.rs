use std::future::Future;
use std::sync::Arc;

use futures_util::future::BoxFuture;
use indexmap::IndexMap;
use snapfire_fsr_core::Value;

use crate::ctx::RequestCtx;

pub use snapfire_fsr_core::ext::FailureKind;

#[derive(Debug, Clone, thiserror::Error)]
#[error("action failed ({}): {message}", kind.as_str())]
pub struct ActionError {
  pub kind: FailureKind,
  pub message: String,
}

impl ActionError {
  pub fn new(kind: FailureKind, message: impl Into<String>) -> Self {
    Self {
      kind,
      message: message.into(),
    }
  }
}

pub trait ActionHandler: Send + Sync {
  fn call(&self, ctx: RequestCtx, input: Value) -> BoxFuture<'static, Result<Value, ActionError>>;
}

struct FnHandler<F>(F);

impl<F, Fut> ActionHandler for FnHandler<F>
where
  F: Fn(RequestCtx, Value) -> Fut + Send + Sync,
  Fut: Future<Output = Result<Value, ActionError>> + Send + 'static,
{
  fn call(&self, ctx: RequestCtx, input: Value) -> BoxFuture<'static, Result<Value, ActionError>> {
    Box::pin((self.0)(ctx, input))
  }
}

/// Stable action ids to handlers. An action is a service call whose
/// implementation happens to be local, see SERVICES.md.
#[derive(Default)]
pub struct ActionRegistry {
  actions: IndexMap<String, Arc<dyn ActionHandler>>,
}

impl ActionRegistry {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn insert(&mut self, id: impl Into<String>, handler: Arc<dyn ActionHandler>) {
    self.actions.insert(id.into(), handler);
  }

  pub fn insert_fn<F, Fut>(&mut self, id: impl Into<String>, f: F)
  where
    F: Fn(RequestCtx, Value) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Value, ActionError>> + Send + 'static,
  {
    self.insert(id, Arc::new(FnHandler(f)));
  }

  /// Every registered id, in registration order. What a host reports at boot.
  pub fn ids(&self) -> Vec<String> {
    self.actions.keys().cloned().collect()
  }

  pub fn dispatch(&self, id: &str, ctx: RequestCtx, input: Value) -> BoxFuture<'static, Result<Value, ActionError>> {
    tracing::debug!(target: "fsr::action", id, "dispatch");
    match self.actions.get(id) {
      Some(handler) => handler.call(ctx, input),
      None => {
        let id = id.to_owned();
        Box::pin(async move { Err(ActionError::new(FailureKind::NotFound, format!("no action `{id}`"))) })
      }
    }
  }
}
