# snapfire_fsr_sites

[![Crates.io](https://img.shields.io/crates/v/snapfire_fsr_sites.svg)](https://crates.io/crates/snapfire_fsr_sites)
[![Docs.rs](https://docs.rs/snapfire_fsr_sites/badge.svg)](https://docs.rs/snapfire_fsr_sites)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

Finds, checks and mounts the sites an FSR shell serves, then swaps in new versions without a restart.

A shell is an FSR application that mounts other FSR applications, called sites, under its own prefixes. The `[sites]` table in its configuration names each site's artifact by version or by path:

```toml
[sites]
root = "/srv/sites"
poll = "30s"

[sites.billing]
artifact = "billing@1.4.2"
```

This crate resolves each row to a directory and hashes it. A row that pins a `hash` is refused when the directory hashes differently. A version the cache lacks can be fetched from a store first. Then every site is mounted on the host. To deploy a new version, change the row and send `SIGHUP`. With `poll` set, the host also notices on its own.

Most applications never depend on this crate. `fsr serve` uses it in development and the `fsr-server` binary uses it in production. Use it directly when your application has its own host crate and mounts sites.

* The task guide is [README.USAGE.md](README.USAGE.md).
* The call surface is [API_REFERENCE.md](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_fsr_sites = "0"
```

Feature `http` adds a store that fetches over HTTP. Feature `reload_route` adds `POST /__fsr/sites/reload` to the host. Feature `server` builds the `fsr-server` binary.

## Quick start

```rust
use std::sync::Arc;
use snapfire_fsr_host::{Config, Host};

let root = std::path::PathBuf::from(".");
let builder = snapfire_fsr_sites::mount_all(snapfire_fsr_sites::mountable(Host::from(&root)?))?;
let host = Arc::new(builder.build()?);
let poll = Config::load(&root).ok().and_then(|c| snapfire_fsr_sites::poll_of(&c));
snapfire_fsr_sites::watch(host.clone(), root, poll);
host.serve("127.0.0.1:8100").await?;
```

## What it covers

| Task | Call |
| --- | --- |
| Mount every site the configuration names | `mount_all` over a `HostBuilder` |
| See where each row resolves and what it hashes to | `resolve` |
| Hash an artifact the way the table pins it | `hash_dir` |
| Lay a project out as a deploy tree | `layout`, `parts` |
| Pack an artifact, unpack it and check it against its manifest | `pack`, `unpack`, `Manifest::verify` |
| Fetch a missing version from a directory of archives or an HTTP server | `[sites] store`, `TarStore`, `HttpStore`, `fetch_missing` |
| Fetch from a registry of your own | a `Store` passed to `mount_all_with` or `mountable_with` |
| Reread the table on `SIGHUP` or a poll and reload the host | `watch`, `poll_of` |
| Run the stock host the way `fsr serve` does | `serve::host_of`, `serve::listen` |
| Serve an application with no crate of its own in production | the `fsr-server` binary, which `fsr bundle` copies into the deploy tree |
