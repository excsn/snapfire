//! `fsr sites`: the shell's table and the site's own section written as a
//! pair, so mounting an application is one command rather than two files
//! edited by hand. A link writes `[site]` beside the site and `[sites.<name>]`
//! beside the shell; an unlink takes both back out.

use std::path::{Path, PathBuf};

use snapfire_fsr_host::config::Config;

use crate::BuildError;

/// One row of a shell's table as `list` reports it.
#[derive(Debug, Clone)]
pub struct Row {
  pub name: String,
  pub artifact: String,
  /// Where the artifact resolved, absent when it does not resolve.
  pub resolved: Option<PathBuf>,
  /// The `at` the artifact's own `[site]` names, absent when it is not a site.
  pub at: Option<String>,
  pub version: String,
  pub hash: String,
  /// Why the row does not hold, absent when it does.
  pub note: Option<String>,
}

#[derive(Debug)]
pub struct Linked {
  pub name: String,
  pub at: String,
  pub shell_config: PathBuf,
  pub site_config: PathBuf,
  /// What the shell's row names, relative to the shell's project root.
  pub artifact: String,
  /// What the site's `[site] shell` names, relative to the site's project root.
  pub shell_json: String,
  /// True when the site already carried the `[site]` this link wanted.
  pub site_kept: bool,
  pub next: Vec<String>,
}

#[derive(Debug)]
pub struct Unlinked {
  pub name: String,
  pub shell_config: PathBuf,
  /// The site's configuration, when its `[site]` was taken out too.
  pub site_config: Option<PathBuf>,
}

/// Every `[sites.<name>]` row of the shell at `shell`, resolved.
/// What pinning one mount did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pinned {
  pub name: String,
  pub hash: String,
  /// The hash the mount carried before, when it carried one.
  pub was: Option<String>,
  pub artifact: PathBuf,
}

impl Pinned {
  /// Whether the file changed, so a caller can say what it did rather than
  /// what it looked at.
  pub fn moved(&self) -> bool {
    self.was.as_deref() != Some(self.hash.as_str())
  }
}

pub fn list(shell: &Path) -> Result<Vec<Row>, BuildError> {
  let config = load(shell)?;
  let Some(section) = &config.sites else { return Ok(Vec::new()) };
  let resolved = snapfire_fsr_sites::resolve(&config).ok();
  let mut rows = Vec::new();
  for (name, mount) in &section.mounts {
    let found = resolved.as_ref().and_then(|all| all.iter().find(|r| r.name == *name));
    let (resolved_path, version, hash) = match found {
      Some(r) => (Some(r.artifact.clone()), r.version.clone(), r.hash.clone()),
      None => (None, "-".to_owned(), "-".to_owned()),
    };
    let (at, note) = match &resolved_path {
      Some(path) => match Config::load(path) {
        Ok(site) => match site.site {
          Some(s) => (Some(s.at), None),
          None => (None, Some("the artifact has no [site]".to_owned())),
        },
        Err(e) => (None, Some(e.to_string())),
      },
      None => (None, Some(format!("{} does not resolve", mount.artifact))),
    };
    rows.push(Row { name: name.clone(), artifact: mount.artifact.clone(), resolved: resolved_path, at, version, hash, note });
  }
  Ok(rows)
}

