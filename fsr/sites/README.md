# snapfire_fsr_sites

Where a mounted site's artifact comes from. The stock host knows how to mount a site it is handed; this crate knows how to find one: the `[sites]` table of a shell's configuration resolved to directories, each hashed and refused when the table pins another hash, then mounted and the table watched so a deploy is a pointer moved and a signal sent.

## Install

```toml
[dependencies]
snapfire_fsr_sites = { path = "../sites" }
```

## What to reach for

| Problem | Piece |
| --- | --- |
| Mount every site the configuration names | `mount_all` over a `HostBuilder` |
| See where each row of the table resolves and what it hashes to | `resolve` |
| Hash an artifact the way the table pins it | `hash_dir` |
| Reread the table on `SIGHUP` or a poll and reload the host when it moved | `watch`, `poll_of` |

## Status

Pre-release and unpublished. `fsr serve` and `portal_react_ts` use it; the host it mounts into is `snapfire_fsr_host`. A site across a transport, a site in its own process the shell reaches by address, is not built.
