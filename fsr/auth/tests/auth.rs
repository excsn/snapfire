use std::sync::Arc;
use std::time::Duration;

use futures::executor::block_on;
use snapfire_fsr_auth::{Auth, AuthError, DevProvider};
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_session::{MemorySessionStore, Opened, SessionConfig, Sessions};

const KEY: &[u8] = b"test-signing-key-32-bytes-long!!";

fn layer() -> Sessions {
  Sessions::new(
    Arc::new(MemorySessionStore::new(128, Duration::from_secs(60))),
    KEY,
    SessionConfig::default(),
  )
}

fn auth() -> Auth {
  Auth::new(Arc::new(DevProvider::new("/login").user("alice", "wonder")))
}

fn creds(user: &str, password: &str) -> ValueMap {
  let mut params = ValueMap::new();
  params.insert("user".to_owned(), Value::Str(user.to_owned()));
  params.insert("password".to_owned(), Value::Str(password.to_owned()));
  params
}

fn begin(auth: &Auth, opened: &Opened, return_to: &str) -> String {
  block_on(auth.login(opened, return_to))
}

#[test]
fn login_redirects_to_the_app_login_page_with_the_destination() {
  let opened = block_on(layer().open(None));
  let redirect = begin(&auth(), &opened, "/dash/servers");
  assert_eq!(redirect, "/login?return_to=%2Fdash%2Fservers");
}

#[test]
fn callback_sets_identity_and_tokens_and_returns_the_destination() {
  let auth = auth();
  let opened = block_on(layer().open(None));
  begin(&auth, &opened, "/dash/servers");

  let back = block_on(auth.callback(&opened, creds("alice", "wonder"))).unwrap();
  assert_eq!(back, "/dash/servers");
  assert_eq!(opened.cell.identity().unwrap().subject, "alice");
  assert_eq!(opened.tokens.get("access_token"), Some(Value::Str("dev-token-alice".into())));
  assert_eq!(opened.cell.get("access_token"), None, "tokens stay in custody");
}

#[test]
fn callback_without_a_begun_flow_is_invalid() {
  let opened = block_on(layer().open(None));
  let err = block_on(auth().callback(&opened, creds("alice", "wonder"))).unwrap_err();
  assert!(matches!(err, AuthError::Invalid(_)));
  assert_eq!(err.http_status(), 400);
}

#[test]
fn wrong_password_is_denied_and_consumes_the_flow() {
  let auth = auth();
  let opened = block_on(layer().open(None));
  begin(&auth, &opened, "/");

  let err = block_on(auth.callback(&opened, creds("alice", "nope"))).unwrap_err();
  assert!(matches!(err, AuthError::Denied(_)));
  assert_eq!(err.http_status(), 403);
  assert!(opened.cell.identity().is_none());

  let err = block_on(auth.callback(&opened, creds("alice", "wonder"))).unwrap_err();
  assert!(matches!(err, AuthError::Invalid(_)), "a failed flow cannot be replayed");
}

#[test]
fn logout_forgets_identity_and_custody() {
  let auth = auth();
  let opened = block_on(layer().open(None));
  begin(&auth, &opened, "/");
  block_on(auth.callback(&opened, creds("alice", "wonder"))).unwrap();

  auth.logout(&opened);
  assert!(opened.cell.identity().is_none());
  assert_eq!(opened.tokens.get("access_token"), None);
  assert!(opened.cell.is_dirty(), "logout persists as a write");
}
