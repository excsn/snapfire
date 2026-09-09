//! `fsr doctor`: what the host would not refuse to start over.
//!
//! The boot already errors on a declared action nothing answers, a bundle
//! carrying a server module and a route claimed twice, so none of that belongs
//! here. What belongs here is the middle: a deployment that starts and serves,
//! with a setting that cannot do what it was written for. Every check answers
//! from what a build already computed, and every finding names its remedy.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use snapfire_fsr_host::config::Config;
use snapfire_fsr_ir::ast::{Expr, Tmpl};
use snapfire_fsr_plan::Manifest;

#[derive(Debug, thiserror::Error)]
pub enum DoctorError {
  #[error("{0}: {1}")]
  Io(PathBuf, std::io::Error),
  #[error("{0}")]
  Config(String),
  #[error("{0}: {1}")]
  Plan(PathBuf, String),
}

/// One thing that is wrong, what it means and what to do about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
  /// A short name for the check, so a report can be grepped.
  pub check: &'static str,
  pub what: String,
  pub remedy: String,
}

impl Finding {
  fn new(check: &'static str, what: impl Into<String>, remedy: impl Into<String>) -> Self {
    Self { check, what: what.into(), remedy: remedy.into() }
  }
}

#[derive(Debug, Default)]
pub struct Report {
  pub findings: Vec<Finding>,
  /// The checks that ran and found nothing, so a clean report still says what
  /// it looked at.
  pub clean: Vec<&'static str>,
}

impl Report {
  pub fn is_clean(&self) -> bool {
    self.findings.is_empty()
  }
}

impl std::fmt::Display for Report {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    for finding in &self.findings {
      writeln!(f, "{:<12} {}", finding.check, finding.what)?;
      writeln!(f, "{:<12} {}", "", finding.remedy)?;
    }
    if self.findings.is_empty() {
      writeln!(f, "{:<12} {} checks, nothing to report", "doctor", self.clean.len())?;
    } else {
      writeln!(
        f,
        "{:<12} {} of {} checks found something",
        "doctor",
        self.findings.len(),
        self.findings.len() + self.clean.len()
      )?;
    }
    Ok(())
  }
}

/// Runs every check over the application at `app`, which is a directory
/// holding an `app.toml` or a project root that names one.
pub fn run(app: &Path) -> Result<Report, DoctorError> {
  let config = Config::load(app).map_err(|e| DoctorError::Config(e.to_string()))?;
  let manifest = read_plan(&config)?;
  let mut report = Report::default();
  for (check, findings) in [
    ("canonical", canonical(&config)),
    ("ctx.host", host_reads(&config, manifest.as_ref())),
    ("locales", catalogs(&config)),
    ("stale", stale(&config)),
    ("vendor", vendor(&config)),
    ("render", render_mode(&config, manifest.as_ref())),
    ("statics", statics(&config)),
    ("sites", sites(&config)),
  ] {
    if findings.is_empty() {
      report.clean.push(check);
    } else {
      report.findings.extend(findings);
    }
  }
  Ok(report)
}

/// The plan as the host would read it, or `None` when there is none to read:
/// an application that has not been built yet is a `stale` finding rather than
/// a failure of every check that wanted a plan.
fn read_plan(config: &Config) -> Result<Option<Manifest>, DoctorError> {
  let path = config.resolve(&config.server.plan);
  let Ok(text) = std::fs::read_to_string(&path) else { return Ok(None) };
  Manifest::from_text(&text)
    .map(Some)
    .map_err(|e| DoctorError::Plan(path, e.to_string()))
}

/// A canonical link is absolute only when the document names an origin, and a
/// relative one is what an audit reports.
fn canonical(config: &Config) -> Vec<Finding> {
  if config.document.origin.is_some() {
    return Vec::new();
  }
  let hosts = config.server.hosts.len();
  let prerendered = config.server.prerender.is_some();
  if hosts == 0 && !prerendered {
    return Vec::new();
  }
  let because = match (hosts, prerendered) {
    (0, _) => "this application prerenders".to_owned(),
    (1, _) => format!("`server.hosts` names {}", config.server.hosts[0]),
    (n, _) => format!("`server.hosts` names {n} hosts"),
  };
  vec![Finding::new(
    "canonical",
    format!("`document.origin` is unset while {because}, so every canonical and alternate link is relative"),
    "set `[document] origin` to the address this deployment is reached at, `https://example.com`",
  )]
}

