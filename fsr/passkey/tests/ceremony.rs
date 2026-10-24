use std::sync::Arc;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use ciborium::value::Value as Cbor;
use futures::executor::block_on;
use ring::rand::SystemRandom;
use ring::signature::{EcdsaKeyPair, KeyPair, ECDSA_P256_SHA256_ASN1_SIGNING};
use serde_json::{json, Value as Json};
use snapfire_fsr_auth::{AuthError, IdentityProvider, Next, Start};
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_passkey::{FilePasskeys, MemoryPasskeys, PasskeyConfig, PasskeyStore, Passkeys};
use snapfire_fsr_runtime::Identity;

const ORIGIN: &str = "https://shop.example";
const RP_ID: &str = "shop.example";

/// A software authenticator holding one P-256 credential, which signs the way a browser's does.
struct Authenticator {
  id: Vec<u8>,
  pair: EcdsaKeyPair,
  counter: u32,
  rp_id: String,
  origin: String,
}

fn b64(bytes: &[u8]) -> String {
  URL_SAFE_NO_PAD.encode(bytes)
}

fn cbor(value: &Cbor) -> Vec<u8> {
  let mut out = Vec::new();
  ciborium::ser::into_writer(value, &mut out).unwrap();
  out
}

fn to_value(json: &Json) -> Value {
  match json {
    Json::Null => Value::Null,
    Json::Bool(b) => Value::Bool(*b),
    Json::Number(n) => Value::int(n.as_i64().unwrap_or_default()),
    Json::String(s) => Value::str(s.clone()),
    Json::Array(items) => Value::seq(items.iter().map(to_value).collect::<Vec<_>>()),
    Json::Object(map) => Value::Map(map.iter().map(|(k, v)| (k.clone(), to_value(v))).collect()),
  }
}

fn challenge_of(data: &ValueMap) -> String {
  let Some(Value::Map(public)) = data.get("publicKey") else { panic!("no publicKey") };
  match public.get("challenge") {
    Some(Value::Str(challenge)) => challenge.to_string(),
    other => panic!("{other:?}"),
  }
}

fn answer(credential: Json) -> ValueMap {
  let mut params = ValueMap::default();
  params.insert("credential".to_owned(), to_value(&credential));
  params
}

impl Authenticator {
  fn new() -> Self {
    let rng = SystemRandom::new();
    let pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, &rng).unwrap();
    let pair = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, pkcs8.as_ref(), &rng).unwrap();
    let id: Vec<u8> = (0..16).map(|_| rand_byte()).collect();
    Self { id, pair, counter: 0, rp_id: RP_ID.to_owned(), origin: ORIGIN.to_owned() }
  }

  fn rp_hash(&self) -> Vec<u8> {
    ring::digest::digest(&ring::digest::SHA256, self.rp_id.as_bytes()).as_ref().to_vec()
  }

  fn create(&self, data: &ValueMap) -> ValueMap {
    let client = json!({ "type": "webauthn.create", "challenge": challenge_of(data), "origin": self.origin }).to_string();
    let public = self.pair.public_key().as_ref();
    let key = Cbor::Map(vec![
      (Cbor::Integer(1.into()), Cbor::Integer(2.into())),
      (Cbor::Integer(3.into()), Cbor::Integer((-7).into())),
      (Cbor::Integer((-1).into()), Cbor::Integer(1.into())),
      (Cbor::Integer((-2).into()), Cbor::Bytes(public[1..33].to_vec())),
      (Cbor::Integer((-3).into()), Cbor::Bytes(public[33..].to_vec())),
    ]);
    let mut auth = self.rp_hash();
    auth.push(0x45);
    auth.extend(self.counter.to_be_bytes());
    auth.extend([0u8; 16]);
    auth.extend((self.id.len() as u16).to_be_bytes());
    auth.extend(&self.id);
    auth.extend(cbor(&key));
    let object = Cbor::Map(vec![(Cbor::Text("fmt".into()), Cbor::Text("none".into())), (Cbor::Text("attStmt".into()), Cbor::Map(vec![])), (Cbor::Text("authData".into()), Cbor::Bytes(auth))]);
    answer(json!({ "id": b64(&self.id), "rawId": b64(&self.id), "type": "public-key", "response": { "clientDataJSON": b64(client.as_bytes()), "attestationObject": b64(&cbor(&object)) } }))
  }

  fn get(&mut self, data: &ValueMap, subject: &str) -> ValueMap {
    self.counter += 1;
    let client = json!({ "type": "webauthn.get", "challenge": challenge_of(data), "origin": self.origin }).to_string();
    let mut auth = self.rp_hash();
    auth.push(0x05);
    auth.extend(self.counter.to_be_bytes());
    let mut message = auth.clone();
    message.extend(ring::digest::digest(&ring::digest::SHA256, client.as_bytes()).as_ref());
    let signature = self.pair.sign(&SystemRandom::new(), &message).unwrap();
    answer(json!({ "id": b64(&self.id), "rawId": b64(&self.id), "type": "public-key", "response": { "clientDataJSON": b64(client.as_bytes()), "authenticatorData": b64(&auth), "signature": b64(signature.as_ref()), "userHandle": b64(subject.as_bytes()) } }))
  }
}

