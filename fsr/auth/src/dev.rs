use std::path::Path;

use futures_util::future::{ready, BoxFuture};
use serde::Deserialize;
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

  /// The table from a TOML file of `[[users]]` rows, each `name`, `password`
  /// and an optional `claims` table. A file with no row is an error: a login
  /// page nobody can pass is a misconfiguration, not an empty provider.
  pub fn from_toml(login_path: impl Into<String>, path: impl AsRef<Path>) -> Result<Self, String> {
    let path = path.as_ref();
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let file: UsersFile = toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    if file.users.is_empty() {
      return Err(format!("{}: no [[users]] row", path.display()));
    }
    let mut provider = Self::new(login_path);
    for row in file.users {
      if row.name.is_empty() {
        return Err(format!("{}: a [[users]] row has an empty name", path.display()));
      }
      let claims = row.claims.into_iter().map(|(k, v)| (k, toml_value(v))).collect();
      provider = provider.user_with_claims(row.name, row.password, claims);
    }
    Ok(provider)
  }
}

#[derive(Deserialize)]
struct UsersFile {
  #[serde(default)]
  users: Vec<UserRow>,
}

#[derive(Deserialize)]
struct UserRow {
  name: String,
  password: String,
  #[serde(default)]
  claims: toml::Table,
}

fn toml_value(value: toml::Value) -> Value {
  match value {
    toml::Value::String(s) => Value::Str(s),
    toml::Value::Integer(i) => Value::int(i),
    toml::Value::Float(f) => Value::F64(f),
    toml::Value::Boolean(b) => Value::Bool(b),
    toml::Value::Datetime(d) => Value::Str(d.to_string()),
    toml::Value::Array(items) => Value::Seq(items.into_iter().map(toml_value).collect()),
    toml::Value::Table(table) => Value::Map(table.into_iter().map(|(k, v)| (k, toml_value(v))).collect()),
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
