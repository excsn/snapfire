# snapfire_fsr_ogimage

MPL-2.0; the bundled Inter is OFL-1.1.

Renders a card, a tree of boxes, text and images styled with a subset of CSS, to a PNG or a JPEG. Layout is flexbox through taffy, text is shaped with rustybuzz and broken into lines with the Unicode line breaking rules and pixels are drawn with tiny-skia. The subset is the one satori supports, so a card written for Next's `ImageResponse` lays out the same. The guide is [README.USAGE.md](README.USAGE.md) and the surface is [API_REFERENCE.md](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_fsr_ogimage = "0.1"
```

No features. Everything is pure Rust. A stripped release binary grows by about 1.9 MB.

| To | Reach for |
| --- | --- |
| Describe a card | `Card`, `Element::boxed`, `Element::text`, `Element::image` |
| Style an element | `Element::with`, CSS declarations as written |
| Give text more than one style | `Run` inside `Element::Text` |
| Use an application's fonts | `Fonts::add` |
| Use a face for one script, such as CJK | `Fonts::add_for_scripts` |
| Give the card its images | `Images::insert`, then `ImageRef::Key` or `url(key)` |
| Render to bytes | `render` with `Format::Png` or `Format::Jpeg` |
| See what was skipped | `Rendered::warnings` |
