//! What a site artifact is: the files a mounted site ships, listed with a
//! digest each, and the hash of that listing.
//!
//! The set is derived from the site's own configuration rather than named a
//! second time, so the hash of a working tree equals the hash of what a
//! release copied out of it, and a pin written during development still holds
//! against the deployed directory. Nothing here knows about a registry: an
//! artifact is a directory or a gzipped tar of one, and where either came from
//! is the caller's business.

use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use snapfire_fsr_host::config::Config;

/// The manifest a packed artifact carries at its root. Dot-prefixed and
/// outside every part, so it is never a member of the listing it describes.
pub const MANIFEST: &str = ".snapfire-site.json";

/// The manifest format this crate writes and reads.
pub const FORMAT: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum ArtifactError {
  #[error("{0}: {1}")]
  Io(PathBuf, #[source] std::io::Error),
  #[error(transparent)]
  Config(#[from] snapfire_fsr_host::HostError),
  #[error("{0} has no [site], so it is not a site artifact")]
  NotASite(PathBuf),
  #[error("{path}: {message}")]
  Manifest { path: PathBuf, message: String },
  #[error("{0}: manifest format {1}, this build reads {FORMAT}")]
  Format(PathBuf, u32),
  #[error("hash {found}, the manifest says {declared}")]
  Hash { found: String, declared: String },
  #[error("{path}: sha256 {found}, the manifest says {declared}")]
  Digest {
    path: String,
    found: String,
    declared: String,
  },
  #[error("{path} is listed and absent")]
  Missing { path: String },
  #[error("{path} is present and not listed")]
  Extra { path: String },
  #[error("{0}: {1}")]
  Archive(PathBuf, String),
}

/// One file of an artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
  pub path: String,
  pub size: u64,
  pub sha256: String,
}

/// Every file an artifact ships, in path order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Listing {
  pub entries: Vec<Entry>,
}

impl Listing {
  /// The listing of the artifact at `dir`, read through its own configuration.
  pub fn of(dir: &Path) -> Result<Self, ArtifactError> {
    let config = Config::load(dir)?;
    Self::of_config(dir, &config)
  }

  /// The listing of the artifact at `root` whose configuration is already
  /// loaded, so a caller that has one does not parse it twice.
  pub fn of_config(root: &Path, config: &Config) -> Result<Self, ArtifactError> {
    let mut entries = Vec::new();
    for part in parts(root, config) {
      let path = root.join(&part);
      if path.is_dir() {
        walk(root, &path, &mut entries)?;
      } else if path.is_file() {
        entries.push(entry(root, &path)?);
      }
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    entries.dedup_by(|a, b| a.path == b.path);
    Ok(Self { entries })
  }

  /// The artifact hash: xxh3 over the listing, path, size and digest of each
  /// entry in path order. Over the listing rather than the bytes, so a
  /// manifest alone yields it and a pin can be checked before a download.
  pub fn hash(&self) -> String {
    let mut hasher = xxhash_rust::xxh3::Xxh3::new();
    for entry in &self.entries {
      hasher.update(entry.path.as_bytes());
      hasher.update(&[0]);
      hasher.update(entry.size.to_string().as_bytes());
      hasher.update(&[0]);
      hasher.update(entry.sha256.as_bytes());
      hasher.update(&[0]);
    }
    format!("{:016x}", hasher.digest())
  }

  pub fn bytes(&self) -> u64 {
    self.entries.iter().map(|e| e.size).sum()
  }
}

/// The parts of a site's project directory that ship, relative to its root:
/// its configuration, whatever holds the plan, the contracts and the prerender
/// cache, every static root it serves and its import map. A part that another
/// part contains is dropped, so `app/generated/contracts` beside
/// `app/generated` is one entry.
pub fn parts(root: &Path, config: &Config) -> Vec<String> {
  let app = relative(root, &config.app);
  let under = |path: &str| {
    if app.is_empty() {
      path.to_owned()
    } else {
      format!("{app}/{path}")
    }
  };

  let mut parts = Vec::new();
  let config_dir = relative(root, &config.config_dir());
  if config_dir.is_empty() {
    // The configuration sits in the artifact root rather than a directory of
    // its own, so the files themselves are the part; the root is not.
    parts.extend(config.sources.iter().map(|source| relative(root, source)));
  } else {
    parts.push(config_dir);
  }
  parts.push(match Path::new(&config.server.plan).parent() {
    Some(dir) if !dir.as_os_str().is_empty() => under(&slashed(dir)),
    _ => under(&config.server.plan),
  });
  parts.push(under(&config.server.contracts));
  if let Some(prerender) = &config.server.prerender {
    parts.push(under(prerender));
  }
  for served in &config.statics {
    parts.push(under(&served.dir));
  }
  if let Some(map) = &config.document.import_map {
    parts.push(under(map));
  }
  parts.retain(|part| !part.is_empty());
  parts.sort();
  parts.dedup();
  subsume(parts)
}

fn subsume(parts: Vec<String>) -> Vec<String> {
  let mut kept: Vec<String> = Vec::new();
  for part in parts {
    if kept
      .iter()
      .any(|held| part == *held || part.starts_with(&format!("{held}/")))
    {
      continue;
    }
    kept.retain(|held| !held.starts_with(&format!("{part}/")));
    kept.push(part);
  }
  kept
}

/// What a packed artifact carries at its root and an install reads back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
  pub format: u32,
  /// The site's own name, `[site] name`, which is not the name the shell
  /// mounts it under.
  pub name: String,
  pub version: String,
  /// The prefix the site was built for, `[site] at`.
  pub at: String,
  pub hash: String,
  pub files: Vec<Entry>,
}

