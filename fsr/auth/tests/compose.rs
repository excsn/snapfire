use std::sync::Arc;

use futures::executor::block_on;
use futures_util::future::{ready, BoxFuture};
use snapfire_fsr_auth::{Auth, AuthError, AuthOutcome, DevProvider, IdentityProvider, Methods, Next, Sequence, Start, Step};
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_runtime::Identity;
use snapfire_fsr_session::{MemorySessionStore, Opened, SessionConfig, Sessions};

const KEY: &[u8] = b"test-signing-key-32-bytes-long!!";

fn opened() -> Opened {
  block_on(Sessions::new(Arc::new(MemorySessionStore::new(128)), KEY, SessionConfig::default()).open(None))
}

fn map(pairs: &[(&str, &str)]) -> ValueMap {
  pairs.iter().map(|(k, v)| ((*k).to_owned(), Value::str((*v).to_owned()))).collect()
}

/// A code sent elsewhere: presents a notice, completes for whoever `begin` named when the answer is `123`.
struct Code;

impl IdentityProvider for Code {
  fn begin(&self, start: Start) -> BoxFuture<'_, Result<Next, AuthError>> {
    let subject = start.identity.map(|identity| identity.subject).unwrap_or_else(|| "visitor".to_owned());
    Box::pin(ready(Ok(Next::present("notice", map(&[("sent", "sms")]), map(&[("subject", &subject)])))))
  }

  fn advance(&self, params: ValueMap, state: ValueMap) -> BoxFuture<'_, Result<Next, AuthError>> {
    let subject = match state.get("subject") {
      Some(Value::Str(subject)) => subject.to_string(),
      _ => String::new(),
    };
    let next = match params.get("code") {
      Some(Value::Str(code)) if code.as_str() == "123" => Ok(Next::Complete(AuthOutcome::new(Identity::new(subject, ValueMap::default()), map(&[("factor", "sms")])))),
      Some(Value::Str(code)) if code.as_str() == "666" => Ok(Next::Complete(AuthOutcome::new(Identity::new("mallory".to_owned(), ValueMap::default()), ValueMap::default()))),
      _ => Err(AuthError::Denied("wrong code".to_owned())),
    };
    Box::pin(ready(next))
  }

  fn refresh(&self, _tokens: ValueMap) -> BoxFuture<'_, Result<Option<ValueMap>, AuthError>> {
    Box::pin(ready(Ok(Some(map(&[("access_token", "from-code")])))))
  }
}

fn methods() -> Methods {
  Methods::new().method("password", Arc::new(DevProvider::new().user("alice", "wonder"))).method("code", Arc::new(Code))
}

#[test]
fn several_methods_offer_a_choice_and_run_the_one_chosen() {
  let auth = Auth::new(Arc::new(methods()));
  let opened = opened();
  match block_on(auth.begin(&opened, Start::sign_in("/inbox"))).unwrap() {
    Step::Present { kind, data } => {
      assert_eq!(kind, "choose");
      assert_eq!(data.get("methods"), Some(&Value::seq(vec![Value::str("password"), Value::str("code")])));
    }
    other => panic!("{other:?}"),
  }
  let step = block_on(auth.advance(&opened, map(&[("method", "password")]))).unwrap();
  assert!(matches!(&step, Step::Present { kind, .. } if kind == "form"), "{step:?}");
  let step = block_on(auth.advance(&opened, map(&[("user", "alice"), ("password", "wonder")]))).unwrap();
  assert_eq!(step, Step::Complete { to: "/inbox".to_owned(), signed_in: true });
  assert_eq!(opened.cell.identity().unwrap().subject, "alice");
}

