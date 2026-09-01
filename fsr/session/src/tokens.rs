use std::sync::Arc;

use parking_lot::Mutex;
use snapfire_fsr_core::{Value, ValueMap};

#[derive(Default)]
struct TokenState {
  tokens: ValueMap,
  dirty: bool,
}

/// Server-side custody for backend credentials and auth flow state. Lives on
/// `Opened` beside the session cell and never enters `RequestCtx`, so loaders,
/// actions and evaluators cannot reach it.
#[derive(Clone, Default)]
pub struct TokenCell(Arc<Mutex<TokenState>>);

impl TokenCell {
  pub fn new(tokens: ValueMap) -> Self {
    Self(Arc::new(Mutex::new(TokenState { tokens, dirty: false })))
  }

  pub fn get(&self, key: &str) -> Option<Value> {
    self.0.lock().tokens.get(key).cloned()
  }

  pub fn set(&self, key: impl Into<String>, value: Value) {
    let mut state = self.0.lock();
    state.tokens.insert(key.into(), value);
    state.dirty = true;
  }

  pub fn remove(&self, key: &str) -> Option<Value> {
    let mut state = self.0.lock();
    let removed = state.tokens.shift_remove(key);
    if removed.is_some() {
      state.dirty = true;
    }
    removed
  }

  pub fn merge(&self, tokens: ValueMap) {
    let mut state = self.0.lock();
    for (key, value) in tokens {
      state.tokens.insert(key, value);
    }
    state.dirty = true;
  }

  pub fn clear(&self) {
    let mut state = self.0.lock();
    state.tokens.clear();
    state.dirty = true;
  }

  pub fn is_dirty(&self) -> bool {
    self.0.lock().dirty
  }

  pub fn snapshot(&self) -> ValueMap {
    self.0.lock().tokens.clone()
  }
}
