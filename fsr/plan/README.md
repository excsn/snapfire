# snapfire_fsr_plan

[![Crates.io](https://img.shields.io/crates/v/snapfire_fsr_plan.svg)](https://crates.io/crates/snapfire_fsr_plan)
[![Docs.rs](https://docs.rs/snapfire_fsr_plan/badge.svg)](https://docs.rs/snapfire_fsr_plan)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

Reads and writes the plan file of an FSR application.

`fsr build` writes the plan to `generated/plan.sexp` and a host reads it at boot. It holds every route as a tree of components, plus a row for each data source, action, route handler and server-rendered component. A row says who answers it. A translated row carries its body, taken from the application's TypeScript, inside the file. A Rust row names code the host supplies.

Most applications never depend on this crate. `fsr build` and the FSR host use it for you. Use it directly when you write tooling that inspects plans or run a host of your own.

* The task guide is [README.USAGE.md](README.USAGE.md).
* The call surface is [API_REFERENCE.md](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_fsr_plan = "0"
```

## Quick start

```rust
use snapfire_fsr_plan::Manifest;

let text = std::fs::read_to_string("app/generated/plan.sexp")?;
let plan = Manifest::from_text(&text)?;

for (pattern, _tree) in plan.routes()? {
  println!("route {pattern}");
}
println!("data sources to bind: {:?}", plan.sources());
println!("actions to bind: {:?}", plan.action_ids());
```

## What to reach for

| Problem | Piece |
| --- | --- |
| Read a plan file in either form | `Manifest::from_text` |
| Write one | `Manifest::to_sexpr` |
| Read or write the JSON form | `Manifest::from_json`, `Manifest::to_json` |
| Print one row the way the file holds it | `Manifest::view`, `Manifest::row_ids` |
| Build one from routes | `Manifest::new`, `with_sources`, `with_actions`, `with_components` |
| Turn a runtime tree into a file row | `Node::from_plan` |
| Get the runtime's trees back | `Manifest::routes` |
| Find out what a host must bind | `Manifest::sources`, `Manifest::modules`, `Manifest::action_ids` |
| Pick out the rows that carry a body | `Manifest::lowered_sources`, `Manifest::lowered_actions`, `Manifest::lowered_handlers` |
| Say who answers a row | `RowOwner`, `SourceEntry::lowered`, `SourceEntry::rust`, `ActionEntry::lowered`, `ActionEntry::rust` |
| Name the type an action's input is checked against | `ActionEntry::with_input` |
| Mount a plan as one site among several | `Manifest::namespaced` |
