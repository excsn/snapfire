//! Where the compiler comes from: as given, from the cache, from a `tsc` on
//! `PATH` that reports the requested version, or from one verified tarball.

use std::path::{Path, PathBuf};
use std::process::Command;

use base64::Engine;
use serde::Deserialize;
use sha2::{Digest, Sha512};

use crate::{Error, Options, Resolved, Source};

/// The version resolved against when nothing asks for another.
pub const DEFAULT_VERSION: &str = "7.0.2";
pub const REGISTRY: &str = "https://registry.npmjs.org";

/// `sha512-` integrity per platform tarball of [`DEFAULT_VERSION`], as the
/// registry publishes it. A version outside this table takes its integrity
/// from the caller or from the registry document.
const PINNED: &[(&str, &str, &str)] = &[
  ("7.0.2", "darwin-arm64", "sha512-gowzar9MwS/aRWp6f3a4KUqzRjAZjOsmGNCM6LcTgXum+dBfgsBVMN+AgvOCCbguXyick6LJhpBszxMebJ8syA=="),
  ("7.0.2", "darwin-x64", "sha512-SZ9xZInqApNlNGc9s0W1VSsktYSOe9cFqNOIqmN1Gs8SmkjKZYFt017G4VwPxASInODuAdbTW7sXiFUf893RgA=="),
  ("7.0.2", "linux-x64", "sha512-EYdf2cNg7rgCWJnxCdJ+F3V39O8ihb37eHAu1LK8oAFizgTQbPOK7zHHXbPt8rX24COqODXeI3sIf0fCXG7H/A=="),
  ("7.0.2", "linux-arm64", "sha512-Qh4eU4/y3yDjnfjjyPYihMj5/ODIlmt+Bzu17OI+fiSRDW57QmU5SiN63exPRNJPKUzcc1INa1NXdrJ+MqHjUQ=="),
  ("7.0.2", "win32-x64", "sha512-0BQ3HkAHHlKLSp1qRvf3SUhGpGsDuhB/jgFw75guyqbxJqEaS0Cw/VFO8i2nHglJUzQCRtMMR/IBAKE3ETMC4g=="),
  ("7.0.2", "win32-arm64", "sha512-Gyl1Vy6OsWesLzmq+EP0Fb7b4Nid5232AvcA2SFcdYreldpNtYFFofPjnt62y9hQy7VTaZp65ICJjuAQRaVcIQ=="),
];

/// The platforms this crate pins hashes for; every other one is published too and takes the registry's integrity.
pub const PINNED_PLATFORMS: &[&str] = &["darwin-arm64", "darwin-x64", "linux-x64", "linux-arm64", "win32-x64", "win32-arm64"];

fn pinned(version: &str, platform: &str) -> Option<&'static str> {
  PINNED.iter().find(|(v, p, _)| *v == version && *p == platform).map(|(_, _, hash)| *hash)
}

/// Whether this crate's own source carries the hash for a version and platform,
/// so a caller knows whether the integrity it was handed is worth recording.
pub fn is_pinned(version: &str, platform: &str) -> bool {
  pinned(version, platform).is_some()
}

/// npm's own `<os>-<arch>` for the running target, which names both the package and its cache directory.
pub fn platform() -> Result<String, Error> {
  let os = match std::env::consts::OS {
    "macos" => "darwin",
    "windows" => "win32",
    other => other,
  };
  let arch = match std::env::consts::ARCH {
    "x86_64" => "x64",
    "aarch64" => "arm64",
    "arm" => "arm",
    "powerpc64" => "ppc64",
    "s390x" => "s390x",
    "riscv64" => "riscv64",
    "loongarch64" => "loong64",
    other => return Err(Error::Platform(format!("{os}-{other}"))),
  };
  Ok(format!("{os}-{arch}"))
}

fn env_dir(name: &str) -> Option<PathBuf> {
  std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from)
}

fn home() -> Option<PathBuf> {
  env_dir("HOME").or_else(|| env_dir("USERPROFILE"))
}

