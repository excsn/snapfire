//! The relying party's checks of a WebAuthn ceremony (Web Authentication Level 2, sections 7.1
//! and 7.2): the client data, the authenticator data, the credential's COSE key (RFC 9053) and
//! the assertion's signature.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use ciborium::value::Value as Cbor;
use ring::signature::{self, RsaPublicKeyComponents, UnparsedPublicKey};
use serde::{Deserialize, Serialize};

/// COSE algorithm identifiers a credential may use, in the order a relying party offers them.
pub const ES256: i64 = -7;
pub const EDDSA: i64 = -8;
pub const RS256: i64 = -257;

/// Authenticator data flags.
const USER_PRESENT: u8 = 0x01;
const USER_VERIFIED: u8 = 0x04;
const ATTESTED: u8 = 0x40;

/// Why a ceremony did not check out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failed(pub String);

impl std::fmt::Display for Failed {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str(&self.0)
  }
}

fn failed(why: impl Into<String>) -> Failed {
  Failed(why.into())
}

pub fn decode(text: &str) -> Result<Vec<u8>, Failed> {
  URL_SAFE_NO_PAD.decode(text.trim_end_matches('=')).map_err(|e| failed(format!("not base64url: {e}")))
}

pub fn encode(bytes: &[u8]) -> String {
  URL_SAFE_NO_PAD.encode(bytes)
}

/// A credential's public key, decoded from its COSE form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "alg")]
pub enum PublicKey {
  /// The uncompressed P-256 point.
  Es256 { point: String },
  Ed25519 { x: String },
  Rs256 { n: String, e: String },
}

impl PublicKey {
  /// Whether `signature` signs `message` under this key; a WebAuthn ES256 signature is DER.
  pub fn verifies(&self, message: &[u8], signature: &[u8]) -> bool {
    let bytes = |text: &str| decode(text).unwrap_or_default();
    match self {
      Self::Es256 { point } => UnparsedPublicKey::new(&signature::ECDSA_P256_SHA256_ASN1, bytes(point)).verify(message, signature).is_ok(),
      Self::Ed25519 { x } => UnparsedPublicKey::new(&signature::ED25519, bytes(x)).verify(message, signature).is_ok(),
      Self::Rs256 { n, e } => RsaPublicKeyComponents { n: bytes(n), e: bytes(e) }.verify(&signature::RSA_PKCS1_2048_8192_SHA256, message, signature).is_ok(),
    }
  }
}

fn label(map: &[(Cbor, Cbor)], key: i64) -> Option<&Cbor> {
  map.iter().find(|(k, _)| k.as_integer().is_some_and(|i| i128::from(i) == i128::from(key))).map(|(_, v)| v)
}

fn int(value: Option<&Cbor>) -> Option<i64> {
  value.and_then(Cbor::as_integer).and_then(|i| i64::try_from(i).ok())
}

fn bytes(value: Option<&Cbor>, name: &str) -> Result<Vec<u8>, Failed> {
  value.and_then(Cbor::as_bytes).cloned().ok_or_else(|| failed(format!("the key has no `{name}`")))
}

/// A COSE key as `PublicKey`: EC2 P-256 with ES256, OKP Ed25519 with EdDSA or RSA with RS256.
pub fn cose_key(map: &[(Cbor, Cbor)]) -> Result<PublicKey, Failed> {
  match (int(label(map, 1)), int(label(map, 3))) {
    (Some(2), Some(ES256)) => {
      if int(label(map, -1)) != Some(1) {
        return Err(failed("an EC2 key that is not P-256"));
      }
      let (x, y) = (bytes(label(map, -2), "x")?, bytes(label(map, -3), "y")?);
      if x.len() != 32 || y.len() != 32 {
        return Err(failed("a P-256 coordinate that is not 32 bytes"));
      }
      let mut point = vec![0x04];
      point.extend(x);
      point.extend(y);
      Ok(PublicKey::Es256 { point: encode(&point) })
    }
    (Some(1), Some(EDDSA)) => {
      if int(label(map, -1)) != Some(6) {
        return Err(failed("an OKP key that is not Ed25519"));
      }
      Ok(PublicKey::Ed25519 { x: encode(&bytes(label(map, -2), "x")?) })
    }
    (Some(3), Some(RS256)) => Ok(PublicKey::Rs256 { n: encode(&bytes(label(map, -1), "n")?), e: encode(&bytes(label(map, -2), "e")?) }),
    (kty, alg) => Err(failed(format!("a key of type {kty:?} and algorithm {alg:?}, not ES256, EdDSA or RS256"))),
  }
}

