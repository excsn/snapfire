# Soak: sustained renders per second

**Bench:** `fsr/examples/shopping_react_ts/benches/soak.rs`, run from `fsr/examples/shopping_react_ts` with `cargo bench --features mimalloc --bench soak` after `--bench render` has prepared the app. `SOAK_SECONDS` sets each row's window (default 10).

**Question:** how many renders a second the IR and React in QuickJS sustain on the 12-product catalogue, with 1, 2, 4 and 8 requesters rendering flat out.

## What is measured

Each requester is a thread that renders the same page in a loop until the window closes. Threads are built and warmed before the clock starts. The QuickJS rows give each requester its own engine with React loaded. This is a load shape rather than a timed batch, so the bench prints its own table.

| Column | Is |
| --- | --- |
| renders | renders every requester finished in the window |
| renders/sec | renders over the window |
| system/render | the window divided by renders |
| per requester | how long one render took on one requester |

## Results

### 2026-11-09, `e2e52913`, MacBook M4 Pro

Release profile, `--features mimalloc`, 10 s a row.

| Engine | Requesters | Renders | Renders/sec | System/render | Per requester |
| --- | --- | --- | --- | --- | --- |
| fsr | 1 | 90864 | 9078 | 110.16 µs | 110.16 µs |
| fsr | 2 | 177472 | 17728 | 56.41 µs | 112.81 µs |
| fsr | 4 | 338992 | 33860 | 29.53 µs | 118.13 µs |
| fsr | 8 | 574208 | 57395 | 17.42 µs | 139.38 µs |
| quickjs | 1 | 11968 | 1195 | 837.08 µs | 837.08 µs |
| quickjs | 2 | 23472 | 2344 | 426.62 µs | 853.24 µs |
| quickjs | 4 | 43920 | 4385 | 228.07 µs | 912.28 µs |
| quickjs | 8 | 79056 | 7889 | 126.75 µs | 1014.02 µs |
