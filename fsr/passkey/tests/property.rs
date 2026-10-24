//! The ceremony's bytes under generated input. Every byte a passkey answer carries is the
//! browser's to choose, so a parser that panics is a sign-in that kills the worker.

use proptest::prelude::*;
use snapfire_fsr_passkey::webauthn::{attestation_auth_data, auth_data, client_data};

fn rp_prefixed() -> impl Strategy<Value = Vec<u8>> {
  let hash = ring::digest::digest(&ring::digest::SHA256, b"shop.example").as_ref().to_vec();
  prop::collection::vec(any::<u8>(), 0..200).prop_map(move |tail| {
    let mut data = hash.clone();
    data.extend(tail);
    data
  })
}

proptest! {
  #![proptest_config(ProptestConfig { cases: 1024, ..ProptestConfig::default() })]

  /// Authenticator data of any length and content answers or refuses, the attested credential
  /// data and its CBOR key included.
  #[test]
  fn authenticator_data_never_panics(data in prop_oneof![prop::collection::vec(any::<u8>(), 0..200), rp_prefixed()]) {
    let _ = auth_data(&data, "shop.example", false);
    let _ = auth_data(&data, "shop.example", true);
  }

  #[test]
  fn attestation_objects_never_panic(object in prop::collection::vec(any::<u8>(), 0..300)) {
    let _ = attestation_auth_data(&object);
  }

  #[test]
  fn client_data_never_panics(json in "(?s).{0,200}") {
    let _ = client_data(json.as_bytes(), "webauthn.get", "c", "https://shop.example");
  }
}
