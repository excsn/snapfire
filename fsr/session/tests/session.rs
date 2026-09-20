use std::sync::Arc;
use std::time::Duration;

use futures::executor::block_on;
use snapfire_fsr_core::Value;
use snapfire_fsr_runtime::Identity;
use snapfire_fsr_session::{CookieCodec, CsrfScheme, Derived, HmacCodec, MemorySessionStore, Opened, PerSession, SessionConfig, SessionId, SessionStore, Sessions, SingleUse, constant_time_eq};

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
  let cookie = block_on(layer.persist(&first)).unwrap().expect("fresh dirty session sets a cookie");
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
  assert_eq!(block_on(layer.persist(&opened)).unwrap(), None, "crawlers never mint sessions");
}

#[test]
fn identity_persists_and_destroy_forgets() {
  let layer = sessions();

  let opened = block_on(layer.open(None));
  opened.cell.set_identity(Some(Identity { subject: "norm".into(), claims: Default::default() }));
  let cookie = block_on(layer.persist(&opened)).unwrap().unwrap();
  let header = cookie.split(';').next().unwrap().to_owned();

  let back = block_on(layer.open(Some(&header)));
  assert_eq!(back.cell.identity().unwrap().subject, "norm");

  let gone = block_on(layer.destroy(&back)).unwrap();
  assert!(gone.contains("Max-Age=0"), "logout expires the cookie");
  let after = block_on(layer.open(Some(&header)));
  assert!(after.cell.identity().is_none(), "the record is gone even if the cookie replays");
}

#[test]
fn csrf_tokens_bind_to_the_session_and_verify_once() {
  let layer = sessions();
  let a = block_on(layer.open(None));
  let b = block_on(layer.open(None));

  let token = layer.csrf_token(&a);
  assert!(!layer.verify_csrf(&b, &token), "a token never validates for another session");
  assert!(!layer.verify_csrf(&a, "deadbeef"));
  assert!(layer.verify_csrf(&a, &token));
  assert!(!layer.verify_csrf(&a, &token), "single use is the default: a second post with the same token is refused");
  assert!(a.csrf.is_dirty(), "the scheme's state is the session's to persist");
}

#[test]
fn single_use_keeps_a_bounded_number_of_outstanding_tokens() {
  let layer = sessions().with_csrf(Arc::new(SingleUse::new(2)));
  let opened = block_on(layer.open(None));
  let first = layer.csrf_token(&opened);
  let second = layer.csrf_token(&opened);
  let third = layer.csrf_token(&opened);
  assert!(!layer.verify_csrf(&opened, &first), "the oldest was dropped when a third was minted");
  assert!(layer.verify_csrf(&opened, &second));
  assert!(layer.verify_csrf(&opened, &third));
  let fourth = layer.csrf_token(&opened);
  layer.rotate_csrf(&opened);
  assert!(!layer.verify_csrf(&opened, &fourth), "rotation drops every outstanding token");
  assert!(layer.csrf_scheme().memo(&opened).is_none(), "a per-render token keeps its subtree out of the render memo");
}

#[test]
fn a_per_session_token_is_stable_until_rotated() {
  let layer = sessions().with_csrf(Arc::new(PerSession::new()));
  let opened = block_on(layer.open(None));
  let token = layer.csrf_token(&opened);
  assert_eq!(layer.csrf_token(&opened), token);
  assert!(layer.verify_csrf(&opened, &token));
  assert!(layer.verify_csrf(&opened, &token), "not single use");
  assert_eq!(layer.csrf_scheme().memo(&opened).as_deref(), Some(token.as_str()));
  layer.rotate_csrf(&opened);
  assert!(!layer.verify_csrf(&opened, &token));
  assert_ne!(layer.csrf_token(&opened), token);
}

