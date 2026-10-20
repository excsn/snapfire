# counter_terminal_ts

An Ink component run in the terminal by `fsr dev --terminal`: no Node and no React in the process. The page is written against Ink's own `Box`, `Text`, `Spacer`, `Newline` and `useInput`, imported from `ink`, so the same file runs under Ink. FSR lowers it, draws it with flexbox in the terminal and runs its `useInput` handler on each key, redrawing only the cells that changed.

```sh
cargo install --path fsr/cli --features terminal
cd fsr/examples/counter_terminal_ts
fsr dev app --terminal
```

`+` and `-` or the arrow keys change the count, the right and left arrows change the step, `r` resets and Ctrl-C quits. The uptime beside the step is a `setInterval` a mount effect starts. Editing `app/routes/page.tsx` while it runs redraws it with the count kept.
