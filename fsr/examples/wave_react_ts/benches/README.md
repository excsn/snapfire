# Typing on one wave

**Bench:** `benches/ops.rs`, run from `fsr/examples/wave_react_ts` with `cargo bench --bench ops`. `OPS_SECONDS` sets each row's length (default 5) and `OPS_RATE` each window's keystrokes a second in the paced sweep (default 10).

**Question:** every keystroke in a wave goes over the host's socket to the one controller that owns the wave and comes back to every window as a view. How many keystrokes a second does that path carry, how long does one take to reach the other windows and what happens past the point where it cannot keep up?

## What is measured

The wave's host as `main` builds it, serving on a port of its own in the bench's process. N windows open real WebSockets on one wave with one session cookie, name themselves and type. A keystroke is one `typing` row: the wire turns it into an `Op::Typing`, the rules apply it and the controller builds one view for every window on the wave, each carrying every other window's draft. One keystroke therefore costs N views of up to N - 1 drafts each.

Each body carries its window, a sequence number and when it was sent. A receiving window measures keystroke to screen from that and counts the sequence numbers it never saw.

| Sweep | One row is |
| --- | --- |
| paced | every window types `OPS_RATE` keystrokes a second for `OPS_SECONDS`, with 2, 4, 8, 16, 32 and 64 windows |
| flat out | every window sends as fast as its socket takes frames for `OPS_SECONDS`, with 2, 4, 8 and 16 windows |

## What is not measured

The clients run in the same process and on the same tokio runtime as the host. They parse every view they receive as JSON, so the host competes with them for the machine and the numbers understate what the host alone carries. Nothing here touches the loaders, the actions or a kept blip.

## How to read it

| Column | Is |
| --- | --- |
| sent/s | keystrokes sent by every window together, over the row's length |
| applied/s | views received divided by the number of windows, over the time until the last frame arrived. Each applied keystroke sends one view to every window, so this is keystrokes applied a second |
| views/s | views received by every window together |
| seen | the share of other windows' keystrokes each window received |
| gaps | sequence numbers a window skipped |
| p50, p99, max | keystroke to screen, over every keystroke a window saw |

A gap is a keystroke the wire dropped: `Wire::submit` hands the op to the controller with `try_send` into a queue 256 deep and ignores a full one. The way back to each socket is unbounded, so nothing is dropped there.

In the flat out rows applied/s is taken until the last frame, well after sending stopped, so it is lower than the rate the host applied at while the row ran. Latency falls at 8 and 16 windows flat out because almost every keystroke is dropped and the few that get through are recent.

The bench prints its own table rather than criterion's three numbers, since a room of sockets typing for a fixed window is not a timed batch.

## Results

### 2026-09-13, `7f8309f`, MacBook M4 Pro

One run, powermode 2, load 3.82 at the start and 6.74 at the end, 14 cores.

**Paced, 10 keystrokes a second per window, 5 s a row**

| Windows | sent/s | applied/s | views/s | seen | gaps | p50 ms | p99 ms | max ms |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 2 | 20 | 20 | 41 | 100.0% | 0 | 0.50 | 1.06 | 1.26 |
| 4 | 41 | 41 | 163 | 100.0% | 0 | 0.79 | 2.17 | 2.74 |
| 8 | 82 | 82 | 652 | 100.0% | 0 | 2.15 | 4.63 | 5.46 |
| 16 | 163 | 163 | 2606 | 100.0% | 0 | 6.68 | 11.00 | 14.26 |
| 32 | 326 | 324 | 10367 | 100.0% | 0 | 16.39 | 32.80 | 35.46 |
| 64 | 653 | 337 | 21596 | 59.6% | 79443 | 702.39 | 775.99 | 787.88 |

**Flat out, 5 s a row**

| Windows | sent/s | applied/s | views/s | seen | gaps | p50 ms | p99 ms | max ms |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 2 | 134294 | 69312 | 138624 | 97.2% | 18774 | 2684.97 | 3967.49 | 3986.11 |
| 4 | 203812 | 38589 | 154358 | 47.6% | 1602735 | 1926.11 | 7392.49 | 7493.19 |
| 8 | 455676 | 19955 | 159637 | 4.4% | 15240652 | 27.87 | 301.37 | 388.24 |
| 16 | 539915 | 5675 | 90797 | 1.1% | 40025100 | 53.53 | 1872.21 | 2044.26 |

Up to 32 windows each typing 10 keystrokes a second, nothing is dropped and the p99 is 33 ms. 64 windows offer 653 keystrokes a second and the path applies about 337 of them: 40% are dropped and the p50 is 702 ms. Flat out, the path delivers 139k to 160k views a second at 2 to 8 windows and 91k at 16, where every view carries 15 drafts.
