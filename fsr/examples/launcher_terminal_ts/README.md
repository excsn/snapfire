# launcher_terminal_ts

A command launcher in the terminal, written against Ink's API and run by `fsr dev --terminal` with no Node. It is the async side of Ink: Enter runs the chosen command from an `async` handler, `await`ing in either branch of an `if` inside a `try`. A command that wants the terminal, the editor `system.env("EDITOR")` names, `less` or a shell, runs inside `useApp().suspendTerminal` with `stdio: "inherit"`; one that does not runs piped and its output shows in a panel. `ms` says how long it took and a failure lands in the `catch`. The system it runs on is read by `uname` behind a `Suspense` boundary, a promise made with `new Promise` and chained with `then`.

```sh
cargo install --path fsr/cli --features terminal
cd fsr/examples/launcher_terminal_ts
fsr dev app --terminal
```

Up and down choose a command, Enter runs it and `q` quits. `fsr/cli/tests/terminal_examples.rs` drives it, handing each command it starts the output and exit it would report.
