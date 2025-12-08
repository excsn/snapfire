# Snapfire Compiler (`snapfirec`)

[![License: MPL 2.0](https://img.shields.io/badge/License-MPL_2.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

A bespoke, high-performance *typescript to browser* build tool written in Rust.

`Snapfire Compiler` replaces the traditional Node.js build chain (TypeScript, Vite/Rollup, PostCSS, Babel) with a single binary. It is designed to compile TypeScript libraries into browser-native ES Modules and standard CSS without requiring a `package.json` or `node_modules` folder.

## Philosophy

-   **No Node.js:** The build process should not require a JavaScript runtime.
-   **Browser Native:** Output files are ES Modules ready to be imported directly by browsers (`<script type="module">`).
-   **Standards Compliant:** Respects `tsconfig.json` and `.browserslistrc`.
-   **Opinionated:** Includes specific transforms (like import rewriting) to make the "TypeScript to Browser" workflow seamless.

## Features

-   **TypeScript Compilation:** Uses [SWC](https://swc.rs) to strip types and output modern JavaScript.
-   **CSS Processing:** Uses [LightningCSS](https://lightningcss.dev) to flatten nested CSS, handle vendor prefixing, and minify output.
-   **Import Rewriting:** Automatically rewrites relative imports (e.g., `import './state'`) to include extensions (`import './state.js'`), which is required for native browser modules.
-   **Console Stripping:** Optional flags to strip `console.log` and `console.debug` statements from the output using AST transformations.
-   **Targeting:** Reads `.browserslistrc` to determine CSS transpilation targets.

## Installation

Since this is a local toolchain, install it from the source:

```bash
# Inside the compiler directory
cargo install --path .
```

## Usage

Run the compiler pointing at your project root. It will look for a `tsconfig.json` to determine input directories and output paths.

```bash
snapfirec --root ./example
```

Inspect the example/dist directory for final output.

### CLI Arguments

| Flag | Description | Default |
| :--- | :--- | :--- |
| `--root <PATH>` | The root directory of the project to build. | `.` (Current Directory) |
| `--config <PATH>` | Path to the `tsconfig.json` file. | `tsconfig.json` |
| `-d`, `--out-dir <PATH>` | Override the output directory. | Read from `tsconfig` or `dist` |
| `--strip-log` | Removes all `console.log` statements from output. | `false` |
| `--strip-debug` | Removes all `console.debug` statements from output. | `false` |
| `-w`, `--watch` | Watch for file changes (Not yet implemented). | `false` |

## Configuration

### TypeScript
`snapfirec` reads the `compilerOptions.outDir` and `include` arrays from your `tsconfig.json`.

```json
{
  "compilerOptions": {
    "outDir": "dist",
    "target": "es2020"
  },
  "include": ["src"]
}
```

### CSS Targeting
To ensure your CSS works in specific browsers (and to control how nesting is flattened), add a `.browserslistrc` file to your project root.

```text
last 2 versions
not dead
> 0.2%
```

## Transforms

### Import Rewriting
TypeScript allows importing files without extensions, but browsers throw 404s. `snapfirec` parses the AST and rewrites these paths automatically.

**Input (`src/index.ts`):**
```typescript
import { state } from './state';
```

**Output (`dist/index.js`):**
```javascript
import { state } from "./state.js";
```

### Console Stripping
When using `--strip-log` or `--strip-debug`, the compiler removes the specific console statement entirely, ensuring clean production builds without leaving empty lines or semi-colons.

## Development & Testing

This project uses integration tests to validate the CLI and compiler logic.

```bash
# Run the test suite
cargo test
```

The tests create temporary directories, generate fixture files, run the compiler, and assert on the actual file system output.

## License

This project is licensed under the **Mozilla Public License 2.0**. See the `LICENSE` file for details.