/// Where compilers are kept: `SNAPFIRE_CACHE`, else the platform's own cache
/// directory. Outside any repository and keyed by version below, so several
/// applications on one machine share it.
pub fn cache_root(explicit: Option<&Path>) -> Result<PathBuf, Error> {
  if let Some(dir) = explicit {
    return Ok(dir.to_path_buf());
  }
  if let Some(dir) = env_dir("SNAPFIRE_CACHE") {
    return Ok(dir);
  }
  let base = match std::env::consts::OS {
    "macos" => home().ok_or(Error::NoHome)?.join("Library").join("Caches"),
    "windows" => match env_dir("LOCALAPPDATA") {
      Some(dir) => dir,
      None => home().ok_or(Error::NoHome)?.join("AppData").join("Local"),
    },
    _ => match env_dir("XDG_CACHE_HOME") {
      Some(dir) => dir,
      None => home().ok_or(Error::NoHome)?.join(".cache"),
    },
  };
  Ok(base.join("snapfire"))
}

/// The unpacked package for one version and platform.
pub fn install_dir(cache: &Path, version: &str, platform: &str) -> PathBuf {
  cache.join("tsc").join(version).join(platform)
}

fn binary_in(dir: &Path) -> Option<PathBuf> {
  [dir.join("lib").join("tsc"), dir.join("lib").join("tsc.exe")].into_iter().find(|p| p.is_file())
}

/// The cached compiler for `version`, when one is unpacked already.
pub fn cached(cache: Option<&Path>, version: &str) -> Result<Option<PathBuf>, Error> {
  Ok(binary_in(&install_dir(&cache_root(cache)?, version, &platform()?)))
}

/// What a compiler says it is: `Version 7.0.2` becomes `7.0.2`.
fn version_of(path: &Path) -> Result<String, Error> {
  let output = Command::new(path).arg("--version").output().map_err(|e| Error::Spawn { path: path.to_path_buf(), source: e })?;
  let text = String::from_utf8_lossy(&output.stdout);
  let line = text.lines().next().unwrap_or_default().trim();
  Ok(line.strip_prefix("Version ").unwrap_or(line).to_owned())
}

pub fn integrity(bytes: &[u8]) -> String {
  let digest = Sha512::digest(bytes);
  format!("sha512-{}", base64::engine::general_purpose::STANDARD.encode(digest))
}

#[derive(Deserialize)]
struct VersionDoc {
  dist: Dist,
}

#[derive(Deserialize)]
struct Dist {
  tarball: String,
  #[serde(default)]
  integrity: Option<String>,
}

/// The compiler for `options.version`: as given, from the cache, from `PATH`
/// when it reports that version, else fetched and verified.
pub fn resolve(options: &Options) -> Result<Resolved, Error> {
  let want = options.version.clone();
  if let Some(path) = &options.tsc {
    let found = version_of(path)?;
    if found != want {
      return Err(Error::Mismatch { path: path.clone(), found, want });
    }
    return Ok(Resolved { tsc: path.clone(), version: found, source: Source::Given, sha512: None });
  }
  let platform = platform()?;
  let dir = install_dir(&cache_root(options.cache.as_deref())?, &want, &platform);
  if let Some(tsc) = binary_in(&dir) {
    return Ok(Resolved { tsc, version: want, source: Source::Cache, sha512: None });
  }
  let on_path = PathBuf::from(if cfg!(windows) { "tsc.exe" } else { "tsc" });
  if version_of(&on_path).is_ok_and(|found| found == want) {
    return Ok(Resolved { tsc: on_path, version: want, source: Source::Path, sha512: None });
  }
  if options.offline {
    return Err(Error::Offline(want));
  }
  fetch(options, &want, &platform, &dir)
}

fn client() -> Result<reqwest::blocking::Client, Error> {
  reqwest::blocking::Client::builder()
    .user_agent(concat!("snapfiretc/", env!("CARGO_PKG_VERSION")))
    .build()
    .map_err(|e| Error::Http(REGISTRY.to_owned(), e.to_string()))
}

fn get(client: &reqwest::blocking::Client, url: &str) -> Result<Vec<u8>, Error> {
  let response = client.get(url).send().map_err(|e| Error::Http(url.to_owned(), e.to_string()))?;
  if !response.status().is_success() {
    return Err(Error::Http(url.to_owned(), format!("HTTP {}", response.status())));
  }
  response.bytes().map(|b| b.to_vec()).map_err(|e| Error::Http(url.to_owned(), e.to_string()))
}

