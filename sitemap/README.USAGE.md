# Usage Guide: snapfire_sitemap

How to build every document the sitemap protocol defines and what each limit refuses.

## Table of Contents

* [Core Concepts](#core-concepts)
* [Quick Start](#quick-start)
  * [A plain sitemap](#a-plain-sitemap)
  * [A sitemap index](#a-sitemap-index)
* [Building a Url](#building-a-url)
* [Supplying Dates](#supplying-dates)
* [Attaching Images](#attaching-images)
* [Attaching Videos](#attaching-videos)
* [Declaring a News Article](#declaring-a-news-article)
* [Pointing at Translations](#pointing-at-translations)
* [Splitting a Large Site](#splitting-a-large-site)
* [Reading the Limits](#reading-the-limits)
* [Error Handling](#error-handling)

## Core Concepts

* **Url**: one `<url>` entry: a location plus optional metadata and extensions.
* **UrlSet**: a `<urlset>` document, the file a crawler fetches.
* **Sitemap**: one `<sitemap>` entry inside an index, pointing at a `UrlSet` document.
* **SitemapIndex**: a `<sitemapindex>` document, a list of other documents.
* **loc**: a location. Must be non-empty and under 2,048 characters.
* **lastmod**: when the target last changed. The only metadata Google acts on.
* **changefreq**: a hint. Google ignores it; other crawlers may not.
* **priority**: a hint between 0.0 and 1.0, relative within your own site only. Google ignores it.
* **Date**: a W3C Datetime at either whole-day or whole-second precision.
* **Image**: an `<image:image>` extension entry. Only its `loc` still carries weight.
* **Video**: a `<video:video>` extension entry, requiring a thumbnail, a title, a description and a source.
* **News**: a `<news:news>` extension entry, naming a publication and a date.
* **Alternate**: an `<xhtml:link rel="alternate">`, naming this page in another language.
* **limit**: the protocol's numbers as constants, so a caller never hardcodes them.
* **Error**: a rejection. Every variant names the constraint that was violated.

## Quick Start

### A plain sitemap

```rust
use snapfire_sitemap::{ChangeFrequency, Url, UrlSet};

fn main() -> Result<(), snapfire_sitemap::Error> {
  let urls = vec![
    Url::builder("https://example.com/")
      .changefreq(ChangeFrequency::Weekly)
      .priority(0.8)
      .build()?,
    Url::builder("https://example.com/about")
      .changefreq(ChangeFrequency::Yearly)
      .build()?,
  ];

  println!("{}", UrlSet::new(urls)?.to_xml()?);
  Ok(())
}
```

### A sitemap index

```rust
use chrono::NaiveDate;
use snapfire_sitemap::{Sitemap, SitemapIndex};

fn main() -> Result<(), snapfire_sitemap::Error> {
  let index = SitemapIndex::new(vec![
    Sitemap::new("https://example.com/sitemap-core.xml")?,
    Sitemap::new("https://example.com/sitemap-2026.xml")?
      .lastmod(NaiveDate::from_ymd_opt(2026, 6, 1).unwrap()),
  ])?;

  println!("{}", index.to_xml()?);
  Ok(())
}
```

## Building a Url

Everything past the location is optional. The builder validates on `build`.

```rust
use snapfire_sitemap::{ChangeFrequency, Url};

let url = Url::builder("https://example.com/post/a-title")
  .changefreq(ChangeFrequency::Monthly)
  .priority(0.7)
  .build()?;
```

A priority outside 0.0 to 1.0 is refused rather than clamped, because a clamped value is a value you never chose:

```rust
assert!(Url::builder("https://example.com/").priority(1.5).build().is_err());
```

Read a location back with `loc` and a date back with `lastmod`:

```rust
assert_eq!(url.loc(), "https://example.com/post/a-title");
assert_eq!(url.lastmod(), None);
```

## Supplying Dates

`Date` takes whichever chrono type you already hold. A `NaiveDate` renders as a day, a `DateTime` as a whole second.

```rust
use chrono::{DateTime, NaiveDate, Utc};
use snapfire_sitemap::{Date, Url};

let day = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap();
let instant: DateTime<Utc> = Utc::now();

let by_day = Url::builder("https://example.com/a").lastmod(day).build()?;
let by_instant = Url::builder("https://example.com/b").lastmod(instant).build()?;
```

`DateTime<FixedOffset>` keeps its offset; `DateTime<Utc>` renders with a `Z`.

```rust
assert_eq!(Date::from(day).render(), "2026-06-01");
```

Pick the day form when you only know the day. A date padded out to midnight claims a precision you do not have.

A date that arrives as text, from a page's head or a response header, parses into the same two forms:

```rust
assert_eq!(Date::parse("2026-06-01").unwrap().render(), "2026-06-01");
assert_eq!(Date::parse("2026-06-01T09:30+02:00").unwrap().render(), "2026-06-01T09:30:00+02:00");
assert_eq!(Date::parse_http("Mon, 01 Jun 2026 09:30:00 GMT").unwrap().render(), "2026-06-01T09:30:00Z");
assert_eq!(Date::parse("2026-06"), None);
```

`as_instant` orders dates of either precision, a day counting from its midnight in UTC, which is how an index entry takes the newest `lastmod` of its document.

## Attaching Images

An image needs a location. The other four fields are still in the schema but Google dropped them in 2022.

```rust
use snapfire_sitemap::{Image, Url};

let url = Url::builder("https://example.com/gallery")
  .images(vec![
    Image::new("https://example.com/a.jpg")?,
    Image::new("https://example.com/b.jpg")?.title("A title"),
  ])
  .build()?;
```

Past 1,000 images on one url, `build` refuses:

```rust
let too_many: Vec<Image> = (0..1_001)
  .map(|n| Image::new(format!("https://example.com/{n}.jpg")).unwrap())
  .collect();

assert!(Url::builder("https://example.com/g").images(too_many).build().is_err());
```

## Attaching Videos

A video always needs a thumbnail, a title, a description and at least one of a content location or a player location.

```rust
use snapfire_sitemap::{Url, Video};

let video = Video::builder(
  "https://example.com/thumb.jpg",
  "Grilling a steak",
  "How to grill a steak",
)
.content_loc("https://example.com/video.mp4")
.duration(600)
.view_count(4_281)
.family_friendly(true)
.build()?;

let url = Url::builder("https://example.com/steak").videos(vec![video]).build()?;
```

Neither source set is the one structural error the extension can make:

```rust
use snapfire_sitemap::Error;

let missing = Video::builder("https://example.com/t.jpg", "T", "D").build();
assert_eq!(missing.unwrap_err(), Error::VideoWithoutSource);
```

Restrictions and platforms are lists carrying an allow-or-deny relationship and render space-delimited:

```rust
use snapfire_sitemap::{Platform, PlatformRestriction, Relationship, Restriction};

let video = Video::builder("https://example.com/t.jpg", "T", "D")
  .player_loc("https://example.com/player")
  .restriction(Restriction {
    relationship: Relationship::Allow,
    countries: vec!["US".to_string(), "CA".to_string()],
  })
  .platform(PlatformRestriction {
    relationship: Relationship::Deny,
    platforms: vec![Platform::Tv],
  })
  .build()?;
```

An uploader carries an optional info url, which must sit on the same domain as the page:

```rust
use snapfire_sitemap::Uploader;

let uploader = Uploader {
  name: "Norm".to_string(),
  info: Some("https://example.com/authors/norm".to_string()),
};
```

Duration is bounded at 1 to 28,800 seconds and rating at 0.0 to 5.0; both are refused out of range rather than clamped.

## Declaring a News Article

A news entry names its publication, the day it ran and its headline.

```rust
use chrono::NaiveDate;
use snapfire_sitemap::{News, Publication, Url};

let url = Url::builder("https://example.com/story")
  .news(News::new(
    Publication {
      name: "The Example Times".to_string(),
      language: "en".to_string(),
    },
    NaiveDate::from_ymd_opt(2026, 6, 1).unwrap(),
    "Companies A and B in merger talks",
  ))
  .build()?;
```

One news entry per url and at most 1,000 across a document. `UrlSet::new` counts them:

```rust
use snapfire_sitemap::{Error, UrlSet};

// A vector of 1,001 urls each carrying news.
assert!(matches!(UrlSet::new(many_news), Err(Error::TooMuchNews { .. })));
```

## Pointing at Translations

Each alternate names a language and the url serving it. List every version on every version, including the page itself.

```rust
use snapfire_sitemap::{Alternate, Url};

let url = Url::builder("https://example.com/english/")
  .alternates(vec![
    Alternate::new("en", "https://example.com/english/")?,
    Alternate::new("de", "https://example.com/deutsch/")?,
  ])
  .build()?;
```

These render directly after the `<loc>`, which is where Google's own examples put them.

## Splitting a Large Site

One document holds 50,000 urls or 50MB, whichever comes first. Past either, split and publish an index. Bucketing by year lets a crawler skip a year it has already seen:

```rust
use std::collections::BTreeMap;
use chrono::{DateTime, Datelike, Utc};
use snapfire_sitemap::{Sitemap, SitemapIndex, Url, UrlSet};

let mut years: BTreeMap<i32, Vec<Url>> = BTreeMap::new();

for (loc, published) in posts {
  let url = Url::builder(loc).lastmod(published).build()?;
  years.entry(published.year()).or_default().push(url);
}

let mut children = Vec::new();

for (year, urls) in years {
  let document = UrlSet::new(urls)?.to_xml()?;
  write_file(&format!("sitemap-{year}.xml"), &document);

  children.push(Sitemap::new(format!("https://example.com/sitemap-{year}.xml"))?);
}

let index = SitemapIndex::new(children)?.to_xml()?;
```

Give each child a `lastmod`. Without one the index tells a crawler nothing it could not work out by fetching everything.

An index holds 50,000 children, so two levels reach 2.5 billion urls. Nesting deeper is not allowed.

## Reading the Limits

Every number the protocol imposes is a constant, so a caller can check before building rather than after failing:

```rust
use snapfire_sitemap::limit;

if urls.len() > limit::MAX_URLS {
  // Split before constructing.
}
```

`MAX_URLS`, `MAX_SITEMAPS`, `MAX_DOCUMENT_BYTES`, `MAX_LOCATION_LEN`, `MIN_PRIORITY`, `MAX_PRIORITY`, `MAX_IMAGES_PER_URL`, `MAX_NEWS_PER_URL_SET`, `MAX_VIDEO_TAGS`, `MAX_VIDEO_CATEGORY_LEN`, `MIN_VIDEO_DURATION_SECONDS`, `MAX_VIDEO_DURATION_SECONDS`, `MIN_VIDEO_RATING` and `MAX_VIDEO_RATING`.

## The Argument

Two decisions are worth defending.

**Rejections are returned, never swallowed.** A sitemap that breaks a limit is not rejected loudly by a search engine; it is ignored. That makes silent construction the worst possible behaviour, because the failure surfaces weeks later as absent traffic. Every constraint is checked and every violation returns an `Error` naming it.

**A `loc` is a validated `String`, not a parsed URL type.** Parsing forces a dependency, rejects relative locations the protocol only discourages and turns a bad location into a dropped entry rather than a reported one. The protocol says two things about a `loc`: it is non-empty and it is under 2,048 characters. Both are checked directly. The count is in characters rather than bytes.

Namespaces are declared only when used, so a site with no video keeps a document that reads like a plain sitemap.

## Error Handling

Every fallible call returns `Result<T, Error>`. The variants name their constraint and carry the offending value.

```rust
use snapfire_sitemap::{Error, UrlSet};

match UrlSet::new(urls).and_then(|set| set.to_xml()) {
  Ok(xml) => serve(xml),
  Err(Error::TooManyUrls { count }) => split_into_an_index(count),
  Err(Error::DocumentTooLarge { bytes }) => split_into_an_index(bytes),
  Err(Error::TooMuchNews { count }) => drop_the_oldest_news(count),
  Err(error) => log::error!("sitemap rejected: {error}"),
}
```

`LocationEmpty`, `LocationTooLong` and `PriorityOutOfRange` come from building a `Url`, an `Image`, an `Alternate` or a `Sitemap`. `TooManyImages`, `TooManyVideoTags`, `VideoCategoryTooLong`, `VideoDurationOutOfRange`, `VideoRatingOutOfRange` and `VideoWithoutSource` come from building their extension. `TooManyUrls`, `TooMuchNews` and `TooManySitemaps` come from constructing a document. `DocumentTooLarge` comes from rendering one.