/// Writes `[site]` beside `site` and `[sites.<name>]` beside `shell`. Refuses
/// a shell that is itself a site, a site that mounts sites, a name the table
/// already holds, and a site whose `[site]` names something else.
pub fn link(shell: &Path, site: &Path, at: &str, name: Option<&str>) -> Result<Linked, BuildError> {
  let shell_config = load(shell)?;
  let site_config = load(site)?;

  if shell_config.site.is_some() {
    return Err(refuse(format!("{} is a site; a site cannot mount sites", shell_config.root.display())));
  }
  if site_config.sites.is_some() {
    return Err(refuse(format!("{} mounts sites; a shell cannot be mounted", site_config.root.display())));
  }
  if shell_config.root == site_config.root {
    return Err(refuse("a shell cannot mount itself".to_owned()));
  }

  let name = match name {
    Some(given) => given.to_owned(),
    None => derive_name(&site_config.root)?,
  };
  check_name(&name)?;
  check_at(at)?;

  if let Some(section) = &shell_config.sites {
    if section.mounts.contains_key(&name) {
      return Err(refuse(format!("`{name}` is already mounted; `fsr sites unlink` it first")));
    }
  }

  let shell_file = writable(&shell_config)?;
  let site_file = writable(&site_config)?;

  let artifact = relative(&shell_config.root, &site_config.root)?;
  let shell_json = relative(&site_config.root, &shell_config.app.join("generated/shell.json"))?;

  let site_kept = match &site_config.site {
    Some(existing) => {
      if existing.name != name || existing.at != at {
        return Err(refuse(format!(
          "{} already names site `{}` at `{}`; unlink it or pass --name {} --at {}",
          site_file.display(),
          existing.name,
          existing.at,
          existing.name,
          existing.at
        )));
      }
      true
    }
    None => false,
  };

  let mut written = Vec::new();
  if !site_kept {
    let section = format!("\n[site]\nname = \"{name}\"\nat = \"{at}\"\nshell = \"{shell_json}\"\n");
    append(&site_file, &section)?;
    written.push(site_file.clone());
  }
  let row = format!("\n[sites.{name}]\nartifact = \"{artifact}\"\n");
  if let Err(e) = append(&shell_file, &row).and_then(|()| confirm(&shell_config.root, &name)) {
    for path in &written {
      truncate(path, &format!("\n[site]\nname = \"{name}\"\nat = \"{at}\"\nshell = \"{shell_json}\"\n"))?;
    }
    truncate(&shell_file, &row).ok();
    return Err(e);
  }

  let mut next = Vec::new();
  if !shell_config.app.join("generated/shell.json").is_file() {
    next.push(format!("fsr build {}", shell_config.app.display()));
  }
  next.push(format!("fsr build {}", site_config.app.display()));

  Ok(Linked { name, at: at.to_owned(), shell_config: shell_file, site_config: site_file, artifact, shell_json, site_kept, next })
}

/// Takes `[sites.<name>]` out of the shell and, unless `keep_site`, the
/// `[site]` out of the artifact it named.
pub fn unlink(shell: &Path, name: &str, keep_site: bool) -> Result<Unlinked, BuildError> {
  let shell_config = load(shell)?;
  if shell_config.site.is_some() {
    return Err(refuse(format!("{} is a site and mounts nothing", shell_config.root.display())));
  }
  let Some(section) = &shell_config.sites else {
    return Err(refuse(format!("{} mounts no sites", shell_config.root.display())));
  };
  let Some(mount) = section.mounts.get(name) else {
    let known: Vec<&str> = section.mounts.keys().map(String::as_str).collect();
    return Err(refuse(format!("`{name}` is not mounted; the table holds {}", if known.is_empty() { "nothing".to_owned() } else { known.join(", ") })));
  };

  let artifact = shell_config.root.join(&mount.artifact);
  let shell_file = writable(&shell_config)?;
  remove_section(&shell_file, &format!("sites.{name}"))?;

  let mut site_config = None;
  if !keep_site {
    if let Ok(site) = Config::load(&artifact) {
      if site.site.as_ref().is_some_and(|s| s.name == name) {
        let file = writable(&site)?;
        remove_section(&file, "site")?;
        site_config = Some(file);
      }
    }
  }
  Ok(Unlinked { name: name.to_owned(), shell_config: shell_file, site_config })
}

fn load(path: &Path) -> Result<Config, BuildError> {
  Config::load(path).map_err(|e| refuse(e.to_string()))
}

fn refuse(message: String) -> BuildError {
  BuildError::Sites(message)
}

/// The configuration file a section is written into: the `app.toml` among the
/// sources. A YAML configuration is read but never written.
fn writable(config: &Config) -> Result<PathBuf, BuildError> {
  config
    .sources
    .iter()
    .find(|p| p.file_name().is_some_and(|n| n == "app.toml"))
    .cloned()
    .ok_or_else(|| refuse(format!("{}: no `app.toml` to write; add the section by hand", config.root.display())))
}

