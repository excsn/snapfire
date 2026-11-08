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

A card's size and its root element. `Debug`, `Clone`, `PartialEq`, `#[non_exhaustive]`.

* `pub struct Card { pub width: u32, pub height: u32, pub root: Element }`
* `pub fn new(width: u32, height: u32, root: Element) -> Card`
* Each side is 1 to `MAX_SIDE` pixels. The root's `width` and `height` default to the card's.

### Element

One node of a card. `Debug`, `Clone`, `PartialEq`, `#[non_exhaustive]`. `tag` names the element in a warning or an error only.

* `Box { tag: String, style: Declarations, children: Vec<Element> }`: laid out with flexbox.
* `Text { tag: String, style: Declarations, runs: Vec<Run> }`: one paragraph.
* `Image { tag: String, style: Declarations, src: ImageRef, width: Option<f32>, height: Option<f32> }`: `width` and `height` are the attributes, in pixels.
* `pub fn boxed(tag: impl Into<String>) -> Element`: an empty box.
* `pub fn text(tag: impl Into<String>, text: impl Into<String>) -> Element`: a text element with one unstyled run.
* `pub fn image(tag: impl Into<String>, src: ImageRef) -> Element`
* `pub fn with(self, property: impl Into<String>, value: impl Into<String>) -> Element`: appends a declaration.
* `pub fn child(self, child: Element) -> Element`: appends to a box. Does nothing on the other kinds.
* `pub fn run(self, run: Run) -> Element`: appends to a text element. Does nothing on the other kinds.
* `pub fn size(self, w: f32, h: f32) -> Element`: sets an image's `width` and `height`. Does nothing on the other kinds.
* `pub fn tag(&self) -> &str`
* `pub fn style(&self) -> &Declarations`
* A tree nests at most 64 levels.

### Run

A span of text with its own style, inheriting its text element's properties. `Debug`, `Clone`, `PartialEq`.

* `pub struct Run { pub style: Declarations, pub text: String }`
* `pub fn new(text: impl Into<String>) -> Run`
* `pub fn with(self, property: impl Into<String>, value: impl Into<String>) -> Run`: appends a declaration.

### ImageRef

Where an image element's pixels come from. `Debug`, `Clone`, `PartialEq`.

* `Key(String)`: an image given to `Images`. Nothing is fetched.
* `Bytes(Vec<u8>)`: PNG or JPEG bytes, decoded on each render.

### Declarations

CSS declarations in source order.

* `pub type Declarations = Vec<(String, String)>`: each a property and its value as CSS writes them.
* Property names are matched in any case. `!important` is ignored. Custom properties (`--*`) are skipped.

## 2. Resources

### Fonts

The faces text is drawn with. `Clone`, `Default`, `Debug`. Clones share one glyph raster cache.

* `pub fn new() -> Fonts`: the bundled Inter at weights 400 and 700, cut to Latin. `Default` is the same.
* `pub fn add(&mut self, bytes: impl Into<Vec<u8>>) -> Result<Vec<FaceInfo>, FontError>`: adds every face of a TTF, OTF, TTC or WOFF2 file in file order, after the faces already added and before Inter.
* `pub fn add_for_scripts(&mut self, bytes: impl Into<Vec<u8>>, scripts: &[&str]) -> Result<Vec<FaceInfo>, FontError>`: the same, as the faces for text in `scripts`, by Unicode full name or four-letter code.
* `pub fn faces(&self) -> Vec<FaceInfo>`: every face in fallback order.
* A character is drawn with the first face that covers it from: the element's `font-family` families, a face added for its script, every other family in the order added, then Inter.
* Within a family the face is the closest weight and style by CSS font matching.
* The raster cache holds at most 8192 glyphs and is emptied when full.

### FaceInfo

One face `Fonts` holds. `Debug`, `Clone`, `PartialEq`, `Eq`, `#[non_exhaustive]`.

* `pub struct FaceInfo { pub family: String, pub weight: u16, pub italic: bool, pub scripts: Vec<String>, pub bundled: bool }`
* `family` is the typographic family name, else the family name.
* `italic` is true for an oblique face too.

### Images

Decoded images by key. `Clone`, `Default`, `Debug`.

* `pub fn new() -> Images`
* `pub fn insert(&mut self, key: impl Into<String>, bytes: &[u8]) -> Result<(), CardError>`: decodes PNG or JPEG and replaces any image under the same key. SVG is refused.
* `pub fn contains(&self, key: &str) -> bool`
* `pub fn size(&self, key: &str) -> Option<(u32, u32)>`: the decoded width and height.

## 3. Rendering

### render

* `pub fn render(card: &Card, fonts: &Fonts, images: &Images, format: Format) -> Result<Rendered, CardError>`: lays out, draws and encodes `card`.
* Reads its arguments only. It can be called from several threads with the same `Fonts` and `Images`.
* A property outside the subset, a value the subset leaves out and a character no face covers are warnings.
* A value that does not parse, an image key `images` lacks, bytes that do not decode, a bad size and a tree past 64 levels are errors.
* The bytes depend only on the arguments, never on the machine.

### Format

The encoding `render` writes. `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `#[non_exhaustive]`.

* `Png`: lossless, RGB when every pixel is opaque, else RGBA.
* `Jpeg { quality: u8 }`: `quality` clamped to 1 to 100. Transparent pixels are drawn over white.
* `pub fn content_type(self) -> &'static str`: `image/png` or `image/jpeg`.

### Rendered

An encoded card and what was skipped. `Debug`, `Clone`, `PartialEq`, `#[non_exhaustive]`.

* `pub struct Rendered { pub bytes: Vec<u8>, pub warnings: Vec<Warning> }`

### Warning

Something the renderer skipped or could not draw. `Debug`, `Clone`, `PartialEq`, `Eq`, `#[non_exhaustive]`.

* `pub struct Warning { pub element: String, pub message: String }`
* `element` is the path of tags from the root, each with its 1-based position among its siblings, such as `div(1) > div(2) > span(1)`. A run is `run(n)`.
* `impl Display`: `element: message`.

### MAX_SIDE

* `pub const MAX_SIDE: u32 = 8192`: the largest side a card may have, in pixels.

## 4. Error Handling

### CardError

Why a card did not render. `Debug`, `thiserror::Error`, `PartialEq`, `#[non_exhaustive]`.

* `Size { width: u32, height: u32 }`: a side is 0 or past `MAX_SIDE`.
* `Value { element: String, property: String, value: String, reason: String }`: a declaration's value does not parse. The first one in the tree is reported.
* `MissingImage { element: String, key: String }`: an `ImageRef::Key` or `url(key)` that `Images` does not hold.
* `Image { key: String, reason: String }`: bytes that are not a PNG or a JPEG or do not decode. For `ImageRef::Bytes` the key names the element.
* `Depth { element: String }`: the tree nests deeper than 64 levels.
* `Layout(String)`: layout failed.
* `Encode(String)`: writing the PNG or JPEG failed.

### FontError

Why a font file was not added. `Debug`, `thiserror::Error`, `PartialEq`, `#[non_exhaustive]`.

* `Woff2(String)`: the WOFF2 file does not decode.
* `Woff1`: WOFF 1 is not read.
* `NotAFont`: the bytes are not TrueType or OpenType.
* `UnknownScript(String)`: a script name Unicode does not have.
