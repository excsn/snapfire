# API Reference: snapfire_fsr_session

The session layer for SnapFire FSR: signed cookies, the session store, token custody and CSRF.

## Contents

* [1. Session Identity](#1-session-identity)
  * [SessionId](#sessionid)
* [2. The Session Layer](#2-the-session-layer)
  * [SessionConfig](#sessionconfig)
  * [Opened](#opened)
  * [Sessions](#sessions)
* [3. Token Custody](#3-token-custody)
  * [TokenCell](#tokencell)
* [4. The Store](#4-the-store)
  * [SessionRecord](#sessionrecord)
  * [SessionStore](#sessionstore)
  * [MemorySessionStore](#memorysessionstore)
* [5. The Cookie Codec](#5-the-cookie-codec)
  * [CookieCodec](#cookiecodec)
  * [HmacCodec](#hmaccodec)
* [6. Wire Formats](#6-wire-formats)
  * [Cookie value](#cookie-value)
  * [Set-Cookie header](#set-cookie-header)
  * [CSRF token](#csrf-token)
  * [Session record](#session-record)
* [7. Types From Other Crates](#7-types-from-other-crates)
  * [SessionCell](#sessioncell)
  * [Identity](#identity)
* [8. Error Handling](#8-error-handling)

## 1. Session Identity

### SessionId

A random opaque identifier. The cookie carries this signed, never session data and never a backend credential.

* `pub struct SessionId(pub String)`
* `SessionId::generate() -> SessionId` produces sixteen random bytes rendered as thirty-two lowercase hex characters.
* Derives `Debug`, `Clone`, `PartialEq`, `Eq`, `Hash`; implements `Display` as the bare string.

The inner `String` is the store key. Nothing in the crate validates its shape on the way back in, so a decoded id is exactly the text that was signed.

## 2. The Session Layer

### SessionConfig

Cookie policy for the layer.

* `pub cookie_name: String` matched against the pairs of the `Cookie` header and written as the name in `Set-Cookie`. Default `"sf_session"`.
* `pub ttl: Duration` written as `Max-Age` in seconds. Default eight hours. It bounds the cookie only; the record's lifetime belongs to the store.
* `pub secure: bool` appends `; Secure`. Default `false`.
* `SessionConfig::default() -> SessionConfig`.

`Path=/`, `HttpOnly` and `SameSite=Lax` are always emitted and cannot be configured.

### Opened

One request's session as the layer sees it.

* `pub id: SessionId`
* `pub cell: SessionCell` is the application-visible half and the value that goes into `RequestCtx::session`.
* `pub tokens: TokenCell` is the custody half and never enters `RequestCtx`.
* `pub fresh: bool` is `true` only when no valid cookie arrived. A cookie that verifies but whose record is gone yields `fresh == false` with empty cells.

Not `Clone`. Passed by reference to `persist` and `destroy`.

### Sessions

The session layer facade. Holds the store, the config and an `HmacCodec` built from the key.

* `Sessions::new(store: Arc<dyn SessionStore>, key: &[u8], config: SessionConfig) -> Sessions`. Any key length is accepted.
* `async fn open(&self, cookie_header: Option<&str>) -> Opened`. Infallible. A missing, malformed, tampered or foreign-signed cookie yields a fresh session.
* `async fn persist(&self, opened: &Opened) -> Option<String>`. Returns without touching the store when neither `opened.cell` nor `opened.tokens` is dirty. Otherwise it saves the record and returns `Some(set_cookie)` when `opened.fresh` is `true`, `None` when it is not.
* `async fn establish(&self, opened: &Opened) -> Option<String>`. Saves the record whether or not anything is dirty and returns `Some(set_cookie)` when `opened.fresh` is `true`; for a host that bound something to the id, such as a CSRF token, before the session held anything.
* `async fn destroy(&self, opened: &Opened) -> String`. Deletes the record and always returns the expiring cookie. It does not clear the cells.
* `fn csrf_token(&self, id: &SessionId) -> String`. Deterministic for a given id and key.
* `fn verify_csrf(&self, id: &SessionId, token: &str) -> bool`. A token signed for one id never verifies for another; a non-hex token returns `false`.

The `Cookie` header is parsed by splitting on `;`, trimming each pair and matching `cookie_name` followed by `=`. The value is taken verbatim, with no percent-decoding and no quoted-string handling.

## 3. Token Custody

### TokenCell

Server-side custody for backend credentials and auth flow state. `Arc`-backed behind a `parking_lot::Mutex`, so a clone shares one set of tokens and one dirty flag.

* `TokenCell::new(tokens: ValueMap) -> TokenCell` starts clean.
* `TokenCell::default() -> TokenCell` is empty and clean.
* `fn get(&self, key: &str) -> Option<Value>` clones the value out.
* `fn set(&self, key: impl Into<String>, value: Value)` marks dirty unconditionally.
* `fn remove(&self, key: &str) -> Option<Value>` marks dirty only when something was removed. Removal shifts the remaining order, since the map is an `IndexMap`.
* `fn merge(&self, tokens: ValueMap)` inserts every pair and marks dirty unconditionally, including for an empty map.
* `fn clear(&self)` empties the map and marks dirty.
* `fn is_dirty(&self) -> bool`.
* `fn snapshot(&self) -> ValueMap` clones the map; it does not clear the dirty flag.

Implements `Clone` and `Default`. There is no method that hands the cell to `RequestCtx`, which is the custody boundary. `snapfire_fsr_service` implements its `Credentials` trait for this type, so an interceptor that refreshes a credential writes back through `set`; the next `persist` stores it.

## 4. The Store

### SessionRecord

Everything held under one session id. Data, identity and tokens are saved, loaded and deleted together.

* `pub data: ValueMap` is the session cell's contents.
* `pub identity: Option<Identity>`
* `pub tokens: ValueMap` is the token cell's contents.
* Derives `Debug`, `Clone`, `Default`.

### SessionStore

The store seam. `Send + Sync`, held as `Arc<dyn SessionStore>`.

* `fn load(&self, id: &SessionId) -> BoxFuture<'_, Option<SessionRecord>>`
* `fn save(&self, id: &SessionId, record: SessionRecord) -> BoxFuture<'_, ()>`
* `fn delete(&self, id: &SessionId) -> BoxFuture<'_, ()>`

`BoxFuture` is `futures_util::future::BoxFuture`. `save` and `delete` return no result, so an implementation reports a backend failure by its own means, not through these signatures. Record expiry is the implementation's responsibility; the layer never asks for it.

### MemorySessionStore

In-process store over a `fibre_cache::Cache<String, SessionRecord>` keyed by `SessionId`'s inner string. Single process only.

* `MemorySessionStore::new(capacity: u64, ttl: Duration) -> MemorySessionStore` builds a cache with that capacity and `time_to_idle(ttl)`, leaving the shard count at the `fibre_cache` default, which is derived from the CPU count.
* `MemorySessionStore::sharded(capacity: u64, ttl: Duration, shards: usize) -> MemorySessionStore` adds an explicit shard count. `fibre_cache` rounds it up to the next power of two. Capacity is accounted across all shards rather than divided by them, so the shard count trades lock contention against the fixed per-shard policy and timer structures, never against usable capacity.
* `MemorySessionStore::with_cache(cache: fibre_cache::Cache<String, SessionRecord>) -> MemorySessionStore` takes a cache built any way the caller likes.

`new` and `sharded` panic if `fibre_cache::CacheBuilder::build` fails. `with_cache` cannot panic. Every record is inserted with a cost of 1, so capacity counts sessions rather than bytes. The `ttl` is time to idle, so an active session slides forward and an untouched one expires.

## 5. The Cookie Codec

### CookieCodec

The signing seam. `Send + Sync`.

* `fn encode(&self, id: &SessionId) -> String`
* `fn decode(&self, value: &str) -> Option<SessionId>`

`Sessions::new` constructs an `HmacCodec` from the key it is given and does not accept a `dyn CookieCodec`, so an alternative implementation is called directly rather than installed into `Sessions`.

### HmacCodec

HMAC-SHA256 over the session id.

* `HmacCodec::new(key: &[u8]) -> HmacCodec` copies the key. Any length is accepted, including empty.
* `fn encode(&self, id: &SessionId) -> String` produces `{id}.{hex hmac}`.
* `fn decode(&self, value: &str) -> Option<SessionId>` splits on the first `.`, rejects an empty id and verifies the signature. Verification is constant-time through the mac.

`decode` returns `None` for: no `.` in the value, an empty id, a signature of odd length, a signature containing non-hex characters and any signature that does not verify under this key. Signing and verification of arbitrary byte strings are crate-internal; the only public entry points are the two trait methods and `Sessions::csrf_token` / `Sessions::verify_csrf`.

## 6. Wire Formats

### Cookie value

```
{session id}.{hex hmac-sha256 of the session id}
```

The id is thirty-two hex characters as generated; the mac is sixty-four lowercase hex characters. Nothing else is encoded in the cookie.

### Set-Cookie header

`persist` returns, for a fresh session only:

```
{cookie_name}={cookie value}; Path=/; HttpOnly; SameSite=Lax; Max-Age={ttl seconds}[; Secure]
```

`destroy` returns, always:

```
{cookie_name}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0[; Secure]
```

`; Secure` is appended when `SessionConfig::secure` is `true`.

### CSRF token

Sixty-four lowercase hex characters, the HMAC-SHA256 of the ASCII bytes:

```
csrf:{session id}
```

The layer's signing key is used, so the token is stable for the life of the session id and is not single use.

### Session record

`SessionRecord` is a plain Rust struct with no encoding of its own. `MemorySessionStore` holds it as a value in memory. Any store that leaves the process must supply its own serialisation; that serialisation covers `tokens`, which is credential material.

## 7. Types From Other Crates

### SessionCell

`snapfire_fsr_runtime::SessionCell`. The application-visible session, `Arc`-backed and clonable into `RequestCtx::session`.

* `SessionCell::new(data: ValueMap, identity: Option<Identity>) -> SessionCell` starts clean.
* `fn get(&self, key: &str) -> Option<Value>`
* `fn insert(&self, key: impl Into<String>, value: Value)` marks dirty.
* `fn remove(&self, key: &str) -> Option<Value>` marks dirty only when something was removed.
* `fn identity(&self) -> Option<Identity>`
* `fn set_identity(&self, identity: Option<Identity>)` marks dirty.
* `fn clear(&self)` drops data and identity in one dirty write.
* `fn is_dirty(&self) -> bool`
* `fn snapshot(&self) -> (ValueMap, Option<Identity>)`

### Identity

`snapfire_fsr_runtime::Identity`. Who the request is, resolved before anything loads.

* `pub subject: String`
* `pub claims: ValueMap`
* Derives `Debug`, `Clone`, `PartialEq`.

`Value` and `ValueMap` come from `snapfire_fsr_core`, where `ValueMap` is `IndexMap<String, Value>`.

## 8. Error Handling

The crate defines no error type; no method returns a `Result`.

| Failure | How it surfaces |
| --- | --- |
| No cookie, an unparseable header or an unknown cookie name | `Opened` with `fresh == true` and a new id |
| A tampered id, a bad or non-hex signature, a signature from another key | `HmacCodec::decode` returns `None`, so `open` yields a fresh session |
| A valid cookie whose record is gone | `Opened` with `fresh == false`, the same id and empty cells |
| Neither cell dirty at persist time | `persist` returns `None` without touching the store |
| An existing session persisted | `persist` returns `None` after saving |
| A CSRF token for another session or a non-hex token | `verify_csrf` returns `false` |
| A backend store failing to save or delete | Not representable in `SessionStore`; the implementation handles it |

Two calls panic rather than returning:

* `MemorySessionStore::new` and `MemorySessionStore::sharded` panic with `"session cache build"` if `fibre_cache::CacheBuilder::build` returns an error. `with_cache` lets the caller handle that instead.
* `HmacCodec` panics with `"hmac accepts any key length"` only if HMAC-SHA256 rejects the key, which it does not for any length.

`HmacCodec::new` accepts a key of any length without complaint, so a weak key is a silent condition rather than a reported error.
