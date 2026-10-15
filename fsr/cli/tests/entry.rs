//! `[build] entry`: a Cargo project `fsr build` builds and runs once the application is built.

use std::path::{Path, PathBuf};
use std::process::Command;

use snapfire_fsr_cli::new::{create, NewOptions};

const MAIN: &str = r#"fn main() {
  let root = std::env::var("FSR_ROOT").unwrap();
  let app = std::env::var("FSR_APP").unwrap();
  let built = std::path::Path::new(&app).join("generated/plan.sexp").is_file();
  std::fs::write(std::path::Path::new(&root).join("site.txt"), format!("app={app} built={built}")).unwrap();
  if std::env::var("FAIL").is_ok() {
    std::process::exit(3);
  }
}
"#;

fn project(tag: &str) -> PathBuf {
  let root = std::env::temp_dir().join(format!("fsr-cli-entry-{}-{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&root);
  create(&root, NewOptions { fetch: false, ..NewOptions::default() }).unwrap();
  std::fs::create_dir_all(root.join("generate/src")).unwrap();
  std::fs::write(root.join("generate/Cargo.toml"), "[package]\nname = \"generate\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n").unwrap();
  std::fs::write(root.join("generate/src/main.rs"), MAIN).unwrap();
  let config = root.join("config/app.toml");
  let text = std::fs::read_to_string(&config).unwrap();
  std::fs::write(&config, format!("{text}\n[build]\nentry = \"generate\"\n")).unwrap();
  root
}

fn fsr_build(root: &Path, fail: bool) -> std::process::Output {
  let snapfire = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/debug");
  let path = format!("{}:{}", snapfire.display(), std::env::var("PATH").unwrap_or_default());
  let mut command = Command::new(env!("CARGO_BIN_EXE_fsr"));
  command.arg("build").arg(root.join("app")).env("PATH", path).env("CARGO_TARGET_DIR", root.join("target"));
  if fail {
    command.env("FAIL", "1");
  }
  command.output().unwrap()
}

#[test]
fn fsr_build_runs_the_entry_once_the_app_is_built() {
  let root = project("runs");
  let out = fsr_build(&root, false);
  assert!(out.status.success(), "{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
  let wrote = std::fs::read_to_string(root.join("site.txt")).unwrap();
  assert_eq!(wrote, format!("app={} built=true", root.join("app").display()));
  assert!(String::from_utf8_lossy(&out.stdout).contains("entry     generate"));
  let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn an_entry_that_fails_fails_the_build() {
  let root = project("fails");
  let out = fsr_build(&root, true);
  assert!(!out.status.success());
  let stderr = String::from_utf8_lossy(&out.stderr);
  assert!(stderr.contains("[build] entry generate: exited with"), "{stderr}");
  let _ = std::fs::remove_dir_all(&root);
}
