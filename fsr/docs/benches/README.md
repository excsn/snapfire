# FSR benches

One file per bench series. Each file says what the series measures, how to run it and holds the results of every run that was kept.

| Series | Question | File |
| --- | --- | --- |
| Render | How fast does the IR renderer produce a page against React's `renderToString` in QuickJS, plus what a QuickJS context costs to bring up? The measurement behind rendering in Rust rather than in an engine. | [render.md](render.md) |
| Scale | How does the IR renderer's cost grow with list length, component depth, template arithmetic and threads? | [scale.md](scale.md) |
| Soak | How many renders a second do the IR and React in QuickJS sustain with 1 to 8 requesters? | [soak.md](soak.md) |
| Patterns | For each way JSX goes where a value goes, does the IR match React and what does each render cost? | [patterns.md](patterns.md) |
| Hashers | Which `BuildHasher` suits a `ValueMap`, given request keys an attacker supplies? | [hashers.md](hashers.md) |
| Fingerprint | What does fingerprinting a rendered page or a nested value cost? | [fingerprint.md](fingerprint.md) |
| Encode | What do props to JSON, a page to the wire format and a page to HTML cost? | [encode.md](encode.md) |
| Wave ops | How many keystrokes a second does one wave carry and how long does one take to reach the other windows? | [wave_react_ts/benches/README.md](../../examples/wave_react_ts/benches/README.md) |

## Before a run

Start from a quiet machine, in the power mode the series is always taken in. A benchmark generates its own load, so what matters is not beginning on the previous run's exhaust.

## Recording a run

Append to the series' file, newest last:

- the date, the git revision and the machine
- criterion's three numbers per benchmark, lower bound, estimate, upper bound
- anything the run printed that bears on the numbers, such as the render fidelity line in the render series

Treat a surprising swing between runs as a machine-state change until it is proven otherwise.
