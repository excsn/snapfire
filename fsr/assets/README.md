# snapfire_fsr_assets

[![Crates.io](https://img.shields.io/crates/v/snapfire_fsr_assets.svg)](https://crates.io/crates/snapfire_fsr_assets)
[![Docs.rs](https://docs.rs/snapfire_fsr_assets/badge.svg)](https://docs.rs/snapfire_fsr_assets)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

What FSR derives from an image or a font: resized and re-encoded variants under one `VariantPolicy`, a face's metrics read from its tables and the `@font-face` for a system fallback sized to match. What a file says about itself, a header's size and orientation or a face's tables, is read through `snapfire_media`, the crate snapfirec reads through too, so the two agree on every number. `fsr build` calls these at build time and a host answering images at request time calls the same functions. The guide is [README.USAGE.md](README.USAGE.md) and the surface is [API_REFERENCE.md](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_fsr_assets = "0.2"
```

No features. AVIF encoding is pure Rust through `ravif`; WebP encoding links `libwebp` through the `webp` crate.

| To | Reach for |
| --- | --- |
| Decide which widths an image gets | `VariantPolicy::widths_for` |
| Resize and encode one variant | `Source::variant` |
| Read a displayed width and height without decoding | `image::dimensions` |
| Know whether a file is served as it is | `image::passthrough` |
| Name a variant or a hashed original | `variant_name`, `hash::emitted_name` |
| Read a woff2, woff, ttf or otf for its family, weight and metrics | `Face::read` |
| Write the fallback face that stops text reflowing | `Face::fallback_face` with `font::fallback` |

## License

This project is licensed under the **Mozilla Public License Version 2.0 (MPL-2.0)**. See the `LICENSE` file in the repository for the full license text.
