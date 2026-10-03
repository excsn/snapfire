# snapfire_media

MPL-2.0. Status: the header and face reads snapfirec and FSR's asset pipeline share; nothing here decodes a pixel or writes a file.

What an image or a font file says about itself. An image's header gives the size it displays at and the EXIF orientation that size accounts for, read from a JPEG's APP1 segment, a PNG's `eXIf` chunk or a WebP's `EXIF` chunk. A font's tables give its family, weight, style and the metrics a fallback face is sized by, with a woff2 or a woff unpacked first. Two tools read the same file through this crate, so the width a compiled module carries and the width a server writes on the element are the same number. The guide is [README.USAGE.md](README.USAGE.md) and the surface is [API_REFERENCE.md](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_media = "0.1"
```

No features. The header read is `imagesize` and `kamadak-exif`; the face read is `wuff` and `ttf-parser`.

| To | Reach for |
| --- | --- |
| Read a displayed width and height without decoding | `Header::read` |
| Know whether a file's pixels are stored on their side | `Header::transposes` |
| Read the orientation tag alone | `image::orientation` |
| Read a woff2, woff, ttf or otf for its family, weight and metrics | `Face::read` |
| Write the fallback face that stops text reflowing | `Face::fallback_face` with `font::fallback` |

## Status

Built for snapfirec's build facts and `snapfire_fsr_assets`, which re-exports it. The orientation is read, never applied: a consumer that resizes applies it to the pixels itself.
