use std::path::Path;

use futures_util::future::{ready, BoxFuture};
use serde::Deserialize;
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_runtime::Identity;

use crate::provider::{AuthError, AuthOutcome, IdentityProvider, Intent, Next, Start};

struct DevUser {
  name: String,
  password: String,
  claims: ValueMap,
}

/// Name and password against a fixed table, the second implementation that
/// proves the seam. `begin` presents a form of `user` and `password` for the
/// application's login page, since auth never renders.
pub struct DevProvider {
  users: Vec<DevUser>,
}

impl DevProvider {
  pub fn new() -> Self {
    Self { users: Vec::new() }
  }

  pub fn user(mut self, name: impl Into<String>, password: impl Into<String>) -> Self {
    self.users.push(DevUser { name: name.into(), password: password.into(), claims: ValueMap::default() });
    self
  }

  pub fn user_with_claims(mut self, name: impl Into<String>, password: impl Into<String>, claims: ValueMap) -> Self {
    self.users.push(DevUser { name: name.into(), password: password.into(), claims });
    self
  }

  /// The table from a TOML file of `[[users]]` rows, each `name`, `password`
  /// and an optional `claims` table. A file with no row is an error: a login
  /// page nobody can pass is a misconfiguration, not an empty provider.
  pub fn from_toml(path: impl AsRef<Path>) -> Result<Self, String> {
    let path = path.as_ref();
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let file: UsersFile = toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    if file.users.is_empty() {
      return Err(format!("{}: no [[users]] row", path.display()));
    }
    let mut provider = Self::new();
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
    toml::Value::String(s) => Value::str(s),
    toml::Value::Integer(i) => Value::int(i),
    toml::Value::Float(f) => Value::F64(f),
    toml::Value::Boolean(b) => Value::Bool(b),
    toml::Value::Datetime(d) => Value::str(d.to_string()),
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

impl Default for DevProvider {
  fn default() -> Self {
    Self::new()
  }
}

/// The step a name-and-password provider presents: a form of `user` and `password`.
pub fn password_form() -> Next {
  let mut data = ValueMap::default();
  data.insert("fields".to_owned(), Value::seq(vec![Value::str("user"), Value::str("password")]));
  Next::present("form", data, ValueMap::default())
}

impl IdentityProvider for DevProvider {
  fn begin(&self, start: Start) -> BoxFuture<'_, Result<Next, AuthError>> {
    let next = match start.intent {
      Intent::SignIn => Ok(password_form()),
      _ => Err(AuthError::Invalid("the file provider enrols nothing".to_owned())),
    };
    Box::pin(ready(next))
  }

  fn advance(&self, params: ValueMap, _state: ValueMap) -> BoxFuture<'_, Result<Next, AuthError>> {
    let result = (|| {
      let name = param(&params, "user").ok_or_else(|| AuthError::Invalid("missing user".to_owned()))?;
      let password = param(&params, "password").ok_or_else(|| AuthError::Invalid("missing password".to_owned()))?;
      let user = self
        .users
        .iter()
        .find(|u| u.name == name && u.password == password)
        .ok_or_else(|| AuthError::Denied("unknown user or wrong password".to_owned()))?;

      let mut tokens = ValueMap::default();
      tokens.insert("access_token".to_owned(), Value::str(format!("dev-token-{}", user.name)));
      Ok(Next::Complete(AuthOutcome::new(Identity::new(user.name.clone(), user.claims.clone()), tokens)))
    })();
    Box::pin(ready(result))
  }
}
