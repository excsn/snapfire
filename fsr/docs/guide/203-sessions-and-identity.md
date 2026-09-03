# 203. Sessions and identity

The question this chapter answers: where does the session a body reads actually live, who is the request and where does a login go?

**For:** platform developers.

## The cookie is a reference

The browser holds one cookie, `sf_session` by default, holding a session id signed with the key from `[session]`. The data is not in the cookie. It is in a store the host owns, keyed by the id; the cookie's signature is what stops a browser from presenting an id it was not given. A cookie that fails the signature is treated as absent and the request gets a fresh session, which the layer marks so a host can tell a first visit from a forged one.

The host opens the session before matching the route and persists it when the response starts. Between those two moments the session is a cell the request context carries: bodies read it, actions write it through their draft and Rust code that took a name over sees the same cell. Persisting writes the store and, when the session is new, sets the cookie with the TTL, the `Secure` flag and the path from configuration. A body sees none of that; [chapter 101](101-actions-and-the-session.md) is its whole view.

## The store

The stock store is in memory, bounded by capacity and expiring by TTL, built on the same cache the runtime uses for rendered subtrees: sharded, frequency-aware, so a burst of new sessions evicts the least useful rather than the oldest. `[session]` sets `capacity` and `ttl`. It is the right store for one process and the wrong one for two, since a session opened on one host is unknown to the other; a `SessionStore` implementation over something shared is the block to write for that; `HostBuilder::session_store` is where it goes.

The signing key is a secret and `app.toml` is not the place for it. The storefront keeps a development key there with a name that says so; a deployment puts the real one in the YAML overlay the configuration ladder reads, encrypted.

## Custody

Beside the cell, a session holds tokens: the credentials the platform obtained for this user, a bearer token from a login, an API key issued for them. They live in a separate cell the request context does not carry, so no body can read one; the credential interceptor in [chapter 202](202-services-and-transports.md) reads them at the moment of a call. Logging out clears both cells. The two cells persist together, so a token refreshed during a request is saved with the session it belongs to.

## Identity

Who the request is lives on the session as an identity: a subject and a map of claims. A body reads it as `identity.subject` and `identity.claims.<name>`; it sees `null` when nobody is signed in. The identity interceptor carries it onto every outbound call. The host does not decide what a claim means; it delivers the ones the provider gave.

Where identity comes from is the `IdentityProvider` seam in `snapfire_fsr_auth`. A provider answers two questions: where to send the browser to log in, with whatever state it needs back, then what to make of the callback, which yields an identity and the tokens that go into custody. The state crosses the round trip in custody too, never in the URL. `Auth` is the facade around a provider: it begins a login, finishes the callback into the session's identity and custody, then clears both on logout. The stock host does not mount login and callback paths yet, so a host that wants a login mounts them beside it and calls the facade; the bodies are unaffected either way, since they only ever see `identity`.

`DevProvider` ships for development: a login path, a list of users with passwords and optional claims, no network. It exists so an application can be written against identity from the first day and swap in the real provider, OIDC or whatever the organisation runs, without touching a body.

## The lab

Load the storefront, add a product to the cart, then open the browser's cookie view: one cookie, `sf_session`, an opaque signed id. Edit its value by one character and reload. The cart is empty, because the signature failed and the request got a fresh session; the old one is still in the store until its TTL passes, but nothing can name it.

Then restart the host and reload with the original cookie. The cart is empty again: the memory store did not survive the process, which is the property a shared store exists to change.
