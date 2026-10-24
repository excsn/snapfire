use std::sync::Arc;

use futures::executor::block_on;
use snapfire_fsr_core::Value;
use snapfire_fsr_runtime::{unix_now, Identity};
use snapfire_fsr_session::{Keyring, SealedCookie, SessionConfig, SessionId, SessionRecord, Sessions, COOKIE_LIMIT};

const KEY: &[u8] = b"test-signing-key-32-bytes-long!!";
const NEXT: &[u8] = b"the-next-signing-key-32-bytes!!!";

fn layer(ring: Arc<Keyring>) -> Sessions {
  Sessions::sealed(Arc::new(SealedCookie::over(ring)), SessionConfig::default())
}

fn header(cookie: &str) -> String {
  cookie.split(';').next().unwrap().to_owned()
}

#[test]
fn the_record_rides_in_the_cookie_and_the_browser_cannot_read_it() {
  let layer = layer(Arc::new(Keyring::new(KEY)));
  let first = block_on(layer.open(None));
  first.cell.insert("who", Value::str("alice-in-the-cookie"));
  first.cell.set_identity(Some(Identity::new("alice".into(), Default::default())));
  first.tokens.set("access", Value::str("bearer-secret-token"));
  let cookie = block_on(layer.persist(&first)).unwrap().expect("a written session sets its cookie");
  assert!(cookie.starts_with("sf_session=") && cookie.contains("HttpOnly"), "{cookie}");
  assert!(!cookie.contains("alice") && !cookie.contains("bearer-secret-token"), "the record is encrypted: {cookie}");

  let back = block_on(layer.open(Some(&header(&cookie))));
  assert!(!back.fresh && !back.stale);
  assert_eq!(back.id, first.id);
  assert_eq!(back.cell.get("who"), Some(Value::str("alice-in-the-cookie")));
  assert_eq!(back.cell.identity().unwrap().subject, "alice");
  assert_eq!(back.tokens.get("access"), Some(Value::str("bearer-secret-token")));
}

#[test]
fn a_changed_or_foreign_cookie_opens_a_fresh_session() {
  let layer = layer(Arc::new(Keyring::new(KEY)));
  let first = block_on(layer.open(None));
  first.cell.insert("n", Value::int(1i64));
  let value = header(&block_on(layer.persist(&first)).unwrap().unwrap());
  let at = value.len() - 3;
  let flipped = if &value[at..at + 1] == "A" { "B" } else { "A" };
  let mut tampered = value.clone();
  tampered.replace_range(at..at + 1, flipped);
  assert!(block_on(layer.open(Some(&tampered))).fresh);
  let foreign = self::layer(Arc::new(Keyring::new(NEXT)));
  assert!(block_on(foreign.open(Some(&value))).fresh, "another key cannot open it");
}

#[test]
fn a_save_past_what_a_browser_keeps_is_refused_naming_the_size() {
  let layer = layer(Arc::new(Keyring::new(KEY)));
  let opened = block_on(layer.open(None));
  opened.cell.insert("notes", Value::str("x".repeat(COOKIE_LIMIT)));
  let refused = block_on(layer.persist(&opened)).unwrap_err().to_string();
  assert!(refused.contains(&COOKIE_LIMIT.to_string()) && refused.contains("bytes"), "{refused}");
}

#[test]
fn a_rotated_key_still_opens_the_cookie_and_the_next_save_moves_it() {
  let ring = Arc::new(Keyring::new(KEY));
  let layer = layer(ring.clone());
  let first = block_on(layer.open(None));
  first.cell.insert("n", Value::int(1i64));
  let old = header(&block_on(layer.persist(&first)).unwrap().unwrap());

  ring.rotate(NEXT);
  let opened = block_on(layer.open(Some(&old)));
  assert!(!opened.fresh && opened.stale, "sealed under a key that no longer seals");
  let moved = header(&block_on(layer.persist(&opened)).unwrap().expect("a stale cookie is set again"));
  ring.retire(KEY);
  assert!(block_on(layer.open(Some(&old))).fresh, "a retired key opens nothing");
  assert_eq!(block_on(layer.open(Some(&moved))).cell.get("n"), Some(Value::Int(1)));
}

#[test]
fn an_expired_record_opens_a_fresh_session() {
  let sealer = SealedCookie::over(Arc::new(Keyring::new(KEY)));
  let mut record = SessionRecord::default();
  record.data.insert("n".to_owned(), Value::int(1i64));
  record.expires = unix_now() - 1;
  let value = sealer.seal(&SessionId::generate(), &record);
  let layer = Sessions::sealed(Arc::new(sealer), SessionConfig::default());
  assert!(block_on(layer.open(Some(&format!("sf_session={value}")))).fresh);
}

#[test]
fn destroy_expires_the_cookie() {
  let layer = layer(Arc::new(Keyring::new(KEY)));
  let opened = block_on(layer.open(None));
  opened.cell.insert("n", Value::int(1i64));
  block_on(layer.persist(&opened)).unwrap();
  assert!(block_on(layer.destroy(&opened)).unwrap().contains("Max-Age=0"));
}

#[test]
fn a_record_reads_back_from_its_encoding() {
  let mut record = SessionRecord::default();
  record.data.insert("who".to_owned(), Value::str("alice"));
  record.identity = Some(Identity::new("alice".into(), Default::default()));
  record.expires = 1_800_000_000;
  let back = SessionRecord::decode(&record.encode(), 7).unwrap();
  assert_eq!(back.data.get("who"), Some(&Value::str("alice")));
  assert_eq!(back.identity.unwrap().subject, "alice");
  assert_eq!(back.expires, 1_800_000_000);
  assert_eq!(SessionRecord::decode(r#"{"data":{}}"#, 7).unwrap().expires, 7, "a record encoded before records carried an end");
}