impl Manifest {
  /// The manifest for the artifact at `dir` released as `version`.
  pub fn of(dir: &Path, version: &str) -> Result<Self, ArtifactError> {
    let config = Config::load(dir)?;
    let site = config
      .site
      .as_ref()
      .ok_or_else(|| ArtifactError::NotASite(dir.to_path_buf()))?;
    let listing = Listing::of_config(dir, &config)?;
    Ok(Self {
      format: FORMAT,
      name: site.name.clone(),
      version: version.to_owned(),
      at: site.at.clone(),
      hash: listing.hash(),
      files: listing.entries,
    })
  }

  pub fn path(dir: &Path) -> PathBuf {
    dir.join(MANIFEST)
  }

  /// The manifest beside the artifact at `dir`, absent when the directory
  /// carries none, which a project directory never does.
  pub fn read(dir: &Path) -> Result<Option<Self>, ArtifactError> {
    let path = Self::path(dir);
    let text = match std::fs::read_to_string(&path) {
      Ok(text) => text,
      Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
      Err(e) => return Err(ArtifactError::Io(path, e)),
    };
    let manifest: Self = serde_json::from_str(&text).map_err(|e| ArtifactError::Manifest {
      path: path.clone(),
      message: e.to_string(),
    })?;
    if manifest.format != FORMAT {
      return Err(ArtifactError::Format(path, manifest.format));
    }
    Ok(Some(manifest))
  }

