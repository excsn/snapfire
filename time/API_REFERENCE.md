# API Reference: snapfire_time

Time zone offsets and `Temporal` as JavaScript reads them, from a compiled IANA database or the host's zoneinfo.

## Contents

* [1. Features](#1-features)
* [2. Sources](#2-sources)
  * [Source](#source)
  * [choose](#choose)
  * [source](#source-1)
* [3. Offsets](#3-offsets)
  * [offset_in](#offset_in)
  * [local_offset](#local_offset)
  * [system_zone](#system_zone)
  * [zone_from_tz](#zone_from_tz)
* [4. Temporal](#4-temporal)
  * [Kind](#kind)
  * [Arg](#arg)
  * [Answer](#answer)
  * [temporal functions](#temporal-functions)
* [5. Error handling](#5-error-handling)
  * [Error](#error)

## 1. Features

* `tz_compiled`, default: the IANA database compiled into the binary.
* `tz_system`: the host's zoneinfo under `/usr/share/zoneinfo`, read when a zone is first used and cached.

## 2. Sources

### Source

Where zone rules are read from. `#[derive(Debug, Clone, Copy, PartialEq, Eq)]`: `Compiled`, `System`.

* `pub fn built(self) -> bool`: whether this build holds the source.
* `pub fn name(self) -> &'static str`: `"compiled"` or `"system"`.
* `pub fn feature(self) -> &'static str`: `"tz_compiled"` or `"tz_system"`.

### choose

* `pub fn choose(source: Source) -> Result<(), Error>`: the source every later lookup reads. Once per process, before the first lookup. `Error::NotBuilt` for a source the build lacks, `Error::AlreadyChosen` for a different source once one is in use.

### source

* `pub fn source() -> Result<Source, Error>`: the chosen source, else `Compiled` when built, else `System`. `Error::NoSource` when the build holds neither.

## 3. Offsets

### offset_in

* `pub fn offset_in(source: Source, zone: &str, ms: f64) -> Result<f64, Error>`: `getTimezoneOffset` of the instant `ms` in `zone`, in whole minutes, truncated toward zero. `Error::OutOfRange` for `ms` not finite or past 8.64e15 either side, `Error::UnknownZone` for a zone the source lacks.

### local_offset

* `pub fn local_offset(ms: f64) -> Result<f64, Error>`: `offset_in(source()?, system_zone(), ms)`. `NaN` for an instant `offset_in` refuses; 0 when the source lacks the system zone.

### system_zone

* `pub fn system_zone() -> &'static str`: `zone_from_tz` of `TZ`, else the host's zone, else `"UTC"`. Read once per process.

### zone_from_tz

* `pub fn zone_from_tz(tz: Option<&str>) -> Option<String>`: the zone a `TZ` value names, a leading `:` dropped and a path under a `zoneinfo/` directory read as the zone below it. `None` for none or an empty value.

## 4. Temporal

`snapfire_time::temporal`. A Temporal value is its `Kind` and its canonical string. Every function parses the strings it is handed and writes its result as one.

### Kind

`#[derive(Debug, Clone, Copy, PartialEq, Eq)]`: `Instant`, `PlainDate`, `PlainTime`, `PlainDateTime`, `ZonedDateTime`, `Duration`.

* `pub fn name(self) -> &'static str`: the type's name under `Temporal`.
* `pub fn of(name: &str) -> Option<Kind>`

### Arg

An argument. `#[derive(Debug, Clone, PartialEq)]`: `Undefined`, `Null`, `Bool(bool)`, `Num(f64)`, `BigInt(i128)`, `Str(String)`, `Obj(Vec<(String, Arg)>)`, `Temporal(Kind, String)`.

### Answer

A result. `#[derive(Debug, Clone, PartialEq)]`: `Undefined`, `Bool(bool)`, `Num(f64)`, `BigInt(i128)`, `Str(String)`, `Temporal(Kind, String)`.

### temporal functions

* `pub fn construct(kind: Kind, args: &[Arg]) -> Result<Answer, Error>`: `new Temporal.<kind>(...args)`.
* `pub fn call_static(kind: Kind, name: &str, args: &[Arg]) -> Result<Answer, Error>`: `from`, `compare`, `fromEpochMilliseconds` and `fromEpochNanoseconds`.
* `pub fn get(kind: Kind, value: &str, name: &str) -> Result<Answer, Error>`: a field. `Answer::Undefined` for one the kind lacks.
* `pub fn call(kind: Kind, value: &str, method: &str, args: &[Arg]) -> Result<Answer, Error>`: a method. `valueOf` is `Error::Type`.
* `pub fn now(name: &str, now_ms: f64, args: &[Arg]) -> Result<Answer, Error>`: `Temporal.Now.<name>(timeZone?)` at `now_ms`, in [`system_zone`](#system_zone) without a zone.
* A function reading a zone reads it from [`source`](#source-1).

## 5. Error handling

### Error

`#[derive(Debug, Clone, PartialEq, Eq)]`, `#[non_exhaustive]`, implements `std::error::Error`.

* `Error::NoSource`: the build holds neither source.
* `Error::NotBuilt(Source)`: a source the build lacks. Displays the feature that builds it in.
* `Error::AlreadyChosen(Source)`: `choose` after the named source was in use.
* `Error::UnknownZone(String)`: a zone the source does not know.
* `Error::OutOfRange`: an instant outside the range of a `Date`.
* `Error::Range(String)`: what JavaScript throws as a `RangeError`.
* `Error::Type(String)`: what JavaScript throws as a `TypeError`.
