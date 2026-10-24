use std::sync::Arc;

use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_session::Opened;

use crate::provider::{AuthError, IdentityProvider, Intent, Next, Start};

const FLOW_STATE_KEY: &str = "_sf_auth";

/// What the host answers the browser with after a step.
#[derive(Debug, Clone, PartialEq)]
pub enum Step {
  /// Send the browser to this URL.
  Redirect(String),
  /// The page acts on `data` as `kind` says and posts its answer to the callback.
  Present { kind: String, data: ValueMap },
  /// The flow is over: land on `to`. `signed_in` when the session's identity changed, which is
  /// when the host gives the session a new id.
  Complete { to: String, signed_in: bool },
}

/// The flow over any provider: `begin` starts it, `advance` takes each answer until it completes,
/// `logout` forgets it. Flow state rides token custody, so application code never sees it and each
/// answer is bound to the session that began.
pub struct Auth {
  provider: Arc<dyn IdentityProvider>,
}

impl Auth {
  pub fn new(provider: Arc<dyn IdentityProvider>) -> Self {
    Self { provider }
  }

  pub fn provider(&self) -> &Arc<dyn IdentityProvider> {
    &self.provider
  }

  /// Starts a flow, replacing any in progress. Enrolling needs a signed-in session.
  pub async fn begin(&self, opened: &Opened, start: Start) -> Result<Step, AuthError> {
    opened.tokens.remove(FLOW_STATE_KEY);
    let identity = opened.cell.identity();
    let start = match start.intent {
      Intent::Enrol if identity.is_none() => return Err(AuthError::Denied("enrolling needs a signed-in session".to_owned())),
      Intent::Enrol => Start { identity, ..start },
      _ => start,
    };
    let flow = Flow { return_to: start.return_to.clone(), intent: start.intent };
    let next = self.provider.begin(start).await?;
    self.settle(opened, flow, next)
  }

  /// Seeds flow state when none is in progress, so a login page reached by a typed URL or a direct
  /// link can still post to the callback.
  pub async fn ensure_flow(&self, opened: &Opened, return_to: &str) {
    if opened.tokens.get(FLOW_STATE_KEY).is_none() {
      let _ = self.begin(opened, Start::sign_in(return_to)).await;
    }
  }

  /// Where the flow in progress will land, when one is in progress.
  pub fn pending_return_to(&self, opened: &Opened) -> Option<String> {
    match opened.tokens.get(FLOW_STATE_KEY) {
      Some(Value::Map(state)) => match state.get("return_to") {
        Some(Value::Str(path)) => Some(path.to_string()),
        _ => None,
      },
      _ => None,
    }
  }

  /// The browser's answer to the step in progress. The step is consumed whatever the answer, so a
  /// refused one cannot be replayed.
  pub async fn advance(&self, opened: &Opened, params: ValueMap) -> Result<Step, AuthError> {
    let mut kept = match opened.tokens.remove(FLOW_STATE_KEY) {
      Some(Value::Map(kept)) => kept,
      _ => return Err(AuthError::Invalid("no sign-in in progress for this session".to_owned())),
    };
    let flow = Flow::take(&mut kept);
    let state = match kept.shift_remove("state") {
      Some(Value::Map(state)) => state,
      _ => ValueMap::default(),
    };
    let next = self.provider.advance(params, state).await?;
    self.settle(opened, flow, next)
  }

  fn settle(&self, opened: &Opened, flow: Flow, next: Next) -> Result<Step, AuthError> {
    match next {
      Next::Redirect { to, state } => {
        flow.keep(opened, state);
        Ok(Step::Redirect(to))
      }
      Next::Present { kind, data, state } => {
        flow.keep(opened, state);
        Ok(Step::Present { kind, data })
      }
      Next::Complete(outcome) => {
        let signed_in = match flow.intent {
          Intent::Enrol => {
            let current = opened.cell.identity().map(|identity| identity.subject);
            if current.as_deref() != Some(outcome.identity.subject.as_str()) {
              return Err(AuthError::Denied("the method enrolled belongs to another identity".to_owned()));
            }
            false
          }
          _ => {
            opened.cell.set_identity(Some(outcome.identity));
            true
          }
        };
        opened.tokens.merge(outcome.tokens);
        Ok(Step::Complete { to: flow.return_to, signed_in })
      }
    }
  }

  /// Clears identity and custody. The adapter still calls `Sessions::destroy` for the record
  /// deletion and the expiring cookie.
  pub fn logout(&self, opened: &Opened) {
    opened.cell.clear();
    opened.tokens.clear();
  }
}

/// What the host keeps about a flow beside the provider's own state.
struct Flow {
  return_to: String,
  intent: Intent,
}

impl Flow {
  fn take(kept: &mut ValueMap) -> Self {
    let return_to = match kept.shift_remove("return_to") {
      Some(Value::Str(path)) => path.to_string(),
      _ => "/".to_owned(),
    };
    let intent = match kept.shift_remove("intent") {
      Some(Value::Str(intent)) if intent.as_str() == "enrol" => Intent::Enrol,
      _ => Intent::SignIn,
    };
    Self { return_to, intent }
  }

  fn keep(self, opened: &Opened, state: ValueMap) {
    let mut kept = ValueMap::default();
    kept.insert("return_to".to_owned(), Value::str(self.return_to));
    kept.insert("intent".to_owned(), Value::str(if self.intent == Intent::Enrol { "enrol" } else { "sign_in" }));
    kept.insert("state".to_owned(), Value::Map(state));
    opened.tokens.set(FLOW_STATE_KEY, Value::Map(kept));
  }
}