/// The client data, checked against the ceremony's type, challenge and origin.
pub fn client_data(json: &[u8], kind: &str, challenge: &str, origin: &str) -> Result<(), Failed> {
  #[derive(Deserialize)]
  struct ClientData {
    #[serde(rename = "type")]
    kind: String,
    challenge: String,
    origin: String,
    #[serde(default, rename = "crossOrigin")]
    cross_origin: bool,
  }
  let data: ClientData = serde_json::from_slice(json).map_err(|e| failed(format!("the client data: {e}")))?;
  if data.kind != kind {
    return Err(failed(format!("a {} ceremony answered a {kind} step", data.kind)));
  }
  if data.challenge.trim_end_matches('=') != challenge {
    return Err(failed("the client data names another challenge"));
  }
  if data.origin != origin {
    return Err(failed(format!("the ceremony ran on {}, not {origin}", data.origin)));
  }
  if data.cross_origin {
    return Err(failed("the ceremony ran in a frame of another origin"));
  }
  Ok(())
}

/// What the authenticator data says.
pub struct AuthData {
  pub flags: u8,
  pub sign_count: u32,
  /// The new credential's id and key, from an attested credential's data.
  pub attested: Option<(Vec<u8>, PublicKey)>,
}

/// The authenticator data, checked against the relying party's id hash and for the user's presence
/// (and verification when `verified`).
pub fn auth_data(data: &[u8], rp_id: &str, verified: bool) -> Result<AuthData, Failed> {
  if data.len() < 37 {
    return Err(failed("the authenticator data is shorter than 37 bytes"));
  }
  let rp_hash = ring::digest::digest(&ring::digest::SHA256, rp_id.as_bytes());
  if &data[..32] != rp_hash.as_ref() {
    return Err(failed(format!("the credential belongs to another relying party than {rp_id}")));
  }
  let flags = data[32];
  if flags & USER_PRESENT == 0 {
    return Err(failed("the authenticator did not see the user"));
  }
  if verified && flags & USER_VERIFIED == 0 {
    return Err(failed("the authenticator did not verify the user"));
  }
  let sign_count = u32::from_be_bytes([data[33], data[34], data[35], data[36]]);
  let attested = if flags & ATTESTED != 0 {
    let rest = &data[37..];
    if rest.len() < 18 {
      return Err(failed("the attested credential data is cut short"));
    }
    let length = u16::from_be_bytes([rest[16], rest[17]]) as usize;
    let id = rest.get(18..18 + length).ok_or_else(|| failed("the credential id is cut short"))?.to_vec();
    let key: Cbor = ciborium::de::from_reader(&rest[18 + length..]).map_err(|e| failed(format!("the credential key: {e}")))?;
    let map = key.as_map().ok_or_else(|| failed("the credential key is not a map"))?;
    Some((id, cose_key(map)?))
  } else {
    None
  };
  Ok(AuthData { flags, sign_count, attested })
}

/// The authenticator data inside an attestation object. The attestation statement is not checked:
/// the relying party asks for none, so a self or packed statement proves nothing it relies on.
pub fn attestation_auth_data(object: &[u8]) -> Result<Vec<u8>, Failed> {
  let value: Cbor = ciborium::de::from_reader(object).map_err(|e| failed(format!("the attestation object: {e}")))?;
  let map = value.as_map().ok_or_else(|| failed("the attestation object is not a map"))?;
  map
    .iter()
    .find(|(k, _)| k.as_text() == Some("authData"))
    .and_then(|(_, v)| v.as_bytes())
    .cloned()
    .ok_or_else(|| failed("the attestation object has no authData"))
}
