# snapfirec example

A small project that uses most of what the compiler does. Each file shows one thing a real build runs into. The output in `dist` is meant to be read.

## Run it

From the `compiler` directory:

```bash
snapfirec --root ./example
```

Or with everything turned on:

```bash
snapfirec --root ./example --strip-log --minify --source-map --import-map importmap.json
```

Then read `example/dist`, which is what a browser would be served.

## What each file is for

| File | Demonstrates |
| :--- | :--- |
| `src/index.ts` | Relative imports, an imported JSON asset, an imported stylesheet, a dynamic import and a `console.debug` that `--strip-log` leaves in |
| `src/ui/toast.ts` | A nested module and an external the page has to resolve |
| `src/utils.ts` | An ordinary sibling module |
| `src/editor.ts` | A chunk reached only by `import()`, so it is never preloaded |
| `src/style.css` | Custom properties, `@media` and nesting |
| `src/ui/toast.css` | Nesting with `&` and an attribute selector |
| `src/data/config.json` | An asset a module names, so the build has to deliver it |
| `tsconfig.json` | `rootDir`, `outDir`, `sourceMap` and a `target` that `tsc` acts on |
| `importmap.json` | What the page must supply for `lit-html` |
| `.browserslistrc` | How far the CSS is compiled down |

## What you get

```text
dist/
├── index.js                  entry point
├── utils.js
├── editor.js                 an entry of its own, loaded on demand
├── style.css
├── data/config.json          copied, because a module names it
├── ui/
│   ├── toast.js              nested, mirroring src/ui
│   └── toast.css
└── .snapfire-build.json      what this build produced
```

Every compiled file also gets a `.map` beside it, because `tsconfig.json` sets `sourceMap`. Add `--minify` and a second `.min` graph appears alongside, with its own maps. Its specifiers point only at other `.min` files.

`.snapfire-build.json` lists the entry points, the externals, every file written and, in `graph`, what each entry needs preloaded. The next build reads its `outputs` list to remove files it no longer writes.

## Things worth noticing

**Imports come out loadable.** `./ui/toast` becomes `./ui/toast.js`. A browser resolves specifiers literally and would otherwise get a 404.

**The nested tree survives.** `rootDir` is `src`, so `src/ui/toast.ts` lands at `dist/ui/toast.js` instead of being flattened.

**`lit-html` passes straight through.** It is not bundled and not vendored. The build reports it and leaves resolving it to the page:

```text
   Externals: 'lit-html'
   These need an import map in the page; nothing in the output resolves them.
```

Pass `--import-map importmap.json` and a missing entry fails the build instead of showing up as a console error in production.

**The dynamic import stays deferred.** `editor.js` is its own entry in `.snapfire-build.json` and is not in the preload list for `index.js`. The author wrote `import()` to load it later, so the build does not preload it.

**Nested CSS is flattened for the browsers you named.** `& span` becomes `.sonner-toast span`.

## Serving it

The output is ES modules and plain CSS. `lit-html` needs the import map, which has to appear before the first module loads:

```html
<script type="importmap">
{ "imports": { "lit-html": "https://cdn.jsdelivr.net/npm/lit-html@3/lit-html.js" } }
</script>
<link rel="stylesheet" href="/style.css">
<link rel="stylesheet" href="/ui/toast.css">
<link rel="modulepreload" href="/ui/toast.js">
<link rel="modulepreload" href="/utils.js">
<script type="module" src="/index.js"></script>
```

The map is inline because every browser that supports import maps supports an inline one.

The two `modulepreload` lines come from the `graph` entry for `index.js` in `.snapfire-build.json`. Without them the browser cannot discover `utils.js` until it has fetched and parsed `index.js`, which turns one round trip into two.

The full picture is in the [usage guide](../README.USAGE.md).
