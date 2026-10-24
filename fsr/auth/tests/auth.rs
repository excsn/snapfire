use std::sync::Arc;

use futures::executor::block_on;
use futures_util::future::{ready, BoxFuture};
use snapfire_fsr_auth::{Auth, AuthError, AuthOutcome, DevProvider, IdentityProvider, Intent, Next, Start, Step};
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_runtime::Identity;
use snapfire_fsr_session::{MemorySessionStore, Opened, SessionConfig, Sessions};

const KEY: &[u8] = b"test-signing-key-32-bytes-long!!";

fn layer() -> Sessions {
  Sessions::new(Arc::new(MemorySessionStore::new(128)), KEY, SessionConfig::default())
}

fn auth() -> Auth {
  Auth::new(Arc::new(DevProvider::new().user("alice", "wonder")))
}

fn map(pairs: &[(&str, &str)]) -> ValueMap {
  pairs.iter().map(|(k, v)| ((*k).to_owned(), Value::str((*v).to_owned()))).collect()
}

fn creds(user: &str, password: &str) -> ValueMap {
  map(&[("user", user), ("password", password)])
}

fn begin(auth: &Auth, opened: &Opened, return_to: &str) -> Step {
  block_on(auth.begin(opened, Start::sign_in(return_to))).unwrap()
}

#[test]
fn a_password_provider_presents_its_form() {
  let opened = block_on(layer().open(None));
  match begin(&auth(), &opened, "/dash/servers") {
    Step::Present { kind, data } => {
      assert_eq!(kind, "form");
      assert_eq!(data.get("fields"), Some(&Value::seq(vec![Value::str("user"), Value::str("password")])));
    }
    other => panic!("{other:?}"),
  }
  assert_eq!(auth().pending_return_to(&opened).as_deref(), Some("/dash/servers"));
}

#[test]
fn an_answer_that_completes_sets_identity_and_tokens_and_returns_the_destination() {
  let auth = auth();
  let opened = block_on(layer().open(None));
  begin(&auth, &opened, "/dash/servers");

  let step = block_on(auth.advance(&opened, creds("alice", "wonder"))).unwrap();
  assert_eq!(step, Step::Complete { to: "/dash/servers".to_owned(), signed_in: true });
  assert_eq!(opened.cell.identity().unwrap().subject, "alice");
  assert_eq!(opened.tokens.get("access_token"), Some(Value::Str("dev-token-alice".into())));
  assert_eq!(opened.cell.get("access_token"), None, "tokens stay in custody");
}

#[test]
fn an_answer_without_a_begun_flow_is_invalid() {
  let opened = block_on(layer().open(None));
  let err = block_on(auth().advance(&opened, creds("alice", "wonder"))).unwrap_err();
  assert!(matches!(err, AuthError::Invalid(_)));
  assert_eq!(err.http_status(), 400);
}

#[test]
fn wrong_password_is_denied_and_consumes_the_flow() {
  let auth = auth();
  let opened = block_on(layer().open(None));
  begin(&auth, &opened, "/");

  let err = block_on(auth.advance(&opened, creds("alice", "nope"))).unwrap_err();
  assert!(matches!(err, AuthError::Denied(_)));
  assert_eq!(err.http_status(), 403);
  assert!(opened.cell.identity().is_none());

  let err = block_on(auth.advance(&opened, creds("alice", "wonder"))).unwrap_err();
  assert!(matches!(err, AuthError::Invalid(_)), "a failed flow cannot be replayed");
}

#[test]
fn logout_forgets_identity_and_custody() {
  let auth = auth();
  let opened = block_on(layer().open(None));
  begin(&auth, &opened, "/");
  block_on(auth.advance(&opened, creds("alice", "wonder"))).unwrap();

  auth.logout(&opened);
  assert!(opened.cell.identity().is_none());
  assert_eq!(opened.tokens.get("access_token"), None);
  assert!(opened.cell.is_dirty(), "logout persists as a write");
}

#[test]
fn ensure_flow_seeds_only_when_none_is_in_progress() {
  let auth = auth();
  let opened = block_on(layer().open(None));

  block_on(auth.ensure_flow(&opened, "/dash/servers"));
  let step = block_on(auth.advance(&opened, creds("alice", "wonder"))).unwrap();
  assert_eq!(step, Step::Complete { to: "/dash/servers".to_owned(), signed_in: true }, "a page reached directly can still finish the flow");

  begin(&auth, &opened, "/first");
  block_on(auth.ensure_flow(&opened, "/second"));
  let step = block_on(auth.advance(&opened, creds("alice", "wonder"))).unwrap();
  assert_eq!(step, Step::Complete { to: "/first".to_owned(), signed_in: true }, "an existing flow is left alone");
}

