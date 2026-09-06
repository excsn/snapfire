# SnapFire

![Project Status: Active](https://img.shields.io/badge/status-active-success.svg)
[![License: MPL 2.0](https://img.shields.io/badge/License-MPL_2.0-brightgreen.svg)](LICENSE)

**Write the application in TypeScript. Run it in Rust.**

SnapFire started as SnapFire Web, a Tera templating library with live reload. It is now a stack built around SnapFire FSR, a runtime where routes, data loading, actions, service clients, sessions, identity, caching, streaming and client navigation are primitives rather than features owned by a UI framework. Beside it sit a compiler that turns TypeScript into browser-native ES modules with no Node and a typechecker that fetches and pins its own `tsc`. SnapFire Web is still here and is now one renderer among several.

Nothing in the chain needs Node, at build time or on the request path. There is no `node_modules`.

## The pieces

Each one installs and is used from its own README.

| What you want | Where it is |
| --- | --- |
| A full-stack application: file routes, loaders and actions in TypeScript, rendered by Rust | [SnapFire FSR](fsr/README.md), the `fsr` executable |
| TypeScript and CSS to browser ES modules, one binary, no `package.json` | [SnapFire Compiler](compiler/README.md), the `snapfirec` executable |
| Types checked against a pinned TypeScript, fetched and verified rather than found installed | [SnapFire Typecheck](typecheck/README.md), the `snapfiretc` executable |
| Tera templates with live reload inside an Actix application | [SnapFire Web](web/README.md), the `snapfire` crate |
| Seven applications in reading order, each carrying what the ones before it do not reach | [The SnapFire FSR examples](fsr/examples/README.md) |
| One question per chapter, with a lab on the running example | [The SnapFire FSR guide](fsr/docs/guide/README.md) |

## Layout

| Directory | What it holds |
| --- | --- |
| `fsr/` | SnapFire FSR: the runtime crates, the client library, the CLI, the guide and the examples |
| `compiler/` | SnapFire Compiler, `snapfirec`, with its own example |
| `typecheck/` | SnapFire Typecheck, `snapfiretc` |
| `web/` | SnapFire Web, the `snapfire` crate: Tera and Actix, with its own example |
| `www/` | the project's site, built with SnapFire FSR |

Every crate carries a `README.md` to skim, a `README.USAGE.md` to work from and an `API_REFERENCE.md` for the surface.

## License

Mozilla Public License 2.0. See `LICENSE`.

[Excerion Sun LLC](https://www.excsn.com)
