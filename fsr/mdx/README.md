# snapfire_fsr_mdx

[![Crates.io](https://img.shields.io/crates/v/snapfire_fsr_mdx.svg)](https://crates.io/crates/snapfire_fsr_mdx)
[![Docs.rs](https://docs.rs/snapfire_fsr_mdx/badge.svg)](https://docs.rs/snapfire_fsr_mdx)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

Turns `.mdx` and `.md` files into page components for an FSR application, without Node.

`convert` writes a TSX module whose default export renders the file, the same shape `@mdx-js/mdx` produces. It can highlight fenced code at build time and read YAML or TOML front matter into `export const meta`. Every byte it writes maps back to the source, so an error in a page points at the line the author wrote.

It writes two versions of each file. One is for the server render, with your components for markdown elements imported directly. The other is for the browser, where the components come from `useMDXComponents` and the `components` prop, as MDX's own output does.

Most applications never depend on this crate. `fsr build` uses it for `.mdx` and `.md` routes and imports when `fsr` is built with its `mdx` features, which are on by default. Use it directly when you build your own tooling around MDX.

* The task guide is [README.USAGE.md](README.USAGE.md).
* The call surface is [API_REFERENCE.md](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_fsr_mdx = { version = "0.1", features = ["highlight", "frontmatter"] }
```

| Feature | Adds |
| --- | --- |
| `highlight` | Fenced code highlighted at build time as nested spans whose classes are TextMate scopes, plus `highlight::theme_css` to colour them |
| `frontmatter` | YAML or TOML front matter written as `export const meta` |

Asking for what a feature adds without it is an `Error::Feature`.

## Quick start

```rust
use snapfire_fsr_mdx::{convert, Format, Options, Target};

let source = "# Hello\n\nSome *markdown*.\n";
let converted = convert(source, &Options::new(Format::Md, Target::Browser))?;
assert!(converted.tsx.contains("MDXContent"));
assert_eq!(converted.toc[0].id, "hello");
assert_eq!(converted.excerpt.as_deref(), Some("Some markdown."));
```

## What to reach for

| Problem | Piece |
| --- | --- |
| Turn a file into the module the server renders | `convert` with `Target::Lowered` |
| Turn a file into the module the browser runs | `convert` with `Target::Browser` and `Options::provider` |
| Use an application's components for markdown's elements | `Options::mapped` |
| Find the source line for a byte of the written module | `Converted::origin`, `original_offset`, `position` |
| List a file's headings | `Converted::toc` |
| Show a file's first paragraph in a list | `Converted::excerpt` |
| Tell whether a file exports `meta` | `Converted::meta`, `Converted::exports` |
| Keep highlight classes out of a class list | `Converted::classes` |
| Colour highlighted code | `highlight::theme_css` |

## What it supports

* CommonMark and GFM: tables, strikethrough, task lists, footnotes and literal autolinks.
* Heading ids as `rehype-slug` writes them, with a `toc` export listing the headings.
* An `export default` layout and a `wrapper` component.
* Code fences with `title="..."`, line highlights such as `{1,3-5}` and `showLineNumbers`, as `rehype-pretty-code` reads them.

What it does not support:

* Remark and rehype plugins, which are JavaScript, are not run.
* Raw HTML in a `.md` file is left out with a warning, as MDX's `md` format leaves it out.
* Math is not parsed.
