//! `[build] before` and `after`: Cargo projects `fsr build` builds and runs in order before and after the application builds.

use std::path::{Path, PathBuf};
use std::process::Command;

use snapfire_fsr_cli::new::{create, NewOptions};

/// Appends `<name> built=<plan exists> app=<FSR_APP>` to `ran.txt` at `FSR_ROOT`, then exits 3 when `FAIL` names it.
fn main_of(name: &str) -> String {
  format!(
    r#"use std::io::Write;

fn main() {{
  let root = std::env::var("FSR_ROOT").unwrap();
  let app = std::env::var("FSR_APP").unwrap();
  let built = std::path::Path::new(&app).join("generated/plan.sexp").is_file();
  let mut ran = std::fs::OpenOptions::new().create(true).append(true).open(std::path::Path::new(&root).join("ran.txt")).unwrap();
  writeln!(ran, "{name} built={{built}} app={{app}}").unwrap();
  if std::env::var("FAIL").as_deref() == Ok("{name}") {{
    std::process::exit(3);
  }}
}}
"#
  )
}

fn project(tag: &str, build: &str, programs: &[&str]) -> PathBuf {
  let root = std::env::temp_dir().join(format!("fsr-cli-programs-{}-{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&root);
  create(&root, NewOptions::default().fetch(false)).unwrap();
  for name in programs {
    std::fs::create_dir_all(root.join(name).join("src")).unwrap();
    std::fs::write(root.join(name).join("Cargo.toml"), format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n")).unwrap();
    std::fs::write(root.join(name).join("src/main.rs"), main_of(name)).unwrap();
  }
  let config = root.join("config/app.toml");
  let text = std::fs::read_to_string(&config).unwrap();
  std::fs::write(&config, format!("{text}\n[build]\n{build}\n")).unwrap();
  root
}

fn fsr_build(root: &Path, fail: Option<&str>) -> std::process::Output {
  let snapfire = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/debug");
  let path = format!("{}:{}", snapfire.display(), std::env::var("PATH").unwrap_or_default());
  let mut command = Command::new(env!("CARGO_BIN_EXE_fsr"));
  command.arg("build").arg(root.join("app")).env("PATH", path).env("CARGO_TARGET_DIR", root.join("target"));
  if let Some(name) = fail {
    command.env("FAIL", name);
  }
  command.output().unwrap()
}

fn ran(root: &Path) -> String {
  std::fs::read_to_string(root.join("ran.txt")).unwrap_or_default()
}

#[test]
fn fsr_build_runs_before_ahead_of_the_app_and_after_once_it_is_built() {
  let root = project("order", "before = [\"one\", \"two\"]\nafter = [\"three\"]", &["one", "two", "three"]);
  let out = fsr_build(&root, None);
  let stdout = String::from_utf8_lossy(&out.stdout);
  assert!(out.status.success(), "{stdout}{}", String::from_utf8_lossy(&out.stderr));
  let app = root.join("app").canonicalize().unwrap().display().to_string();
  assert_eq!(ran(&root), format!("one built=false app={app}\ntwo built=false app={app}\nthree built=true app={app}\n"));
  let at = |row: &str| stdout.find(row).unwrap_or_else(|| panic!("no `{row}` row: {stdout}"));
  assert!(at("before    one") < at("before    two") && at("before    two") < at("wrote ") && at("wrote ") < at("after     three"), "{stdout}");
  let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_before_program_that_fails_stops_the_build_ahead_of_the_rest() {
  let root = project("before-fails", "before = [\"one\", \"two\"]\nafter = [\"three\"]", &["one", "two", "three"]);
  let out = fsr_build(&root, Some("one"));
  assert!(!out.status.success());
  let stderr = String::from_utf8_lossy(&out.stderr);
  assert!(stderr.contains("[build] before one: exited with"), "{stderr}");
  assert_eq!(ran(&root), format!("one built=false app={}\n", root.join("app").canonicalize().unwrap().display()));
  assert!(!root.join("app/generated/plan.sexp").is_file(), "the app is not built");
  let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn an_after_program_that_fails_fails_the_build() {
  let root = project("after-fails", "after = [\"one\", \"two\"]", &["one", "two"]);
  let out = fsr_build(&root, Some("one"));
  assert!(!out.status.success());
  let stderr = String::from_utf8_lossy(&out.stderr);
  assert!(stderr.contains("[build] after one: exited with"), "{stderr}");
  assert_eq!(ran(&root), format!("one built=true app={}\n", root.join("app").canonicalize().unwrap().display()));
  let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_program_declares_what_it_reads_and_the_declaration_is_not_output() {
  let root = project("declares", "before = [\"reads\"]", &[]);
  std::fs::create_dir_all(root.join("reads/src")).unwrap();
  std::fs::write(root.join("reads/Cargo.toml"), "[package]\nname = \"reads\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n").unwrap();
  std::fs::write(root.join("reads/src/main.rs"), "fn main() {\n  println!(\"fsr:watch=guide\");\n  println!(\"read the guide\");\n}\n").unwrap();
  let out = fsr_build(&root, None);
  let stdout = String::from_utf8_lossy(&out.stdout);
  assert!(out.status.success(), "{stdout}{}", String::from_utf8_lossy(&out.stderr));
  assert!(stdout.contains("read the guide"), "{stdout}");
  assert!(!stdout.contains("fsr:watch"), "{stdout}");
  let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_relative_app_path_reaches_the_program_absolute() {
  let root = project("relative", "after = [\"one\"]", &["one"]);
  let snapfire = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/debug");
  let path = format!("{}:{}", snapfire.display(), std::env::var("PATH").unwrap_or_default());
  let name = root.file_name().unwrap().to_string_lossy().into_owned();
  let out = Command::new(env!("CARGO_BIN_EXE_fsr"))
    .arg("build")
    .arg(format!("{name}/app"))
    .current_dir(root.parent().unwrap())
    .env("PATH", path)
    .env("CARGO_TARGET_DIR", root.join("target"))
    .output()
    .unwrap();
  assert!(out.status.success(), "{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
  assert_eq!(ran(&root), format!("one built=true app={}\n", root.join("app").canonicalize().unwrap().display()));
  let _ = std::fs::remove_dir_all(&root);
}
