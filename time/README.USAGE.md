# Usage Guide: snapfire_time

How to read time zone offsets and run `Temporal` with snapfire_time and how to pick where its zone rules come from.

## Table of Contents

* [Core Concepts](#core-concepts)
* [Quick Start](#quick-start)
* [Reading an Offset](#reading-an-offset)
* [Reading This Machine's Offset](#reading-this-machines-offset)
* [Running Temporal](#running-temporal)
* [Choosing the Zone Source](#choosing-the-zone-source)
* [Error Handling](#error-handling)

## Core Concepts

* **Offset**: what `Date.prototype.getTimezoneOffset` answers, the minutes to add to local time for UTC: 240 for New York in summer, -330 for India.
* **Instant**: milliseconds since 1970-01-01 UTC, as a `Date` holds them, at most 8.64e15 either side.
* **Zone**: an IANA identifier such as `America/New_York` or `UTC`.
* **System zone**: the zone `TZ` names, else the host's own, else `UTC`.
* **Source**: where zone rules are read from, the compiled database or the host's zoneinfo.
* **Temporal value**: one of `Temporal`'s types held as its kind and its canonical string, `(PlainDate, "2024-02-29")`.
* **Arg** and **Answer**: what a Temporal call takes and gives, JavaScript's values without a JavaScript engine.

## Quick Start

```rust
use snapfire_time::{local_offset, offset_in, Source};

fn main() {
  assert_eq!(offset_in(Source::Compiled, "America/New_York", 1_726_011_440_123.0), Ok(240.0));
  println!("here: {}", local_offset(1_726_011_440_123.0).unwrap());
}
```

## Reading an Offset

`offset_in` takes the source, the zone and the instant:

```rust
use snapfire_time::{offset_in, Source};

assert_eq!(offset_in(Source::Compiled, "Asia/Kolkata", 0.0), Ok(-330.0));
assert_eq!(offset_in(Source::Compiled, "Europe/Berlin", 1_704_067_200_000.0), Ok(-60.0));
```

A zone whose offset has seconds, a city's mean time before standard time, is cut to whole minutes as V8 cuts it: Kolkata in 1900 is -321.

## Reading This Machine's Offset

`local_offset` reads the system zone from the source in use. An invalid instant is `NaN`. A `TZ` naming a zone the source does not know reads as `UTC`, as in node:

```rust
use snapfire_time::{local_offset, system_zone};

println!("{} is {} minutes from UTC", system_zone(), local_offset(0.0).unwrap());
assert!(local_offset(f64::NAN).unwrap().is_nan());
```

`TZ` is read as node reads it: `:America/New_York` and `/usr/share/zoneinfo/America/New_York` both name `America/New_York`.

## Running Temporal

`snapfire_time::temporal` runs `Temporal` over values held as strings. `construct` is `new`, `call_static` the type's own functions, `get` a field read and `call` a method:

```rust
use snapfire_time::temporal::{call, call_static, get, Answer, Arg, Kind};

let due = call_static(Kind::PlainDate, "from", &[Arg::Str("2024-01-31".into())]).unwrap();
let Answer::Temporal(kind, text) = due else { unreachable!() };
let next = call(kind, &text, "add", &[Arg::Obj(vec![("months".into(), Arg::Num(1.0))])]).unwrap();
assert_eq!(next, Answer::Temporal(Kind::PlainDate, "2024-02-29".into()));
assert_eq!(get(Kind::PlainDate, "2024-02-29", "dayOfWeek").unwrap(), Answer::Num(4.0));
```

`now` answers `Temporal.Now` at an instant the caller passes, so a host reads its own clock once:

```rust
use snapfire_time::temporal::{now, Answer, Arg};

let at = now("plainDateISO", 1_726_011_440_123.0, &[Arg::Str("Asia/Tokyo".into())]).unwrap();
assert!(matches!(at, Answer::Temporal(_, text) if text == "2024-09-11"));
```

A value out of range or a string that does not parse is `Error::Range`. An argument of the wrong kind is `Error::Type`.

## Choosing the Zone Source

With one source built in there is nothing to choose. With both, call `choose` before the first lookup; without a call the compiled database answers:

```rust
use snapfire_time::{choose, source, Source};

choose(Source::System).unwrap();
assert_eq!(source(), Ok(Source::System));
```

`Source::built` says whether this build holds a source. `Source::feature` names the feature that builds it in.

## Error Handling

Every call returns `Result<_, snapfire_time::Error>`:

```rust
use snapfire_time::{offset_in, Error, Source};

match offset_in(Source::Compiled, "Mars/Olympus", 0.0) {
  Err(Error::UnknownZone(zone)) => assert_eq!(zone, "Mars/Olympus"),
  Err(Error::NotBuilt(source)) => eprintln!("enable `{}`", source.feature()),
  other => panic!("{other:?}"),
}
```

`Error::NoSource` means the build holds neither source. `Error::AlreadyChosen` means `choose` came after the source was in use. `Error::OutOfRange` is an instant a `Date` cannot hold.
