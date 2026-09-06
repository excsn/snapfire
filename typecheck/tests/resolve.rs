use std::path::{Path, PathBuf};

use snapfire_typecheck::{check, resolve, Options, Severity, Source};

/// A stand-in compiler: `--version` answers `version`, anything else prints `output` and exits `code`.
#[cfg(unix)]
fn fake_tsc(dir: &Path, version: &str, output: &str, code: i32) -> PathBuf {
  use std::os::unix::fs::PermissionsExt;
  let path = dir.join("tsc");
  let script = format!("#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'Version {version}'; exit 0; fi\ncat <<'END'\n{output}\nEND\nexit {code}\n");
  std::fs::write(&path, script).unwrap();
  std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
  path
}

#[cfg(unix)]
#[test]
fn a_given_compiler_is_taken_only_when_it_reports_the_requested_version() {
  let dir = tempfile::tempdir().unwrap();
  let tsc = fake_tsc(dir.path(), "7.0.2", "", 0);
  let options = Options { version: "7.0.2".to_owned(), tsc: Some(tsc.clone()), offline: true, ..Options::default() };
  let resolved = resolve(&options).unwrap();
  assert_eq!(resolved.source, Source::Given);
  assert_eq!(resolved.version, "7.0.2");

  let wanted_elsewhere = Options { version: "7.1.0".to_owned(), ..options };
  let error = resolve(&wanted_elsewhere).unwrap_err().to_string();
  assert!(error.contains("7.0.2") && error.contains("7.1.0"), "{error}");
}

#[cfg(unix)]
#[test]
fn nothing_is_fetched_when_fetching_is_off() {
  let cache = tempfile::tempdir().unwrap();
  let options = Options { version: "6.6.6".to_owned(), cache: Some(cache.path().to_path_buf()), offline: true, ..Options::default() };
  let error = resolve(&options).unwrap_err().to_string();
  assert!(error.contains("6.6.6"), "{error}");
}

#[cfg(unix)]
#[test]
fn diagnostics_come_back_with_their_positions_and_codes() {
  let dir = tempfile::tempdir().unwrap();
  let tsc = fake_tsc(dir.path(), "7.0.2", "src/a.ts(3,5): error TS2322: Type 'string' is not assignable to type 'number'.", 1);
  let diagnostics = check(&tsc, dir.path(), Path::new("tsconfig.json")).unwrap();
  assert_eq!(diagnostics.len(), 1);
  assert_eq!(diagnostics[0].file.as_deref(), Some("src/a.ts"));
  assert_eq!(diagnostics[0].code, "TS2322");
  assert_eq!(diagnostics[0].severity, Severity::Error);
}

#[cfg(unix)]
#[test]
fn a_compiler_that_fails_without_a_diagnostic_is_an_error_rather_than_a_clean_run() {
  let dir = tempfile::tempdir().unwrap();
  let tsc = fake_tsc(dir.path(), "7.0.2", "", 2);
  let error = check(&tsc, dir.path(), Path::new("tsconfig.json")).unwrap_err().to_string();
  assert!(error.contains("exited with"), "{error}");
  let clean = fake_tsc(dir.path(), "7.0.2", "", 0);
  assert!(check(&clean, dir.path(), Path::new("tsconfig.json")).unwrap().is_empty());
}
