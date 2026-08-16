use std::fs;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

mod common;
use common::{Fixture, get_snapfirec_cmd};

/// Kills the watcher however the test ends, including on a panic.
struct Watcher(Child);

impl Drop for Watcher {
  fn drop(&mut self) {
    let _ = self.0.kill();
    let _ = self.0.wait();
  }
}

fn start(root: &Path) -> Watcher {
  let mut cmd: Command = get_snapfirec_cmd();
  let child = cmd
    .arg("--root")
    .arg(root)
    .arg("--watch")
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .spawn()
    .expect("failed to start the watcher");

  Watcher(child)
}

/// Polls until the predicate holds, so a slow machine costs time rather than a false failure.
fn until(what: &str, mut ready: impl FnMut() -> bool) {
  let deadline = Instant::now() + Duration::from_secs(20);

  while Instant::now() < deadline {
    if ready() {
      return;
    }
    sleep(Duration::from_millis(50));
  }

  panic!("timed out waiting for {what}");
}

#[test]
fn test_watch_recompiles_a_changed_file() {
  let fixture = Fixture::new("computed-root");
  let _watcher = start(fixture.root());

  let output = fixture.root().join("dist/button.js");
  until("the first build", || output.is_file());

  fs::write(fixture.root().join("src/ui/button.ts"), "export const button = 42;\n").unwrap();

  until("the recompile", || {
    fs::read_to_string(&output).is_ok_and(|c| c.contains("42"))
  });
}

#[test]
fn test_watch_picks_up_a_new_file() {
  let fixture = Fixture::new("computed-root");
  let _watcher = start(fixture.root());

  until("the first build", || fixture.root().join("dist/button.js").is_file());

  fs::write(fixture.root().join("src/ui/extra.ts"), "export const extra = 1;\n").unwrap();

  until("the new output", || fixture.root().join("dist/extra.js").is_file());
}

#[test]
fn test_watch_removes_output_for_a_deleted_source() {
  let fixture = Fixture::new("computed-root");
  let _watcher = start(fixture.root());

  let orphan = fixture.root().join("dist/panel.js");
  until("the first build", || orphan.is_file());

  fs::remove_file(fixture.root().join("src/ui/panel.ts")).unwrap();

  until("the output to be pruned", || !orphan.exists());
}

#[test]
fn test_watch_survives_a_compile_error() {
  let fixture = Fixture::new("computed-root");
  let _watcher = start(fixture.root());

  let output = fixture.root().join("dist/panel.js");
  until("the first build", || output.is_file());

  fs::write(fixture.root().join("src/ui/panel.ts"), "const broken = ;\n").unwrap();
  sleep(Duration::from_millis(500));

  fs::write(fixture.root().join("src/ui/panel.ts"), "export const panel = 7;\n").unwrap();

  until("recovery after the error", || {
    fs::read_to_string(&output).is_ok_and(|c| c.contains('7'))
  });
}
