# 204. Reloading in place

The question this chapter answers: what changes when the application changes under a running host, and what does not?

**For:** platform developers.

## Everything a request reads is one set of tables

The host keeps the plan, the contracts, the clients, the document head, the static roots, the locales and the identity flow in one structure a request takes once at the edge and keeps for its lifetime. Nothing a request reads can change halfway through it. The sessions are the exception: the store that holds every signed-in user lives beside the tables, not in them.

That split is what makes a reload cheap. `Host::reload` rebuilds the tables through a reloader the builder was given, runs every check a boot runs, then swaps the pointer. A request in flight finishes on the tables it started with; the next one sees the new ones; a rebuild that fails leaves the old ones serving. The sessions never notice.

```rust
let host = Host::from(".")?
  .reloader(|| Host::from("."))
  .build()?;
let report = host.reload()?;
```

The reloader is a builder for the application as it now stands on disk, with whatever the first builder added in Rust added again. `fsr serve` sets one that rereads the project; a Rust binary sets its own, or hands `reload_with` a builder it made.

## What is refused

A reload whose `[session]` differs from the one the running store was built from is refused and leaves the tables alone. The store outlives the reload, so a changed key would sign every existing cookie wrong, and a changed store would strand the records: those want a restart, and the error says so.

Everything else a boot refuses, a reload refuses the same way: a name nothing binds, a contract two files define, a bundle carrying a loader, a site that does not fit. The tables never half-swap.

## The dev loop

`fsr dev` used to restart the process when the generated files changed. It now posts `POST /__fsr/reload` to the running server and prints the report that comes back; the process restarts only when the reload is refused, or when the Rust project itself changed. A page edit keeps every session and every open document, which then hears on `/__fsr/events` that something moved and refreshes its route in place.

| What changed | What happens |
| --- | --- |
| a page, a stylesheet | rebundle, open documents refresh |
| a loader, an action, a client document, the middleware | regenerate, rebundle, reload in place |
| `[session]`, Rust under `src/` | rebuild, restart |

## The lab

Run the portal with `fsr dev app` from `examples/portal_react_ts`, sign in, then change a line of `routes/page.tsx` and save. Watch the loop print a fresh boot report without a `dev: server started` line, reload the page and see the new text with your sign-in intact: the portal's binary sets a reloader, so the loop reloaded it in place. Then change `session.key` in `config/app.toml`: the loop prints `reload refused` and restarts, and the next request is anonymous, since the old cookie no longer verifies. The ops console restarts on every generated change instead, because its binary sets no reloader; add one and it stops.
