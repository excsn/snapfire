# API Reference: snapfire_compiler

`snapfirec`, the command that compiles TypeScript, TSX, JavaScript and CSS to browser ES modules and stylesheets.

## Contents

* [1. Command Line](#1-command-line)
  * [snapfirec](#snapfirec)
  * [Selecting and Writing](#selecting-and-writing)
  * [Emitting](#emitting)
  * [Resolving](#resolving)
  * [Tailwind](#tailwind)
  * [Plugins](#plugins)
  * [Typechecking](#typechecking)
* [2. Configuration](#2-configuration)
  * [tsconfig.json](#tsconfigjson)
  * [compilerOptions](#compileroptions)
* [3. Outputs](#3-outputs)
  * [Modules and Stylesheets](#modules-and-stylesheets)
  * [.snapfire-build.json](#snapfire-buildjson)
  * [.snapfire-plugin-cache.json](#snapfire-plugin-cachejson)
  * [tailwind.classes](#tailwindclasses)
* [4. Driven Protocol](#4-driven-protocol)
  * [Hello](#hello)
  * [Batches](#batches)
  * [Asset References](#asset-references)
  * [Plugin Requests](#plugin-requests)
* [5. Cargo Features](#5-cargo-features)
  * [minify](#minify)
* [6. Error Handling](#6-error-handling)
  * [Exit Codes](#exit-codes)
  * [Messages](#messages)

## 1. Command Line

### snapfirec

* `snapfirec [OPTIONS]`: one build of the project at `--root`, then exit.
* `--root <dir>`, default `.`: the directory every other relative path is read against. The process changes into it before reading anything.
* `-w`, `--watch`: rebuild whenever a source changes. Conflicts with `--driven`.
* `--driven`: rebuild the paths named on stdin instead of watching; [section 4](#4-driven-protocol). Conflicts with `--watch`.
* `-c`, `--config <path>`, default `tsconfig.json`: the tsconfig, relative to the root. An absent file is not an error.

### Selecting and Writing

* `-d`, `--out-dir <dir>`: the output directory, relative to the root. Overrides `compilerOptions.outDir`; with neither, `dist`.
* `--copy-assets`: copy every selected file the compiler does not compile into the output, at its relative path.
* `--overlay <dir>`: a directory whose files stand in for the root's at the same relative path.
* `--strip-log`: remove every `console.log` call statement.
* `--strip-debug`: remove every `console.debug` call statement.
* `--locale-from <specifier>`: the module whose `localeTag()` export is the page's locale. An `Intl` formatter, `toLocaleString`, `toLocaleDateString`, `toLocaleTimeString` or `localeCompare` written without a locale is given it.

### Emitting

* `--source-map`: a `.map` beside each output. Same as `compilerOptions.sourceMap`.
* `--inline-source-map`: the source map embedded in each output as a data URI. Same as `compilerOptions.inlineSourceMap`. With `--source-map` or `sourceMap` as well, the build refuses.
* `--declaration`: a `.d.ts` beside each TypeScript output. Same as `compilerOptions.declaration`.
* `--minify [compact|full]`: a second, minified graph beside the plain one, each output named with `.min` before its extension. Given with no value, `compact`: whitespace stripped at codegen, identifiers kept. `full` mangles, inlines and drops dead code. It needs the `minify` feature; without it the build refuses.
* `--public-path <prefix>`: the URL prefix the output is served under, recorded as `publicPath` in [.snapfire-build.json](#snapfire-buildjson).

### Resolving

* `--import-map <path>`: an import map every bare specifier the output keeps is checked against. A specifier the map does not name fails the build.
* `--asset-map <path>`: the asset map a driving process wrote (`snapfire_compiler_wire::driven::AssetMap`). Every image and font a module or stylesheet names is rewritten from its row and none is emitted by the compiler.

### Tailwind

* `--tailwind-classes <path>`: the classes a stylesheet that imports Tailwind generates utilities for, relative to the root: one class per line, optionally a tab and the modules using it.
* `--tailwind-vendor <dir>`: the vendor tree whose packages' classes a Tailwind stylesheet reads: a package's `tailwind.classes` when it ships one, else its modules. Requires `--tailwind-vendor-base`.
* `--tailwind-vendor-base <prefix>`: the URL prefix the import map names the vendor tree under. Requires `--tailwind-vendor`.
* `--emit-tailwind-classes`: write the classes the sources name to [tailwind.classes](#tailwindclasses) in the output directory.

### Plugins

* `--plugin-compiler <ext>=<path>`, repeatable: the framework compiler a plugin boots in place of the one it carries, for the extension `ext`. A value without `=` is refused.

### Typechecking

* `--typecheck`: once the build has emitted, typecheck the same tsconfig with `snapfiretc`, which compiles nothing. A diagnostic fails the build.
* `--snapfiretc <path>`: the typechecker. Absent, `$SNAPFIRETC`, else `snapfiretc` beside this binary, else on `PATH`.
* `--tsc <path>`: a `tsc` for the typechecker to use as given. Its version must be the requested one.
* `--tsc-version <version>`: the TypeScript version to typecheck with.

## 2. Configuration

### tsconfig.json

* Read keys: `compilerOptions`, `files`, `include`, `exclude`. Every other key is ignored.
* `files`: each entry is compiled; a missing one aborts the build.
* `include` (default `["**/*"]`) and `exclude`: glob patterns relative to the tsconfig's directory. A pattern matching nothing is a warning.

### compilerOptions

* `outDir`, default `dist`.
* `rootDir`: the directory outputs are written relative to. An input outside an explicit `rootDir` aborts the build.
* `target`: refused below `es2017`. Output is not downlevelled.
* `jsx`: `preserve` (the default) writes the markup through; `react-jsx` and `react-jsxdev` lower it through the automatic runtime. `react` and `react-native` are refused, since the classic runtime needs a `React` binding the compiler does not inject.
* `jsxImportSource`, default `react`: the runtime is imported from `<jsxImportSource>/jsx-runtime` (`jsx-dev-runtime` under `react-jsxdev`).
* `sourceMap`, `inlineSourceMap`, `inlineSources`, `declaration`: as the flags of the same names. `inlineSources` puts each source's text in its map.
* `baseUrl`, `paths`: tsc's aliases. A pattern has at most one `*`; the longest matching prefix wins; the first target of an entry is used. A bare specifier an alias matches is a file in the tree, not an external.

## 3. Outputs

### Modules and Stylesheets

* `.ts`, `.tsx`, `.js` and `.jsx` compile to `.js` at the same relative path under the output directory, `.mjs` to `.mjs` and `.css` to `.css`. `.ts` may not hold JSX; the others may. Two sources that write one output fail the build.
* A file of a plugin's extension, such as `.vue` or `.svelte`, is compiled by that plugin and written under its source name.
* With `--source-map`, `<output>.map` beside each output; with `--declaration`, `<output>.d.ts`; with `--minify`, `<name>.min.<ext>` beside each.
* A file the previous build listed in `outputs` that this build does not produce is removed. Nothing else in the output directory is touched.

### .snapfire-build.json

Written to the output directory on every build.

* `version`: `2`.
* `entries`: the outputs nothing else statically imports.
* `externals`: the bare specifiers the output keeps.
* `outputs`: every file the build wrote, this one included.
* `styles`, `assets`: the stylesheets and the asset files written.
* `minified`: the suffix of the minified graph, `.min`, when one was written.
* `graph`: each entry's static dependencies, transitive, in preload order. Stylesheets and dynamic imports are left out.
* `sheets`: written when a module imports a stylesheet.
* `publicPath`: the `--public-path` value, when given.

### .snapfire-plugin-cache.json

Written to the output directory when a plugin compiled anything: each compile keyed by the plugin's name, version and compiler string, the source, the options and every sibling file the component read. It is not in `outputs`.

### tailwind.classes

* Written by `--emit-tailwind-classes`: the classes the sources name, one per line.
* Read from a vendored package by `--tailwind-vendor`.

## 4. Driven Protocol

### Hello

* The first stdout line under `--driven` is `snapfirec: driven 3`, where `3` is `snapfire_compiler_wire::driven::PROTOCOL`. The first build runs at once after it.

### Batches

* Each stdin line is a path, relative to the root or absolute. An empty line ends a batch; an empty batch rebuilds everything.
* A batch ends with `snapfirec: rebuilt` or `snapfirec: failed` on stdout, after everything the build printed.
* A failed batch keeps the process running. Closing stdin ends it.

### Asset References

* With `--asset-map`, a batch that names an image or font the map lacks prints `snapfirec: references`, one path per line under the root and an empty line, then waits.
* The driver answers `mapped` after rewriting the map. The compiler reads it again and finishes the batch. A path still unmapped fails the batch.

### Plugin Requests

* Under `--driven` the compiler starts no plugin. It prints `snapfirec: plugin` and a JSON request on the next line: `{"ask":"hello","ext":…}` or `{"ask":"compile","ext":…,"units":[…]}`.
* The driver answers one JSON line: `{"answer":"hello","hello":{…}}`, `{"answer":"compiled","outcomes":[…]}` with one outcome per unit or `{"answer":"refused","why":…}`. A refusal fails the files of that extension.

## 5. Cargo Features

### minify

* Off by default. Compiles in swc's minifier, which `--minify full` needs.

## 6. Error Handling

### Exit Codes

* `0`: the build succeeded.
* `1`: any file failed or the configuration could not be read. Under `--watch` and `--driven` the process does not exit on a failed build.

### Messages

* Progress goes to stdout, findings to stderr.
* `❌`: a file failed to compile, write or copy; two sources collide; a relative import resolves to nothing. Fatal; every input is still attempted and the process exits `1` once.
* `⚠️`: a pattern matched nothing, a path could not be read or no browser targets resolved. Not fatal.
* With no prefix, before compiling starts: a malformed `tsconfig.json`, a `--root` that does not exist, a `files` entry naming a missing file, an input outside an explicit `rootDir`, a `target` below `es2017`.
