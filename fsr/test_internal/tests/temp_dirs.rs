//! A test's directories go under `scratch_root`, which goes when its process does. A directory named under the system's directly stays there for ever, one per run.

use std::path::{Path, PathBuf};
use std::process::Command;

use snapfire_fsr_test_internal::scratch_root;

/// Reads the system's temporary directory outside any test: an application's `system.tempDir`, a build lock shared between runs and a scratch tree the build removes.
const ALLOWED: &[(&str, usize)] = &[("test_internal/src/lib.rs", 1), ("test_internal/tests/temp_dirs.rs", 3), ("ir/src/std/system.rs", 1), ("cli/src/sites.rs", 1), ("cli/src/types.rs", 1)];

fn rust_files(dir: &Path, found: &mut Vec<PathBuf>) {
  let Ok(entries) = std::fs::read_dir(dir) else { return };
  for entry in entries.flatten() {
    let path = entry.path();
    let name = entry.file_name().to_string_lossy().into_owned();
    if path.is_dir() {
      if !["target", "corpus", "node_modules"].contains(&name.as_str()) && !name.starts_with('.') {
        rust_files(&path, found);
      }
    } else if name.ends_with(".rs") {
      found.push(path);
    }
  }
}

#[test]
fn nothing_but_the_shared_helper_names_the_systems_temporary_directory() {
  let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
  let mut files = Vec::new();
  rust_files(workspace, &mut files);
  assert!(files.len() > 100, "the walk found the workspace: {}", files.len());
  let named: Vec<String> = files
    .iter()
    .filter_map(|file| {
      let times = std::fs::read_to_string(file).ok()?.matches("temp_dir()").count();
      let name = file.strip_prefix(workspace).unwrap().to_string_lossy().into_owned();
      let allowed = ALLOWED.iter().find(|(path, _)| *path == name).map_or(0, |(_, times)| *times);
      (times > allowed).then(|| format!("{name}: {times}"))
    })
    .collect();
  assert!(named.is_empty(), "{} files leave a directory behind for every run; use snapfire_fsr_test_internal::scratch_root():\n{}", named.len(), named.join("\n"));
}

const CHILD: &str = "FSR_TEMP_DIRS_CHILD";

/// Under `CHILD`, what a test does with the root: prints it and leaves a file in it.
#[test]
fn child_writes_under_the_root() {
  if std::env::var_os(CHILD).is_some() {
    let root = scratch_root();
    std::fs::write(root.join("left"), "x").unwrap();
    println!("root={}", root.display());
  }
}

fn run_child() -> PathBuf {
  let out = Command::new(std::env::current_exe().unwrap()).args(["--exact", "child_writes_under_the_root", "--nocapture"]).env(CHILD, "1").output().unwrap();
  assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
  let text = String::from_utf8_lossy(&out.stdout).into_owned();
  PathBuf::from(text.lines().find_map(|line| line.strip_prefix("root=")).unwrap_or_else(|| panic!("{text}")))
}

#[test]
fn a_process_takes_its_root_with_it_when_it_exits() {
  let root = run_child();
  assert!(!root.exists(), "{} outlived its process", root.display());
}

#[test]
fn a_killed_process_root_goes_when_the_next_one_starts_and_a_live_ones_stays() {
  let live = scratch_root();
  std::fs::write(live.join("held"), "x").unwrap();
  let mut dead = Command::new("true").spawn().unwrap();
  let pid = dead.id();
  dead.wait().unwrap();
  let killed = live.parent().unwrap().join(format!("fsr-test-root-{pid}"));
  std::fs::create_dir_all(&killed).unwrap();
  run_child();
  assert!(!killed.exists(), "{} was left by a dead process", killed.display());
  assert!(live.join("held").exists(), "a running process keeps its root");
}
