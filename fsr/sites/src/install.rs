//! The cache under `[sites] root` and how a version lands in it.
//!
//! Where the bytes come from is a [`Store`], which a deployment implements
//! against whatever it already runs. What happens to them is not a seam: a
//! fetch stages under a dot-prefixed directory beside its destination, the
//! staged tree is verified against the manifest that came with it, and only
//! then is it renamed into place. A fetch that dies leaves nothing a mount can
//! see, and a fetch that arrives wrong leaves the running version serving.

use std::path::{Path, PathBuf};

use crate::artifact::{unpack, ArtifactError, Manifest};

/// The directory a fetch stages under, inside the cache so the rename that
/// installs never crosses a filesystem.
pub const STAGING: &str = ".staging";

#[derive(Debug, thiserror::Error)]
pub enum InstallError {
  #[error(transparent)]
  Artifact(#[from] ArtifactError),
  #[error("{0}: {1}")]
  Io(PathBuf, #[source] std::io::Error),
  #[error("{name}@{version} is already installed at {path}")]
  Held {
    name: String,
    version: String,
    path: PathBuf,
  },
  #[error("{store} holds no {package} at {version}")]
  Absent {
    store: String,
    package: String,
    version: String,
  },
  #[error("the artifact is {name}@{version} and {package}@{wanted} was asked for")]
  Mismatch {
    name: String,
    version: String,
    package: String,
    wanted: String,
  },
}

/// Where a version's bytes come from. Local sources ship here; a deployment
/// that fetches from a registry, an object store or its own artifact service
/// implements this and hands it to a [`Cache`].
pub trait Store: Send + Sync {
  /// How the store names itself in a report or an error.
  fn describe(&self) -> String;

  /// Writes the artifact for `package` at `version` into `into`, an empty
  /// directory the cache owns, leaving its manifest at the root. The cache
  /// verifies what lands, so a store is not required to.
  fn fetch(&self, package: &str, version: &str, into: &Path) -> Result<Manifest, InstallError>;
}

/// A directory of packed artifacts named `<package>-<version>.tar.gz`. A
/// synced folder, a mounted share or the output of a build job, which is the
/// smallest thing that is a store at all.
#[derive(Debug, Clone)]
pub struct TarStore {
  pub dir: PathBuf,
}

impl TarStore {
  pub fn new(dir: impl Into<PathBuf>) -> Self {
    Self { dir: dir.into() }
  }

  pub fn archive(&self, package: &str, version: &str) -> PathBuf {
    self.dir.join(format!("{package}-{version}.tar.gz"))
  }
}

impl Store for TarStore {
  fn describe(&self) -> String {
    self.dir.display().to_string()
  }

  fn fetch(&self, package: &str, version: &str, into: &Path) -> Result<Manifest, InstallError> {
    let archive = self.archive(package, version);
    if !archive.is_file() {
      return Err(InstallError::Absent {
        store: self.describe(),
        package: package.to_owned(),
        version: version.to_owned(),
      });
    }
    Ok(unpack(&archive, into)?)
  }
}

/// One artifact archive, wherever it sits, as a store of a single version.
#[derive(Debug, Clone)]
pub struct ArchiveStore {
  pub archive: PathBuf,
}

impl Store for ArchiveStore {
  fn describe(&self) -> String {
    self.archive.display().to_string()
  }

  fn fetch(&self, _package: &str, _version: &str, into: &Path) -> Result<Manifest, InstallError> {
    Ok(unpack(&self.archive, into)?)
  }
}

/// What an install did.
#[derive(Debug, Clone)]
pub struct Installed {
  /// The name the cache holds it under, which is what a `[sites.<name>]` row's
  /// `artifact` names and not always the site's own `[site] name`.
  pub name: String,
  pub version: String,
  pub hash: String,
  pub path: PathBuf,
  /// True when the version was already in the cache and nothing was fetched.
  pub held: bool,
  /// Versions the sweep removed.
  pub swept: Vec<String>,
}

/// The artifact cache: `<root>/<name>/<version>` per installed version, which
/// is where `artifact = "<name>@<version>"` already resolves.
#[derive(Debug, Clone)]
pub struct Cache {
  pub root: PathBuf,
}

impl Cache {
  pub fn new(root: impl Into<PathBuf>) -> Self {
    Self { root: root.into() }
  }

  pub fn path(&self, name: &str, version: &str) -> PathBuf {
    self.root.join(name).join(version)
  }

  pub fn holds(&self, name: &str, version: &str) -> bool {
    self.path(name, version).is_dir()
  }

  /// Every version of `name` the cache holds, oldest install first.
  pub fn versions(&self, name: &str) -> Vec<String> {
    let Ok(read) = std::fs::read_dir(self.root.join(name)) else {
      return Vec::new();
    };
    let mut found: Vec<(std::time::SystemTime, String)> = Vec::new();
    for entry in read.flatten() {
      if !entry.path().is_dir() {
        continue;
      }
      let name = entry.file_name().to_string_lossy().into_owned();
      if name.starts_with('.') {
        continue;
      }
      let at = entry
        .metadata()
        .and_then(|m| m.modified())
        .unwrap_or(std::time::UNIX_EPOCH);
      found.push((at, name));
    }
    found.sort();
    found.into_iter().map(|(_, name)| name).collect()
  }

  /// Fetches `package` at `version` from `store` and installs it as `name`,
  /// then sweeps down to `keep` versions. A version the cache already holds is
  /// reported and not fetched, so an install is safe to repeat.
  pub fn install(
    &self,
    store: &dyn Store,
    name: &str,
    package: &str,
    version: &str,
    keep: Option<usize>,
  ) -> Result<Installed, InstallError> {
    let destination = self.path(name, version);
    if destination.is_dir() {
      let manifest = Manifest::read(&destination)?;
      let hash = match manifest {
        Some(manifest) => manifest.hash,
        None => crate::hash_dir(&destination)?,
      };
      return Ok(Installed {
        name: name.to_owned(),
        version: version.to_owned(),
        hash,
        path: destination,
        held: true,
        swept: Vec::new(),
      });
    }

    let staged = self.stage(name, version)?;
    let outcome = self.fill(store, package, version, &staged);
    let manifest = match outcome {
      Ok(manifest) => manifest,
      Err(e) => {
        let _ = std::fs::remove_dir_all(&staged);
        return Err(e);
      }
    };

    if let Some(parent) = destination.parent() {
      std::fs::create_dir_all(parent).map_err(|e| InstallError::Io(parent.to_path_buf(), e))?;
    }
    std::fs::rename(&staged, &destination).map_err(|e| {
      let _ = std::fs::remove_dir_all(&staged);
      InstallError::Io(destination.clone(), e)
    })?;

    let swept = match keep {
      Some(keep) => self.sweep(name, keep, &[version.to_owned()])?,
      None => Vec::new(),
    };
    Ok(Installed {
      name: name.to_owned(),
      version: manifest.version,
      hash: manifest.hash,
      path: destination,
      held: false,
      swept,
    })
  }

  /// Fetches into `staged` and verifies what landed. Split out so every early
  /// return above discards the staging directory.
  fn fill(&self, store: &dyn Store, package: &str, version: &str, staged: &Path) -> Result<Manifest, InstallError> {
    let manifest = store.fetch(package, version, staged)?;
    if manifest.version != version {
      return Err(InstallError::Mismatch {
        name: manifest.name,
        version: manifest.version,
        package: package.to_owned(),
        wanted: version.to_owned(),
      });
    }
    manifest.verify(staged)?;
    Ok(manifest)
  }

  /// A directory under `<root>/.staging` no other install is using. Dot
  /// prefixed, so a crash leaves nothing that resolves as a version.
  fn stage(&self, name: &str, version: &str) -> Result<PathBuf, InstallError> {
    let staging = self.root.join(STAGING);
    std::fs::create_dir_all(&staging).map_err(|e| InstallError::Io(staging.clone(), e))?;
    for attempt in 0..64 {
      let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
      let path = staging.join(format!("{name}-{version}-{}-{nanos}-{attempt}", std::process::id()));
      match std::fs::create_dir(&path) {
        Ok(()) => return Ok(path),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
        Err(e) => return Err(InstallError::Io(path, e)),
      }
    }
    Err(InstallError::Io(
      staging,
      std::io::Error::other("no free staging directory"),
    ))
  }

  /// Removes installed versions of `name` past the `keep` most recent, never
  /// one named in `hold`. Returns what it removed.
  pub fn sweep(&self, name: &str, keep: usize, hold: &[String]) -> Result<Vec<String>, InstallError> {
    let versions = self.versions(name);
    let over = versions.len().saturating_sub(keep.max(1));
    let mut removed = Vec::new();
    for version in versions.into_iter().take(over) {
      if hold.contains(&version) {
        continue;
      }
      let path = self.path(name, &version);
      std::fs::remove_dir_all(&path).map_err(|e| InstallError::Io(path, e))?;
      removed.push(version);
    }
    Ok(removed)
  }

  /// Removes every staging directory left by a fetch that died. Run at boot,
  /// where nothing else is staging.
  pub fn sweep_staging(&self) -> Result<usize, InstallError> {
    let staging = self.root.join(STAGING);
    let Ok(read) = std::fs::read_dir(&staging) else {
      return Ok(0);
    };
    let mut swept = 0;
    for entry in read.flatten() {
      let path = entry.path();
      let outcome = if path.is_dir() {
        std::fs::remove_dir_all(&path)
      } else {
        std::fs::remove_file(&path)
      };
      outcome.map_err(|e| InstallError::Io(path, e))?;
      swept += 1;
    }
    Ok(swept)
  }
}
