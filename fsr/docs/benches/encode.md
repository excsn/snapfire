# Encode: props to JSON, a page to the wire format and to HTML

**Bench:** `fsr/payload/benches/encode.rs`, run from `fsr` with `cargo bench -p snapfire_fsr_payload --bench encode`.

**Question:** what the payload crate's three encoders cost on the same pages and props.

## What is measured

| Benchmark | One iteration is |
| --- | --- |
| `json/value_typical_props` | `value_to_json` over props holding 50 server rows of four fields, then `to_string` |
| `wire/page_1_section_100_points` | `serialize_page` over a page of one section holding one client island with a 100-point `f64` series |
| `wire/page_20_sections_10k_points` | the same over 20 sections, each island carrying 10,000 points |
| `html/page_1_section_100_points` | `html_serialize` over the one-section page |
| `html/page_20_sections_10k_points` | `html_serialize` over the 20-section page |

## Results

### 2026-11-09, `e2e52913`, MacBook M4 Pro

Criterion, release profile, one run.

| Benchmark | Lower | Estimate | Upper |
| --- | --- | --- | --- |
| `json/value_typical_props` | 31.823 µs | 31.888 µs | 31.955 µs |
| `wire/page_1_section_100_points` | 3.6557 µs | 3.6622 µs | 3.6689 µs |
| `wire/page_20_sections_10k_points` | 1.3259 ms | 1.3378 ms | 1.3491 ms |
| `html/page_1_section_100_points` | 1.6905 µs | 1.6960 µs | 1.7016 µs |
| `html/page_20_sections_10k_points` | 1.3075 ms | 1.3094 ms | 1.3112 ms |
