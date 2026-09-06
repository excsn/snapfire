//! Running the compiler and reading what it printed.

use std::path::Path;
use std::process::Command;

use crate::{Diagnostic, Error, Severity};

/// Runs `tsc` over `config` with `root` as the working directory, so every
/// diagnostic's file is relative to the project.
pub fn check(tsc: &Path, root: &Path, config: &Path) -> Result<Vec<Diagnostic>, Error> {
  let output = Command::new(tsc)
    .current_dir(root)
    .args(["--noEmit", "--pretty", "false", "-p"])
    .arg(config)
    .output()
    .map_err(|e| Error::Spawn { path: tsc.to_path_buf(), source: e })?;
  let stdout = String::from_utf8_lossy(&output.stdout);
  let diagnostics = parse(&stdout);
  if diagnostics.is_empty() && !output.status.success() {
    let stderr = String::from_utf8_lossy(&output.stderr);
    return Err(Error::Tsc { path: tsc.to_path_buf(), status: output.status.to_string(), output: format!("{stdout}{stderr}") });
  }
  Ok(diagnostics)
}

/// The lines `tsc --pretty false` prints, as diagnostics. An indented line
/// continues the one above it, and a line in no other shape becomes a
/// diagnostic of its own rather than being dropped.
pub fn parse(text: &str) -> Vec<Diagnostic> {
  let mut out: Vec<Diagnostic> = Vec::new();
  for line in text.lines() {
    if line.trim().is_empty() {
      continue;
    }
    if let Some(diagnostic) = one(line) {
      out.push(diagnostic);
      continue;
    }
    match out.last_mut() {
      Some(previous) => {
        previous.message.push('\n');
        previous.message.push_str(line.trim_end());
      }
      None => out.push(Diagnostic { file: None, line: 0, column: 0, code: String::new(), severity: Severity::Error, message: line.trim().to_owned() }),
    }
  }
  out
}

fn one(line: &str) -> Option<Diagnostic> {
  if line.starts_with(' ') || line.starts_with('\t') {
    return None;
  }
  let (head, rest, severity) = split(line)?;
  let (code, message) = rest.split_once(": ")?;
  if code.is_empty() || !code.bytes().all(|b| b.is_ascii_digit()) {
    return None;
  }
  let (file, at) = match head {
    Some(head) => location(head),
    None => (None, (0, 0)),
  };
  Some(Diagnostic { file, line: at.0, column: at.1, code: format!("TS{code}"), severity, message: message.to_owned() })
}

/// `path(1,2): error TS1: m` splits into the path, `1: m` and the severity; a
/// diagnostic about the project itself has no head.
fn split(line: &str) -> Option<(Option<&str>, &str, Severity)> {
  for (marker, severity) in [(": error TS", Severity::Error), (": warning TS", Severity::Warning)] {
    if let Some(at) = line.find(marker) {
      return Some((Some(&line[..at]), &line[at + marker.len()..], severity));
    }
  }
  for (prefix, severity) in [("error TS", Severity::Error), ("warning TS", Severity::Warning)] {
    if let Some(rest) = line.strip_prefix(prefix) {
      return Some((None, rest, severity));
    }
  }
  None
}

fn location(head: &str) -> (Option<String>, (u32, u32)) {
  let parsed = head.rfind('(').and_then(|open| {
    let close = head.rfind(')')?;
    if close < open {
      return None;
    }
    let (line, column) = head[open + 1..close].split_once(',')?;
    Some((head[..open].to_owned(), line.trim().parse().ok()?, column.trim().parse().ok()?))
  });
  match parsed {
    Some((file, line, column)) => (Some(file), (line, column)),
    None => (Some(head.to_owned()), (0, 0)),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_diagnostic_keeps_its_file_position_and_code() {
    let parsed = parse("app/routes/page.tsx(44,26): error TS7006: Parameter 'p' implicitly has an 'any' type.\n");
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].file.as_deref(), Some("app/routes/page.tsx"));
    assert_eq!((parsed[0].line, parsed[0].column), (44, 26));
    assert_eq!(parsed[0].code, "TS7006");
    assert_eq!(parsed[0].severity, Severity::Error);
    assert_eq!(parsed[0].to_string(), "app/routes/page.tsx(44,26): error TS7006: Parameter 'p' implicitly has an 'any' type.");
  }

  #[test]
  fn a_project_diagnostic_has_no_file_and_a_continuation_joins_the_one_above() {
    let parsed = parse("error TS18003: No inputs were found in config file 'tsconfig.json'.\nsrc/a.ts(1,1): error TS2322: Type 'string' is not assignable to type 'number'.\n  The expected type comes from property 'total'.\n");
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].file, None);
    assert_eq!(parsed[0].code, "TS18003");
    assert!(parsed[1].message.ends_with("The expected type comes from property 'total'."));
    assert_eq!(parsed[1].message.lines().count(), 2);
  }

  #[test]
  fn a_line_in_no_known_shape_is_reported_rather_than_dropped() {
    let parsed = parse("something the compiler said\n");
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].message, "something the compiler said");
    assert!(parsed[0].is_error());
  }

  #[test]
  fn a_warning_is_told_from_an_error() {
    let parsed = parse("src/a.ts(3,5): warning TS6133: 'x' is declared but its value is never read.\n");
    assert_eq!(parsed[0].severity, Severity::Warning);
    assert!(!parsed[0].is_error());
  }
}
