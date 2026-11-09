# Scale: the renderer on the axes the storefront pages cannot show

**Bench:** `fsr/examples/shopping_react_ts/benches/scale.rs`, run from `fsr/examples/shopping_react_ts` with `cargo bench --features mimalloc --bench scale`.

**Question:** how the IR renderer's cost grows with list length, component depth, arithmetic in a template and the number of threads rendering at once.

## What is measured

| Group | One iteration is |
| --- | --- |
| `scale/list/<n>` | the storefront's catalogue route rendered with 1, 12, 100 and 1000 products |
| `scale/depth/<n>` | 1, 4, 16 and 64 components, each rendering the next inside one element, the innermost printing a prop |
| `scale/compute/<n>` | one element whose text is 0, 8, 64 and 512 chained additions over a prop |
| `scale/threads/<n>` | the 12-product catalogue rendered on 1, 2, 4 and 8 threads at once |

`depth` and `compute` are synthetic IR rather than app source, so only the named shape changes between rows. The bench writes `props-list-<n>.json` beside the render fixtures so `render.node.mjs` can render the same list lengths with React in V8.

## Results

### 2026-11-09, `e2e52913`, MacBook M4 Pro

Criterion, release profile, `--features mimalloc`, one run.

| Benchmark | Lower | Estimate | Upper |
| --- | --- | --- | --- |
| `scale/list/1` | 17.502 µs | 17.545 µs | 17.587 µs |
| `scale/list/12` | 124.79 µs | 125.07 µs | 125.38 µs |
| `scale/list/100` | 987.77 µs | 990.05 µs | 992.41 µs |
| `scale/list/1000` | 9.8995 ms | 9.9270 ms | 9.9567 ms |
| `scale/depth/1` | 643.93 ns | 645.62 ns | 647.32 ns |
| `scale/depth/4` | 1.4283 µs | 1.4310 µs | 1.4339 µs |
| `scale/depth/16` | 5.2987 µs | 5.3124 µs | 5.3272 µs |
| `scale/depth/64` | 21.619 µs | 21.662 µs | 21.707 µs |
| `scale/compute/0` | 279.02 ns | 280.70 ns | 282.44 ns |
| `scale/compute/8` | 401.66 ns | 402.73 ns | 403.79 ns |
| `scale/compute/64` | 1.3414 µs | 1.3433 µs | 1.3452 µs |
| `scale/compute/512` | 10.763 µs | 10.779 µs | 10.796 µs |
| `scale/threads/1` | 5.6098 ms | 5.6307 ms | 5.6520 ms |
| `scale/threads/2` | 5.7152 ms | 5.7285 ms | 5.7421 ms |
| `scale/threads/4` | 5.9643 ms | 5.9911 ms | 6.0190 ms |
| `scale/threads/8` | 6.8611 ms | 6.8909 ms | 6.9294 ms |
