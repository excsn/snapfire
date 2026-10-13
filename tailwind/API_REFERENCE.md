# API Reference: snapfire_tailwind

Tailwind CSS v4.3.2 in Rust: a CSS-first configuration compiled once and built for any set of class names.

## Contents

* [1. Compiling](#1-compiling)
  * [Compiler](#compiler)
  * [CompileOptions](#compileoptions)
  * [NamedTheme](#namedtheme)
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
* [4. Card Declarations](#4-card-declarations)
  * [CardRule](#cardrule)
* [5. Optimizing](#5-optimizing)
  * [optimize](#optimize)
  * [OptimizeOptions](#optimizeoptions)
* [6. Error Handling](#6-error-handling)
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
* `pub fn warnings(&self) -> Vec<String>`: what Tailwind would print to the console, in order, including a named theme that sets `--breakpoint-*` or `--container-*`.
* `pub fn canonical_candidate(&self, candidate: &str) -> Option<String>`: the candidate in Tailwind's preferred spelling, `!flex` as `flex!`. `None` when it does not parse.
* `pub fn candidate_css(&self, candidate: &str) -> Option<String>`: one candidate's CSS without polyfills. `None` when it is not a utility.
* `pub fn card_rules<'c>(&self, candidates: impl IntoIterator<Item = &'c str>) -> Vec<CardRule>`: see [4. Card Declarations](#4-card-declarations).

### CompileOptions

What `Compiler::new` reads besides the stylesheet. `Default` is every field below at its default.

* `pub base: String`: the directory the stylesheet's own imports resolve against. Default empty.
* `pub load_stylesheet: Option<Box<LoadStylesheet<'a>>>`: resolves every `@import`. `None` resolves Tailwind's built-ins only.
* `pub polyfills: PolyfillFlags`: the fallbacks written for older browsers. Default `PolyfillFlags::ALL`.
* `pub banner: bool`: starts the output with Tailwind's license comment. Default `true`.
* `pub from: Option<String>`: the stylesheet's file name; a syntax error then starts with `file:line:column`, columns in UTF-16 code units. Default `None`.
* `pub themes: Vec<NamedTheme>`: written after the root theme in this order. Default empty.

### NamedTheme

A stylesheet of `@theme` blocks applied under its own selector.

* `pub selector: String`: the rule's selector, such as `[data-theme="ocean"]`. Written as given.
* `pub css: String`: read for its top-level `@theme` blocks; custom properties only, everything else ignored.
* `pub from: Option<String>`: names the file in errors and warnings.

Every variable is written, used or not. A variable the root theme lacks is added to it as a reference: its utilities exist and fall back to this theme's value; the root does not write it.

### PolyfillFlags

Bit flags over `u8`, combined with `|` on the inner value.

* `pub const NONE`, `pub const AT_PROPERTY`, `pub const COLOR_MIX`, `pub const ALL`
* `pub fn has(self, other: PolyfillFlags) -> bool`

## 2. Loading Stylesheets

### LoadStylesheet

`pub type LoadStylesheet<'a> = dyn Fn(&str, &str) -> Result<Loaded, Error> + 'a`

Called with the id an `@import` names, quotes removed, then the importing stylesheet's base. Imports nested more than 100 deep are refused as a cycle.

### Loaded

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

* `pub base: String`: the directory of the stylesheet that named it.
* `pub pattern: String`: as written.
* `pub negated: bool`: `@source not "..."`.

### Root

* `Root::None`: `source(none)`.
* `Root::Path { base: String, pattern: String }`

## 4. Card Declarations

### CardRule

One candidate as plain declarations.

* `pub candidate: String`
* `pub min_width: Option<f32>`: the width a breakpoint variant applies from, in CSS pixels, a `rem` as 16.
* `pub declarations: Vec<(String, String)>`: in order, custom properties left out.

`Compiler::card_rules` returns rules in Tailwind's output order. A candidate has none when it is not a utility or when a variant other than a minimum-width breakpoint wraps it (`hover:`, `dark:` and `group-*` among them). Values are rewritten:

* `var(--x)` from the candidate's own custom properties, then the theme, then a registered property's initial value, then the fallback.
* `calc()` of plain numbers and one unit, times or divided by numbers, as one value; any other `calc()` is left.
* `oklch(...)` and `color-mix(in oklab, <color> N%, transparent)` as `#rrggbb` or `rgba(r, g, b, a)`.
* `calc(infinity * 1px)` as `9999px`.
* `padding-inline`, `padding-block`, `margin-inline`, `margin-block`, `inset-inline`, `inset-block` and their `-start` and `-end` forms as left, right, top and bottom.

## 5. Optimizing

### optimize

`pub fn optimize(css: &str, options: &OptimizeOptions) -> Result<String, Error>`

Tailwind's Lightning CSS pass, run twice: targets Safari 16.4, iOS Safari 16.4, Firefox 128 and Chrome 111; nesting and media query ranges always lowered; logical properties, `:dir()` and `light-dark()` never.

### OptimizeOptions

* `pub minify: bool`
* `pub file: Option<String>`: the file name messages name.

## 6. Error Handling

### Error

`#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]`. `Display` is the message.

* `Error::Syntax(String)`: the stylesheet or an imported one does not parse.
* `Error::Message(String)`: anything else Tailwind refuses, such as an unknown utility in `@apply`, an import cycle, `@plugin` or `@config`, a theme block holding something other than custom properties and `@keyframes`.
