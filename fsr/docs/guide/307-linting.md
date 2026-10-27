# 307. Linting

The question this chapter answers: how do you get what ESLint gives a TypeScript project, in an application that has no Node?

**For:** everyone, in development and in CI.

## Running it

```sh
fsr lint app
```

It lints every script under the application with ESLint's 61 recommended rules: the `.ts`, `.tsx`, `.js` and `.jsx` files, plus the `<script>` blocks of `.vue` and `.svelte` components. It prints each finding with its rule, file, line and column, then the totals:

```
error    eslint/no-unused-vars              app/routes/notes/page.tsx:3:9  Variable 'draft' is declared but never used. Unused variables should start with a '_'.
                                            Consider removing this declaration.
lint     1 error, 0 warnings in 31 files
```

An error is exit 1 and nothing found is exit 0, so a CI step stops on it. `--deny-warnings` fails on a warning too. `--json` prints the findings for another tool. `--rules` lists every rule with its default level.

No build runs it. `fsr build`, `fsr check`, `fsr dev` and `fsr bundle` never lint, so a warning never blocks a reload or a deploy. A lint rule says code is probably wrong; the build says it cannot be built.

The vendored packages and the fetched declarations are not the application's and are skipped: `vendor/` and `types/`, unless an `xwpm.wmf` names other directories. So is everything `.gitignore` names, which in a new project is `generated/`, `dist/` and `.fsr/`.

## Setting levels

Levels live in `fsr.lint.toml` at the project root, beside `config/` rather than in it. A file in `config/` can be overridden per environment. A rule's level is a property of the source, not of a deployment.

```toml
[rules]
"eslint/no-unused-vars" = ["warn", { argsIgnorePattern = "^_" }]
"no-empty" = "off"

[[overrides]]
files = ["routes/**/*.test.ts"]
rules = { "eslint/no-debugger" = "off" }
```

The levels and names are ESLint's: `off`, `warn` and `error`, also written `0`, `1` and `2`. A rule with options takes a list with the level first. A name without a plugin is the ESLint rule, so the `rules` of an existing ESLint configuration carry over as written. Globs are relative to the application directory. With no file, every recommended rule is an error, as with `eslint:recommended`.

A site in the portal shape inherits the shell's levels when it sits inside the shell's repository. `fsr lint` reads every `fsr.lint.toml` from the repository root down to the project, so the site's file holds only what differs. `inherit = false` at the top of a file starts it from the defaults.

## Silencing one finding

```ts
// fsr-lint-disable-next-line eslint/no-debugger -- kept while profiling the scroll
debugger;
```

ESLint's comments work too, so code brought over keeps the comments it has: `eslint-disable-next-line`, `eslint-disable-line` and the `eslint-disable` and `eslint-enable` pair. Text after ` -- ` is the reason. A comment naming a rule `fsr lint` has, that silences nothing, is reported as `lint/unused-disable`, which catches a comment left behind after its code was fixed.

## What it does not lint yet

The rules are ESLint's core recommended set. React's hooks rules, jsx-a11y, the typescript-eslint rules, Vue and Svelte templates, Markdown and Tera come later. A suppression comment naming one of those rules is left alone, since what it silences does not run here yet.
