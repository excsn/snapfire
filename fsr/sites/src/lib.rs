//! The `[sites]` table of a shell's configuration turned into mounts on the
//! stock host: each artifact resolved under the root or at its path, hashed
//! and refused when the table pins another hash, then the table watched so a
//! deploy is a pointer moved and a signal sent, or a poll noticing.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use snapfire_fsr_host::config::Config;
use snapfire_fsr_host::{Host, HostBuilder, HostError, Mount};

pub mod artifact;
pub mod install;

pub use artifact::{pack, parts, unpack, ArtifactError, Entry, Listing, Manifest};
pub use install::{ArchiveStore, Cache, InstallError, Installed, Store, TarStore};

#[derive(Debug, thiserror::Error)]
pub enum SitesError {
  #[error(transparent)]
  Host(#[from] HostError),
  #[error(transparent)]
  Content(#[from] ArtifactError),
  #[error("sites.{name}: {message}")]
  Artifact { name: String, message: String },
}

/// One row of the table, resolved: where the artifact is and what it hashes to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
  pub name: String,
  pub artifact: PathBuf,
  pub version: String,
  pub hash: String,
  pub allow_engine: bool,
}

/// Resolves every `[sites.<name>]` row: `name@version` under `sites.root`,
/// anything else a path against the configuration's root. Hashes each
/// directory and refuses one whose pinned `hash` differs.
pub fn resolve(config: &Config) -> Result<Vec<Resolved>, SitesError> {
  let Some(section) = &config.sites else {
    return Ok(Vec::new());
  };
  let mut out = Vec::new();
  for (name, mount) in &section.mounts {
    let (artifact, version) = if mount.artifact.contains('@') && !mount.artifact.contains('/') {
      let (artifact_name, version) = mount.artifact.split_once('@').expect("an @");
      let root = section.root.as_deref().expect("the configuration checked the root");
      (
        config.root.join(root).join(artifact_name).join(version),
        version.to_owned(),
      )
    } else {
      (config.root.join(&mount.artifact), "path".to_owned())
    };
    if !artifact.is_dir() {
      return Err(SitesError::Artifact {
        name: name.clone(),
        message: format!("{} is not a directory", artifact.display()),
      });
    }
    let hash = hash_dir(&artifact).map_err(|e| SitesError::Artifact {
      name: name.clone(),
      message: e.to_string(),
    })?;
    if let Some(pinned) = &mount.hash {
      if *pinned != hash {
        return Err(SitesError::Artifact {
          name: name.clone(),
          message: format!("hash {hash} at {}, pinned {pinned}", artifact.display()),
        });
      }
    }
    out.push(Resolved {
      name: name.clone(),
      artifact,
      version,
      hash,
      allow_engine: mount.allow_engine,
    });
  }
  Ok(out)
}

/// The content hash of the artifact at `dir`: xxh3 over the listing of every
/// file it ships, path, size and digest each. The set is derived from the
/// artifact's own configuration, so the hash of a working tree equals the hash
/// of what a release copied out of it.
pub fn hash_dir(dir: &Path) -> Result<String, ArtifactError> {
  Ok(Listing::of(dir)?.hash())
}

/// Installs [`mount_all`] as the host's sites mounter, so `Host::reload_sites`
/// and `POST /__fsr/sites/reload` read the artifacts again while the shell
/// stays exactly as the process booted it.
///
/// An application opts in by calling this; a host that does not has no such
/// route and no such reload.
pub fn mountable(builder: HostBuilder) -> HostBuilder {
  builder.sites_mounter(|builder| mount_all(builder).map_err(|e| HostError::Value("sites".to_owned(), e.to_string())))
}

/// Mounts every site the builder's configuration names.
pub fn mount_all(builder: HostBuilder) -> Result<HostBuilder, SitesError> {
  let resolved = resolve(builder.config())?;
  let mut builder = builder;
  for site in resolved {
    let mount = Mount::load(&site.name, &site.artifact, &site.version, &site.hash, site.allow_engine)?;
    builder = builder.mount(mount);
  }
  Ok(builder)
}

/// What the table resolves to now, as one string, so a poll can tell whether
/// anything moved without building tables.
fn table_shape(root: &Path) -> Option<String> {
  let config = Config::load(root).ok()?;
  let resolved = resolve(&config).ok()?;
  Some(
    resolved
      .iter()
      .map(|r| format!("{} {} {} {}", r.name, r.artifact.display(), r.version, r.hash))
      .collect::<Vec<_>>()
      .join("\n"),
  )
}

/// Watches the table for `host`, read from `root`: `SIGHUP` reloads at
/// once, and with `sites.poll` set the table is reread on that interval and
/// the host reloaded when a row moved. Runs until the runtime stops.
pub fn watch(host: Arc<Host>, root: PathBuf, poll: Option<Duration>) {
  let sighup = host.clone();
  tokio::spawn(async move {
    #[cfg(unix)]
    {
      let Ok(mut signal) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup()) else {
        return;
      };
      while signal.recv().await.is_some() {
        match sighup.reload() {
          Ok(report) => tracing::info!(target: "fsr::sites", "reloaded on SIGHUP\n{report}"),
          Err(e) => tracing::warn!(target: "fsr::sites", error = %e, "reload on SIGHUP refused"),
        }
      }
    }
  });
  let Some(every) = poll else { return };
  tokio::spawn(async move {
    let mut last = table_shape(&root);
    loop {
      tokio::time::sleep(every).await;
      let now = table_shape(&root);
      if now != last {
        match host.reload() {
          Ok(report) => {
            tracing::info!(target: "fsr::sites", "the sites table moved; reloaded\n{report}");
            last = now;
          }
          Err(e) => tracing::warn!(target: "fsr::sites", error = %e, "the sites table moved; reload refused"),
        }
      }
    }
  });
}

/// The poll interval the configuration names, if any.
pub fn poll_of(config: &Config) -> Option<Duration> {
  config
    .sites
    .as_ref()
    .and_then(|s| s.poll.as_deref())
    .and_then(snapfire_fsr_core::parse_duration)
}
