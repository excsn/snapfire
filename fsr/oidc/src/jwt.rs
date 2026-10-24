//! JSON Web Signature verification over `ring`, for the ID tokens an issuer signs (RFC 7515,
//! RFC 7518 section 3, RFC 8037). Only the asymmetric algorithms an issuer publishes keys for are
//! accepted: `none` and the HMAC family never verify.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use ring::signature::{self, RsaPublicKeyComponents, UnparsedPublicKey};
use serde::Deserialize;
use serde_json::{Map, Value};

/// One key of an issuer's JWKS.
#[derive(Debug, Clone, Deserialize)]
pub struct Jwk {
  #[serde(default)]
  pub kid: Option<String>,
  pub kty: String,
  #[serde(default)]
  pub alg: Option<String>,
  #[serde(default, rename = "use")]
  pub usage: Option<String>,
  #[serde(default)]
  pub n: Option<String>,
  #[serde(default)]
  pub e: Option<String>,
  #[serde(default)]
  pub crv: Option<String>,
  #[serde(default)]
  pub x: Option<String>,
  #[serde(default)]
  pub y: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JwkSet {
  pub keys: Vec<Jwk>,
}

#[derive(Debug, Deserialize)]
struct Header {
  alg: String,
  #[serde(default)]
  kid: Option<String>,
}

/// Why a token did not verify.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
  Malformed(String),
  /// No key of the set carries the token's `kid`, which is when the set is fetched again.
  UnknownKey(String),
  Signature,
  Algorithm(String),
}

impl std::fmt::Display for Refused {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::Malformed(why) => write!(f, "a malformed token: {why}"),
      Self::UnknownKey(kid) => write!(f, "no key `{kid}` in the issuer's key set"),
      Self::Signature => f.write_str("the signature does not verify"),
      Self::Algorithm(alg) => write!(f, "the algorithm `{alg}` is not accepted"),
    }
  }
}

fn decode(part: &str) -> Result<Vec<u8>, Refused> {
  URL_SAFE_NO_PAD.decode(part).map_err(|e| Refused::Malformed(e.to_string()))
}

fn key_bytes(field: &Option<String>, name: &str) -> Result<Vec<u8>, Refused> {
  decode(field.as_deref().ok_or_else(|| Refused::Malformed(format!("a key without `{name}`")))?)
}

/// The claims of `token` when its signature verifies under a key of `keys` that fits its header.
pub fn verify(token: &str, keys: &[Jwk]) -> Result<Map<String, Value>, Refused> {
  let mut parts = token.split('.');
  let (Some(head), Some(body), Some(sig), None) = (parts.next(), parts.next(), parts.next(), parts.next()) else {
    return Err(Refused::Malformed("not three parts".to_owned()));
  };
  let header: Header = serde_json::from_slice(&decode(head)?).map_err(|e| Refused::Malformed(e.to_string()))?;
  let signature = decode(sig)?;
  let message = format!("{head}.{body}");
  let kty = match header.alg.as_str() {
    "RS256" => "RSA",
    "ES256" => "EC",
    "EdDSA" => "OKP",
    other => return Err(Refused::Algorithm(other.to_owned())),
  };
  let candidates: Vec<&Jwk> = keys
    .iter()
    .filter(|key| key.kty == kty && key.usage.as_deref().is_none_or(|usage| usage == "sig"))
    .filter(|key| key.alg.as_deref().is_none_or(|alg| alg == header.alg))
    .filter(|key| header.kid.is_none() || key.kid == header.kid)
    .collect();
  if candidates.is_empty() {
    return Err(Refused::UnknownKey(header.kid.unwrap_or_default()));
  }
  let verified = candidates.into_iter().any(|key| check(&header.alg, key, message.as_bytes(), &signature).unwrap_or(false));
  if !verified {
    return Err(Refused::Signature);
  }
  match serde_json::from_slice(&decode(body)?) {
    Ok(Value::Object(claims)) => Ok(claims),
    Ok(_) => Err(Refused::Malformed("the claims are not an object".to_owned())),
    Err(e) => Err(Refused::Malformed(e.to_string())),
  }
}

fn check(alg: &str, key: &Jwk, message: &[u8], sig: &[u8]) -> Result<bool, Refused> {
  Ok(match alg {
    "RS256" => {
      let components = RsaPublicKeyComponents { n: key_bytes(&key.n, "n")?, e: key_bytes(&key.e, "e")? };
      components.verify(&signature::RSA_PKCS1_2048_8192_SHA256, message, sig).is_ok()
    }
    "ES256" => {
      if key.crv.as_deref() != Some("P-256") {
        return Ok(false);
      }
      let mut point = vec![0x04];
      point.extend(key_bytes(&key.x, "x")?);
      point.extend(key_bytes(&key.y, "y")?);
      UnparsedPublicKey::new(&signature::ECDSA_P256_SHA256_FIXED, point).verify(message, sig).is_ok()
    }
    "EdDSA" => key.crv.as_deref() == Some("Ed25519") && UnparsedPublicKey::new(&signature::ED25519, key_bytes(&key.x, "x")?).verify(message, sig).is_ok(),
    _ => false,
  })
}

/// A compact JWS of `claims` signed ES256 with a PKCS#8 P-256 key, which is how a client secret is
/// minted for an issuer that wants one signed (Apple).
pub fn sign_es256(pkcs8: &[u8], kid: &str, claims: &Value) -> Result<String, String> {
  let rng = ring::rand::SystemRandom::new();
  let pair = signature::EcdsaKeyPair::from_pkcs8(&signature::ECDSA_P256_SHA256_FIXED_SIGNING, pkcs8, &rng).map_err(|e| format!("the signing key: {e}"))?;
  let header = serde_json::json!({ "alg": "ES256", "kid": kid, "typ": "JWT" });
  let message = format!("{}.{}", URL_SAFE_NO_PAD.encode(header.to_string()), URL_SAFE_NO_PAD.encode(claims.to_string()));
  let sig = pair.sign(&rng, message.as_bytes()).map_err(|e| format!("signing: {e}"))?;
  Ok(format!("{message}.{}", URL_SAFE_NO_PAD.encode(sig.as_ref())))
}
