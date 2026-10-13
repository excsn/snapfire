# snapfire_sitemap: Sitemap Writing with Every Protocol Limit Enforced

[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

**snapfire_sitemap** writes `<urlset>` and `<sitemapindex>` documents, with the image, video, news and xhtml alternate-link extensions.

Write a sitemap with 50,001 URLs and Google will not tell you. No warning, no partial index, nothing in Search Console. It ignores the file and you find out weeks later as absent traffic.

Every limit in the protocol fails that way: silently and at a distance from the code that caused it. So every one of them is a constructor error here. Nothing is clamped, truncated or quietly dropped.

One dependency, `chrono`. No XML crate, no URL crate, no builder macro.

The [usage guide](README.USAGE.md) walks the tasks. The [API reference](API_REFERENCE.md) lists the surface.

## Install

```toml
[dependencies]
snapfire_sitemap = "0.1"
```

No feature flags.

## What it refuses

| Constraint | Limit | Error |
|---|---|---|
| URLs in a document | 50,000 | `TooManyUrls` |
| Children in an index | 50,000 | `TooManySitemaps` |
| Uncompressed document | 50MB | `DocumentTooLarge` |
| Location length | under 2,048 chars | `LocationTooLong` |
| Priority | 0.0 to 1.0 | `PriorityOutOfRange` |
| Images on one URL | 1,000 | `TooManyImages` |
| News entries per document | 1,000 | `TooMuchNews` |
| Video duration | 1 to 28,800 seconds | `VideoDurationOutOfRange` |
| Video rating | 0.0 to 5.0 | `VideoRatingOutOfRange` |
| Video tags | 32 | `TooManyVideoTags` |
| Video category length | 256 chars | `VideoCategoryTooLong` |

A location that fails to validate comes back as an error rather than vanishing from the output, which is the failure that makes a broken sitemap look like a working one.

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
