# snapfire_fsr_runtime

[![Crates.io](https://img.shields.io/crates/v/snapfire_fsr_runtime.svg)](https://crates.io/crates/snapfire_fsr_runtime)
[![Docs.rs](https://docs.rs/snapfire_fsr_runtime/badge.svg)](https://docs.rs/snapfire_fsr_runtime)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

Turns a request into a streamed page for SnapFire FSR.

It matches the path to a route, loads the route's data, renders each part of the page and streams the result. The first visit gets HTML. A client-side navigation gets a compact row format the browser applies in place. Slow parts of a page can be deferred: the page streams at once with a fallback and each deferred part follows when its data arrives.

Most applications never depend on this crate. The FSR host builds the runtime from your plan file and runs every request through it. Use it directly when you write your own host, plug in a renderer of your own or test a pipeline without a server.

* The task guide is [README.USAGE.md](README.USAGE.md).
* The call surface is [API_REFERENCE.md](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_fsr_runtime = "0"
snapfire_fsr_core = "0"
```

## Quick start

A shell rendered in Rust around a `.tsx` page. No renderer is registered for the page, so it ships to the browser to mount.

```rust
use std::sync::Arc;

use futures::executor::block_on;
use futures_util::{stream, StreamExt};
use snapfire_fsr_core::{Data, ModuleId, Node, NodeId, PlanNode, SlotName};
use snapfire_fsr_runtime::{
  assemble, html_stream, Chunk, DataSources, Evaluator, Evaluators, NodeChunks, RequestCtx, Runtime,
};

struct Shell;

impl Evaluator for Shell {
  fn evaluate(&self, _module: &ModuleId, _props: &Data) -> NodeChunks {
    Box::pin(stream::iter([
      Ok(Chunk::Node(Node::raw("<main>"))),
      Ok(Chunk::Slot(SlotName("content".into()))),
      Ok(Chunk::Node(Node::raw("</main>"))),
    ]))
  }
}

fn main() {
  let mut plan = PlanNode::new(NodeId(0), ModuleId::new("shell.tera", "default"));
  plan.children.push((SlotName("content".into()), PlanNode::new(NodeId(1), ModuleId::new("components/App.tsx", "default"))));

  let mut evaluators = Evaluators::new();
  evaluators.register(|m: &ModuleId| m.path == "shell.tera", Arc::new(Shell));
  let runtime = Runtime::new(DataSources::new(), evaluators);

  let ctx = RequestCtx::anonymous(Default::default());
  let assembly = block_on(assemble(&runtime, &plan, &ctx, Node::raw(""))).expect("assembly");
  let html: String = block_on(html_stream(assembly).collect::<Vec<_>>()).concat();
  println!("{html}");
}
```

## What it gives you

* **Data before rendering.** Every part of a page that is not deferred loads in parallel before anything renders, so a renderer is a plain function of a module and its props.
* **Deferred parts.** A part marked `deferred` in the plan streams later with its own data, while its fallback shows.
* **A render memo.** A part with a `cache_key` is rendered once per distinct input and served from the memo after that. A bounded, expiring cache is built in.
* **Navigation that keeps state.** Each part of a page has a key. A navigation keeps every part whose key is unchanged, along with the island state inside it.
* **Failure that stays local.** A part whose loader fails renders its error module and the rest of the page renders as usual.
* **One request context.** Params, query, session, identity, locale, CSRF token and the service handle travel together to every loader and action.

## What to reach for

| Problem | Piece |
| :--- | :--- |
| Turn a path into a route entry plus params | `Matcher`, `MatchitMatcher` |
| Turn a route entry into a render plan | `Resolver`, `TableResolver` |
| Load a part's data before anything renders | `DataSources::insert_fn` |
| Turn a module plus props into nodes | `Evaluator`, `Chunk`, `NodeChunks` |
| Ship a module the server does not render | `NullEvaluator` |
| Wire the pipeline together once | `Runtime::builder` |
| Turn a plan plus a request into a payload | `assemble`, `Assembly` |
| Hold a region open and fill it later | `deferred` and `fallback` on the plan node |
| Skip rendering for an unchanged subtree | `cache_key` on the plan node, plus a `NodeCache` |
| Get a bounded, expiring subtree cache | `FibreCache::bounded` |
| Drop everything one plan key produced | `NodeCache::invalidate` |
| Keep DOM and island state across a navigation | `SegmentKeyer`, `DefaultKeyer`, `SegmentInfo` |
| Tell a part that changed from one that only changed key | `SegmentInfo::digest` |
| Seed the browser's store from a part's data | `RuntimeBuilder::store`, `Seeds` |
| Set the document's title and head from data | `RuntimeBuilder::meta`, `Metadata`, `Meta` |
| Carry params, session and CSRF into a loader | `RequestCtx` |
| Read who the request is | `Identity`, `SessionCell::identity` |
| Call the service layer from a loader or action | `ServiceHandle::call` |
| Handle a mutation posted by the browser | `ActionRegistry` |
| Say what kind of failure it was | `FailureKind`, `FailureKind::http_status` |
| Stream the first HTML response | `html_stream`, `FILL_SCRIPT` |
| Stream a client navigation payload | `wire_stream`, `segments_to_json` |
| Read a navigation payload back | `wire::RowReader`, `wire::Row` |

## Tracing

The runtime logs through `tracing` under the targets `fsr::load`, `fsr::cache`, `fsr::stream`, `fsr::action`, `fsr::island` and `fsr::trace`.
