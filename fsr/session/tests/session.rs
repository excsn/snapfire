use std::sync::Arc;
use std::time::Duration;

use futures::executor::block_on;
use snapfire_fsr_core::Value;
use snapfire_fsr_runtime::{unix_now, Identity};
use snapfire_fsr_session::{CookieCodec, CsrfScheme, Derived, HmacCodec, MemorySessionStore, Opened, PerSession, SessionConfig, SessionId, SessionRecord, SessionStore, Sessions, SingleUse, constant_time_eq};

const KEY: &[u8] = b"test-signing-key-32-bytes-long!!";

fn sessions() -> Sessions {
  Sessions::new(
    Arc::new(MemorySessionStore::new(128)),
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

  let mut ctx = snapfire_fsr_runtime::RequestCtx::default();
  ctx.session = back.cell.clone();
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

  for store in [MemorySessionStore::sharded(32, 4), supplied] {
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
  let store = MemorySessionStore::new(64);
  let ids: Vec<SessionId> = (0..64).map(|_| SessionId::generate()).collect();
  for id in &ids {
    let record = SessionRecord { expires: unix_now() + 60, ..Default::default() };
    block_on(store.save(id, record)).expect("the memory store saves");
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

  let store = Arc::new(MemorySessionStore::new(16));
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

mod keyring {
  use super::*;
  use snapfire_fsr_session::Keyring;

  #[test]
  fn the_first_key_signs_and_every_key_verifies_until_retired() {
    let ring = Keyring::from_keys([b"new".as_slice(), b"old".as_slice()]);
    let under_old = Keyring::new(b"old").sign(b"x");
    assert_eq!(ring.verify(b"x", &under_old), Some(1));
    assert_eq!(ring.verify(b"x", &ring.sign(b"x")), Some(0));
    assert_eq!(ring.verify(b"x", "zz"), None);
    ring.retire(b"old");
    assert_eq!(ring.verify(b"x", &under_old), None);
    ring.rotate(b"newer");
    assert!(ring.is_current(b"newer"));
    assert_eq!(ring.len(), 2);
    ring.rotate(b"new");
    assert!(ring.is_current(b"new"), "a key already held moves to the front");
    assert_eq!(ring.len(), 2);
    ring.replace([b"only".as_slice()]);
    assert_eq!(ring.len(), 1);
    assert_eq!(Keyring::from_keys(Vec::<&[u8]>::new()).sign(b"x"), "");
  }

  #[test]
  fn a_cookie_under_a_previous_key_opens_stale_and_is_set_again_under_the_current_one() {
    let ring = Arc::new(Keyring::new(b"old"));
    let store = Arc::new(MemorySessionStore::new(16));
    let layer = Sessions::with_codec(store, b"", Arc::new(HmacCodec::over(ring.clone())), SessionConfig::default());
    let opened = block_on(layer.open(None));
    opened.cell.insert("who", Value::str("alice"));
    let cookie = block_on(layer.persist(&opened)).unwrap().unwrap();
    let header = cookie.split(';').next().unwrap().to_owned();
    assert!(!block_on(layer.open(Some(&header))).stale);

    ring.rotate(b"new");
    let back = block_on(layer.open(Some(&header)));
    assert!(!back.fresh && back.stale);
    assert_eq!(back.cell.get("who"), Some(Value::str("alice")));
    let reset = block_on(layer.persist(&back)).unwrap().expect("a stale cookie is set again with nothing written");
    let current = HmacCodec::new(b"new").encode(&back.id);
    assert!(reset.starts_with(&format!("sf_session={current};")), "{reset}");
    assert!(!block_on(layer.open(Some(reset.split(';').next().unwrap()))).stale);

    ring.retire(b"old");
    assert!(block_on(layer.open(Some(&header))).fresh, "a retired key no longer verifies");
  }

  #[test]
  fn a_derived_token_over_the_ring_survives_a_rotation_until_the_key_retires() {
    let ring = Arc::new(Keyring::new(b"old"));
    let scheme = Derived::over(ring.clone());
    let opened = block_on(sessions().open(None));
    let token = scheme.issue(&opened);
    ring.rotate(b"new");
    assert!(scheme.verify(&opened, &token));
    assert_ne!(scheme.issue(&opened), token, "a fresh token is under the new key");
    ring.retire(b"old");
    assert!(!scheme.verify(&opened, &token));
  }
}

/// DEFECTS 4.3: one end on the record, followed by the cookie and the store
/// and moved only by `SessionCell::extend`.
mod lifetime {
  use super::*;
  use std::collections::HashMap;
  use futures::future::BoxFuture;
  use parking_lot::Mutex;
  use snapfire_fsr_session::StoreError;

  fn max_age(cookie: &str) -> u64 {
    cookie
      .split(';')
      .find_map(|part| part.trim().strip_prefix("Max-Age="))
      .and_then(|n| n.parse().ok())
      .expect("a Max-Age")
  }

  fn header(cookie: &str) -> String {
    cookie.split(';').next().unwrap().to_owned()
  }

  /// Keeps every record whatever its end, so the layer's own check is what is
  /// tested.
  #[derive(Default)]
  struct Keeps(Mutex<HashMap<String, SessionRecord>>);

  impl SessionStore for Keeps {
    fn load(&self, id: &SessionId) -> BoxFuture<'_, Option<SessionRecord>> {
      let record = self.0.lock().get(&id.0).cloned();
      Box::pin(async move { record })
    }
    fn save(&self, id: &SessionId, record: SessionRecord) -> BoxFuture<'_, Result<(), StoreError>> {
      self.0.lock().insert(id.0.clone(), record);
      Box::pin(async { Ok(()) })
    }
    fn delete(&self, id: &SessionId) -> BoxFuture<'_, Result<(), StoreError>> {
      self.0.lock().remove(&id.0);
      Box::pin(async { Ok(()) })
    }
  }

  #[test]
  fn a_session_ends_one_ttl_from_open_and_the_cookie_and_the_record_count_down_to_it() {
    let store = Arc::new(MemorySessionStore::new(16));
    let layer = Sessions::new(store.clone(), KEY, SessionConfig { ttl: Duration::from_secs(100), ..SessionConfig::default() });
    let before = unix_now();
    let opened = block_on(layer.open(None));
    assert!((before + 100..=before + 101).contains(&opened.cell.expires()));
    opened.cell.insert("who", Value::str("alice"));
    let cookie = block_on(layer.persist(&opened)).unwrap().unwrap();
    assert!((99..=100).contains(&max_age(&cookie)), "{cookie}");
    let record = block_on(store.load(&opened.id)).unwrap();
    assert_eq!(record.expires, opened.cell.expires());

    let back = block_on(layer.open(Some(&header(&cookie))));
    assert_eq!(back.cell.expires(), opened.cell.expires(), "a read leaves the end where it was");
    assert_eq!(block_on(layer.persist(&back)).unwrap(), None, "a read writes nothing");
  }

  #[test]
  fn an_extension_saves_and_sets_the_cookie_again_when_nothing_else_changed() {
    let store = Arc::new(MemorySessionStore::new(16));
    let layer = Sessions::new(store.clone(), KEY, SessionConfig { ttl: Duration::from_secs(100), ..SessionConfig::default() });
    let opened = block_on(layer.open(None));
    opened.cell.insert("who", Value::str("alice"));
    let cookie = block_on(layer.persist(&opened)).unwrap().unwrap();

    let back = block_on(layer.open(Some(&header(&cookie))));
    back.cell.extend(Duration::from_secs(1000));
    assert!(!back.cell.is_dirty());
    let again = block_on(layer.persist(&back)).unwrap().expect("an extension sets the cookie");
    assert!((999..=1000).contains(&max_age(&again)), "{again}");
    let record = block_on(store.load(&back.id)).unwrap();
    assert_eq!(record.expires, back.cell.expires());
    assert_eq!(record.data.get("who"), Some(&Value::str("alice")), "the record kept its contents");

    let later = block_on(layer.open(Some(&header(&again))));
    assert_eq!(later.cell.expires(), back.cell.expires());
  }

  #[test]
  fn a_record_past_its_end_opens_as_gone_whatever_the_store_kept() {
    let store = Arc::new(Keeps::default());
    let layer = Sessions::new(store.clone(), KEY, SessionConfig::default());
    let opened = block_on(layer.open(None));
    opened.cell.insert("who", Value::str("alice"));
    let cookie = block_on(layer.persist(&opened)).unwrap().unwrap();
    let mut record = block_on(store.load(&opened.id)).unwrap();
    record.expires = unix_now() - 1;
    block_on(store.save(&opened.id, record)).unwrap();

    let back = block_on(layer.open(Some(&header(&cookie))));
    assert_eq!(back.cell.get("who"), None);
    assert!(!back.fresh);
    assert!(block_on(store.load(&opened.id)).is_none(), "the expired record was deleted");
    assert!(back.cell.expires() > unix_now(), "the request carries on with a new end");
  }

  #[test]
  fn the_memory_store_keeps_a_record_until_its_end_and_not_past_it() {
    let store = MemorySessionStore::new(16);
    let live = SessionId::generate();
    let gone = SessionId::generate();
    block_on(store.save(&live, SessionRecord { expires: unix_now() + 1, ..Default::default() })).unwrap();
    block_on(store.save(&gone, SessionRecord { expires: unix_now(), ..Default::default() })).unwrap();
    assert!(block_on(store.load(&live)).is_some());
    assert!(block_on(store.load(&gone)).is_none(), "a record already past its end is not stored");
    std::thread::sleep(Duration::from_millis(1100));
    assert!(block_on(store.load(&live)).is_none(), "the store drops the record at its end");
  }
}
