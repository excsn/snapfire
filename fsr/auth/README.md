# snapfire_fsr_auth

MPL-2.0. Pre-release, unpublished.

Auth for Snapfire FSR: the front door of the session layer, covering how an anonymous session becomes an identified one and where backend tokens live. `IdentityProvider` is the seam, `Auth` is the flow over it (`login`, `callback`, `logout`) and `DevProvider` is a name-and-password implementation for development. The crate never renders and owns no HTTP endpoints; the login page is an ordinary route through the ordinary plan, identity reaches templates only as the injected `identity` prop and the flow endpoints live at whatever HTTP adapter the application brings. Task-shaped instructions are in [README.USAGE.md](README.USAGE.md); signatures are in [API_REFERENCE.md](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_fsr_auth = { path = "../auth" }
snapfire_fsr_core = { path = "../core" }
snapfire_fsr_runtime = { path = "../runtime" }
snapfire_fsr_session = { path = "../session" }
```

`snapfire_fsr_session` is not optional: `Auth` takes an `Opened` session on every call. No cargo features are defined; everything the crate exports is always compiled.

## What to reach for

| What you are trying to do | What to reach for |
| --- | --- |
| Start a login and get the URL to redirect the browser to | `Auth::login` |
| Finish a login from the provider's response | `Auth::callback` |
| Forget identity plus backend tokens on the way out | `Auth::logout` |
| Plug in a provider of your own | `IdentityProvider` |
| Log in against a fixed user table while developing | `DevProvider` |
| Give the browser a redirect plus state that must survive the round trip | `Begin` |
| Hand back an identity plus the tokens the backend tier will need | `AuthOutcome` |
| Turn a failed flow into an HTTP response | `AuthError::http_status` |

## Status

Pre-release and unpublished; nothing here is on crates.io yet, no compatibility is promised across versions and the API moves with the rest of FSR. `DevProvider` is the only provider that ships, so an application that needs a real identity source writes its own `IdentityProvider`. The crate is exercised end to end by the `advanced_tera_app` example under `fsr/examples/`, which wires the flow endpoints, the login route and the logout form. It carries 5 integration tests in `tests/auth.rs`.
