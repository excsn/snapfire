# API Reference: snapfire_fsr_passkey

WebAuthn passkeys as a `snapfire_fsr_auth::IdentityProvider`, over a credential store.

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
  * [AuthData](#authdata)
  * [cose_key](#cose_key)
  * [client_data](#client_data)
  * [auth_data](#auth_data)
  * [attestation_auth_data](#attestation_auth_data)
  * [encode](#encode)
  * [decode](#decode)
* [4. Error Handling](#4-error-handling)
  * [AuthError](#autherror)
  * [Failed](#failed)

## 1. The Provider

### Passkeys

An `IdentityProvider` that signs in and enrols with passkeys from a `PasskeyStore`.

* `pub struct Passkeys { /* private */ }`. Implements `snapfire_fsr_auth::IdentityProvider`.
* `pub fn new(config: PasskeyConfig, store: Arc<dyn PasskeyStore>) -> Self`
* `fn begin(&self, start: Start) -> BoxFuture<'_, Result<Next, AuthError>>`: by `start.intent` and `start.identity`:
  * `Intent::SignIn`, no identity: `Next::Present` of kind `webauthn.get`, `data` `{ publicKey: { challenge, rpId, timeout, userVerification, allowCredentials: [] } }`. The challenge is 32 random bytes, base64url.
  * `Intent::SignIn` with an identity: the same with `allowCredentials` the identity's passkeys, each `{ type: "public-key", id }`. For an identity holding none, the `Intent::Enrol` step.
  * `Intent::Enrol` with an identity: `Next::Present` of kind `webauthn.create`, `data` `{ publicKey: { challenge, rp: { id, name }, user: { id, name, displayName }, pubKeyCredParams, timeout, attestation: "none", authenticatorSelection: { residentKey: "required", requireResidentKey: true, userVerification }, excludeCredentials } }`.
  * `Intent::Enrol`, no identity: `AuthError::Denied`.
* In a `webauthn.create` step, `user.id` is the subject's bytes in base64url. `name` and `displayName` are the `email` claim when it is a string, else the subject. `pubKeyCredParams` offers ES256, EdDSA and RS256 in that order. `excludeCredentials` lists the identity's passkeys.
* `userVerification` is `required` when `PasskeyConfig::require_verification` is set, else `preferred`.
* `fn advance(&self, params: ValueMap, state: ValueMap) -> BoxFuture<'_, Result<Next, AuthError>>`: `params.credential` is a serialised `PublicKeyCredential`, each byte field base64url. Its id is `rawId`, else `id`.
* A `get` step completes with the credential's subject, its stored claims and no tokens when all of these hold. It then saves the new sign count.
  * The client data is `webauthn.get` with the step's challenge, the configured origin and no `crossOrigin`.
  * The credential is in the store and belongs to the step's identity when the step named one.
  * `response.userHandle`, when present, is the credential's subject bytes.
  * The authenticator data starts with the SHA-256 of the RP ID and carries user presence. It also carries user verification when that is required.
  * The signature over the authenticator data followed by the SHA-256 of the client data verifies.
  * The sign count is above the stored one, unless both are zero.
* A `create` step completes for the enrolling identity and saves the credential when all of these hold. The attestation statement is not checked.
  * The client data is `webauthn.create` with the step's challenge and the configured origin.
  * The attestation object's authenticator data carries an attested credential whose id is the posted id.
  * No credential with that id is enrolled.

### PasskeyConfig

The relying party. `Debug`, `Clone`, `#[non_exhaustive]`.

* `pub rp_id: String`: the domain a passkey is bound to, the origin's host or a registrable suffix of it. Never an IP address.
* `pub rp_name: String`: the site's name in the browser's prompt.
* `pub origin: String`: the exact origin ceremonies run on, such as `https://shop.example`.
* `pub require_verification: bool`: `false` by default, which asks for verification as `preferred`.
* `pub timeout_ms: u64`: how long the browser's prompt waits. `60_000` by default.
* `pub fn new(rp_id: impl Into<String>, rp_name: impl Into<String>, origin: impl Into<String>) -> Self`
* `pub fn require_verification(self, required: bool) -> Self`

## 2. Credentials

### Credential

One enrolled passkey. `Debug`, `Clone`, `PartialEq`, `Serialize`, `Deserialize`, `#[non_exhaustive]`.

* `pub id: String`: the credential id, base64url.
* `pub subject: String`
* `pub claims: serde_json::Map<String, serde_json::Value>`: the identity's claims at enrolment.
* `pub key: PublicKey`
* `pub sign_count: u32`
* `pub fn new(id: String, subject: String, claims: Map<String, Json>, key: PublicKey, sign_count: u32) -> Self`

### PasskeyStore

Where passkeys are kept. `Send + Sync`. An error string is reported as `AuthError::Invalid`.

* `fn find(&self, id: &str) -> BoxFuture<'_, Result<Option<Credential>, String>>`
* `fn of_subject(&self, subject: &str) -> BoxFuture<'_, Result<Vec<Credential>, String>>`: every credential `subject` enrolled.
* `fn save(&self, credential: Credential) -> BoxFuture<'_, Result<(), String>>`: adds `credential` or replaces the one with its id.

### MemoryPasskeys

Passkeys held in this process and lost when it stops.

* `pub fn new() -> Self`. `Default`.

### FilePasskeys

Passkeys in a JSON file `{ "credentials": [...] }`.

* `pub fn new(path: impl Into<PathBuf>) -> Self`
* The file is read on every call. A missing file holds no credentials.
* `save` writes the whole file to `path` with its extension set to `json.writing`, then renames it over `path`, under a lock held in this process.
* Two processes writing one file lose each other's writes.

## 3. The Ceremony's Checks

Module `webauthn`. Module constants: `ES256: i64 = -7`, `EDDSA: i64 = -8`, `RS256: i64 = -257`.

### PublicKey

A credential's public key. `Debug`, `Clone`, `PartialEq`, `Eq`, `Serialize`, `Deserialize`, `#[serde(tag = "alg")]`.

* `Es256 { point: String }`: the uncompressed P-256 point, base64url.
* `Ed25519 { x: String }`
* `Rs256 { n: String, e: String }`
* `pub fn verifies(&self, message: &[u8], signature: &[u8]) -> bool`: an ES256 signature is DER.

### AuthData

The parsed authenticator data.

* `pub struct AuthData { pub flags: u8, pub sign_count: u32, pub attested: Option<(Vec<u8>, PublicKey)> }`
* `attested` is the new credential's id and key, present when the attested-credential flag is set.

### cose_key

* `pub fn cose_key(map: &[(ciborium::value::Value, ciborium::value::Value)]) -> Result<PublicKey, Failed>`: an EC2 P-256 key with ES256, an OKP Ed25519 key with EdDSA or an RSA key with RS256. Any other key is `Failed`.

### client_data

* `pub fn client_data(json: &[u8], kind: &str, challenge: &str, origin: &str) -> Result<(), Failed>`: `Failed` unless `type` is `kind`, `challenge` matches with trailing `=` ignored, `origin` matches and `crossOrigin` is absent or `false`.

### auth_data

* `pub fn auth_data(data: &[u8], rp_id: &str, verified: bool) -> Result<AuthData, Failed>`: `Failed` when `data` is shorter than 37 bytes, when its first 32 bytes are not the SHA-256 of `rp_id`, when user presence is not set and when `verified` is true but user verification is not set.

### attestation_auth_data

* `pub fn attestation_auth_data(object: &[u8]) -> Result<Vec<u8>, Failed>`: the `authData` bytes of a CBOR attestation object. The attestation statement is not checked.

### encode

* `pub fn encode(bytes: &[u8]) -> String`: base64url without padding.

### decode

* `pub fn decode(text: &str) -> Result<Vec<u8>, Failed>`: base64url, with any trailing `=` ignored.

## 4. Error Handling

### AuthError

`Passkeys` returns `snapfire_fsr_auth::AuthError`.

* `Denied`: an enrolment with no signed-in identity or a ceremony that fails a check. That covers another origin, challenge or relying party, a signature that does not verify, a sign count that did not move and a passkey not enrolled or belonging to another identity.
* `Invalid`: a malformed answer, a store error, an attestation whose credential is missing or is not the posted one and a passkey enrolled twice.

### Failed

Why a `webauthn` check failed.

* `pub struct Failed(pub String)`. `Debug`, `Clone`, `PartialEq`, `Eq`, `Display`.