/// The site's directory name as a site name: what the host's own rule allows,
/// with `.` and uppercase folded the way a directory usually spells them.
fn derive_name(root: &Path) -> Result<String, BuildError> {
  let raw = root.file_name().and_then(|n| n.to_str()).unwrap_or_default();
  name_from(raw)
}

/// A directory name folded to what [`check_name`] allows.
pub fn name_from(raw: &str) -> Result<String, BuildError> {
  let name: String = raw.to_ascii_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect();
  if name.is_empty() {
    return Err(refuse(format!("`{raw}` leaves no name to derive; pass --name")));
  }
  Ok(name)
}

/// The host's own rules on a site's name and the path it mounts at, so a
/// command refuses what the configuration would refuse at boot.
pub fn check(name: &str, at: &str) -> Result<(), BuildError> {
  check_name(name)?;
  check_at(at)
}

fn check_name(name: &str) -> Result<(), BuildError> {
  if name.is_empty() || !name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-') {
    return Err(refuse(format!("site name `{name}` must be lowercase letters, digits, `_` or `-`")));
  }
  Ok(())
}

fn check_at(at: &str) -> Result<(), BuildError> {
  if !at.starts_with('/') || at.len() < 2 || at.ends_with('/') || at.contains('{') {
    return Err(refuse(format!("`{at}` must be a path such as `/billing`, with no trailing slash")));
  }
  Ok(())
}

/// `to` written against `from`, with `/` separators. Both are canonicalized as
/// far as they exist, since a link names `generated/shell.json` before the
/// shell has been built and a path that is not there yet still has to resolve.
fn relative(from: &Path, to: &Path) -> Result<String, BuildError> {
  let from = from.canonicalize().map_err(|e| BuildError::Io(from.to_path_buf(), e))?;
  let (base, tail) = anchor(to)?;
  let shared = from.components().zip(base.components()).take_while(|(a, b)| a == b).count();
  let mut parts: Vec<String> = std::iter::repeat_n("..".to_owned(), from.components().count() - shared).collect();
  parts.extend(base.components().skip(shared).map(|c| c.as_os_str().to_string_lossy().into_owned()));
  parts.extend(tail);
  if parts.is_empty() {
    parts.push(".".to_owned());
  }
  Ok(parts.join("/"))
}

/// The deepest ancestor of `path` that exists, canonicalized, and the segments
/// below it that do not.
fn anchor(path: &Path) -> Result<(PathBuf, Vec<String>), BuildError> {
  let mut tail = Vec::new();
  let mut here = path;
  loop {
    if let Ok(real) = here.canonicalize() {
      tail.reverse();
      return Ok((real, tail));
    }
    let name = here.file_name().and_then(|n| n.to_str()).ok_or_else(|| refuse(format!("{} does not resolve", path.display())))?;
    tail.push(name.to_owned());
    here = here.parent().ok_or_else(|| refuse(format!("{} does not resolve", path.display())))?;
  }
}

fn append(path: &Path, section: &str) -> Result<(), BuildError> {
  let mut text = std::fs::read_to_string(path).map_err(|e| BuildError::Io(path.to_path_buf(), e))?;
  if !text.is_empty() && !text.ends_with('\n') {
    text.push('\n');
  }
  text.push_str(section);
  std::fs::write(path, &text).map_err(|e| BuildError::Io(path.to_path_buf(), e))?;
  toml::from_str::<toml::Value>(&text).map_err(|e| refuse(format!("{}: {e}", path.display())))?;
  Ok(())
}

/// Takes an appended section back off the end, for a link that could not finish.
fn truncate(path: &Path, section: &str) -> Result<(), BuildError> {
  let text = std::fs::read_to_string(path).map_err(|e| BuildError::Io(path.to_path_buf(), e))?;
  if let Some(rest) = text.strip_suffix(section) {
    std::fs::write(path, rest).map_err(|e| BuildError::Io(path.to_path_buf(), e))?;
  }
  Ok(())
}