/// `ctx.host` answers null unless the deployment lists what it answers on, so
/// a body that reads it against an empty list reads nothing, for ever.
fn host_reads(config: &Config, manifest: Option<&Manifest>) -> Vec<Finding> {
  if !config.server.hosts.is_empty() {
    return Vec::new();
  }
  let Some(manifest) = manifest else { return Vec::new() };
  if !reads_host(manifest) {
    return Vec::new();
  }
  vec![Finding::new(
    "ctx.host",
    "a body reads `ctx.host` while `server.hosts` is empty, so it always answers null",
    "list the hosts this deployment answers on in `[server] hosts`, and make sure the server in front sets the header",
  )]
}

/// Every locale the table names needs a catalog, or `t` falls back for a
/// language the application says it supports.
fn catalogs(config: &Config) -> Vec<Finding> {
  let Some(locales) = &config.locales else { return Vec::new() };
  let dir = config.app.join("locales");
  let mut missing = Vec::new();
  for tag in &locales.supported {
    if !dir.join(format!("{tag}.toml")).exists() {
      missing.push(tag.clone());
    }
  }
  if missing.is_empty() {
    return Vec::new();
  }
  vec![Finding::new(
    "locales",
    format!("`locales.supported` names {} with no catalog under {}", missing.join(", "), dir.display()),
    format!("write {} , or take the locale out of `supported`", missing.iter().map(|t| format!("locales/{t}.toml")).collect::<Vec<_>>().join(" and ")),
  )]
}

/// A plan older than the sources it was lowered from is a plan that answers
/// yesterday's routes.
fn stale(config: &Config) -> Vec<Finding> {
  let plan = config.resolve(&config.server.plan);
  let Ok(meta) = std::fs::metadata(&plan) else {
    return vec![Finding::new(
      "stale",
      format!("{} does not exist", plan.display()),
      "run `fsr build <app dir>`",
    )];
  };
  let Ok(built) = meta.modified() else { return Vec::new() };
  let mut newer = BTreeSet::new();
  for dir in ["routes", "src", "clients", "schemas"] {
    let root = config.app.join(dir);
    if newest(&root).is_some_and(|t| t > built) {
      newer.insert(dir);
    }
  }
  if newer.is_empty() {
    return Vec::new();
  }
  vec![Finding::new(
    "stale",
    format!(
      "{} is older than {}",
      plan.display(),
      newer.into_iter().map(|d| format!("`{d}/`")).collect::<Vec<_>>().join(" and ")
    ),
    "run `fsr build <app dir>`; the host reads the plan and never the sources",
  )]
}

/// The newest modification time anywhere under `root`.
fn newest(root: &Path) -> Option<std::time::SystemTime> {
  let mut best: Option<std::time::SystemTime> = None;
  let mut stack = vec![root.to_path_buf()];
  while let Some(dir) = stack.pop() {
    let Ok(entries) = std::fs::read_dir(&dir) else { continue };
    for entry in entries.flatten() {
      let path = entry.path();
      let Ok(meta) = entry.metadata() else { continue };
      if meta.is_dir() {
        stack.push(path);
        continue;
      }
      if let Ok(time) = meta.modified() {
        best = Some(best.map_or(time, |b: std::time::SystemTime| b.max(time)));
      }
    }
  }
  best
}

