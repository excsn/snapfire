# API Reference: snapfire_media

What an image or a font file says about itself: a header's displayed size and orientation, a face's family, weight, style and metrics.

## Contents

* [1. Images](#1-images)
  * [`Header`](#header)
  * [`image::orientation`](#imageorientation)
  * [`image::svg_size`](#imagesvg_size)
  * [`strip::strip`](#stripstrip)
  * [`strip::carries_metadata`](#stripcarries_metadata)
* [2. Fonts](#2-fonts)
  * [`Style`](#style)
  * [`Metrics`](#metrics)
  * [`Face`](#face)
  * [`Fallback`](#fallback)
  * [`font::fallback`](#fontfallback)
  * [`font::fallbacks`](#fontfallbacks)
* [3. Error Handling](#3-error-handling)
  * [`Error`](#error)

## 1. Images

### Header

What an image's header says, read without decoding pixels.

* `pub struct Header { pub width: u32, pub height: u32, pub orientation: u8 }`. `#[non_exhaustive]`. `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`.
* `width` and `height`: the displayed size, the stored size with the orientation applied. When `orientation` is 5 to 8, `width` is the stored height.
* `orientation`: the EXIF tag, 1 to 8. 1 for a file that carries none or an unreadable one.
* `pub fn read(path: &Path) -> Result<Header, Error>`
* `pub fn from_bytes(path: &Path, bytes: &[u8]) -> Result<Header, Error>`: `path` names the file in an error only.
* An SVG takes its size from `image::svg_size`, with orientation 1. Any other format takes its size from `imagesize`: PNG, JPEG, GIF, WebP, AVIF, BMP, ICO and TIFF among them.
* The orientation is read from a JPEG's APP1 segment, a PNG's `eXIf` chunk or a WebP's `EXIF` chunk. Any other format reads as 1.
* `pub fn transposes(&self) -> bool`: `true` for orientations 5 to 8, where the stored frame is the transpose of the displayed one.

### image::orientation

* `pub fn orientation(bytes: &[u8]) -> u8`: the EXIF orientation tag alone. 1 when there is none or the value is outside 1 to 8.

### image::svg_size

* `pub fn svg_size(bytes: &[u8]) -> Option<(u32, u32)>`: the size an SVG draws at, from the root element's `width` and `height` when both are positive plain or `px` lengths, else from its `viewBox`. Rounded to whole pixels.
* `None` for bytes that are not an SVG and for an SVG with neither. Only the first 4096 bytes are read.

### strip::strip

* `pub fn strip(bytes: &[u8]) -> Result<Option<Vec<u8>>, String>`: the file without its metadata. The compressed image data is copied unchanged.
* A JPEG keeps JFIF, the ICC profile and Adobe's colour transform. It loses every other application segment and every comment.
* A PNG loses `tEXt`, `zTXt`, `iTXt`, `tIME` and `eXIf`.
* A WebP loses `XMP ` and `EXIF`. The extended header's flags are set to match.
* A GIF loses comments and every application extension except `NETSCAPE2.0` and `ANIMEXTS1.0`.
* The kept EXIF tags are the orientation when it is not 1, `Artist` and `Copyright`. They are written back as a little-endian TIFF block. No block is written when nothing is kept.
* `Ok(None)` for a format it does not rewrite: AVIF, SVG, BMP and ICO.
* `Err` names a JPEG, PNG, WebP or GIF whose container does not hold together.

### strip::carries_metadata

* `pub fn carries_metadata(bytes: &[u8]) -> bool`: whether the file holds an XMP packet or an EXIF block in any container, AVIF included.

## 2. Fonts

### Style

* `pub enum Style { Normal, Italic }`. `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Serialize`, `Deserialize`. Serializes lowercase.
* `pub fn as_css(self) -> &'static str`: `normal` or `italic`.
* An oblique face reads as `Italic`.

### Metrics

The vertical metrics and average advance a fallback is matched to, in font units.

* `pub struct Metrics { pub units_per_em: u16, pub ascender: i16, pub descender: i16, pub line_gap: i16, pub avg_char_width: Option<i16>, pub cap_height: Option<i16>, pub x_height: Option<i16> }`. `#[non_exhaustive]`. `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Serialize`, `Deserialize`.
* `ascender`, `descender` and `line_gap` come from the `OS/2` typographic metrics when `USE_TYPO_METRICS` is set, else from `hhea`.
* `avg_char_width`: the advance of `a` to `z` weighted by English letter frequency. `None` for a face with none of those glyphs.

### Face

One face as its tables describe it.

* `pub struct Face { pub family: String, pub weight: u16, pub style: Style, pub weight_range: Option<(u16, u16)>, pub metrics: Metrics }`. `#[non_exhaustive]`. `Debug`, `Clone`, `PartialEq`, `Eq`, `Serialize`, `Deserialize`.
* `family`: the typographic family name when present, else the family name. A Unicode name record is preferred. A Macintosh Roman one is read when that is all the font carries.
* `weight`: `OS/2`'s weight class. For a variable face, its default instance's.
* `weight_range`: the `wght` axis, lowest to highest. `None` for a static face.
* `pub fn read(path: &Path) -> Result<Face, Error>`: by extension. `woff2` and `woff` are unpacked, `ttf` and `otf` are read as they are.
* `pub fn from_bytes(path: &Path, bytes: &[u8], ext: &str) -> Result<Face, Error>`: the same over bytes. `path` names the file in an error only.
* `pub fn fallback_face(&self, family: &str, fallback: &Fallback) -> Option<String>`: one `@font-face` rule declaring `family` as `local(<fallback>)` with `size-adjust`, `ascent-override`, `descent-override` and `line-gap-override`, each a percentage to two decimals.
* `fallback_face` is `None` for a face with no `avg_char_width`.

### Fallback

A face a visitor's machine has, named as `local()` finds it.

* `pub struct Fallback { pub local: &'static str, pub metrics: Metrics }`. `#[non_exhaustive]`. `Debug`, `Clone`, `Copy`, `PartialEq`.

### font::fallback

* `pub fn fallback(name: &str) -> Option<&'static Fallback>`: matched without regard to case or surrounding space.

### font::fallbacks

* `pub fn fallbacks() -> impl Iterator<Item = &'static str>`: Arial, Arial Black, Helvetica, Verdana, Tahoma, Trebuchet MS, Georgia, Times New Roman and Courier New.

## 3. Error Handling

### Error

* `pub enum Error`. `#[non_exhaustive]`. `Debug`, `std::error::Error`.
* `Io(PathBuf, std::io::Error)`: reading the file. Displays as `reading <path>: <error>`.
* `Image(PathBuf, String)`: not an image whose header this crate reads. Displays as `<path> is not an image this build reads: <detail>`.
* `Font(PathBuf, String)`: not a font this crate reads, including one without a family name. Displays as `<path> is not a font this build reads: <detail>`.
