use std::sync::Arc;

use futures_util::future::BoxFuture;
use snapfire_fsr_core::{Value, ValueMap};

use crate::actions::FailureKind;

#[derive(Debug, Clone, thiserror::Error)]
#[error("{service}.{method} failed ({}): {message}", kind.as_str())]
pub struct ServiceError {
  pub kind: FailureKind,
  pub service: String,
  pub method: String,
  pub message: String,
}

impl ServiceError {
  pub fn new(
    kind: FailureKind,
    service: impl Into<String>,
    method: impl Into<String>,
    message: impl Into<String>,
  ) -> Self {
    Self { kind, service: service.into(), method: method.into(), message: message.into() }
  }
}

/// What a loader or action may do to the service layer: name a method and pass
/// arguments. The caller is bound to the request before it reaches application
/// code, so identity and credentials are attached without being reachable.
pub trait ServiceCaller: Send + Sync {
  fn call(
    &self,
    service: &str,
    method: &str,
    args: ValueMap,
  ) -> BoxFuture<'static, Result<Value, ServiceError>>;
}

/// `ctx.services`. Empty unless the edge bound a service layer, and an unbound
/// handle fails the call rather than pretending.
#[derive(Clone, Default)]
pub struct ServiceHandle(Option<Arc<dyn ServiceCaller>>);

impl ServiceHandle {
  pub fn new(caller: Arc<dyn ServiceCaller>) -> Self {
    Self(Some(caller))
  }

  pub fn is_bound(&self) -> bool {
    self.0.is_some()
  }

  pub fn call(
    &self,
    service: &str,
    method: &str,
    args: ValueMap,
  ) -> BoxFuture<'static, Result<Value, ServiceError>> {
    match &self.0 {
      Some(caller) => caller.call(service, method, args),
      None => {
        let error = ServiceError::new(
          FailureKind::Unavailable,
          service,
          method,
          "no service layer is bound to this request",
        );
        Box::pin(async move { Err(error) })
      }
    }
  }
}
