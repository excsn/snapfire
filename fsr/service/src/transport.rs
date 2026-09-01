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

#[derive(Default)]
struct Recorded {
  calls: Vec<(String, ValueMap, ValueMap)>,
}

/// A transport is a block, so `mock` is a transport: the whole application
/// runs against canned responses with no backend and no code change. It also
/// records what the chain produced, which is how interceptors get tested.
#[derive(Default)]
pub struct MockTransport {
  responses: IndexMap<String, Result<Value, ServiceError>>,
  recorded: Mutex<Recorded>,
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

  pub fn calls(&self) -> Vec<(String, ValueMap, ValueMap)> {
    self.recorded.lock().calls.clone()
  }

  pub fn last_metadata(&self, key: &str) -> Option<String> {
    let calls = self.recorded.lock();
    match calls.calls.last()?.2.get(key) {
      Some(Value::Str(s)) => Some(s.clone()),
      _ => None,
    }
  }
}

impl Transport for MockTransport {
  fn call(&self, call: Call) -> BoxFuture<'static, Result<Value, ServiceError>> {
    let path = format!("{}.{}", call.service, call.method);
    self.recorded.lock().calls.push((path.clone(), call.args.clone(), call.metadata.clone()));
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
