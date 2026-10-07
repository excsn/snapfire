//! `fsr census`: what the lowering refuses or leaves to the browser across
//! applications, counted by cause.

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

use crate::plugins::Plugins;
use crate::{build_in, Options};

/// One place a cause was met.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sighting {
  pub app: String,
  /// `file:line:column`, or the file alone when it did not parse.
  pub at: String,
  pub message: String,
}

/// Every cause met, keyed by the cause with each backticked name written `…`,
/// so one construct met under different names counts once.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Census {
  pub apps: usize,
  /// Residue in a loader, an action, a handler, middleware or an extension,
  /// which `fsr build` stops on.
  pub refused: BTreeMap<String, Vec<Sighting>>,
  /// A component the build leaves to render in the browser alone.
  pub client: BTreeMap<String, Vec<Sighting>>,
  /// A component another framework renders, which the build does not lower.
  pub foreign: BTreeMap<String, Vec<Sighting>>,
  /// An application whose build failed for something other than residue.
  pub failed: Vec<(String, String)>,
}

/// The census of `apps`, each built in full with every residue recorded
/// rather than stopping the build. Nothing is written.
pub fn run(apps: &[PathBuf]) -> Census {
  let mut census = Census { apps: apps.len(), ..Census::default() };
  let mut plugins = Plugins::new();
  for app in apps {
    let name = app.display().to_string();
    let mut skipped = Some(Vec::new());
    let built = build_in(app, &Options::beside(app), &mut plugins, &mut skipped);
    for residue in skipped.unwrap_or_default() {
      let at = format!("{}:{}:{}", residue.file, residue.line, residue.column);
      file(&mut census.refused, &name, at, residue.message);
    }
    match built {
      Ok(built) => {
        for cause in &built.report.causes {
          file(&mut census.client, &name, cause.at.clone(), cause.message.clone());
        }
        for cause in &built.report.foreign {
          file(&mut census.foreign, &name, cause.at.clone(), cause.message.clone());
        }
      }
      Err(e) => census.failed.push((name, e.to_string())),
    }
  }
  census
}

fn file(into: &mut BTreeMap<String, Vec<Sighting>>, app: &str, at: String, message: String) {
  into.entry(cause_of(&message)).or_default().push(Sighting { app: app.to_owned(), at, message });
}

/// `message` with each backticked span written `` `…` ``.
pub fn cause_of(message: &str) -> String {
  let mut out = String::with_capacity(message.len());
  let mut inside = false;
  for c in message.chars() {
    match (c, inside) {
      ('`', false) => {
        out.push_str("`…");
        inside = true;
      }
      ('`', true) => {
        out.push('`');
        inside = false;
      }
      (_, true) => {}
      (c, false) => out.push(c),
    }
  }
  if inside {
    out.push('`');
  }
  out
}

impl fmt::Display for Census {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    writeln!(f, "census of {} application{}", self.apps, if self.apps == 1 { "" } else { "s" })?;
    let kinds = [("refused", &self.refused), ("client", &self.client), ("foreign", &self.foreign)];
    for (kind, causes) in kinds {
      let mut ranked: Vec<(&String, &Vec<Sighting>)> = causes.iter().collect();
      ranked.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then(a.0.cmp(b.0)));
      for (i, (cause, sightings)) in ranked.into_iter().enumerate() {
        let label = if i == 0 { kind } else { "" };
        writeln!(f, "{label:<9} {:>4}  {cause}", sightings.len())?;
        for sighting in sightings.iter().take(3) {
          writeln!(f, "{:<16}{}  {}", "", sighting.at, sighting.app)?;
        }
        if sightings.len() > 3 {
          writeln!(f, "{:<16}and {} more", "", sightings.len() - 3)?;
        }
      }
    }
    for (i, (app, error)) in self.failed.iter().enumerate() {
      let label = if i == 0 { "failed" } else { "" };
      writeln!(f, "{label:<9} {app}: {error}")?;
    }
    if self.refused.is_empty() && self.client.is_empty() && self.foreign.is_empty() && self.failed.is_empty() {
      writeln!(f, "every module lowered")?;
    }
    Ok(())
  }
}