/// An import map naming a file the browser will ask for and not find.
fn vendor(config: &Config) -> Vec<Finding> {
  let Some(map) = &config.document.import_map else { return Vec::new() };
  let path = config.resolve(map);
  let Ok(text) = std::fs::read_to_string(&path) else {
    return vec![Finding::new(
      "vendor",
      format!("`document.import_map` names {} which does not exist", path.display()),
      "run `fsr build <app dir>`, or correct `[document] import_map`",
    )];
  };
  let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
    return vec![Finding::new(
      "vendor",
      format!("{} is not JSON", path.display()),
      "correct the import map, which is `{\"imports\": {\"<name>\": \"<url>\"}}`",
    )];
  };
  let Some(imports) = json.get("imports").and_then(|i| i.as_object()) else { return Vec::new() };
  let mut missing = Vec::new();
  for (name, target) in imports {
    let Some(target) = target.as_str() else { continue };
    let Some(rest) = target.split_once("/vendor/").map(|(_, rest)| rest) else { continue };
    if !config.app.join("vendor").join(rest).exists() {
      missing.push(name.clone());
    }
  }
  if missing.is_empty() {
    return Vec::new();
  }
  vec![Finding::new(
    "vendor",
    format!("the import map names {} with nothing under `vendor/` to answer it", missing.join(", ")),
    format!("run `fsr add <app dir> {}@<version>`", missing[0]),
  )]
}

/// `server.render = "islands"` on an application with no island renders every
/// page in the browser for no reason.
fn render_mode(config: &Config, manifest: Option<&Manifest>) -> Vec<Finding> {
  if config.server.render != "islands" {
    return Vec::new();
  }
  let Some(manifest) = manifest else { return Vec::new() };
  if manifest.components.iter().any(|entry| has_island(&entry.body.render)) {
    return Vec::new();
  }
  vec![Finding::new(
    "render",
    "`server.render` is `islands` and the plan carries no island",
    "set `[server] render = \"rust\"` to render pages on the server, or place an island",
  )]
}

