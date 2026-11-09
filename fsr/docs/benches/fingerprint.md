# Fingerprint: hashing a rendered page and a value

**Bench:** `fsr/core/benches/fingerprint.rs`, run from `fsr` with `cargo bench -p snapfire_fsr_core --bench fingerprint`.

**Question:** what `fingerprint()` costs on a rendered page tree and on a nested value.

## What is measured

| Benchmark | One iteration is |
| --- | --- |
| `fingerprint/page_1_section_100_points` | `Node::fingerprint` over a page of one section holding one client island with a 100-point `f64` series |
| `fingerprint/page_20_sections_10k_points` | the same over 20 sections, each island carrying 10,000 points |
| `fingerprint/nested_map_depth5_width8` | `Value::fingerprint` over maps nested 5 deep with 8 entries at each level |

## Results

### 2026-11-09, `e2e52913`, MacBook M4 Pro

Criterion, release profile, one run.

| Benchmark | Lower | Estimate | Upper |
| --- | --- | --- | --- |
| `fingerprint/page_1_section_100_points` | 462.03 ns | 463.21 ns | 464.42 ns |
| `fingerprint/page_20_sections_10k_points` | 631.80 µs | 632.63 µs | 633.47 µs |
| `fingerprint/nested_map_depth5_width8` | 1.3945 ms | 1.4057 ms | 1.4184 ms |
