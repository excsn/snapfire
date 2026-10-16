use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_runtime::Identity;
use snapfire_fsr_session::TokenCell;

/// One outbound call as it travels the chain. Interceptors read the identity
/// and write metadata; `credentials` is reachable here and nowhere in
/// application code, which is what the custody claim rests on.
pub struct Call {
  pub service: String,
  pub method: String,
  pub args: ValueMap,
  pub identity: Option<Identity>,
  pub metadata: ValueMap,
  pub credentials: Arc<dyn Credentials>,
  /// The request the call is made for, shared by every call one binding makes.
  pub request: u64,
}

static REQUESTS: AtomicU64 = AtomicU64::new(1);

/// A request number no other binding in this process holds.
pub fn next_request() -> u64 {
  REQUESTS.fetch_add(1, Ordering::Relaxed)
}

impl Call {
  pub fn set_metadata(&mut self, key: impl Into<String>, value: impl Into<String>) {
    self.metadata.insert(key.into(), Value::str(value.into()));
  }

  pub fn metadata_str(&self, key: &str) -> Option<&str> {
    match self.metadata.get(key) {
      Some(Value::Str(s)) => Some(s.as_str()),
      _ => None,
    }
  }
}

/// Read and write access to the request's backend credentials. The session
/// crate's `TokenCell` is the production implementation; refresh writes back
/// through `set`.
pub trait Credentials: Send + Sync {
  fn get(&self, key: &str) -> Option<Value>;
  fn set(&self, key: &str, value: Value);
}

#[derive(Default)]
pub struct NoCredentials;

impl Credentials for NoCredentials {
  fn get(&self, _key: &str) -> Option<Value> {
    None
  }

  fn set(&self, _key: &str, _value: Value) {}
}

impl Credentials for snapfire_fsr_session::TokenCell {
  fn get(&self, key: &str) -> Option<Value> {
    TokenCell::get(self, key)
  }

  fn set(&self, key: &str, value: Value) {
    TokenCell::set(self, key, value)
  }
}
