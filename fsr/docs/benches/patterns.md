# Patterns: each way JSX goes where a value goes, the IR against React

**Bench:** `fsr/examples/patterns/benches/patterns.rs`, run from `fsr/examples/patterns` with `PATTERNS_LABEL=<label> cargo bench --bench patterns`. It needs `snapfirec` on `PATH` or in `SNAPFIREC` and the network once, for `react-dom/server`. The fidelity table goes to `results/<label>.tsv`.

**Question:** for every case under `cases/` that builds, does the IR's markup match React's `renderToString` in QuickJS and what does each render cost.

## What is measured

| Group | One iteration is |
| --- | --- |
| `<label>/ir/<case>` | the interpreter rendering the case's page with the plan's constants |
| `<label>/react/<case>` | `renderToString` of the case's page in a warm QuickJS context, with the hoisted table the IR's render took |

The fidelity column says `same` when the two agree once React's `<!-- -->` between adjacent text, a void element's `/` and attribute order are taken out. It says `equivalent` when they agree once FSR's own markers are taken out of the IR's as well and `differs` otherwise. `fsr/examples/patterns/README.md` lists what is taken out of each.

## Results

### 2026-11-09, `e2e52913`, MacBook M4 Pro

Criterion, release profile, 20 samples of 1 s after 300 ms of warm-up.

| Case | Outcome | Against React | IR bytes | React bytes |
| --- | --- | --- | --- | --- |
| adjacent_text | lowered | same | 37 | 37 |
| and_zero | lowered | same | 8 | 8 |
| array_from_length | lowered | same | 39 | 39 |
| array_local | lowered | same | 29 | 29 |
| array_returned | lowered | same | 35 | 35 |
| array_spread_concat | lowered | same | 47 | 47 |
| as_prop | lowered | same | 37 | 37 |
| children_count | lowered | same | 46 | 46 |
| children_function | lowered | React fails: `routes/page.tsx#Value\|0@c0` is a subtree the server renders, but this root mounted with no hoisted table | - | - |
| children_map | lowered | same | 43 | 43 |
| children_toarray_slice | lowered | same | 27 | 27 |
| clone_element | lowered | same | 57 | 57 |
| columns_render_fns | refused | - | - | - |
| component_as_prop | lowered | same | 28 | 28 |
| component_from_table | lowered | same | 24 | 24 |
| conditional_wrapper | lowered | same | 65 | 65 |
| context_markup | lowered | same | 34 | 34 |
| control_props | lowered | same | 40 | 40 |
| counted_for | lowered | same | 49 | 49 |
| element_prop | lowered | same | 55 | 55 |
| falsy_children | lowered | same | 9 | 9 |
| filter_boolean | lowered | same | 19 | 19 |
| flat | lowered | same | 27 | 27 |
| helper_module_markup | lowered | same | 42 | 42 |
| helper_returns_array | lowered | same | 59 | 59 |
| is_valid_element | lowered | same | 53 | 53 |
| island_markup_prop | lowered | equivalent | 59 | 32 |
| join_with_separator | lowered | same | 63 | 63 |
| keyed_fragment_inline | lowered | same | 27 | 27 |
| let_branches | lowered | same | 41 | 41 |
| local_const | lowered | same | 18 | 18 |
| lookup_object_elements | lowered | same | 36 | 36 |
| map_array | lowered | same | 49 | 49 |
| map_fragment | lowered | same | 49 | 49 |
| markup_in_object_prop | lowered | same | 49 | 49 |
| nested_arrays | lowered | same | 35 | 35 |
| object_entries_map | lowered | same | 84 | 84 |
| object_field_mapped | lowered | same | 123 | 123 |
| optional_map | lowered | same | 53 | 53 |
| push_for_of | lowered | same | 29 | 29 |
| push_if | lowered | same | 57 | 57 |
| recursive_tree | lowered | same | 71 | 71 |
| reduce_markup | lowered | same | 47 | 47 |
| render_prop | lowered | same | 47 | 47 |
| switch_helper | lowered | same | 29 | 29 |
| tag_from_table | lowered | same | 28 | 28 |
| ternary_local | lowered | same | 28 | 28 |
| use_id | lowered | same | 505 | 505 |
| use_memo_markup | lowered | same | 29 | 29 |

