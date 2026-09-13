# Typing on one wave

**Bench:** `benches/ops.rs`, run from `fsr/examples/wave_react_ts` with `cargo bench --bench ops`. `OPS_SECONDS` sets each row's length (default 5), `OPS_RATE` each window's keystrokes a second in the paced sweep (default 10) and `OPS_SWEEP` runs `paced`, `flat` or both (default).

**Question:** every keystroke in a wave goes over the host's socket to the one controller that owns the wave and comes back to every window as a view. How many keystrokes a second does that path carry, how long does one take to reach the other windows and what happens past the point where it cannot keep up?

## What is measured

The wave's host as `main` builds it, serving on a port of its own in the bench's process. N windows open real WebSockets on one wave with one session cookie, name themselves and type. A keystroke is one `typing` row: the wire turns it into an `Op::Typing`, the rules apply it and the controller builds one view for every window on the wave, each carrying every other window's draft. One keystroke therefore costs N views of up to N - 1 drafts each.

Each body carries its window, a sequence number and when it was sent. A receiving window measures keystroke to screen from that and counts the sequence numbers it never saw.

| Sweep | One row is |
| --- | --- |
| paced | every window types `OPS_RATE` keystrokes a second for `OPS_SECONDS`, with 2, 4, 8, 16, 32 and 64 windows |
| flat out | every window sends as fast as its socket takes frames for `OPS_SECONDS`, with 2, 4, 8 and 16 windows |

The field's settings, each off by default so the wave runs as it always has:

| Variable | Setting | What changes |
| --- | --- | --- |
| `OPS_TICK_HZ` | `Rules::on_tick` with a `TickDriver` at that rate | a keystroke only updates the draft and the wave's views go out once a tick |
| `OPS_UNIFORM=1` | `Rules::uniform` | one view per change for everyone on the wave, carrying every draft, instead of one per window without the window's own |
| `OPS_DEPTH` | `Wire::with_depth` | how many ops may wait for the controller before a new one is dropped (default 256) |

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

On a tick, a view carries each window's latest draft and the keystrokes between two ticks are never sent, so gaps count superseded drafts rather than drops and seen falls toward 0 flat out. Latency is then the age of the draft a view carries. In the paced rows every window types on the same beat, so a 20 Hz tick finds a new keystroke on every other tick and applied/s reads 10.

The bench prints its own table rather than criterion's three numbers, since a room of sockets typing for a fixed window is not a timed batch.

## Results

### 2026-09-13, `7f8309f`, MacBook M4 Pro

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

### 2026-09-13, `7dc9112`, MacBook M4 Pro

The settings, one run each. The default was taken again in the same stretch as the others.

**Default: a view per window after every keystroke, queue 256**

| Windows | sent/s | applied/s | views/s | seen | gaps | p50 ms | p99 ms | max ms |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 2 | 20 | 20 | 41 | 100.0% | 0 | 0.45 | 1.51 | 1.82 |
| 4 | 41 | 41 | 163 | 100.0% | 0 | 0.72 | 4.34 | 5.06 |
| 8 | 82 | 82 | 652 | 100.0% | 0 | 1.02 | 4.95 | 5.50 |
| 16 | 163 | 163 | 2606 | 100.0% | 0 | 4.16 | 10.38 | 14.04 |
| 32 | 326 | 324 | 10367 | 100.0% | 0 | 17.70 | 34.86 | 38.57 |
| 64 | 653 | 305 | 19550 | 54.7% | 88767 | 777.85 | 863.60 | 872.92 |
| flat 2 | 144426 | 76147 | 152294 | 97.5% | 18063 | 2629.50 | 3733.25 | 3768.73 |
| flat 4 | 202913 | 38052 | 152208 | 44.6% | 1686087 | 1595.65 | 6882.07 | 6923.44 |
| flat 8 | 472191 | 25345 | 202757 | 5.5% | 15613934 | 105.56 | 388.90 | 505.01 |
| flat 16 | 531312 | 6156 | 98493 | 1.2% | 39348885 | 50.25 | 1153.12 | 1594.71 |

**Views on a 20 Hz tick**

| Windows | sent/s | applied/s | views/s | seen | gaps | p50 ms | p99 ms | max ms |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 2 | 20 | 10 | 20 | 100.0% | 0 | 17.52 | 19.39 | 19.64 |
| 4 | 41 | 10 | 40 | 100.0% | 0 | 34.57 | 37.62 | 37.90 |
| 8 | 82 | 10 | 81 | 100.0% | 0 | 36.67 | 39.65 | 39.95 |
| 16 | 163 | 10 | 162 | 100.0% | 0 | 27.03 | 34.77 | 35.58 |
| 32 | 326 | 10 | 324 | 100.0% | 0 | 25.52 | 31.70 | 33.19 |
| 64 | 653 | 10 | 647 | 100.0% | 0 | 35.21 | 41.46 | 43.10 |
| flat 2 | 287262 | 20 | 40 | 0.0% | 1436107 | 1.40 | 3.60 | 44.26 |
| flat 4 | 518119 | 20 | 80 | 0.0% | 7770567 | 1.58 | 4.07 | 36.30 |
| flat 8 | 893978 | 20 | 160 | 0.0% | 31283581 | 2.10 | 5.13 | 39.96 |
| flat 16 | 897254 | 20 | 321 | 0.0% | 67269795 | 3.60 | 19.42 | 33.13 |

