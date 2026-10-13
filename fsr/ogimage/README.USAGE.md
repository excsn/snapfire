# Usage Guide: snapfire_fsr_ogimage

How to describe a card, style it, give it fonts and images and render it.

## Table of Contents

* [Core Concepts](#core-concepts)
* [Quick Start](#quick-start)
* [Building a Card](#building-a-card)
  * [Boxes](#boxes)
  * [Text and Runs](#text-and-runs)
  * [Images](#images)
* [Styling with the CSS Subset](#styling-with-the-css-subset)
  * [Layout](#layout)
  * [Paint](#paint)
  * [Text](#text)
  * [Lengths and Colors](#lengths-and-colors)
* [Adding Fonts](#adding-fonts)
  * [Choosing a Face](#choosing-a-face)
  * [A Face for One Script](#a-face-for-one-script)
* [Giving the Card Images](#giving-the-card-images)
* [Rendering](#rendering)
  * [Reading the Warnings](#reading-the-warnings)
  * [Choosing PNG or JPEG](#choosing-png-or-jpeg)
* [Error Handling](#error-handling)

## Core Concepts

* **Card**: the image to render, its width and height in pixels and one root element.
* **Element**: a box, a text element or an image. Every element is a flexbox item.
* **Box**: an element holding other elements, laid out with flexbox.
* **Text element**: an element holding one paragraph.
* **Run**: a stretch of a paragraph with declarations of its own, the way a `span` is.
* **Declarations**: CSS properties and values in source order, as `("font-size", "72px")`.
* **Subset**: the properties and values the renderer reads. Anything else warns.
* **Fonts**: the faces text is drawn with. The bundled Inter always comes last.
* **Fallback**: the order faces are tried for a character no earlier face covers.
* **Images**: decoded images by key. A card names them and the renderer never fetches.
* **Warning**: something skipped or not drawn. The card still renders.
* **Rendered**: the encoded bytes and the warnings.

## Quick Start

```rust
use snapfire_fsr_ogimage::{render, Card, Element, Fonts, Format, Images};

fn main() -> Result<(), Box<dyn std::error::Error>> {
  let root = Element::boxed("div")
    .with("flex-direction", "column")
    .with("justify-content", "space-between")
    .with("width", "100%")
    .with("height", "100%")
    .with("padding", "64px")
    .with("background-image", "linear-gradient(135deg, #0f172a, #1e3a8a)")
    .with("color", "white")
    .child(Element::text("span", "Noticeboard").with("font-size", "28px").with("font-weight", "700"))
    .child(Element::text("h1", "Roof work on the east stair").with("font-size", "72px").with("line-clamp", "2"));
  let card = Card::new(1200, 630, root);
  let rendered = render(&card, &Fonts::new(), &Images::new(), Format::Png)?;
  for warning in &rendered.warnings {
    eprintln!("{warning}");
  }
  std::fs::write("card.png", &rendered.bytes)?;
  Ok(())
}
```

`examples/card.rs` is the same card with a shadow, an opacity and an underlined run: `cargo run --release --example card -- card.png`.

## Building a Card

### Boxes

```rust
use snapfire_fsr_ogimage::Element;

let row = Element::boxed("div")
  .with("align-items", "center")
  .with("gap", "16px")
  .child(Element::boxed("div").with("width", "48px").with("height", "48px").with("background-color", "#f97316"))
  .child(Element::text("span", "Noticeboard"));
```

Every element starts at `display: flex` with `flex-direction: row`, as in satori. The root is stretched to the card's size unless it sets its own. `tag` names the element in warnings and errors and changes nothing else.

### Text and Runs

```rust
use snapfire_fsr_ogimage::{Element, Run};

let posted = Element::Text {
  tag: "p".into(),
  style: vec![("font-size".into(), "28px".into())],
  runs: vec![
    Run::new("Posted 15 September 2026 · "),
    Run::new("noticeboard.example").with("text-decoration", "underline").with("color", "#fdba74"),
  ],
};
```

A run inherits the text element's properties and adds its own. Box properties on a run, such as `padding`, have no effect. Whitespace collapses under `white-space: normal` and `nowrap`, as in CSS.

### Images

```rust
use snapfire_fsr_ogimage::{Element, ImageRef};

let logo = Element::image("img", ImageRef::Key("logo".into())).size(120.0, 120.0).with("border-radius", "60px");
let inline = Element::image("img", ImageRef::Bytes(std::fs::read("photo.jpg").unwrap())).with("object-fit", "cover");
```

`size` sets the `width` and `height` attributes. Without them an image lays out at its decoded size. A CSS `width` or `height` alone keeps its aspect ratio.

## Styling with the CSS Subset

### Layout

```rust
use snapfire_fsr_ogimage::Element;

let badge = Element::boxed("div")
  .with("position", "absolute")
  .with("right", "40px")
  .with("bottom", "40px")
  .with("padding", "8px 16px")
  .with("flex", "0 0 auto");
```

| Properties | Values |
| --- | --- |
| `display` | `flex`, `none` |
| `position` and `top`, `right`, `bottom`, `left`, `inset` | `relative`, `absolute`; lengths, `auto` |
| `width`, `height`, `min-*`, `max-*` | lengths, percentages, `auto`, `none` for `max-*` |
| `margin`, `padding` and their sides | lengths, percentages; `auto` for margin |
| `box-sizing`, `overflow` | `border-box`, `content-box`; `visible`, `hidden`, `clip` |
| `flex-direction`, `flex-wrap`, `flex-flow`, `flex`, `flex-grow`, `flex-shrink`, `flex-basis` | as CSS |
| `align-items`, `align-self`, `align-content`, `justify-content` | as CSS without `baseline` |
| `gap`, `row-gap`, `column-gap` | lengths, percentages |

### Paint

```rust
use snapfire_fsr_ogimage::Element;

let panel = Element::boxed("div")
  .with("background", "radial-gradient(circle at top left, #fde68a, transparent 60%), #111827")
  .with("border", "2px solid rgba(255,255,255,0.2)")
  .with("border-radius", "24px")
  .with("box-shadow", "0 10px 30px rgba(0,0,0,0.35), inset 0 0 0 1px #000")
  .with("transform", "rotate(-2deg)")
  .with("opacity", "0.9");
```

| Properties | Values |
| --- | --- |
| `background-color`, `color`, `opacity` | colors, a number or a percentage |
| `background-image`, `background` | `linear-gradient()`, `radial-gradient()`, `url(key)`, with a color last in `background` |
| `background-size`, `background-position`, `background-repeat` | as CSS without `space` and `round` |
| `border`, its sides, `border-width`, `border-style`, `border-color` | `solid` only |
| `border-radius` and its corners | lengths, percentages, `/` for elliptical corners |
| `box-shadow` | offsets, blur, spread, color, `inset`; several |
| `transform`, `transform-origin` | `translate`, `rotate`, `scale`, `skew`, `matrix` and their axes |
| `object-fit`, `object-position` | `fill`, `contain`, `cover`, `none`, `scale-down`; positions |

### Text

```rust
use snapfire_fsr_ogimage::Element;

let title = Element::text("h1", "A title that may run long")
  .with("font-family", "Fraunces, Inter")
  .with("font-size", "72px")
  .with("font-weight", "700")
  .with("line-height", "1.1")
  .with("letter-spacing", "-0.02em")
  .with("line-clamp", "2")
  .with("text-shadow", "0 2px 8px rgba(0,0,0,0.4)");
```

| Properties | Values |
| --- | --- |
| `font-family`, `font-size`, `font-weight`, `font-style` | as CSS; a generic family falls through to fallback |
| `line-height`, `letter-spacing` | `normal`, a number, lengths, percentages |
| `text-align`, `text-transform` | `left`, `right`, `center`, `justify`, `start`, `end`; `uppercase`, `lowercase`, `capitalize` |
| `white-space`, `word-break`, `overflow-wrap` | `normal`, `nowrap`, `pre`, `pre-wrap`; `break-all`, `break-word` |
| `text-overflow`, `line-clamp`, `-webkit-line-clamp` | `clip`, `ellipsis`; a line count |
| `text-decoration`, `text-decoration-line`, `text-decoration-color` | `underline`, `line-through` and a color |
| `text-shadow` | offsets, blur, color; several |

A clamped paragraph ends in `…`. When the last face has no ellipsis it ends in `...`. `nowrap` with `text-overflow: ellipsis` cuts the one line at the box's width.

### Lengths and Colors

```rust
use snapfire_fsr_ogimage::Element;

let side = Element::boxed("div").with("width", "calc(50% - 2rem)").with("padding", "2vh 1.5em").with("color", "hsl(220 60% 20% / 80%)");
```

Lengths are `px`, `em`, `rem`, `%`, `vw`, `vh`, `vmin`, `vmax`, `pt`, `pc`, `in`, `cm` and `mm`, with `calc()` over them. `vw` and `vh` are the card's size and `rem` is the root's font size. Colors are the CSS named colors, hex, `rgb()`, `rgba()`, `hsl()`, `hsla()`, `transparent` and `currentColor`. `inherit`, `initial` and `unset` work on the inherited properties.

## Adding Fonts

```rust
use snapfire_fsr_ogimage::Fonts;

let mut fonts = Fonts::new();
for path in ["fonts/Fraunces-Regular.woff2", "fonts/Fraunces-Bold.woff2"] {
  let added = fonts.add(std::fs::read(path).unwrap()).unwrap();
  println!("{} {}", added[0].family, added[0].weight);
}
```

TTF, OTF, TTC and WOFF2 files are read. Family, weight and style come from the file's own tables. `Fonts::new` holds the bundled Inter at 400 and 700, cut to Latin. Every face added comes before it. `fonts.faces()` lists them in fallback order. One `Fonts` serves every render and caches each glyph's raster across renders.

### Choosing a Face

Each character takes the first face that covers it:

1. the families in the element's `font-family`, in order;
2. a face added for the character's Unicode script;
3. every other family, in the order added;
4. the bundled Inter.

Within a family the weight and style closest to the one asked for win, by CSS's matching rules. A character no face covers warns and is left out.

### A Face for One Script

```rust
use snapfire_fsr_ogimage::Fonts;

let mut fonts = Fonts::new();
fonts.add_for_scripts(std::fs::read("fonts/NotoSansJP-Regular.otf").unwrap(), &["Han", "Hiragana", "Katakana"]).unwrap();
```

Scripts are named as Unicode names them, in full or by their four-letter code. Japanese text then takes this face wherever an element's own families do not cover it, while Latin text keeps the faces before it.

## Giving the Card Images

```rust
use snapfire_fsr_ogimage::{Element, ImageRef, Images};

let mut images = Images::new();
images.insert("logo", &std::fs::read("logo.png").unwrap()).unwrap();
let header = Element::boxed("div")
  .with("background-image", "url(logo)")
  .with("background-size", "contain")
  .with("background-repeat", "no-repeat")
  .child(Element::image("img", ImageRef::Key("logo".into())));
```

PNG and JPEG decode once on `insert`. A key the card names that `Images` does not hold is an error, whatever it looks like: a URL is a key like any other and nothing is fetched.

## Rendering

```rust
use snapfire_fsr_ogimage::{render, Card, Element, Fonts, Format, Images};

let card = Card::new(1200, 630, Element::boxed("div").with("background-color", "#111"));
let rendered = render(&card, &Fonts::new(), &Images::new(), Format::Png).unwrap();
assert_eq!(Format::Png.content_type(), "image/png");
```

`render` only reads its arguments, so one `Fonts` and one `Images` serve renders on several threads. A 1200x630 card takes a few milliseconds. PNG deflate runs on up to eight scoped threads.

### Reading the Warnings

```rust
use snapfire_fsr_ogimage::{render, Card, Element, Fonts, Format, Images};

let card = Card::new(400, 200, Element::boxed("div").with("filter", "blur(2px)").child(Element::text("p", "東京")));
let rendered = render(&card, &Fonts::new(), &Images::new(), Format::Png).unwrap();
for w in &rendered.warnings {
  println!("{}: {}", w.element, w.message);
}
```

This prints two warnings. `div(1)` says `filter` is skipped and `div(1) > p(1)` names `東京` with its code points, since no face covers it. A warning names the element by tag and position among its siblings.

### Choosing PNG or JPEG

```rust
use snapfire_fsr_ogimage::Format;

let png = Format::Png;
let jpeg = Format::Jpeg { quality: 85 };
```

PNG is lossless and written as RGB when every pixel is opaque. JPEG suits a photographic card and has no transparency, so transparent pixels are drawn over white.

## Error Handling

`render` returns `CardError`. Every variant names what to fix.

```rust
use snapfire_fsr_ogimage::{render, Card, CardError, Element, Fonts, Format, Images};

let card = Card::new(1200, 630, Element::boxed("div").with("color", "notacolor"));
match render(&card, &Fonts::new(), &Images::new(), Format::Png) {
  Ok(rendered) => drop(rendered),
  Err(CardError::Value { element, property, value, reason }) => eprintln!("{element}: {property}: {value}: {reason}"),
  Err(CardError::MissingImage { element, key }) => eprintln!("{element}: give an image for {key}"),
  Err(other) => eprintln!("{other}"),
}
```

`Value` is a value that does not parse, `MissingImage` a key with no image, `Image` bytes that do not decode, `Size` a side of 0 or past `MAX_SIDE` and `Depth` a tree nested deeper than 64 levels. `Fonts::add` returns `FontError` for a file that is not a font, a WOFF 1 file or an unknown script name.
