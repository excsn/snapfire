# Usage Guide: snapfirec

This guide covers running the `snapfirec` build tool: selecting source files the way `tsc` does, compiling TypeScript, JavaScript and CSS for the browser, resolving relative imports, emitting source maps and a minified graph, delivering assets, watching for changes and reading what it reports when a build fails.

## Table of Contents

* [Core Concepts](#core-concepts)
* [Quick Start](#quick-start)
* [Installing the Binary](#installing-the-binary)
* [Building a Project](#building-a-project)
  * [Choosing the Root](#choosing-the-root)
  * [Choosing the Output Directory](#choosing-the-output-directory)
  * [Selecting Source Files](#selecting-source-files)
  * [Pinning the Output Layout](#pinning-the-output-layout)
* [Compiling TypeScript](#compiling-typescript)
  * [Compiling TSX](#compiling-tsx)
  * [Compiling JavaScript](#compiling-javascript)
  * [Resolving Relative Imports](#resolving-relative-imports)
  * [Stripping Console Calls](#stripping-console-calls)
* [Compiling CSS](#compiling-css)
  * [Targeting Browsers](#targeting-browsers)
* [Emitting Source Maps](#emitting-source-maps)
* [Emitting a Minified Graph](#emitting-a-minified-graph)
* [Emitting Declarations](#emitting-declarations)
  * [Annotating Exports for It](#annotating-exports-for-it)
* [Delivering Assets](#delivering-assets)
* [Resolving Externals](#resolving-externals)
  * [Checking Relative Specifiers](#checking-relative-specifiers)
  * [Checking Against an Import Map](#checking-against-an-import-map)
* [Preloading the Module Graph](#preloading-the-module-graph)
* [Keeping the Output Directory Clean](#keeping-the-output-directory-clean)
* [Watching for Changes](#watching-for-changes)
* [Loading the Output in a Browser](#loading-the-output-in-a-browser)
* [Wiring the Build into a Snapfire Site](#wiring-the-build-into-a-snapfire-site)
* [Scope](#scope)
* [Error Handling](#error-handling)

## Core Concepts

* **`snapfirec`** - The binary produced by the `snapfire_compiler` crate. One process, one build, no subcommands.
* **Root** - The directory the compiler changes into before it does anything else. Every path on the command line resolves relative to it.
* **`tsconfig.json`** - The project file, read as JSONC and interpreted the way `tsc` interprets it, so the same file can drive `tsc --noEmit` and your editor without the three disagreeing. Keys `snapfirec` does not use are `tsc`'s business and are left alone.
* **Config directory** - The directory holding `tsconfig.json`. `include`, `exclude`, `files`, `outDir` and `rootDir` all resolve against it, not against the root.
* **`include`** - Glob patterns naming the project's files. An entry with no glob character names a directory and stands for everything under it. Defaults to `**/*`.
* **`exclude`** - Glob patterns removed from what `include` matched. Defaults to `node_modules`, `bower_components` and `jspm_packages`, and an explicit `exclude` replaces that default rather than adding to it.
* **`files`** - An explicit list of inputs. `exclude` does not apply to it.
* **`rootDir`** - The directory output paths are mirrored against. Set it explicitly, or let it be computed as the longest common prefix of the compilable inputs.
* **Compilable file** - `.ts`, `.tsx`, `.js`, `.jsx`, `.mjs` and `.css`. `.d.ts` files are skipped, and anything else is an asset.
* **Type stripping** - SWC removes type annotations without checking them. `snapfirec` is not a type checker; it never reports a type error.
* **Import resolution** - Rewriting a relative specifier into one a browser can fetch: renaming a `.ts` extension to `.js`, appending `.js`, or expanding a directory to its `index.js`.
* **Bare specifier** - An import a package resolver would have to satisfy, such as `import x from "lit"`. Left untouched, and reported at the end of the build as an external, because the page has to resolve it.
* **External** - A bare specifier the finished output carries. A URL or a root-relative path is not one, since the browser resolves those unaided.
* **Entry point** - A module nothing else statically imports, worked out from the graph rather than declared. What a page loads directly.
* **Public path** - The URL prefix the output directory is served under. Optional, and absent everywhere except the preload manifest and import map scopes, which are the only two things that cannot be expressed as paths.
* **Declaration** - The `.d.ts` describing one module's exported types. Emitted per file from that file alone, so an export whose type only inference across files could supply is an error rather than a guess.
* **Minified graph** - The parallel set of `.min` files `--minify` adds. Its specifiers point only at other `.min` files, so loading the minified entry never pulls an unminified dependency.
* **Build facts** - `.snapfire-build.json` in the output directory, recording the entry points, the module graph, the bare specifiers the output carries and every file it produced. A page preloads from it; a packager vendors from it; the next build prunes from it.
* **`.browserslistrc`** - Sets which browsers the CSS is compiled for, searched for from the root upward.

## Quick Start

```text
my-lib/
├── .browserslistrc
├── tsconfig.json
└── src/
    ├── index.ts
    ├── state.ts
    └── style.css
```

```json
// tsconfig.json
{
  "compilerOptions": {
    "outDir": "dist",
    "rootDir": "src",
  },
  "include": ["src/**/*"]
}
```

```text
# .browserslistrc
last 2 versions
not dead
```

```typescript
// src/state.ts
export const state: { count: number } = { count: 0 };
```

```typescript
// src/index.ts
import { state } from './state';

export const bump = (by: number): number => {
  console.log('bumping', by);
  state.count += by;
  return state.count;
};
```

```css
/* src/style.css */
body {
  font-family: sans-serif;

  .container {
    padding: 20px;
    margin: 0 auto;
  }
}
```

```bash
snapfirec --root ./my-lib --strip-log
```

```text
🔥 snapfirec started
   Root:     "/home/me/my-lib"
   Config:   "tsconfig.json"
   Root Dir: "src"
   Output:   "dist"
   Sources:  ["src/**/*"]
   Browser Targets: 'chrome 143, chrome 142, firefox 146, firefox 145, safari 26.2, safari 26.1'
   Compiling TS: "index.ts"
   Compiling TS: "state.ts"
   Compiling CSS: "style.css"
```

```javascript
// dist/index.js
import { state } from "./state.js";
export const bump = (by)=>{
    state.count += by;
    return state.count;
};
```

```css
/* dist/style.css */
body {
  font-family: sans-serif;
}

body .container {
  padding: 20px;
  margin: 0 auto;
}
```

## Installing the Binary

```bash
cargo install --path compiler
```

```bash
cargo install snapfire_compiler
```

Both install a binary called `snapfirec`, not `snapfire_compiler`. Add the full minifier if you want it:

```bash
cargo install snapfire_compiler --features minify
```

```bash
snapfirec --version
snapfirec --help
```

## Building a Project

One invocation with flags. There are no subcommands.

```bash
snapfirec
```

| Flag | Meaning | Default |
| :--- | :--- | :--- |
| `--root <PATH>` | Directory to build | `.` |
| `-c`, `--config <PATH>` | `tsconfig.json` path, relative to the root | `tsconfig.json` |
| `-d`, `--out-dir <PATH>` | Output directory, relative to the root | `outDir`, else `dist` |
| `--strip-log` | Delete `console.log` statements | off |
| `--strip-debug` | Delete `console.debug` statements | off |
| `--copy-assets` | Copy every selected file that is not compiled | off |
| `--source-map` | Emit a `.map` beside each output | `sourceMap` |
| `--inline-source-map` | Embed each map as a data URI | `inlineSourceMap` |
| `--minify[=compact\|full]` | Additionally emit a minified `.min` graph | off |
| `--public-path <PREFIX>` | URL prefix for the preload manifest | paths, not URLs |
| `--import-map <PATH>` | Fail the build if an external is not in this map | off |
| `-w`, `--watch` | Rebuild whenever a source changes | off |

These `tsconfig.json` keys are read, and everything else in the file is ignored:

```json
{
  "compilerOptions": {
    "outDir": "dist",
    "rootDir": "src",
    "target": "es2022",
    "sourceMap": true,
    "inlineSourceMap": false,
    "inlineSources": false,
    "jsx": "react-jsx",
    "jsxImportSource": "react"
  },
  "files": [],
  "include": ["src/**/*"],
  "exclude": ["**/*.test.ts"]
}
```

The file is JSONC, so comments and trailing commas are fine:

```json
{
  // Everything the browser gets.
  "compilerOptions": {
    "outDir": "dist",
  },
  "include": ["src",],
}
```

`target` belongs to `tsc` and `tsc` acts on it, so set it as your project needs. `snapfirec` only checks that it is satisfiable, because nothing is downlevelled here: the emitted syntax is whatever the source used, and browser support is governed by `.browserslistrc`.

Anything below `es2017` is refused, since no engine that old can load an ES module at all:

```text
Error: 'target': "es5" cannot be honoured. snapfirec emits ES modules, which no pre-ES2017 engine can load.
```

Anything at or above it passes without comment. A value that is not a real edition is reported, which catches a typo that would otherwise look like a target far in the future:

```text
⚠️  'target': "es2O22" is not recognised and has no effect.
```

### Choosing the Root

`--root` changes the working directory before anything else happens. These do the same work:

```bash
snapfirec --root ./my-lib
```

```bash
cd ./my-lib && snapfirec
```

`--out-dir` is relative to the root, and a relative path beginning with `..` escapes it, which is how several libraries land in one place:

```bash
snapfirec --root ./packages/ui   --out-dir ../../public/assets/ui
snapfirec --root ./packages/data --out-dir ../../public/assets/data
```

Everything the config names, on the other hand, resolves against the config's own directory, so a config in a subdirectory can reach back out:

```json
// configs/tsconfig.build.json
{
  "compilerOptions": { "outDir": "../dist" },
  "include": ["../src/**/*.ts"]
}
```

```bash
snapfirec --config configs/tsconfig.build.json
```

### Choosing the Output Directory

First of these that is set wins:

```bash
snapfirec --out-dir build                     # 1. the flag, relative to the root
```

```json
{ "compilerOptions": { "outDir": "lib" } }    // 2. outDir, relative to the config
```

```text
dist                                          # 3. the fallback
```

It is excluded from the source walk, so building into a directory inside a source tree does not feed output back in as input. It is created when the build has something to write, so a run that matches no inputs fails without leaving a directory behind.

### Selecting Source Files

`include` takes glob patterns. `*` matches within one path segment, `**` matches any depth:

```json
{
  "include": ["src/**/*.ts", "vendor/*.ts"],
  "exclude": ["**/*.test.ts"]
}
```

```text
src/index.ts             matched
src/ui/button.ts         matched by src/**/*.ts
src/ui/button.test.ts    excluded
vendor/legacy.ts         matched by vendor/*.ts
vendor/deep/skipped.ts   not matched, * does not cross a /
```

An entry with no glob character names a directory and stands for everything under it, so the short form still means what it always did:

```json
{ "include": ["src"] }                        // same as ["src/**/*"]
```

`files` lists inputs explicitly and `exclude` does not apply to it, which is how you compile one entry point out of a directory you otherwise ignore:

```json
{
  "files": ["src/entry.ts"],
  "exclude": ["src"]
}
```

A pattern that matches nothing is reported and skipped, so a directory produced by an earlier step can be listed before that step has run:

```json
{ "include": ["src", "generated"] }
```

```text
⚠️  'include' pattern "generated" matched no files
```

The build only fails if that leaves nothing at all to compile, which is what catches a typo:

```text
⚠️  'include' pattern "srcc" matched no files
Error: No inputs were found in "tsconfig.json". Specified 'include' paths were ["srcc"].
```

With neither `include` nor `files`, every file under the config directory is taken, and the build says so:

```text
   Sources:  ["**/*"]
⚠️  No 'include' or 'files' in "tsconfig.json": compiling every file under "/home/me/my-lib". Set 'include' to narrow it.
```

### Pinning the Output Layout

Output mirrors each file's path relative to `rootDir`. Left unset, `rootDir` is the longest common prefix of the compilable inputs, which means it moves when the set of inputs changes:

```text
src/ui/button.ts                     rootDir "src/ui"   ->  dist/button.js
src/ui/button.ts + src/main.ts       rootDir "src"      ->  dist/ui/button.js
                                                            dist/main.js
```

Adding `src/main.ts` moved `button.js` without anything else changing. Set `rootDir` to stop that:

```json
{
  "compilerOptions": { "outDir": "dist", "rootDir": "src" }
}
```

```text
src/ui/button.ts   ->  dist/ui/button.js       whatever else exists
```

The resolved value is printed on every run, so it is never a mystery:

```text
   Root Dir: "src"
```

A file outside an explicit `rootDir` is an error rather than an output written somewhere surprising:

```text
Error: File "/my-lib/vendor/legacy.ts" is not under 'rootDir' "/my-lib/src".
```

Two inputs that would produce one output fail rather than silently overwriting each other:

```text
❌ Output collision on "dist/index.js": "/my-lib/src/index.js" and "/my-lib/src/index.ts" compile to the same file
```

## Compiling TypeScript

Every `.ts` and `.tsx` file is parsed, stripped of types and written as `.js` at the mirrored path. Decorators parse without a flag, and `.d.ts` files are skipped rather than emitted as empty modules.

```typescript
// src/greet.ts
export interface Person {
  name: string;
}

export const greet = (p: Person): string => `Hello, ${p.name}!`;
```

```javascript
// dist/greet.js
export const greet = (p)=>`Hello, ${p.name}!`;
```

Types are removed, not verified, so keep `tsc --noEmit` in the loop if you want them checked. Both tools read the same `tsconfig.json` and select the same files:

```bash
tsc --noEmit && snapfirec --root ./my-lib
```

Constructs carrying runtime behaviour are lowered rather than erased:

```typescript
// src/model.ts
export enum Color { Red = 'red' }

export namespace Shapes {
  export const square = 'square';
}

export class Point {
  constructor(private readonly x: number) {}
}
```

```javascript
// dist/model.js
export var Color = /*#__PURE__*/ function(Color) {
    Color["Red"] = "red";
    return Color;
}({});
(function(Shapes) {
    Shapes.square = 'square';
})(Shapes || (Shapes = {}));
export class Point {
    x;
    constructor(x){
        this.x = x;
    }
}
export var Shapes;
```

### Compiling TSX

What JSX becomes is `jsx` in `tsconfig.json`, the same key `tsc` reads. Set it to `"react-jsx"` and JSX is lowered through the automatic runtime:

```tsx
// src/component.tsx
export const Hello = (props: { name: string }) => <div>Hello, {props.name}</div>;
```

```javascript
// dist/component.js
import { jsx as _jsx } from "react/jsx-runtime";
export const Hello = (props)=>_jsx("div", {
        children: [
            "Hello, ",
            props.name
        ]
    });
```

The runtime import is injected, never written by hand, and it is an ordinary bare import afterwards: `--import-map` fails the build when nothing resolves `react/jsx-runtime`, so a missing entry surfaces at build time instead of in the browser.

| `jsx` | Output |
| --- | --- |
| unset, or `"preserve"` | JSX written through untouched. No browser runs that file as-is; it is input for another tool. |
| `"react-jsx"` | Automatic runtime, importing `jsx`, `jsxs` and `Fragment` from `<jsxImportSource>/jsx-runtime`. |
| `"react-jsxdev"` | Automatic runtime against `<jsxImportSource>/jsx-dev-runtime`. |
| `"react"`, `"react-native"` | Refused. The classic runtime needs a `React` binding snapfirec does not inject. |

`jsxImportSource` picks the package the runtime comes from, defaulting to `react`. It is what points the same lowering at another library:

```json
{ "compilerOptions": { "jsx": "react-jsx", "jsxImportSource": "preact" } }
```

An import referenced only by an element name (`import { Badge } from "./badge.js"` used as `<Badge/>`) is kept, so type stripping cannot leave a dangling reference.

### Compiling JavaScript

`.js`, `.jsx` and `.mjs` go through the same pipeline, so a mixed tree builds in one pass. `.js` and `.jsx` emit `.js`; `.mjs` stays `.mjs`:

```javascript
// src/legacy.js
import { helper } from './helper';
console.log('starting');
export const run = () => helper();
```

```bash
snapfirec --root ./my-lib --strip-log
```

```javascript
// dist/legacy.js
import { helper } from "./helper.js";
export const run = ()=>helper();
```

### Resolving Relative Imports

Browsers resolve specifiers literally, so an extensionless import 404s. Relative specifiers are rewritten to something fetchable:

```typescript
// src/index.ts
import { state } from './state';
import { widget } from './widgets';
import { typed } from './typed.ts';
import { already } from './already.js';
import { pkg } from 'some-package';
import './theme.css';
export * from './state';

export const lazy = () => import('./state');
```

```javascript
// dist/index.js
import { state } from "./state.js";
import { widget } from "./widgets/index.js";
import { typed } from "./typed.js";
import { already } from './already.js';
import { pkg } from 'some-package';
import './theme.css';
export * from "./state.js";
export const lazy = ()=>import("./state.js");
```

Four rules. A specifier not starting with `.` is never touched, so bare names must be resolved by the page, usually with an import map:

```typescript
import { html } from 'lit';                   // stays 'lit'
```

An extension the compiler emits `.js` for is renamed rather than suffixed, which makes writing the source extension safe:

```typescript
import { a } from './a.ts';                   // becomes './a.js'
import { b } from './b.tsx';                  // becomes './b.js'
```

Any other extension is left exactly as written, so a stylesheet or data file still points at the file it names:

```typescript
import './theme.css';                         // stays './theme.css'
import data from './data.json' with { type: 'json' };
```

No extension gains `.js`, unless it names a directory, in which case it expands to that directory's `index.js`:

```typescript
import { state } from './state';              // becomes './state.js'
import { widget } from './widgets';           // becomes './widgets/index.js'
```

Static `import`, `export * from`, `export { x } from` and dynamic `import()` with a literal are all covered. A computed dynamic import is left alone, because there is nothing to rewrite:

```typescript
const mod = await import(`./locale/${lang}.js`);   // write .js yourself
```

### Stripping Console Calls

Independent flags, neither on by default:

```bash
snapfirec --strip-log                         # console.log only
snapfirec --strip-debug                       # console.debug only
snapfirec --strip-log --strip-debug           # both
```

```typescript
// src/main.ts
console.log('module loaded');

export const run = () => {
  console.log('starting');
  console.debug('details', 1);
  console.warn('kept');
  doWork();
};
```

```javascript
// dist/main.js  (built with --strip-log --strip-debug)
export const run = ()=>{
    console.warn('kept');
    doWork();
};
```

Two things bound what is removed. The call has to be on `console` itself, so a logger of your own exposing the same method names survives:

```typescript
logger.log('kept');                           // kept
this.logger.debug('kept');                    // kept
console.log('removed');                       // removed
```

And it has to be a whole statement, because deleting a call used as a value would change what the surrounding expression evaluates to:

```typescript
const x = console.log('kept');                // kept
foo(console.log('kept'));                     // kept
const f = () => console.log('kept');          // kept, it is the arrow body
```

`console.info`, `console.warn`, `console.error` and the rest have no flag and always survive.

## Compiling CSS

Every `.css` file is parsed with nesting enabled, compiled against the browser targets and written to the mirrored path. Output is readable; `--minify` adds a compressed `.min.css` beside it:

```css
/* src/style.css */
body {
  font-family: sans-serif;

  .container {
    padding: 20px;
    margin: 0 auto;
  }
}
```

```css
/* dist/style.css */
body {
  font-family: sans-serif;
}

body .container {
  padding: 20px;
  margin: 0 auto;
}
```

Nesting flattening and vendor prefixing are driven by the browser targets and happen either way. `@import` is not inlined and files are not concatenated, so each input `.css` stays a separate output `.css`.

### Targeting Browsers

Targets come from a `.browserslistrc`, searched for from the root upward, so one file at the top of a monorepo covers every package under it:

```text
# .browserslistrc
last 2 versions
not dead
> 0.2%
```

The resolved list is printed on every run, which is the fastest way to confirm the file was found:

```text
   Browser Targets: 'chrome 143, chrome 142, firefox 146, firefox 145, safari 26.2, safari 26.1'
```

The usual browserslist environment variables apply, which is the quickest way to try a different set without editing the file:

```bash
BROWSERSLIST="last 4 versions" snapfirec --root ./my-lib
```

If nothing resolves, the build says so and continues with no downlevelling and no prefixing:

```text
⚠️  No browser targets resolved: CSS will be compiled without downlevelling or prefixing
```

## Emitting Source Maps

Off by default. The `tsconfig.json` keys are the ones `tsc` defines, and the flags override them:

```bash
snapfirec --source-map                        # sourceMap
snapfirec --inline-source-map                 # inlineSourceMap
```

```json
{
  "compilerOptions": {
    "sourceMap": true,
    "inlineSources": true
  }
}
```

An external map lands beside its output, with a comment pointing at it. Both JavaScript and CSS get one:

```text
dist/
  index.js       //# sourceMappingURL=index.js.map
  index.js.map
  style.css      /*# sourceMappingURL=style.css.map */
  style.css.map
```

`sources` entries are written relative to the map, so they resolve from where the map sits rather than from wherever the build ran:

```json
{ "version": 3, "sources": ["../src/ui/button.ts"], "mappings": "…" }
```

`inlineSources` embeds the pre-compilation text in the map, which makes it self-contained and means the browser needs no access to your source tree:

```json
{ "version": 3, "sources": ["../src/ui/button.ts"], "sourcesContent": ["export const button: number = 1;\n"] }
```

Inline maps embed the whole map in the output instead, so nothing extra is written:

```javascript
// dist/index.js
export const bump = (by)=>state.count += by;
//# sourceMappingURL=data:application/json;base64,eyJ2ZXJzaW9uIjoz…
```

Setting both at once is an error, as it is under `tsc`:

```text
Error: 'sourceMap' and 'inlineSourceMap' cannot both be set.
```

## Emitting a Minified Graph

`--minify` adds files rather than replacing them. The readable output stays exactly where it was and a parallel `.min` graph appears beside it:

```bash
snapfirec --minify
```

```text
dist/
  index.js       index.min.js
  state.js       state.min.js
  widgets/
    index.js     index.min.js
  theme.css      theme.min.css
```

The point of the parallel graph is that it is self-contained. Every specifier inside a `.min` file points at another `.min` file, so loading the minified entry never drags an unminified dependency in behind it:

```javascript
// dist/index.js
import { state } from "./state.js";
import { widget } from "./widgets/index.js";
import { pkg } from 'some-package';
import './theme.css';
export const lazy = ()=>import("./state.js");
```

```javascript
// dist/index.min.js
import{state}from"./state.min.js";import{widget}from"./widgets/index.min.js";import{pkg}from"some-package";import"./theme.min.css";export const lazy=()=>import("./state.min.js");
```

Bare specifiers stay bare, and assets with no minified counterpart stay shared between the two graphs:

```javascript
import data from './data.json' with { type: 'json' };   // same file in both graphs
```

Two levels are available. `--minify` on its own is codegen compaction, which strips whitespace and keeps every identifier, so stack traces stay readable without a map:

```javascript
export const compute=input=>{const aVeryLongLocalName=input*2;const anotherVeryLongLocalName=aVeryLongLocalName+1;return anotherVeryLongLocalName;};
```

`--minify=full` runs the real minifier, which mangles, inlines and folds:

```javascript
const n=n=>2*n+1;export{n as compute};
```

That one needs a binary built with the `minify` feature, and says so rather than quietly giving you compaction:

```text
Error: '--minify=full' needs a binary built with the 'minify' feature: cargo install snapfire_compiler --features minify
```

Source maps apply to both graphs independently:

```bash
snapfirec --minify --source-map
```

```text
dist/
  index.js       index.js.map
  index.min.js   index.min.js.map
```

## Emitting Declarations

`--declaration`, or `declaration` in `tsconfig.json`, writes a `.d.ts` beside each compiled TypeScript file:

```bash
snapfirec --root ./my-lib --declaration
```

```text
   Compiling TS: "state.ts"
   Compiling TS (dts): "state.ts"
```

```text
dist/state.js
dist/state.d.ts
```

Declarations describe types, which the minified graph shares, so `--minify --declaration` still emits one set. There is no `index.min.d.ts` and no `.d.ts.map`.

Specifiers are resolved exactly as they are in the modules, so a declaration reaches its sibling declaration by the path the browser uses for the module:

```typescript
// src/index.ts
import { state } from './state';
export const current: number = state.count;
```

```typescript
// dist/index.d.ts
import { state } from "./state.js";
export declare const current: number;
```

`"./state.js"` resolves to `state.d.ts` because that substitution is TypeScript's own, which is what lets one set of specifiers serve both graphs. Bare specifiers are left alone, since a type-only import needs no import map entry.

### Annotating Exports for It

Emit is per file, with no dependency graph, for the same reason type stripping is. That is what keeps a build free of `node_modules`, and it is the whole bargain: an export the compiler would have to look in another file to describe is an error rather than a guess.

```typescript
export const inferred = build();          // ❌ TS9010
export function double(x: number) {       // ❌ TS9007
  return x * 2;
}

export const annotated: Shape = build();  // ✅
export function halve(x: number): number { // ✅
  return x / 2;
}
```

```text
❌ Error compiling "/my-lib/src/bad.ts": /my-lib/src/bad.ts:3:14: TS9010: Variable must have an explicit type annotation with --isolatedDeclarations.
```

This is TypeScript's own `isolatedDeclarations` contract, so setting `"isolatedDeclarations": true` in `tsconfig.json` makes `tsc --noEmit` report the same thing in the same terms. `snapfirec` does not read that key, since emit here is always isolated, but a project that sets it gets the errors from its editor rather than from a build.

A class keeps its private members without describing them, so nothing internal has to be annotated for the sake of the declaration:

```typescript
export declare class Toaster extends HTMLElement {
	private list;
	connectedCallback(): void;
}
```

## Delivering Assets

A module that names a file the compiler does not produce would resolve to nothing in the browser, so those files are copied whether or not you ask:

```typescript
// src/index.ts
import config from './data/config.json' with { type: 'json' };
```

```text
dist/
  index.js
  data/config.json     copied, because the emitted module names it
```

Everything else stays where it is. `--copy-assets` sweeps the rest of the selected files, which is what you want when `dist` has to be a complete servable directory:

```bash
snapfirec --copy-assets
```

```text
dist/
  index.js
  data/config.json
  img/logo.png         only with --copy-assets
  README.md            only with --copy-assets
  types.d.ts           only with --copy-assets
```

Compiled files are never also copied, so `theme.css` in the output is the compiled stylesheet rather than a byte copy of the source.

## Resolving Externals

A specifier that names a package rather than a file passes through untouched, because `snapfirec` has no `node_modules` to resolve it against and does not bundle:

```typescript
import { html } from 'lit';
import { debounce } from 'lodash/debounce';
```

```javascript
import { html } from 'lit';
import { debounce } from 'lodash/debounce';
```

A browser cannot resolve those on its own, so the build lists them once it knows the full set:

```text
   Externals: '@scope/pkg', 'lit', 'lodash/debounce'
   These need an import map in the page; nothing in the output resolves them.
```

Read it as a checklist for the page. Every name on that line has to appear in an import map, and a name that does not is a runtime failure rather than a build one:

```html
<script type="importmap">
{
  "imports": {
    "lit": "https://cdn.jsdelivr.net/gh/lit/dist@3/core/lit-core.min.js",
    "lodash/debounce": "https://cdn.jsdelivr.net/npm/lodash-es@4/debounce.js",
    "@scope/pkg": "/assets/vendor/pkg.js"
  }
}
</script>
<script type="module" src="/assets/index.js"></script>
```

Serving a Tera site, the natural home for that block is the base template, so one file carries the versions for every page.

Two kinds of specifier never appear on the line, because nothing has to resolve them:

```typescript
import { cdn } from 'https://cdn.example.com/mod.js';   // a URL
import { local } from '/assets/vendor.js';              // root-relative
```

The third option is to avoid externals altogether. Drop a package's ESM build into your source tree and import it relatively, and it stops being external: `snapfirec` compiles it as an ordinary input, rewrites its imports, folds it into the `.min` graph and caches it with everything else.

```typescript
import { html } from './vendor/lit-core.js';
```

Nothing is reported when a build has no externals, so a quiet line means the output is self-contained.

### Checking Relative Specifiers

Every build resolves each relative specifier against what it produced, and a target that was never emitted fails the build:

```text
❌ "dist/index.js" imports './nope.js', which resolves to nothing
```

Nothing enables this and nothing turns it off. It needs no type information, only the output paths the build already resolved, so it catches the case a rename leaves behind: the module compiles, the output looks right, and the browser 404s on a specifier nobody re-read. A dynamic `import()` with a literal argument is checked the same way, since deferring the fetch only moves when the 404 happens.

Both graphs are checked against themselves, so a `.min.js` naming an unminified sibling is reported too. Copied assets count as produced, so a stylesheet or a font the build delivers resolves.

A named import is checked against what the target actually exports, which catches the other half of a rename:

```text
❌ "dist/index.js" imports 'notExported' from './real.js', which does not export it
```

`export *` is followed, so a name a barrel offers only by re-export resolves normally. A star at a bare specifier is the one case that cannot be settled here, since only the page supplying that module knows what it carries; the importing module's names go unchecked rather than reported wrongly.

### Checking Against an Import Map

Point the build at the map the page serves and a missing entry becomes a build failure rather than a console error in production:

```bash
snapfirec --import-map ./static/importmap.json
```

```text
   Externals: 'lit', 'lodash/debounce'
   All externals resolve through "./static/importmap.json"
```

```text
   Externals: 'lit', 'lodash/debounce'
❌ 'lodash/debounce' is not resolved by "./static/importmap.json"
Error: Build failed. See the errors above.
```

Resolution follows the spec rather than matching keys, so a trailing-slash key covers everything beneath it and is not reported as missing:

```json
{ "imports": { "lodash/": "https://cdn.jsdelivr.net/npm/lodash-es@4/" } }
```

```text
   All externals resolve through "./static/importmap.json"
```

Scopes are the exception, because a scope selects a mapping by the URL of the module doing the importing, and a build with no public path has only paths. Say where the output will be served and they can be evaluated:

```bash
snapfirec --import-map ./static/importmap.json
```

```text
⚠️  "./static/importmap.json" defines scopes, which are keyed by the importing module's URL. Without --public-path only 'imports' can be checked.
```

```bash
snapfirec --import-map ./static/importmap.json --public-path /assets/
```

```text
   All externals resolve through "./static/importmap.json"
```

## Preloading the Module Graph

Unbundled modules are discovered one hop at a time: the browser cannot ask for `state.js` until it has fetched and parsed the module that imports it. Deep graphs turn into serialised round trips, and that is what bundling is usually bought to avoid.

Every build writes `.snapfire-build.json`, whose `graph` names what each entry point needs, so the page can ask for all of it at once instead:

```json
{
  "version": 1,
  "entries": ["index.js", "deferred.js", "standalone.js"],
  "externals": ["markdown-it"],
  "outputs": [".snapfire-build.json", "index.js", "deep/a.js", "deep/b.js"],
  "minified": ".min",
  "graph": {
    "index.js": ["deep/a.js", "deep/b.js"],
    "deferred.js": [],
    "standalone.js": []
  }
}
```

One file, for a page and for a packager alike. `entries` and `graph` are what a page preloads from. `outputs`, `externals` and `minified` are what a tool that vendors this output would otherwise recover by parsing the JavaScript, and the compiler already resolved every one of them.

Paths stay in the output directory's own terms. `--public-path` is recorded as `publicPath` rather than prefixed onto every path, so one build is mountable anywhere and a consumer joins the two.

An entry point is a module nothing else statically imports, which the build works out for itself. Dependencies are transitive, cycles are walked once, and stylesheets are left out because `modulepreload` is the wrong `rel` for them.

Dynamic imports are edges but never preloads, since deferring them is the point of writing one:

```typescript
export const lazy = () => import('./deferred');   // an entry of its own, not a dependency of this file
```

Paths are relative to the output directory by default, which keeps the build usable at any mount point. Give it the URL prefix and it emits URLs instead:

```bash
snapfirec --public-path /assets/
```

```json
{
  "index.js": ["/assets/deep/a.js", "/assets/deep/b.js"]
}
```

Consuming it from a Tera template keeps the base path where it already lives, on the server:

```html
{% for dep in preload[entry] %}
<link rel="modulepreload" href="{{ asset_base }}{{ dep }}">
{% endfor %}
<script type="module" src="{{ asset_base }}{{ entry }}"></script>
```

With `--minify` the minified graph is walked separately, so `index.min.js` gets a `graph` record listing `.min` dependencies and never mixes the two. It is not listed in `entries`, because `minified` states the suffix that derives it and naming both would make a consumer pair them back up.

## Keeping the Output Directory Clean

`outputs` in `.snapfire-build.json` lists what the build produced. The next build removes anything in that list it no longer produces, and touches nothing else:

```bash
snapfirec                                     # dist/{button.js, panel.js}
mv src/ui/panel.ts src/ui/board.ts
snapfirec
```

```text
   Compiling TS: "board.ts"
   Compiling TS: "button.ts"
   Removed: "dist/panel.js"
```

Only manifest entries are ever deleted, which is what makes it safe to build into a directory holding files you wrote by hand:

```bash
snapfirec --root ./assets --out-dir ../static
```

```text
static/
  index.js          managed by snapfirec
  favicon.ico       never touched
  hand-written.css  never touched
```

Dropping a flag cleans up after it, so the stale `.min` graph goes when you stop asking for one:

```bash
snapfirec --minify                            # dist/button.js, dist/button.min.js
snapfirec                                     # Removed: "dist/button.min.js"
```

A failed build prunes nothing, so a compile error never deletes the outputs it could not replace.

## Watching for Changes

```bash
snapfirec --watch
```

```text
👀 watching "src"; press Ctrl-C to stop
```

Editing a file already in the build recompiles that file alone. Adding or deleting one, or editing `tsconfig.json` or `.browserslistrc`, rebuilds everything, because any of those can change which files are selected and where they land:

```text
   Compiling TS: "button.ts"                  edit, one file
   Compiling TS: "button.ts"                  add, everything
   Compiling TS: "extra.ts"
   Compiling TS: "panel.ts"
   Removed: "dist/old.js"                     delete, pruned by the full rebuild
```

A compile error is reported and the watcher keeps running, so a mistyped line does not end the session:

```text
❌ Error compiling "/my-lib/src/ui/panel.ts": /my-lib/src/ui/panel.ts:1:16: Expression expected
   waiting for changes
```

Editors save in bursts, writing a file then renaming it then touching its mode. Events are batched until the filesystem has been quiet for 120ms, so one save is one rebuild.

## Loading the Output in a Browser

The output is ES modules and plain CSS, served as static files. Nothing else is needed at runtime:

```html
<link rel="stylesheet" href="/assets/style.css">
<script type="module" src="/assets/index.js"></script>
```

Bare specifiers that survived the build have to be resolved by the page, and the build's `Externals:` line tells you exactly which:

```html
<script type="importmap">
{
  "imports": {
    "lit": "https://cdn.jsdelivr.net/gh/lit/dist@3/core/lit-core.min.js"
  }
}
</script>
<script type="module" src="/assets/index.js"></script>
```

Point production at the minified graph and it stays minified all the way down:

```html
<link rel="stylesheet" href="/assets/style.min.css">
<script type="module" src="/assets/index.min.js"></script>
```

## Wiring the Build into a Snapfire Site

Compile into the directory the `snapfire` crate serves and watches, and a rebuild becomes a live reload:

```rust
let app_state = TeraWeb::builder("templates/**/*.html")
  .watch_static("static")
  .build()
  .expect("Failed to build TeraWeb app");
```

```bash
snapfirec --root ./assets --out-dir ../static --watch
```

The two watchers compose through the filesystem with nothing shared between them. A `.css` change lands in a watched directory and `watch_static` swaps the stylesheet in place; a `.js` change triggers a full page reload. Manifest pruning is what makes pointing at a shared `static/` safe.

A production build strips the development logging out of the assets the same way the `devel` feature strips it out of the server:

```bash
snapfirec --root ./assets --out-dir ../static --strip-log --strip-debug --minify
cargo build --release
```

## Scope

`snapfirec` compiles files. These jobs belong to something else, mostly on purpose:

| Not this tool's job | Why, and what covers it |
| :--- | :--- |
| Type checking | Stripping types is per-file and needs no dependency graph, which is exactly what lets a build run with no `node_modules`. `tsc --noEmit` does the checking, over the same `tsconfig.json` and therefore the same files. Declaration *emit* is here, since isolated declarations is per-file too, and so is the graph check below, since resolving a specifier needs no types. Checking those types is not |
| Bundling | Output is browser-native ES modules by design. An import map resolves the bare specifiers, and the `Externals:` line names them |
| Downlevelling | Every engine that can load an ES module is already ES2017 or later, so there is no lower target worth emitting for. `target` is checked for satisfiability and otherwise left to `tsc` |
| `@import` inlining | Bundling again, for stylesheets. Each input `.css` stays a separate output the browser fetches |
| Content hashing | Not a principled exclusion, just not built. Cache busting usually belongs to whatever serves the files |

Bundling is the one worth a second look, because the reason it is usually reached for is latency rather than bundling itself, and [Preloading the Module Graph](#preloading-the-module-graph) addresses that directly. What stays out of reach without it is tree shaking and cross-module mangling, both of which need the whole program in one place.

A production pipeline therefore has more than one step in it, and each step is a tool that does one thing:

```bash
tsc --noEmit                                              # types
snapfirec --root ./assets --out-dir ../static \           # compile
  --strip-log --strip-debug --minify --source-map
cargo build --release                                     # server
```

Two consequences are worth stating outright rather than leaving to be discovered.

Nothing stops you shipping code that does not type check, so the `tsc --noEmit` step is load-bearing rather than optional. It is only sound because the file selection matches: both tools read the same `include`, `exclude`, `files` and `rootDir`, so neither can be looking at a file the other ignores.

And without bundling there is no tree shaking and no cross-module mangling, so `--minify=full` can only rename within a single file. That costs little for code you wrote and imported deliberately, and a great deal if you ever start pulling in large third-party packages, which is the point at which this set of trade-offs stops being the right one.

## Error Handling

`snapfirec` exits `0` on success and `1` on failure, printing progress on stdout and everything else on stderr. Under `--watch` nothing exits: failures are reported and the watcher waits.

```bash
snapfirec --root ./my-lib
echo $?
```

| Prefix | Meaning | Fatal |
| :--- | :--- | :--- |
| `❌` | A file failed to compile, write or copy, or two sources collide | Yes |
| `⚠️` | A pattern matched nothing, a path could not be read, or no browser targets resolved | No |

A relative specifier naming something the build did not produce is reported with the same `❌` prefix, after every file has been compiled:

```text
❌ "dist/index.js" imports './nope.js', which resolves to nothing
```

A compile error names the file, the line and the column:

```text
❌ Error compiling "/my-lib/src/bad.ts": /my-lib/src/bad.ts:2:29: Expression expected
Error: Build failed due to compilation errors.
```

Errors the parser recovers from are reported too rather than compiled into subtly wrong output, so one file can produce several lines before the build gives up.

The build does not stop at the first bad file. Every input is attempted and every failure reported, then the process exits `1` once. That makes a fix-and-rerun loop cheap, and it means the output directory holds a partial build after a failure, so treat a non-zero exit as "do not ship this directory".

Failures before compilation starts abort immediately with no prefix: a malformed `tsconfig.json`, a `--root` that does not exist, a `files` entry naming a missing file, an input outside an explicit `rootDir`, or a `target` below `es2017`.

```text
Error: Failed to set working directory to "./nope"

Caused by:
    No such file or directory (os error 2)
```

An absent `tsconfig.json` is not an error at all: `include` defaults to `**/*` and `outDir` to `dist`. If a build compiles the wrong tree, check the banner before suspecting the file's contents, since it reports every resolved value the build actually used:

```text
   Config:   "tsconfig.json"
   Root Dir: "src"
   Output:   "dist"
   Sources:  ["src/**/*"]
```
