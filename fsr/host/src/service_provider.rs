//! The identity provider behind a client and the refresh the bearer interceptor calls through it.

use std::future::ready;
use std::sync::Arc;

use futures_util::future::BoxFuture;
use snapfire_fsr_auth::{AuthError, AuthOutcome, IdentityProvider, Intent, Next, Start};
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_runtime::{FailureKind, Identity, ServiceError};
use snapfire_fsr_service::Services;

fn identity_of(map: &ValueMap) -> Option<Identity> {
  let subject = match map.get("subject") {
    Some(Value::Str(subject)) => subject.to_string(),
    _ => return None,
  };
  let claims = match map.get("claims") {
    Some(Value::Map(claims)) => claims.clone(),
    _ => ValueMap::default(),
  };
  Some(Identity::new(subject, claims))
}

/// The provider behind a client: `begin`, `advance` and `refresh` go to its `beginSignIn`,
/// `advanceSignIn` and `refreshSignIn` with the same values, so any auth service is a provider. A
/// client declaring only `authenticate` is a password service: the step is the password form and
/// the answer goes to `authenticate`.
pub struct ServiceProvider {
  services: Arc<Services>,
  client: String,
}

pub const BEGIN_SIGN_IN: &str = "beginSignIn";
pub const ADVANCE_SIGN_IN: &str = "advanceSignIn";
pub const REFRESH_SIGN_IN: &str = "refreshSignIn";
pub const AUTHENTICATE: &str = "authenticate";

impl ServiceProvider {
  pub fn new(services: Arc<Services>, client: impl Into<String>) -> Self {
    Self { services, client: client.into() }
  }

  fn declares(&self, method: &str) -> bool {
    self.services.contract().services.get(&self.client).is_some_and(|service| service.methods.contains_key(method))
  }

  fn call(&self, method: &str, args: ValueMap) -> BoxFuture<'static, Result<Value, AuthError>> {
    let call = self.services.bind_anonymous().call(&self.client, method, args);
    let method = method.to_owned();
    Box::pin(async move { call.await.map_err(|error| refusal(&method, error)) })
  }

  fn authenticate(&self, params: ValueMap) -> BoxFuture<'_, Result<Next, AuthError>> {
    let (user, password) = (param(&params, "user"), param(&params, "password"));
    let call = match (user, password) {
      (Some(user), Some(password)) => {
        let mut args = ValueMap::default();
        args.insert("user".to_owned(), Value::str(user));
        args.insert("password".to_owned(), Value::str(password));
        Some(self.call(AUTHENTICATE, args))
      }
      _ => None,
    };
    Box::pin(async move {
      let Some(call) = call else {
        return Err(AuthError::Invalid("missing user or password".to_owned()));
      };
      let answer = match call.await? {
        Value::Map(answer) => answer,
        other => return Err(AuthError::Invalid(format!("authenticate answered {other:?}"))),
      };
      let identity = identity_of(&answer).ok_or_else(|| AuthError::Invalid("authenticate answered without a subject".to_owned()))?;
      let mut tokens = ValueMap::default();
      if let Some(Value::Str(token)) = answer.get("access_token") {
        tokens.insert("access_token".to_owned(), Value::Str(token.clone()));
      }
      Ok(Next::Complete(AuthOutcome::new(identity, tokens)))
    })
  }
}

/// A client error is the service saying no; anything else is a failure to answer.
fn refusal(method: &str, error: ServiceError) -> AuthError {
  match error.kind {
    kind if kind.is_client_error() && !matches!(kind, FailureKind::RequestTimeout | FailureKind::Conflict) => AuthError::Denied(error.message.clone()),
    _ => AuthError::Invalid(format!("{method}: {error}")),
  }
}

fn param(params: &ValueMap, key: &str) -> Option<String> {
  match params.get(key) {
    Some(Value::Str(s)) => Some(s.to_string()),
    _ => None,
  }
}

/// A map a step carries, written as a map or as one string in plain JSON, which
/// is how a contract with no open map type declares it.
fn map_of(map: &ValueMap, key: &str) -> ValueMap {
  match map.get(key) {
    Some(Value::Map(inner)) => inner.clone(),
    Some(Value::Str(text)) => serde_json::from_str::<serde_json::Value>(text)
      .ok()
      .map(|json| snapfire_fsr_core::json::to_value(&json))
      .and_then(|value| match value {
        Value::Map(inner) => Some(inner),
        _ => None,
      })
      .unwrap_or_default(),
    _ => ValueMap::default(),
  }
}

/// A map as one string in plain JSON.
fn blob(map: ValueMap) -> Value {
  Value::str(snapfire_fsr_core::json::stringify(&Value::Map(map), "", snapfire_fsr_core::json::Integers::Digits).unwrap_or_else(|_| "{}".to_owned()))
}

