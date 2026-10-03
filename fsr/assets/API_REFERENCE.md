# API Reference: snapfire_fsr_assets

What FSR derives from an image or a font: variants under a policy, a face's metrics and the fallback that matches them. The header and face reads are `snapfire_media`'s, re-exported here.

## Contents

* [1. Policy](#1-policy)
  * [Format](#format)
  * [VariantPolicy](#variantpolicy)
* [2. Images](#2-images)
  * [Source](#source)
  * [Functions](#functions)
* [3. Fonts](#3-fonts)
  * [Style](#style)
  * [Metrics](#metrics)
  * [Face](#face)
  * [Fallback](#fallback)
  * [Font functions](#font-functions)
* [4. Hashing](#4-hashing)
* [5. Error Handling](#5-error-handling)

## 1. Policy

### Format

An output format. Serialises in lowercase.

* `Format::Avif`, `Format::Webp`; `Format::ALL: [Format; 2]`.
* `fn extension(self) -> &'static str`: `avif` or `webp`.
* `fn mime(self) -> &'static str`: `image/avif` or `image/webp`.
* `fn parse(name: &str) -> Option<Format>`: by extension, any case, trimmed.
* `fn default_quality(self) -> u8`: 60 for AVIF, 80 for WebP.
* `impl Display`: the extension.

### VariantPolicy

`pub struct VariantPolicy { pub widths: Vec<u32>, pub formats: Vec<Format>, pub quality: BTreeMap<Format, u8> }`. Serialises as the `[images]` table.

* `Default`: widths `[640, 960, 1280, 1920, 2560]`, formats `[Avif, Webp]`, quality `{avif: 60, webp: 80}`.
* `fn quality(&self, format: Format) -> u8`: the configured quality, else the format's default.
* `fn widths_for(&self, source_width: u32) -> Vec<u32>`: every policy width strictly below `source_width`, sorted and deduplicated, then `source_width`. Empty for a width of 0.
* `fn with_widths(self, widths: Vec<u32>) -> Self`, `fn with_quality(self, format: Format, quality: u8) -> Self`: the same policy with one field replaced; a quality above 100 is clamped.

## 2. Images

### Source

A decoded image held as RGBA.

* `fn open(path: &Path) -> Result<Source, Error>`.
* `fn from_bytes(path: &Path, bytes: &[u8]) -> Result<Source, Error>`: `path` names the file in an error only. PNG, JPEG, GIF, WebP, BMP and ICO decode. The EXIF orientation the bytes carry is applied, so the pixels are upright and `width` and `height` are the displayed size.
* `fn width(&self) -> u32`, `fn height(&self) -> u32`.
* `fn height_at(&self, width: u32) -> u32`: the height at `width` keeping the aspect ratio, never 0; the source's height for a width at or past its own.
* `fn variant(&self, width: u32, format: Format, quality: u8) -> Result<Vec<u8>, Error>`: resized to `width` when below the source's, Lanczos3, then encoded; `quality` is 0 to 100 and clamped.

### Functions

* `image::dimensions(path: &Path) -> Result<(u32, u32), Error>`: the displayed size from the header, nothing decoded; `snapfire_media::Header::read` with the orientation applied.
* `image::passthrough(path: &Path) -> Result<bool, Error>`: `true` for an SVG, a GIF with more than one frame or a PNG carrying `acTL`.
* `variant_name(stem: &str, hash: &str, width: u32, format: Format) -> String`: `<stem>.<hash>.<width>.<ext>`.

## 3. Fonts

`font` is `snapfire_media::font` re-exported. `Face`, `Fallback`, `Metrics`, `Style` and `Header` are re-exported at the root.

### Style

`Style::Normal | Style::Italic`; `fn as_css(self) -> &'static str`. An oblique face reads as italic.

### Metrics

`pub struct Metrics { pub units_per_em: u16, pub ascender: i16, pub descender: i16, pub line_gap: i16, pub avg_char_width: Option<i16>, pub cap_height: Option<i16>, pub x_height: Option<i16> }`, all in font units. `ascender`, `descender` and `line_gap` follow `OS/2` typographic metrics when `USE_TYPO_METRICS` is set and `hhea` otherwise. `avg_char_width` is the advance of `a` to `z` weighted by English letter frequency, `None` for a face with none of those glyphs, such as a provider's non-Latin subset.

### Face

`pub struct Face { pub family: String, pub weight: u16, pub style: Style, pub weight_range: Option<(u16, u16)>, pub metrics: Metrics }`. `weight_range` is the `wght` axis of a variable face, the `font-weight` range one file serves; `weight` is the default instance's class, which for a variable face is not the weight a provider declared it under.

* `fn read(path: &Path) -> Result<Face, Error>`: by extension, `woff2` and `woff` unpacked, `ttf` and `otf` as they are.
* `fn from_bytes(path: &Path, bytes: &[u8], ext: &str) -> Result<Face, Error>`: the same over bytes; `path` names the file in an error only.
* `fn fallback_face(&self, family: &str, fallback: &Fallback) -> Option<String>`: one `@font-face` rule, `None` for a face with no `a` to `z` advance to size it by, declaring `family` as `local(<fallback>)` with `size-adjust`, `ascent-override`, `descent-override` and `line-gap-override`, each a percentage to two decimals.

`family` is the typographic family name when present, else the family name; a Unicode name record is preferred and a Macintosh Roman one read when that is all the font carries.

### Fallback

`pub struct Fallback { pub local: &'static str, pub metrics: Metrics }`: a face a visitor has, named as `local()` finds it.

### Font functions

* `font::fallback(name: &str) -> Option<&'static Fallback>`: matched without regard to case or surrounding space.
* `font::fallbacks() -> impl Iterator<Item = &'static str>`: Arial, Arial Black, Helvetica, Verdana, Tahoma, Trebuchet MS, Georgia, Times New Roman, Courier New.

## 4. Hashing

* `hash::of(bytes: &[u8]) -> String`: eight lowercase hex digits of xxh3-64's low 32 bits, the digest snapfirec names an emitted asset with.
* `hash::emitted_name(stem: &str, hash: &str, ext: &str) -> String`: `<stem>.<hash>.<ext>`.

## 5. Error Handling

`pub enum Error`, `#[non_exhaustive]`, `thiserror`:

* `Io(PathBuf, std::io::Error)`: reading the file.
* `Decode(PathBuf, String)`: not an image this crate decodes.
* `Encode(&'static str, String)`: the format that failed to encode.
* `Media(snapfire_media::Error)`, transparent and `From`: a header or a face `snapfire_media` could not read, including a font without a family name.
