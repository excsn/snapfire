# SnapFire Project

[![License: MPL 2.0](https://img.shields.io/badge/License-MPL_2.0-brightgreen.svg)](LICENSE)

**Write the application in TypeScript. Run it in Rust.**

SnapFire is a web stack with no Node. You write pages, loaders and actions in TypeScript and a Rust server renders them.

Nothing needs Node, at build time or on the request path. There is no `node_modules`.

Demos, documentation, tutorials and guides are on the [SnapFire website](https://snapfirers.com).

## SnapFire FSR

[SnapFire FSR](fsr/README.md) is the flagship. It is a full-stack runtime: file routes, data loading, actions, service clients, sessions, identity, caching, streaming and client navigation, with React, Vue, Svelte or Tera for the UI.

```sh
cargo install snapfire_compiler snapfire_fsr_cli
cd fsr/examples/shopping_react_ts
cargo build
fsr dev app
```

* [The guide](fsr/docs/guide/README.md) teaches it one question per chapter, with a lab on the running example.
* [The examples](fsr/examples/README.md) are complete applications in reading order.

## The tools

| What you want | Where it is |
| --- | --- |
| TypeScript and CSS compiled to browser ES modules | [SnapFire Compiler](compiler/README.md), the `snapfirec` executable |
| Types checked against a pinned TypeScript that it fetches and verifies | [SnapFire Typecheck](typecheck/README.md), the `snapfiretc` executable |
| ESLint's rules over TypeScript, Vue, Svelte, Markdown and Tera | [snapfire_lint](lint/README.md), which `fsr lint` runs |
| Tailwind CSS v4 built in Rust | [snapfire_tailwind](tailwind/README.md) |
| Image sizes and EXIF orientation, font families, weights and fallback metrics | [snapfire_media](media/README.md) |
| Sitemaps that enforce every protocol limit when you build them | [snapfire_sitemap](sitemap/README.md) |
| Tera templates with live reload inside an Actix application | [SnapFire Web](web/README.md), the `snapfire` crate |

Each one installs and is used from its own README.

## License

Mozilla Public License 2.0. See `LICENSE`.

[Excerion Sun LLC](https://www.excsn.com)