  /// The manifest inside the gzipped tar at `archive`, read without unpacking
  /// it, so a caller can name a version before it commits to a fetch.
  pub fn read_archive(archive: &Path) -> Result<Self, ArtifactError> {
    let file = std::fs::File::open(archive).map_err(|e| ArtifactError::Io(archive.to_path_buf(), e))?;
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(file));
    let entries = tar.entries().map_err(|e| ArtifactError::Io(archive.to_path_buf(), e))?;
    for entry in entries {
      let mut entry = entry.map_err(|e| ArtifactError::Io(archive.to_path_buf(), e))?;
      let path = entry
        .path()
        .map_err(|e| ArtifactError::Io(archive.to_path_buf(), e))?
        .into_owned();
      if slashed(&path) != MANIFEST {
        continue;
      }
      let mut bytes = Vec::new();
      entry
        .read_to_end(&mut bytes)
        .map_err(|e| ArtifactError::Io(archive.to_path_buf(), e))?;
      let read: Self = serde_json::from_slice(&bytes).map_err(|e| ArtifactError::Manifest {
        path: archive.to_path_buf(),
        message: e.to_string(),
      })?;
      if read.format != FORMAT {
        return Err(ArtifactError::Format(archive.to_path_buf(), read.format));
      }
      return Ok(read);
    }
    Err(ArtifactError::Archive(
      archive.to_path_buf(),
      format!("no {MANIFEST} at its root"),
    ))
  }

  pub fn write(&self, dir: &Path) -> Result<(), ArtifactError> {
    let path = Self::path(dir);
    let text = serde_json::to_string_pretty(self).map_err(|e| ArtifactError::Manifest {
      path: path.clone(),
      message: e.to_string(),
    })?;
    std::fs::write(&path, format!("{text}\n")).map_err(|e| ArtifactError::Io(path, e))
  }

  /// Every listed file present with the digest listed, nothing present that is
  /// not listed and the whole listing hashing to what the manifest declares.
  /// What an install checks before a staged directory is renamed into place.
  pub fn verify(&self, dir: &Path) -> Result<(), ArtifactError> {
    let found = Listing::of(dir)?;
    let listed: std::collections::BTreeMap<&str, &Entry> = self.files.iter().map(|e| (e.path.as_str(), e)).collect();
    for entry in &found.entries {
      let Some(declared) = listed.get(entry.path.as_str()) else {
        return Err(ArtifactError::Extra {
          path: entry.path.clone(),
        });
      };
      if entry.sha256 != declared.sha256 {
        return Err(ArtifactError::Digest {
          path: entry.path.clone(),
          found: entry.sha256.clone(),
          declared: declared.sha256.clone(),
        });
      }
    }
    let present: std::collections::BTreeSet<&str> = found.entries.iter().map(|e| e.path.as_str()).collect();
    if let Some(missing) = self.files.iter().find(|e| !present.contains(e.path.as_str())) {
      return Err(ArtifactError::Missing {
        path: missing.path.clone(),
      });
    }
    let hash = found.hash();
    if hash != self.hash {
      return Err(ArtifactError::Hash {
        found: hash,
        declared: self.hash.clone(),
      });
    }
    Ok(())
  }
}

/// Writes the artifact at `dir` as a gzipped tar at `out`, its manifest at the
/// archive root, and returns the manifest. Entries carry no timestamp and no
/// owner, so packing the same tree twice produces the same bytes.
pub fn pack(dir: &Path, version: &str, out: &Path) -> Result<Manifest, ArtifactError> {
  let manifest = Manifest::of(dir, version)?;
  if let Some(parent) = out.parent() {
    if !parent.as_os_str().is_empty() {
      std::fs::create_dir_all(parent).map_err(|e| ArtifactError::Io(parent.to_path_buf(), e))?;
    }
  }
  let file = std::fs::File::create(out).map_err(|e| ArtifactError::Io(out.to_path_buf(), e))?;
  let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(file, flate2::Compression::default()));

  let text = serde_json::to_string_pretty(&manifest).map_err(|e| ArtifactError::Manifest {
    path: out.to_path_buf(),
    message: e.to_string(),
  })?;
  append(&mut builder, out, MANIFEST, format!("{text}\n").as_bytes())?;
  for entry in &manifest.files {
    let path = dir.join(&entry.path);
    let bytes = std::fs::read(&path).map_err(|e| ArtifactError::Io(path.clone(), e))?;
    append(&mut builder, out, &entry.path, &bytes)?;
  }
  builder
    .into_inner()
    .and_then(|gz| gz.finish())
    .map_err(|e| ArtifactError::Io(out.to_path_buf(), e))?;
  Ok(manifest)
}

fn append<W: std::io::Write>(
  builder: &mut tar::Builder<W>,
  out: &Path,
  name: &str,
  bytes: &[u8],
) -> Result<(), ArtifactError> {
  let mut header = tar::Header::new_gnu();
  header.set_size(bytes.len() as u64);
  header.set_mode(0o644);
  header.set_mtime(0);
  header.set_uid(0);
  header.set_gid(0);
  header.set_entry_type(tar::EntryType::Regular);
  builder
    .append_data(&mut header, name, bytes)
    .map_err(|e| ArtifactError::Io(out.to_path_buf(), e))
}

