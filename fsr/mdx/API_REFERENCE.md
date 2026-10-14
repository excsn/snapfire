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

`pub fn convert(source: &str, options: &Options) -> Result<Converted, Error>`

Parses `source` with markdown-rs in the format `options.format` names and writes the module `options.target` names. An ESM block may hold only imports and exports; an expression must be a whole expression, a comment or empty.

### Options

* `pub fn new(format: Format, target: Target) -> Options`: `gfm` and `heading_ids` on, `frontmatter` and `highlight` off, `ClassAttr::ClassName`, nothing mapped, no provider.
* `pub format: Format`
* `pub target: Target`
* `pub gfm: bool`: tables, strikethrough, task lists, footnotes and literal autolinks.
* `pub heading_ids: bool`: an `id` on every heading whose text is not empty, plus the `toc` export.
* `pub frontmatter: bool`: YAML (`---`) or TOML (`+++`) frontmatter as `export const meta`. Needs the `frontmatter` feature.
* `pub highlight: bool`: fenced code whose language names a grammar written as classed spans. Needs the `highlight` feature.
* `pub class_attr: ClassAttr`
* `pub mapped: Vec<Mapped>`: read for `Target::Lowered` only.
* `pub provider: Option<String>`: read for `Target::Browser` only; the specifier `useMDXComponents` is imported from.

### Format

* `Format::Mdx`, `Format::Md`
* `pub fn of(path: &str) -> Option<Format>`: by extension, `.mdx` or `.md`, any case.

### Target

* `Target::Lowered`: mapped elements placed as `Mdx_<key>`, imported by name; the wrapper takes the content as its children and no props.
* `Target::Browser`: every element written as `_components.<tag>`, the object filled with each tag's own name, then `useMDXComponents()` when there is a provider, then `props.components`.

### ClassAttr

* `ClassAttr::ClassName`, `ClassAttr::Class`: the attribute a class the conversion writes takes.

### Mapped

`#[derive(Debug, Clone, PartialEq, Eq)]`

* `pub key: String`: a tag in `ELEMENTS` or `WRAPPER`.
* `pub specifier: String`: imported as written.
* `pub export: String`

### Constants

* `pub const ELEMENTS: &[&str]`: the tags markdown constructs write: `a`, `blockquote`, `br`, `code`, `del`, `em`, `h1` to `h6`, `hr`, `img`, `input`, `li`, `ol`, `p`, `pre`, `section`, `strong`, `sup`, `table`, `tbody`, `td`, `th`, `thead`, `tr`, `ul`.
* `pub const WRAPPER: &str = "wrapper"`
* `pub const CONTENT: &str = "MDXContent"`: the default export's name.

## 2. What a Conversion Returns

### Converted

`#[derive(Debug, Clone)]`

* `pub tsx: String`
* `pub segments: Vec<Segment>`: in order of `generated`.
* `pub toc: Vec<TocEntry>`: empty with `heading_ids` off.
* `pub excerpt: Option<String>`: the plain text of the first top-level paragraph, whitespace collapsed; `None` when there is none or it holds no text. Frontmatter, headings and ESM are not paragraphs.
* `pub meta: bool`: the module exports `meta`.
* `pub exports: Vec<String>`: every exported name but `default`, `toc` last when written.
* `pub classes: BTreeSet<String>`: each class the highlighter wrote, `hl-code` among them.
* `pub placed: BTreeSet<String>`: the keys of `Options::mapped` the module imports.
* `pub warnings: Vec<Warning>`
* `pub fn origin(&self, source: &str, offset: usize) -> (usize, usize)`: [`position`](#position) of [`original_offset`](#original_offset).

### Segment

`#[derive(Debug, Clone, Copy, PartialEq, Eq)]`

* `pub generated: usize`: byte offset in `tsx`.
* `pub len: usize`
* `pub original: usize`: byte offset in the source.
* `pub verbatim: bool`: the run is source text; each byte maps to the byte at the same distance from `original`. Otherwise the run maps whole to `original`.

### TocEntry

`#[derive(Debug, Clone, PartialEq, Eq)]`

* `pub depth: u8`: 1 to 6.
* `pub id: String`
* `pub text: String`: the heading's text, inline code and image alt text included.

Written as `export const toc = [{ depth, id, text }, ...]`.

### Warning

`#[derive(Debug, Clone, PartialEq, Eq)]`

* `pub line: usize`, `pub column: usize`: one-based, the column in characters.
* `pub message: String`

### original_offset

`pub fn original_offset(segments: &[Segment], offset: usize) -> usize`

The source byte that written byte `offset` came from; 0 before the first segment.

### position

`pub fn position(source: &str, offset: usize) -> (usize, usize)`

The one-based line and column, in characters, of byte `offset`, clamped to the source's length.

## 3. Highlighting

The `highlight` feature. Grammars are bat's (two-face 0.5) on fancy-regex; `jsx` reads with the TSX grammar; `text`, `txt`, `plain` and `plaintext` name none. `SESSIONS`, `shell-session`, `console`, `sh-session` and `shellsession` in any case, read a terminal session: prompt lines `hl-meta hl-prompt` with the command through Bash, other lines `hl-meta hl-output`.

### highlight::lines

`pub fn lines(code: &str, lang: &str) -> Option<Vec<Vec<Piece>>>`

One tree per line, every scope open at the line's start reopened, the newline left out. `None` when `lang` names no grammar.

### highlight::knows

`pub fn knows(lang: &str) -> bool`

### highlight::Piece

`#[derive(Debug, Clone, PartialEq, Eq)]`

* `Piece::Text(String)`
* `Piece::Span { classes: String, children: Vec<Piece> }`: `classes` is the scope's atoms, each prefixed with `PREFIX`, space separated.

Constants: `pub const PREFIX: &str = "hl-"`, `pub const CODE_CLASS: &str = "hl-code"`.

### highlight::theme_css

`pub fn theme_css(name: &str, scope: &str) -> Option<String>`

syntect's stylesheet for theme `name` with `hl-` classes, each rule's selectors prefixed with `scope` and a space when `scope` is not empty. `None` for a name two-face does not carry.

### highlight::themes

`pub fn themes() -> Vec<&'static str>`

The theme names offered for `theme_css`, light and dark ones among them.

## 4. Error Handling

### Error

`#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]`. `Display` is the message.

* `Error::Syntax { line: usize, column: usize, message: String }`: `{line}:{column}: {message}`. Any of: the file does not parse, an ESM block holds a statement, a second `export default`, frontmatter that does not read and frontmatter beside its own `export const meta`.
* `Error::Feature(&'static str, &'static str)`: what was asked for and the feature it needs.
