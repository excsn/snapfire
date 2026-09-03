# snapfire_fsr_plan

MPL-2.0. Pre-release, version 0.1.0, not published to crates.io.

The plan file for SnapFire FSR: `generated/plan.json`, the artifact `fsr build` writes and a host reads at boot. It carries the routes as trees of nodes, a row per data source and per action saying who answers it, lowered or Rust, with the lowered body inline, and a row per component the build lowered to a render tree. The format is its own rather than serde over the runtime's types, so a field can be added to the file without the vocabulary crate gaining a dependency. `Manifest` is the file in memory and `Node` its serialized tree; a `Manifest` converts to the runtime's `PlanNode`s with `routes` and lists what a host has to bind with `sources` and `modules`. Task-by-task instructions are in [README.USAGE.md](README.USAGE.md); the surface is in [API_REFERENCE.md](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_fsr_plan = { path = "../plan" }
```

No features. The crate depends on `snapfire_fsr_core` for the vocabulary types, `snapfire_fsr_ir` for the lowered bodies and `serde_json` with `preserve_order`, so a file reads back in the order it was written.

## What to reach for

| What you are doing | What to reach for |
| --- | --- |
| Read a plan file | `Manifest::from_json` |
| Write one | `Manifest::to_json` |
| Build one from routes | `Manifest::new`, `with_sources`, `with_actions`, `with_components` |
| Turn a runtime tree into a file row | `Node::from_plan` |
| Get the runtime's trees back | `Manifest::routes` |
| Find out what a host must bind | `Manifest::sources`, `Manifest::modules`, `Manifest::action_ids` |
| Pick out the rows that carry a body | `Manifest::lowered_sources`, `Manifest::lowered_actions` |
| Say who answers a row | `RowOwner`, `SourceEntry::lowered`, `SourceEntry::rust`, `ActionEntry::lowered`, `ActionEntry::rust` |
| Name the type an action's input is checked against | `ActionEntry::with_input` |

## Status

Pre-release and unpublished. Format 2 is current and a format 1 file, with bare action ids and no `sources` table, still reads. `fsr build` writes the file and `snapfire_fsr` reads it through `App::from_manifest`; `shopping_react_ts` is built and served that way. The crate's 12 tests cover the round trip, absent fields staying absent, the source and module lists, the version check, the module id and duplicate refusals, a hand-written file, a format 1 file and lowered rows with and without a body.
