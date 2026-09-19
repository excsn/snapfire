# 206. What a request did

The question this chapter answers: when a page is slow or wrong, how do you find out what actually happened, without wiring a collector first?

**For:** everyone.

## The problem with logs

A log line tells you one thing happened. It does not tell you what else was happening around it, what it cost or what it was waiting for. Ten lines from one request are ten facts you have to reassemble in your head and they interleave with every other request in flight.

What you want is the request as one object: every step it took, nested the way it nested, each with its own cost. That is a trace and `tracing`, the crate the Rust ecosystem instruments with, does not give you one back. Spans go to a subscriber and nothing returns. That is right for logs, whose destination is a file. It is wrong here.

So fsr keeps them. `fibre_tracing` is a layer that collects a request's spans and the host installs it, holds it and serves what it kept.

## Turning it on

Two lines in `main`, before the host is built:

```rust
let logging = Path::new(env!("CARGO_MANIFEST_DIR")).join("fibre_logging.yaml");
let (traces, _logging, why) = snapfire_fsr_host::trace::observe(&logging);

let host = Host::from(env!("CARGO_MANIFEST_DIR"))
  .and_then(|builder| builder.traces(traces).build())?;
```

One call composes two things on one registry: `fibre_logging` taking the events out to its appenders and the collector keeping the spans. They are different jobs and neither replaces the other. Hold `_logging`, because dropping it flushes the appenders.

Every example does exactly this.

## Reading it

Under `fsr dev`, the host answers `GET /__fsr/traces` with the last fifty, newest last. One request to the console's agents page:

```
request 19.92ms ok {"method": "GET", "path": "/agents", "status": "200"}
  source 16.25ms ok {"id": "layout", "memo": "miss", "node": "1"}
  source 16.60ms ok {"id": "agents.layout", "node": "2"}
    call 15.99ms ok {"service": "fleet", "method": "listAlerts", "cache": "none"}
    call 15.54ms ok {"service": "fleet", "method": "listAgents", "cache": "miss"}
  render 1.37ms {"module": "shell#document"}
    render 1.16ms {"module": "routes/layout.tsx#default", "cache": "miss"}
      render 0.82ms {"module": "routes/agents/layout.tsx#default", "cache": "miss"}
        render 0.10ms {"module": "routes/agents/page.tsx#default", "cache": "miss"}
```

The endpoint answers a JSON array. Every span carries its own `depth`, so laying it out as a tree is the reader's job; the shape of the page is in that. Two loaders ran and they ran together rather than one after the other, because both took about sixteen milliseconds inside a request that took twenty. The two service calls sit under the loader that made them, so you know which loader is waiting on which backend. Rendering the whole tree cost one and a half milliseconds against sixteen spent waiting, which tells you where to look and where not to.

Three caches report themselves on three spans. A `source` span carries `memo: hit` or `memo: miss` when that loader is memoizable. A `call` span carries `cache: hit` or `cache: miss` when the method has a cache policy and `cache: none` when it has none, so two identical requests differ in exactly the call the data cache answered. A `render` span carries `cache: hit` or `cache: miss` when the render cache was consulted for that node. Ask for the same page again and the render subtree gets shorter rather than faster: a `cache: hit` high in the tree means the nodes beneath it were never rendered, so they have no spans at all.

None of that is deducible from ten log lines.

## The four spans

The framework opens four and anything you open with `tracing` joins whichever request it is inside.

| Span | One per | Says |
| --- | --- | --- |
| `request` | request, the root | method, path, status and whether it succeeded |
| `source` | plan node with a loader | the `id`, the `node`, whether it failed and `memo` when it is memoizable |
| `call` | service method, whatever the transport | service, method, `cache` when a policy was consulted and the failure kind when it failed |
| `render` | plan node | the `module`, plus `cache` when the render cache was consulted |

A failure names its kind rather than a raw status, because the failure vocabulary is what your code acts on and what a dashboard should group by. Only `request`, `source` and `call` set an outcome. A `render` span has none, so read its `cache` field instead of looking for `ok` on it.

## What it costs when nobody is watching

Nothing worth measuring. With no collector installed a span is a relaxed atomic load and a branch: no allocation, no formatting of fields. So the instrumentation stays in production builds and you decide separately whether anything collects.

The ring holds the last few hundred traces in memory, about a kilobyte each. That is the whole storage story until you want more.

## Getting them out

Nothing leaves the process on its own. A listener is called with each trace as the request finishes:

```rust
traces.on_finish(|trace| exporter.send(trace));
```

That is also the only place tail sampling can happen, keeping the traces that failed or ran long and dropping the rest, because the decision needs the finished trace. Anything sampling when a span opens has not seen it yet. `tracing-opentelemetry` composes here or as a second layer beside the collector if you export everything.

## The lab

Start the ops console and load `/agents`, then fetch `/__fsr/traces` and find that request. Note how long the two loaders took and that they overlap.

Now open the fleet backend and make `listAlerts` sleep for half a second. Load the page again and read the trace: the request grows by roughly half a second, one `source` span grows with it and the `call` span underneath names which method did it. The other loader is unchanged, which is the parallelism showing itself.

Then load the same page twice without changing anything and compare the `render` spans. The second one says `cache: hit` where the first said `cache: miss`; the spans that sat beneath it are gone. The `call` spans say the same thing about the data cache: a method with a policy goes from `cache: miss` to `cache: hit` and its transport is never reached the second time.
