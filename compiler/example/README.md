# snapfirec example

A small project exercising most of what the compiler does. Every file is here because it demonstrates something a real build runs into, so the output is worth reading rather than just checking for.

## Run it

From the `compiler` directory:

```bash
snapfirec --root ./example
```

Or with everything turned on:

```bash
snapfirec --root ./example --strip-log --minify --source-map --import-map importmap.json
```

Then read `dist`, which is what a browser would be served.

## What each file is for

| File | Demonstrates |
| :--- | :--- |
| `src/index.ts` | Relative imports, an imported JSON asset, a dynamic import, a surviving `console.debug` |
| `src/ui/toast.ts` | A nested module, and an external the page has to resolve |
| `src/utils.ts` | An ordinary sibling module |
| `src/editor.ts` | A chunk reached only by `import()`, so it is never preloaded |
| `src/style.css` | Custom properties, `@media`, and nesting |
| `src/ui/toast.css` | Nesting with `&` and an attribute selector |
| `src/data/config.json` | An asset a module names, so the build has to deliver it |
| `tsconfig.json` | `rootDir`, `outDir`, `sourceMap`, and a `target` that `tsc` acts on |
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
├── preload-manifest.json     what each entry point needs up front
└── .snapfirec-manifest       what this build produced, so the next can clean up
```

Every compiled file also gets a `.map` beside it, because `tsconfig.json` sets `sourceMap`. Add `--minify` and a second `.min` graph appears alongside, with its own maps and its specifiers pointing only at other `.min` files.

## Things worth noticing

**Imports come out loadable.** `./ui/toast` becomes `./ui/toast.js`, because a browser resolves specifiers literally and would otherwise 404.

**The nested tree survives.** `rootDir` is `src`, so `src/ui/toast.ts` lands at `dist/ui/toast.js` rather than being flattened.

**`lit-html` passes straight through.** It is not bundled and not vendored, so the build reports it and leaves resolving it to the page:

```text
   Externals: 'lit-html'
   These need an import map in the page; nothing in the output resolves them.
```

Pass `--import-map importmap.json` and a missing entry becomes a build failure instead of a console error in production.

**The dynamic import is deferred, and stays that way.** `editor.js` is its own entry in the preload manifest rather than a dependency of `index.js`, because preloading something the author deliberately deferred would defeat the point of writing `import()`.

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

The map is inline because a page can rely on that everywhere import maps work at all, and it has to come before the first module loads.

Those two `modulepreload` lines are what `preload-manifest.json` is for: without them the browser cannot discover `utils.js` until it has fetched and parsed `index.js`, which turns one round trip into two.

The full picture is in the [usage guide](../README.USAGE.md).
