# log_terminal_ts

A scrolling `git log` viewer in the terminal, written against Ink's API and run by `fsr dev --terminal` with no Node. A mount effect spawns `git log` with `node:child_process` and appends what it writes, its colours stripped by `strip-ansi`; `useAnimation` turns a spinner until it exits; a clock component ticks on a timer of its own in local time; and the commits sit in a Box that `useBoxMetrics` measures, cut at its border by `overflow="hidden"` and scrolled with `contentOffsetY`.

```sh
cargo install --path fsr/cli --features terminal
cd fsr/examples/log_terminal_ts
fsr dev app --terminal
```

Run it inside a git repository. `j` and `k` or the arrows scroll, `g` and `G` jump to either end and Ctrl-C quits. `fsr/cli/tests/terminal_examples.rs` hands it thirty commits as `git` would write them and checks the frames.
