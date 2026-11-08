# snapfire_sitemap

[![Crates.io](https://img.shields.io/crates/v/snapfire_sitemap.svg)](https://crates.io/crates/snapfire_sitemap)
[![Docs.rs](https://docs.rs/snapfire_sitemap/badge.svg)](https://docs.rs/snapfire_sitemap)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

Writes XML sitemaps and sitemap indexes. It refuses any document a search engine would reject.

It writes `<urlset>` and `<sitemapindex>` documents, with the image, video, news and xhtml alternate-link extensions.

Search engines do not report a sitemap that breaks the protocol. A file with 50,001 URLs gets no warning and no partial index. Nothing shows up in Search Console either. Google ignores it and you find out weeks later from missing traffic.

This crate checks every protocol limit when you build the document and returns an error for any it breaks. It never clamps, truncates or drops a value to make a document fit.

Its only dependency is `chrono`.

* The task guide is [README.USAGE.md](README.USAGE.md).
* The call surface is [API_REFERENCE.md](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_sitemap = "0"
```

No feature flags.

## Quick start

```rust
use snapfire_sitemap::{ChangeFrequency, Url, UrlSet};

fn main() -> Result<(), snapfire_sitemap::Error> {
  let urls = vec![
    Url::builder("https://example.com/")
      .changefreq(ChangeFrequency::Weekly)
      .priority(0.8)
      .build()?,
    Url::builder("https://example.com/about").build()?,
  ];

  println!("{}", UrlSet::new(urls)?.to_xml()?);
  Ok(())
}
```

A priority of `1.5` fails `build` with `PriorityOutOfRange`. A 50,001st URL fails `UrlSet::new` with `TooManyUrls`.

## What it refuses

| Constraint | Limit | Error |
|---|---|---|
| URLs in a document | 50,000 | `TooManyUrls` |
| Children in an index | 50,000 | `TooManySitemaps` |
| Uncompressed document | 50 MiB (52,428,800 bytes) | `DocumentTooLarge` |
| Location length | under 2,048 chars | `LocationTooLong` |
| Priority | 0.0 to 1.0 | `PriorityOutOfRange` |
| Images on one URL | 1,000 | `TooManyImages` |
| News entries per document | 1,000 | `TooMuchNews` |
| Video duration | 1 to 28,800 seconds | `VideoDurationOutOfRange` |
| Video rating | 0.0 to 5.0 | `VideoRatingOutOfRange` |
| Video tags | 32 | `TooManyVideoTags` |
| Video category length | 256 chars | `VideoCategoryTooLong` |
| Video with neither `content_loc` nor `player_loc` | one is required | `VideoWithoutSource` |

An empty location is refused with `LocationEmpty`. A location that fails a check comes back as an error and is never left out of the output silently.

## What to reach for

| Problem | Piece |
|---|---|
| Write a plain sitemap | `Url::builder`, then `UrlSet::new` |
| Split a large site across files | `Sitemap`, then `SitemapIndex` |
| Attach images to a page | `Image`, then `UrlBuilder::images` |
| Attach video metadata | `Video::builder`, then `UrlBuilder::videos` |
| Declare a news article | `News`, `Publication`, then `UrlBuilder::news` |
| Point at translations of a page | `Alternate`, then `UrlBuilder::alternates` |
| Read a limit rather than hardcode it | `limit` |
| Know why a document was refused | `Error` |