/// Redirects to an identity provider, then asks for a second factor, then completes; enrols a
/// method for whoever is signed in. Each step's state names the step it expects.
struct Steps;

impl IdentityProvider for Steps {
  fn begin(&self, start: Start) -> BoxFuture<'_, Result<Next, AuthError>> {
    let next = match start.intent {
      Intent::Enrol => {
        let subject = start.identity.expect("enrolling carries the identity").subject;
        Next::present("webauthn.create", map(&[("challenge", "c1")]), map(&[("step", "enrol"), ("subject", &subject)]))
      }
      _ => Next::redirect(format!("https://idp.example/authorize?method={}", start.method.unwrap_or_default()), map(&[("step", "code")])),
    };
    Box::pin(ready(Ok(next)))
  }

  fn advance(&self, params: ValueMap, state: ValueMap) -> BoxFuture<'_, Result<Next, AuthError>> {
    let step = match state.get("step") {
      Some(Value::Str(step)) => Some(step.to_string()),
      _ => None,
    };
    let next = match step.as_deref() {
      Some("code") if params.get("code") == Some(&Value::str("ok")) => Ok(Next::present("form", map(&[("fields", "otp")]), map(&[("step", "otp")]))),
      Some("otp") if params.get("otp") == Some(&Value::str("123")) => {
        Ok(Next::Complete(AuthOutcome::new(Identity::new("bea".to_owned(), ValueMap::default()), map(&[("access_token", "t-bea")]))))
      }
      Some("enrol") => {
        let subject = match state.get("subject") {
          Some(Value::Str(subject)) => subject.to_string(),
          _ => String::new(),
        };
        let who = if params.get("as") == Some(&Value::str("other")) { "mallory".to_owned() } else { subject };
        Ok(Next::Complete(AuthOutcome::new(Identity::new(who, ValueMap::default()), map(&[("passkey", "pk1")]))))
      }
      _ => Err(AuthError::Denied("wrong answer".to_owned())),
    };
    Box::pin(ready(next))
  }
}

#[test]
fn a_flow_runs_as_many_steps_as_the_provider_asks() {
  let auth = Auth::new(Arc::new(Steps));
  let opened = block_on(layer().open(None));
  let step = block_on(auth.begin(&opened, Start::sign_in("/inbox").method(Some("corp".to_owned())))).unwrap();
  assert_eq!(step, Step::Redirect("https://idp.example/authorize?method=corp".to_owned()));
  let step = block_on(auth.advance(&opened, map(&[("code", "ok")]))).unwrap();
  assert!(matches!(&step, Step::Present { kind, .. } if kind == "form"), "{step:?}");
  assert!(opened.cell.identity().is_none(), "nobody is signed in between steps");
  let step = block_on(auth.advance(&opened, map(&[("otp", "123")]))).unwrap();
  assert_eq!(step, Step::Complete { to: "/inbox".to_owned(), signed_in: true });
  assert_eq!(opened.cell.identity().unwrap().subject, "bea");
}

#[test]
fn enrolling_needs_a_signed_in_session_and_keeps_its_identity() {
  let auth = Auth::new(Arc::new(Steps));
  let opened = block_on(layer().open(None));
  let refused = block_on(auth.begin(&opened, Start::enrol("/settings", Identity::new("x".to_owned(), ValueMap::default())))).unwrap_err();
  assert!(matches!(refused, AuthError::Denied(_)), "an anonymous session enrols nothing");

  opened.cell.set_identity(Some(Identity::new("bea".to_owned(), ValueMap::default())));
  let step = block_on(auth.begin(&opened, Start::enrol("/settings", Identity::new("ignored".to_owned(), ValueMap::default())))).unwrap();
  assert!(matches!(&step, Step::Present { kind, .. } if kind == "webauthn.create"), "{step:?}");
  let step = block_on(auth.advance(&opened, ValueMap::default())).unwrap();
  assert_eq!(step, Step::Complete { to: "/settings".to_owned(), signed_in: false });
  assert_eq!(opened.cell.identity().unwrap().subject, "bea");
  assert_eq!(opened.tokens.get("passkey"), Some(Value::str("pk1")));

  block_on(auth.begin(&opened, Start::enrol("/settings", Identity::new("bea".to_owned(), ValueMap::default())))).unwrap();
  let refused = block_on(auth.advance(&opened, map(&[("as", "other")]))).unwrap_err();
  assert!(matches!(refused, AuthError::Denied(_)), "a method belonging to someone else is refused");
  assert_eq!(opened.cell.identity().unwrap().subject, "bea");
}
