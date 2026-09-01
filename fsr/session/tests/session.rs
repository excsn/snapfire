use std::sync::Arc;
use std::time::Duration;

use futures::executor::block_on;
use snapfire_fsr_core::Value;
use snapfire_fsr_runtime::Identity;
use snapfire_fsr_session::{
  CookieCodec, HmacCodec, MemorySessionStore, SessionConfig, SessionId, Sessions,
};

const KEY: &[u8] = b"test-signing-key-32-bytes-long!!";

fn sessions() -> Sessions {
  Sessions::new(
    Arc::new(MemorySessionStore::new(128, Duration::from_secs(60))),
    KEY,
    SessionConfig::default(),
  )
}

#[test]
fn cookie_codec_round_trips_and_rejects_tampering() {
  let codec = HmacCodec::new(KEY);
  let id = SessionId::generate();
  let value = codec.encode(&id);
  assert_eq!(codec.decode(&value), Some(id.clone()));

  let mut forged = value.clone();
  forged.replace_range(0..1, if &value[0..1] == "a" { "b" } else { "a" });
  assert_eq!(codec.decode(&forged), None, "a tampered id fails verification");

  let other_key = HmacCodec::new(b"a-different-signing-key---------");
  assert_eq!(other_key.decode(&value), None, "a foreign signature fails");
}

#[test]
fn a_session_survives_the_cookie_round_trip() {
  let layer = sessions();

  let first = block_on(layer.open(None));
  assert!(first.fresh);
  first.cell.insert("visits", Value::int(1i64));
  let cookie = block_on(layer.persist(&first)).expect("fresh dirty session sets a cookie");
  assert!(cookie.starts_with("sf_session="));
  assert!(cookie.contains("HttpOnly"));

  let header = cookie.split(';').next().unwrap().to_owned();
  let second = block_on(layer.open(Some(&header)));
  assert!(!second.fresh);
  assert_eq!(second.id, first.id);
  assert_eq!(second.cell.get("visits"), Some(Value::Int(1)));
}

#[test]
fn a_clean_fresh_session_sets_no_cookie() {
  let layer = sessions();
  let opened = block_on(layer.open(None));
  assert_eq!(block_on(layer.persist(&opened)), None, "crawlers never mint sessions");
}

#[test]
fn identity_persists_and_destroy_forgets() {
  let layer = sessions();

  let opened = block_on(layer.open(None));
  opened.cell.set_identity(Some(Identity { subject: "norm".into(), claims: Default::default() }));
  let cookie = block_on(layer.persist(&opened)).unwrap();
  let header = cookie.split(';').next().unwrap().to_owned();

  let back = block_on(layer.open(Some(&header)));
  assert_eq!(back.cell.identity().unwrap().subject, "norm");

  let gone = block_on(layer.destroy(&back));
  assert!(gone.contains("Max-Age=0"), "logout expires the cookie");
  let after = block_on(layer.open(Some(&header)));
  assert!(after.cell.identity().is_none(), "the record is gone even if the cookie replays");
}

#[test]
fn csrf_tokens_bind_to_the_session() {
  let layer = sessions();
  let a = SessionId::generate();
  let b = SessionId::generate();

  let token = layer.csrf_token(&a);
  assert!(layer.verify_csrf(&a, &token));
  assert!(!layer.verify_csrf(&b, &token), "a token never validates for another session");
  assert!(!layer.verify_csrf(&a, "deadbeef"));
}

#[test]
fn tokens_round_trip_but_never_reach_the_cell() {
  let layer = sessions();

  let opened = block_on(layer.open(None));
  opened.tokens.set("access_token", Value::Str("secret-abc".into()));
  let cookie = block_on(layer.persist(&opened)).expect("a token-only write persists and sets the cookie");

  let header = cookie.split(';').next().unwrap().to_owned();
  let back = block_on(layer.open(Some(&header)));
  assert_eq!(back.tokens.get("access_token"), Some(Value::Str("secret-abc".into())));
  assert_eq!(back.cell.get("access_token"), None, "custody: the cell cannot see tokens");

  let ctx = snapfire_fsr_runtime::RequestCtx {
    params: Default::default(),
    session: back.cell.clone(),
    csrf: None,
  };
  assert_eq!(ctx.session.get("access_token"), None, "loaders and actions cannot reach tokens");
  let (data, _) = back.cell.snapshot();
  assert!(data.is_empty());
}

#[test]
fn destroy_forgets_tokens() {
  let layer = sessions();
  let opened = block_on(layer.open(None));
  opened.tokens.set("refresh_token", Value::Str("secret-r".into()));
  let cookie = block_on(layer.persist(&opened)).unwrap();
  let header = cookie.split(';').next().unwrap().to_owned();

  let back = block_on(layer.open(Some(&header)));
  block_on(layer.destroy(&back));
  let after = block_on(layer.open(Some(&header)));
  assert_eq!(after.tokens.get("refresh_token"), None, "the record and its tokens are gone");
}
