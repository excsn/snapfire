# API Reference: snapfire_compiler

`snapfirec`, the command that compiles TypeScript, TSX, JavaScript and CSS to browser ES modules and stylesheets.

## Contents

* [1. Command Line](#1-command-line)
  * [`snapfirec`](#snapfirec)
* [2. Configuration](#2-configuration)
  * [`tsconfig.json`](#tsconfigjson)
  * [`compilerOptions`](#compileroptions)
* [3. Outputs](#3-outputs)
  * [Compiled files](#compiled-files)
  * [`.snapfire-build.json`](#snapfire-buildjson)
  * [`.snapfire-plugin-cache.json`](#snapfire-plugin-cachejson)
  * [`tailwind.classes`](#tailwindclasses)
* [4. Driven Protocol](#4-driven-protocol)
  * [`snapfirec: driven`](#snapfirec-driven)
  * [Batch](#batch)
  * [`snapfirec: references`](#snapfirec-references)
  * [`snapfirec: plugin`](#snapfirec-plugin)
* [5. Cargo Features](#5-cargo-features)
  * [`minify`](#minify)
* [6. Error Handling](#6-error-handling)
  * [Exit codes](#exit-codes)
  * [Messages](#messages)

## 1. Command Line

### snapfirec

`snapfirec [OPTIONS]`: one build of the project at `--root`, then exit. Every relative path is read against the root.

* `--root <dir>`, default `.`: the process changes into it before reading anything.
* `-w`, `--watch`: rebuild whenever a source changes. Conflicts with `--driven`.
* `--driven`: rebuild the paths named on stdin, as in section 4. Conflicts with `--watch`.
* `-c`, `--config <path>`, default `tsconfig.json`: the tsconfig. An absent file is read as an empty one.
* `-d`, `--out-dir <dir>`: the output directory. Overrides `compilerOptions.outDir`. With neither, `dist`.
* `--copy-assets`: copy every selected file the compiler does not compile into the output, at its relative path.
* `--overlay <dir>`: a directory whose files stand in for the root's at the same relative path. A file `files` names may exist in the overlay alone.
* `--strip-log`: remove every `console.log` call statement.
* `--strip-debug`: remove every `console.debug` call statement.
* `--locale-from <specifier>`: the module whose `localeTag()` export is the page's locale. An `Intl` formatter, `toLocaleString`, `toLocaleDateString`, `toLocaleTimeString` or `localeCompare` called without a locale is given `localeTag()`.
* `--source-map`: a `.map` beside each output. Same as `compilerOptions.sourceMap`.
* `--inline-source-map`: the source map embedded in each output as a data URI. Same as `compilerOptions.inlineSourceMap`. The build refuses it together with `--source-map` or `sourceMap`.
* `--declaration`: a `.d.ts` beside each TypeScript output. Same as `compilerOptions.declaration`.
* `--minify [compact|full]`: a second, minified graph beside the plain one, each output named with `.min` before its extension. With no value, `compact`: whitespace stripped at codegen, identifiers kept. `full` mangles, inlines and drops dead code. `full` needs the `minify` feature and is refused without it.
* `--public-path <prefix>`: the URL prefix the output is served under, recorded as `publicPath` in `.snapfire-build.json`. A trailing `/` is added when missing.
* `--import-map <path>`: an import map every bare specifier the output keeps is checked against. A specifier the map does not name fails the build.
* `--asset-map <path>`: the asset map a driving process wrote, `snapfire_compiler_wire::driven::AssetMap` at version `1`. Every image and font a module or stylesheet names is rewritten from its row and none is emitted. A reference the map lacks fails its source.
* `--tailwind-classes <path>`: the classes a stylesheet that imports Tailwind generates utilities for. One class per line, optionally followed by a tab and the modules using it.
* `--tailwind-vendor <dir>`: the vendor tree whose packages' classes a Tailwind stylesheet reads. A package's `tailwind.classes` when it ships one, else its modules. Requires `--tailwind-vendor-base`.
* `--tailwind-vendor-base <prefix>`: the URL prefix the import map names the vendor tree under. Requires `--tailwind-vendor`.
* `--emit-tailwind-classes`: write the classes the sources name to `tailwind.classes` in the output directory.
* `--plugin-compiler <ext>=<path>`, repeatable: the framework compiler the plugin for `ext` boots in place of the one it carries. A value without `=` and an `ext` other than `vue` or `svelte` are refused.
* `--typecheck`: once the build has emitted without a failure, typecheck the same tsconfig with `snapfiretc`. A diagnostic fails the build.
* `--snapfiretc <path>`: the typechecker. Without it, `$SNAPFIRETC`, else `snapfiretc` beside this binary, else on `PATH`.
* `--tsc <path>`: a `tsc` for the typechecker to use as given. Its version must be the requested one.
* `--tsc-version <version>`: the TypeScript version to typecheck with.

## 2. Configuration

### tsconfig.json

Read as JSONC, with comments and trailing commas.

* Read keys: `compilerOptions`, `files`, `include`, `exclude`. Every other key is ignored.
* `files`: each entry is compiled. A missing one aborts the build.
* `include` and `exclude`: glob patterns relative to the tsconfig's directory. `include` defaults to `["**/*"]`. A pattern matching nothing is a warning.

### compilerOptions

* `outDir`, default `dist`.
* `rootDir`: the directory outputs are written relative to. An input outside an explicit `rootDir` aborts the build.
* `target`: refused below `es2017`. A value that is not an ECMAScript edition is a warning. Output is not downlevelled.
* `jsx`: `preserve`, the default, writes the markup through. `react-jsx` and `react-jsxdev` compile it to calls into the automatic runtime. `react`, `react-native` and any other value are refused.
* `jsxImportSource`, default `react`: the runtime is imported from `<jsxImportSource>/jsx-runtime`. Under `react-jsxdev` it is `<jsxImportSource>/jsx-dev-runtime`.
* `sourceMap`, `inlineSourceMap` and `declaration`: as the flags of the same names.
* `inlineSources`: each source's text is put in its map.
* `baseUrl` and `paths`: tsc's aliases. A pattern has at most one `*`. The longest matching prefix wins and the first target of an entry is used. A bare specifier an alias matches is a file in the tree, not an external.

`.browserslistrc` in the root sets the CSS targets.

## 3. Outputs

### Compiled files

* `.ts`, `.tsx`, `.js` and `.jsx` compile to `.js` at the same relative path under the output directory. `.mjs` compiles to `.mjs` and `.css` to `.css`.
* `.ts` may not hold JSX. The other script extensions may.
* Two sources that write one output fail the build.
* A file of a plugin's extension, `.vue` or `.svelte`, is compiled by that plugin and written under its source name.
* `--source-map` adds `<output>.map` beside each output. `--declaration` adds `<output>.d.ts`. `--minify` adds `<name>.min.<ext>`.
* After a build with no failure, a file the previous build listed in `outputs` that this build did not write is removed. Nothing else in the output directory is touched.

### .snapfire-build.json

Written to the output directory on every build. Paths are relative to the output directory.

* `version`: `2`.
* `entries`: the outputs nothing else statically imports, minified files left out.
* `externals`: the bare specifiers the output keeps.
* `outputs`: every file the build wrote, this one included.
* `styles`: the stylesheet each plugin component produced, as `<source>.css`.
* `sheets`: each module that imports stylesheets, with those sheets in import order. Present only when one does.
* `assets`: each emitted image or font as `source`, `path`, `url` when there is a public path, `hash`, `kind` and, for an image whose size was read, `width` and `height`.
* `minified`: `.min`, when `--minify` was given.
* `publicPath`: the `--public-path` value, when given.
* `graph`: each entry's static dependencies, transitive, in preload order. Stylesheets and dynamic imports are left out.

### .snapfire-plugin-cache.json

Written to the output directory when a plugin compiled anything. Each compile is keyed by the plugin's name, version and compiler string, the source, the options and every sibling file the component read. Not listed in `outputs`.

### tailwind.classes

* Written by `--emit-tailwind-classes`: the classes the sources name, one per line.
* Read from a vendored package by `--tailwind-vendor`.

## 4. Driven Protocol

Constants and types are in `snapfire_compiler_wire::driven`.

### snapfirec: driven

* The first stdout line under `--driven` is `snapfirec: driven 3`. `3` is `driven::PROTOCOL`.
* The first full build runs straight after it.

### Batch

* Each stdin line is a path, relative to the root or absolute. An empty line ends a batch. An empty batch rebuilds everything.
* A batch ends with `snapfirec: rebuilt` or `snapfirec: failed` on stdout, after everything the build printed.
* A failed batch keeps the process running. Closing stdin ends it.

### snapfirec: references

* With `--asset-map`, a batch that names an image or font the map lacks prints `snapfirec: references`, then one root-relative path per line and an empty line. The compiler then waits.
* The driver answers `mapped` once it has rewritten the map. The compiler reads the map again and finishes the batch. A path still unmapped fails the batch.

### snapfirec: plugin

* Under `--driven` the compiler starts no plugin. It prints `snapfirec: plugin` and a `PluginRequest` as JSON on the next line: `{"ask":"hello","ext":…}` or `{"ask":"compile","ext":…,"units":[…]}`.
* The driver answers with one `PluginAnswer` JSON line: `{"answer":"hello","hello":{…}}`, `{"answer":"compiled","outcomes":[…]}` with one outcome per unit or `{"answer":"refused","why":…}`.
* A refusal fails the files of that extension. A batch may ask any number of times before it ends.

## 5. Cargo Features

### minify

* Off by default. Compiles in swc's minifier, which `--minify full` needs.

## 6. Error Handling

### Exit codes

* `0`: the build succeeded.
* `1`: a file failed, the configuration could not be read or no input was found. Under `--watch` and `--driven` a failed build does not end the process.

### Messages

* Progress goes to stdout and findings go to stderr.
* `❌` is fatal: a file failed to compile, write or copy, two sources collide or a relative import resolves to nothing. Every input is still attempted and the process exits `1` once.
* `⚠️` is not fatal: a pattern matched nothing, a path could not be read or no browser targets resolved.
* An unprefixed error stops the build before compiling: a malformed `tsconfig.json`, a `--root` that does not exist, a `files` entry naming a missing file, an input outside an explicit `rootDir` or a `target` below `es2017`.
* `No inputs were found in <config>. Specified 'include' paths were <patterns>.`: the build selected nothing.