/// Removes the `[header]` table and the lines under it, up to the next table
/// header or the end.
fn remove_section(path: &Path, header: &str) -> Result<(), BuildError> {
  let text = std::fs::read_to_string(path).map_err(|e| BuildError::Io(path.to_path_buf(), e))?;
  let wanted = format!("[{header}]");
  let lines: Vec<&str> = text.lines().collect();
  let Some(start) = lines.iter().position(|l| l.trim() == wanted) else {
    return Err(refuse(format!("{}: no `{wanted}` to remove", path.display())));
  };
  let end = lines[start + 1..].iter().position(|l| l.trim_start().starts_with('[')).map(|i| start + 1 + i).unwrap_or(lines.len());
  let mut kept: Vec<&str> = lines[..start].to_vec();
  while kept.last().is_some_and(|l| l.trim().is_empty()) {
    kept.pop();
  }
  kept.extend_from_slice(&lines[end..]);
  let mut out = kept.join("\n");
  if !out.is_empty() {
    out.push('\n');
  }
  std::fs::write(path, &out).map_err(|e| BuildError::Io(path.to_path_buf(), e))?;
  toml::from_str::<toml::Value>(&out).map_err(|e| refuse(format!("{}: {e}", path.display())))?;
  Ok(())
}

/// Reloads the shell and checks the row is there, so a write that the host
/// would refuse is reported by the command that made it.
fn confirm(root: &Path, name: &str) -> Result<(), BuildError> {
  let config = load(root)?;
  match &config.sites {
    Some(section) if section.mounts.contains_key(name) => Ok(()),
    _ => Err(refuse(format!("{}: `{name}` did not take", root.display()))),
  }
}

/// What `hash` reports: the artifact hash and the files it covers.
#[derive(Debug)]
pub struct Hashed {
  pub name: String,
  pub at: String,
  pub hash: String,
  /// The directories and files that ship, relative to the site's root.
  pub parts: Vec<String>,
  pub files: Vec<snapfire_fsr_sites::Entry>,
  pub bytes: u64,
}

/// The artifact hash of the site at `site`, with the parts and files it covers
/// so a build can see what it is about to ship.
pub fn hash(site: &Path) -> Result<Hashed, BuildError> {
  let config = Config::load(site).map_err(|e| BuildError::Sites(e.to_string()))?;
  let Some(section) = &config.site else {
    return Err(BuildError::Sites(format!("{} has no [site], so it is not a site", site.display())));
  };
  let listing = snapfire_fsr_sites::Listing::of_config(site, &config).map_err(|e| BuildError::Sites(e.to_string()))?;
  Ok(Hashed {
    name: section.name.clone(),
    at: section.at.clone(),
    hash: listing.hash(),
    parts: snapfire_fsr_sites::parts(site, &config),
    bytes: listing.bytes(),
    files: listing.entries,
  })
}

/// What `pack` wrote.
#[derive(Debug)]
pub struct Packed {
  pub manifest: snapfire_fsr_sites::Manifest,
  pub out: PathBuf,
  /// The size of the archive, against the size of what it holds.
  pub bytes: u64,
  pub unpacked: u64,
}

/// Packs the site at `site` as `version` into `out`, defaulting to
/// `<name>-<version>.tar.gz` beside the site.
pub fn pack(site: &Path, version: &str, out: Option<&Path>) -> Result<Packed, BuildError> {
  let hashed = hash(site)?;
  let out = match out {
    Some(path) => path.to_path_buf(),
    None => site.join(format!("{}-{version}.tar.gz", hashed.name)),
  };
  let manifest = snapfire_fsr_sites::pack(site, version, &out).map_err(|e| BuildError::Sites(e.to_string()))?;
  let bytes = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
  Ok(Packed { manifest, out, bytes, unpacked: hashed.bytes })
}

/// Installs the packed artifact at `archive` into the shell's cache, which is
/// `[sites] root`, under `name` when given and the artifact's own name
/// otherwise. Verifies before anything is renamed into place, so a shell
/// running the previous version keeps running it when the archive is wrong.
/// What an install did, and the pin it wrote.
pub struct Installation {
  pub installed: snapfire_fsr_sites::Installed,
  /// The mount that now names this version, when the table has one. An
  /// archive installed before it is linked has nothing to pin.
  pub pinned: Option<Pinned>,
}

