# 203. Sessions and identity

The question this chapter answers: where does the session a body reads actually live, who is the request and where does a login go?

**For:** platform developers.

## The session cookie

The browser holds one cookie, `sf_session` by default, holding a session id signed with the key from `[session]`. The session data lives in a store the host owns, keyed by the id. The cookie's signature is what stops a browser from presenting an id it was not given. A cookie that fails the signature is treated as absent and the request gets a fresh session, which the layer marks so a host can tell a first visit from a forged one.

The host opens the session before matching the route and persists it when the response starts. Between those two moments the session is a cell the request context carries: bodies read it, actions write it through their draft and Rust code that took a name over sees the same cell. Persisting writes the store and, when the session is new, sets the cookie with a `Max-Age` counting down to the session's end, the `Secure` flag and the path from configuration. A body sees none of that; [chapter 101](101-actions-and-the-session.md) covers everything a body sees of the session.

## The store

The stock store is in memory, bounded by capacity and dropping each record at its own end, built on the same cache the runtime uses for rendered subtrees: sharded, frequency-aware, so a burst of new sessions evicts the least useful rather than the oldest. `[session]` sets `capacity` and `ttl`, the length of a new session. The end is one number on the record that the cookie and the store both follow and it never slides: a session read a thousand times still ends one `ttl` after it opened unless an action, a route handler or middleware calls `session.extend(seconds)` or Rust code with the request calls `extend` on the cell, so an application can choose to write the session once a day instead of once a request. The memory store works for a single process. With two, a session opened on one host is unknown to the other. There are two ways out. `store = "cookie"` keeps no record on the server: the record rides in the cookie, sealed with AES-256-GCM under a key derived from the session key, so every host with the key opens it. A browser keeps 4096 bytes of a cookie, so a save past that is refused with an error naming the size. Signing out cannot recall a copy of the cookie taken earlier. Otherwise write a `SessionStore` implementation over something shared and pass it to `HostBuilder::session_store`.

```toml
[session]
store = "cookie"
ttl = "2h"
```

The trait is three methods over one record, which carries the data, the identity and the tokens and expires as a unit. A store over Redis is an application's own crate, so the framework carries no Redis client for the deployments that do not want one:

```rust
use futures_util::future::BoxFuture;
use snapfire_fsr_session::{SessionId, SessionRecord, SessionStore, StoreError};

struct RedisSessions {
  pool: deadpool_redis::Pool,
}

impl SessionStore for RedisSessions {
  fn load(&self, id: &SessionId) -> BoxFuture<'_, Option<SessionRecord>> {
    let key = format!("session:{}", id.0);
    Box::pin(async move { self.read(&key).await })
  }

  fn save(&self, id: &SessionId, record: SessionRecord) -> BoxFuture<'_, Result<(), StoreError>> {
    let key = format!("session:{}", id.0);
    Box::pin(async move { self.write_until(&key, record).await })
  }

  fn delete(&self, id: &SessionId) -> BoxFuture<'_, Result<(), StoreError>> {
    let key = format!("session:{}", id.0);
    Box::pin(async move { self.remove(&key).await })
  }
}

let host = Host::from("app.toml")?.session_store(Arc::new(RedisSessions { pool })).build()?;
```

The record holds the tokens in custody, so a store off the machine encrypts what it writes. `SessionRecord::encode` and `SessionRecord::decode` give the record as one JSON string to keep. Set the key's expiry from the record's end so the store drops a session when the cookie does. The session crate's guide has the record's fields.

The signing key is a secret, so it does not belong in `app.toml`. The storefront keeps a development key there with a name that says so; a deployment puts the real one in the YAML overlay the configuration ladder reads, encrypted.

## Custody

Beside the cell, a session holds tokens: the credentials the platform obtained for this user, a bearer token from a login, an API key issued for them. They live in a separate cell the request context does not carry, so no body can read one; the credential interceptor in [chapter 202](202-services-and-transports.md) reads them at the moment of a call. Logging out clears both cells. The two cells persist together, so a token refreshed during a request is saved with the session it belongs to.

## Identity

Who the request is lives on the session as an identity: a subject and a map of claims. A body reads it as `identity.subject` and `identity.claims.<name>`; it sees `null` when nobody is signed in. The identity interceptor carries it onto every outbound call. The host does not decide what a claim means; it delivers the ones the provider gave.

Identity comes from an `IdentityProvider`, a trait in `snapfire_fsr_auth`. Every way of signing in, a password, an OIDC redirect, a passkey or a second factor, is the same exchange repeated: the provider says what the browser does next and the browser answers. The state between steps stays in custody, never in the URL. There are three answers. `Redirect` sends the browser to an identity provider, `Present` hands the page data to act on (a form, a passkey challenge) and `Complete` yields an identity and the tokens that go into custody. `Auth` is the facade around a provider: it begins a sign-in or an enrolment, takes each answer until the flow completes, then clears identity and custody on logout. A provider in another process is the same trait over a client, so an auth service plugs in with `provider = "service"`.

An `[auth]` section mounts that flow on four framework-owned routes, the way the action route is owned: `GET /auth/login` begins a sign-in, `POST /auth/enrol` adds a method to whoever is signed in, `/auth/callback` takes each answer and `POST /auth/logout` clears both cells. A page that asks for JSON gets each step as data. The login page itself stays the application's own route, since auth never renders; `[auth] login` names it. Chapter 200 has the section and what each route does. A host with its own flow can still call the facade directly. The bodies are unaffected either way, since they only ever see `identity`. The stock `fsr serve` carries every method. A host built in Rust turns sign-in on with the host's `auth` feature and adds `oidc` or `passkey` for those methods; without it an `[auth]` section stops the boot naming the feature. A site with no sign-in carries none of it.

`DevProvider` ships for development: a list of users with passwords and optional claims, presenting a password form, no network. `provider = "oidc"` signs in against an OpenID Connect issuer, Google, Microsoft and Apple by `preset` and any other by `issuer`. Until its client secret has a value in development it stands in with a "sign in as" form over the same users, so an application can be written against identity from the first day and switch to the real issuer by setting one secret, without touching a body. `provider = "passkey"` signs in with passkeys, alone or as the `second_factor` after another method; the page runs the browser's prompt with `signIn` from the client.

## The lab

Load the storefront, add a product to the cart, then open the browser's cookie view: one cookie, `sf_session`, an opaque signed id. Edit its value by one character and reload. The cart is empty, because the signature failed and the request got a fresh session; the old one is still in the store until its TTL passes, but nothing can name it.

Then restart the host and reload with the original cookie. The cart is empty again because the memory store did not survive the process. A shared store keeps sessions across restarts.
