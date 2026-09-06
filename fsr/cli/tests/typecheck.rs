use std::path::{Path, PathBuf};

use snapfire_fsr_cli::typecheck::{self, Typecheck};

static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn root(tag: &str) -> PathBuf {
  let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let dir = std::env::temp_dir().join(format!("fsr-cli-typecheck-{}-{n}-{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  std::fs::create_dir_all(&dir).unwrap();
  dir
}

/// A stand-in checker: prints one report and exits.
#[cfg(unix)]
fn fake_checker(dir: &Path, report: &str) -> PathBuf {
  use std::os::unix::fs::PermissionsExt;
  let path = dir.join("snapfiretc");
  std::fs::write(&path, format!("#!/bin/sh\ncat <<'END'\n{report}\nEND\n")).unwrap();
  std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
  path
}

#[cfg(unix)]
fn app_with_tsconfig(dir: &Path) -> PathBuf {
  let app = dir.join("app");
  std::fs::create_dir_all(&app).unwrap();
  std::fs::write(app.join("tsconfig.json"), "{}\n").unwrap();
  app
}

#[cfg(unix)]
#[test]
fn a_report_is_read_back_and_the_version_recorded_where_the_configuration_names_none() {
  let dir = root("records");
  let app = app_with_tsconfig(&dir);
  let config = dir.join("app.toml");
  std::fs::write(&config, "[server]\nlisten = \"127.0.0.1:8080\"\n").unwrap();
  let checker = fake_checker(&dir, r#"{"tsc":"/c/tsc","version":"7.0.2","source":"cache","sha512":null,"pinned":true,"diagnostics":[{"file":"routes/page.tsx","line":1,"column":15,"code":"TS2305","severity":"error","message":"no such member"}]}"#);
  let options = Typecheck { enabled: true, checker: Some(checker), record: Some(config.clone()), ..Typecheck::default() };

  let checked = typecheck::run(&app, &options).unwrap().expect("the checker ran");
  assert_eq!(checked.errors(), 1);
  assert_eq!(checked.row(), "tsc 7.0.2 from cache, 1 error");
  assert_eq!(checked.diagnostics[0].to_string(), "routes/page.tsx(1,15): error TS2305: no such member");
  assert_eq!(checked.recorded.as_deref(), Some(config.as_path()));
  assert!(std::fs::read_to_string(&config).unwrap().contains("[typecheck]\nversion = \"7.0.2\""));

  let again = typecheck::run(&app, &options).unwrap().expect("the checker ran");
  assert_eq!(again.recorded, None, "a version already recorded is not written twice");
}

#[cfg(unix)]
#[test]
fn nothing_runs_when_typechecking_is_off_or_no_checker_is_installed() {
  let dir = root("off");
  let app = app_with_tsconfig(&dir);
  let off = Typecheck { enabled: false, ..Typecheck::default() };
  assert!(typecheck::run(&app, &off).unwrap().is_none());

  let missing = Typecheck { enabled: true, checker: Some(dir.join("nothing-here")), ..Typecheck::default() };
  assert!(typecheck::run(&app, &missing).unwrap().is_none());

  let bare = dir.join("bare");
  std::fs::create_dir_all(&bare).unwrap();
  let checker = fake_checker(&dir, "{}");
  let no_tsconfig = Typecheck { enabled: true, checker: Some(checker), ..Typecheck::default() };
  assert!(typecheck::run(&bare, &no_tsconfig).unwrap().is_none());
}
