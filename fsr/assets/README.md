# snapfire_fsr_assets

[![Crates.io](https://img.shields.io/crates/v/snapfire_fsr_assets.svg)](https://crates.io/crates/snapfire_fsr_assets)
[![Docs.rs](https://docs.rs/snapfire_fsr_assets/badge.svg)](https://docs.rs/snapfire_fsr_assets)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

Makes responsive AVIF and WebP versions of an FSR application's images. It also writes font fallbacks that keep text from jumping when a web font loads.

For an image it picks the widths to make, resizes and encodes each one and names it with a content hash. It never makes a version wider than the original. SVGs and animated images are served as they are.

For a font it reads the family, weight, style and metrics from a woff2, woff, ttf or otf file. It then writes an `@font-face` that sizes a system font, such as Arial or Georgia, to take up the same space.

Most applications never depend on this crate. `fsr build` calls it on every image and font in the app. A host that resizes images on request calls the same functions. It reads an image's size and orientation with [`snapfire_media`](../../media/README.md), as snapfirec does, so the two always agree on the size. Use the crate directly when you write your own build step.

* The task guide is [README.USAGE.md](README.USAGE.md).
* The call surface is [API_REFERENCE.md](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_fsr_assets = "0.3"
```

The crate has no features.

## Quick start

```rust
use snapfire_fsr_assets::{hash, Face, Format, Source, VariantPolicy, font, variant_name};
use std::path::Path;

let policy = VariantPolicy::default();
let source = Source::open(Path::new("app/img/hero.png"))?;
let digest = hash::of(&std::fs::read("app/img/hero.png")?);
for width in policy.widths_for(source.width()) {
  let avif = source.variant(width, Format::Avif, policy.quality(Format::Avif))?;
  std::fs::write(variant_name("hero", &digest, width, Format::Avif), avif)?;
}

let face = Face::read(Path::new("app/fonts/Inter.woff2"))?;
let css = face.fallback_face("Inter Fallback", font::fallback("Arial").unwrap());
```

## What to reach for

| Problem | Piece |
| --- | --- |
| Decide which widths an image gets | `VariantPolicy::widths_for` |
| Resize and encode one variant | `Source::variant` |
| Read a displayed width and height without decoding | `image::dimensions` |
| Know whether a file is served as it is | `image::passthrough` |
| Name a variant or a hashed original | `variant_name`, `hash::emitted_name` |
| Remove GPS and camera metadata from a served original | `strip::strip` |
| Read a woff2, woff, ttf or otf for its family, weight and metrics | `Face::read` |
| Write the fallback face that stops text reflowing | `Face::fallback_face` with `font::fallback` |

## License

This project is licensed under the **Mozilla Public License Version 2.0 (MPL-2.0)**. See the `LICENSE` file in the repository for the full license text.