/// One HTTPS GET for the document, one for the tarball, then the hash before anything is written.
fn fetch(options: &Options, want: &str, platform: &str, dir: &Path) -> Result<Resolved, Error> {
  let client = client()?;
  let url = format!("{}/@typescript%2ftypescript-{platform}/{want}", options.registry.trim_end_matches('/'));
  let bytes = get(&client, &url)?;
  let doc: VersionDoc = serde_json::from_slice(&bytes).map_err(|e| Error::Http(url.clone(), e.to_string()))?;
  let tarball = get(&client, &doc.dist.tarball)?;
  let found = integrity(&tarball);
  let want_hash = pinned(want, platform)
    .map(str::to_owned)
    .or_else(|| options.expect.clone())
    .or(doc.dist.integrity)
    .ok_or_else(|| Error::NoIntegrity(url.clone()))?;
  if found != want_hash {
    return Err(Error::Integrity { url: doc.dist.tarball.clone(), found, want: want_hash });
  }
  unpack(&tarball, dir)?;
  let tsc = binary_in(dir).ok_or_else(|| Error::NoBinary(dir.to_path_buf()))?;
  Ok(Resolved { tsc, version: want.to_owned(), source: Source::Fetched, sha512: Some(found) })
}

/// Unpacks beside the target and renames, so a cache directory is either whole or absent.
fn unpack(tarball: &[u8], dir: &Path) -> Result<(), Error> {
  let parent = dir.parent().ok_or_else(|| Error::NoBinary(dir.to_path_buf()))?;
  std::fs::create_dir_all(parent).map_err(|e| Error::Io(parent.to_path_buf(), e))?;
  let staging = parent.join(format!(".{}.{}", dir.file_name().unwrap_or_default().to_string_lossy(), std::process::id()));
  if staging.exists() {
    std::fs::remove_dir_all(&staging).map_err(|e| Error::Io(staging.clone(), e))?;
  }
  let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(tarball));
  archive.unpack(&staging).map_err(|e| Error::Io(staging.clone(), e))?;
  let unpacked = staging.join("package");
  if !unpacked.is_dir() {
    let _ = std::fs::remove_dir_all(&staging);
    return Err(Error::NoBinary(dir.to_path_buf()));
  }
  if dir.exists() {
    std::fs::remove_dir_all(dir).map_err(|e| Error::Io(dir.to_path_buf(), e))?;
  }
  std::fs::rename(&unpacked, dir).map_err(|e| Error::Io(dir.to_path_buf(), e))?;
  let _ = std::fs::remove_dir_all(&staging);
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn every_pinned_platform_has_a_hash_for_the_default_version() {
    for platform in PINNED_PLATFORMS {
      let hash = pinned(DEFAULT_VERSION, platform).unwrap_or_else(|| panic!("{platform} has no pinned hash"));
      assert!(hash.starts_with("sha512-"), "{platform}: {hash}");
    }
    assert_eq!(pinned(DEFAULT_VERSION, "solaris-x64"), None);
  }

  #[test]
  fn the_cache_is_keyed_by_version_and_platform() {
    let root = PathBuf::from("/c");
    assert_eq!(install_dir(&root, "7.0.2", "darwin-arm64"), PathBuf::from("/c/tsc/7.0.2/darwin-arm64"));
    assert_eq!(install_dir(&root, "7.1.0", "darwin-arm64"), PathBuf::from("/c/tsc/7.1.0/darwin-arm64"));
  }

  #[test]
  fn an_explicit_cache_wins_over_the_platform_directory() {
    assert_eq!(cache_root(Some(Path::new("/tmp/c"))).unwrap(), PathBuf::from("/tmp/c"));
    let default = cache_root(None).unwrap();
    assert!(default.ends_with("snapfire"), "{}", default.display());
  }

  #[test]
  fn the_platform_is_npms_own_spelling() {
    let platform = platform().unwrap();
    let (os, arch) = platform.split_once('-').unwrap();
    assert!(["darwin", "linux", "win32", "freebsd", "netbsd", "openbsd"].contains(&os), "{platform}");
    assert!(!arch.is_empty());
  }
}
