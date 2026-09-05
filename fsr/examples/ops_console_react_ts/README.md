# ops_console_react_ts

An operations console for a small fleet of build agents, built the way the storefront is: loaders, actions, a handler and middleware in TypeScript lowered by the build and run by the Rust host; pages and layouts as React islands rendered on the server and hydrated in the browser; one backend it does not own, described only by the OpenAPI document it publishes.

Where the storefront is the first thing to read, this is the second: it exercises what the storefront never touches.

| It shows | Where |
| --- | --- |
| A store the whole document shares, seeded by two layouts, the inner one winning a key both set | `routes/layout.loader.ts`, `routes/agents/layout.loader.ts`, `src/store.ts` |
| A key nothing seeds, written in one root and read in another | `fleet/selected`, written by the list, shown in the header |
| A derived key the server seeds and the browser keeps recomputing | `fleet/headline` in `src/main.ts` and the root loader |
| A setting held in the session, written optimistically, seeded back on every request | row density in `routes/settings/`, read by `src/ui/AgentRows.tsx` |
| A nested layout with a parallel slot beside it and a slot of its own | `routes/agents/layout.tsx` under `routes/layout.tsx`, `routes/slots/alerts/` |
| Both fallback spellings for a slot | `{alerts ?? …}` in the root layout, `<Slot name="peek">…</Slot>` in the agents layout |
| Two routes with a variant each, in slots two layouts apart | `routes/settings/page.drawer.tsx` for the root, `routes/agents/view/[id]/page.peek.tsx` for the agents layout |
| The three kinds of link | `full` on an agent's name, `into="peek"` on its peek button, a plain link from an alert the server intercepts only when the origin shares the declaring layout |
| Two segments streaming behind their own fallbacks in one document | the alerts slot and an agent page, each with a `loading.tsx` |
| An island timed on idle and one timed on visibility | `island(TipList, { when: "idle" })` on the summary, `<Island when="visible">` around the job timeline |
| An error boundary on a nested segment | `routes/agents/view/[id]/error.tsx` |
| Two locales, the default unprefixed and French under `/fr_FR/`, remembered in a cookie once chosen | `[locales]` in `config/app.toml`, the picker in `src/ui/LanguagePicker.tsx`, `useLocale()` in `routes/help/page.tsx` |
| A login the host serves over a users file, a guarded route, the identity and the CSRF token as layout props, a loader whose backend call carries the session's token | `[auth]` in `config/app.toml`, `config/auth.toml`, `routes/login/`, `routes/account/`, the guard in `middleware.ts`, `src/ui/Header.tsx` |

Nothing here is prerendered: the root layout reads the session for the header, so every route is dynamic. The storefront's `/about` is the example of a prerendered page.

## Run it

From a fresh checkout, the same four steps as the storefront, with this directory in the third and fourth:

```sh
cargo build -p snapfire_compiler -p snapfire_fsr_cli
cd fsr/client && ../../target/debug/snapfirec --source-map --public-path /static/js/fsr --import-map importmap.json
cd ../examples/ops_console_react_ts && ../../../target/debug/fsr types app
../../../target/debug/fsr dev app
```

Then open <http://127.0.0.1:8090>. The fleet backend listens on 8091. Sign in as `alice` / `wonder` or `bob` / `builder`; the accounts are in `config/auth.toml`.

## Try it

Open the summary, then `Agents`. Click a name: its page renders under the list, and the list stays as it is. Click `peek` instead: the same route renders into the panel beside the list, and the URL changes just the same. Pick a region first and either one keeps it.

Open the gear: the settings route renders into a drawer over the console. Switch the rows to compact and the list behind the drawer changes before the server has answered; reload, and it is still compact, because the setting lives in the session and the root layout seeds it back into the store. `watch` an agent from the list and the header's count moves the same way, then the settings drawer lists it.

Acknowledge an alert in the right column: the count in the header and the headline beside it move at once, and the revalidation that follows agrees. From the agent list, `open` on an alert peeks at that agent; from the summary, where the agents layout is not on the page, the same link navigates.

## Tests

`cargo test` runs the Rust suite in `tests/console.rs` over a mocked fleet: the two seeds and their merge, the two streamed segments, both fallbacks, the variant each slot picks and the actions. `fsr test app` runs the page specs under `app/tests/` in QuickJS over linkedom: the store across roots, the derived key, the optimistic writes and both intercepts.
