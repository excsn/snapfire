# API Reference: snapfire_sitemap

Writes sitemap and sitemap index documents with the protocol's limits enforced.

## Contents

* [1. Documents](#1-documents)
  * [UrlSet](#urlset)
  * [SitemapIndex](#sitemapindex)
  * [Sitemap](#sitemap)
* [2. Urls](#2-urls)
  * [Url](#url)
  * [UrlBuilder](#urlbuilder)
  * [ChangeFrequency](#changefrequency)
* [3. Dates](#3-dates)
  * [Date](#date)
* [4. Image Extension](#4-image-extension)
  * [Image](#image)
* [5. Video Extension](#5-video-extension)
  * [Video](#video)
  * [VideoBuilder](#videobuilder)
  * [Restriction](#restriction)
  * [PlatformRestriction](#platformrestriction)
  * [Relationship](#relationship)
  * [Platform](#platform)
  * [Uploader](#uploader)
* [6. News Extension](#6-news-extension)
  * [News](#news)
  * [Publication](#publication)
* [7. Alternate Links](#7-alternate-links)
  * [Alternate](#alternate)
* [8. Limits and Namespaces](#8-limits-and-namespaces)
* [9. Error Handling](#9-error-handling)

## 1. Documents

### UrlSet

A `<urlset>` document.

* `UrlSet::new(urls: Vec<Url>) -> Result<UrlSet>`: fails with `TooManyUrls` past `MAX_URLS` and with `TooMuchNews` past `MAX_NEWS_PER_URL_SET` urls carrying news.
* `to_xml(&self) -> Result<String>`: renders. Fails with `DocumentTooLarge` past `MAX_DOCUMENT_BYTES`. Declares only the namespaces the contained urls use.
* `urls(&self) -> &[Url]`
* `len(&self) -> usize`
* `is_empty(&self) -> bool`

An empty `UrlSet` renders a well-formed document with no `<url>` children.

### SitemapIndex

A `<sitemapindex>` document.

* `SitemapIndex::new(sitemaps: Vec<Sitemap>) -> Result<SitemapIndex>`: fails with `TooManySitemaps` past `MAX_SITEMAPS`.
* `to_xml(&self) -> Result<String>`: fails with `DocumentTooLarge` past `MAX_DOCUMENT_BYTES`.
* `sitemaps(&self) -> &[Sitemap]`
* `len(&self) -> usize`
* `is_empty(&self) -> bool`

An index may not contain another index.

### Sitemap

One `<sitemap>` entry of an index.

* `Sitemap::new(loc: impl Into<String>) -> Result<Sitemap>`: validates the location.
* `lastmod(self, at: impl Into<Date>) -> Sitemap`
* `loc(&self) -> &str`

## 2. Urls

### Url

One `<url>` entry. Built through `UrlBuilder`; the fields are private and validated.

* `Url::builder(loc: impl Into<String>) -> UrlBuilder`
* `loc(&self) -> &str`
* `lastmod(&self) -> Option<Date>`

Elements render in the order `loc`, alternates, `lastmod`, `changefreq`, `priority`, images, videos, news.

### UrlBuilder

* `lastmod(self, at: impl Into<Date>) -> UrlBuilder`
* `changefreq(self, changefreq: ChangeFrequency) -> UrlBuilder`
* `priority(self, priority: f32) -> UrlBuilder`
* `alternates(self, alternates: Vec<Alternate>) -> UrlBuilder`
* `images(self, images: Vec<Image>) -> UrlBuilder`
* `videos(self, videos: Vec<Video>) -> UrlBuilder`
* `news(self, news: News) -> UrlBuilder`: at most one per url.
* `build(self) -> Result<Url>`: validates the location, `MIN_PRIORITY..=MAX_PRIORITY` and `MAX_IMAGES_PER_URL`.

### ChangeFrequency

`Always`, `Hourly`, `Daily`, `Weekly`, `Monthly`, `Yearly`, `Never`. Renders lowercase.

## 3. Dates

### Date

A W3C Datetime at one of two precisions.

* `Date::Day(NaiveDate)`: renders `%Y-%m-%d`.
* `Date::Instant(DateTime<FixedOffset>)`: renders RFC 3339 to whole seconds.
* `render(&self) -> String`
* `Date::parse(text: &str) -> Option<Date>`: a W3C Datetime as a page writes one, a day or an instant with an offset to the minute or past it; fractional seconds are dropped on render. `None` for a year or month alone, an instant with no offset and anything else.
* `Date::parse_http(text: &str) -> Option<Date>`: an HTTP date, the `Last-Modified` form, as an instant.
* `as_instant(&self) -> DateTime<FixedOffset>`: the instant the date starts at, a day at midnight UTC, for ordering dates of either precision.

`From` is implemented for `NaiveDate`, `DateTime<FixedOffset>` and `DateTime<Utc>`. A `DateTime<Utc>` renders with a `Z` suffix; a `DateTime<FixedOffset>` keeps its offset.

## 4. Image Extension

### Image

An `<image:image>` entry. Only `loc` is acted on by Google; the rest remain in the schema.

* `Image::new(loc: impl Into<String>) -> Result<Image>`
* `caption(self, caption: impl Into<String>) -> Image`
* `geo_location(self, geo_location: impl Into<String>) -> Image`
* `title(self, title: impl Into<String>) -> Image`
* `license(self, license: impl Into<String>) -> Result<Image>`: validated as a location.
* `loc(&self) -> &str`

## 5. Video Extension

### Video

A `<video:video>` entry.

* `Video::builder(thumbnail_loc, title, description) -> VideoBuilder`: all three `impl Into<String>`, all three required.
* `title(&self) -> &str`

### VideoBuilder

* `content_loc(self, content_loc: impl Into<String>) -> VideoBuilder`
* `player_loc(self, player_loc: impl Into<String>) -> VideoBuilder`
* `duration(self, seconds: u32) -> VideoBuilder`
* `expiration_date(self, at: impl Into<Date>) -> VideoBuilder`
* `rating(self, rating: f32) -> VideoBuilder`
* `view_count(self, view_count: u64) -> VideoBuilder`
* `publication_date(self, at: impl Into<Date>) -> VideoBuilder`
* `family_friendly(self, family_friendly: bool) -> VideoBuilder`: renders `yes` or `no`.
* `restriction(self, restriction: Restriction) -> VideoBuilder`
* `platform(self, platform: PlatformRestriction) -> VideoBuilder`
* `requires_subscription(self, requires_subscription: bool) -> VideoBuilder`
* `uploader(self, uploader: Uploader) -> VideoBuilder`
* `live(self, live: bool) -> VideoBuilder`
* `tags(self, tags: Vec<String>) -> VideoBuilder`
* `category(self, category: impl Into<String>) -> VideoBuilder`
* `build(self) -> Result<Video>`

`build` requires at least one of `content_loc` or `player_loc` and enforces `MAX_VIDEO_TAGS`, `MAX_VIDEO_CATEGORY_LEN`, `MIN_VIDEO_DURATION_SECONDS..=MAX_VIDEO_DURATION_SECONDS` and `MIN_VIDEO_RATING..=MAX_VIDEO_RATING`. Category length is counted in characters.

### Restriction

* `relationship: Relationship`
* `countries: Vec<String>`: uppercase ISO 3166 codes, rendered space-delimited.

### PlatformRestriction

* `relationship: Relationship`
* `platforms: Vec<Platform>`: rendered space-delimited.

### Relationship

`Allow`, `Deny`. Renders as the `relationship` attribute.

### Platform

`Web`, `Mobile`, `Tv`. Renders lowercase.

### Uploader

* `name: String`
* `info: Option<String>`: rendered as the `info` attribute; must share the page's domain.

## 6. News Extension

### News

A `<news:news>` entry.

* `News::new(publication: Publication, publication_date: impl Into<Date>, title: impl Into<String>) -> News`: infallible.
* `title(&self) -> &str`

### Publication

* `name: String`
* `language: String`: ISO 639, with `zh-cn` and `zh-tw` the accepted exceptions.

## 7. Alternate Links

### Alternate

An `<xhtml:link rel="alternate">`, rendered as an empty element directly after the `<loc>`.

* `Alternate::new(hreflang: impl Into<String>, href: impl Into<String>) -> Result<Alternate>`: validates `href` as a location.
* `hreflang(&self) -> &str`
* `href(&self) -> &str`

## 8. Limits and Namespaces

`limit` holds every bound the protocol imposes.

| Constant | Value |
|---|---|
| `MAX_URLS` | 50,000 |
| `MAX_SITEMAPS` | 50,000 |
| `MAX_DOCUMENT_BYTES` | 52,428,800 |
| `MAX_LOCATION_LEN` | 2,048 |
| `MIN_PRIORITY` | 0.0 |
| `MAX_PRIORITY` | 1.0 |
| `MAX_IMAGES_PER_URL` | 1,000 |
| `MAX_NEWS_PER_URL_SET` | 1,000 |
| `MAX_VIDEO_TAGS` | 32 |
| `MAX_VIDEO_CATEGORY_LEN` | 256 |
| `MIN_VIDEO_DURATION_SECONDS` | 1 |
| `MAX_VIDEO_DURATION_SECONDS` | 28,800 |
| `MIN_VIDEO_RATING` | 0.0 |
| `MAX_VIDEO_RATING` | 5.0 |

`MAX_LOCATION_LEN` is exclusive: a location of exactly 2,048 characters is refused. Location length is counted in characters, not bytes. `MAX_DOCUMENT_BYTES` applies to the uncompressed document and is measured in bytes.

The namespace URIs are exported as `SITEMAP`, `IMAGE`, `VIDEO`, `NEWS` and `XHTML`.

## 9. Error Handling

`Result<T>` is `std::result::Result<T, Error>`. `Error` implements `Display`, `Debug`, `Clone`, `PartialEq` and `std::error::Error`.

| Variant | Raised by |
|---|---|
| `LocationEmpty` | any location |
| `LocationTooLong { length }` | any location |
| `PriorityOutOfRange { priority }` | `UrlBuilder::build` |
| `TooManyUrls { count }` | `UrlSet::new` |
| `TooManySitemaps { count }` | `SitemapIndex::new` |
| `TooManyImages { count }` | `UrlBuilder::build` |
| `TooMuchNews { count }` | `UrlSet::new` |
| `TooManyVideoTags { count }` | `VideoBuilder::build` |
| `VideoCategoryTooLong { length }` | `VideoBuilder::build` |
| `VideoDurationOutOfRange { seconds }` | `VideoBuilder::build` |
| `VideoRatingOutOfRange { rating }` | `VideoBuilder::build` |
| `VideoWithoutSource` | `VideoBuilder::build` |
| `DocumentTooLarge { bytes }` | `UrlSet::to_xml`, `SitemapIndex::to_xml` |

No call panics and no constraint is silently clamped or dropped.