/// Installs an archive into the shell's cache and pins the mount that names
/// it. Install is the one moment when computing the hash and meaning to ship
/// that version are the same act, so the pin is written here rather than left
/// as a step someone remembers.
pub fn install(shell: &Path, archive: &Path, name: Option<&str>, keep: Option<usize>, pin_it: bool) -> Result<Installation, BuildError> {
  let config = Config::load(shell).map_err(|e| BuildError::Sites(e.to_string()))?;
  let root = config
    .sites
    .as_ref()
    .and_then(|s| s.root.as_deref())
    .ok_or_else(|| BuildError::Sites(format!("{} has no [sites] root, so it has no cache to install into", shell.display())))?;
  let manifest = snapfire_fsr_sites::Manifest::read_archive(archive).map_err(|e| BuildError::Sites(e.to_string()))?;
  let name = name.unwrap_or(&manifest.name).to_owned();
  let store = snapfire_fsr_sites::ArchiveStore { archive: archive.to_path_buf() };
  let cache = snapfire_fsr_sites::Cache::new(config.root.join(root));
  let installed = cache
    .install(&store, &name, &manifest.name, &manifest.version, keep)
    .map_err(|e| BuildError::Sites(e.to_string()))?;
  // Only the mount that names this very version: installing 1.1.0 while the
  // table still mounts 1.0.0 has installed a version nothing serves yet, and
  // moving the pointer is a separate decision.
  let names_it = config
    .sites
    .as_ref()
    .and_then(|section| section.mounts.get(&name))
    .is_some_and(|mount| mount.artifact == format!("{name}@{}", installed.version));
  let pinned = match pin_it && names_it {
    true => pin(shell, Some(&name))?.into_iter().next(),
    false => None,
  };
  Ok(Installation { installed, pinned })
}

/// Writes `hash` into the shell's `[sites.<name>]` rows: the content hash of
/// the artifact each one resolves to, so the mount is the version someone
/// meant rather than whatever is at the path.
///
/// Only a `name@version` artifact is pinned. A mount naming a path is a linked
/// working tree that changes on every build, and a pin there would be stale by
/// the next one.
pub fn pin(shell: &Path, only: Option<&str>) -> Result<Vec<Pinned>, BuildError> {
  let config = load(shell)?;
  let Some(section) = &config.sites else {
    return Err(refuse(format!("{} mounts no sites", config.root.display())));
  };
  if let Some(name) = only {
    if !section.mounts.contains_key(name) {
      return Err(refuse(format!("`{name}` is not mounted by {}", config.root.display())));
    }
  }
  let file = writable(&config)?;
  let mut pinned = Vec::new();
  for (name, mount) in &section.mounts {
    if only.is_some_and(|wanted| wanted != name) {
      continue;
    }
    let versioned = mount.artifact.contains('@') && !mount.artifact.contains('/');
    if !versioned {
      continue;
    }
    let (artifact_name, version) = mount.artifact.split_once('@').expect("an @");
    let root = section
      .root
      .as_deref()
      .ok_or_else(|| refuse(format!("sites.{name}.artifact names a version, which needs sites.root")))?;
    let artifact = config.root.join(root).join(artifact_name).join(version);
    if !artifact.is_dir() {
      return Err(refuse(format!("sites.{name}: {} is not a directory", artifact.display())));
    }
    let hash = snapfire_fsr_sites::hash_dir(&artifact).map_err(|e| BuildError::Sites(e.to_string()))?;
    let entry = Pinned { name: name.clone(), hash: hash.clone(), was: mount.hash.clone(), artifact };
    if entry.moved() {
      set_key(&file, &format!("sites.{name}"), "hash", &hash)?;
    }
    pinned.push(entry);
  }
  Ok(pinned)
}

