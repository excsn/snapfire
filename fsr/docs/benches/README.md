# FSR benches

One file per bench series. Each file says what the series measures, how to run it and holds the results of every run that was kept.

| Series | Question | File |
| --- | --- | --- |
| Render | How fast does the IR renderer produce a page against React's `renderToString` in QuickJS, plus what a QuickJS context costs to bring up? The measurement behind rendering in Rust rather than in an engine. | [render.md](render.md) |

## Before a run

Start from a quiet machine, in the power mode the series is always taken in. A benchmark generates its own load, so what matters is not beginning on the previous run's exhaust.

## Recording a run

Append to the series' file, newest last:

- the date, the git revision and the machine
- criterion's three numbers per benchmark, lower bound, estimate, upper bound
- anything the run printed that bears on the numbers, such as the render fidelity line in the render series

Treat a surprising swing between runs as a machine-state change until it is proven otherwise.
