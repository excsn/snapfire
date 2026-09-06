# arrivals_react_ts

An arrivals board over three services, one of which takes a second and one of which takes two. It exists because streaming is invisible in a fast example: here the document goes out with the board rendered and a skeleton where each panel will be, and each panel fills as its service answers, so a `Pending` slot is something to watch rather than something to take on faith.

## Running it

```sh
cargo run -p arrivals_react_ts                     # the field is 1s behind, the gates 2s
ARRIVALS_PAUSE_MS=4000 cargo run -p arrivals_react_ts   # slow enough to read the skeletons
```

Then watch the response arrive rather than the page settle:

```sh
curl -sN http://127.0.0.1:8120/ | cat -u
```

Three chunks: the document, the weather, the gate changes.

## How it is put together

| Piece | What it is |
| --- | --- |
| `routes/page.tsx` | the board, from `board.listArrivals`, which answers at once |
| `routes/slots/weather/` | a parallel segment with its own loader and its own `loading.tsx` |
| `routes/slots/gates/` | the same, behind a service that takes twice as long |
| `routes/layout.tsx` | places `children`, `weather` and `gates` |
| `src/backend.rs` | the three services in process, sleeping on purpose |

The stalling is the backend's, not the framework's: `backend::board(pause)` is a `LocalTransport` whose weather method sleeps for `pause` and whose gate method sleeps for twice that. It reads the same `app/clients/board.mock.json` the dev loop and the specs use, so there is one set of answers and only the timing differs. `[clients.board]` is `transport = "mock"`, so `fsr dev app` and `fsr test app` run without the Rust backend at all.

## What it is worth reading for

A row's CSS class is `status-${arrival.code}`, not `status-${arrival.status.replace(" ", "-")}`. The first version cost the whole page its server rendering, and the build said so: `routes/page.tsx:17: .replace(), which is not a builtin`, with the component listed as `client` rather than `lowered`. A string method that is not in the lowered subset is not an error, it is a component that quietly moves to the browser, and the build report is where you find out. The fix was to make the modifier data.

## Tests

`cargo test -p arrivals_react_ts`: that the first chunk carries the board rendered and a skeleton for each panel and neither answer, that each panel arrives in its own chunk in the order the services answer and carries no skeleton, that the settled document holds every value, and that all six components lowered so a stall is the only thing the reader waits for.

`fsr test app`: a row per arrival with its status as a class, and each panel filled from its own service.

Checked in a browser: the board hydrates with an empty console, and with a four second pause the skeletons are on screen while the panels are still out.
