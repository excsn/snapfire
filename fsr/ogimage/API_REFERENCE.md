# API Reference: snapfire_fsr_ogimage

Renders a card, a tree of boxes, text and images styled with a subset of CSS, to PNG or JPEG.

## Contents

* [1. The Card](#1-the-card)
  * [Card](#card)
  * [Element](#element)
  * [Run](#run)
  * [ImageRef](#imageref)
  * [Declarations](#declarations)
* [2. Resources](#2-resources)
  * [Fonts](#fonts)
  * [FaceInfo](#faceinfo)
  * [Images](#images)
* [3. Rendering](#3-rendering)
  * [render](#render)
  * [Format](#format)
  * [Rendered](#rendered)
  * [Warning](#warning)
  * [MAX_SIDE](#max_side)
* [4. Error Handling](#4-error-handling)
  * [CardError](#carderror)
  * [FontError](#fonterror)

## 1. The Card

### Card

`pub struct Card { pub width: u32, pub height: u32, pub root: Element }`.

* `fn new(width: u32, height: u32, root: Element) -> Card`.
* Each side is 1 to `MAX_SIDE` pixels. The root's `width` and `height` default to the card's.

### Element

`#[non_exhaustive] pub enum Element`. `tag` names the element in a warning or an error only.

* `Box { tag: String, style: Declarations, children: Vec<Element> }`: laid out with flexbox.
* `Text { tag: String, style: Declarations, runs: Vec<Run> }`: one paragraph.
* `Image { tag: String, style: Declarations, src: ImageRef, width: Option<f32>, height: Option<f32> }`: `width` and `height` are the attributes, in pixels.
* `fn boxed(tag: impl Into<String>) -> Element`: an empty box.
* `fn text(tag: impl Into<String>, text: impl Into<String>) -> Element`: a text element with one unstyled run.
* `fn image(tag: impl Into<String>, src: ImageRef) -> Element`.
* `fn with(self, property: impl Into<String>, value: impl Into<String>) -> Element`: appends a declaration.
* `fn child(self, child: Element) -> Element`: appends to a box; does nothing on the other kinds.
* `fn run(self, run: Run) -> Element`: appends to a text element; does nothing on the other kinds.
* `fn size(self, width: f32, height: f32) -> Element`: sets an image's attributes; does nothing on the other kinds.
* `fn tag(&self) -> &str`, `fn style(&self) -> &Declarations`.
* A tree nests at most 64 levels.

### Run

`pub struct Run { pub style: Declarations, pub text: String }`. Inherits its text element's properties.

* `fn new(text: impl Into<String>) -> Run`.
* `fn with(self, property: impl Into<String>, value: impl Into<String>) -> Run`.

### ImageRef

`pub enum ImageRef`.

* `Key(String)`: an image given to `Images`. Nothing is fetched.
* `Bytes(Vec<u8>)`: PNG or JPEG bytes, decoded on each render.

### Declarations

`pub type Declarations = Vec<(String, String)>`. A property and its value as CSS writes them, in source order. Names are matched without regard to case; `!important` is ignored; custom properties (`--*`) are skipped.

## 2. Resources

### Fonts

The faces text is drawn with. `Clone`, `Default`, `Debug`. Clones share one glyph raster cache.

* `fn new() -> Fonts`: the bundled Inter at weights 400 and 700, cut to Latin.
* `fn add(&mut self, bytes: impl Into<Vec<u8>>) -> Result<Vec<FaceInfo>, FontError>`: every face of a TTF, OTF, TTC or WOFF2 file, after the faces already added and before Inter.
* `fn add_for_scripts(&mut self, bytes: impl Into<Vec<u8>>, scripts: &[&str]) -> Result<Vec<FaceInfo>, FontError>`: the same, as the faces for text in `scripts`, by Unicode full name or four-letter code.
* `fn faces(&self) -> Vec<FaceInfo>`: every face in fallback order.
* Fallback for a character: the element's `font-family` families, a face added for its script, every other family in the order added, then Inter. Within a family the closest weight and style by CSS font matching.
* The raster cache holds at most 8192 glyphs and starts over past that.

### FaceInfo

`pub struct FaceInfo { pub family: String, pub weight: u16, pub italic: bool, pub scripts: Vec<String>, pub bundled: bool }`. `family` is the typographic family name, else the family name. `italic` is true for an oblique face too.

### Images

Decoded images by key. `Clone`, `Default`, `Debug`.

* `fn new() -> Images`.
* `fn insert(&mut self, key: impl Into<String>, bytes: &[u8]) -> Result<(), CardError>`: decodes PNG or JPEG, replacing an image under the same key. SVG is refused.
* `fn contains(&self, key: &str) -> bool`.
* `fn size(&self, key: &str) -> Option<(u32, u32)>`: the decoded width and height.

## 3. Rendering

### render

`pub fn render(card: &Card, fonts: &Fonts, images: &Images, format: Format) -> Result<Rendered, CardError>`.

* Reads its arguments only; safe to call from several threads with the same `Fonts` and `Images`.
* A property outside the subset, a value the subset leaves out and a character no face covers are warnings.
* A value that does not parse, an image key `images` lacks, bytes that do not decode, a bad size and a tree past 64 levels are errors.
* PNG deflate runs on up to eight scoped threads. The bytes depend only on the card, never on the machine.

### Format

`pub enum Format`. `Copy`, `PartialEq`.

* `Png`: lossless, RGB when every pixel is opaque, else RGBA.
* `Jpeg { quality: u8 }`: `quality` clamped to 1 to 100; transparent pixels are drawn over white.
* `fn content_type(self) -> &'static str`: `image/png` or `image/jpeg`.

### Rendered

`pub struct Rendered { pub bytes: Vec<u8>, pub warnings: Vec<Warning> }`.

### Warning

`pub struct Warning { pub element: String, pub message: String }`. `element` is the path of tags from the root, each with its 1-based position among its siblings: `div(1) > div(2) > span(1)`; a run is `run(n)`. `impl Display`: `element: message`.

### MAX_SIDE

`pub const MAX_SIDE: u32 = 8192`.

## 4. Error Handling

### CardError

`#[non_exhaustive] pub enum CardError`. `thiserror`, `PartialEq`.

* `Size { width: u32, height: u32 }`: a side is 0 or past `MAX_SIDE`.
* `Value { element: String, property: String, value: String, reason: String }`: a declaration's value does not parse. The first one in the tree is reported.
* `MissingImage { element: String, key: String }`: an `ImageRef::Key` or `url(key)` that `Images` does not hold.
* `Image { key: String, reason: String }`: bytes that are not a PNG or a JPEG or do not decode. For `ImageRef::Bytes` the key names the element.
* `Depth { element: String }`: the tree nests deeper than 64 levels.
* `Layout(String)`: taffy refused the tree.
* `Encode(String)`: writing the PNG or JPEG failed.

### FontError

`#[non_exhaustive] pub enum FontError`. `thiserror`, `PartialEq`.

* `Woff2(String)`: the WOFF2 file does not decode.
* `Woff1`: WOFF 1 is not read.
* `NotAFont`: the bytes are not TrueType or OpenType.
* `UnknownScript(String)`: a script name Unicode does not have.
