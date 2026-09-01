use futures_util::future::{ready, BoxFuture};
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_runtime::Identity;

use crate::provider::{AuthError, AuthOutcome, Begin, IdentityProvider};

struct DevUser {
  name: String,
  password: String,
  claims: ValueMap,
}

/// Name and password against a fixed table, the second implementation that
/// proves the seam. `begin` redirects to an application-owned login page,
/// since auth never renders.
pub struct DevProvider {
  login_path: String,
  users: Vec<DevUser>,
}

impl DevProvider {
  pub fn new(login_path: impl Into<String>) -> Self {
    Self { login_path: login_path.into(), users: Vec::new() }
  }

  pub fn user(mut self, name: impl Into<String>, password: impl Into<String>) -> Self {
    self.users.push(DevUser { name: name.into(), password: password.into(), claims: ValueMap::new() });
    self
  }

  pub fn user_with_claims(mut self, name: impl Into<String>, password: impl Into<String>, claims: ValueMap) -> Self {
    self.users.push(DevUser { name: name.into(), password: password.into(), claims });
    self
  }
}

fn param<'p>(params: &'p ValueMap, key: &str) -> Option<&'p str> {
  match params.get(key) {
    Some(Value::Str(s)) => Some(s.as_str()),
    _ => None,
  }
}

impl IdentityProvider for DevProvider {
  fn begin(&self, return_to: &str) -> BoxFuture<'_, Begin> {
    let encoded: String = form_urlencoded::byte_serialize(return_to.as_bytes()).collect();
    let redirect = format!("{}?return_to={}", self.login_path, encoded);
    Box::pin(ready(Begin { redirect, state: ValueMap::new() }))
  }

  fn callback(&self, params: ValueMap, _state: ValueMap) -> BoxFuture<'_, Result<AuthOutcome, AuthError>> {
    let result = (|| {
      let name = param(&params, "user").ok_or_else(|| AuthError::Invalid("missing user".to_owned()))?;
      let password = param(&params, "password").ok_or_else(|| AuthError::Invalid("missing password".to_owned()))?;
      let user = self
        .users
        .iter()
        .find(|u| u.name == name && u.password == password)
        .ok_or_else(|| AuthError::Denied("unknown user or wrong password".to_owned()))?;

      let mut tokens = ValueMap::new();
      tokens.insert("access_token".to_owned(), Value::Str(format!("dev-token-{}", user.name)));
      Ok(AuthOutcome {
        identity: Identity { subject: user.name.clone(), claims: user.claims.clone() },
        tokens,
      })
    })();
    Box::pin(ready(result))
  }
}
