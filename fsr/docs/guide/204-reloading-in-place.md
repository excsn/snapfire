# 204. Reloading in place

The question this chapter answers: what changes when the application changes under a running host and what does not?

**For:** platform developers.

## Everything a request reads is one set of tables

The host keeps the plan, the contracts, the clients, the document head, the static roots, the locales and the identity flow in one structure a request takes once at the edge and keeps for its lifetime. Nothing a request reads can change halfway through it. The sessions are the exception: the store that holds every signed-in user lives beside the tables, not in them.

That split is what makes a reload cheap. `Host::reload` reads the artifact again through the loader the host was built from, runs every check a boot runs, then swaps the pointer. A request in flight finishes on the tables it started with; the next one sees the new ones; a rebuild that fails leaves the old ones serving. The sessions never notice.

```rust
let host = Host::from(".")?.build()?;
let report = host.reload()?;
```

That is enough for a host built from a configuration alone, which `fsr serve` is. A Rust binary that added services, a store, an evaluator or a mount by hand cannot be rebuilt by a reread, since the reread would drop them; it gives the builder a reloader, a builder for the application as it now stands with those added again. `reload_with` takes such a builder made on the spot.

## What is refused

A reload whose `[session]` differs from the one the running store was built from is refused and leaves the tables alone. The store outlives the reload, so a changed store would strand the records and a changed ttl would disagree with it: those want a restart and the error says so. The keys are the exception: a changed `key` or `previous_keys` is a rotation and a reload applies it to the running ring, so an old key kept in `previous_keys` still verifies until it is removed.

Everything else a boot refuses, a reload refuses the same way: a name nothing binds, a contract two files define, a bundle carrying a loader, a site that does not fit. The tables never half-swap.

## The dev loop

`fsr dev` used to restart the process when the generated files changed. It now posts `POST /__fsr/reload` to the running server and prints the report that comes back; the process restarts only when the reload is refused or when the Rust project itself changed. A page edit keeps every session and every open document, which then hears on `/__fsr/events` that something moved and refreshes its route in place.

| What changed | What happens |
| --- | --- |
| a page, a stylesheet | rebundle, open documents refresh |
| a loader, an action, a client document, the middleware | regenerate, rebundle, reload in place |
| `[session]` apart from the keys, Rust under `src/` | rebuild, restart |
| `session.key`, `session.previous_keys` | reload, which rotates the ring |

## The lab

Run the portal with `fsr dev app` from `examples/portal_react_ts`, sign in, then change a line of `routes/page.tsx` and save. Watch the loop print a fresh boot report without a `dev: server started` line, reload the page and see the new text with your sign-in intact: the portal's binary adds nothing to its configuration, so the loop had the host reread it in place. Then change `session.key` in `config/app.toml` without keeping the old value in `previous_keys`: the loop reloads in place and the next request is anonymous, since the old cookie no longer verifies. Change `session.ttl` instead and the loop prints `reload refused` and restarts. The ops console restarts on every generated change instead, because its binary registers extensions and sets no reloader; add one and it stops.
