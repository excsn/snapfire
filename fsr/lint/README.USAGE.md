# Usage Guide: snapfire_fsr_lint

How FSR's rules run, what each finds and the levels FSR sets.

## Table of Contents

* [Core Concepts](#core-concepts)
* [Quick Start](#quick-start)
* [Finding Browser Globals Where the Server Runs](#finding-browser-globals-where-the-server-runs)
* [Finding File Reads in Render](#finding-file-reads-in-render)
* [Finding Two Sources With One Output](#finding-two-sources-with-one-output)
* [Finding Imports of Undeclared Packages](#finding-imports-of-undeclared-packages)
* [The Base Levels](#the-base-levels)

## Core Concepts

* **Check** is `snapfire_lint`'s trait for a rule from outside it; each rule here is one.
* **Base** is the configuration FSR puts under a project's own `fsr.lint.toml`, as a framework's shared ESLint config does.
* **Render** is a component's own body and its module's top level. It takes in the callbacks `useMemo`, `useState` and `useReducer` run while the component renders. A handler or an effect is not render.
* **Server module** is a file only the server runs, by FSR's names: `*.loader.*`, `actions.*`, `route.*` and `middleware.*`.
* **Vendored only as a dependency** is a package in the vendor tree because another package imports it, which `fsr add` records in `.fsr-vendor.json` as `dependency_of`. In an application vendored by xwpm it is an import map entry no module in `xwpm.wmf`'s `[modules]` covers.

## Quick Start

```rust
use std::path::Path;

use snapfire_lint::{Config, Linter};

fn main() -> Result<(), Box<dyn std::error::Error>> {
  let config = Config::discover(Path::new("."))?.over(snapfire_fsr_lint::base());
  let linter = Linter::new_with(config, snapfire_fsr_lint::checks())?;
  println!("{}", linter.lint_dir(Path::new("app")).render());
  Ok(())
}
```

## Finding Browser Globals Where the Server Runs

`fsr/no-browser-globals-in-render`, `error`, finds `window`, `document`, `localStorage` and the other browser-only globals read in a component's render or at the top of a `.tsx` or `.jsx` file. In a server module it finds them anywhere.

```tsx
export default function Page() {
  const wide = window.innerWidth > 800;
  return <p>{wide ? "wide" : "narrow"}</p>;
}
```

These are left alone: a read inside an effect or a handler, behind `typeof window !== "undefined"`, in a helper the render does not call and in `src/main.ts`, which only the browser runs.

```tsx
export default function Page() {
  useEffect(() => { document.title = "Notes"; }, []);
  return <button onClick={() => alert("hi")}>hi</button>;
}
```

## Finding File Reads in Render

`fsr/no-file-read-in-render`, `error`, finds a call to `file` from `@snapfire/fsr/host` while a component renders. A component renders again on every change, a terminal app many times a second, so the read repeats each time.

```tsx
import { file } from "@snapfire/fsr/host";

export default function Notes() {
  const text = file.read("notes.txt");
  return <p>{text}</p>;
}
```

Read it in the page's loader instead, which runs once per request. A `file` that is not FSR's is not checked.

## Finding Two Sources With One Output

`fsr/output-name-clash`, `error`, finds two sources in one directory that snapfirec writes to the same output name, compared without case: `format.ts` beside `Format.js`; `Card.svelte.ts` beside `Card.svelte.js`. The one that sorts later is reported. A `.vue` or `.svelte` component keeps its own name, so `Pantry.vue` beside `pantry.ts` is no clash; a `.d.ts` file is not compiled alone.

## Finding Imports of Undeclared Packages

`import/no-extraneous-dependencies`, `warn`, finds an import of a package the application has vendored only because another package imports it. It needs to know which packages those are, so it is not among `checks()`; `fsr lint` builds it from the application's vendor manifest.

```rust
use std::collections::BTreeMap;

let vendored_for = BTreeMap::from([("clsx".to_owned(), vec!["svelte".to_owned()])]);
let mut checks = snapfire_fsr_lint::checks();
checks.push(Box::new(snapfire_fsr_lint::ExtraneousDependencies::new(vendored_for)));
```

```ts
import clsx from "clsx";
// warning  import/no-extraneous-dependencies  `clsx` is vendored only as a dependency of `svelte`; declare it to import it
```

`fsr add app clsx@2.1.1` declares it. A subpath such as `clsx/lite` is judged by its package. A type-only import, `import type` or `export type ... from`, is not judged, as ESLint's rule does by default. It keeps ESLint's name, so an `eslint-disable` comment naming it carries over.

## The Base Levels

`base()` sets `react/no-unknown-property` to ignore htmx's attributes, `hx-get`, `hx-swap` and the rest of htmx 2's and htmx 4's, which a JSX element carries when htmx drives it. A project's own file goes over it:

```toml
[rules]
"react/no-unknown-property" = "warn"
```

keeps the ignore list and changes only the level.
