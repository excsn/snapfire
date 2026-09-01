use std::sync::Arc;

use parking_lot::Mutex;
use snapfire_fsr_core::{Params, Value, ValueMap};

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
/// the session and the CSRF token the page should embed. Serializable values
/// only, per the boundary rules.
#[derive(Clone, Default)]
pub struct RequestCtx {
  pub params: Params,
  pub session: SessionCell,
  pub csrf: Option<String>,
}

impl RequestCtx {
  pub fn anonymous(params: Params) -> Self {
    Self { params, session: SessionCell::default(), csrf: None }
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
