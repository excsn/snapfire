use std::fmt;

use futures_util::future::BoxFuture;
use snapfire_fsr_core::ValueMap;
use snapfire_fsr_runtime::Identity;

/// Where `begin` sends the browser, plus whatever the provider needs back at
/// the callback. The state never leaves the server; `Auth` keeps it in token
/// custody across the round trip.
pub struct Begin {
  pub redirect: String,
  pub state: ValueMap,
}

pub struct AuthOutcome {
  pub identity: Identity,
  pub tokens: ValueMap,
}

#[derive(Debug, Clone, PartialEq)]
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

pub trait IdentityProvider: Send + Sync {
  fn begin(&self, return_to: &str) -> BoxFuture<'_, Begin>;
  fn callback(&self, params: ValueMap, state: ValueMap) -> BoxFuture<'_, Result<AuthOutcome, AuthError>>;
}
