# Hashers: which `BuildHasher` suits a `ValueMap`

**Bench:** `fsr/examples/shopping_react_ts/benches/hashers.rs`, run from `fsr/examples/shopping_react_ts` with `cargo bench --features mimalloc --bench hashers`.

**Question:** which hasher a `ValueMap` should use for short string keys in maps of four to fourteen entries, built and read many times per render, when the keys of a request body are attacker-supplied.

## What is measured

Each hasher gets the same three groups: siphash, foldhash fast, foldhash quality, ahash, fxhash, seahash, fnv and xxh3.

| Group | One iteration is |
| --- | --- |
| `hash/insert/<hasher>` | building an `IndexMap` from a product's 14 keys, the payload decoder's shape |
| `hash/lookup/<hasher>` | ten reads from that map, four of them keys it lacks, the renderer's shape |
| `hash/bulk_insert_4096/<hasher>` | inserting 4096 keys of one shape, `k` and eight hex digits, what a request body can carry |

## How to read it

`bulk_insert_4096` is the adversarial group and it decides the choice rather than the speed. The keys reach `snapfire_fsr_payload::json_to_value` from the request, so a hasher whose collisions can be constructed turns one request into quadratic work.

## Results

### 2026-11-09, `e2e52913`, MacBook M4 Pro

Criterion, release profile, `--features mimalloc`, one run.

| Benchmark | Lower | Estimate | Upper |
| --- | --- | --- | --- |
| `hash/insert/siphash` | 541.67 ns | 544.84 ns | 548.93 ns |
| `hash/lookup/siphash` | 77.154 ns | 77.328 ns | 77.515 ns |
| `hash/bulk_insert_4096/siphash` | 151.53 µs | 151.98 µs | 152.47 µs |
| `hash/insert/foldhash_fast` | 472.02 ns | 476.41 ns | 481.78 ns |
| `hash/lookup/foldhash_fast` | 31.862 ns | 31.982 ns | 32.101 ns |
| `hash/bulk_insert_4096/foldhash_fast` | 132.03 µs | 132.41 µs | 132.83 µs |
| `hash/insert/foldhash_quality` | 470.39 ns | 471.78 ns | 473.41 ns |
| `hash/lookup/foldhash_quality` | 32.733 ns | 32.849 ns | 32.969 ns |
| `hash/bulk_insert_4096/foldhash_quality` | 132.70 µs | 133.00 µs | 133.32 µs |
| `hash/insert/ahash` | 479.59 ns | 480.76 ns | 482.10 ns |
| `hash/lookup/ahash` | 39.567 ns | 39.746 ns | 39.931 ns |
| `hash/bulk_insert_4096/ahash` | 131.84 µs | 132.26 µs | 132.75 µs |
| `hash/insert/fxhash` | 452.76 ns | 454.75 ns | 457.19 ns |
| `hash/lookup/fxhash` | 32.104 ns | 32.244 ns | 32.390 ns |
| `hash/bulk_insert_4096/fxhash` | 123.94 µs | 124.28 µs | 124.64 µs |
| `hash/insert/seahash` | 571.60 ns | 573.40 ns | 575.45 ns |
| `hash/lookup/seahash` | 122.35 ns | 122.96 ns | 123.54 ns |
| `hash/bulk_insert_4096/seahash` | 157.52 µs | 157.90 µs | 158.36 µs |
| `hash/insert/fnv` | 505.62 ns | 506.45 ns | 507.32 ns |
| `hash/lookup/fnv` | 50.033 ns | 50.235 ns | 50.455 ns |
| `hash/bulk_insert_4096/fnv` | 142.49 µs | 143.03 µs | 143.62 µs |
| `hash/insert/xxh3` | 819.14 ns | 824.01 ns | 829.32 ns |
| `hash/lookup/xxh3` | 271.69 ns | 272.66 ns | 273.81 ns |
| `hash/bulk_insert_4096/xxh3` | 208.27 µs | 209.70 µs | 211.31 µs |
