# Ink in the terminal

The corpus in `fsr/examples/corpus` runs the 29 examples of Ink (MIT, at `26d2c3f`) through FSR's terminal mode as one application per example and records what each one does in `ink_outcomes.txt`. Each example's `render(…)` names what it draws, which the harness makes the root component; Ink's own source is imported as `ink`. Run it from `fsr/examples`: `cargo test -p corpus --test ink`. The counts here are from `7f54a389` and the commits after it.

| Outcome | Examples | Meaning |
| --- | --- | --- |
| Runs in the terminal | 21 | the example lowers, its first frame draws and it steps on each of its timers and on a key as Ink's does |
| Not run by design | 4 | the example is built on an npm package the build does not follow or on `react-router`, whose routes FSR's own routes stand in for |
| Refused | 4 | the example uses something FSR's terminal mode cannot run yet; a terminal has no browser to fall back on |
| Total | 29 | |

The examples that run are `alternate-screen`, `aria`, `border-backgrounds`, `borders`, `box-backgrounds`, `chat`, `counter`, `incremental-rendering`, `justify-content`, `render-throttle`, `scroll`, `select-input`, `static`, `terminal-resize`, `use-animation`, `use-focus`, `use-focus-with-id`, `use-input`, `use-stderr`, `use-stdout` and `use-transition`. A frame that still holds a component the browser would render is refused, so a stateful component an example places counts only when it draws in place.

## Only the first cause is listed

The build stops a component at its first cause, so removing one can uncover the next. `jest` names the `ms` package; before the clock lowered in terminal mode it named `Date`.

## Not run by design

| Cause | Examples |
| --- | --- |
| An npm package: `ms`, `@faker-js/faker` and `strip-ansi` | jest, table, subprocess-output |
| `react-router` | router |

## What refuses an example

| Cause | Examples |
| --- | --- |
| A call made for its effect in a component body | concurrent-suspense |
| `useCursor` and `useApp`'s `suspendTerminal` | cursor-ime, suspend-terminal |
| A promise read with `use` under `Suspense` | suspense |