#[test]
fn a_derived_token_is_the_hmac_of_the_id_and_survives_rotation() {
  let layer = sessions().with_csrf(Arc::new(Derived::new(b"k")));
  let opened = block_on(layer.open(None));
  let token = layer.csrf_token(&opened);
  layer.rotate_csrf(&opened);
  assert!(layer.verify_csrf(&opened, &token));
  assert!(!opened.csrf.is_dirty(), "nothing is stored");
  assert_eq!(layer.csrf_scheme().memo(&opened).as_deref(), Some(token.as_str()));
}

#[test]
fn a_scheme_of_the_callers_own_is_asked_at_both_ends() {
  struct Fixed;
  impl CsrfScheme for Fixed {
    fn issue(&self, _: &Opened) -> String {
      "fixed".to_owned()
    }
    fn verify(&self, _: &Opened, token: &str) -> bool {
      constant_time_eq(token, "fixed")
    }
    fn memo(&self, _: &Opened) -> Option<String> {
      Some("fixed".to_owned())
    }
  }
  let layer = sessions().with_csrf(Arc::new(Fixed));
  let opened = block_on(layer.open(None));
  assert_eq!(layer.csrf_token(&opened), "fixed");
  assert!(layer.verify_csrf(&opened, "fixed"));
  assert!(!layer.verify_csrf(&opened, "other"));
}

#[test]
fn the_csrf_state_round_trips_through_the_store_and_dies_with_the_session() {
  let layer = sessions();
  let opened = block_on(layer.open(None));
  let token = layer.csrf_token(&opened);
  let header = block_on(layer.persist(&opened)).unwrap().map(|c| c.split(';').next().unwrap().to_owned()).expect("a minted token establishes the session");
  let back = block_on(layer.open(Some(&header)));
  assert!(layer.verify_csrf(&back, &token), "the outstanding token survived the round trip");
  let again = layer.csrf_token(&back);
  block_on(layer.destroy(&back)).unwrap();
  assert!(!layer.verify_csrf(&back, &again));
}

#[test]
fn tokens_round_trip_but_never_reach_the_cell() {
  let layer = sessions();

  let opened = block_on(layer.open(None));
  opened.tokens.set("access_token", Value::Str("secret-abc".into()));
  let cookie = block_on(layer.persist(&opened)).unwrap().expect("a token-only write persists and sets the cookie");

  let header = cookie.split(';').next().unwrap().to_owned();
  let back = block_on(layer.open(Some(&header)));
  assert_eq!(back.tokens.get("access_token"), Some(Value::Str("secret-abc".into())));
  assert_eq!(back.cell.get("access_token"), None, "custody: the cell cannot see tokens");

  let ctx = snapfire_fsr_runtime::RequestCtx {
    session: back.cell.clone(),
    ..Default::default()
  };
  assert_eq!(ctx.session.get("access_token"), None, "loaders and actions cannot reach tokens");
  let (data, _) = back.cell.snapshot();
  assert!(data.is_empty());
}

#[test]
fn destroy_empties_the_cells_it_was_given() {
  let layer = sessions();
  let opened = block_on(layer.open(None));
  opened.cell.set_identity(Some(Identity { subject: "norm".into(), claims: Default::default() }));
  opened.cell.insert("visits", Value::Int(3));
  opened.tokens.set("access_token", Value::Str("secret-abc".into()));
  block_on(layer.destroy(&opened)).unwrap();
  assert!(opened.cell.identity().is_none(), "the identity is gone for the rest of the request");
  assert_eq!(opened.cell.get("visits"), None);
  assert_eq!(opened.tokens.get("access_token"), None);
}

#[test]
fn destroy_forgets_tokens() {
  let layer = sessions();
  let opened = block_on(layer.open(None));
  opened.tokens.set("refresh_token", Value::Str("secret-r".into()));
  let cookie = block_on(layer.persist(&opened)).unwrap().unwrap();
  let header = cookie.split(';').next().unwrap().to_owned();

  let back = block_on(layer.open(Some(&header)));
  block_on(layer.destroy(&back)).unwrap();
  let after = block_on(layer.open(Some(&header)));
  assert_eq!(after.tokens.get("refresh_token"), None, "the record and its tokens are gone");
}

