use std::sync::Arc;

use futures_util::future::BoxFuture;
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_runtime::Identity;

use crate::provider::{AuthError, AuthOutcome, IdentityProvider, Intent, Next, Start};

fn text(map: &ValueMap, key: &str) -> Option<String> {
  match map.get(key) {
    Some(Value::Str(text)) => Some(text.to_string()),
    _ => None,
  }
}

fn inner(map: &ValueMap, key: &str) -> ValueMap {
  match map.get(key) {
    Some(Value::Map(inner)) => inner.clone(),
    _ => ValueMap::default(),
  }
}

fn identity_value(identity: &Identity) -> Value {
  let mut map = ValueMap::default();
  map.insert("subject".to_owned(), Value::str(identity.subject.clone()));
  map.insert("claims".to_owned(), Value::Map(identity.claims.clone()));
  Value::Map(map)
}

fn identity_of(map: &ValueMap) -> Option<Identity> {
  Some(Identity::new(text(map, "subject")?, inner(map, "claims")))
}

/// A step of the provider inside, its state kept under `inner` beside `extra`.
fn wrapped(next: Next, extra: ValueMap) -> Next {
  let keep = |state: ValueMap| {
    let mut kept = extra.clone();
    kept.insert("inner".to_owned(), Value::Map(state));
    kept
  };
  match next {
    Next::Redirect { to, state } => Next::Redirect { to, state: keep(state) },
    Next::Present { kind, data, state } => Next::Present { kind, data, state: keep(state) },
    complete => complete,
  }
}

fn start_map(start: &Start) -> ValueMap {
  let mut map = ValueMap::default();
  map.insert("intent".to_owned(), Value::str(if start.intent == Intent::Enrol { "enrol" } else { "sign_in" }));
  map.insert("return_to".to_owned(), Value::str(start.return_to.clone()));
  if let Some(identity) = &start.identity {
    map.insert("identity".to_owned(), identity_value(identity));
  }
  map
}

fn start_of(map: &ValueMap, method: Option<String>) -> Start {
  let intent = if text(map, "intent").as_deref() == Some("enrol") { Intent::Enrol } else { Intent::SignIn };
  Start { intent, return_to: text(map, "return_to").unwrap_or_else(|| "/".to_owned()), method, identity: identity_of(&inner(map, "identity")) }
}

/// Several methods on one login page, by name. `Start::method` picks one; with none and more than
/// one method the first step is a `choose` the page presents, `{ methods: [names] }`, answered with
/// `{ method }`. Each method's steps carry its name, so its answers reach it alone.
#[derive(Default)]
pub struct Methods {
  methods: Vec<(String, Arc<dyn IdentityProvider>)>,
}

impl Methods {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn method(mut self, name: impl Into<String>, provider: Arc<dyn IdentityProvider>) -> Self {
    self.methods.push((name.into(), provider));
    self
  }

  pub fn names(&self) -> Vec<&str> {
    self.methods.iter().map(|(name, _)| name.as_str()).collect()
  }

  fn find(&self, name: &str) -> Result<&Arc<dyn IdentityProvider>, AuthError> {
    self.methods.iter().find(|(own, _)| own == name).map(|(_, provider)| provider).ok_or_else(|| AuthError::Invalid(format!("no sign-in method named `{name}`; the methods are {}", self.names().join(", "))))
  }

  fn run(&self, name: String, start: Start) -> BoxFuture<'_, Result<Next, AuthError>> {
    Box::pin(async move {
      let provider = self.find(&name)?;
      let mut extra = ValueMap::default();
      extra.insert("method".to_owned(), Value::str(name.clone()));
      let start = Start { method: None, ..start };
      Ok(wrapped(provider.begin(start).await?, extra))
    })
  }
}

impl IdentityProvider for Methods {
  fn begin(&self, start: Start) -> BoxFuture<'_, Result<Next, AuthError>> {
    let name = match (&start.method, self.methods.as_slice()) {
      (Some(name), _) => name.clone(),
      (None, [(only, _)]) => only.clone(),
      (None, []) => return Box::pin(futures_util::future::ready(Err(AuthError::Invalid("no sign-in method is configured".to_owned())))),
      (None, _) => {
        let mut data = ValueMap::default();
        data.insert("methods".to_owned(), Value::seq(self.names().into_iter().map(Value::str).collect::<Vec<_>>()));
        let mut state = ValueMap::default();
        state.insert("start".to_owned(), Value::Map(start_map(&start)));
        return Box::pin(futures_util::future::ready(Ok(Next::present("choose", data, state))));
      }
    };
    self.run(name, start)
  }

  fn advance(&self, params: ValueMap, state: ValueMap) -> BoxFuture<'_, Result<Next, AuthError>> {
    if let Some(Value::Map(start)) = state.get("start") {
      let Some(name) = text(&params, "method") else {
        return Box::pin(futures_util::future::ready(Err(AuthError::Invalid("choose a method".to_owned()))));
      };
      return self.run(name.clone(), start_of(start, Some(name)));
    }
    Box::pin(async move {
      let name = text(&state, "method").ok_or_else(|| AuthError::Invalid("a step without its method".to_owned()))?;
      let provider = self.find(&name)?;
      let mut extra = ValueMap::default();
      extra.insert("method".to_owned(), Value::str(name));
      Ok(wrapped(provider.advance(params, inner(&state, "inner")).await?, extra))
    })
  }

  fn refresh(&self, tokens: ValueMap) -> BoxFuture<'_, Result<Option<ValueMap>, AuthError>> {
    Box::pin(async move {
      for (_, provider) in &self.methods {
        if let Some(renewed) = provider.refresh(tokens.clone()).await? {
          return Ok(Some(renewed));
        }
      }
      Ok(None)
    })
  }
}

