# monitor_terminal_ts

A system monitor in the terminal, written against Ink's API and run by `fsr dev --terminal` with no Node. Each tab reads a command, `uptime`, `df -h` or `ps`, which an effect on the chosen tab starts with `node:child_process`, so switching tabs runs the effect again. A `useAnimation` spinner turns while the command runs. The panels are 70% and 30% of the width `useWindowSize` reports and stack under 60 columns. The footer shows the width `useBoxMetrics` measured for the output panel.

```sh
cargo install --path fsr/cli --features terminal
cd fsr/examples/monitor_terminal_ts
fsr dev app --terminal
```

Left and right or `1` to `3` switch tabs and `q` quits. `fsr/cli/tests/terminal_examples.rs` drives it, handing each command it starts the output and exit it would report.
