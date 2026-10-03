# Usage Guide: snapfire_media

How to read what an image or a font says about itself before deciding what to do with it.

## Table of Contents

* [Core Concepts](#core-concepts)
* [Quick Start](#quick-start)
* [Reading an Image's Header](#reading-an-images-header)
* [Applying the Orientation](#applying-the-orientation)
* [Reading a Font](#reading-a-font)
* [Writing the Fallback Face](#writing-the-fallback-face)
* [Error Handling](#error-handling)

## Core Concepts

* **Header**: an image's displayed width and height and the EXIF orientation they account for, read without decoding a pixel.
* **Orientation**: the EXIF tag, 1 to 8. A browser rotates a served file by it; a tool that re-encodes has to rotate the pixels itself, since the output carries no tag.
* **Displayed size**: the stored size with the orientation applied. Orientations 5 to 8 transpose it.
* **Face**: one font file read for its family, weight, style and metrics, a woff2 or a woff unpacked first.
* **Metrics**: units per em, ascender, descender, line gap and the frequency-weighted advance of `a` to `z`.
* **Fallback**: a face a visitor already has, with the metrics a fallback `@font-face` is computed against.

## Quick Start

```rust
use snapfire_media::{Face, Header, font};
use std::path::Path;

fn main() -> Result<(), snapfire_media::Error> {
  let header = Header::read(Path::new("app/img/photo.jpg"))?;
  println!("{}x{} as displayed, orientation {}", header.width, header.height, header.orientation);

  let inter = Face::read(Path::new("app/fonts/Inter-Regular.woff2"))?;
  let css = inter.fallback_face("Inter Fallback", font::fallback("Arial").unwrap());
  println!("{css}");
  Ok(())
}
```

## Reading an Image's Header

`Header::read` gives the size the file displays at, which is what belongs on a `width` and `height` attribute:

```rust
use snapfire_media::Header;
use std::path::Path;

let header = Header::read(Path::new("app/img/photo.jpg"))?;
// a phone photo stored 4000x3000 with orientation 6
assert_eq!((header.width, header.height), (3000, 4000));
assert_eq!(header.orientation, 6);
assert!(header.transposes());
```

A file with no tag is upright at its stored size, orientation 1. The same read over bytes already in memory is `Header::from_bytes`, where the path only names the file in an error.

## Applying the Orientation

The crate reads the tag and never touches pixels. A decoder that resizes applies it, since its output carries no tag for the browser to honour. With the `image` crate:

```rust
use image::metadata::Orientation;
use snapfire_media::image::orientation;

let bytes = std::fs::read("app/img/photo.jpg")?;
let mut decoded = image::load_from_memory(&bytes)?;
if let Some(orientation) = Orientation::from_exif(orientation(&bytes)) {
  decoded.apply_orientation(orientation);
}
// decoded is now 3000x4000, the way the browser shows the original
```

## Reading a Font

`Face::read` unpacks a woff2 or a woff to the sfnt inside and reads a ttf or an otf as it is:

```rust
use snapfire_media::{Face, Style};
use std::path::Path;

let inter = Face::read(Path::new("app/fonts/Inter-Bold.woff2"))?;
assert_eq!(inter.family, "Inter");
assert_eq!(inter.weight, 700);
assert_eq!(inter.style, Style::Normal);
assert!(inter.metrics.units_per_em > 0);
```

The family is the typographic family name when the font has one, so `Inter Bold` reads as `Inter` with weight 700.

## Writing the Fallback Face

A fallback face is the system font a visitor already has, scaled so its text takes the room the real face's will:

```rust
use snapfire_media::{Face, font};
use std::path::Path;

let inter = Face::read(Path::new("app/fonts/Inter-Regular.woff2"))?;
let arial = font::fallback("Arial").expect("a known fallback");
let css = inter.fallback_face("Inter Fallback", arial);
// @font-face { font-family: "Inter Fallback"; src: local("Arial"); size-adjust: 108.20%; ascent-override: 89.53%; descent-override: 22.32%; line-gap-override: 0.00%; }
```

`size-adjust` is the ratio of the two faces' frequency-weighted average advances; the three overrides are the real face's vertical metrics divided by that scale. The fallbacks on offer are the ones `font::fallbacks` lists: Arial, Arial Black, Helvetica, Verdana, Tahoma, Trebuchet MS, Georgia, Times New Roman and Courier New, with metrics read from the files macOS and Windows ship.

## Error Handling

Every function returns `snapfire_media::Error`. The variants name the file:

```rust
use snapfire_media::{Error, Header};
use std::path::Path;

match Header::read(Path::new("app/img/photo.jpg")) {
  Ok(header) => { let _ = header; }
  Err(Error::Io(path, e)) => eprintln!("could not read {}: {e}", path.display()),
  Err(Error::Image(path, why)) => eprintln!("{} is not an image: {why}", path.display()),
  Err(other) => eprintln!("{other}"),
}
```

`Font` names the file that was not a font, including one without a family name or `a` to `z` glyphs.
