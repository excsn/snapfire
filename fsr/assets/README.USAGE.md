# Usage Guide: snapfire_fsr_assets

How to turn one image into its variants and one font into the CSS that serves it without a layout shift.

## Table of Contents

* [Core Concepts](#core-concepts)
* [Quick Start](#quick-start)
* [Choosing the Variants](#choosing-the-variants)
* [Generating a Variant](#generating-a-variant)
* [Reading a Font](#reading-a-font)
* [Writing the Fallback Face](#writing-the-fallback-face)
* [Naming What Is Emitted](#naming-what-is-emitted)
* [Error Handling](#error-handling)

## Core Concepts

* **Variant**: one image at one width in one format, resized from the source and encoded at a quality.
* **VariantPolicy**: the widths, the formats and a quality per format one build uses for every image.
* **Never upscale**: a source gets every policy width below its own, then its own; no variant is wider than the file it came from.
* **Format**: `avif` or `webp`. The original stays as the `<img>` a browser with neither falls back to.
* **Passthrough**: an SVG, an animated GIF or an APNG is served as it is and gets no variant.
* **Header**: what an image's header says, read through `snapfire_media`: the displayed width and height and the EXIF orientation they account for.
* **Face**: one font file read for its family, weight, style and metrics, through `snapfire_media`.
* **Metrics**: units per em, ascender, descender, line gap and the frequency-weighted advance of `a` to `z`.
* **Fallback**: a face a visitor already has, with the metrics a fallback `@font-face` is computed against.
* **Hash**: eight hex digits of xxh3, the digest snapfirec names an emitted asset with.

## Quick Start

```rust
use snapfire_fsr_assets::{Face, Source, VariantPolicy, font, variant_name, hash};
use std::path::Path;

fn main() -> Result<(), snapfire_fsr_assets::Error> {
  let policy = VariantPolicy::default();
  let bytes = std::fs::read("app/img/hero.png").map_err(|e| snapfire_fsr_assets::Error::Io("app/img/hero.png".into(), e))?;
  let digest = hash::of(&bytes);
  let source = Source::from_bytes(Path::new("app/img/hero.png"), &bytes)?;
  for width in policy.widths_for(source.width()) {
    for format in &policy.formats {
      let encoded = source.variant(width, *format, policy.quality(*format))?;
      std::fs::write(format!("dist/img/{}", variant_name("hero", &digest, width, *format)), encoded).unwrap();
    }
  }

  let inter = Face::read(Path::new("app/fonts/Inter-Regular.woff2"))?;
  let css = inter.fallback_face("Inter Fallback", font::fallback("Arial").unwrap()).expect("Inter has a to z");
  println!("{css}");
  Ok(())
}
```

## Choosing the Variants

The default policy is five widths, two formats and one quality per format:

```rust
use snapfire_fsr_assets::{Format, VariantPolicy};

let policy = VariantPolicy::default();
assert_eq!(policy.widths, [640, 960, 1280, 1920, 2560]);
assert_eq!(policy.formats, [Format::Avif, Format::Webp]);
assert_eq!(policy.quality(Format::Avif), 60);
assert_eq!(policy.quality(Format::Webp), 80);
```

`widths_for` applies the never-upscale rule, so the list depends on the source:

```rust
assert_eq!(policy.widths_for(400), [400]);
assert_eq!(policy.widths_for(4000), [640, 960, 1280, 1920, 2560, 4000]);
```

A per-image override is the same policy with a field replaced:

```rust
let thumbnails = VariantPolicy::default().with_widths(vec![160, 320]);
let hero = VariantPolicy::default().with_quality(Format::Avif, 75);
```

The policy serialises as the `[images]` table an application writes, so one struct is read from configuration and handed to every caller.

## Generating a Variant

Decode once, then resize and encode per width and format:

```rust
use snapfire_fsr_assets::{Format, Source};
use std::path::Path;

let source = Source::open(Path::new("app/img/hero.png"))?;
let avif = source.variant(640, Format::Avif, 60)?;
let webp = source.variant(640, Format::Webp, 80)?;
assert_eq!(source.height_at(640), source.height() * 640 / source.width());
```

A width at or past the source's own is encoded without resizing. Resizing is Lanczos3 through `fast_image_resize`. A photo carrying an EXIF orientation is decoded upright, so a variant, which carries no tag, shows the way the browser shows the original:

```rust
let photo = Source::open(Path::new("app/img/photo.jpg"))?;
// stored 4000x3000 with orientation 6
assert_eq!((photo.width(), photo.height()), (3000, 4000));
```

Before decoding, ask whether the file should be touched at all:

```rust
use snapfire_fsr_assets::image::{dimensions, passthrough};

let (width, height) = dimensions(Path::new("app/img/hero.png"))?;
// the displayed size, so a tagged photo reports 3000x4000 here too
if passthrough(Path::new("app/img/spinner.gif"))? {
  // served as it is, with width and height only
}
```

## Reading a Font

`Face::read` unpacks a woff2 or a woff to the sfnt inside and reads a ttf or an otf as it is:

```rust
use snapfire_fsr_assets::{Face, Style};
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
use snapfire_fsr_assets::{Face, font};
use std::path::Path;

let inter = Face::read(Path::new("app/fonts/Inter-Regular.woff2"))?;
let arial = font::fallback("Arial").expect("a known fallback");
let css = inter.fallback_face("Inter Fallback", arial).expect("Inter has a to z");
// @font-face { font-family: "Inter Fallback"; src: local("Arial"); size-adjust: 108.20%; ascent-override: 89.53%; descent-override: 22.32%; line-gap-override: 0.00%; }
```

`size-adjust` is the ratio of the two faces' frequency-weighted average advances; the three overrides are the real face's vertical metrics divided by that scale. A subset with no `a` to `z` glyphs has no average advance and `fallback_face` is `None` for it. The fallbacks on offer are the ones `font::fallbacks` lists: Arial, Arial Black, Helvetica, Verdana, Tahoma, Trebuchet MS, Georgia, Times New Roman and Courier New, with metrics read from the files macOS and Windows ship.

## Naming What Is Emitted

The hash is the one snapfirec uses, so a file hashed here and one it hashed agree:

```rust
use snapfire_fsr_assets::{Format, hash, variant_name};

let digest = hash::of(b"...bytes...");
assert_eq!(digest.len(), 8);
assert_eq!(hash::emitted_name("hero", &digest, "png"), format!("hero.{digest}.png"));
assert_eq!(variant_name("hero", &digest, 640, Format::Avif), format!("hero.{digest}.640.avif"));
```

## Error Handling

Every function returns `snapfire_fsr_assets::Error`. The variants name the file:

```rust
use snapfire_fsr_assets::{Error, Source};
use std::path::Path;

match Source::open(Path::new("app/img/hero.png")) {
  Ok(source) => { let _ = source; }
  Err(Error::Io(path, e)) => eprintln!("could not read {}: {e}", path.display()),
  Err(Error::Decode(path, why)) => eprintln!("{} is not an image: {why}", path.display()),
  Err(other) => eprintln!("{other}"),
}
```

`Encode` names the format that failed and `Media` carries `snapfire_media`'s error for a header or a face that could not be read.