/// Sets `key = "value"` inside `[header]`, replacing the line when it is there
/// and adding it under the header when it is not.
fn set_key(path: &Path, header: &str, key: &str, value: &str) -> Result<(), BuildError> {
  let text = std::fs::read_to_string(path).map_err(|e| BuildError::Io(path.to_path_buf(), e))?;
  let wanted = format!("[{header}]");
  let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
  let Some(start) = lines.iter().position(|l| l.trim() == wanted) else {
    return Err(refuse(format!("{}: no [{header}] to set `{key}` in", path.display())));
  };
  let end = lines[start + 1..]
    .iter()
    .position(|l| l.trim_start().starts_with('['))
    .map(|at| start + 1 + at)
    .unwrap_or(lines.len());
  let row = format!("{key} = \"{value}\"");
  match lines[start + 1..end].iter().position(|l| l.trim_start().starts_with(&format!("{key} "))) {
    Some(at) => lines[start + 1 + at] = row,
    None => lines.insert(start + 1, row),
  }
  let mut out = lines.join("\n");
  out.push('\n');
  std::fs::write(path, &out).map_err(|e| BuildError::Io(path.to_path_buf(), e))?;
  toml::from_str::<toml::Value>(&out).map_err(|e| refuse(format!("{}: {e}", path.display())))?;
  Ok(())
}

/// Every version of every site the shell's cache holds, against what its table
/// mounts.
pub fn cached(shell: &Path) -> Result<Vec<(String, Vec<String>)>, BuildError> {
  let config = Config::load(shell).map_err(|e| BuildError::Sites(e.to_string()))?;
  let Some(root) = config.sites.as_ref().and_then(|s| s.root.as_deref()) else { return Ok(Vec::new()) };
  let cache = snapfire_fsr_sites::Cache::new(config.root.join(root));
  let Ok(read) = std::fs::read_dir(&cache.root) else { return Ok(Vec::new()) };
  let mut names: Vec<String> = read
    .flatten()
    .filter(|e| e.path().is_dir())
    .map(|e| e.file_name().to_string_lossy().into_owned())
    .filter(|name| !name.starts_with('.'))
    .collect();
  names.sort();
  Ok(names.into_iter().map(|name| { let versions = cache.versions(&name); (name, versions) }).collect())
}

// ---------------------------------------------------------- a running shell

/// One site as an instance reports it, which is what it is serving rather than
/// what a table asked for.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct Mounted {
  pub name: String,
  pub at: String,
  pub version: String,
  pub hash: String,
}

/// What one instance answered.
#[derive(Debug, Clone)]
pub struct Instance {
  /// The host as it was given, so a report names what the caller named.
  pub host: String,
  pub sites: Vec<Mounted>,
}

/// A site the table names, against every instance asked about it.
#[derive(Debug, Clone)]
pub struct Compared {
  pub row: Row,
  /// Host, what it serves, and whether that is what the table says.
  pub against: Vec<(String, Option<Mounted>)>,
}

impl Compared {
  /// Whether every instance serves what the table names. An instance that does
  /// not mount the site at all does not agree either.
  pub fn agrees(&self) -> bool {
    self.against.iter().all(|(_, mounted)| {
      mounted.as_ref().is_some_and(|m| m.version == self.row.version && m.hash == self.row.hash)
    })
  }
}

/// What one instance did when asked to reload.
#[derive(Debug, Clone)]
pub struct Reloaded {
  pub host: String,
  /// The sites it serves now, when it reloaded.
  pub sites: Option<Vec<Mounted>>,
  /// Why it refused, when it did. The host answers 409 with the reason, which
  /// is the whole value of the route over a signal.
  pub refused: Option<String>,
}

impl Reloaded {
  pub fn ok(&self) -> bool {
    self.refused.is_none()
  }
}

/// A `--header 'Name: Value'` as the two halves, refusing one with no colon
/// rather than sending a header the caller did not mean.
pub fn header(raw: &str) -> Result<(String, String), BuildError> {
  let (name, value) = raw.split_once(':').ok_or_else(|| refuse(format!("`{raw}` is not `Name: Value`")))?;
  let (name, value) = (name.trim(), value.trim());
  if name.is_empty() || value.is_empty() {
    return Err(refuse(format!("`{raw}` is not `Name: Value`")));
  }
  Ok((name.to_owned(), value.to_owned()))
}

/// `http://<host>` unless the caller already said which scheme.
fn url(host: &str, path: &str) -> String {
  match host.contains("://") {
    true => format!("{}{path}", host.trim_end_matches('/')),
    false => format!("http://{}{path}", host.trim_end_matches('/')),
  }
}

