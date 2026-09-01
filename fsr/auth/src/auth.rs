use std::sync::Arc;

use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_session::Opened;

use crate::provider::{AuthError, IdentityProvider};

const FLOW_STATE_KEY: &str = "_sf_auth";

/// The flow over any provider: `login` starts it, `callback` finishes it,
/// `logout` forgets it. Flow state rides token custody, so application code
/// never sees it and the callback is bound to the session that began.
pub struct Auth {
  provider: Arc<dyn IdentityProvider>,
}

impl Auth {
  pub fn new(provider: Arc<dyn IdentityProvider>) -> Self {
    Self { provider }
  }

  pub async fn login(&self, opened: &Opened, return_to: &str) -> String {
    let begin = self.provider.begin(return_to).await;
    let mut state = begin.state;
    state.insert("return_to".to_owned(), Value::Str(return_to.to_owned()));
    opened.tokens.set(FLOW_STATE_KEY, Value::Map(state));
    begin.redirect
  }

  pub async fn callback(&self, opened: &Opened, params: ValueMap) -> Result<String, AuthError> {
    let state = match opened.tokens.remove(FLOW_STATE_KEY) {
      Some(Value::Map(state)) => state,
      _ => return Err(AuthError::Invalid("no login in progress for this session".to_owned())),
    };
    let return_to = match state.get("return_to") {
      Some(Value::Str(path)) => path.clone(),
      _ => "/".to_owned(),
    };

    let outcome = self.provider.callback(params, state).await?;
    opened.cell.set_identity(Some(outcome.identity));
    opened.tokens.merge(outcome.tokens);
    Ok(return_to)
  }

  /// Clears identity and custody. The adapter still calls
  /// `Sessions::destroy` for the record deletion and the expiring cookie.
  pub fn logout(&self, opened: &Opened) {
    opened.cell.clear();
    opened.tokens.clear();
  }
}