/// `{ intent, return_to, method, identity }` as `beginSignIn` takes it, `identity` being
/// `{ subject, claims }` as one string in plain JSON.
pub fn encode_start(start: &Start) -> ValueMap {
  let mut args = ValueMap::default();
  args.insert("intent".to_owned(), Value::str(if start.intent == Intent::Enrol { "enrol" } else { "sign_in" }));
  args.insert("return_to".to_owned(), Value::str(start.return_to.clone()));
  args.insert("method".to_owned(), start.method.clone().map(Value::str).unwrap_or(Value::Null));
  args.insert(
    "identity".to_owned(),
    match &start.identity {
      Some(identity) => {
        let mut map = ValueMap::default();
        map.insert("subject".to_owned(), Value::str(identity.subject.clone()));
        map.insert("claims".to_owned(), Value::Map(identity.claims.clone()));
        blob(map)
      }
      None => Value::Null,
    },
  );
  args
}

/// A step as a service answers it: `{ next: "redirect", to, state }`, `{ next: "present", kind,
/// data, state }` or `{ next: "complete", subject, claims, tokens }`, each map written as a map or
/// as one string in plain JSON.
pub fn decode_next(answer: Value) -> Result<Next, AuthError> {
  let Value::Map(answer) = answer else {
    return Err(AuthError::Invalid(format!("a sign-in step answered {answer:?}")));
  };
  let text = |key: &str| param(&answer, key);
  match text("next").as_deref() {
    Some("redirect") => {
      let to = text("to").ok_or_else(|| AuthError::Invalid("a redirect step without `to`".to_owned()))?;
      Ok(Next::redirect(to, map_of(&answer, "state")))
    }
    Some("present") => {
      let kind = text("kind").ok_or_else(|| AuthError::Invalid("a present step without `kind`".to_owned()))?;
      Ok(Next::present(kind, map_of(&answer, "data"), map_of(&answer, "state")))
    }
    Some("complete") => {
      let subject = text("subject").ok_or_else(|| AuthError::Invalid("a complete step without a subject".to_owned()))?;
      let identity = Identity::new(subject, map_of(&answer, "claims"));
      Ok(Next::Complete(AuthOutcome::new(identity, map_of(&answer, "tokens"))))
    }
    other => Err(AuthError::Invalid(format!("a sign-in step answered next = {other:?}"))),
  }
}

impl IdentityProvider for ServiceProvider {
  fn begin(&self, start: Start) -> BoxFuture<'_, Result<Next, AuthError>> {
    if !self.declares(BEGIN_SIGN_IN) {
      return Box::pin(ready(match start.intent {
        Intent::SignIn => Ok(snapfire_fsr_auth::password_form()),
        _ => Err(AuthError::Invalid(format!("{} declares no {BEGIN_SIGN_IN}, so it enrols nothing", self.client))),
      }));
    }
    let call = self.call(BEGIN_SIGN_IN, encode_start(&start));
    Box::pin(async move { decode_next(call.await?) })
  }

  fn advance(&self, params: ValueMap, state: ValueMap) -> BoxFuture<'_, Result<Next, AuthError>> {
    if !self.declares(ADVANCE_SIGN_IN) {
      return self.authenticate(params);
    }
    let mut args = ValueMap::default();
    args.insert("params".to_owned(), blob(params));
    args.insert("state".to_owned(), blob(state));
    let call = self.call(ADVANCE_SIGN_IN, args);
    Box::pin(async move { decode_next(call.await?) })
  }

  fn refresh(&self, tokens: ValueMap) -> BoxFuture<'_, Result<Option<ValueMap>, AuthError>> {
    if !self.declares(REFRESH_SIGN_IN) {
      return Box::pin(ready(Ok(None)));
    }
    let mut args = ValueMap::default();
    args.insert("tokens".to_owned(), blob(tokens));
    let call = self.call(REFRESH_SIGN_IN, args);
    Box::pin(async move {
      match call.await? {
        Value::Map(answer) => Ok(Some(map_of(&answer, "tokens"))),
        Value::Null => Ok(None),
        other => Err(AuthError::Invalid(format!("{REFRESH_SIGN_IN} answered {other:?}"))),
      }
    })
  }
}

/// The identity provider's `refresh` behind the credential interceptor. The provider is made after
/// the services it may call, so it is bound once it exists. Nothing is renewed until it is bound.
#[derive(Default)]
pub(crate) struct ProviderRefresh(std::sync::OnceLock<Arc<dyn IdentityProvider>>);

impl ProviderRefresh {
  pub(crate) fn bind(&self, provider: Arc<dyn IdentityProvider>) {
    let _ = self.0.set(provider);
  }
}

impl snapfire_fsr_service::Refresher for ProviderRefresh {
  fn refresh(&self, tokens: ValueMap) -> BoxFuture<'static, Option<ValueMap>> {
    let Some(provider) = self.0.get().cloned() else {
      return Box::pin(ready(None));
    };
    Box::pin(async move {
      match provider.refresh(tokens).await {
        Ok(renewed) => renewed,
        Err(error) => {
          tracing::warn!(target: "fsr::auth", error = %error, "refreshing the session's tokens failed");
          None
        }
      }
    })
  }
}
