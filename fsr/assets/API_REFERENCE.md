# API Reference: snapfire_fsr_assets

Image variants under a policy, a font face's metrics and the fallback face sized to match them.

## Contents

* [1. Crate Root](#1-crate-root)
  * [Re-exports](#re-exports)
* [2. Policy](#2-policy)
  * [Format](#format)
  * [VariantPolicy](#variantpolicy)
* [3. Images](#3-images)
  * [Source](#source)
  * [Header](#header)
  * [image::dimensions](#imagedimensions)
  * [image::passthrough](#imagepassthrough)
  * [variant_name](#variant_name)
  * [strip](#strip)
* [4. Fonts](#4-fonts)
  * [Style](#style)
  * [Metrics](#metrics)
  * [Face](#face)
  * [Fallback](#fallback)
  * [font::fallback](#fontfallback)
  * [font::fallbacks](#fontfallbacks)
* [5. Hashing](#5-hashing)
  * [hash::of](#hashof)
  * [hash::emitted_name](#hashemitted_name)
* [6. Error Handling](#6-error-handling)
  * [Error](#error)

## 1. Crate Root

Modules: `hash`, `image`, `policy`, `font`, `strip`.

### Re-exports

* `pub use snapfire_media::{Face, Fallback, Header, Metrics, Style};`
* `pub use image::{Source, variant_name};`
* `pub use policy::{Format, VariantPolicy};`
* `font` re-exports `snapfire_media::font::*`. `strip` re-exports `snapfire_media::strip::*`.

## 2. Policy

### Format

An output format. `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `PartialOrd`, `Ord`, `Hash`, `Serialize`, `Deserialize`, `#[non_exhaustive]`. Serialises in lowercase.

* `Format::Avif`, `Format::Webp`.
* `pub const ALL: [Format; 2]`: `[Avif, Webp]`.
* `pub fn extension(self) -> &'static str`: `avif` or `webp`.
* `pub fn mime(self) -> &'static str`: `image/avif` or `image/webp`.
* `pub fn parse(name: &str) -> Option<Format>`: by extension, trimmed, in any case.
* `pub fn default_quality(self) -> u8`: 60 for AVIF, 80 for WebP.
* `impl Display`: the extension.

### VariantPolicy

The widths, formats and per-format quality an image's variants are made with. `Debug`, `Clone`, `PartialEq`, `Eq`, `Serialize`, `Deserialize`, `#[non_exhaustive]`.

* `pub struct VariantPolicy { pub widths: Vec<u32>, pub formats: Vec<Format>, pub quality: BTreeMap<Format, u8> }`. `quality` is `#[serde(default)]`.
* `Default`: widths `[640, 960, 1280, 1920, 2560]`, formats `[Avif, Webp]`, quality `{avif: 60, webp: 80}`.
* `pub fn quality(&self, format: Format) -> u8`: the configured quality, else `format.default_quality()`.
* `pub fn widths_for(&self, source_width: u32) -> Vec<u32>`: every non-zero policy width strictly below `source_width`, sorted and deduplicated, then `source_width`. Empty for a width of 0.
* `pub fn widths(self, widths: Vec<u32>) -> Self`: replaces `widths`.
* `pub fn formats(self, formats: Vec<Format>) -> Self`: replaces `formats`.
* `pub fn with_widths(self, widths: Vec<u32>) -> Self`: replaces `widths`.
* `pub fn with_quality(self, format: Format, quality: u8) -> Self`: sets the quality for `format`, clamped to 100.

## 3. Images

### Source

A decoded image held as RGBA, upright.

* `pub fn open(path: &Path) -> Result<Source, Error>`
* `pub fn from_bytes(path: &Path, bytes: &[u8]) -> Result<Source, Error>`: `path` names the file in an error only. PNG, JPEG, GIF, WebP, BMP and ICO decode. The EXIF orientation in the bytes is applied to the pixels.
* `pub fn width(&self) -> u32`: the displayed width.
* `pub fn height(&self) -> u32`: the displayed height.
* `pub fn height_at(&self, width: u32) -> u32`: the height at `width` with the aspect ratio kept, rounded and never 0. The source's height for a width at or past its own.
* `pub fn variant(&self, width: u32, format: Format, quality: u8) -> Result<Vec<u8>, Error>`: resized with Lanczos3 when `width` is below the source's width, then encoded. `quality` is 0 to 100, clamped.

### Header

What an image's header says. `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `#[non_exhaustive]`.

* `pub struct Header { pub width: u32, pub height: u32, pub orientation: u8 }`
* `width` and `height` are the displayed size, with the orientation applied.
* `orientation` is the EXIF orientation, 1 to 8. It is 1 when the file carries none or an unreadable one.
* `pub fn read(path: &Path) -> Result<Header, snapfire_media::Error>`
* `pub fn from_bytes(path: &Path, bytes: &[u8]) -> Result<Header, snapfire_media::Error>`: `path` names the file in an error only.
* `pub fn transposes(&self) -> bool`: whether the stored frame is the transpose of the displayed one.

### image::dimensions

* `pub fn dimensions(path: &Path) -> Result<(u32, u32), Error>`: the displayed width and height from `Header::read`. Nothing is decoded.

### image::passthrough

* `pub fn passthrough(path: &Path) -> Result<bool, Error>`: `true` for a `.svg`, a `.gif` with more than one frame and a `.png` or `.apng` carrying `acTL` before its first `IDAT`. By extension, in any case.

### variant_name

* `pub fn variant_name(stem: &str, hash: &str, width: u32, format: Format) -> String`: `<stem>.<hash>.<width>.<ext>`.

### strip

* `pub fn strip(bytes: &[u8]) -> Result<Option<Vec<u8>>, String>`: a JPEG, PNG, WebP or GIF with its metadata removed and its image data copied unchanged. The ICC profile, the EXIF orientation, artist and copyright are kept. `None` for AVIF, SVG, BMP and ICO. An error names a container that does not hold together.
* `pub fn carries_metadata(bytes: &[u8]) -> bool`: whether `bytes` holds an EXIF or XMP block, in any container.

## 4. Fonts

### Style

`Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Serialize`, `Deserialize`.

* `Style::Normal`, `Style::Italic`. An oblique face reads as `Italic`.
* `pub fn as_css(self) -> &'static str`

### Metrics

A face's vertical and width metrics, in font units. `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Serialize`, `Deserialize`, `#[non_exhaustive]`.

* `pub struct Metrics { pub units_per_em: u16, pub ascender: i16, pub descender: i16, pub line_gap: i16, pub avg_char_width: Option<i16>, pub cap_height: Option<i16>, pub x_height: Option<i16> }`
* `ascender`, `descender` and `line_gap` come from the `OS/2` typographic metrics when `USE_TYPO_METRICS` is set, else from `hhea`.
* `avg_char_width` is the advance of `a` to `z` weighted by English letter frequency. `None` for a face with none of those glyphs.

### Face

One font face. `Debug`, `Clone`, `PartialEq`, `Eq`, `Serialize`, `Deserialize`, `#[non_exhaustive]`.

* `pub struct Face { pub family: String, pub weight: u16, pub style: Style, pub weight_range: Option<(u16, u16)>, pub metrics: Metrics }`
* `family` is the typographic family name when present, else the family name. A Unicode name record is preferred over a Macintosh Roman one.
* `weight` is the default instance's weight class. `weight_range` is the `wght` axis of a variable face.
* `pub fn read(path: &Path) -> Result<Face, snapfire_media::Error>`: by extension, in any case. `woff2` and `woff` are unpacked. Anything else is read as `ttf` or `otf`.
* `pub fn from_bytes(path: &Path, bytes: &[u8], ext: &str) -> Result<Face, snapfire_media::Error>`: the same over bytes, with `ext` lowercase. `path` names the file in an error only.
* `pub fn fallback_face(&self, family: &str, fallback: &Fallback) -> Option<String>`: one `@font-face` rule declaring `family` as `local(<fallback>)` with `size-adjust`, `ascent-override`, `descent-override` and `line-gap-override`, each a percentage to two decimals. `None` when `metrics.avg_char_width` is `None`.

### Fallback

A face a visitor already has, with its metrics. `Debug`, `Clone`, `Copy`, `PartialEq`, `#[non_exhaustive]`.

* `pub struct Fallback { pub local: &'static str, pub metrics: Metrics }`: `local` is the name `local()` finds it under.

### font::fallback

* `pub fn fallback(name: &str) -> Option<&'static Fallback>`: matched with `name` trimmed, in any ASCII case.

### font::fallbacks

* `pub fn fallbacks() -> impl Iterator<Item = &'static str>`: Arial, Arial Black, Helvetica, Verdana, Tahoma, Trebuchet MS, Georgia, Times New Roman, Courier New.

## 5. Hashing

### hash::of

* `pub fn of(bytes: &[u8]) -> String`: the low 32 bits of xxh3-64 over `bytes`, as eight lowercase hex digits. The same digest snapfirec names an emitted asset with.

### hash::emitted_name

* `pub fn emitted_name(stem: &str, hash: &str, ext: &str) -> String`: `<stem>.<hash>.<ext>`.

## 6. Error Handling

### Error

`Debug`, `thiserror::Error`, `#[non_exhaustive]`.

* `Io(PathBuf, std::io::Error)`: reading the file. Displays as `reading {0}: {1}`.
* `Decode(PathBuf, String)`: not an image this crate decodes. Displays as `{0} is not an image this build decodes: {1}`.
* `Encode(&'static str, String)`: the format that failed to encode, then why.
* `Media(snapfire_media::Error)`: `#[error(transparent)]` and `From`. A header or a face `snapfire_media` could not read, including a font with no family name.
