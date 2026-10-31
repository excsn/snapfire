# 307. Linting

The question this chapter answers: how do you get what ESLint gives a TypeScript project, in an application that has no Node?

**For:** everyone, in development and in CI.

## Running it

```sh
fsr lint app
```

It lints everything under the application that it can read:
- the `.ts`, `.tsx`, `.js` and `.jsx` files, with the recommended rules of ESLint, eslint-plugin-react with its hooks rules, jsx-a11y and typescript-eslint, plus six that are off until `fsr.lint.toml` turns them on (`fsr lint --rules` lists them);
- the `<script>` blocks of `.vue` and `.svelte` components with the same rules;
- their templates, through the framework's own compiler;
- `.md` and `.mdx` pages;
- `.tera` templates;
- FSR's own rules over all of these.

It prints each finding with its rule, file, line and column, then the totals:

```
error    eslint/no-unused-vars              app/routes/notes/page.tsx:3:9  Variable 'draft' is declared but never used. Unused variables should start with a '_'.
                                            Consider removing this declaration.
lint     1 error, 0 warnings in 31 files
```

An error is exit 1 and nothing found is exit 0, so a CI step stops on it. `--deny-warnings` fails on a warning too. `--fix` applies the safe fixes and then reports what is left. `--sarif` prints SARIF for a CI system that annotates pull requests. `--json` prints the findings for another tool. `--rules` lists every rule with its default level.

No build runs it. `fsr build`, `fsr check`, `fsr dev` and `fsr bundle` never lint, so a warning never blocks a reload or a deploy. A lint rule says code is probably wrong; the build says it cannot be built.

The vendored packages and the fetched declarations are not the application's and are skipped: `vendor/` and `types/`, unless an `xwpm.wmf` names other directories. So is everything `.gitignore` names, which in a new project is `generated/`, `dist/` and `.fsr/`.

## What FSR adds

Six rules know what only FSR knows:

| Rule | Finds |
| --- | --- |
| `fsr/no-browser-globals-in-render` | `window`, `document` or `localStorage` read while a component renders; in a loader, `actions.ts` or `route.ts` read anywhere. Every page renders on the server first, where they do not exist. |
| `fsr/no-file-read-in-render` | `file.read` from `@snapfire/fsr/host` called while a component renders, so it repeats on every render. Read it in the loader. |
| `fsr/no-focused-tests` | `test.only`, `it.only`, `describe.only`, `fit` or `fdescribe` from `@snapfire/fsr/testing` or `@snapfire/fsr/client/testing`, which skip the rest of the file while the run passes |
| `fsr/no-module-state-in-render` | a module-level variable written while a component renders, directly or through a function of the file. Each request renders on its own copy, so the write is lost. A cache filled the first time is not a finding |
| `fsr/output-name-clash` | two sources snapfirec writes to one name, such as `format.ts` beside `Format.js` |
| `import/no-extraneous-dependencies` | an import of a package vendored only because another package imports it, such as `clsx` brought in by `svelte`. `fsr add` declares it. In an xwpm application, a module the import map holds that `[modules]` does not name. It is a warning. It keeps ESLint's name so ESLint's comments for it still work |

FSR also tells `react/no-unknown-property` about htmx's attributes, so `hx-get` and `hx-swap` on a JSX element are not findings.

## Templates and pages

A `.vue` component's template is checked by eslint-plugin-vue's essential rules: `vue/require-v-for-key`, `vue/no-use-v-if-with-v-for`, `vue/require-component-is` and `vue/no-template-key`. A `.svelte` component reports every warning Svelte's compiler gives, its accessibility checks among them, as `svelte/` and the warning's code: `svelte/a11y-missing-attribute`. Both go through `snapfirec-vue` and `snapfirec-svelte`, the plugins the build uses. When one is not installed the file gets one `lint/compiler` warning and its scripts are still linted.

A `.md` or `.mdx` page is checked by `@eslint/markdown`'s correctness rules: a skipped heading level, a second `#` title, an empty link, an image with no alt text, a code fence with no language, a `#fragment` link no heading makes. A route page's front-matter `title` is its `meta`, so it does not count as the page's first heading; a page with a `title` and one `#` heading is fine.

A `.tera` template is handed to tera. What tera cannot parse is a finding at the place tera names. `tera/no-safe-filter` flags `| safe`, `tera/no-unused-set` a `{% set %}` nothing reads and `tera/no-debug` a leftover `__tera_context`.

## Setting levels

Levels live in `fsr.lint.toml` at the project root, beside `config/` rather than in it. A file in `config/` can be overridden per environment. A rule's level is a property of the source, not of a deployment.

```toml
[rules]
"eslint/no-unused-vars" = ["warn", { argsIgnorePattern = "^_" }]
"no-empty" = "off"
"svelte/a11y-autofocus" = "off"

[[overrides]]
files = ["routes/**/*.test.ts"]
rules = { "eslint/no-debugger" = "off" }

[settings."jsx-a11y".components]
Image = "img"
```

The levels and names are ESLint's: `off`, `warn` and `error`, also written `0`, `1` and `2`. A rule with options takes a list with the level first. A name without a plugin is the ESLint rule. `@typescript-eslint/` and `react-hooks/` names mean the rules they name here, so the `rules` of an existing ESLint configuration carry over as written. `[settings]` is ESLint's `settings`, which the React and jsx-a11y rules read. Globs are relative to the application directory. With no file, every recommended rule is an error, as with `eslint:recommended`.

A site in the portal shape inherits the shell's levels when it sits inside the shell's repository. `fsr lint` reads every `fsr.lint.toml` from the repository root down to the project, so the site's file holds only what differs. `inherit = false` at the top of a file starts it from the defaults.

`fsr migrate` writes `fsr.lint.toml` from an application's ESLint configuration: `.eslintrc.json`, `package.json`'s `eslintConfig` or an `eslint.config.js` read without running it. It names each rule and preset that has no counterpart.

## Silencing one finding

```ts
// fsr-lint-disable-next-line eslint/no-debugger -- kept while profiling the scroll
debugger;
```

ESLint's comments work too, so code brought over keeps the comments it has: `eslint-disable-next-line`, `eslint-disable-line` and the `eslint-disable` and `eslint-enable` pair. In a template or a Markdown page the comment is `<!-- -->`, in MDX `{/* */}` and in Tera `{# #}`. Text after ` -- ` is the reason. A comment naming a rule `fsr lint` has, that silences nothing, is reported as `lint/unused-disable`, which catches a comment left behind after its code was fixed.

## What it does not lint yet

The type-aware rules of typescript-eslint, which need a type checker running beside the linter, are not here; `fsr check` runs the type checker itself. Neither are eslint-plugin-react's rules that need the React Compiler. An MDX page's imports and expressions are read but not linted as JavaScript. A suppression comment naming a rule `fsr lint` does not have is left alone, since what it silences does not run here.
