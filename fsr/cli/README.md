# snapfire_fsr_cli

[![Crates.io](https://img.shields.io/crates/v/snapfire_fsr_cli.svg)](https://crates.io/crates/snapfire_fsr_cli)
[![Docs.rs](https://docs.rs/snapfire_fsr_cli/badge.svg)](https://docs.rs/snapfire_fsr_cli)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

`fsr` is the CLI for a SnapFire FSR application.

It scaffolds an app, runs it in development, tests and lints it, builds the IR plan file and writes the deploy tree.

It is also a Rust library: `snapfire_fsr_cli::build` runs the same build as `fsr build` inside your own process, which is how the examples' Rust tests build their apps. 

* The task guide is [README.USAGE.md](README.USAGE.md).
* The call surface is [API_REFERENCE.md](API_REFERENCE.md).

## Install

```sh
cargo install snapfire_fsr_cli
```

The default features cover a web application. Add `--features desktop` for a native window. `--no-default-features --features terminal` builds an `fsr` for terminal programs only.

| Feature | Default | Adds |
| --- | --- | --- |
| `test` | on | `fsr test` and the QuickJS engine it runs specs in. It turns `web` on. Off, the binary links no JavaScript engine |
| `web` | on | The web half: `fsr prerender`, `bundle`, `sites`, `census`, `use`, `add`, `types` and `fonts`. Off, `fsr` still builds, serves and checks an app but has none of these commands |
| `tera` | on | `page.tera` and `layout.tera` routes, rendered by the stock host |
| `mdx` | on | `.mdx` and `.md` routes and imports, `[mdx]` and `fsr use <app> mdx` |
| `mdx-highlight` | on | `mdx`, plus fenced code highlighted at build and `styles/code.css` from the direction |
| `mdx-frontmatter` | on | `mdx`, plus YAML or TOML frontmatter as `export const meta` |
| `share_cards` | on | Card images drawn by the stock host |
| `grpc` | on | `clients/<name>.proto` contracts, imported at build and called over gRPC |
| `graphql` | on | `clients/<name>.graphql` contracts |
| `http` | on | OpenAPI contracts called over HTTP by the stock host |
| `tls` | off | TLS in the stock host |
| `client-source-maps` | off | The browser client's source maps, with its TypeScript, served beside it so devtools show the client's source; about 1.3 MB |
| `desktop` | off | Experimental. It turns `web` on. `fsr dev --desktop` and `fsr serve --desktop`: the application in a native window over the system webview, through wry, tao and muda |
| `terminal` | off | Experimental. `fsr dev --terminal`: the root page run in the terminal with Ink's elements and `useInput`, through `snapfire_fsr_terminal` and crossterm. Needs no `web`, so `--no-default-features --features terminal` is an `fsr` without Tera or MDX |

## Commands

| Command | What it does |
| --- | --- |
| `fsr new <dir>` | Scaffold an app; `--with react` adds a framework, `--shell`, `--site` and `--terminal` scaffold the other kinds |
| `fsr migrate <from> [dir]` | Rewrite a Next.js, SvelteKit, Nuxt or Ink app into an FSR project; with no `dir`, move an FSR project onto the client and host imports |
| `fsr use <app> <direction>` | Adopt react, vue, svelte, elements, htmx, tailwind, tera or mdx |
| `fsr add <app> <pkg>@<ver>` | Vendor a package for the browser and name it in the import map |
| `fsr types <app>` | Fetch declarations for every package the import map names |
| `fsr fonts add <app> <spec>` | Download a provider's font faces, as `google:Inter@400,700` |
| `fsr dev <app>` | Build, serve and rebuild on change |
| `fsr serve <app>` | Serve an app that is already built |
| `fsr run <app> -- <args>` | Run a terminal program once |
| `fsr test <app>` | Run the specs and the Rust tests of the crate beside the app |
| `fsr lint <app>` | Lint the source with ESLint's rules |
| `fsr check <app>` | Build and typecheck, writing only what the checker reads |
| `fsr build <app>` | Build without serving |
| `fsr doctor <app>` | Report what a deployment would find wrong before it ships |
| `fsr prerender <app>` | Write the documents for every prerenderable route |
| `fsr bundle <app>` | Write the deploy tree, checked first |
| `fsr census <app>...` | Count what the build refuses or leaves to the browser, by cause |
| `fsr sites <cmd>` | Mount sites in a shell: `link`, `unlink`, `list`, `pack`, `install`, `pin`, `hash` and `reload` |
| `fsr ir view <app> <id>` | Print what the server runs for one loader, action, handler or component |

## Common tasks

| To do this | Use |
| --- | --- |
| Start a React app and run it | `fsr new app --with react`, then `fsr dev app` |
| Check the app in CI | `fsr check app`, `fsr lint app --sarif` and `fsr test app` |
| Ship an app with no Rust crate beside it | `fsr bundle app`, which puts `fsr-server` in the tree |
| Check a deployment's configuration before release | `fsr doctor app --runtime`, with `--env`, `--app-env` and `--region` for the ladder |
| Give bodies typed `services` | a document under `app/clients/`: `.openapi.json`, `.proto` or `.graphql` |
| Let one team's site mount under another's shell | `fsr new site --site --at /docs --into shell` or `fsr sites link shell site` |
| Find why a component renders only in the browser | `fsr build app --verbose`, then `fsr ir view` |
| Keep an xwpm application in step | an `xwpm.wmf` in the app; the same commands delegate to it |
| Run the build from Rust | `build` and `write`, then read the `Report` |