#[test]
fn a_store_can_be_tuned_or_supplied_whole() {
  let supplied = MemorySessionStore::with_cache(
    fibre_cache::CacheBuilder::default()
      .capacity(32)
      .time_to_idle(Duration::from_secs(60))
      .shards(2)
      .build()
      .unwrap(),
  );

  for store in [MemorySessionStore::sharded(32, Duration::from_secs(60), 4), supplied] {
    let layer = Sessions::new(Arc::new(store), KEY, SessionConfig::default());
    let opened = block_on(layer.open(None));
    opened.cell.insert("visits", Value::int(1i64));
    let cookie = block_on(layer.persist(&opened)).unwrap().unwrap();

    let header = cookie.split(';').next().unwrap().to_owned();
    let back = block_on(layer.open(Some(&header)));
    assert_eq!(back.cell.get("visits"), Some(Value::Int(1)));
  }
}

#[test]
fn capacity_is_accounted_across_shards_not_divided_by_them() {
  let store = MemorySessionStore::new(64, Duration::from_secs(60));
  let ids: Vec<SessionId> = (0..64).map(|_| SessionId::generate()).collect();
  for id in &ids {
    block_on(store.save(id, Default::default())).expect("the memory store saves");
  }
  let resident = ids.iter().filter(|id| block_on(store.load(id)).is_some()).count();
  assert_eq!(resident, 64, "a small store under the default shard count keeps everything");
}

/// DEFECTS 2.1: `CookieCodec` is a seam only if the layer will take one.
#[test]
fn a_codec_of_the_caller_s_own_carries_the_session() {
  struct Plain;
  impl snapfire_fsr_session::CookieCodec for Plain {
    fn encode(&self, id: &snapfire_fsr_session::SessionId) -> String {
      format!("plain:{}", id.0)
    }
    fn decode(&self, value: &str) -> Option<snapfire_fsr_session::SessionId> {
      value.strip_prefix("plain:").map(|id| snapfire_fsr_session::SessionId(id.to_owned()))
    }
  }

  let store = Arc::new(MemorySessionStore::new(16, Duration::from_secs(60)));
  let layer = Sessions::with_codec(store, b"key", Arc::new(Plain), SessionConfig::default());
  let opened = block_on(layer.open(None));
  opened.cell.insert("who", Value::str("alice"));
  let cookie = block_on(layer.persist(&opened)).unwrap().unwrap();
  assert!(cookie.contains("=plain:"), "the caller's codec wrote the cookie: {cookie}");

  let header = cookie.split(';').next().unwrap().to_owned();
  let back = block_on(layer.open(Some(&header)));
  assert!(!back.fresh, "and read it back");
  assert_eq!(back.cell.get("who"), Some(Value::str("alice")));
}

/// DEFECTS 3.7: the cookie header is RFC 6265, not a prefix match.
#[test]
fn a_cookie_is_read_by_name_unquoted_and_decoded() {
  let layer = sessions();
  let opened = block_on(layer.open(None));
  opened.cell.insert("who", Value::str("alice"));
  let cookie = block_on(layer.persist(&opened)).unwrap().unwrap();
  let value = cookie.split(';').next().unwrap().split_once('=').unwrap().1.to_owned();

  for header in [
    format!("sf_session={value}"),
    format!("other=1; sf_session={value}; last=2"),
    format!("sf_session=\"{value}\""),
    format!("sf_session_old=junk; sf_session={value}"),
    format!("sf_session={}", value.replace('.', "%2E")),
  ] {
    let back = block_on(layer.open(Some(&header)));
    assert!(!back.fresh, "the session was not read from `{header}`");
    assert_eq!(back.cell.get("who"), Some(Value::str("alice")));
  }

  let wrong = block_on(layer.open(Some(&format!("sf_session_old={value}"))));
  assert!(wrong.fresh, "a longer name must not answer for this one");
}
