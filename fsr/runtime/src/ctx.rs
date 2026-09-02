use std::sync::Arc;

use parking_lot::Mutex;
use snapfire_fsr_core::{Params, Value, ValueMap};

use crate::services::ServiceHandle;

/// Who the request is, resolved by the session layer before anything loads.
/// Application code reads it; it never sees a token.
#[derive(Debug, Clone, PartialEq)]
pub struct Identity {
  pub subject: String,
  pub claims: ValueMap,
}

#[derive(Debug, Default)]
struct SessionState {
  data: ValueMap,
  identity: Option<Identity>,
  dirty: bool,
}

/// The request's session, shared across loaders and actions. Mutation marks it
/// dirty; the session layer persists a dirty cell when the response starts.
#[derive(Clone, Default)]
pub struct SessionCell(Arc<Mutex<SessionState>>);

impl SessionCell {
  pub fn new(data: ValueMap, identity: Option<Identity>) -> Self {
    Self(Arc::new(Mutex::new(SessionState { data, identity, dirty: false })))
  }

  pub fn get(&self, key: &str) -> Option<Value> {
    self.0.lock().data.get(key).cloned()
  }

  pub fn insert(&self, key: impl Into<String>, value: Value) {
    let mut state = self.0.lock();
    state.data.insert(key.into(), value);
    state.dirty = true;
  }

  pub fn remove(&self, key: &str) -> Option<Value> {
    let mut state = self.0.lock();
    let removed = state.data.shift_remove(key);
    if removed.is_some() {
      state.dirty = true;
    }
    removed
  }

  pub fn identity(&self) -> Option<Identity> {
    self.0.lock().identity.clone()
  }

  pub fn set_identity(&self, identity: Option<Identity>) {
    let mut state = self.0.lock();
    state.identity = identity;
    state.dirty = true;
  }

  /// Logout: drops data and identity in one dirty write.
  pub fn clear(&self) {
    let mut state = self.0.lock();
    state.data.clear();
    state.identity = None;
    state.dirty = true;
  }

  pub fn is_dirty(&self) -> bool {
    self.0.lock().dirty
  }

  pub fn snapshot(&self) -> (ValueMap, Option<Identity>) {
    let state = self.0.lock();
    (state.data.clone(), state.identity.clone())
  }
}

/// Everything a loader or action may know about the request: matched params,
/// the session, the CSRF token the page should embed and the bound service
/// handle. Serializable values only, per the boundary rules, plus the handle,
/// which is callable but carries nothing readable.
#[derive(Clone, Default)]
pub struct RequestCtx {
  pub params: Params,
  /// The query string, decoded, one value per key with the last repeat
  /// winning. Keys starting with `__` are the runtime's own and are dropped.
  pub query: Params,
  pub session: SessionCell,
  pub csrf: Option<String>,
  pub services: ServiceHandle,
}

impl RequestCtx {
  pub fn anonymous(params: Params) -> Self {
    Self { params, query: Params::new(), session: SessionCell::default(), csrf: None, services: ServiceHandle::default() }
  }

  pub fn identity_value(&self) -> Option<Value> {
    self.session.identity().map(|identity| {
      let mut map = ValueMap::new();
      map.insert("subject".to_owned(), Value::Str(identity.subject));
      map.insert("claims".to_owned(), Value::Map(identity.claims));
      Value::Map(map)
    })
  }
}

/// Decodes a query string into `Params`: `+` and `%XX` decoded, empty keys and
/// keys starting with `__` dropped, a repeated key keeping its last value.
pub fn parse_query(raw: &str) -> Params {
  let mut out = Params::new();
  for pair in raw.split('&') {
    if pair.is_empty() {
      continue;
    }
    let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
    let key = percent_decode(key);
    if key.is_empty() || key.starts_with("__") {
      continue;
    }
    out.insert(key, percent_decode(value));
  }
  out
}

fn percent_decode(raw: &str) -> String {
  let bytes = raw.as_bytes();
  let mut out = Vec::with_capacity(bytes.len());
  let mut i = 0;
  while i < bytes.len() {
    match bytes[i] {
      b'+' => out.push(b' '),
      b'%' => {
        let hex = |b: u8| (b as char).to_digit(16).map(|d| d as u8);
        match (bytes.get(i + 1).copied().and_then(hex), bytes.get(i + 2).copied().and_then(hex)) {
          (Some(hi), Some(lo)) => {
            out.push(hi * 16 + lo);
            i += 2;
          }
          _ => out.push(b'%'),
        }
      }
      b => out.push(b),
    }
    i += 1;
  }
  String::from_utf8_lossy(&out).into_owned()
}
