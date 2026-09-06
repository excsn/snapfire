//! `fsr sites`: the shell's table and the site's own section written as a
//! pair, so mounting an application is one command rather than two files
//! edited by hand. A link writes `[site]` beside the site and `[sites.<name>]`
//! beside the shell; an unlink takes both back out.

use std::path::{Path, PathBuf};

use snapfire_fsr_host::config::Config;

use crate::BuildError;

/// One row of a shell's table as `list` reports it.
#[derive(Debug)]
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
