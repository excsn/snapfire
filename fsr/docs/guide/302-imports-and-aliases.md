# 302. Imports and aliases

The question this chapter answers: why does a page write `@src/ui/Header` rather than `../../src/ui/Header`, who understands that and what does the browser see?

**For:** everyone.

## The five aliases

Every fsr application has the same five import aliases, each a prefix rooted at the app directory:

| Alias | Resolves to |
| --- | --- |
| `@app/*` | `app/*` |
| `@routes/*` | `app/routes/*` |
| `@src/*` | `app/src/*` |
| `@schemas/*` | `app/schemas/*` |
| `@generated/*` | `app/generated/*` |

The build writes them into both generated tsconfigs, so they are the same in every application and a reader who sees `@generated/client` knows where it points without opening a config. The two framework imports sit beside them: `@snapfire/fsr` for the authoring types a body uses and `@snapfire/fsr/testing` for the test helpers, both mapped to files under `generated/`.

Sibling imports stay relative. `ProductCard.tsx` imports `./Stars` because the two live together; a page two directories away imports `@src/ui/ProductCard`. Use an alias to cross folders and keep `./` for files in the same folder.

## Who resolves an alias

An import is read by three things and each resolves an alias to the same file.

**TypeScript** reads `paths` from `tsconfig.json`, which is what makes the editor follow the import and `tsc` check it.

**snapfirec** reads the same `paths` from `tsconfig.build.json` and rewrites an aliased specifier to the relative path of the target's output, then applies its usual rules: `.js` appended, a directory to its `index.js`, a target that does not exist a build error. The browser never sees an alias and the import map is not involved, so `@src/ui/Header` costs nothing at runtime that `../../src/ui/Header` did not.

**The lowerers** resolve the five prefixes themselves when a page imports a component or a helper and when a test imports a loader. The table they use is the one the build writes, so the three cannot disagree.

A bare specifier that is not an alias is still an external: `react`, `sweetalert2`, `@snapfire/fsr-client`. Those resolve through the import map [chapter 301](301-dependencies-without-npm.md) maintains; snapfirec refuses a build whose import map does not cover one.

## Custom aliases

`@ui/*` or `@components/*` would each be a name to learn and custom alias maps in large React projects are hard to read for that reason. The five fixed prefixes match the five directories every application has. An application cannot declare its own alias, since both tsconfigs are generated; this is deliberate. If the need is real, the place to add one is a build setting and not a hand edit to a generated file.

## The lab

Open `dist/routes/page.js` after a bundle and read its imports: `../src/ui/Header.js`, relative, with the extension. The source said `@src/ui/Header`. Then open `tsconfig.build.json` at the app root and find the `paths` block that snapfirec read to do it.

Now write `import { Header } from "@ui/Header"` in a page and run `fsr check app`. The report marks the page `client`, since the lowerer cannot follow the alias; run the bundle and snapfirec reports `@ui/Header` as an external the import map does not resolve.
