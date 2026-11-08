# snapfire_media

[![Crates.io](https://img.shields.io/crates/v/snapfire_media.svg)](https://crates.io/crates/snapfire_media)
[![Docs.rs](https://docs.rs/snapfire_media/badge.svg)](https://docs.rs/snapfire_media)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

Reads what an image or a font file says about itself, without decoding it.

For an image, that is the width and height it displays at with any EXIF rotation applied. For a font, it is the family, weight, style and the metrics needed to size a fallback face so text does not reflow when the web font loads. It reads JPEG, PNG, GIF, WebP, AVIF, SVG and other image formats. It reads woff2, woff, ttf and otf fonts.

`snapfirec` and FSR's asset pipeline both read files through this crate. The width a compiled module carries and the width the server writes on the `<img>` element are therefore always the same number. It can also strip metadata such as EXIF and XMP from an image without re-encoding it.

* The task guide is [README.USAGE.md](README.USAGE.md).
* The call surface is [API_REFERENCE.md](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_media = "0"
```

No features.

## Quick start

```rust
use snapfire_media::{font, Face, Header};
use std::path::Path;

let header = Header::read(Path::new("static/hero.jpg"))?;
println!("{}x{}", header.width, header.height);

let face = Face::read(Path::new("static/fonts/inter.woff2"))?;
let arial = font::fallback("Arial").expect("a known fallback");
if let Some(rule) = face.fallback_face("Inter Fallback", arial) {
  println!("{rule}");
}
```

## What to reach for

| Problem | Piece |
| --- | --- |
| Read a displayed width and height without decoding | `Header::read` |
| Know whether a file's pixels are stored on their side | `Header::transposes` |
| Read the orientation tag alone | `image::orientation` |
| Read a woff2, woff, ttf or otf for its family, weight and metrics | `Face::read` |
| Write the fallback face that stops text reflowing | `Face::fallback_face` with `font::fallback` |
| Remove EXIF, XMP and comments from an image | `strip::strip` |
| Size an SVG from its `width`, `height` or `viewBox` | `Header::read` or `image::svg_size` |

## License

This project is licensed under the **Mozilla Public License Version 2.0 (MPL-2.0)**. See the [LICENSE](LICENSE) file for the full license text.
