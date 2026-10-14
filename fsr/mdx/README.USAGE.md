# Usage Guide: snapfire_fsr_mdx

How to turn `.mdx` and `.md` files into TSX modules, map what was written back to the source, place components for markdown's elements and highlight code.

## Table of Contents

* [Core Concepts](#core-concepts)
* [Quick Start](#quick-start)
* [Converting a File](#converting-a-file)
  * [Choosing the format](#choosing-the-format)
  * [Choosing the target](#choosing-the-target)
* [Naming the Source Line](#naming-the-source-line)
* [Placing Components for Elements](#placing-components-for-elements)
  * [For the lowered module](#for-the-lowered-module)
  * [For the browser](#for-the-browser)
  * [Wrapping the content](#wrapping-the-content)
* [Reading What a File Exports](#reading-what-a-file-exports)
* [Turning Constructs On and Off](#turning-constructs-on-and-off)
  * [GFM](#gfm)
  * [Heading ids](#heading-ids)
  * [Frontmatter](#frontmatter)
* [Highlighting Code](#highlighting-code)
  * [Fence options](#fence-options)
  * [Colouring with a theme](#colouring-with-a-theme)
* [Error Handling](#error-handling)

## Core Concepts

* **Format**: `Format::Mdx` reads markdown with JSX, `{expressions}` and ESM; `Format::Md` reads markdown alone, as MDX's `md` format does.
* **Target**: which module `convert` writes. `Target::Lowered` is for a build that lowers TSX, `Target::Browser` for a bundle the browser runs.
* **Segment**: a run of the written module and the source byte it came from; a verbatim segment maps byte for byte.
* **Origin**: the one-based line and column in the source of a byte of the written module.
* **Mapped element**: a tag a markdown construct writes (`h2`, `a`, `pre`) that the lowered module places as a named component instead.
* **Provider**: the module whose `useMDXComponents` the browser module calls for its map.
* **Wrapper**: the component that takes the whole content as its children: the file's own `export default`, else the map's `wrapper`.
* **TOC**: the file's headings with their ids, exported as `toc`.
* **Meta**: the file's `export const meta`, written or from frontmatter.
* **Highlighted code**: a fenced block written as nested spans whose classes are the TextMate scopes of its tokens, `hl-` prefixed.
* **Theme CSS**: a stylesheet colouring those classes in one syntect theme.
* **Warning**: something the conversion left out, with the line and column it sits at.

## Quick Start

```rust
use snapfire_fsr_mdx::{convert, Format, Options, Target};

fn main() {
  let source = "export const meta = { title: \"Hello\" }\n\n# {meta.title}\n\nSome *text* and {1 + 1}.\n";
  let options = Options::new(Format::Mdx, Target::Lowered);
  let converted = convert(source, &options).expect("a valid file");
  println!("{}", converted.tsx);
  assert!(converted.meta);
  assert_eq!(converted.exports, ["meta", "toc"]);
}
```

The module it prints:

```tsx
export const meta = { title: "Hello" }

export default function MDXContent(props) {
  return <>
<h1>{meta.title
}</h1>
<p>{"Some "}<em>{"text"}</em>{" and "}{1 + 1
}{"."}</p>
</>;
}

export const toc = [];
```

## Converting a File

### Choosing the format

`Format::of` reads the extension:

```rust
use snapfire_fsr_mdx::Format;

assert_eq!(Format::of("content/post.mdx"), Some(Format::Mdx));
assert_eq!(Format::of("routes/faq/page.md"), Some(Format::Md));
assert_eq!(Format::of("routes/page.tsx"), None);
```

In `Format::Md`, `<div>` and `{braces}` are text. Raw HTML is left out with a warning:

```rust
use snapfire_fsr_mdx::{convert, Format, Options, Target};

let converted = convert("# A\n\n<div>raw</div>\n", &Options::new(Format::Md, Target::Lowered)).unwrap();
assert_eq!((converted.warnings[0].line, converted.warnings[0].column), (3, 1));
```

### Choosing the target

The lowered module places mapped components by name and never spreads the page's props. The browser module reads the map at render time, as MDX's own output does:

```rust
use snapfire_fsr_mdx::{convert, Format, Options, Target};

let mut options = Options::new(Format::Mdx, Target::Browser);
options.provider = Some("@app/mdx-components".into());
let converted = convert("# Hi\n", &options).unwrap();
assert!(converted.tsx.contains("import { useMDXComponents as _provideComponents } from \"@app/mdx-components\";"));
assert!(converted.tsx.contains("const _components = { h1: \"h1\", ..._provideComponents(), ...props.components };"));
```

`Options::class_attr` picks `className` or `class` for the classes the conversion writes:

```rust
use snapfire_fsr_mdx::{ClassAttr, Format, Options, Target};

let mut options = Options::new(Format::Mdx, Target::Lowered);
options.class_attr = ClassAttr::Class;
```

## Naming the Source Line

`Converted::origin` maps a byte of `tsx` to the line and column of the source. A diagnostic against the written module names the line the author wrote:

```rust
use snapfire_fsr_mdx::{convert, Format, Options, Target};

let source = "# Title\n\nText {value.missing}\n";
let converted = convert(source, &Options::new(Format::Mdx, Target::Lowered)).unwrap();
let at = converted.tsx.find("value.missing").unwrap();
assert_eq!(converted.origin(source, at), (3, 7));
```

The segments are public for a caller that keeps its own map:

```rust
use snapfire_fsr_mdx::{original_offset, position};

let offset = original_offset(&converted.segments, at);
assert_eq!(position(source, offset), (3, 7));
```

## Placing Components for Elements

### For the lowered module

Each `Mapped` names a key and the component to import for it. Only the keys the file uses are imported:

```rust
use snapfire_fsr_mdx::{convert, Format, Mapped, Options, Target};

let mut options = Options::new(Format::Mdx, Target::Lowered);
options.mapped = vec![Mapped { key: "h2".into(), specifier: "@app/mdx-components".into(), export: "__mdx_h2".into() }];
let converted = convert("## Setup\n", &options).unwrap();
assert!(converted.tsx.contains("import { __mdx_h2 as Mdx_h2 } from \"@app/mdx-components\";"));
assert!(converted.tsx.contains("<Mdx_h2 id={\"setup\"}>"));
assert!(converted.placed.contains("h2"));
```

`ELEMENTS` lists the tags a markdown construct writes. A JSX tag written in the file is never mapped.

### For the browser

The browser module routes every element through `_components`, filled from `useMDXComponents()` and then from the `components` prop:

```tsx
const _components = { h2: "h2", p: "p", ..._provideComponents(), ...props.components };
return <><_components.h2 id={"setup"}>{"Setup"}</_components.h2></>;
```

### Wrapping the content

A file's own `export default` is its layout. A declared function keeps its name; anything else is bound to `MDXLayout`:

```rust
use snapfire_fsr_mdx::{convert, Format, Options, Target};

let converted = convert("export default function Layout({ children }) { return <main>{children}</main> }\n\nHi\n", &Options::new(Format::Mdx, Target::Lowered)).unwrap();
assert!(converted.tsx.contains("return <Layout><>"));
```

Without one, a mapped `wrapper` key wraps the content. Either takes the content as `children` and no other props.

## Reading What a File Exports

```rust
use snapfire_fsr_mdx::{convert, Format, Options, Target, TocEntry};

let converted = convert("export const meta = { title: \"T\" }\n\n# Intro\n\n## Setup\n", &Options::new(Format::Mdx, Target::Lowered)).unwrap();
assert!(converted.meta);
assert_eq!(converted.toc[1], TocEntry { depth: 2, id: "setup".into(), text: "Setup".into() });
assert_eq!(converted.exports, ["meta", "toc"]);
```

## Turning Constructs On and Off

### GFM

On by default: tables, strikethrough, task lists, footnotes and literal autolinks, written as `remark-gfm` and `mdast-util-to-hast` write them.

```rust
let mut options = Options::new(Format::Mdx, Target::Lowered);
options.gfm = false;
```

### Heading ids

On by default: an `id` on every heading as `github-slugger` makes it with `-1` and `-2` for a repeat, plus the `toc` export. A heading whose text is only an expression gets no id.

```rust
let mut options = Options::new(Format::Mdx, Target::Lowered);
options.heading_ids = false;
```

### Frontmatter

Off by default. It needs the `frontmatter` feature. A YAML or TOML block becomes `export const meta`; a file with both a block and its own `export const meta` is refused.

```rust
let mut options = Options::new(Format::Md, Target::Lowered);
options.frontmatter = true;
let converted = convert("---\ntitle: Hello\n---\n\n# Hi\n", &options).unwrap();
assert!(converted.tsx.starts_with("export const meta = {\"title\":\"Hello\"};"));
```

## Highlighting Code

Needs the `highlight` feature. A fence whose language names a grammar is written as one `<span data-line>` per line inside `<figure data-rehype-pretty-code-figure>`; any other fence stays `<pre><code className="language-x">`.

```rust
let mut options = Options::new(Format::Mdx, Target::Lowered);
options.highlight = true;
let converted = convert("```ts\nconst a = 1;\n```\n", &options).unwrap();
assert!(converted.tsx.contains("<pre className={\"hl-code\"} data-language={\"ts\"}>"));
assert!(converted.classes.contains("hl-storage"));
```

`highlight::lines` and `highlight::knows` reach the highlighter directly:

```rust
use snapfire_fsr_mdx::highlight;

assert!(highlight::knows("tsx"));
let lines = highlight::lines("let x = 1;\n", "ts").unwrap();
assert_eq!(lines.len(), 1);
```

### Fence options

~~~text
```ts title="routes/page.loader.ts" {2,4-5} showLineNumbers
~~~

`title` writes a `<figcaption data-rehype-pretty-code-title>`, each listed line carries `data-highlighted-line` and `showLineNumbers` puts `data-line-numbers` on the `<code>`.

### Colouring with a theme

```rust
use snapfire_fsr_mdx::highlight::{theme_css, themes};

let light = theme_css("GitHub", "").unwrap();
let dark = theme_css("OneHalfDark", "[data-sf-scheme=\"dark\"]").unwrap();
assert!(dark.contains("[data-sf-scheme=\"dark\"] .hl-code {"));
assert!(themes().contains(&"Nord"));
```

## Error Handling

`convert` returns `Error`:

```rust
use snapfire_fsr_mdx::{convert, Error, Format, Options, Target};

match convert("export const a = 1\nconsole.log(a)\n", &Options::new(Format::Mdx, Target::Lowered)) {
  Err(Error::Syntax { line, column, message }) => eprintln!("{line}:{column}: {message}"),
  Err(Error::Feature(what, feature)) => eprintln!("{what} needs the `{feature}` feature"),
  Ok(converted) => println!("{}", converted.tsx),
}
```

`Error::Syntax` is a file that does not parse: markdown-rs's own message, an ESM block holding a statement that is not an import or an export, a second `export default` or frontmatter that does not read. `Error::Feature` is an option the crate was built without.
