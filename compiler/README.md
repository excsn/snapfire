# Snapfire Compiler (`snapfirec`)

[![License: MPL 2.0](https://img.shields.io/badge/License-MPL_2.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)
![Crates.io](https://img.shields.io/crates/v/snapfire_compiler?style=flat-square)
![Docs.rs](https://img.shields.io/docsrs/snapfire_compiler?style=flat-square)

A bespoke, high-performance *typescript to browser* build tool written in Rust.

`Snapfire Compiler` replaces the traditional Node.js build chain (TypeScript, Vite/Rollup, PostCSS, Babel) with a single binary. It is designed to compile TypeScript libraries into browser-native ES Modules and standard CSS without requiring a `package.json` or `node_modules` folder. Task-by-task instructions live in the [usage guide](README.USAGE.md).

## Philosophy

-   **No Node.js:** The build process should not require a JavaScript runtime.
-   **Browser Native:** Output files are ES Modules ready to be imported directly by browsers (`<script type="module">`).
-   **Standards Compliant:** Reads `tsconfig.json` the way `tsc` does, so the same file can drive type checking and your editor without the three disagreeing.
-   **Types Travel With The Code:** A library can emit its own `.d.ts`, so a TypeScript consumer of the built output is not left importing `any`.
-   **Opinionated:** Includes specific transforms (like import rewriting) to make the "TypeScript to Browser" workflow seamless.

## Install

```bash
cargo install snapfire_compiler
```

```bash
cargo install --path .
```

| Feature | Adds |
| :--- | :--- |
| *(default)* | Everything below, with `--minify` doing codegen compaction |
| `minify` | `--minify=full`: mangling, inlining and dead code elimination via `swc_ecma_minifier`. Larger dependency, longer build |

## Try it

`example/` is a small project using most of what the compiler does: a nested source tree, an external resolved through an import map, a JSON asset imported by a module, a dynamically imported chunk and nested CSS.

```bash
snapfirec --root ./example --strip-log --minify --source-map --import-map importmap.json
```

```text
   Compiling TS: "index.ts"
   Compiling CSS: "ui/toast.css"
   Copying: "data/config.json"
   Externals: 'lit-html'
   All externals resolve through "importmap.json"
   Preload manifest: "dist/preload-manifest.json"
```

Then read `example/dist`, which is what a browser would be served.

## What to reach for

| You want to | Reach for |
| :--- | :--- |
| Compile a project | `snapfirec --root ./my-lib` |
| Choose which files are in it | `include`, `exclude` and `files` in `tsconfig.json` |
| Control where output lands | `compilerOptions.rootDir` and `outDir`, or `--out-dir` |
| Debug the emitted code | `--source-map`, or `sourceMap` in `tsconfig.json` |
| Ship smaller files | `--minify`, which adds a `.min` graph beside the readable one |
| Ship types with the library | `--declaration`, or `declaration` in `tsconfig.json` |
| Get fonts and images into `dist` | `--copy-assets` |
| Know which packages the page must supply | The `Externals:` line the build prints |
| Catch a missing import map entry at build time | `--import-map ./static/importmap.json` |
| Catch a relative import that points at nothing | Nothing to pass; every build checks it |
| Kill the module-discovery waterfall | `dist/preload-manifest.json`, written every build |
| Rebuild as you edit | `--watch` |
| Strip development logging | `--strip-log --strip-debug` |
| Pick browser targets | `.browserslistrc` in the project root |

## CLI Arguments

| Flag | Description | Default |
| :--- | :--- | :--- |
| `--root <PATH>` | The root directory of the project to build. | `.` (Current Directory) |
| `-c`, `--config <PATH>` | Path to the `tsconfig.json` file. | `tsconfig.json` |
| `-d`, `--out-dir <PATH>` | Override the output directory. | Read from `tsconfig` or `dist` |
| `--strip-log` | Removes all `console.log` statements from output. | `false` |
| `--strip-debug` | Removes all `console.debug` statements from output. | `false` |
| `--copy-assets` | Copies every selected file the compiler does not compile. | `false` |
| `--source-map` | Emits a `.map` beside each output. | `sourceMap` |
| `--inline-source-map` | Embeds each map in its output as a data URI. | `inlineSourceMap` |
| `--minify[=compact\|full]` | Additionally emits a minified `.min` graph. | off |
| `--declaration` | Emits a `.d.ts` beside each TypeScript output. | `declaration` |
| `--public-path <PREFIX>` | URL prefix the output is served under, used for the preload manifest. | paths, not URLs |
| `--import-map <PATH>` | Fails the build if an external is not resolved by this map. | off |
| `-w`, `--watch` | Rebuilds whenever a source changes. | `false` |

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

`compilerOptions.target` is `tsc`'s to act on; set it as your project needs. `snapfirec` only refuses a value below `es2017`, since no engine that old can load an ES module, and reports one that is not a real edition. Nothing is downlevelled here, so emitted syntax follows the source and browser support is governed by `.browserslistrc`.

To control CSS transpilation and how nesting is flattened, add a `.browserslistrc` to your project root:

```text
last 2 versions
not dead
> 0.2%
```

## Status

Active. The output layout follows `tsc`'s `rootDir` rules, so upgrading from a version before that changed where files land: set `rootDir` explicitly to pin it.

## Development & Testing

```bash
cargo test
cargo test --features minify
```

The tests create temporary directories, copy in fixtures, run the compiler, and assert on the actual file system output.

## License

This project is licensed under the **Mozilla Public License 2.0**. See the `LICENSE` file for details.
