use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use futures_util::future::BoxFuture;
use snapfire_fsr_core::Value;
use snapfire_fsr_runtime::ServiceError;

use crate::call::Call;
use crate::transport::Transport;

/// One step of the outbound path: an ordered list of functions, tower-style,
/// not a workflow engine. See SERVICES.md section 3b.
pub trait Interceptor: Send + Sync {
  fn call(&self, call: Call, next: Next) -> BoxFuture<'static, Result<Value, ServiceError>>;
}

pub(crate) struct Chain {
  pub(crate) interceptors: Vec<Arc<dyn Interceptor>>,
  pub(crate) transport: Arc<dyn Transport>,
}

/// The rest of the chain. Calling `run` continues; not calling it short
/// circuits, which is how a cache or a circuit breaker would sit here.
pub struct Next {
  chain: Arc<Chain>,
  index: usize,
}

impl Next {
  pub(crate) fn start(chain: Arc<Chain>) -> Self {
    Self { chain, index: 0 }
  }

  pub fn run(self, call: Call) -> BoxFuture<'static, Result<Value, ServiceError>> {
    match self.chain.interceptors.get(self.index) {
      Some(interceptor) => {
        let next = Next { chain: self.chain.clone(), index: self.index + 1 };
        interceptor.call(call, next)
      }
      None => self.chain.transport.call(call),
    }
  }
}

/// Propagates who the request is onto every outbound call.
pub struct IdentityInterceptor {
  key: String,
}

impl IdentityInterceptor {
  pub fn new() -> Self {
    Self { key: "x-sf-subject".to_owned() }
  }

  pub fn key(mut self, key: impl Into<String>) -> Self {
    self.key = key.into();
    self
  }
}

impl Default for IdentityInterceptor {
  fn default() -> Self {
    Self::new()
  }
}

impl Interceptor for IdentityInterceptor {
  fn call(&self, mut call: Call, next: Next) -> BoxFuture<'static, Result<Value, ServiceError>> {
    if let Some(identity) = call.identity.clone() {
      call.set_metadata(self.key.clone(), identity.subject);
    }
    next.run(call)
  }
}

/// Reads one credential out of custody and attaches it. This is the step that
/// makes "application code never sees a token" structural.
pub struct CredentialInterceptor {
  credential: String,
  header: String,
  scheme: String,
}

impl CredentialInterceptor {
  pub fn bearer(credential: impl Into<String>) -> Self {
    Self {
      credential: credential.into(),
      header: "authorization".to_owned(),
      scheme: "Bearer ".to_owned(),
    }
  }

  pub fn header(mut self, header: impl Into<String>) -> Self {
    self.header = header.into();
    self
  }

  pub fn scheme(mut self, scheme: impl Into<String>) -> Self {
    self.scheme = scheme.into();
    self
  }
}

impl Interceptor for CredentialInterceptor {
  fn call(&self, mut call: Call, next: Next) -> BoxFuture<'static, Result<Value, ServiceError>> {
    if let Some(Value::Str(token)) = call.credentials.get(&self.credential) {
      call.set_metadata(self.header.clone(), format!("{}{token}", self.scheme));
    }
    next.run(call)
  }
}

/// A request id that survives the whole fanout, minted once per request and
/// reused by every call that carries it in.
pub struct TraceInterceptor {
  key: String,
  counter: AtomicU64,
}

impl TraceInterceptor {
  pub fn new() -> Self {
    Self { key: "x-sf-request-id".to_owned(), counter: AtomicU64::new(1) }
  }

  pub fn key(mut self, key: impl Into<String>) -> Self {
    self.key = key.into();
    self
  }
}

impl Default for TraceInterceptor {
  fn default() -> Self {
    Self::new()
  }
}

impl Interceptor for TraceInterceptor {
  fn call(&self, mut call: Call, next: Next) -> BoxFuture<'static, Result<Value, ServiceError>> {
    if call.metadata_str(&self.key).is_none() {
      let id = self.counter.fetch_add(1, Ordering::Relaxed);
      call.set_metadata(self.key.clone(), format!("{id:016x}"));
    }
    tracing::debug!(
      target: "fsr::service",
      service = call.service.as_str(),
      method = call.method.as_str(),
      request_id = call.metadata_str(&self.key).unwrap_or_default(),
      "call"
    );
    next.run(call)
  }
}
