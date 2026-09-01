# Usage Guide: snapfire_fsr_session

How to open, read, write, persist and destroy a session, where credentials are allowed to live, how CSRF tokens work and how to swap the store or the cookie codec.

## Table of Contents

* [Core Concepts](#core-concepts)
* [Quick Start](#quick-start)
* [Opening a Session](#opening-a-session)
* [Reading and Writing Session Data](#reading-and-writing-session-data)
  * [Setting the Identity](#setting-the-identity)
* [Holding Tokens in Custody](#holding-tokens-in-custody)
  * [Handing Custody to the Service Layer](#handing-custody-to-the-service-layer)
* [Persisting at the Response](#persisting-at-the-response)
* [Destroying a Session on Logout](#destroying-a-session-on-logout)
* [Issuing and Verifying CSRF Tokens](#issuing-and-verifying-csrf-tokens)
* [Wiring the Layer at the HTTP Edge](#wiring-the-layer-at-the-http-edge)
* [Configuring the Cookie](#configuring-the-cookie)
* [Choosing and Tuning a Store](#choosing-and-tuning-a-store)
  * [The Default Store](#the-default-store)
  * [Tuning the Shard Count](#tuning-the-shard-count)
  * [Supplying a Built Cache](#supplying-a-built-cache)
  * [Writing Your Own Store](#writing-your-own-store)
* [Signing and Verifying the Cookie Value](#signing-and-verifying-the-cookie-value)
* [Why Two Cells](#why-two-cells)
* [Error Handling](#error-handling)

## Core Concepts

* **Session id** is a random opaque `SessionId`, sixteen bytes rendered as thirty-two hex characters, meaningless anywhere off the box that stores the record.
* **Cookie** carries the signed session id and nothing else: no session data, no identity, no credential.
* **Signed value** is what `HmacCodec` produces, `{id}.{hex hmac}`, verified constant-time through the mac.
* **Session cell** is `SessionCell` from `snapfire_fsr_runtime`, the application-visible half: data plus the resolved `Identity`; it is the field `RequestCtx::session`.
* **Token cell** is `TokenCell`, the custody half: backend credentials and auth flow state, reachable by the session, auth and service layers only.
* **Custody boundary** is the rule that keeps them apart: everything a loader, action or evaluator can reach goes in the cell, everything else goes in the tokens.
* **Opened** is one request's session as the layer sees it: the id, both cells and whether the request arrived without a valid cookie.
* **Fresh** means no valid cookie arrived, so this id has never been sent to a browser.
* **Dirty** means a cell has been mutated since it was loaded; each cell tracks its own flag.
* **Session record** is what the store holds: `data`, `identity` and `tokens` together under one id.
* **Store** is the `SessionStore` trait, three async methods over the record; `MemorySessionStore` is the in-process implementation.
* **Codec** is the `CookieCodec` trait, encode an id to a cookie value and decode one back; `HmacCodec` is the implementation `Sessions` uses.
* **Open** happens before route matching, **persist** when the response starts, **destroy** on logout.
* **CSRF token** is an hmac over `csrf:{id}`, bound to one session id, stable for that session's life.
* **Value and ValueMap** come from `snapfire_fsr_core`; `ValueMap` is an `IndexMap<String, Value>`. Both cells store their contents in one.

## Quick Start

A session created, written to, persisted and read back through the cookie it set.

```rust
use std::sync::Arc;
use std::time::Duration;

use futures::executor::block_on;
use snapfire_fsr_core::Value;
use snapfire_fsr_session::{MemorySessionStore, SessionConfig, Sessions};

fn main() {
  let sessions = Sessions::new(
    Arc::new(MemorySessionStore::new(4096, Duration::from_secs(8 * 3600))),
    b"a-32-byte-or-longer-signing-key!",
    SessionConfig::default(),
  );

  let opened = block_on(sessions.open(None));
  assert!(opened.fresh);
  opened.cell.insert("visits", Value::int(1i64));

  let set_cookie = block_on(sessions.persist(&opened)).expect("a fresh dirty session sets a cookie");
  let header = set_cookie.split(';').next().unwrap().to_owned();

  let back = block_on(sessions.open(Some(&header)));
  assert!(!back.fresh);
  assert_eq!(back.cell.get("visits"), Some(Value::Int(1)));
}
```

The same three calls in an async adapter, which is where they actually belong.

```rust
let opened = sessions.open(cookie_header.as_deref()).await;
let mut response = route(&request, &opened).await;
if let Some(set_cookie) = sessions.persist(&opened).await {
  response.set_header("set-cookie", set_cookie);
}
```

## Opening a Session

`open` takes the raw `Cookie` header, splits it on `;`, finds the pair whose name matches `SessionConfig::cookie_name` and decodes the value. It is infallible: a missing, malformed, tampered or foreign-signed cookie all produce a fresh session rather than an error.

```rust
let opened = sessions.open(cookie_header.as_deref()).await;
```

Three outcomes are worth telling apart. A fresh session's id has never left the box. A returning session carries an identity. A valid cookie whose record has expired or been deleted keeps the same id and comes back with empty cells.

```rust
let state = if opened.fresh {
  "fresh"
} else if opened.cell.identity().is_some() {
  "signed in"
} else {
  "cookie without a record"
};
```

Call it once per request, before route matching, so the cell and the tokens exist before anything can want them.

## Reading and Writing Session Data

`opened.cell` is a `SessionCell` from `snapfire_fsr_runtime`. It is `Clone` and shares one lock, so cloning it into a `RequestCtx` shares the same session rather than copying it. Every mutation marks it dirty.

```rust
opened.cell.insert("cart_id", Value::Str("c-4711".to_owned()));
let cart = opened.cell.get("cart_id");
let dropped = opened.cell.remove("cart_id");
```

`clear` drops data and identity in one dirty write, which is what a logout wants before the record is deleted.

```rust
opened.cell.clear();
```

### Setting the Identity

`Identity` is `snapfire_fsr_runtime::Identity`, a subject plus a claims map. It rides in the cell rather than the tokens because the application is meant to read it.

```rust
use snapfire_fsr_runtime::Identity;

opened.cell.set_identity(Some(Identity { subject: "norm".into(), claims: Default::default() }));
assert_eq!(opened.cell.identity().unwrap().subject, "norm");
```

Templates and loaders read it through the context rather than the cell directly.

```rust
let who = ctx.identity_value();
```

## Holding Tokens in Custody

`opened.tokens` is a `TokenCell`. It has the same shape as the session cell and the opposite audience: nothing that can reach `RequestCtx` can reach it.

```rust
opened.tokens.set("access_token", Value::Str("secret-abc".to_owned()));
let access = opened.tokens.get("access_token");
let flow_state = opened.tokens.remove("_sf_auth");
```

`merge` folds a whole map in with one dirty write, which is what an identity provider's callback returns.

```rust
opened.tokens.merge(outcome.tokens);
```

`clear` empties custody without touching the cell; the reverse holds too.

```rust
opened.tokens.clear();
```

The boundary is observable: a token written on one request is not visible through the cell on the next.

```rust
let back = sessions.open(Some(&header)).await;
assert_eq!(back.tokens.get("access_token"), Some(Value::Str("secret-abc".to_owned())));
assert_eq!(back.cell.get("access_token"), None);
```

### Handing Custody to the Service Layer

The service registry is bound per request with the identity from the cell and the tokens as `Arc<dyn Credentials>`. `snapfire_fsr_service` implements `Credentials` for `TokenCell`, so a refresh inside an interceptor writes back through `set`; `persist` picks it up.

```rust
use snapfire_fsr_runtime::RequestCtx;

let ctx = RequestCtx {
  params: Default::default(),
  session: opened.cell.clone(),
  csrf: None,
  services: services.bind(opened.cell.identity(), Arc::new(opened.tokens.clone())),
};
```

What comes back from `bind` only calls, so handing it to application code hands over no readable credential.

## Persisting at the Response

`persist` saves the record when either cell is dirty. It returns a `Set-Cookie` value only when the session is also fresh.

```rust
if let Some(set_cookie) = sessions.persist(&opened).await {
  response.headers_mut().append(header::SET_COOKIE, HeaderValue::from_str(&set_cookie)?);
}
```

A token-only write is enough to save and to set the cookie when the session is also fresh, so an auth flow that has stored nothing readable still survives the redirect.

```rust
opened.tokens.set("access_token", Value::Str("secret-abc".to_owned()));
let set_cookie = sessions.persist(&opened).await.expect("a token-only write persists");
```

A fresh session that stored nothing writes nothing and sets no cookie, which is why a crawler never mints a session.

```rust
let opened = sessions.open(None).await;
assert_eq!(sessions.persist(&opened).await, None);
```

An existing session that is dirty saves and returns `None`, because the browser already holds the right cookie.

## Destroying a Session on Logout

`destroy` deletes the record and returns the expiring cookie. It always returns a value, so the header is always worth setting.

```rust
let expire = sessions.destroy(&opened).await;
response.headers_mut().append(header::SET_COOKIE, HeaderValue::from_str(&expire)?);
```

The record is gone even if the cookie is replayed, since the id decodes but loads nothing.

```rust
let after = sessions.open(Some(&header)).await;
assert!(after.cell.identity().is_none());
assert_eq!(after.tokens.get("refresh_token"), None);
```

Clearing the cells first, then destroying, is what the auth crate and the adapter do between them.

```rust
auth.logout(&opened);
let expire = sessions.destroy(&opened).await;
```

## Issuing and Verifying CSRF Tokens

`csrf_token` signs `csrf:{id}` with the same key that signs the cookie. It is deterministic, so the same session gets the same token for as long as the id lives.

```rust
let csrf = sessions.csrf_token(&opened.id);
```

Pass it into the render so pages can embed it in forms, then check it on the way back.

```rust
if !sessions.verify_csrf(&opened.id, &submitted) {
  return HttpResponse::Forbidden().body("csrf verification failed");
}
```

A token never validates for another session. A wrong or non-hex token returns `false` rather than panicking.

```rust
assert!(sessions.verify_csrf(&a, &token));
assert!(!sessions.verify_csrf(&b, &token));
assert!(!sessions.verify_csrf(&a, "deadbeef"));
```

## Wiring the Layer at the HTTP Edge

The layer lives at the HTTP adapter because cookies are HTTP. One handler opens, routes, then persists; logout returns before persist because it has already written its own cookie.

```rust
async fn handle(req: HttpRequest, app: Data<Arc<AppCore>>, body: Bytes) -> HttpResponse {
  let cookie_header = req
    .headers()
    .get(header::COOKIE)
    .and_then(|v| v.to_str().ok())
    .map(str::to_owned);
  let opened = app.sessions.open(cookie_header.as_deref()).await;

  if req.path() == "/auth/logout" && req.method() == Method::POST {
    return handle_logout(&app, &opened, body).await;
  }

  let mut response = route(&req, &app, &opened, body).await;
  if let Some(set_cookie) = app.sessions.persist(&opened).await {
    if let Ok(value) = header::HeaderValue::from_str(&set_cookie) {
      response.headers_mut().append(header::SET_COOKIE, value);
    }
  }
  response
}
```

Inside the route, the cell and the CSRF token go into the render; the tokens go only to the service binding.

```rust
let csrf = app.sessions.csrf_token(&opened.id);
let incoming = Incoming::new(opened.cell.clone(), Some(csrf), Arc::new(opened.tokens.clone()));
```

Form-posted actions verify CSRF before the action runs.

```rust
let token = match params.shift_remove("_csrf") {
  Some(Value::Str(token)) => token,
  _ => String::new(),
};
if !app.sessions.verify_csrf(&opened.id, &token) {
  return HttpResponse::Forbidden().body("csrf verification failed");
}
```

## Configuring the Cookie

`SessionConfig` has three fields and a `Default`: the name is `sf_session`, the lifetime is eight hours and `secure` is off so a plain-HTTP dev server works.

```rust
let config = SessionConfig::default();
```

Change what you need and leave the rest.

```rust
let config = SessionConfig {
  cookie_name: "app_sid".to_owned(),
  ttl: Duration::from_secs(24 * 3600),
  secure: true,
};
```

`ttl` is the cookie's `Max-Age` only. The record's lifetime is the store's, so set the two together or the browser will keep sending an id whose record has gone.

```rust
let ttl = Duration::from_secs(8 * 3600);
let sessions = Sessions::new(
  Arc::new(MemorySessionStore::new(4096, ttl)),
  key,
  SessionConfig { ttl, ..SessionConfig::default() },
);
```

`Path=/`, `HttpOnly` and `SameSite=Lax` are always set and are not configurable.

## Choosing and Tuning a Store

### The Default Store

`MemorySessionStore::new` takes a capacity in entries and a time-to-idle. Idle, not absolute: an active session keeps sliding forward, an abandoned one falls out.

```rust
let store = MemorySessionStore::new(4096, Duration::from_secs(8 * 3600));
```

It is a single-process store. Two app instances behind a load balancer do not share it.

### Tuning the Shard Count

`sharded` adds an explicit shard count for lock contention. `fibre_cache` rounds the count up to the next power of two; its own default is derived from the CPU count.

```rust
let store = MemorySessionStore::sharded(4096, Duration::from_secs(8 * 3600), 4);
```

Capacity is accounted across all shards rather than divided by them, so raising the shard count trades against the fixed per-shard policy and timer structures, never against usable capacity. A store built for sixty-four entries holds sixty-four.

```rust
let store = MemorySessionStore::new(64, Duration::from_secs(60));
let ids: Vec<SessionId> = (0..64).map(|_| SessionId::generate()).collect();
for id in &ids {
  store.save(id, Default::default()).await;
}
let mut resident = 0;
for id in &ids {
  resident += store.load(id).await.is_some() as usize;
}
assert_eq!(resident, 64);
```

### Supplying a Built Cache

`with_cache` is the escape hatch for an eviction listener, a hasher or a timer preset the two constructors above do not reach.

```rust
let store = MemorySessionStore::with_cache(
  fibre_cache::CacheBuilder::default()
    .capacity(32)
    .time_to_idle(Duration::from_secs(60))
    .shards(2)
    .build()
    .unwrap(),
);
```

The value type is fixed: the cache must be a `fibre_cache::Cache<String, SessionRecord>`, keyed by the session id's string.

### Writing Your Own Store

`SessionStore` is three methods returning `BoxFuture`, which is what lets a real backend await. `SessionRecord` is the whole unit: data, identity and tokens travel and expire together.

```rust
use futures_util::future::BoxFuture;
use snapfire_fsr_session::{SessionId, SessionRecord, SessionStore};

struct RedisSessionStore {
  pool: RedisPool,
}

impl SessionStore for RedisSessionStore {
  fn load(&self, id: &SessionId) -> BoxFuture<'_, Option<SessionRecord>> {
    let key = id.0.clone();
    Box::pin(async move { self.fetch_and_decode(&key).await })
  }

  fn save(&self, id: &SessionId, record: SessionRecord) -> BoxFuture<'_, ()> {
    let key = id.0.clone();
    Box::pin(async move { self.write_with_expiry(&key, record).await })
  }

  fn delete(&self, id: &SessionId) -> BoxFuture<'_, ()> {
    let key = id.0.clone();
    Box::pin(async move { self.drop_key(&key).await })
  }
}
```

Serialising a record means serialising credentials, so an off-box store is the point where encryption at rest stops being optional.

```rust
let sessions = Sessions::new(Arc::new(RedisSessionStore::new(pool)), key, SessionConfig::default());
```

## Signing and Verifying the Cookie Value

`HmacCodec` is HMAC-SHA256 over the id, rendered as `{id}.{hex hmac}`. It accepts a key of any length; verification runs constant-time through the mac.

```rust
use snapfire_fsr_session::{CookieCodec, HmacCodec, SessionId};

let codec = HmacCodec::new(b"a-32-byte-or-longer-signing-key!");
let id = SessionId::generate();
let value = codec.encode(&id);
assert_eq!(codec.decode(&value), Some(id));
```

A tampered id, a bad signature, a non-hex signature, a missing `.` and a signature from another key all decode to `None`.

```rust
assert_eq!(codec.decode("not-signed"), None);
assert_eq!(HmacCodec::new(b"a-different-key").decode(&value), None);
```

`CookieCodec` is a trait, so an alternative signing scheme can implement it. `Sessions::new` builds an `HmacCodec` from the key it is given rather than accepting a codec, so a different implementation is used by calling it directly, not by passing it to `Sessions`.

```rust
let signed = my_codec.encode(&opened.id);
```

Changing the key invalidates every cookie signed with the old one; those requests open fresh.

## Why Two Cells

One map with a naming convention would be one `get` away from leaking. The split is structural instead: `RequestCtx` has a `session` field and no `tokens` field, so a loader, an action or an evaluator cannot reach a credential no matter what key it guesses. What crosses to application code is the identity, the data the application itself wrote and a `ServiceHandle` that calls without exposing what it calls with.

```rust
let ctx = RequestCtx { session: opened.cell.clone(), ..Default::default() };
assert_eq!(ctx.session.get("access_token"), None);
```

The same split explains where auth flow state lives. A login's provider state is not the application's business and must be bound to the session that began the flow, so it rides custody under `_sf_auth`; the callback consumes it from there.

```rust
opened.tokens.set("_sf_auth", Value::Map(state));
```

Both cells are `Arc`-backed and independently dirty-tracked, which is what lets `persist` save on either flag while `destroy` forgets both at once.

## Error Handling

The crate defines no error type. Every fallible path is expressed as an `Option`, a `bool` or a fresh session, because a bad cookie is an ordinary request rather than a fault.

```rust
match sessions.persist(&opened).await {
  Some(set_cookie) => response.set_header("set-cookie", set_cookie),
  None => {}
}

if !sessions.verify_csrf(&opened.id, &submitted) {
  return forbidden();
}

match codec.decode(value) {
  Some(id) => id,
  None => SessionId::generate(),
}
```

Three things behave in ways worth knowing before they surprise you.

* `Sessions::open`, `persist` and `destroy` never fail. A tampered or foreign cookie yields a fresh session; a valid cookie with no record yields the same id with empty cells and `fresh` false, so no new cookie is sent.
* `MemorySessionStore::new` and `sharded` panic if `fibre_cache` refuses the builder. Build the cache yourself and pass it to `with_cache` when you want to handle that.
* `HmacCodec::new` accepts any key length without complaint, so a short key is a silent weakness rather than an error. Use thirty-two bytes or more.

Errors raised further out belong to their own crates: `snapfire_fsr_auth` returns `AuthError` from the login callback; the service layer's failures surface through `snapfire_fsr_service`.
