# snapfire_time

[![Crates.io](https://img.shields.io/crates/v/snapfire_time.svg)](https://crates.io/crates/snapfire_time)
[![Docs.rs](https://docs.rs/snapfire_time/badge.svg)](https://docs.rs/snapfire_time)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

**snapfire_time** answers time questions the way JavaScript does. It gives `getTimezoneOffset` for any instant in the zone `TZ` names or the host's own. It gives the daylight saving in that offset as ICU counts it. It runs the `Temporal` global's types: `Instant`, `PlainDate`, `PlainTime`, `PlainDateTime`, `ZonedDateTime` and `Duration`. It works through [`temporal_rs`](https://crates.io/crates/temporal_rs), the Temporal implementation V8 uses, so its answers match node's.

You pick where the zone rules come from:

* `tz_compiled`, on by default, builds the IANA database into the binary. Every host answers the same, slim containers included. `cargo update -p jiff-tzdb` takes new rules without a new release of this crate.
* `tz_system` reads the host's zoneinfo under `/usr/share/zoneinfo` when a zone is first used. The binary is smaller and zones follow the host's tzdata.

Build in either or both. With both, `choose` picks one at startup.

* The task guide is [README.USAGE.md](README.USAGE.md).
* The call surface is [API_REFERENCE.md](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_time = "0"
```

For the host's zoneinfo alone:

```toml
[dependencies]
snapfire_time = { version = "0", default-features = false, features = ["tz_system"] }
```

## What it costs

Measured against a binary already carrying chrono and ICU4X's calendar, cold release build, stripped:

| Source | Build time | Binary size |
|---|---|---|
| `tz_compiled` | +3.0 s | +407 KiB |
| `tz_system` | +3.2 s | +165 KiB |

## License

MPL-2.0, in [LICENSE](LICENSE). The time zone database `tz_compiled` builds in is in the public domain.
