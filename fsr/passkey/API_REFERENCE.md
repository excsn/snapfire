# API Reference: snapfire_fsr_passkey

WebAuthn passkeys as an `IdentityProvider`, with the credential store seam.

## Contents

* [1. The Provider](#1-the-provider)
  * [Passkeys](#passkeys)
  * [PasskeyConfig](#passkeyconfig)
* [2. Credentials](#2-credentials)
  * [Credential](#credential)
  * [PasskeyStore](#passkeystore)
  * [MemoryPasskeys](#memorypasskeys)
  * [FilePasskeys](#filepasskeys)
* [3. The Ceremony's Checks](#3-the-ceremonys-checks)
  * [PublicKey](#publickey)
  * [Functions](#functions)
* [4. Error Handling](#4-error-handling)
  * [AuthError](#autherror)
  * [Failed](#failed)

## 1. The Provider

### Passkeys

* `pub fn new(config: PasskeyConfig, store: Arc<dyn PasskeyStore>) -> Self`
* `begin` with `Intent::SignIn` and no identity: `Next::Present` of kind `webauthn.get`, `data` `{ publicKey: { challenge, rpId, timeout, userVerification, allowCredentials: [] } }`, a fresh 32-byte base64url challenge.
* `begin` with `Intent::SignIn` and `Start::identity` set: the same with `allowCredentials` the identity's passkeys, `{ type: "public-key", id }`; for an identity holding none, the enrolment step below.
* `begin` with `Intent::Enrol`: `Next::Present` of kind `webauthn.create`, `data` `{ publicKey: { challenge, rp: { id, name }, user: { id, name, displayName }, pubKeyCredParams, timeout, attestation: "none", authenticatorSelection: { residentKey: "required", requireResidentKey: true, userVerification }, excludeCredentials } }`. `user.id` is the subject's bytes base64url; `name` is the `email` claim, else the subject; `pubKeyCredParams` offers ES256, EdDSA and RS256 in that order. Without an identity, `AuthError::Denied`.
* `advance(params, state)`: `params.credential` is the credential as the browser's `PublicKeyCredential` serialises, each byte field base64url. For a `get` step: the client data must be `webauthn.get` with the step's challenge, the configured origin and no `crossOrigin`; the credential must be in the store, belong to the step's identity when it named one and match a `userHandle` when present; the authenticator data must hash the RP ID and carry user presence (and verification when required); the signature over the authenticator data and the client data's SHA-256 must verify; the sign count must move past the stored one unless both are zero. Completes with the credential's subject and stored claims and no tokens. It saves the new count. For a `create` step: the client data must be `webauthn.create`; the attestation object's authenticator data must carry an attested credential whose id is the one posted and is not yet enrolled. The attestation statement is not checked. Completes for the enrolling identity and saves the credential.

### PasskeyConfig

The relying party. `Debug`, `Clone`; `#[non_exhaustive]`.

* `pub rp_id: String`: the domain a passkey is bound to, never an IP address
* `pub rp_name: String`
* `pub origin: String`: the exact origin ceremonies run on
* `pub require_verification: bool`, `false` by default (`preferred`)
* `pub timeout_ms: u64`, `60_000`
* `pub fn new(rp_id, rp_name, origin) -> Self`, `pub fn require_verification(self, required: bool) -> Self`

## 2. Credentials

### Credential

One enrolled passkey. `Debug`, `Clone`, `PartialEq`, `Serialize`, `Deserialize`; `#[non_exhaustive]`.

* `pub id: String`: the credential id, base64url
* `pub subject: String`
* `pub claims: serde_json::Map<String, serde_json::Value>`: the identity's claims at enrolment
* `pub key: PublicKey`
* `pub sign_count: u32`
* `pub fn new(id, subject, claims, key, sign_count) -> Self`

### PasskeyStore

`Send + Sync`. Errors are strings the provider reports as `AuthError::Invalid`.

* `fn find(&self, id: &str) -> BoxFuture<'_, Result<Option<Credential>, String>>`
* `fn of_subject(&self, subject: &str) -> BoxFuture<'_, Result<Vec<Credential>, String>>`
* `fn save(&self, credential: Credential) -> BoxFuture<'_, Result<(), String>>`: adds or replaces by id

### MemoryPasskeys

In this process; `new()` and `Default`.

### FilePasskeys

* `pub fn new(path: impl Into<PathBuf>) -> Self`

A JSON file `{ "credentials": [...] }` read on every call; a missing file holds none. `save` writes `<path>.json.writing` and renames it into place under a lock. For one process: two writing one file lose each other's writes.

## 3. The Ceremony's Checks

Module `webauthn`.

### PublicKey

A credential's key, `#[serde(tag = "alg")]`.

* `Es256 { point: String }`: the uncompressed P-256 point, base64url
* `Ed25519 { x: String }`
* `Rs256 { n: String, e: String }`
* `pub fn verifies(&self, message: &[u8], signature: &[u8]) -> bool`: ES256 takes the DER signature WebAuthn uses

### Functions

* `pub const ES256: i64 = -7`, `EDDSA: i64 = -8`, `RS256: i64 = -257`
* `pub fn cose_key(map: &[(ciborium::value::Value, ciborium::value::Value)]) -> Result<PublicKey, Failed>`: EC2 P-256 with ES256, OKP Ed25519 with EdDSA or RSA with RS256; any other is `Failed`
* `pub fn client_data(json: &[u8], kind: &str, challenge: &str, origin: &str) -> Result<(), Failed>`
* `pub fn auth_data(data: &[u8], rp_id: &str, verified: bool) -> Result<AuthData, Failed>`: `AuthData { flags, sign_count, attested: Option<(Vec<u8>, PublicKey)> }`
* `pub fn attestation_auth_data(object: &[u8]) -> Result<Vec<u8>, Failed>`
* `pub fn decode(text: &str) -> Result<Vec<u8>, Failed>`, `pub fn encode(bytes: &[u8]) -> String`: base64url without padding

## 4. Error Handling

### AuthError

`Passkeys` answers `snapfire_fsr_auth::AuthError`: `Denied` for a ceremony that does not check out (another origin, challenge or relying party, a signature that does not verify, a counter that did not move, a passkey not enrolled or belonging to someone else), `Invalid` for a malformed answer, a store that failed or a passkey enrolled twice.

### Failed

`pub struct Failed(pub String)`, `Display`, why one of the `webauthn` checks failed.