/// A sign-in that passes `first` and then `second` for the same subject: a second factor. The
/// visitor is signed in only when `second` completes for the subject `first` named, with both
/// methods' tokens. An enrolment enrols `second`.
pub struct Sequence {
  first: Arc<dyn IdentityProvider>,
  second: Arc<dyn IdentityProvider>,
}

impl Sequence {
  pub fn new(first: Arc<dyn IdentityProvider>, second: Arc<dyn IdentityProvider>) -> Self {
    Self { first, second }
  }

  /// `second` begun for the subject `first` completed for; the pair's outcome when `second`
  /// completes at once.
  async fn after_first(&self, start: &ValueMap, outcome: AuthOutcome) -> Result<Next, AuthError> {
    let second = Start { intent: Intent::SignIn, return_to: text(start, "return_to").unwrap_or_else(|| "/".to_owned()), method: None, identity: Some(outcome.identity.clone()) };
    let next = self.second.begin(second).await?;
    let mut extra = ValueMap::default();
    extra.insert("stage".to_owned(), Value::str("second"));
    extra.insert("identity".to_owned(), identity_value(&outcome.identity));
    extra.insert("tokens".to_owned(), Value::Map(outcome.tokens.clone()));
    finish(wrapped(next, extra.clone()), &extra)
  }
}

/// A completed second step as the pair's outcome: the first factor's identity, both factors'
/// tokens, refused when the second completed for anyone else.
fn finish(next: Next, kept: &ValueMap) -> Result<Next, AuthError> {
  let Next::Complete(second) = next else {
    return Ok(next);
  };
  let first = identity_of(&inner(kept, "identity")).ok_or_else(|| AuthError::Invalid("a second factor without the first".to_owned()))?;
  if second.identity.subject != first.subject {
    return Err(AuthError::Denied("the second factor belongs to another identity".to_owned()));
  }
  let mut tokens = inner(kept, "tokens");
  tokens.extend(second.tokens);
  Ok(Next::Complete(AuthOutcome::new(first, tokens)))
}

impl IdentityProvider for Sequence {
  fn begin(&self, start: Start) -> BoxFuture<'_, Result<Next, AuthError>> {
    Box::pin(async move {
      let mut extra = ValueMap::default();
      if start.intent == Intent::Enrol {
        extra.insert("stage".to_owned(), Value::str("enrol"));
        return Ok(wrapped(self.second.begin(start).await?, extra));
      }
      let kept = start_map(&start);
      extra.insert("stage".to_owned(), Value::str("first"));
      extra.insert("start".to_owned(), Value::Map(kept.clone()));
      match self.first.begin(start).await? {
        Next::Complete(outcome) => self.after_first(&kept, outcome).await,
        next => Ok(wrapped(next, extra)),
      }
    })
  }

  fn advance(&self, params: ValueMap, state: ValueMap) -> BoxFuture<'_, Result<Next, AuthError>> {
    Box::pin(async move {
      let mut extra = state.clone();
      extra.shift_remove("inner");
      match text(&state, "stage").as_deref() {
        Some("first") => match self.first.advance(params, inner(&state, "inner")).await? {
          Next::Complete(outcome) => self.after_first(&inner(&state, "start"), outcome).await,
          next => Ok(wrapped(next, extra)),
        },
        Some("second") => finish(wrapped(self.second.advance(params, inner(&state, "inner")).await?, extra.clone()), &extra),
        Some("enrol") => Ok(wrapped(self.second.advance(params, inner(&state, "inner")).await?, extra)),
        _ => Err(AuthError::Invalid("a step without its stage".to_owned())),
      }
    })
  }

  fn refresh(&self, tokens: ValueMap) -> BoxFuture<'_, Result<Option<ValueMap>, AuthError>> {
    self.first.refresh(tokens)
  }
}
