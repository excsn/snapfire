//! The token under generated input. An ID token is whatever the issuer's token endpoint or a
//! forger sends, so a verifier that panics is a sign-in that kills the worker.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use proptest::prelude::*;
use snapfire_fsr_oidc::{verify, Jwk};

fn keys() -> Vec<Jwk> {
  serde_json::from_value(serde_json::json!([
    { "kty": "EC", "crv": "P-256", "kid": "k1", "x": "f83OJ3D2xF1Bg8vub9tLe1gHMzV76e8Tus9uPHvRVEU", "y": "x_FEzRu9m36HLN_tue659LNpXW6pCyStikYjKIWI5a0" },
    { "kty": "RSA", "kid": "r1", "n": "sXchDaQebHnPiGvyDOAT4saGEUetSyo9MKLOoWFsueri23bOdgWp4Dy1WlUzewbgBHod5pcM9H95GQRV3JDXboIRROSBigeC5yjU1hGzHHyXss8UDprecbAYxknTcQkhslANGRUZmdTOQ5qTRsLAt6BTYuyvVRdhS8exSZEy_c4gs_7svlJJQ4H9_NxsiIoLwAEk7-Q3UXERGYw_75IDrGA84-lA_-Ct4eTlXHBIY2EaV7t7LjJaynVJCpkv4LKjTTAumiGUIuQhrNhZLuF_RJLqHpM2kgWFLU7-VTdL1VbC2tejvcI2BlMkEpk1BzBZI0KQB0GaDWFLN-aEAw3vRw", "e": "AQAB" }
  ]))
  .unwrap()
}

fn part() -> impl Strategy<Value = String> {
  prop_oneof![
    "[A-Za-z0-9_-]{0,80}",
    Just(URL_SAFE_NO_PAD.encode(r#"{"alg":"ES256","kid":"k1"}"#)),
    Just(URL_SAFE_NO_PAD.encode(r#"{"alg":"RS256","kid":"r1"}"#)),
    Just(URL_SAFE_NO_PAD.encode(r#"{"alg":"none"}"#)),
    Just(URL_SAFE_NO_PAD.encode(r#"{"sub":"mallory"}"#)),
  ]
}

proptest! {
  #![proptest_config(ProptestConfig { cases: 1024, ..ProptestConfig::default() })]

  /// Whatever arrives, verifying answers or refuses.
  #[test]
  fn arbitrary_text_never_panics(token in "(?s).{0,200}") {
    let _ = verify(&token, &keys());
  }

  /// Three well-formed parts that no published key signed never verify.
  #[test]
  fn unsigned_parts_never_verify(head in part(), body in part(), sig in "[A-Za-z0-9_-]{0,120}") {
    let token = format!("{}.{}.{}", head, body, sig);
    prop_assert!(verify(&token, &keys()).is_err());
  }
}
