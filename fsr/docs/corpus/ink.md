# Ink in the terminal

The corpus in `fsr/examples/corpus` runs the 29 examples of Ink (MIT, at `26d2c3f`) through FSR's terminal mode as one application per example and records what each one does in `ink_outcomes.txt`. Each example's `render(…)` names what it draws, which the harness makes the root component; Ink's own source is imported as `ink`. Run it from `fsr/examples`: `cargo test -p corpus --test ink`. The counts here are from `7f54a389` and the commits after it.

| Outcome | Examples | Meaning |
| --- | --- | --- |
| Runs in the terminal | 26 | the example lowers, its first frame draws and it steps on each of its timers and on a key as Ink's does. Each process it spawns is handed output and an exit rather than started |
| Not run by design | 3 | the example is built on an npm package the build does not follow or on `react-router`, whose routes FSR's own routes stand in for |
| Refused | 0 | the example uses something FSR's terminal mode cannot run yet; a terminal has no browser to fall back on |
| Total | 29 | |

The examples that run are `alternate-screen`, `aria`, `border-backgrounds`, `borders`, `box-backgrounds`, `chat`, `concurrent-suspense`, `counter`, `cursor-ime`, `incremental-rendering`, `justify-content`, `render-throttle`, `scroll`, `select-input`, `static`, `subprocess-output`, `suspend-terminal`, `suspense`, `terminal-resize`, `use-animation`, `use-focus`, `use-focus-with-id`, `use-input`, `use-stderr`, `use-stdout` and `use-transition`. A frame that still holds a component the browser would render is refused, so a stateful component an example places counts only when it draws in place.

## Only the first cause is listed

The build stops a component at its first cause, so removing one can uncover the next. `jest` names the `p-queue` package; before `ms` ran natively it named `ms`. Before the clock lowered in terminal mode it named `Date`.

## Not run by design

| Cause | Examples |
| --- | --- |
| An npm package: `@faker-js/faker` and `p-queue` with `delay` | jest, table |
| `react-router` | router |

## Running an Ink application outside the corpus

`tests/ink_app.rs` opens one Ink application of your own the way `fsr dev --terminal` does and prints what it draws or why it is refused. Lay the application out as an app: its source under `app/src` and an `app/routes/page.tsx` that renders its root component with the props its `render(…)` call gave it:

```tsx
// app/routes/page.tsx
import App from "../src/ui.js";

export default function Page() {
  return <App />;
}
```

Then run it from `fsr/examples`, naming the `app/` directory:

```sh
INK_APP=/path/to/app cargo nextest run -p corpus --test ink_app --no-capture
INK_APP=/path/to/app INK_KEYS=hjk INK_COLUMNS=80 INK_ROWS=24 cargo nextest run -p corpus --test ink_app --no-capture
```

It prints the first frame at `INK_COLUMNS` by `INK_ROWS` (100 by 30 when unset), then the frame after each character of `INK_KEYS`; a newline in it is Enter and an escape character is Esc, `INK_KEYS=$'q\e'` in a shell. A refused application fails the test with the first cause, as the corpus records it. With `INK_APP` unset the test says so and passes, so it never changes a normal run.