| Benchmark | Lower | Estimate | Upper |
| --- | --- | --- | --- |
| `patterns/ir/adjacent_text` | 871.11 ns | 875.53 ns | 880.64 ns |
| `patterns/react/adjacent_text` | 22.327 µs | 22.471 µs | 22.592 µs |
| `patterns/ir/and_zero` | 643.89 ns | 652.61 ns | 660.63 ns |
| `patterns/react/and_zero` | 22.394 µs | 22.464 µs | 22.539 µs |
| `patterns/ir/array_from_length` | 2.4982 µs | 2.5090 µs | 2.5227 µs |
| `patterns/react/array_from_length` | 23.004 µs | 23.103 µs | 23.211 µs |
| `patterns/ir/array_local` | 834.24 ns | 839.64 ns | 844.08 ns |
| `patterns/react/array_local` | 23.964 µs | 24.048 µs | 24.143 µs |
| `patterns/ir/array_returned` | 1.0525 µs | 1.0568 µs | 1.0611 µs |
| `patterns/react/array_returned` | 22.967 µs | 23.125 µs | 23.290 µs |
| `patterns/ir/array_spread_concat` | 997.39 ns | 1.0030 µs | 1.0072 µs |
| `patterns/react/array_spread_concat` | 25.001 µs | 25.142 µs | 25.286 µs |
| `patterns/ir/as_prop` | 878.45 ns | 880.78 ns | 882.94 ns |
| `patterns/react/as_prop` | 22.736 µs | 22.837 µs | 22.959 µs |
| `patterns/ir/children_count` | 1.1172 µs | 1.1246 µs | 1.1319 µs |
| `patterns/react/children_count` | 39.390 µs | 39.589 µs | 39.766 µs |
| `patterns/ir/children_function` | 2.1166 µs | 2.1340 µs | 2.1541 µs |
| `patterns/ir/children_map` | 3.2140 µs | 3.2268 µs | 3.2392 µs |
| `patterns/react/children_map` | 30.736 µs | 30.881 µs | 31.045 µs |
| `patterns/ir/children_toarray_slice` | 2.0211 µs | 2.0308 µs | 2.0406 µs |
| `patterns/react/children_toarray_slice` | 30.597 µs | 30.812 µs | 31.056 µs |
| `patterns/ir/clone_element` | 2.9126 µs | 2.9405 µs | 2.9677 µs |
| `patterns/react/clone_element` | 30.062 µs | 30.212 µs | 30.341 µs |
| `patterns/ir/component_as_prop` | 1.2722 µs | 1.2801 µs | 1.2925 µs |
| `patterns/react/component_as_prop` | 30.266 µs | 30.543 µs | 30.926 µs |
| `patterns/ir/component_from_table` | 1.2278 µs | 1.2322 µs | 1.2368 µs |
| `patterns/react/component_from_table` | 22.601 µs | 22.719 µs | 22.821 µs |
| `patterns/ir/conditional_wrapper` | 1.3352 µs | 1.3418 µs | 1.3505 µs |
| `patterns/react/conditional_wrapper` | 35.145 µs | 35.305 µs | 35.527 µs |
| `patterns/ir/context_markup` | 1.3421 µs | 1.3458 µs | 1.3506 µs |
| `patterns/react/context_markup` | 26.960 µs | 27.106 µs | 27.216 µs |
| `patterns/ir/control_props` | 1.0580 µs | 1.0643 µs | 1.0690 µs |
| `patterns/react/control_props` | 40.090 µs | 40.287 µs | 40.515 µs |
| `patterns/ir/counted_for` | 3.0281 µs | 3.0400 µs | 3.0532 µs |
| `patterns/react/counted_for` | 24.517 µs | 24.831 µs | 25.159 µs |
| `patterns/ir/element_prop` | 1.4270 µs | 1.4331 µs | 1.4408 µs |
| `patterns/react/element_prop` | 30.765 µs | 30.873 µs | 30.952 µs |
| `patterns/ir/falsy_children` | 601.29 ns | 605.10 ns | 608.36 ns |
| `patterns/react/falsy_children` | 22.330 µs | 22.486 µs | 22.648 µs |
| `patterns/ir/filter_boolean` | 768.41 ns | 773.88 ns | 780.14 ns |
| `patterns/react/filter_boolean` | 21.996 µs | 22.098 µs | 22.204 µs |
| `patterns/ir/flat` | 1.8304 µs | 1.8373 µs | 1.8454 µs |
| `patterns/react/flat` | 23.396 µs | 23.631 µs | 23.996 µs |
| `patterns/ir/helper_module_markup` | 829.14 ns | 834.63 ns | 839.31 ns |
| `patterns/react/helper_module_markup` | 22.388 µs | 22.489 µs | 22.596 µs |
| `patterns/ir/helper_returns_array` | 1.0725 µs | 1.0787 µs | 1.0848 µs |
| `patterns/react/helper_returns_array` | 22.950 µs | 23.033 µs | 23.147 µs |
| `patterns/ir/is_valid_element` | 1.9777 µs | 1.9903 µs | 2.0037 µs |
| `patterns/react/is_valid_element` | 22.581 µs | 22.801 µs | 23.009 µs |
| `patterns/ir/island_markup_prop` | 1.8907 µs | 1.9049 µs | 1.9171 µs |
| `patterns/react/island_markup_prop` | 31.906 µs | 32.081 µs | 32.284 µs |
| `patterns/ir/join_with_separator` | 5.6259 µs | 5.6438 µs | 5.6658 µs |
| `patterns/react/join_with_separator` | 25.472 µs | 25.587 µs | 25.726 µs |
| `patterns/ir/keyed_fragment_inline` | 406.60 ns | 408.77 ns | 411.00 ns |
| `patterns/react/keyed_fragment_inline` | 29.984 µs | 30.176 µs | 30.327 µs |
| `patterns/ir/let_branches` | 837.35 ns | 843.21 ns | 850.33 ns |
| `patterns/react/let_branches` | 23.654 µs | 23.779 µs | 23.945 µs |
| `patterns/ir/local_const` | 394.62 ns | 397.94 ns | 402.47 ns |
| `patterns/react/local_const` | 22.760 µs | 22.897 µs | 23.018 µs |
| `patterns/ir/lookup_object_elements` | 1.3844 µs | 1.3914 µs | 1.3971 µs |
| `patterns/react/lookup_object_elements` | 23.579 µs | 23.770 µs | 23.972 µs |
| `patterns/ir/map_array` | 1.7747 µs | 1.7825 µs | 1.7931 µs |
| `patterns/react/map_array` | 23.328 µs | 23.411 µs | 23.493 µs |
| `patterns/ir/map_fragment` | 1.2536 µs | 1.2624 µs | 1.2688 µs |
| `patterns/react/map_fragment` | 23.122 µs | 23.229 µs | 23.348 µs |
| `patterns/ir/markup_in_object_prop` | 1.6309 µs | 1.6401 µs | 1.6477 µs |
| `patterns/react/markup_in_object_prop` | 31.978 µs | 32.061 µs | 32.163 µs |
| `patterns/ir/nested_arrays` | 936.86 ns | 941.06 ns | 944.79 ns |
| `patterns/react/nested_arrays` | 22.004 µs | 22.148 µs | 22.268 µs |
| `patterns/ir/object_entries_map` | 1.6430 µs | 1.6514 µs | 1.6606 µs |
| `patterns/react/object_entries_map` | 23.059 µs | 23.190 µs | 23.328 µs |
| `patterns/ir/object_field_mapped` | 3.1661 µs | 3.1891 µs | 3.2148 µs |
| `patterns/react/object_field_mapped` | 31.625 µs | 32.123 µs | 32.510 µs |
| `patterns/ir/optional_map` | 1.2649 µs | 1.2751 µs | 1.2835 µs |
| `patterns/react/optional_map` | 22.518 µs | 22.634 µs | 22.765 µs |
| `patterns/ir/push_for_of` | 3.0227 µs | 3.0396 µs | 3.0552 µs |
| `patterns/react/push_for_of` | 25.352 µs | 25.478 µs | 25.595 µs |
| `patterns/ir/push_if` | 1.9896 µs | 1.9981 µs | 2.0071 µs |
| `patterns/react/push_if` | 24.897 µs | 25.040 µs | 25.161 µs |
| `patterns/ir/recursive_tree` | 2.6520 µs | 2.6620 µs | 2.6764 µs |
| `patterns/react/recursive_tree` | 85.885 µs | 86.323 µs | 86.802 µs |
| `patterns/ir/reduce_markup` | 1.2894 µs | 1.3010 µs | 1.3092 µs |
| `patterns/react/reduce_markup` | 25.604 µs | 25.736 µs | 25.870 µs |
| `patterns/ir/render_prop` | 3.4055 µs | 3.4299 µs | 3.4482 µs |
| `patterns/react/render_prop` | 30.609 µs | 30.788 µs | 31.022 µs |
| `patterns/ir/switch_helper` | 854.51 ns | 861.24 ns | 868.31 ns |
| `patterns/react/switch_helper` | 22.653 µs | 22.921 µs | 23.185 µs |
| `patterns/ir/tag_from_table` | 798.96 ns | 805.24 ns | 810.15 ns |
| `patterns/react/tag_from_table` | 25.313 µs | 25.498 µs | 25.680 µs |
| `patterns/ir/ternary_local` | 762.46 ns | 764.88 ns | 768.13 ns |
| `patterns/react/ternary_local` | 23.542 µs | 23.866 µs | 24.155 µs |
| `patterns/ir/use_id` | 3.9765 µs | 4.0040 µs | 4.0236 µs |
| `patterns/react/use_id` | 30.938 µs | 31.123 µs | 31.286 µs |
| `patterns/ir/use_memo_markup` | 1.0136 µs | 1.0208 µs | 1.0291 µs |
| `patterns/react/use_memo_markup` | 25.400 µs | 25.576 µs | 25.787 µs |
