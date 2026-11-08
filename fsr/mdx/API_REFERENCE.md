# API Reference: snapfire_fsr_mdx

MDX and markdown converted to TSX modules for the SnapFire FSR build.

## Contents

* [1. Converting](#1-converting)
  * [convert](#convert)
  * [Options](#options)
  * [Format](#format)
  * [Target](#target)
  * [ClassAttr](#classattr)
  * [Mapped](#mapped)
  * [Constants](#constants)
* [2. What a Conversion Returns](#2-what-a-conversion-returns)
  * [Converted](#converted)
  * [Segment](#segment)
  * [TocEntry](#tocentry)
  * [Warning](#warning)
  * [original_offset](#original_offset)
  * [position](#position)
* [3. Highlighting](#3-highlighting)
  * [highlight::lines](#highlightlines)
  * [highlight::knows](#highlightknows)
  * [highlight::Piece](#highlightpiece)
  * [highlight::theme_css](#highlighttheme_css)
  * [highlight::themes](#highlightthemes)
* [4. Error Handling](#4-error-handling)
  * [Error](#error)

## 1. Converting

### convert

* `pub fn convert(source: &str, options: &Options) -> Result<Converted, Error>`

Converts `source`, read in the format `options.format` names, into the module `options.target` names.

* An ESM block may hold only imports and exports.
* An expression must be a whole expression, a comment or empty.
* `Error::Feature` when `options.highlight` or `options.frontmatter` is set without its feature.

### Options

What `convert` reads and writes. `Debug`, `Clone`, `#[non_exhaustive]`.

* `pub fn new(format: Format, target: Target) -> Options`: `gfm` and `heading_ids` on, `frontmatter` and `highlight` off, `ClassAttr::ClassName`, nothing mapped, no provider.
* `pub format: Format`
* `pub target: Target`
* `pub gfm: bool`: tables, strikethrough, task lists, footnotes and literal autolinks.
* `pub heading_ids: bool`: an `id` on every heading whose text is not empty, plus the `toc` export.
* `pub frontmatter: bool`: YAML (`---`) or TOML (`+++`) frontmatter as `export const meta`. Needs the `frontmatter` feature.
* `pub highlight: bool`: fenced code whose language names a grammar written as classed spans. Needs the `highlight` feature.
* `pub class_attr: ClassAttr`
* `pub mapped: Vec<Mapped>`: read for `Target::Lowered` only.
* `pub provider: Option<String>`: read for `Target::Browser` only. The specifier `useMDXComponents` is imported from, relative to the file.

### Format

What a file is written in. `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`.

* `Format::Mdx`: markdown with JSX, expressions and ESM.
* `Format::Md`: markdown alone, MDX's `md` format.
* `pub fn of(path: &str) -> Option<Format>`: by extension, `.mdx` or `.md`, any case.

### Target

Which module `convert` writes. `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`.

* `Target::Lowered`: mapped elements placed as `Mdx_<key>`, imported by name. The wrapper takes the content as its children and no props.
* `Target::Browser`: every element written as `_components.<tag>`, the object filled with each tag's own name, then `useMDXComponents()` when there is a provider, then `props.components`.

### ClassAttr

The attribute a written class goes in. `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `#[non_exhaustive]`.

* `ClassAttr::ClassName`: `className`.
* `ClassAttr::Class`: `class`.

### Mapped

One element the `Target::Lowered` module places as a component. `Debug`, `Clone`, `PartialEq`, `Eq`, `#[non_exhaustive]`.

* `pub fn new(key: String, specifier: String, export: String) -> Self`
* `pub key: String`: a tag in `ELEMENTS` or `WRAPPER`.
* `pub specifier: String`: imported as written, relative to the file.
* `pub export: String`

### Constants

* `pub const ELEMENTS: &[&str]`: the tags markdown constructs write: `a`, `blockquote`, `br`, `code`, `del`, `em`, `h1` to `h6`, `hr`, `img`, `input`, `li`, `ol`, `p`, `pre`, `section`, `strong`, `sup`, `table`, `tbody`, `td`, `th`, `thead`, `tr`, `ul`.
* `pub const WRAPPER: &str = "wrapper"`
* `pub const CONTENT: &str = "MDXContent"`: the default export's name.

## 2. What a Conversion Returns

### Converted

The written module and what was read from the source. `Debug`, `Clone`, `#[non_exhaustive]`.

* `pub tsx: String`
* `pub segments: Vec<Segment>`: in order of `generated`.
* `pub toc: Vec<TocEntry>`: empty with `heading_ids` off.
* `pub excerpt: Option<String>`: the plain text of the first top-level paragraph that has any, whitespace collapsed. An expression or JSX inside it adds nothing. Frontmatter, headings and ESM are not paragraphs.
* `pub meta: bool`: the module exports `meta`, written or from frontmatter.
* `pub exports: Vec<String>`: every exported name but `default`, `toc` last when written.
* `pub classes: BTreeSet<String>`: each class the highlighter wrote, `hl-code` among them.
* `pub placed: BTreeSet<String>`: the keys of `Options::mapped` the module imports.
* `pub warnings: Vec<Warning>`
* `pub fn origin(&self, source: &str, offset: usize) -> (usize, usize)`: [`position`](#position) of [`original_offset`](#original_offset).

### Segment

A run of the written module and where it came from. `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `#[non_exhaustive]`.

* `pub generated: usize`: byte offset in `tsx`.
* `pub len: usize`
* `pub original: usize`: byte offset in the source.
* `pub verbatim: bool`: the run is source text and each byte maps to the byte at the same distance from `original`. Otherwise the run maps whole to `original`.

### TocEntry

One heading of the `toc` export. `Debug`, `Clone`, `PartialEq`, `Eq`, `#[non_exhaustive]`.

* `pub depth: u8`: 1 to 6.
* `pub id: String`
* `pub text: String`: the heading's text, inline code and image alt text included.

Written as `export const toc = [{ depth, id, text }, ...]`.

### Warning

A construct `convert` left out. `Debug`, `Clone`, `PartialEq`, `Eq`, `#[non_exhaustive]`.

* `pub line: usize`, `pub column: usize`: one-based, the column in characters.
* `pub message: String`

### original_offset

* `pub fn original_offset(segments: &[Segment], offset: usize) -> usize`: the source byte that written byte `offset` came from. 0 before the first segment.

### position

* `pub fn position(source: &str, offset: usize) -> (usize, usize)`: the one-based line and column, in characters, of byte `offset`, clamped to the source's length.

## 3. Highlighting

Module `highlight`, behind the `highlight` feature. Module constants: `PREFIX: &str = "hl-"`, `CODE_CLASS: &str = "hl-code"`, `SESSIONS: &[&str] = &["shell-session", "console", "sh-session", "shellsession"]`.

* A language is matched in any case. `jsx` reads with the TSX grammar. `text`, `txt`, `plain` and `plaintext` name no grammar.
* A language in `SESSIONS` reads as a terminal session. Prompt lines are `hl-meta hl-prompt` with the command highlighted as Bash. Other lines are `hl-meta hl-output`.

### highlight::lines

* `pub fn lines(code: &str, lang: &str) -> Option<Vec<Vec<Piece>>>`: one tree per line, with every scope open at the line's start reopened and the newline left out. `None` when `lang` names no grammar.

### highlight::knows

* `pub fn knows(lang: &str) -> bool`: whether `lang` names a grammar or is in `SESSIONS`.

### highlight::Piece

One node of a highlighted line. `Debug`, `Clone`, `PartialEq`, `Eq`, `#[non_exhaustive]`.

* `Piece::Text(String)`
* `Piece::Span { classes: String, children: Vec<Piece> }`: `classes` is the scope's atoms, each prefixed with `PREFIX`, space separated.

### highlight::theme_css

* `pub fn theme_css(name: &str, scope: &str) -> Option<String>`: the stylesheet for theme `name` over `hl-` classes. Each rule's selectors are prefixed with `scope` and a space when `scope` is not empty. `None` when no theme has that name.

### highlight::themes

* `pub fn themes() -> Vec<&'static str>`: the 17 theme names `theme_css` reads.

## 4. Error Handling

### Error

`Debug`, `Clone`, `PartialEq`, `Eq`, `thiserror::Error`, `#[non_exhaustive]`.

* `Error::Syntax { line: usize, column: usize, message: String }`: `{line}:{column}: {message}`. Any of: the file does not parse, an ESM block holds a statement, a second `export default`, frontmatter that does not read and frontmatter beside its own `export const meta`.
* `Error::Feature(&'static str, &'static str)`: what was asked for and the feature it needs. Displays as ``{0} needs snapfire_fsr_mdx's `{1}` feature``.