fn rand_byte() -> u8 {
  let mut byte = [0u8; 1];
  ring::rand::SecureRandom::fill(&SystemRandom::new(), &mut byte).unwrap();
  byte[0]
}

fn passkeys(store: Arc<dyn PasskeyStore>) -> Passkeys {
  Passkeys::new(PasskeyConfig::new(RP_ID, "Shop", ORIGIN), store)
}

fn ada() -> Identity {
  let mut claims = ValueMap::default();
  claims.insert("email".to_owned(), Value::str("ada@shop.example"));
  Identity::new("ada".to_owned(), claims)
}

fn present(next: Next) -> (String, ValueMap, ValueMap) {
  match next {
    Next::Present { kind, data, state } => (kind, data, state),
    _ => panic!("a present step"),
  }
}

/// Enrols `authenticator` for `identity` and answers the store it lives in.
fn enrolled(authenticator: &Authenticator, identity: Identity) -> (Passkeys, Arc<MemoryPasskeys>) {
  let store = Arc::new(MemoryPasskeys::new());
  let provider = passkeys(store.clone());
  let (kind, data, state) = present(block_on(provider.begin(Start::enrol("/settings", identity))).unwrap());
  assert_eq!(kind, "webauthn.create");
  assert!(matches!(block_on(provider.advance(authenticator.create(&data), state)).unwrap(), Next::Complete(_)));
  (provider, store)
}

#[test]
fn a_passkey_enrolled_for_an_identity_signs_it_in_with_its_claims() {
  let mut authenticator = Authenticator::new();
  let (provider, store) = enrolled(&authenticator, ada());
  assert_eq!(block_on(store.of_subject("ada")).unwrap().len(), 1);

  let (kind, data, state) = present(block_on(provider.begin(Start::sign_in("/"))).unwrap());
  assert_eq!(kind, "webauthn.get");
  let Some(Value::Map(public)) = data.get("publicKey") else { panic!() };
  assert_eq!(public.get("rpId"), Some(&Value::str(RP_ID)));
  assert_eq!(public.get("allowCredentials"), Some(&Value::seq(Vec::<Value>::new())), "a discoverable sign-in names no credential");
  let Next::Complete(outcome) = block_on(provider.advance(authenticator.get(&data, "ada"), state)).unwrap() else { panic!("complete") };
  assert_eq!(outcome.identity.subject, "ada");
  assert_eq!(outcome.identity.claims.get("email"), Some(&Value::str("ada@shop.example")));
}

#[test]
fn the_enrolment_offers_the_algorithms_and_excludes_what_the_identity_holds() {
  let authenticator = Authenticator::new();
  let (provider, _) = enrolled(&authenticator, ada());
  let (_, data, _) = present(block_on(provider.begin(Start::enrol("/", ada()))).unwrap());
  let Some(Value::Map(public)) = data.get("publicKey") else { panic!() };
  let Some(Value::Seq(exclude)) = public.get("excludeCredentials") else { panic!() };
  assert_eq!(exclude.len(), 1);
  assert_eq!(public.get("attestation"), Some(&Value::str("none")));
  let Some(Value::Map(user)) = public.get("user") else { panic!() };
  assert_eq!(user.get("id"), Some(&Value::str(b64(b"ada"))));
  assert_eq!(user.get("name"), Some(&Value::str("ada@shop.example")));
}

#[test]
fn a_ceremony_on_another_origin_or_for_another_challenge_is_denied() {
  let mut authenticator = Authenticator::new();
  let (provider, _) = enrolled(&authenticator, ada());
  let (_, data, state) = present(block_on(provider.begin(Start::sign_in("/"))).unwrap());
  authenticator.origin = "https://evil.example".to_owned();
  assert!(matches!(block_on(provider.advance(authenticator.get(&data, "ada"), state.clone())), Err(AuthError::Denied(why)) if why.contains("evil.example")));
  authenticator.origin = ORIGIN.to_owned();
  let (_, other, _) = present(block_on(provider.begin(Start::sign_in("/"))).unwrap());
  assert!(matches!(block_on(provider.advance(authenticator.get(&other, "ada"), state)), Err(AuthError::Denied(why)) if why.contains("challenge")));
}

#[test]
fn a_passkey_for_another_relying_party_is_denied() {
  let mut authenticator = Authenticator::new();
  let (provider, _) = enrolled(&authenticator, ada());
  authenticator.rp_id = "evil.example".to_owned();
  let (_, data, state) = present(block_on(provider.begin(Start::sign_in("/"))).unwrap());
  assert!(matches!(block_on(provider.advance(authenticator.get(&data, "ada"), state)), Err(AuthError::Denied(why)) if why.contains("relying party")));
}