fn has_island(tmpl: &Tmpl) -> bool {
  match tmpl {
    Tmpl::Island { .. } => true,
    Tmpl::Element { children, .. } | Tmpl::Fragment(children) | Tmpl::Component { children, .. } | Tmpl::Baked { children, .. } => {
      children.iter().any(has_island)
    }
    Tmpl::If { then, r#else, .. } => has_island(then) || r#else.as_deref().is_some_and(has_island),
    Tmpl::For { body, .. } => has_island(body),
    Tmpl::Let { then, .. } => has_island(then),
    Tmpl::Text(_) | Tmpl::Expr(_) | Tmpl::Slot(_) => false,
  }
}

/// Whether any body or render tree the plan carries reads `ctx.host`.
fn reads_host(manifest: &Manifest) -> bool {
  let mut found = false;
  let mut look = |expr: &Expr| {
    if matches!(expr, Expr::Host) {
      found = true;
    }
  };
  for row in &manifest.sources {
    for body in [&row.body, &row.meta, &row.store].into_iter().flatten() {
      snapfire_fsr_ir::body_visit(body, &mut look);
    }
  }
  for row in &manifest.actions {
    if let Some(body) = &row.body {
      snapfire_fsr_ir::body_visit(body, &mut look);
    }
  }
  for row in &manifest.handlers {
    if let Some(body) = &row.body {
      snapfire_fsr_ir::body_visit(body, &mut look);
    }
  }
  if let Some(body) = &manifest.middleware {
    snapfire_fsr_ir::body_visit(body, &mut look);
  }
  for entry in &manifest.components {
    entry.body.visit(&mut look);
  }
  found
}

/// A static root that is not there answers 404 for everything under its route.
fn statics(config: &Config) -> Vec<Finding> {
  let mut missing = Vec::new();
  for root in &config.statics {
    if !config.resolve(&root.dir).is_dir() {
      missing.push(format!("`{}` serving {}", root.route, config.resolve(&root.dir).display()));
    }
  }
  if missing.is_empty() {
    return Vec::new();
  }
  vec![Finding::new(
    "statics",
    format!("a static root has no directory: {}", missing.join(", ")),
    "create the directory or correct `[[statics]] dir`; a missing root answers 404 for every path under its route",
  )]
}

/// The mounted sites, which a shell serves and never builds, so nothing about
/// them is checked until one is asked for.
fn sites(config: &Config) -> Vec<Finding> {
  let Some(section) = &config.sites else { return Vec::new() };
  if section.mounts.is_empty() {
    return Vec::new();
  }
  let resolved = match snapfire_fsr_sites::resolve(config) {
    Ok(resolved) => resolved,
    // The host refuses to start over this; saying so here is saying it before
    // the deploy rather than instead of it.
    Err(e) => {
      return vec![Finding::new(
        "sites",
        format!("the host will refuse to start: {e}"),
        "correct the artifact the mount names, or repin it with `fsr sites hash <site dir>`",
      )]
    }
  };
  let mut findings = Vec::new();
  let mut unpinned = Vec::new();
  for site in &resolved {
    if section.mounts.get(&site.name).is_some_and(|m| m.hash.is_none()) {
      unpinned.push(site.name.clone());
    }
    findings.extend(site_artifact(site));
  }
  if !unpinned.is_empty() {
    findings.push(Finding::new(
      "sites",
      format!("{} pins no hash, so any content under the artifact is mounted", unpinned.join(", ")),
      format!("take the hash with `fsr sites hash <site dir>` and set `hash` on the mount, for {}", unpinned[0]),
    ));
  }
  findings.extend(orphans(config, section, &resolved));
  findings
}

/// One artifact: what its own configuration says it ships, and whether its
/// plan is older than the sources beside it.
fn site_artifact(site: &snapfire_fsr_sites::Resolved) -> Vec<Finding> {
  let Ok(config) = Config::load(&site.artifact) else { return Vec::new() };
  let mut findings = Vec::new();
  let absent: Vec<String> = snapfire_fsr_sites::parts(&site.artifact, &config)
    .into_iter()
    .filter(|part| !site.artifact.join(part).exists())
    .collect();
  if !absent.is_empty() {
    findings.push(Finding::new(
      "sites",
      format!("the site `{}` ships {} and it is not in the artifact", site.name, absent.join(", ")),
      "rebuild the site and pack it again with `fsr sites pack <site dir> --version <version>`; a part that is absent is hashed as absent and answers 404",
    ));
  }
  let plan = config.resolve(&config.server.plan);
  if !plan.exists() {
    findings.push(Finding::new(
      "sites",
      format!("the site `{}` has no plan at {}", site.name, plan.display()),
      "run `fsr build` in the site before packing it",
    ));
  } else if let Some(built) = std::fs::metadata(&plan).ok().and_then(|m| m.modified().ok()) {
    let stale: Vec<&str> = ["routes", "src"]
      .into_iter()
      .filter(|dir| newest(&config.app.join(dir)).is_some_and(|t| t > built))
      .collect();
    if !stale.is_empty() {
      findings.push(Finding::new(
        "sites",
        format!("the site `{}` has a plan older than {}", site.name, stale.iter().map(|d| format!("`{d}/`")).collect::<Vec<_>>().join(" and ")),
        "run `fsr build` in the site; a shell serves the plan the artifact carries",
      ));
    }
  }
  findings
}

/// Artifacts under the root that no mount names: what an install leaves behind
/// and a table never picked up.
fn orphans(
  config: &Config,
  section: &snapfire_fsr_host::config::SitesSection,
  resolved: &[snapfire_fsr_sites::Resolved],
) -> Vec<Finding> {
  let Some(root) = &section.root else { return Vec::new() };
  let root = config.root.join(root);
  let Ok(entries) = std::fs::read_dir(&root) else { return Vec::new() };
  let mounted: BTreeSet<PathBuf> = resolved.iter().map(|s| s.artifact.clone()).collect();
  let mut orphans = Vec::new();
  for name in entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()) {
    let Ok(versions) = std::fs::read_dir(&name) else { continue };
    for version in versions.flatten().map(|e| e.path()).filter(|p| p.is_dir()) {
      if !mounted.contains(&version) {
        orphans.push(format!(
          "{}@{}",
          name.file_name().unwrap_or_default().to_string_lossy(),
          version.file_name().unwrap_or_default().to_string_lossy()
        ));
      }
    }
  }
  if orphans.is_empty() {
    return Vec::new();
  }
  orphans.sort();
  vec![Finding::new(
    "sites",
    format!("{} sits under the sites root with no mount naming it: {}", orphans.len(), orphans.join(", ")),
    "point a mount at it, or drop it; `fsr sites install --keep <n>` bounds what is kept",
  )]
}
