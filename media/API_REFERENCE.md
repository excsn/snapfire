# API Reference: snapfire_media

What an image or a font file says about itself: a header's displayed size and orientation, a face's family, weight, style and metrics.

## Contents

* [1. Images](#1-images)
  * [Header](#header)
  * [Image functions](#image-functions)
  * [Stripping metadata](#stripping-metadata)
* [2. Fonts](#2-fonts)
  * [Style](#style)
  * [Metrics](#metrics)
  * [Face](#face)
  * [Fallback](#fallback)
  * [Font functions](#font-functions)
* [3. Error Handling](#3-error-handling)

## 1. Images

### Header

`pub struct Header { pub width: u32, pub height: u32, pub orientation: u8 }`. `width` and `height` are the displayed size: the stored size with the orientation applied, so the stored width when `orientation` is 5 to 8 is `height` here. `orientation` is the EXIF tag, 1 to 8, with 1 for a file that carries none.

* `fn read(path: &Path) -> Result<Header, Error>`.
* `fn from_bytes(path: &Path, bytes: &[u8]) -> Result<Header, Error>`: `path` names the file in an error only.
* `fn transposes(&self) -> bool`: `true` for orientations 5 to 8, where the stored frame is the transpose of the displayed one.

The size is read by `imagesize`, so every format it knows is covered: PNG, JPEG, GIF, WebP, AVIF, BMP, ICO, TIFF and more. The tag is read from a JPEG's APP1 segment, a PNG's `eXIf` chunk or a WebP's `EXIF` chunk; any other container reads as 1.

### Image functions

* `image::orientation(bytes: &[u8]) -> u8`: the tag alone, 1 when there is none or the value is outside 1 to 8.

### Stripping metadata

* `strip::strip(bytes: &[u8]) -> Result<Option<Vec<u8>>, String>`: the file without its metadata, the compressed image data copied unchanged. A JPEG keeps JFIF, the ICC profile and Adobe's colour transform and loses every other application segment and comment; a PNG loses `tEXt`, `zTXt`, `iTXt`, `tIME` and `eXIf`; a WebP loses `XMP ` and `EXIF` with its extended header's flags set to match; a GIF loses comments and every application extension but `NETSCAPE2.0` and `ANIMEXTS1.0`. What EXIF keeps, the orientation when it is not 1 and the `Artist` and `Copyright` tags, is written back as a little-endian TIFF block, with none at all when nothing is kept. `Ok(None)` for a format it does not rewrite: AVIF, SVG, BMP, ICO. An error names a container that does not hold together.
* `strip::carries_metadata(bytes: &[u8]) -> bool`: whether the file holds an EXIF block in any container `kamadak-exif` reads, AVIF among them. An XMP packet counts too.

## 2. Fonts

### Style

`Style::Normal | Style::Italic`; `fn as_css(self) -> &'static str`. An oblique face reads as italic.

### Metrics

`pub struct Metrics { pub units_per_em: u16, pub ascender: i16, pub descender: i16, pub line_gap: i16, pub avg_char_width: Option<i16>, pub cap_height: Option<i16>, pub x_height: Option<i16> }`, all in font units. `ascender`, `descender` and `line_gap` follow `OS/2` typographic metrics when `USE_TYPO_METRICS` is set and `hhea` otherwise. `avg_char_width` is the advance of `a` to `z` weighted by English letter frequency, `None` for a face with none of those glyphs, such as a provider's non-Latin subset.

### Face

`pub struct Face { pub family: String, pub weight: u16, pub style: Style, pub weight_range: Option<(u16, u16)>, pub metrics: Metrics }`. `weight` is `OS/2`'s weight class, which for a variable face is its default instance's and not the weight a provider declared it under; `weight_range` is the `wght` axis, lowest to highest, the `font-weight` range one variable file serves, `None` for a static face.

* `fn read(path: &Path) -> Result<Face, Error>`: by extension, `woff2` and `woff` unpacked, `ttf` and `otf` as they are.
* `fn from_bytes(path: &Path, bytes: &[u8], ext: &str) -> Result<Face, Error>`: the same over bytes; `path` names the file in an error only.
* `fn fallback_face(&self, family: &str, fallback: &Fallback) -> Option<String>`: one `@font-face` rule, `None` for a face with no `a` to `z` advance to size it by, declaring `family` as `local(<fallback>)` with `size-adjust`, `ascent-override`, `descent-override` and `line-gap-override`, each a percentage to two decimals.

`family` is the typographic family name when present, else the family name; a Unicode name record is preferred and a Macintosh Roman one read when that is all the font carries.

### Fallback

`pub struct Fallback { pub local: &'static str, pub metrics: Metrics }`: a face a visitor has, named as `local()` finds it.

### Font functions

* `font::fallback(name: &str) -> Option<&'static Fallback>`: matched without regard to case or surrounding space.
* `font::fallbacks() -> impl Iterator<Item = &'static str>`: Arial, Arial Black, Helvetica, Verdana, Tahoma, Trebuchet MS, Georgia, Times New Roman, Courier New.

## 3. Error Handling

`pub enum Error`, `#[non_exhaustive]`, `thiserror`:

* `Io(PathBuf, std::io::Error)`: reading the file.
* `Image(PathBuf, String)`: not an image whose header this crate reads.
* `Font(PathBuf, String)`: not a font this crate reads, including one without a family name.