/// Unpacks the gzipped tar at `archive` into `into`, which must not exist, and
/// returns its manifest without verifying it. A caller installs by verifying
/// the unpacked directory against the manifest this returns.
///
/// The archive is what a caller was handed rather than what it built, so an
/// entry that is absolute, that climbs out with `..` or that is not a regular
/// file is refused before anything is written.
pub fn unpack(archive: &Path, into: &Path) -> Result<Manifest, ArtifactError> {
  let file = std::fs::File::open(archive).map_err(|e| ArtifactError::Io(archive.to_path_buf(), e))?;
  let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(file));
  let entries = tar.entries().map_err(|e| ArtifactError::Io(archive.to_path_buf(), e))?;
  let mut manifest = None;
  for entry in entries {
    let mut entry = entry.map_err(|e| ArtifactError::Io(archive.to_path_buf(), e))?;
    if entry.header().entry_type() != tar::EntryType::Regular {
      return Err(ArtifactError::Archive(
        archive.to_path_buf(),
        "an entry is not a regular file".to_owned(),
      ));
    }
    let path = entry
      .path()
      .map_err(|e| ArtifactError::Io(archive.to_path_buf(), e))?
      .into_owned();
    let name = slashed(&path);
    if path.is_absolute() || path.components().any(|c| c.as_os_str() == "..") {
      return Err(ArtifactError::Archive(
        archive.to_path_buf(),
        format!("{name} climbs out of the archive"),
      ));
    }
    let mut bytes = Vec::new();
    entry
      .read_to_end(&mut bytes)
      .map_err(|e| ArtifactError::Io(archive.to_path_buf(), e))?;
    if name == MANIFEST {
      let read: Manifest = serde_json::from_slice(&bytes).map_err(|e| ArtifactError::Manifest {
        path: archive.to_path_buf(),
        message: e.to_string(),
      })?;
      if read.format != FORMAT {
        return Err(ArtifactError::Format(archive.to_path_buf(), read.format));
      }
      manifest = Some(read);
      continue;
    }
    let out = into.join(&name);
    if let Some(parent) = out.parent() {
      std::fs::create_dir_all(parent).map_err(|e| ArtifactError::Io(parent.to_path_buf(), e))?;
    }
    std::fs::write(&out, &bytes).map_err(|e| ArtifactError::Io(out, e))?;
  }
  let manifest =
    manifest.ok_or_else(|| ArtifactError::Archive(archive.to_path_buf(), format!("no {MANIFEST} at its root")))?;
  manifest.write(into)?;
  Ok(manifest)
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<Entry>) -> Result<(), ArtifactError> {
  let read = std::fs::read_dir(dir).map_err(|e| ArtifactError::Io(dir.to_path_buf(), e))?;
  let mut paths: Vec<PathBuf> = Vec::new();
  for found in read {
    paths.push(found.map_err(|e| ArtifactError::Io(dir.to_path_buf(), e))?.path());
  }
  paths.sort();
  for path in paths {
    if path.is_dir() {
      walk(root, &path, out)?;
    } else if path.is_file() {
      out.push(entry(root, &path)?);
    }
  }
  Ok(())
}

fn entry(root: &Path, path: &Path) -> Result<Entry, ArtifactError> {
  let bytes = std::fs::read(path).map_err(|e| ArtifactError::Io(path.to_path_buf(), e))?;
  Ok(Entry {
    path: relative(root, path),
    size: bytes.len() as u64,
    sha256: format!("{:x}", Sha256::digest(&bytes)),
  })
}

fn relative(root: &Path, path: &Path) -> String {
  slashed(path.strip_prefix(root).unwrap_or(path))
}

fn slashed(path: &Path) -> String {
  path.to_string_lossy().replace('\\', "/")
}
