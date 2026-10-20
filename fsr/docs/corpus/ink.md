# Ink in the terminal

The corpus in `fsr/examples/corpus` runs the 29 examples of Ink (MIT, at `26d2c3f`) through FSR's terminal mode as one application per example and records what each one does in `ink_outcomes.txt`. Each example's `render(…)` names what it draws, which the harness makes the root component; Ink's own source is imported as `ink`. Run it from `fsr/examples`: `cargo test -p corpus --test ink`. The counts here are from `6ea2c687`.

| Outcome | Examples | Meaning |
| --- | --- | --- |
| Runs in the terminal | 12 | the example lowers, its first frame draws and its keys, timers and `exit` run as Ink's do |
| Refused | 17 | the example uses something FSR's terminal mode cannot run yet; a terminal has no browser to fall back on |
| Total | 29 | |

The examples that run are `aria`, `border-backgrounds`, `borders`, `box-backgrounds`, `counter`, `justify-content`, `render-throttle`, `select-input`, `terminal-resize`, `use-input`, `use-stderr` and `use-stdout`.

## Only the first cause is listed

The build stops a component at its first cause, so removing one can uncover the next. `jest` names the `ms` package; before the clock lowered in terminal mode it named `Date`.

## What refuses an example

| Cause | Examples |
| --- | --- |
| A statement a component body cannot hold yet: a loop or a call made for its effect | alternate-screen, use-transition, concurrent-suspense |
| An npm package: `ms`, `@faker-js/faker`, `strip-ansi` and `react-router` | jest, table, subprocess-output, router |
| `useFocus` and `useFocusManager` | use-focus, use-focus-with-id |
| `useCursor`, `useBoxMetrics` and `useApp`'s `suspendTerminal` | cursor-ime, scroll, suspend-terminal |
| `Static` | static |
| A ref written during render | use-animation |
| `new` of the example's own class | chat |
| A promise read with `use` under `Suspense` | suspense |
| `.toLocaleTimeString()`, local time | incremental-rendering |
