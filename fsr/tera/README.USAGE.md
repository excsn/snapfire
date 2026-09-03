# Usage Guide: snapfire_fsr_tera

How to register the marker functions on a Tera instance, wire the evaluator into a runtime and write templates that emit islands, slots and the document head.

## Table of Contents

* [Core Concepts](#core-concepts)
* [Quick Start](#quick-start)
  * [The Template](#the-template)
  * [The Wiring](#the-wiring)
* [Registering the Marker Functions](#registering-the-marker-functions)
* [Building and Registering the Evaluator](#building-and-registering-the-evaluator)
* [Placing an Island](#placing-an-island)
* [Placing a Slot](#placing-a-slot)
* [Placing the Document Head](#placing-the-document-head)
* [Reading Props in a Template](#reading-props-in-a-template)
  * [Props the Assembler Injects](#props-the-assembler-injects)
  * [Props from the Data Source](#props-from-the-data-source)
* [Passing Structured Values to an Island](#passing-structured-values-to-an-island)
* [Writing Fallback and Error Templates](#writing-fallback-and-error-templates)
* [Calling the Evaluator Directly](#calling-the-evaluator-directly)
* [Why Marker and Split](#why-marker-and-split)
* [Error Handling](#error-handling)

## Core Concepts

* **Evaluator** is the runtime seam this crate fills: given a module id and its props, produce a stream of payload chunks.
* **Module id** is `path#export`. This evaluator looks the template up by `path` alone.
* **Chunk** is one item of that stream, either `Chunk::Node` (finished payload) or `Chunk::Slot` (a hole the assembler fills).
* **Marker** is the private-use codepoint `U+F8FF` that delimits a token inside rendered output. `MARKER` is public so a caller can recognise one.
* **Marker token** is the text between two markers: `island:<base64>` or `slot:<name>`.
* **Marker-and-split** is the whole strategy: template functions emit tokens into an ordinary string, the evaluator splits the finished string on the marker and turns each piece into a chunk.
* **Island** is a client component placed inside server-rendered markup. `island()` in a template becomes a client node in the payload.
* **Slot** is a named hole. `slot(name="content")` becomes `Chunk::Slot`, which the assembler replaces with the plan child registered under that name.
* **Head** is the reserved slot `head`. `head()` emits it; the assembler substitutes the head node it computed before rendering started.
* **Props** are the module's `Data` (an ordered string-to-`Value` map), inserted into the Tera context one key at a time.
* **Data source** is the loader whose output becomes the bulk of a node's props, resolved before evaluation begins.
* **Plan node** names the module, the data source, the children per slot name and the optional fallback and error modules.
* **Fallback module** renders while a deferred child is still resolving; **error module** renders when a loader failed. Neither may contain slots.
* **Assembler** drives all of this: it injects context props, calls the evaluator, stitches children into slots and caches what it may.

## Quick Start

A complete template plus the registration that makes it renderable.

### The Template

`templates/layout.tera`:

```jinja
<!doctype html>
<html>
  <head>{{ head() }}</head>
  <body>
    <nav>
      {{ nav_label }}
      {% if identity is defined %}<span class="who">signed in as {{ identity.subject }}</span>{% endif %}
    </nav>
    <main>{{ slot(name="content") }}</main>
  </body>
</html>
```

`templates/page.tera`:

```jinja
<section>
  <h1>{{ params.section }}</h1>
  <table>{% for s in servers %}<tr><td>{{ s.name }}</td><td>{{ s.load }}</td></tr>{% endfor %}</table>
  <form method="post" action="/_sf/action/add_server">
    <input name="name" placeholder="name">
    <input type="hidden" name="_csrf" value="{{ csrf_token | default(value="") }}">
    <button>add server</button>
  </form>
  {{ island(module="components/ServerChart.tsx#default", props=chart) }}
</section>
```

### The Wiring

```rust
use std::sync::Arc;

use snapfire_fsr_core::ModuleId;
use snapfire_fsr_runtime::{DataSources, Evaluators, Runtime};
use snapfire_fsr_tera::TeraEvaluator;

fn templates() -> tera::Tera {
  let mut tera = tera::Tera::new();
  snapfire_fsr_tera::register_markers(&mut tera);
  tera
    .add_raw_templates([
      ("layout.tera", include_str!("../templates/layout.tera")),
      ("page.tera", include_str!("../templates/page.tera")),
    ])
    .expect("templates parse");
  tera
}

pub fn runtime(sources: DataSources) -> Arc<Runtime> {
  let mut evaluators = Evaluators::new();
  evaluators.register(
    |m: &ModuleId| m.path.ends_with(".tera"),
    Arc::new(TeraEvaluator::new(templates())),
  );

  Runtime::builder().sources(sources).evaluators(evaluators).build()
}
```

A plan node reaches these templates by module id, `ModuleId::new("page.tera", "default")`, with `SlotName("content")` naming the child that fills the layout's slot.

## Registering the Marker Functions

Tera 2 validates function names when a template is added, so registration comes first:

```rust
let mut tera = tera::Tera::new();
snapfire_fsr_tera::register_markers(&mut tera);
tera.add_raw_templates([("page.tera", include_str!("../templates/page.tera"))]).expect("templates parse");
```

The reverse order fails the add, because `island`, `slot` and `head` are unknown at parse time:

```rust
let mut tera = tera::Tera::new();
tera.add_raw_templates([("page.tera", "{{ slot(name=\"content\") }}")]).unwrap(); // fails here
snapfire_fsr_tera::register_markers(&mut tera);
```

`TeraEvaluator::new` calls `register_markers` for you, which covers templates added to the instance afterwards but not the ones already loaded. Registering explicitly before the first `add_raw_templates` is the order that always works.

## Building and Registering the Evaluator

`TeraEvaluator::new` takes ownership of a configured instance, so an application's own filters, functions and tests are registered before it is handed over:

```rust
let mut tera = tera::Tera::new();
snapfire_fsr_tera::register_markers(&mut tera);
tera.register_function("asset_url", |kwargs: tera::Kwargs, _: &tera::State| -> tera::TeraResult<String> {
  let name = kwargs.must_get::<String>("name")?;
  Ok(format!("/static/{name}"))
});
tera.add_raw_templates([("page.tera", include_str!("../templates/page.tera"))]).expect("templates parse");

let evaluator = TeraEvaluator::new(tera);
```

Dispatch is by predicate on the module id, so one runtime can mix this evaluator with others:

```rust
let mut evaluators = Evaluators::new();
evaluators.register(|m: &ModuleId| m.path.ends_with(".tera"), Arc::new(evaluator));
```

Any module the predicates decline goes to the runtime's null evaluator, which emits a client node instead.

## Placing an Island

`island` takes `module` (required) and `props` (optional):

```jinja
{{ island(module="components/ServerChart.tsx#default", props=chart) }}
```

`module` is a full module id, `path#export`. Omitting `props` gives the island an empty prop map:

```jinja
{{ island(module="components/Clock.tsx#default") }}
```

Any expression the template can evaluate works as `props`, including a loop variable, so one template can emit an island per row:

```jinja
{% for s in servers %}{{ island(module="components/Row.tsx#default", props=s) }}{% endfor %}
```

Each call becomes one client node in the payload, in the position it occupied in the rendered string. Islands emitted this way have no children and no server-rendered subtree.

## Placing a Slot

A slot is a named hole, filled by the plan child registered under the same name:

```jinja
<main>{{ slot(name="content") }}</main>
```

The plan side names the same slot:

```rust
let mut layout = PlanNode::new(NodeId(0), ModuleId::new("layout.tera", "default"));
layout.children.push((SlotName("content".into()), content));
```

Slot names may hold ASCII letters, digits, `_` and `-`; an empty name or any other character fails the render. A slot with no matching plan child fails assembly rather than rendering empty, so the two sides stay in step.

A template can hold several slots; a deferred child's slot is the one that streams in later:

```jinja
{{ island(module="components/ServerChart.tsx#default", props=chart) }}{{ slot(name="chart") }}
```

## Placing the Document Head

`head()` takes no arguments and emits the reserved slot `head`, which the assembler replaces with the head node it computed before rendering started:

```jinja
<html><head>{{ head() }}</head><body>...</body></html>
```

Use it in exactly one template per response, normally the outermost layout. A segment whose subtree called `head()` is never written to the node cache, so keep the call in the layout rather than in a cacheable page template.

## Reading Props in a Template

Every key of the module's props becomes a top-level Tera variable, so a template reads them by name.

### Props the Assembler Injects

Three arrive without any loader:

| Variable | Shape | Present when |
| --- | --- | --- |
| `params` | map of route parameter name to string | always |
| `identity` | map with `subject` and `claims` | the session carries an identity |
| `csrf_token` | string | the request context has a CSRF token |

Both optional ones need guarding:

```jinja
{% if identity is defined %}<span class="who">signed in as {{ identity.subject }}</span>{% endif %}
<input type="hidden" name="_csrf" value="{{ csrf_token | default(value="") }}">
```

### Props from the Data Source

Whatever the plan node's loader returned sits alongside them under its own keys:

```rust
sources.insert_fn("layout_loader", |ctx| async move {
  let mut data = ValueMap::new();
  data.insert("nav_label".to_owned(), Value::Str("SnapFire FSR".to_owned()));
  data.insert("visits".to_owned(), Value::Int(1));
  Ok(data)
});
```

```jinja
<nav>{{ nav_label }} <span class="visits">visits {{ visits }}</span></nav>
```

## Passing Structured Values to an Island

Props reach the template as JSON, in the payload crate's lossless encoding. Booleans, strings, ordinary integers, sequences and plain maps look the way a template expects. Anything whose JSON form would be ambiguous (typed arrays, byte strings, integers past 2^53 - 1, floats with no fractional part) arrives as a tagged object carrying a `$` key:

```rust
let mut map = ValueMap::new();
map.insert("series".to_owned(), Value::TypedArray(TypedArray::F64(vec![12.0, 15.5, 9.25])));
data.insert("chart".to_owned(), Value::Map(map));
```

A template cannot iterate that, but it can hand it straight to an island, where the tag decodes back to the original value:

```jinja
{{ island(module="components/ServerChart.tsx#default", props=chart) }}
```

So the rule is: shape data for the template when the template reads it; leave it in its native `Value` form when it is only passing through to a client component.

## Writing Fallback and Error Templates

A deferred child's fallback renders immediately, before its loader has finished:

```jinja
<div class="skl">loading latency</div>
```

An error module renders in place of a segment whose loader failed. It receives the failure message as `error`:

```jinja
<section class="error"><h2>Backend unavailable</h2><p>{{ error }}</p></section>
```

Both are wired on the plan node:

```rust
let mut chart = PlanNode::new(NodeId(2), ModuleId::new("chart_section.tera", "default"));
chart.deferred = true;
chart.fallback = Some(ModuleId::new("chart_loading.tera", "default"));

let mut page = PlanNode::new(NodeId(1), ModuleId::new("page.tera", "default"));
page.error = Some(ModuleId::new("error_section.tera", "default"));
```

Neither may call `slot()` or `head()`: there is no plan child to stitch into a fallback, so a slot there fails assembly. Islands are fine in both.

## Calling the Evaluator Directly

The evaluator is usable without the assembler, which is the quickest way to see what a template produces:

```rust
use futures_util::TryStreamExt;
use snapfire_fsr_core::{ModuleId, Value, ValueMap};
use snapfire_fsr_runtime::{Chunk, EvalError, Evaluator};
use snapfire_fsr_tera::TeraEvaluator;

async fn chunks_of(evaluator: &TeraEvaluator) -> Result<Vec<Chunk>, EvalError> {
  let module = ModuleId::new("layout.tera", "default");
  let mut props = ValueMap::new();
  props.insert("nav_label".to_owned(), Value::Str("SnapFire FSR".to_owned()));
  evaluator.evaluate(&module, &props).try_collect().await
}
```

The stream is chunking of complete output, not incremental rendering: the template renders in full before the first chunk is yielded. Nothing here ever produces a pending node; deferral is the assembler's business.

## Why Marker and Split

The seam an evaluator has to satisfy is one method returning a stream of chunks. Nothing in it assumes a component tree, a hydration boundary, a virtual DOM or a JavaScript runtime. This crate is the demonstration: Tera renders a string, three functions leave tokens in it, one split turns the string into chunks. That is the entire adapter.

The token design follows from wanting the split to be unambiguous:

* The delimiter is `U+F8FF`, a private-use codepoint, so nothing a template legitimately outputs collides with it.
* An island's payload is base64 of a small JSON object, so no template syntax, markup or delimiter can appear inside a token.
* Splitting on a single character makes the marker positions the chunk boundaries, so a token's position in the output is the node's position in the payload.

The cost is that an unbalanced marker count is a render-time failure rather than something the type system rules out, which is why the evaluator checks parity before interpreting anything.

## Error Handling

`EvalError` is a struct rather than an enum: a `module` string and a `message`, displayed as `evaluate {module}: {message}`. There are no variants to match on, so callers log it or propagate it.

```rust
match evaluator.evaluate(&module, &props).try_collect::<Vec<Chunk>>().await {
  Ok(chunks) => chunks,
  Err(EvalError { module, message }) => {
    tracing::error!(module = %module, error = %message, "template evaluation failed");
    return Err(other_error);
  }
}
```

Under the assembler it arrives wrapped:

```rust
match assemble(&runtime, &plan, &ctx, &head).await {
  Err(AssembleError::Eval(e)) => tracing::error!(error = %e, "evaluation"),
  Err(AssembleError::MissingSlot { node, slot }) => tracing::error!(node, %slot, "plan has no child for this slot"),
  Err(AssembleError::SlotInFallback(module)) => tracing::error!(%module, "fallback template used a slot"),
  Err(other) => tracing::error!(error = %other, "assembly"),
  Ok(assembly) => { /* ... */ }
}
```

Failures split by when they happen. Tera raises the first group while rendering. The evaluator forwards the message: an unknown template name, a missing `module` argument on `island`, a slot name that is not `[A-Za-z0-9_-]+`, props that will not serialise. The evaluator raises the second group while splitting. Every message there means a malformed token: unbalanced delimiters, invalid base64, invalid JSON, a missing or unparseable island module id, island props that are not a map, an unrecognised token prefix. The [API reference](API_REFERENCE.md) lists the exact messages.