fn send(
  method: reqwest::Method,
  host: &str,
  path: &str,
  headers: &[(String, String)],
) -> Result<(reqwest::StatusCode, String), BuildError> {
  let target = url(host, path);
  let client = crate::vendor::client()?;
  let mut request = client.request(method, &target);
  for (name, value) in headers {
    request = request.header(name.as_str(), value.as_str());
  }
  let response = request.send().map_err(|e| BuildError::Http(target.clone(), e.to_string()))?;
  let status = response.status();
  let body = response.text().map_err(|e| BuildError::Http(target.clone(), e.to_string()))?;
  if status == reqwest::StatusCode::NOT_FOUND {
    return Err(refuse(format!(
      "{target}: no such route. The host serves it only when it was built with the `sites_reload` feature and the application installed a sites mounter"
    )));
  }
  Ok((status, body))
}

/// `GET /__fsr/sites` on one instance: what it is serving now.
pub fn mounted(host: &str, headers: &[(String, String)]) -> Result<Instance, BuildError> {
  let (status, body) = send(reqwest::Method::GET, host, "/__fsr/sites", headers)?;
  if !status.is_success() {
    return Err(refuse(format!("{}: HTTP {status}", url(host, "/__fsr/sites"))));
  }
  #[derive(serde::Deserialize)]
  struct Answer {
    sites: Vec<Mounted>,
  }
  let answer: Answer = serde_json::from_str(&body)
    .map_err(|e| refuse(format!("{}: {e}", url(host, "/__fsr/sites"))))?;
  Ok(Instance { host: host.to_owned(), sites: answer.sites })
}

/// The shell's table against what every instance is serving, which is the
/// comparison a fleet is watched with.
pub fn compare(shell: &Path, hosts: &[String], headers: &[(String, String)]) -> Result<Vec<Compared>, BuildError> {
  let rows = list(shell)?;
  let mut instances = Vec::new();
  for host in hosts {
    instances.push(mounted(host, headers)?);
  }
  Ok(
    rows
      .into_iter()
      .map(|row| {
        let against = instances
          .iter()
          .map(|instance| (instance.host.clone(), instance.sites.iter().find(|s| s.name == row.name).cloned()))
          .collect();
        Compared { row, against }
      })
      .collect(),
  )
}

/// `POST /__fsr/sites/reload` on each instance in turn.
///
/// One at a time, stopping at the first refusal unless `all`: a 409 says what
/// was published is bad, so carrying on ships it to the rest of the fleet.
pub fn reload(hosts: &[String], headers: &[(String, String)], all: bool) -> Result<Vec<Reloaded>, BuildError> {
  #[derive(serde::Deserialize)]
  struct Answer {
    #[serde(default)]
    sites: Vec<Mounted>,
    #[serde(default)]
    error: Option<String>,
  }
  let mut out = Vec::new();
  for host in hosts {
    let (status, body) = send(reqwest::Method::POST, host, "/__fsr/sites/reload", headers)?;
    let answer: Answer = serde_json::from_str(&body)
      .map_err(|e| refuse(format!("{}: {e}", url(host, "/__fsr/sites/reload"))))?;
    let refused = match status == reqwest::StatusCode::CONFLICT {
      true => Some(answer.error.unwrap_or_else(|| format!("HTTP {status}"))),
      false if !status.is_success() => Some(format!("HTTP {status}")),
      false => None,
    };
    let stop = refused.is_some() && !all;
    out.push(Reloaded { host: host.clone(), sites: refused.is_none().then_some(answer.sites), refused });
    if stop {
      break;
    }
  }
  Ok(out)
}

/// Where to ask, given what the caller said: every `--host`, else the shell's
/// own `server.listen` when a shell was named.
pub fn hosts_for(shell: Option<&Path>, given: &[String]) -> Result<Vec<String>, BuildError> {
  if !given.is_empty() {
    return Ok(given.to_vec());
  }
  let Some(shell) = shell else {
    return Err(refuse("name a host with --host, or a shell directory to read `server.listen` from".to_owned()));
  };
  Ok(vec![load(shell)?.server.listen.clone()])
}
