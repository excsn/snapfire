use std::future::Future;
use std::sync::Arc;

use futures_util::future::{ready, BoxFuture};
use indexmap::IndexMap;
use parking_lot::Mutex;
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_runtime::{FailureKind, ServiceError};

use crate::call::Call;

pub trait Transport: Send + Sync {
  fn call(&self, call: Call) -> BoxFuture<'static, Result<Value, ServiceError>>;
}

pub fn unavailable(call: &Call, message: impl Into<String>) -> ServiceError {
  ServiceError::new(FailureKind::Unavailable, &call.service, &call.method, message)
}

type LocalFn = Arc<dyn Fn(Call) -> BoxFuture<'static, Result<Value, ServiceError>> + Send + Sync>;

/// Implementations that happen to be in-process, keyed `service.method`. The
/// same machinery as a remote call with the network removed.
#[derive(Default)]
pub struct LocalTransport {
  methods: IndexMap<String, LocalFn>,
}

impl LocalTransport {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn method<F, Fut>(mut self, path: impl Into<String>, f: F) -> Self
  where
    F: Fn(Call) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Value, ServiceError>> + Send + 'static,
  {
    self.methods.insert(path.into(), Arc::new(move |call| Box::pin(f(call))));
    self
  }
}

impl Transport for LocalTransport {
  fn call(&self, call: Call) -> BoxFuture<'static, Result<Value, ServiceError>> {
    let path = format!("{}.{}", call.service, call.method);
    match self.methods.get(&path) {
      Some(f) => f(call),
      None => Box::pin(ready(Err(ServiceError::new(
        FailureKind::NotFound,
        &call.service,
        &call.method,
        format!("no local implementation for `{path}`"),
      )))),
    }
  }
}

/// A transport is a block, so `mock` is a transport: the whole application runs against canned responses with no backend and no code change. It keeps nothing per call, so a host serving it holds the same memory however many requests it answers. Wrap it in [`Recording`] to see what the chain sent.
#[derive(Default)]
pub struct MockTransport {
  responses: IndexMap<String, Result<Value, ServiceError>>,
}

impl MockTransport {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn returns(mut self, path: impl Into<String>, value: Value) -> Self {
    self.responses.insert(path.into(), Ok(value));
    self
  }

  pub fn fails(mut self, path: impl Into<String>, kind: FailureKind, message: impl Into<String>) -> Self {
    let path = path.into();
    let (service, method) = path.split_once('.').unwrap_or((path.as_str(), ""));
    let error = ServiceError::new(kind, service, method, message);
    self.responses.insert(path, Err(error));
    self
  }
}

impl Transport for MockTransport {
  fn call(&self, call: Call) -> BoxFuture<'static, Result<Value, ServiceError>> {
    let path = format!("{}.{}", call.service, call.method);
    let response = match self.responses.get(&path) {
      Some(Ok(value)) => Ok(value.clone()),
      Some(Err(error)) => Err(error.clone()),
      None => Err(ServiceError::new(
        FailureKind::NotFound,
        &call.service,
        &call.method,
        format!("no mock response for `{path}`"),
      )),
    };
    Box::pin(ready(response))
  }
}

/// One call as it reached the transport: `service.method`, its arguments and its metadata.
pub type Recorded = (String, ValueMap, ValueMap);

/// Any transport, with every call that reaches it kept in order: what the chain in front produced, after its interceptors ran. For tests, since it keeps every call it sees.
pub struct Recording {
  inner: Arc<dyn Transport>,
  calls: Mutex<Vec<Recorded>>,
}

impl Recording {
  pub fn over(inner: impl Transport + 'static) -> Self {
    Self { inner: Arc::new(inner), calls: Mutex::new(Vec::new()) }
  }

  pub fn calls(&self) -> Vec<Recorded> {
    self.calls.lock().clone()
  }

  /// The metadata value `key` the last call carried, when it is text.
  pub fn last_metadata(&self, key: &str) -> Option<String> {
    match self.calls.lock().last()?.2.get(key) {
      Some(Value::Str(s)) => Some(s.to_string()),
      _ => None,
    }
  }
}

impl Transport for Recording {
  fn call(&self, call: Call) -> BoxFuture<'static, Result<Value, ServiceError>> {
    self.calls.lock().push((format!("{}.{}", call.service, call.method), call.args.clone(), call.metadata.clone()));
    self.inner.call(call)
  }
}
