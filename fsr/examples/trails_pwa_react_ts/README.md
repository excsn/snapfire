# trails_pwa_react_ts

Four walking trails to read on the way. The example exists for `[pwa]`: the page installs as an app, a trail opened once with a signal is still there without one and a deploy tells an open page that a new version is ready.

## Running it

```sh
cargo run -p trails_pwa_react_ts
```

Then open http://127.0.0.1:8205/. Development links the manifest and registers no worker, so a rebuild is never hidden behind a cache. To see the worker, run the production shape:

```sh
fsr bundle app --out /tmp/trails
cp fibre_logging.yaml /tmp/trails/
cd /tmp/trails && C5_SESSION__KEY=$(openssl rand -hex 32) RELEASE_ENV=production <path to the built trails_pwa_react_ts binary>
```

## What is on show

| On the page | What does it |
| --- | --- |
| The browser offers to install it | `[pwa]` in `config/app.toml` writes `/manifest.webmanifest` from `name`, `short_name`, `theme_color` and `background_color`, with the 192 and 512 icons from `app/icons/`. |
| A trail you opened reads the same with the server stopped | The host's worker at `/sw.js` keeps each page and payload it fetched that is not `private`. When the network fails it answers from those. |
| A trail you never opened shows "No signal" | `offline = "/offline"`: the worker precaches that route and shows it for a page it never kept. |
| "Offline: you are reading saved pages." in the header | `Status` reads `online` from `@snapfire/fsr/client` through `useStore`; the client keeps it current from the browser's `online` and `offline` events. |
| "A new version is ready. Open it" after a deploy | The new build's worker takes over the open page and the client sets `updateReady`. The page keeps running on the modules it loaded, which the worker kept from the build before. |

## Checking it

```sh
cargo nextest run -p trails_pwa_react_ts
fsr test app
```

The Rust tests render the head and the manifest in-process. `fsr test` runs the loader tests and the specs, which set `online` and `updateReady` and read what `Status` shows.