**Uniform views**

| Windows | sent/s | applied/s | views/s | seen | gaps | p50 ms | p99 ms | max ms |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 2 | 20 | 20 | 41 | 100.0% | 0 | 0.66 | 1.55 | 1.60 |
| 4 | 41 | 41 | 163 | 100.0% | 0 | 1.26 | 3.89 | 4.28 |
| 8 | 82 | 82 | 652 | 100.0% | 0 | 2.67 | 10.65 | 11.24 |
| 16 | 163 | 163 | 2604 | 100.0% | 0 | 6.62 | 12.33 | 13.54 |
| 32 | 326 | 325 | 10399 | 100.0% | 0 | 9.94 | 20.46 | 107.62 |
| 64 | 653 | 589 | 37664 | 100.0% | 0 | 43.70 | 58.20 | 132.30 |
| flat 2 | 148001 | 76066 | 152132 | 97.2% | 20595 | 2766.99 | 4057.01 | 4111.25 |
| flat 4 | 272105 | 65885 | 263541 | 73.9% | 1063959 | 5252.52 | 9747.50 | 9975.71 |
| flat 8 | 301664 | 32186 | 257487 | 32.6% | 7113624 | 4026.40 | 11146.13 | 11232.62 |
| flat 16 | 242552 | 12987 | 207785 | 14.2% | 14288107 | 4542.27 | 7478.39 | 12259.14 |

**Uniform views on a 20 Hz tick**

| Windows | sent/s | applied/s | views/s | seen | gaps | p50 ms | p99 ms | max ms |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 2 | 20 | 10 | 20 | 100.0% | 0 | 20.59 | 24.11 | 24.22 |
| 4 | 41 | 10 | 40 | 100.0% | 0 | 41.61 | 42.65 | 42.74 |
| 8 | 82 | 10 | 81 | 100.0% | 0 | 35.88 | 38.46 | 39.50 |
| 16 | 163 | 10 | 162 | 100.0% | 0 | 37.08 | 39.35 | 40.39 |
| 32 | 326 | 10 | 326 | 100.0% | 0 | 30.61 | 35.45 | 37.08 |
| 64 | 653 | 10 | 630 | 100.0% | 0 | 46.38 | 60.65 | 62.00 |
| flat 2 | 286172 | 20 | 40 | 0.0% | 1430659 | 1.42 | 2.55 | 41.16 |
| flat 4 | 514043 | 20 | 80 | 0.0% | 7709427 | 1.52 | 3.95 | 44.29 |
| flat 8 | 878446 | 20 | 160 | 0.0% | 30739968 | 2.04 | 6.34 | 43.41 |
| flat 16 | 904589 | 20 | 321 | 0.0% | 67819950 | 3.61 | 22.79 | 35.78 |

**Queue 4096**

| Windows | sent/s | applied/s | views/s | seen | gaps | p50 ms | p99 ms | max ms |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 2 | 20 | 20 | 41 | 100.0% | 0 | 0.87 | 2.04 | 2.52 |
| 4 | 41 | 41 | 163 | 100.0% | 0 | 1.36 | 5.34 | 5.81 |
| 8 | 82 | 82 | 652 | 100.0% | 0 | 2.77 | 7.14 | 8.60 |
| 16 | 163 | 163 | 2605 | 100.0% | 0 | 8.09 | 11.75 | 12.34 |
| 32 | 326 | 324 | 10377 | 100.0% | 0 | 17.54 | 31.37 | 33.97 |
| 64 | 653 | 319 | 20388 | 100.0% | 0 | 2560.06 | 5142.89 | 5244.53 |
| flat 2 | 146124 | 75125 | 150250 | 100.0% | 0 | 2975.03 | 4201.77 | 4236.00 |
| flat 4 | 205385 | 40261 | 161046 | 48.6% | 1584201 | 2189.36 | 7658.19 | 7695.66 |
| flat 8 | 654673 | 14955 | 119644 | 2.4% | 22354633 | 275.96 | 316.69 | 390.77 |
| flat 16 | 614545 | 6147 | 98354 | 1.2% | 45499035 | 615.51 | 983.69 | 1019.15 |

At 64 windows typing 10 keystrokes a second, the default applies 305 a second, sees 54.7% and has a p50 of 778 ms. Uniform views apply 589 with nothing dropped and a p50 of 44 ms. A 20 Hz tick sends each window 10 views a second at every size with a p50 of 25 to 46 ms, so the work no longer depends on how fast anyone types: flat out at 16 windows, 897k keystrokes a second arrive and views stay at 321 a second. With the tick on, uniform views change nothing measurable. A 4096-deep queue drops nothing at 64 windows but its p50 is 2,560 ms, because it holds the backlog without raising the ceiling.

On a tick the queue still drops the newest op when it is full, so under load the other windows can be left showing a draft without its last keystroke.
