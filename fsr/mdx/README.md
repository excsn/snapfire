# snapfire_fsr_mdx

License: MPL-2.0. Status: used by `snapfire_fsr_cli` behind its `mdx` features.

MDX and markdown for SnapFire FSR. `convert` turns an `.mdx` or `.md` file into a TSX module whose default export renders it, as `@mdx-js/mdx` compiles one, with a map from every byte it wrote back to the source so a diagnostic names the line the author wrote. It writes one module for the build to lower and another for the browser. It can also highlight fenced code into classed spans at build. The [usage guide](README.USAGE.md) walks through each task and the [API reference](API_REFERENCE.md) lists the surface.

## Install

```toml
[dependencies]
snapfire_fsr_mdx = { version = "0.1", features = ["highlight", "frontmatter"] }
```

| Feature | Adds |
| --- | --- |
| `highlight` | Fenced code highlighted at build with syntect and bat's grammars (`two-face`), as nested spans whose classes are TextMate scopes, plus `highlight::theme_css` |
| `frontmatter` | YAML or TOML frontmatter written as `export const meta` |

Without a feature, asking for what it adds is an `Error::Feature`.

## What to reach for

| You want to | Reach for |
| --- | --- |
| Turn a file into the module the build lowers | `convert` with `Target::Lowered` |
| Turn a file into the module the browser runs | `convert` with `Target::Browser` and `Options::provider` |
| Place an application's components for markdown's elements | `Options::mapped` |
| Name a source line from a byte of the written module | `Converted::origin`, `original_offset`, `position` |
| List a file's headings | `Converted::toc` |
| Show a file's first paragraph in a list | `Converted::excerpt` |
| Tell whether a file exports `meta` | `Converted::meta`, `Converted::exports` |
| Keep highlight classes out of a class list | `Converted::classes` |
| Colour highlighted code | `highlight::theme_css` |

## Status

Parses with markdown-rs (`markdown` 1.0) and checks ESM and expressions with swc. CommonMark, GFM (tables, strikethrough, task lists, footnotes, literal autolinks), heading ids as `rehype-slug` writes them, a `toc` export, an `export default` layout, `wrapper` and code fences with `title="..."`, `{1,3-5}` and `showLineNumbers` as `rehype-pretty-code` reads them. Remark and rehype plugins are JavaScript and are not run. Raw HTML in a `.md` file is left out with a warning, as MDX's `md` format leaves it out. Math is not parsed.
