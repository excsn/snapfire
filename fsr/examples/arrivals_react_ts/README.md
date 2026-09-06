# arrivals_react_ts

An arrivals board on a field whose clock runs fast. Flights land, depart, run late and change gates while you watch, the weather drifts, and the page follows without a reload or a poll. Two things are on show: streaming, because the panels arrive after the board does, and push, because the server says when something changed.

## Running it

```sh
cargo run -p arrivals_react_ts
```

| Variable | Default | What it does |
| --- | --- | --- |
| `ARRIVALS_SPEED` | `2` | simulated minutes per real second |
| `ARRIVALS_TICK_MS` | `3000` | how often the server publishes `board` |
| `ARRIVALS_PAUSE_MS` | `900` | how long the weather takes; the gate system takes twice that |

The field opens at 07:00 and the schedule runs out before 10:20, where the clock wraps and the morning happens again, so the board is never empty.

To watch the document stream rather than the page settle:

```sh
curl -sN http://127.0.0.1:8120/ | cat -u
```

Three chunks: the board, then the weather, then the gate changes.

## How it is put together

| Piece | What it is |
| --- | --- |
| `routes/page.tsx` | the clock and both boards, from one call to `board.getBoard` |
| `routes/slots/weather/` | a parallel segment with its own loader and its own `loading.tsx` |
| `routes/slots/gates/` | the same, behind a service that takes twice as long |
| `routes/layout.tsx` | places `children`, `weather` and `gates`, and the island that follows the field |
| `src/ui/Live.tsx` | one `live(["board"])` in an effect |
| `src/backend.rs` | the field's three systems, in process, answering from the clock |

## The two mechanisms

**Streaming** is the first paint. The board answers at once and the two panels do not, so the document goes out with the board rendered and a skeleton where each panel will be, and each panel fills as its service answers. Measured at a 900ms pause: the board and both skeletons at 0.02s, the weather at 0.92s, the gates at 1.82s.

**Push** is everything after. A tick publishes `board` on the server, every open `/_sf/live?topics=board` stream hears it, and `live()` in the browser calls `refresh()`, which re-runs the loaders and patches the page in place. No polling, no reload, and no data on the stream: the topic says only that something changed, and the loaders answer what. Watch it directly:

```sh
curl -sN "http://127.0.0.1:8120/_sf/live?topics=board"
```

The browser reconnects the stream itself, so restarting the server picks back up without touching the page.

## What it is worth reading for

A row's CSS class is `status-${flight.code}`, not `status-${flight.status.replace(" ", "-")}`. The first version cost the whole page its server rendering, and the build said so: `routes/page.tsx:17: .replace(), which is not a builtin`, with the component listed as `client` rather than `lowered`. A string method outside the lowered subset is not an error, it is a component that quietly moves to the browser, and the build report is where you find out.

The field's temperature is a `double` in the contract, not an `int64`. An `int64` generates a `bigint`, React refuses to render one, and `tsc` caught it at build. A reading is not an identifier.

## Tests

`cargo test -p arrivals_react_ts`, with the clock frozen at 08:25, since a board that moves is a board no assertion can hold: that the first chunk carries both boards rendered and a skeleton for each panel and neither answer, that each panel arrives in its own chunk in the order the services answer, that the clock decides what every flight says, that a gate change reaches both the flight's row and the panel, and that every component but the live island renders on the server.

`fsr test app`: both tables with a row per flight, each panel filled from its own service, and the live island in the layout.

Checked in a browser: the clock advances and flights change status with `performance.getEntriesByType("navigation").length` still 1, and the console empty.
