# API Reference: snapfire_fsr_session

The session layer for SnapFire FSR: signed or sealed cookies, the session store, token custody and CSRF.

## Contents

* [1. Session Identity](#1-session-identity)
  * [`SessionId`](#sessionid)
* [2. The Session Layer](#2-the-session-layer)
  * [`SessionConfig`](#sessionconfig)
  * [`Opened`](#opened)
  * [`Sessions`](#sessions)
* [3. Token Custody](#3-token-custody)
  * [`TokenCell`](#tokencell)
* [4. The Store](#4-the-store)
  * [`SessionRecord`](#sessionrecord)
  * [`SessionStore`](#sessionstore)
  * [`MemorySessionStore`](#memorysessionstore)
* [5. The Cookie Codec](#5-the-cookie-codec)
  * [`CookieCodec`](#cookiecodec)
  * [`HmacCodec`](#hmaccodec)
  * [`Keyring`](#keyring)
  * [`SealedCookie`](#sealedcookie)
* [6. Wire Formats](#6-wire-formats)
  * [Cookie value](#cookie-value)
  * [Sealed cookie value](#sealed-cookie-value)
  * [Set-Cookie header](#set-cookie-header)
  * [State cookie](#state-cookie)
  * [CSRF token](#csrf-token)
  * [Session record](#session-record)
* [7. The CSRF Scheme](#7-the-csrf-scheme)
  * [`CsrfScheme`](#csrfscheme)
  * [`SingleUse`](#singleuse)
  * [`PerSession`](#persession)
  * [`Derived`](#derived)
  * [`random_token`](#random_token)
  * [`constant_time_eq`](#constant_time_eq)
* [8. Types From Other Crates](#8-types-from-other-crates)
  * [`SessionCell` and `Identity`](#sessioncell-and-identity)
  * [`Value` and `ValueMap`](#value-and-valuemap)
* [9. Error Handling](#9-error-handling)
  * [`StoreError`](#storeerror)
  * [Failures without an error](#failures-without-an-error)
  * [Panics](#panics)

## 1. Session Identity

### `SessionId`

A random opaque identifier. The cookie carries it signed. A sealed cookie carries it with the record. `Debug`, `Clone`, `PartialEq`, `Eq`, `Hash`.

* `pub struct SessionId(pub String)`: the inner string is the store key.
* `pub fn generate() -> SessionId`: 16 random bytes as 32 lowercase hex characters.
* `Display`: the bare string.
* A decoded id is not checked for shape. It is exactly the text that was signed.

## 2. The Session Layer

### `SessionConfig`

Cookie policy for the layer. `Default`. `#[non_exhaustive]`: build one with `default` and the setters.

* `pub cookie_name: String`: default `"sf_session"`.
* `pub ttl: Duration`: the length of a new session. Default eight hours.
* `pub secure: bool`: appends `; Secure` to every cookie. Default `false`.
* `pub max_bytes: usize`: the most a session's data, tokens and CSRF state may hold, approximately, in bytes. Default `DEFAULT_MAX_BYTES`.
* `pub fn cookie_name(self, cookie_name: String) -> Self`
* `pub fn ttl(self, ttl: Duration) -> Self`
* `pub fn secure(self, secure: bool) -> Self`
* `pub fn max_bytes(self, max_bytes: usize) -> Self`
* `pub const DEFAULT_MAX_BYTES: usize = 16 * 1024`
* A new session ends one `ttl` after `open`. Only `SessionCell::extend` moves the end.
* `Path=/`, `HttpOnly` and `SameSite=Lax` are always written on the session cookie.

### `Opened`

One request's session as the layer sees it. `Clone` shares the cells.

* `pub id: SessionId`
* `pub cell: SessionCell`: the application-visible half, the value that goes into `RequestCtx::session`.
* `pub tokens: TokenCell`: backend credentials and auth flow state. It never enters `RequestCtx`.
* `pub csrf: TokenCell`: the CSRF scheme's state, read and written by the scheme alone.
* `pub fresh: bool`: `true` when no valid cookie arrived.
* `pub base: Option<SessionRecord>`: the stored record as this request found it. `None` for a fresh session, a sealed one or a cookie whose record is gone.
* `pub stale: bool`: the cookie decoded under a key that is no longer the ring's first.
* A cookie that verifies but whose record is gone or expired gives `fresh == false`, the same id and empty cells.

### `Sessions`

The session layer. Holds the store or the sealer, the config and the CSRF scheme.

Construction:

* `pub fn new(store: Arc<dyn SessionStore>, key: &[u8], config: SessionConfig) -> Self`: over `HmacCodec::new(key)`. Any key length is accepted. The CSRF scheme is `SingleUse::default()`.
* `pub fn with_codec(store: Arc<dyn SessionStore>, key: &[u8], codec: Arc<dyn CookieCodec>, config: SessionConfig) -> Self`: over the caller's codec. `key` is ignored.
* `pub fn sealed(sealer: Arc<SealedCookie>, config: SessionConfig) -> Self`: no store. Each session's id and record are sealed in its cookie.
* `pub fn with_csrf(self, scheme: Arc<dyn CsrfScheme>) -> Self`

Opening and saving:

* `pub async fn open(&self, cookie_header: Option<&str>) -> Opened`: never fails. A missing, malformed, tampered or foreign-signed cookie gives a fresh session.
  * A stored record past its `expires` is deleted and treated as absent.
  * A sealed cookie is opened from its own value. `stale` is set when a key other than the current one opened it.
* `pub async fn persist(&self, opened: &Opened) -> Result<Option<String>, StoreError>`: saves when any of `cell`, `tokens` and `csrf` is dirty or the cell was extended.
  * With nothing to save, it touches nothing and returns `Some(set_cookie)` when `stale`, `None` otherwise.
  * After saving, it returns `Some(set_cookie)` when the session is sealed, `fresh` or `stale` or the cell was extended, `None` otherwise.
  * A session over `max_bytes` is refused with a `StoreError` before saving.
* `pub async fn establish(&self, opened: &Opened) -> Result<Option<String>, StoreError>`: for a fresh session, saves whether or not anything is dirty and returns `Some(set_cookie)`. For any other session it is `persist`.
* `pub async fn destroy(&self, opened: &Opened) -> Result<String, StoreError>`: deletes the record, clears `cell`, `tokens` and `csrf`, calls the scheme's `rotate` and returns the expiring cookie.
* `pub async fn renew(&self, opened: &Opened) -> Result<Opened, StoreError>`: deletes the old record and returns the same cells under a new id, with `fresh` set.
* `pub fn too_large(&self, opened: &Opened) -> Option<usize>`: the approximate size of the session when it is over `max_bytes`, else `None`.
* `pub fn state_cookie(&self) -> String`: the `Set-Cookie` for `STATE_COOKIE` with a new generation.
* `pub const STATE_COOKIE: &str = "sf_state"`

Saving a stored session that has a `base`:

* Saves for the same id are serialised within this process.
* The latest stored record is loaded. When it is gone, nothing is saved.
* Each key of `data` and `tokens` comes from whichever side changed it since `base`, this request winning when both did. `identity` and `expires` follow the same rule.
* `csrf` is merged by `CsrfScheme::merge`.
* Two processes sharing a store can still interleave saves.

A sealed session:

* A save writes no store. The cookie is the record.
* A cookie whose name and value would exceed `COOKIE_LIMIT` makes `persist` and `establish` return a `StoreError` naming its size.
* `destroy` and `renew` delete nothing. A copy of the cookie taken before `destroy` still opens until its record's `expires`.

CSRF:

* `pub fn csrf_token(&self, opened: &Opened) -> String`: the scheme's `issue`.
* `pub fn verify_csrf(&self, opened: &Opened, token: &str) -> bool`: the scheme's `verify`. A token never verifies for another session.
* `pub fn rotate_csrf(&self, opened: &Opened)`: the scheme's `rotate`.
* `pub fn csrf_scheme(&self) -> Arc<dyn CsrfScheme>`

Cookie header parsing:

* The header is split on `;`. A pair matches when its trimmed name equals `cookie_name` exactly.
* The value is trimmed, one pair of surrounding double quotes is removed and `%xx` escapes are decoded. An incomplete escape is kept as written.

## 3. Token Custody

### `TokenCell`

Server-side custody for backend credentials and auth flow state. `Clone`, `Default`. A clone shares one map and one dirty flag.

* `pub fn new(tokens: ValueMap) -> Self`: starts clean.
* `pub fn get(&self, key: &str) -> Option<Value>`: a clone of the value.
* `pub fn set(&self, key: impl Into<String>, value: Value)`: marks dirty.
* `pub fn remove(&self, key: &str) -> Option<Value>`: marks dirty only when something was removed. The remaining keys keep their order.
* `pub fn merge(&self, tokens: ValueMap)`: inserts every pair and marks dirty, even for an empty map.
* `pub fn clear(&self)`: empties the map and marks dirty.
* `pub fn is_dirty(&self) -> bool`
* `pub fn snapshot(&self) -> ValueMap`: a clone of the map. The dirty flag is unchanged.
* `pub fn mark_issued(&self)`: records that this request handed a token from the cell to a response. Held for the request only and never saved.
* `pub fn is_issued(&self) -> bool`
* No method of this crate places a `TokenCell` in `RequestCtx`.
* `snapfire_fsr_service` implements its `Credentials` trait for this type.

## 4. The Store

### `SessionRecord`

Everything held under one session id. `Debug`, `Clone`, `Default`. `#[non_exhaustive]`: build one with `Default` and the setters.

* `pub data: ValueMap`: the session cell's data.
* `pub identity: Option<Identity>`
* `pub tokens: ValueMap`: the token cell's contents.
* `pub csrf: ValueMap`: the CSRF scheme's state.
* `pub expires: u64`: the end, in seconds since the Unix epoch. A default record has `expires: 0` and has already expired.
* `pub fn data(self, data: ValueMap) -> Self`
* `pub fn identity(self, identity: Option<Identity>) -> Self`
* `pub fn tokens(self, tokens: ValueMap) -> Self`
* `pub fn csrf(self, csrf: ValueMap) -> Self`
* `pub fn expires(self, expires: u64) -> Self`
* `pub fn encode(&self) -> String`: the record as one JSON string in the payload encoding.
* `pub fn decode(text: &str, expires_when_missing: u64) -> Option<SessionRecord>`: what `encode` wrote. `expires_when_missing` is the end of a record encoded without one. `None` for text that is not a JSON object.

### `SessionStore`

The store trait. `Send + Sync`, held as `Arc<dyn SessionStore>`. `BoxFuture` is `futures_util::future::BoxFuture`.

* `fn load(&self, id: &SessionId) -> BoxFuture<'_, Option<SessionRecord>>`: a record that cannot be read is `None`.
* `fn save(&self, id: &SessionId, record: SessionRecord) -> BoxFuture<'_, Result<(), StoreError>>`
* `fn delete(&self, id: &SessionId) -> BoxFuture<'_, Result<(), StoreError>>`
* The layer treats a loaded record past its `expires` as absent and deletes it. A store may drop a record at its end or keep it until asked.

### `MemorySessionStore`

An in-process store over `fibre_cache::Cache<String, SessionRecord>`, keyed by the `SessionId` string.

* `pub fn new(capacity: u64) -> Self`: a cache of that capacity with the `fibre_cache` default shard count.
* `pub fn sharded(capacity: u64, shards: usize) -> Self`: an explicit shard count, rounded up to the next power of two. Capacity is counted across all shards.
* `pub fn with_cache(cache: fibre_cache::Cache<String, SessionRecord>) -> Self`: a cache the caller built.
* Every record is inserted with cost 1 and a time to live of `expires` minus now. Capacity counts sessions.
* A record saved at or past its end is removed instead of stored.
* `save` and `delete` always return `Ok`.

## 5. The Cookie Codec

### `CookieCodec`

How a session id is written into the cookie. `Send + Sync`. Passed to `Sessions::with_codec`.

* `fn encode(&self, id: &SessionId) -> String`
* `fn decode(&self, value: &str) -> Option<SessionId>`
* `fn current(&self, value: &str) -> bool`: whether `value`, which `decode` accepted, is what `encode` writes now. `false` sets `Opened::stale`. Default `true`.

### `HmacCodec`

HMAC-SHA256 over the session id, under a `Keyring`. Implements `CookieCodec`.

* `pub fn new(key: &[u8]) -> Self`: a ring of that one key. Any length is accepted, empty included.
* `pub fn over(ring: Arc<Keyring>) -> Self`: over a ring the caller holds and rotates.
* `pub fn keyring(&self) -> &Arc<Keyring>`
* `encode` writes `{id}.{hex hmac}` under the ring's first key.
* `decode` splits on the last `.` and verifies under each key in order.
* `decode` is `None` for a value with no `.`, an empty id, a signature of odd length, a signature with non-hex characters and a signature no key verifies.
* `current` is whether the signature verifies under the first key.

### `Keyring`

An ordered set of HMAC-SHA256 keys behind a lock. The first key signs and every key verifies. `Debug` prints the key count only.

* `pub fn new(key: &[u8]) -> Self`
* `pub fn from_keys<I, K>(keys: I) -> Self` where `I: IntoIterator<Item = K>`, `K: AsRef<[u8]>`: the first key is current. An empty ring signs an empty string and verifies nothing.
* `pub fn rotate(&self, key: &[u8])`: makes `key` current. A key already held moves to the front.
* `pub fn retire(&self, key: &[u8])`: drops the key. Retiring the current key makes the next one current.
* `pub fn replace<I, K>(&self, keys: I)`: the whole list, as `from_keys` takes it.
* `pub fn sign(&self, input: &[u8]) -> String`: the hex HMAC-SHA256 under the current key.
* `pub fn verify(&self, input: &[u8], signature_hex: &str) -> Option<usize>`: the position of the key the signature verifies under, `0` for the current one. Constant-time through the MAC.
* `pub fn is_current(&self, key: &[u8]) -> bool`
* `pub fn len(&self) -> usize`
* `pub fn is_empty(&self) -> bool`

### `SealedCookie`

A session's id and record sealed with AES-256-GCM. Each ring key derives a sealing key as the HMAC-SHA256 of `snapfire session cookie` under it. Values are sealed under the current key and opened under any.

* `pub fn over(ring: Arc<Keyring>) -> Self`
* `pub fn keyring(&self) -> &Arc<Keyring>`
* `pub fn seal(&self, id: &SessionId, record: &SessionRecord) -> String`: a [sealed cookie value](#sealed-cookie-value) with a fresh random nonce. Empty for an empty ring.
* `pub fn open(&self, value: &str) -> Option<(SessionId, SessionRecord, usize)>`: the id, the record and the position of the key that opened it.
  * `None` for a value without the `v1.` prefix, base64 that does not decode, a value no key opens or plaintext that is not an id and a record.
  * A record without `expires` decodes with `expires: 0`.
* `pub const COOKIE_LIMIT: usize = 4096`: the most a browser keeps of one cookie, name and value together (RFC 6265 section 6.1).

## 6. Wire Formats

### Cookie value

```
{session id}.{hex hmac-sha256 of the session id}
```

* The id is 32 hex characters as generated. The MAC is 64 lowercase hex characters.

### Sealed cookie value

```
v1.{unpadded base64url of: 12 byte nonce, then AES-256-GCM ciphertext and tag}
```

* The plaintext is the session id, a line break and `SessionRecord::encode`.

### Set-Cookie header

For a fresh, stale, extended or sealed session:

```
{cookie_name}={cookie value}; Path=/; HttpOnly; SameSite=Lax; Max-Age={seconds to the end}[; Secure]
```

From `destroy`:

```
{cookie_name}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0[; Secure]
```

### State cookie

From `state_cookie`:

```
sf_state={epoch nanoseconds in hex}; Path=/; SameSite=Lax; Max-Age={ttl seconds}[; Secure]
```

* It has no `HttpOnly`, so page script can read it.

### CSRF token

* `SingleUse` and `PerSession` tokens are `random_token()`: 64 lowercase hex characters.
* A `Derived` token is the hex HMAC-SHA256 of the ASCII bytes `csrf:{session id}` under the scheme's key. It is stable for the life of the session id.

### Session record

`SessionRecord::encode` writes a JSON object:

* `data`: a map.
* `identity`: `{ subject, claims }` or `null`.
* `tokens`: a map. This is credential material.
* `csrf`: a map.
* `expires`: an integer.

A sealed cookie carries the same JSON. `MemorySessionStore` holds the record as a value and does not encode it.

## 7. The CSRF Scheme

### `CsrfScheme`

How CSRF tokens are issued and checked. `Send + Sync`. A scheme keeps its state in `opened.csrf`.

* `fn issue(&self, opened: &Opened) -> String`: a token for this request.
* `fn verify(&self, opened: &Opened, token: &str) -> bool`
* `fn rotate(&self, opened: &Opened)`: the session was identified or destroyed. Default does nothing.
* `fn memo(&self, opened: &Opened) -> Option<String>`: the key the render memo uses for a subtree that renders the token. `None` when every issue differs. It must not mint a token.
* `fn merge(&self, base: &ValueMap, mine: &ValueMap, latest: &ValueMap) -> ValueMap`: the scheme's state after two requests from the same `base` both changed it. Default takes each key from whichever side changed it, `mine` winning.

### `SingleUse`

A fresh random token per issue, each good for one verification. `Default` is `new(8)`.

* `pub fn new(keep: usize) -> Self`: `keep` is at least 1.
* `issue` adds a `random_token()` to the front of the `outstanding` list and truncates it to `keep`.
* `verify` removes the token it matched and returns whether it found one. Tokens are compared in constant time.
* `rotate` drops the list. `memo` is `None`.
* `merge` keeps tokens either side minted and drops tokens either side spent, newest first, truncated to `keep`.

### `PerSession`

One random token per session. `Default`.

* `pub fn new() -> Self`
* `issue` returns the stored `token`, minting it on the first call.
* `verify` compares in constant time. `rotate` replaces the token. `memo` is the stored token. It is `None` before one is minted.

### `Derived`

A token computed from the session id. Nothing is stored.

* `pub fn new(key: &[u8]) -> Self`: a ring of one key.
* `pub fn over(ring: Arc<Keyring>) -> Self`: a token minted under an older key verifies until that key is retired.
* `issue` is the HMAC of `csrf:{id}`. `verify` recomputes it. `rotate` does nothing. `memo` is the token.

### `random_token`

* `pub fn random_token() -> String`: 32 random bytes as 64 lowercase hex characters.

### `constant_time_eq`

* `pub fn constant_time_eq(a: &str, b: &str) -> bool`: string equality without an early exit on the first differing byte.

## 8. Types From Other Crates

### `SessionCell` and `Identity`

`snapfire_fsr_runtime::SessionCell` and `snapfire_fsr_runtime::Identity`, documented under Request Context in [the runtime reference](../runtime/API_REFERENCE.md).

* `open` builds the cell with `SessionCell::new(data, identity).expiring(expires)`.
* `persist` saves the cell when `is_dirty` or `is_extended` is true. `Max-Age` counts down to `expires()`.

### `Value` and `ValueMap`

`snapfire_fsr_core::Value` and `snapfire_fsr_core::ValueMap`, documented in [the core reference](../core/API_REFERENCE.md).

## 9. Error Handling

### `StoreError`

Why a store or the layer could not write. `Debug`, `Clone`, `PartialEq`, `Eq`, `std::error::Error`.

* `pub struct StoreError(pub String)`
* `pub fn new(message: impl Into<String>) -> Self`
* `Display`: the message.
* Returned by `SessionStore::save`, `SessionStore::delete`, `Sessions::persist`, `Sessions::establish`, `Sessions::destroy` and `Sessions::renew`.
* The layer itself returns one for a session over `max_bytes` and for a sealed cookie over `COOKIE_LIMIT`.

### Failures without an error

| Failure | Result |
| --- | --- |
| No cookie, an unparseable header or no pair named `cookie_name` | `Opened` with `fresh == true` and a new id |
| A tampered id, a bad or non-hex signature, a signature from another key | `HmacCodec::decode` is `None` and `open` gives a fresh session |
| A sealed cookie no key opens or whose record is past its end | `Opened` with `fresh == true` and a new id |
| A valid cookie whose record is gone or expired | `Opened` with `fresh == false`, the same id and empty cells |
| A record a store cannot read | `load` returns `None`, the same as a missing record |
| Nothing dirty at `persist` | `Ok(None)` without touching the store. `Ok(Some(cookie))` when `stale` |
| An existing session persisted | `Ok(None)` after saving, unless it is sealed, stale or extended |
| A CSRF token for another session or a non-hex token | `verify_csrf` returns `false` |

### Panics

* `MemorySessionStore::new` and `sharded` panic with `"session cache build"` if `fibre_cache::CacheBuilder::build` fails. `with_cache` cannot panic.
* `Keyring` panics with `"hmac accepts any key length"` only if HMAC-SHA256 rejects a key, which it does for no length.
* An empty or weak key is accepted without an error.
