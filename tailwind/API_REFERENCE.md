# API Reference: snapfire_tailwind

Tailwind CSS v4.3.2 in Rust: a CSS-first configuration compiled once and built for any set of class names.

## Contents

* [1. Compiling](#1-compiling)
  * [Compiler](#compiler)
  * [CompileOptions](#compileoptions)
  * [PolyfillFlags](#polyfillflags)
* [2. Loading Stylesheets](#2-loading-stylesheets)
  * [LoadStylesheet](#loadstylesheet)
  * [Loaded](#loaded)
  * [tailwind_stylesheet](#tailwind_stylesheet)
  * [TAILWIND_BASE](#tailwind_base)
* [3. Reading the Configuration](#3-reading-the-configuration)
  * [Features](#features)
  * [Source](#source)
  * [Root](#root)
* [4. Optimizing](#4-optimizing)
  * [optimize](#optimize)
  * [OptimizeOptions](#optimizeoptions)
* [5. Error Handling](#5-error-handling)
  * [Error](#error)

## 1. Compiling

### Compiler

A parsed configuration. Neither `Send` nor `Sync`: it holds `Rc` and `RefCell` caches.

* `pub fn new(css: &str, options: CompileOptions) -> Result<Compiler, Error>`: parses `css`, resolves its imports through `options.load_stylesheet`, reads `@theme`, `@utility`, `@variant`, `@custom-variant`, `@source` and `@reference`. It refuses `@plugin` and `@config`.
* `pub fn build<'c>(&mut self, candidates: impl IntoIterator<Item = &'c str>) -> String`: the stylesheet with the utilities of every candidate given to any build so far. Tailwind's output before an optimize pass. A build that adds no valid candidate returns the previous output.
* `pub fn features(&self) -> Features`: what the configuration uses.
* `pub fn sources(&self) -> &[Source]`: the `@source` paths, in order. `inline(...)` sources are not among them: their candidates are already in the build.
* `pub fn root(&self) -> Option<&Root>`: what `@import "tailwindcss" source(...)` or `@tailwind utilities source(...)` named.
* `pub fn invalid_candidates(&self) -> Vec<String>`: every candidate a build was given that is not a utility, a variant of one or a theme variable, sorted.
* `pub fn warnings(&self) -> Vec<String>`: what Tailwind would print to the console, in order.
* `pub fn canonical_candidate(&self, candidate: &str) -> Option<String>`: the candidate in Tailwind's preferred spelling, `!flex` as `flex!`. `None` when it does not parse.
* `pub fn candidate_css(&self, candidate: &str) -> Option<String>`: one candidate's CSS without polyfills. `None` when it is not a utility.

### CompileOptions

What `Compiler::new` reads besides the stylesheet. `Default` is every field below at its default. `#[non_exhaustive]`: build one with `Default` and the setters.

* `pub base: String`: the directory the stylesheet's own imports resolve against. Default empty.
* `pub load_stylesheet: Option<Box<LoadStylesheet<'a>>>`: resolves every `@import`. `None` resolves Tailwind's built-ins only.
* `pub polyfills: PolyfillFlags`: the fallbacks written for older browsers. Default `PolyfillFlags::ALL`.
* `pub banner: bool`: starts the output with Tailwind's license comment. Default `true`.
* `pub from: Option<String>`: the stylesheet's file name; a syntax error then starts with `file:line:column`, columns in UTF-16 code units. Default `None`.
* `pub fn base(self, base: String) -> Self`, `pub fn load_stylesheet(self, load_stylesheet: Option<Box<LoadStylesheet<'a>>>) -> Self`, `pub fn polyfills(self, polyfills: PolyfillFlags) -> Self`, `pub fn banner(self, banner: bool) -> Self` and `pub fn from(self, from: Option<String>) -> Self`: setters for the field of the same name.

### PolyfillFlags

Bit flags over `u8`, combined with `|` on the inner value.

* `pub const NONE`, `pub const AT_PROPERTY`, `pub const COLOR_MIX`, `pub const ALL`
* `pub fn has(self, other: PolyfillFlags) -> bool`

## 2. Loading Stylesheets

### LoadStylesheet

`pub type LoadStylesheet<'a> = dyn Fn(&str, &str) -> Result<Loaded, Error> + 'a`

Called with the id an `@import` names, quotes removed, then the importing stylesheet's base. Imports nested more than 100 deep are refused as a cycle.

### Loaded

`#[non_exhaustive]`: build one with `new`.

* `pub fn new(path: String, base: String, content: String) -> Self`
* `pub path: String`: where the stylesheet was found, for messages.
* `pub base: String`: the directory its own imports resolve against.
* `pub content: String`

### tailwind_stylesheet

`pub fn tailwind_stylesheet(id: &str, base: &str) -> Option<Loaded>`

Tailwind's own stylesheets: `tailwindcss`, `tailwindcss/index`, `tailwindcss/theme`, `tailwindcss/preflight` and `tailwindcss/utilities`, each with or without `.css`. `./theme.css`, `./preflight.css` and `./utilities.css` resolve too when `base` is [`TAILWIND_BASE`](#tailwind_base). `None` for any other id.

### TAILWIND_BASE

`pub const TAILWIND_BASE: &str = "tailwindcss"`

The base a built-in stylesheet's own imports carry.

## 3. Reading the Configuration

### Features

Bit flags over `u8`.

* `pub const NONE`, `pub const AT_APPLY`, `pub const AT_IMPORT`, `pub const JS_PLUGIN_COMPAT`, `pub const THEME_FUNCTION`, `pub const UTILITIES`, `pub const VARIANTS`, `pub const AT_THEME`
* `pub fn has(self, other: Features) -> bool`

A configuration with `Features::NONE` builds to its input printed back, with no Tailwind processing.

### Source

`#[non_exhaustive]`: build one with `new`.

* `pub fn new(base: String, pattern: String, negated: bool) -> Self`
* `pub base: String`: the directory of the stylesheet that named it.
* `pub pattern: String`: as written.
* `pub negated: bool`: `@source not "..."`.

### Root

* `Root::None`: `source(none)`.
* `Root::Path { base: String, pattern: String }`

## 4. Optimizing

### optimize

`pub fn optimize(css: &str, options: &OptimizeOptions) -> Result<String, Error>`

Tailwind's Lightning CSS pass, run twice: targets Safari 16.4, iOS Safari 16.4, Firefox 128 and Chrome 111; nesting and media query ranges always lowered; logical properties, `:dir()` and `light-dark()` never.

### OptimizeOptions

`#[non_exhaustive]`: build one with `Default` and the setters.

* `pub minify: bool`
* `pub file: Option<String>`: the file name messages name.
* `pub fn minify(self, minify: bool) -> Self`, `pub fn file(self, file: Option<String>) -> Self`: setters for the field of the same name.

## 5. Error Handling

### Error

`#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]`. `Display` is the message.

* `Error::Syntax(String)`: the stylesheet or an imported one does not parse.
* `Error::Message(String)`: anything else Tailwind refuses, such as an unknown utility in `@apply`, an import cycle, `@plugin` or `@config`, a theme block holding something other than custom properties and `@keyframes`.