#[test]
fn a_changed_signature_or_a_replayed_counter_is_denied() {
  let mut authenticator = Authenticator::new();
  let (provider, _) = enrolled(&authenticator, ada());
  let (_, data, state) = present(block_on(provider.begin(Start::sign_in("/"))).unwrap());
  let mut answer = authenticator.get(&data, "ada");
  let Some(Value::Map(credential)) = answer.get_mut("credential") else { panic!() };
  let Some(Value::Map(response)) = credential.get_mut("response") else { panic!() };
  response.insert("signature".to_owned(), Value::str(b64(&[0x30, 0x06, 0x02, 0x01, 0x01, 0x02, 0x01, 0x01])));
  assert!(matches!(block_on(provider.advance(answer, state)), Err(AuthError::Denied(why)) if why.contains("signature")));

  let (_, data, state) = present(block_on(provider.begin(Start::sign_in("/"))).unwrap());
  assert!(matches!(block_on(provider.advance(authenticator.get(&data, "ada"), state)), Ok(Next::Complete(_))));
  authenticator.counter -= 1;
  let (_, data, state) = present(block_on(provider.begin(Start::sign_in("/"))).unwrap());
  assert!(matches!(block_on(provider.advance(authenticator.get(&data, "ada"), state)), Err(AuthError::Denied(why)) if why.contains("counter")));
}

#[test]
fn a_passkey_never_enrolled_or_enrolled_twice_is_refused() {
  let mut stranger = Authenticator::new();
  let (provider, _) = enrolled(&Authenticator::new(), ada());
  let (_, data, state) = present(block_on(provider.begin(Start::sign_in("/"))).unwrap());
  assert!(matches!(block_on(provider.advance(stranger.get(&data, "ada"), state)), Err(AuthError::Denied(why)) if why.contains("not enrolled")));

  let authenticator = Authenticator::new();
  let (provider, _) = enrolled(&authenticator, ada());
  let (_, data, state) = present(block_on(provider.begin(Start::enrol("/", ada()))).unwrap());
  assert!(matches!(block_on(provider.advance(authenticator.create(&data), state)), Err(AuthError::Invalid(why)) if why.contains("already enrolled")));
}

#[test]
fn as_a_second_factor_only_the_identitys_own_passkeys_are_offered_and_accepted() {
  let store = Arc::new(MemoryPasskeys::new());
  let provider = passkeys(store.clone());
  let mut ada_key = Authenticator::new();
  let mut bob_key = Authenticator::new();
  for (key, who) in [(&ada_key, ada()), (&bob_key, Identity::new("bob".to_owned(), ValueMap::default()))] {
    let (_, data, state) = present(block_on(provider.begin(Start::enrol("/", who))).unwrap());
    block_on(provider.advance(key.create(&data), state)).unwrap();
  }
  let second = Start::sign_in("/").identity(Some(ada()));
  let (_, data, state) = present(block_on(provider.begin(second.clone())).unwrap());
  let Some(Value::Map(public)) = data.get("publicKey") else { panic!() };
  let Some(Value::Seq(allow)) = public.get("allowCredentials") else { panic!() };
  assert_eq!(allow.len(), 1, "ada's passkey alone");
  assert!(matches!(block_on(provider.advance(bob_key.get(&data, "bob"), state)), Err(AuthError::Denied(why)) if why.contains("another identity")));
  let (_, data, state) = present(block_on(provider.begin(second)).unwrap());
  assert!(matches!(block_on(provider.advance(ada_key.get(&data, "ada"), state)), Ok(Next::Complete(_))));

  let carol = Identity::new("carol".to_owned(), ValueMap::default());
  let (kind, data, state) = present(block_on(provider.begin(Start::sign_in("/").identity(Some(carol)))).unwrap());
  assert_eq!(kind, "webauthn.create", "an identity with no passkey creates its first");
  let Next::Complete(outcome) = block_on(provider.advance(Authenticator::new().create(&data), state)).unwrap() else { panic!("complete") };
  assert_eq!(outcome.identity.subject, "carol");
}

#[test]
fn a_file_store_keeps_passkeys_across_instances() {
  let path = std::env::temp_dir().join(format!("fsr-passkeys-{}.json", std::process::id()));
  let _ = std::fs::remove_file(&path);
  let mut authenticator = Authenticator::new();
  let store: Arc<dyn PasskeyStore> = Arc::new(FilePasskeys::new(&path));
  let provider = passkeys(store);
  let (_, data, state) = present(block_on(provider.begin(Start::enrol("/", ada()))).unwrap());
  block_on(provider.advance(authenticator.create(&data), state)).unwrap();

  let reopened = passkeys(Arc::new(FilePasskeys::new(&path)));
  let (_, data, state) = present(block_on(reopened.begin(Start::sign_in("/"))).unwrap());
  assert!(matches!(block_on(reopened.advance(authenticator.get(&data, "ada"), state)), Ok(Next::Complete(_))));
  let saved: Json = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
  assert_eq!(saved["credentials"][0]["sign_count"], 1, "the counter moved on disk");
  std::fs::remove_file(&path).unwrap();
}