#[test]
fn a_method_named_at_the_start_runs_directly_and_an_unknown_one_is_invalid() {
  let auth = Auth::new(Arc::new(methods()));
  let opened = opened();
  let step = block_on(auth.begin(&opened, Start::sign_in("/").method(Some("code".to_owned())))).unwrap();
  assert!(matches!(&step, Step::Present { kind, .. } if kind == "notice"), "{step:?}");
  assert_eq!(block_on(auth.advance(&opened, map(&[("code", "123")]))).unwrap(), Step::Complete { to: "/".to_owned(), signed_in: true });

  let refused = block_on(auth.begin(&opened, Start::sign_in("/").method(Some("carrier-pigeon".to_owned())))).unwrap_err();
  assert!(matches!(&refused, AuthError::Invalid(message) if message.contains("password") && message.contains("code")), "{refused:?}");
}

#[test]
fn one_method_needs_no_choice() {
  let auth = Auth::new(Arc::new(Methods::new().method("password", Arc::new(DevProvider::new().user("alice", "wonder")))));
  let step = block_on(auth.begin(&opened(), Start::sign_in("/"))).unwrap();
  assert!(matches!(&step, Step::Present { kind, .. } if kind == "form"), "{step:?}");
}

#[test]
fn methods_refresh_through_whichever_method_renews() {
  let renewed = block_on(methods().refresh(ValueMap::default())).unwrap();
  assert_eq!(renewed, Some(map(&[("access_token", "from-code")])));
}

#[test]
fn a_second_factor_follows_the_first_for_the_same_subject() {
  let auth = Auth::new(Arc::new(Sequence::new(Arc::new(DevProvider::new().user("alice", "wonder")), Arc::new(Code))));
  let opened = opened();
  block_on(auth.begin(&opened, Start::sign_in("/vault"))).unwrap();
  let step = block_on(auth.advance(&opened, map(&[("user", "alice"), ("password", "wonder")]))).unwrap();
  assert!(matches!(&step, Step::Present { kind, .. } if kind == "notice"), "the first factor passed, the second is asked: {step:?}");
  assert!(opened.cell.identity().is_none(), "nobody is signed in after one factor");
  let step = block_on(auth.advance(&opened, map(&[("code", "123")]))).unwrap();
  assert_eq!(step, Step::Complete { to: "/vault".to_owned(), signed_in: true });
  assert_eq!(opened.cell.identity().unwrap().subject, "alice");
  assert_eq!(opened.tokens.get("access_token"), Some(Value::str("dev-token-alice")), "the first factor's tokens");
  assert_eq!(opened.tokens.get("factor"), Some(Value::str("sms")), "and the second's");
}

#[test]
fn a_second_factor_for_someone_else_is_denied() {
  let auth = Auth::new(Arc::new(Sequence::new(Arc::new(DevProvider::new().user("alice", "wonder")), Arc::new(Code))));
  let opened = opened();
  block_on(auth.begin(&opened, Start::sign_in("/"))).unwrap();
  block_on(auth.advance(&opened, map(&[("user", "alice"), ("password", "wonder")]))).unwrap();
  let refused = block_on(auth.advance(&opened, map(&[("code", "666")]))).unwrap_err();
  assert!(matches!(refused, AuthError::Denied(_)), "{refused:?}");
  assert!(opened.cell.identity().is_none());
}

#[test]
fn enrolling_through_a_sequence_enrols_its_second_factor() {
  let auth = Auth::new(Arc::new(Sequence::new(Arc::new(DevProvider::new().user("alice", "wonder")), Arc::new(Code))));
  let opened = opened();
  opened.cell.set_identity(Some(Identity::new("alice".to_owned(), ValueMap::default())));
  let step = block_on(auth.begin(&opened, Start::enrol("/settings", Identity::new("alice".to_owned(), ValueMap::default())))).unwrap();
  assert!(matches!(&step, Step::Present { kind, .. } if kind == "notice"), "{step:?}");
  assert_eq!(block_on(auth.advance(&opened, map(&[("code", "123")]))).unwrap(), Step::Complete { to: "/settings".to_owned(), signed_in: false });
}
