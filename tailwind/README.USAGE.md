# Usage Guide: snapfire_tailwind

How to build Tailwind stylesheets from Rust: compiling a configuration, giving it classes, loading imports, switching themes and drawing classes on card images.

## Table of Contents

* [Core Concepts](#core-concepts)
* [Quick Start](#quick-start)
  * [Building a Stylesheet](#building-a-stylesheet)
  * [Writing What Tailwind's CLI Writes](#writing-what-tailwinds-cli-writes)
* [Compiling a Configuration](#compiling-a-configuration)
  * [Loading Imports](#loading-imports)
  * [Naming the File](#naming-the-file)
  * [Reading What the Stylesheet Uses](#reading-what-the-stylesheet-uses)
* [Giving It Classes](#giving-it-classes)
  * [Building Again With More Classes](#building-again-with-more-classes)
  * [Listing Classes in the Stylesheet](#listing-classes-in-the-stylesheet)
  * [Finding Classes That Are Not Utilities](#finding-classes-that-are-not-utilities)
* [Switching Themes](#switching-themes)
  * [Adding Named Themes](#adding-named-themes)
  * [Tokens Only a Theme Defines](#tokens-only-a-theme-defines)
  * [Taking the Root Theme Back](#taking-the-root-theme-back)
* [Drawing Classes on Cards](#drawing-classes-on-cards)
* [Error Handling](#error-handling)

## Core Concepts

* **Configuration**: the stylesheet you write, with `@import "tailwindcss"`, `@theme`, `@utility`, `@variant`, `@custom-variant` and `@source`, as in Tailwind CSS v4.
* **Candidate**: a class name you hand the compiler, such as `md:bg-sky-500/50`. It becomes a utility or it is invalid.
* **Utility**: the CSS rule a candidate produces.
* **Compiler**: a parsed configuration. It builds as many times as you like and remembers every candidate it has seen.
* **Loader**: the function that resolves `@import`. Without one only Tailwind's own stylesheets resolve.
* **Built-in stylesheets**: `tailwindcss`, `tailwindcss/theme`, `tailwindcss/preflight` and `tailwindcss/utilities`, carried by the crate.
* **Optimize pass**: the two Lightning CSS passes Tailwind runs on its output, with its browser targets.
* **Theme variable**: a custom property from `@theme`. A variable no utility uses is left out of the output.
* **Named theme**: a stylesheet of `@theme` blocks written under its own selector after the root theme, so an attribute switches it.
* **Card rule**: a candidate as plain declarations, with variables and colors resolved, for a renderer that matches no selectors.
* **Feature**: what the configuration uses, such as `@apply` or `@import`, read back as flags.
* **Source**: a `@source` path the configuration names. The crate reports it and never scans it.

## Quick Start

### Building a Stylesheet

```rust
use snapfire_tailwind::{CompileOptions, Compiler};

fn main() -> Result<(), snapfire_tailwind::Error> {
  let css = "@import \"tailwindcss\";\n@theme {\n  --color-brand: #e11d48;\n}";
  let mut compiler = Compiler::new(css, CompileOptions::default())?;
  let out = compiler.build(["flex", "p-4", "bg-brand", "hover:underline"]);
  assert!(out.contains(".bg-brand"));
  println!("{out}");
  Ok(())
}
```

`build` returns Tailwind's own output before any optimize pass: nesting kept, every layer in place, the license banner first.

### Writing What Tailwind's CLI Writes

```rust
use snapfire_tailwind::{optimize, CompileOptions, Compiler, OptimizeOptions};

fn main() -> Result<(), snapfire_tailwind::Error> {
  let mut compiler = Compiler::new("@import \"tailwindcss\";", CompileOptions::default())?;
  let built = compiler.build(["grid", "gap-6", "md:grid-cols-3"]);
  let css = optimize(&built, &OptimizeOptions { minify: true, file: Some("app.css".to_owned()) })?;
  assert!(!css.contains("\n  "));
  Ok(())
}
```

Skip `optimize` when your own build already runs the CSS through Lightning CSS with its own targets.

## Compiling a Configuration

### Loading Imports

A loader receives the id an `@import` names and the directory of the stylesheet naming it. Try Tailwind's built-ins first, then your files:

```rust
use std::path::Path;
use snapfire_tailwind::{tailwind_stylesheet, CompileOptions, Compiler, Error, Loaded};

fn compile(dir: &Path, css: &str) -> Result<Compiler, Error> {
  let load = |id: &str, base: &str| -> Result<Loaded, Error> {
    if let Some(builtin) = tailwind_stylesheet(id, base) {
      return Ok(builtin);
    }
    let file = Path::new(base).join(id);
    let content = std::fs::read_to_string(&file).map_err(|e| Error::Message(format!("{}: {e}", file.display())))?;
    let base = file.parent().unwrap_or(Path::new(".")).to_string_lossy().into_owned();
    Ok(Loaded { path: file.to_string_lossy().into_owned(), base, content })
  };
  let options = CompileOptions { base: dir.to_string_lossy().into_owned(), load_stylesheet: Some(Box::new(load)), ..CompileOptions::default() };
  Compiler::new(css, options)
}
```

A loader handles every import once you give one, which is why it calls `tailwind_stylesheet` itself. An `@import url(...)` is never loaded: it stays in the output for the browser.

### Naming the File

```rust
use snapfire_tailwind::{CompileOptions, Compiler};

let options = CompileOptions { from: Some("styles/app.css".to_owned()), ..CompileOptions::default() };
let err = Compiler::new(".a {\n  color: red;\n", options).err().unwrap();
assert!(err.to_string().starts_with("styles/app.css:"));
```

With `from` set, a syntax error starts with `file:line:column`, the column counted the way a browser counts it.

### Reading What the Stylesheet Uses

```rust
use snapfire_tailwind::{CompileOptions, Compiler, Features};

let compiler = Compiler::new("@import \"tailwindcss\";\n@source \"../src\";", CompileOptions::default()).unwrap();
assert!(compiler.features().has(Features::UTILITIES));
assert_eq!(compiler.sources()[0].pattern, "../src");
```

`sources` lists `@source` paths for a caller that scans files; the crate itself never reads them.

## Giving It Classes

### Building Again With More Classes

```rust
use snapfire_tailwind::{CompileOptions, Compiler};

let mut compiler = Compiler::new("@import \"tailwindcss/utilities\";", CompileOptions::default()).unwrap();
let first = compiler.build(["flex"]);
let second = compiler.build(["grid"]);
assert!(second.contains(".flex") && second.contains(".grid"));
assert!(!first.contains(".grid"));
```

Each build holds every candidate any build was given, so a watcher passes only what it found since the last one.

### Listing Classes in the Stylesheet

A class your code assembles at runtime never reaches the compiler. List its values in the configuration with brace expansion:

```css
@import "tailwindcss";
@source inline("bg-{red,amber,emerald}-{100,700}");
```

### Finding Classes That Are Not Utilities

```rust
use snapfire_tailwind::{CompileOptions, Compiler};

let mut compiler = Compiler::new("@import \"tailwindcss\";", CompileOptions::default()).unwrap();
compiler.build(["flex", "flx", "brand-card"]);
assert_eq!(compiler.invalid_candidates(), ["brand-card", "flx"]);
```

A candidate your own CSS defines, such as `.brand-card`, is invalid to Tailwind too. Check your stylesheet's selectors before calling it a typo.

## Switching Themes

### Adding Named Themes

```rust
use snapfire_tailwind::{CompileOptions, Compiler, NamedTheme};

let ocean = NamedTheme {
  selector: "[data-theme=\"ocean\"]".to_owned(),
  css: "@theme {\n  --color-accent: #0e7490;\n}".to_owned(),
  from: Some("themes/ocean.css".to_owned()),
};
let options = CompileOptions { themes: vec![ocean], ..CompileOptions::default() };
let mut compiler = Compiler::new("@import \"tailwindcss\";\n@theme {\n  --color-accent: #1f6feb;\n}", options).unwrap();
let css = compiler.build(["text-accent"]);
assert!(css.find("[data-theme=\"ocean\"]").unwrap() > css.find("--color-accent: #1f6feb").unwrap());
```

The named theme's rule follows the root theme inside the same layer, so at equal specificity it wins. Every variable it sets is written, used or not, because other stylesheets on the page may read them. A theme that sets `--breakpoint-*` or `--container-*` is a warning: those are read when the stylesheet is built.

### Tokens Only a Theme Defines

```rust
use snapfire_tailwind::{CompileOptions, Compiler, NamedTheme};

let brand = NamedTheme { selector: "[data-theme=\"brand\"]".to_owned(), css: "@theme {\n  --color-brand: #e11d48;\n}".to_owned(), from: None };
let options = CompileOptions { themes: vec![brand], ..CompileOptions::default() };
let mut compiler = Compiler::new("@import \"tailwindcss/utilities\";", options).unwrap();
assert!(compiler.build(["bg-brand"]).contains("background-color: var(--color-brand, #e11d48)"));
```

The utility exists and falls back to the theme's value; the root never takes it.

### Taking the Root Theme Back

```rust
use snapfire_tailwind::{CompileOptions, Compiler, NamedTheme};

let ocean = NamedTheme { selector: "[data-theme=\"ocean\"]".to_owned(), css: "@theme {\n  --color-accent: #0e7490;\n}".to_owned(), from: None };
let options = CompileOptions { themes: vec![ocean], root_selectors: vec!["[data-theme=\"default\"]".to_owned()], ..CompileOptions::default() };
let mut compiler = Compiler::new("@import \"tailwindcss\";\n@theme {\n  --color-accent: #1f6feb;\n}", options).unwrap();
assert!(compiler.build(["text-accent"]).contains(":root, :host, [data-theme=\"default\"] {"));
```

A named theme's selector matches any element, so `<div data-theme="ocean">` previews a theme inside a page in another. A root selector does the same for the root theme. A variable declared as `var(--other)` resolves on the element that declares it, so a token derived from another has to be declared again wherever a theme changes the one it reads.

## Drawing Classes on Cards

```rust
use snapfire_tailwind::{CompileOptions, Compiler};

let compiler = Compiler::new("@import \"tailwindcss\";", CompileOptions::default()).unwrap();
let rules = compiler.card_rules(["px-4", "bg-sky-500", "text-4xl", "md:text-6xl", "hover:underline"]);
let px = rules.iter().find(|r| r.candidate == "px-4").unwrap();
assert_eq!(px.declarations, [("padding-left".to_owned(), "1rem".to_owned()), ("padding-right".to_owned(), "1rem".to_owned())]);
let md = rules.iter().find(|r| r.candidate == "md:text-6xl").unwrap();
assert_eq!(md.min_width, Some(768.0));
assert!(rules.iter().all(|r| r.candidate != "hover:underline"));
```

Rules come in Tailwind's output order, so applying them in order gives the stylesheet's result. Variables are resolved, `calc()` of one unit is worked out, `oklch()` and opacity modifiers become sRGB and logical sides become physical ones. A breakpoint keeps its width in CSS pixels, a `rem` taken as 16. Any other variant has no rule.

## Error Handling

`Error` has two variants. `Syntax` is the stylesheet failing to parse, with `file:line:column` when `from` was given. `Message` is everything else Tailwind refuses: an unknown utility in `@apply`, an import cycle, a JavaScript plugin.

```rust
use snapfire_tailwind::{CompileOptions, Compiler, Error};

match Compiler::new(".a {\n  @apply not-a-utility;\n}", CompileOptions::default()) {
  Err(Error::Syntax(at)) => eprintln!("the stylesheet does not parse: {at}"),
  Err(Error::Message(why)) => eprintln!("Tailwind refused it: {why}"),
  Ok(_) => {}
}
```

Warnings Tailwind would print, such as a named theme setting a breakpoint, are not errors. Read them with `Compiler::warnings` after building.
