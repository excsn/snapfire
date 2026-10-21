//! JSX written where a value goes, one page per pattern under `cases/`, each built as an application of its own from `template/`. `tests/outcomes.rs` holds what the build makes of each against `outcomes.tsv`; `benches/patterns.rs` renders the ones that build with the IR and with React and records whether the two agree and what each costs.

use std::path::{Path, PathBuf};

use snapfire_fsr_cli::{build, Built, Options};
use snapfire_fsr_ir::render::{Rendered, ISLAND_MARK};

pub mod react;

pub struct Case {
  pub name: String,
  pub source: String,
}

pub fn root() -> PathBuf {
  Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// Every case, by name.
pub fn cases() -> Vec<Case> {
  let mut out: Vec<Case> = std::fs::read_dir(root().join("cases"))
    .expect("cases/")
    .flatten()
    .filter_map(|e| {
      let path = e.path();
      let name = path.file_stem()?.to_string_lossy().into_owned();
      (path.extension()? == "tsx").then(|| Case { name, source: std::fs::read_to_string(&path).expect("a case reads") })
    })
    .collect();
  out.sort_by(|a, b| a.name.cmp(&b.name));
  out
}

pub(crate) fn copy(from: &Path, to: &Path) {
  std::fs::create_dir_all(to).unwrap();
  for entry in std::fs::read_dir(from).unwrap().flatten() {
    let target = to.join(entry.file_name());
    if entry.file_type().unwrap().is_dir() {
      copy(&entry.path(), &target);
    } else {
      std::fs::copy(entry.path(), target).unwrap();
    }
  }
}

/// `case` laid out as an application whose only page is the case, under `scratch`.
pub fn app_of(case: &Case, scratch: &Path) -> PathBuf {
  let dir = scratch.join(&case.name);
  let _ = std::fs::remove_dir_all(&dir);
  copy(&root().join("template"), &dir);
  std::fs::write(dir.join("app/routes/page.tsx"), &case.source).unwrap();
  dir.join("app")
}

pub const PAGE: &str = "routes/page.tsx#default";

/// What the build made of the case: `lowered`, `client:` with the causes that sent a component of the case to the browser, or `refused:` with the error.
pub fn outcome(app: &Path) -> (String, Option<Built>) {
  match build(app, &Options::default()) {
    Err(e) => (format!("refused: {}", e.to_string().lines().next().unwrap_or_default().replace(&app.display().to_string(), "<app>")), None),
    Ok(built) => {
      let mut causes: Vec<String> = built.report.causes.iter().filter(|c| c.at.starts_with("routes/page")).map(|c| format!("{} {}", c.at, c.message)).collect();
      causes.sort();
      causes.dedup();
      let line = if causes.is_empty() { "lowered".to_owned() } else { format!("client: {}", causes.join(" | ")) };
      (line, Some(built))
    }
  }
}

/// The page's markup with every island's body spliced into its marker, which is what React writes.
pub fn flatten(rendered: &Rendered) -> String {
  let mut out = String::with_capacity(rendered.html.len());
  let mut rest = rendered.html.as_str();
  while let Some(start) = rest.find(ISLAND_MARK) {
    out.push_str(&rest[..start]);
    let after = &rest[start + ISLAND_MARK.len()..];
    let end = after.find('\0').expect("a marker is closed");
    let index: usize = after[..end].parse().expect("a marker carries its index");
    out.push_str(&flatten(&rendered.islands[index].body));
    rest = &after[end + 1..];
  }
  out.push_str(rest);
  out
}

/// The IR's markup without what FSR adds for the browser and React never writes: an `sf-s` or `sf-i` region kept with what it holds, a `<template data-sf-…>` held for later and a `<script data-sf-…>` of props, both dropped with their contents. What is left is what a viewer sees before the scripts run.
pub fn without_fsr_markers(html: &str) -> String {
  let mut out = String::with_capacity(html.len());
  let mut rest = html;
  while let Some(lt) = rest.find('<') {
    out.push_str(&rest[..lt]);
    let after = &rest[lt..];
    let Some(gt) = after.find('>') else {
      out.push_str(after);
      return out;
    };
    let tag = &after[1..gt];
    let name = tag.trim_start_matches('/').split([' ', '>']).next().unwrap_or_default();
    match name {
      "sf-s" | "sf-i" => rest = &after[gt + 1..],
      "template" | "script" if !tag.starts_with('/') && tag.contains(" data-sf-") => {
        let close = format!("</{name}>");
        rest = match after.find(&close) {
          Some(end) => &after[end + close.len()..],
          None => "",
        };
      }
      _ => {
        out.push_str(&after[..=gt]);
        rest = &after[gt + 1..];
      }
    }
  }
  out.push_str(rest);
  out
}

/// Both renderings with what legitimately differs taken out: React's `<!-- -->` between adjacent text, its `&#x27;` for an apostrophe, a void element's `/`, attribute order and case.
pub fn normalised(html: &str) -> String {
  let text = html.replace("<!-- -->", "").replace("&#x27;", "'");
  let mut out = String::with_capacity(text.len());
  let mut rest = text.as_str();
  while let Some(lt) = rest.find('<') {
    out.push_str(&rest[..lt]);
    let Some(gt) = rest[lt..].find('>') else { break };
    let tag = &rest[lt + 1..lt + gt];
    rest = &rest[lt + gt + 1..];
    if tag.starts_with('/') || tag.starts_with('!') {
      out.push('<');
      out.push_str(tag);
      out.push('>');
      continue;
    }
    let tag = tag.trim_end_matches('/');
    let (name, attrs) = tag.split_once(' ').unwrap_or((tag, ""));
    let mut attrs: Vec<String> = split_attrs(attrs)
      .into_iter()
      .filter(|a| a != "/")
      .map(|a| match a.split_once('=') {
        Some((k, v)) => format!("{}={v}", k.to_ascii_lowercase()),
        None => a.to_ascii_lowercase(),
      })
      .collect();
    attrs.sort();
    out.push('<');
    out.push_str(name);
    for attr in attrs {
      out.push(' ');
      out.push_str(&attr);
    }
    out.push('>');
  }
  out.push_str(rest);
  out
}

fn split_attrs(attrs: &str) -> Vec<String> {
  let mut out = Vec::new();
  let mut current = String::new();
  let mut quoted = false;
  for ch in attrs.chars() {
    match ch {
      '"' => {
        quoted = !quoted;
        current.push(ch)
      }
      ' ' if !quoted => {
        if !current.is_empty() {
          out.push(std::mem::take(&mut current))
        }
      }
      _ => current.push(ch),
    }
  }
  if !current.is_empty() {
    out.push(current)
  }
  out
}
