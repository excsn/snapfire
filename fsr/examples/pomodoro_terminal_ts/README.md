# pomodoro_terminal_ts

A pomodoro timer in the terminal, written against Ink's API and run by `fsr dev --terminal` with no Node. An effect on `running` starts a one-second `setInterval` and returns the `clearInterval` that stops it, so pausing stops the tick and starting again makes a new one. A second effect on the time left writes a line above the timer with `useStdout().write` when it reaches zero, saying how long the round was with `ms`. It then moves between work and a break.

```sh
cargo install --path fsr/cli --features terminal
cd fsr/examples/pomodoro_terminal_ts
fsr dev app --terminal
```

Space starts or pauses, `r` resets the round, `s` skips to its end and `q` quits. `fsr/cli/tests/terminal_examples.rs` drives it, firing the tick itself.
