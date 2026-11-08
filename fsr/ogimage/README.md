# snapfire_fsr_ogimage

[![Crates.io](https://img.shields.io/crates/v/snapfire_fsr_ogimage.svg)](https://crates.io/crates/snapfire_fsr_ogimage)
[![Docs.rs](https://docs.rs/snapfire_fsr_ogimage/badge.svg)](https://docs.rs/snapfire_fsr_ogimage)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

Draws social share cards, the images a link preview shows, as PNG or JPEG, from boxes, text and images styled with CSS.

It supports the same CSS subset as satori, so a card written for Next's `ImageResponse` lays out the same way here. That covers flexbox layout, text with mixed styles that wraps by the Unicode line-breaking rules and fonts with fallback for other scripts such as CJK. It needs no browser and no JavaScript.

In an FSR application you write `opengraph-image.tsx` or `twitter-image.tsx` beside a page. The host draws the card on request with this crate. That needs the host's `share_cards` feature, which `fsr serve` has. Use the crate directly when you draw cards from your own Rust code.

* The task guide is [README.USAGE.md](README.USAGE.md).
* The call surface is [API_REFERENCE.md](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_fsr_ogimage = "0.2"
```

The crate has no features and is pure Rust. A stripped release binary grows by about 1.9 MB.

## Quick start

```rust
use snapfire_fsr_ogimage::{render, Card, Element, Fonts, Format, Images};

let card = Card::new(1200, 630, Element::boxed("div")
  .with("display", "flex")
  .with("align-items", "center")
  .with("justify-content", "center")
  .with("background-color", "#0b1020")
  .child(Element::text("h1", "Hello from FSR").with("color", "white").with("font-size", "72px")));
let png = render(&card, &Fonts::new(), &Images::new(), Format::Png)?;
std::fs::write("card.png", png.bytes)?;
```

## What to reach for

| Problem | Piece |
| --- | --- |
| Describe a card | `Card`, `Element::boxed`, `Element::text`, `Element::image` |
| Style an element | `Element::with`, CSS declarations as written |
| Give text more than one style | `Run` inside `Element::Text` |
| Use an application's fonts | `Fonts::add` |
| Use a face for one script, such as CJK | `Fonts::add_for_scripts` |
| Give the card its images | `Images::insert`, then `ImageRef::Key` or `url(key)` |
| Render to bytes | `render` with `Format::Png` or `Format::Jpeg` |
| See what was skipped | `Rendered::warnings` |

## License

This project is licensed under the **Mozilla Public License Version 2.0 (MPL-2.0)**. See the `LICENSE` file in the repository for the full license text.

The bundled Inter in `fonts/` is licensed under the **SIL Open Font License 1.1 (OFL-1.1)**. See `fonts/OFL.txt`.
