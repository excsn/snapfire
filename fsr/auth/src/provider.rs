use std::fmt;

use futures_util::future::BoxFuture;
use snapfire_fsr_core::ValueMap;
use snapfire_fsr_runtime::Identity;

/// What the visitor set out to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Intent {
  SignIn,
  /// Add a way of signing in, a passkey or a linked account, to the identity already signed in.
  Enrol,
}

/// What a flow starts from.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Start {
  pub intent: Intent,
  /// The path on this origin the flow lands on when it completes.
  pub return_to: String,
  /// The method the visitor chose, for a provider offering several; `None` for its default.
  pub method: Option<String>,
  /// Who is signed in: always present for [`Intent::Enrol`].
  pub identity: Option<Identity>,
}

impl Start {
  pub fn sign_in(return_to: impl Into<String>) -> Self {
    Self { intent: Intent::SignIn, return_to: return_to.into(), method: None, identity: None }
  }

  pub fn enrol(return_to: impl Into<String>, identity: Identity) -> Self {
    Self { intent: Intent::Enrol, return_to: return_to.into(), method: None, identity: Some(identity) }
  }

  pub fn method(mut self, method: Option<String>) -> Self {
    self.method = method;
    self
  }

  /// Who the flow is for: set on a sign-in by a provider asking for a second factor.
  pub fn identity(mut self, identity: Option<Identity>) -> Self {
    self.identity = identity;
    self
  }
}

/// What the browser does next. `state` never leaves the server: `Auth` keeps it in token custody
/// and hands it to the next `advance`.
pub enum Next {
  /// Send the browser to `to`, an identity provider's page, which comes back to the callback.
  Redirect { to: String, state: ValueMap },
  /// Hand `data` to the page, which posts its answer to the callback: `kind` names what the page
  /// does with it, `form` for fields to fill, `webauthn.get` and `webauthn.create` for a passkey
  /// ceremony's options, `notice` for a message such as a code sent elsewhere.
  Present { kind: String, data: ValueMap, state: ValueMap },
  /// The visitor is signed in or the new method is enrolled.
  Complete(AuthOutcome),
}

impl Next {
  pub fn redirect(to: impl Into<String>, state: ValueMap) -> Self {
    Self::Redirect { to: to.into(), state }
  }

  pub fn present(kind: impl Into<String>, data: ValueMap, state: ValueMap) -> Self {
    Self::Present { kind: kind.into(), data, state }
  }
}

#[non_exhaustive]
pub struct AuthOutcome {
  pub identity: Identity,
  pub tokens: ValueMap,
}

impl AuthOutcome {
  pub fn new(identity: Identity, tokens: ValueMap) -> Self {
    Self { identity, tokens }
  }
}

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum AuthError {
  Denied(String),
  Invalid(String),
}

impl AuthError {
  pub fn http_status(&self) -> u16 {
    match self {
      Self::Denied(_) => 403,
      Self::Invalid(_) => 400,
    }
  }
}

impl fmt::Display for AuthError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::Denied(m) => write!(f, "denied: {m}"),
      Self::Invalid(m) => write!(f, "invalid: {m}"),
    }
  }
}

impl std::error::Error for AuthError {}

/// One way of proving who the visitor is, as a sequence of steps. Only value-model data crosses
/// it, so a provider in another process is the same trait over a transport.
pub trait IdentityProvider: Send + Sync {
  fn begin(&self, start: Start) -> BoxFuture<'_, Result<Next, AuthError>>;
  /// The browser's answer to the last step: the callback's query, form or JSON body, with the
  /// state that step kept.
  fn advance(&self, params: ValueMap, state: ValueMap) -> BoxFuture<'_, Result<Next, AuthError>>;
  /// New tokens for the ones custody holds, `None` when this provider cannot refresh them.
  fn refresh(&self, tokens: ValueMap) -> BoxFuture<'_, Result<Option<ValueMap>, AuthError>> {
    let _ = tokens;
    Box::pin(futures_util::future::ready(Ok(None)))
  }
}
