# SnapFire Compiler (`snapfirec`)

[![Crates.io](https://img.shields.io/crates/v/snapfire_compiler.svg)](https://crates.io/crates/snapfire_compiler)
[![Docs.rs](https://docs.rs/snapfire_compiler/badge.svg)](https://docs.rs/snapfire_compiler)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

Compiles TypeScript, TSX, JavaScript and CSS into ES modules and stylesheets a browser loads directly. It is one Rust binary, `snapfirec`. It needs no Node.js, `package.json` or `node_modules`.

It reads `tsconfig.json` the way `tsc` does, so the same file drives the build, the type checker and your editor. It can also emit `.d.ts` declarations, minify, check every external against an import map and write the preload list each entry point needs.

Use it to build a TypeScript library or a page's scripts for the browser. An FSR application never calls it directly, because `fsr` runs it for you.

* The task guide is [README.USAGE.md](README.USAGE.md).
* The command, its outputs and the driven protocol are in [API_REFERENCE.md](API_REFERENCE.md).

## Install

```bash
cargo install snapfire_compiler
```

From a checkout, `cargo install --path compiler`.

| Feature | Adds |
| :--- | :--- |
| *(default)* | Everything below. `--minify` strips whitespace and keeps identifiers |
| `minify` | `--minify=full`, which also mangles, inlines and removes dead code. The binary takes longer to build |

## Try it

[`example/`](example/README.md) is a small project that uses most of what the compiler does. It has a nested source tree, an external resolved through an import map, a JSON asset imported by a module, a dynamically imported chunk and nested CSS.

```bash
snapfirec --root ./example --strip-log --minify --source-map --import-map importmap.json
```

```text
   Compiling TS: "index.ts"
   Compiling CSS: "ui/toast.css"
   Copying: "data/config.json"
   Externals: 'lit-html'
   All externals resolve through "importmap.json"
   Build facts: "dist/.snapfire-build.json"
```

Then read `example/dist`, which is what a browser would be served.

## What to reach for

| Problem | Piece |
| :--- | :--- |
| Compile a project | `snapfirec --root ./my-lib` |
| Choose which files are in it | `include`, `exclude` and `files` in `tsconfig.json` |
| Control where output lands | `compilerOptions.rootDir` and `outDir` (or `--out-dir`) |
| Debug the emitted code | `--source-map` or `sourceMap` in `tsconfig.json` |
| Ship smaller files | `--minify`, which adds a `.min` graph beside the readable one |
| Ship types with the library | `--declaration` or `declaration` in `tsconfig.json` |
| Turn JSX into calls a browser runs | `"jsx": "react-jsx"` in `tsconfig.json` |
| Compile a `.vue` component | Put `snapfirec-vue` on `PATH` with `cargo install snapfire_vue`. Nothing to pass |
| Compile a `.svelte` component | Put `snapfirec-svelte` on `PATH` with `cargo install snapfire_svelte`. Nothing to pass |
| Skip recompiling components nothing changed | Nothing to pass. Unchanged components come from the plugin cache and the banner says how many |
| Hash an imported image or font and rewrite the reference | Nothing to pass. A default import or a CSS `url()` of one is written under its hash. An image's displayed size is read too |
| Rewrite every image and font reference from a map another tool wrote, emitting none | `--asset-map <path>` |
| Build a Tailwind CSS v4 stylesheet for the classes the sources use | `@import "tailwindcss"` in the stylesheet |
| Get other files into `dist` | `--copy-assets` |
| Know which packages the page must supply | The `Externals:` line the build prints |
| Catch a missing import map entry at build time | `--import-map ./static/importmap.json` |
| Catch a relative import that points at nothing | Nothing to pass. Every build checks it |
| Preload every module an entry point needs | The `graph` in `dist/.snapfire-build.json`, written every build |
| Check the types it strips | `--typecheck`, which runs `snapfiretc` over the same tsconfig |
| Rebuild as you edit | `--watch` |
| Rebuild what another process names on stdin | `--driven` |
| Strip development logging | `--strip-log --strip-debug` |
| Pick browser targets | `.browserslistrc` in the project root |

## CLI Arguments

| Flag | Description | Default |
| :--- | :--- | :--- |
| `--root <PATH>` | The project directory. Every other path is relative to it. | `.` |
| `-c`, `--config <PATH>` | The `tsconfig.json` file. | `tsconfig.json` |
| `-d`, `--out-dir <PATH>` | The output directory. | `outDir` from the tsconfig, else `dist` |
| `--strip-log` | Removes every `console.log` statement. | off |
| `--strip-debug` | Removes every `console.debug` statement. | off |
| `--copy-assets` | Copies every selected file the compiler does not compile. | off |
| `--source-map` | Writes a `.map` beside each output. | `sourceMap` |
| `--inline-source-map` | Embeds each map in its output as a data URI. | `inlineSourceMap` |
| `--minify[=compact\|full]` | Also writes a minified `.min` copy of every output. | off |
| `--declaration` | Writes a `.d.ts` beside each TypeScript output. | `declaration` |
| `--public-path <PREFIX>` | The URL prefix the output is served under, recorded in `.snapfire-build.json`. | none |
| `--import-map <PATH>` | Fails the build when an external is missing from this map. | off |
| `--locale-from <SPECIFIER>` | A module exporting `localeTag()`. Every `Intl` formatter or `toLocale*` call written without a locale uses the page's locale from it. | off |
| `--overlay <PATH>` | Reads a file from this directory in place of the one at the same path under `--root`. | off |
| `--asset-map <PATH>` | Rewrites image and font references from this map and emits none. | off |
| `--tailwind-classes <PATH>` | Extra classes to generate Tailwind utilities for, one per line. | none |
| `--tailwind-vendor <PATH>` | The vendor tree whose packages' classes Tailwind reads. Needs `--tailwind-vendor-base`. | none |
| `--tailwind-vendor-base <PREFIX>` | The URL prefix the import map uses for that vendor tree. | none |
| `--emit-tailwind-classes` | Writes the classes the sources use to `tailwind.classes`. | off |
| `--plugin-compiler <EXT>=<PATH>` | The Vue or Svelte compiler a plugin boots instead of the one it carries. | the plugin's own |
| `--typecheck` | Runs `snapfiretc` over the same tsconfig after the build. A diagnostic fails the build. | off |
| `--tsc <PATH>` | The `tsc` the typechecker uses. Its version must be the requested one. | resolved |
| `--tsc-version <VERSION>` | The TypeScript version to typecheck with. | the checker's default |
| `--snapfiretc <PATH>` | The typechecker itself. | `$SNAPFIRETC`, beside this binary, then `PATH` |
| `-w`, `--watch` | Rebuilds whenever a source changes. | off |
| `--driven` | Rebuilds the paths another process writes on stdin. The protocol is in the API reference. | off |

## Configuration

`snapfirec` reads `tsconfig.json` as JSONC, so comments and trailing commas are accepted. These keys are used and the rest are ignored:

```json
{
  "compilerOptions": {
    "outDir": "dist",
    "rootDir": "src",
    "sourceMap": true,
    "declaration": true
  },
  "include": ["src/**/*"],
  "exclude": ["**/*.test.ts"]
}
```

Set `compilerOptions.target` for `tsc` as your project needs. `snapfirec` refuses a value below `es2017`, because no engine that old can load an ES module. It warns about a value that is not an ECMAScript edition.

`snapfirec` does not downlevel scripts, so the emitted syntax matches the source.

A `.browserslistrc` in the project root sets how far CSS is compiled down, including how nesting is flattened:

```text
last 2 versions
not dead
> 0.2%
```

## Development & Testing

```bash
cargo test
cargo test --features minify
```

## License

This project is licensed under the **Mozilla Public License 2.0**. See the [LICENSE](LICENSE) file for details.
