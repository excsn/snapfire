# snapfire_fsr_sites guide

## Table of Contents

* [Core Concepts](#core-concepts)
* [Quick Start](#quick-start)
* [Writing the Table](#writing-the-table)
* [Pinning an Artifact](#pinning-an-artifact)
* [Deploying a Site](#deploying-a-site)
* [Error Handling](#error-handling)

## Core Concepts

* **Artifact.** The directory a site's build leaves behind, `config/` beside `app/`, the way `Host::from` reads a project. It is what the shell mounts.
* **Table.** The `[sites]` section of the shell's configuration: one `[sites.<name>]` per mounted site, a `root` versions resolve under and a `poll` interval.
* **Hash.** xxh3 over every file of the artifact in path order, name and bytes, dot entries and `target` skipped. The table may pin it; a directory whose hash differs is refused.
* **Reread.** The host rebuilds its tables through its reloader; this crate asks for it on `SIGHUP` and when a poll finds the table resolving differently from last time.

## Quick Start

```rust
use std::sync::Arc;
use snapfire_fsr_host::{Config, Host};

let root = std::path::PathBuf::from(".");
let builder = snapfire_fsr_sites::mount_all(Host::from(&root)?)?;
let reload_root = root.clone();
let host = Arc::new(
  builder
    .reloader(move || snapfire_fsr_sites::mount_all(Host::from(&reload_root)?).map_err(|e| snapfire_fsr_host::HostError::Value("sites".to_owned(), e.to_string())))
    .build()?,
);
let poll = Config::load(&root).ok().and_then(|c| snapfire_fsr_sites::poll_of(&c));
snapfire_fsr_sites::watch(host.clone(), root, poll);
host.serve("127.0.0.1:8100").await?;
```

`fsr serve` does exactly this over a shell with no Rust beside it.

## Writing the Table

```toml
[sites]
root = "/srv/sites"
poll = "30s"

[sites.billing]
artifact = "billing@1.4.2"

[sites.reports]
artifact = "../reports"
```

`billing@1.4.2` resolves to `<root>/billing/1.4.2` and reports version `1.4.2`; a path resolves against the project root and reports version `path`. A version without a `root` is refused when the configuration loads.

```rust
for site in snapfire_fsr_sites::resolve(&config)? {
  println!("{} at {} is {} ({})", site.name, site.artifact.display(), site.version, site.hash);
}
```

## Pinning an Artifact

```toml
[sites.billing]
artifact = "billing@1.4.2"
hash = "3a098783bbb3ebc5"
```

The hash is the one `hash_dir` computes and the report and `GET /__fsr/sites` print. A directory that hashes differently is refused before anything is mounted, so a table that names a version and its hash cannot mount bytes it did not mean.

```rust
let hash = snapfire_fsr_sites::hash_dir(std::path::Path::new("/srv/sites/billing/1.4.2"))?;
```

## Deploying a Site

A deploy is the artifact laid out under the root, the table's pointer moved and the host told. `watch` reloads on `SIGHUP` at once and, with `poll` set, rereads the table on that interval and reloads when a row's path, version or hash moved. Rereading an unchanged table does nothing, so a signal can be resent at any time, and an instance that missed one converges on the next poll.

```sh
kill -HUP $(pgrep portal_react_ts)
curl http://127.0.0.1:8100/__fsr/sites
```

A reload that is refused, a bad hash or a site that does not boot, leaves the running tables alone and logs why under the `fsr::sites` target.

## Error Handling

`SitesError::Host` is a `HostError` from loading or mounting; `SitesError::Artifact { name, message }` names the row whose artifact is missing, unreadable or hashes differently from its pin.

```rust
match snapfire_fsr_sites::mount_all(builder) {
  Ok(builder) => builder,
  Err(snapfire_fsr_sites::SitesError::Artifact { name, message }) => return refuse(format!("sites.{name}: {message}")),
  Err(e) => return refuse(e.to_string()),
}
```
