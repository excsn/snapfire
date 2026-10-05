use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

mod common;
use common::{Fixture, get_snapfirec_cmd};

const REBUILT: &str = "snapfirec: rebuilt";
const FAILED: &str = "snapfirec: failed";

struct Driven {
  child: Child,
  stdin: Option<ChildStdin>,
  stdout: BufReader<ChildStdout>,
}

impl Drop for Driven {
  fn drop(&mut self) {
    drop(self.stdin.take());
    let _ = self.child.kill();
    let _ = self.child.wait();
  }
}

impl Driven {
  fn start(root: &Path) -> Self {
    let mut cmd: Command = get_snapfirec_cmd();
    let mut child = cmd
      .arg("--root")
      .arg(root)
      .arg("--driven")
      .stdin(Stdio::piped())
      .stdout(Stdio::piped())
      .stderr(Stdio::null())
      .spawn()
      .expect("failed to start the compiler");
    let stdin = child.stdin.take();
    let stdout = BufReader::new(child.stdout.take().unwrap());
    let mut driven = Driven { child, stdin, stdout };
    assert_eq!(driven.settled(), REBUILT, "the build it starts with");
    driven
  }

  fn rebuild(&mut self, paths: &[&str]) -> String {
    let stdin = self.stdin.as_mut().unwrap();
    for path in paths {
      writeln!(stdin, "{path}").unwrap();
    }
    writeln!(stdin).unwrap();
    stdin.flush().unwrap();
    self.settled()
  }

  /// Every line up to and including the one that ends the batch, which is the one returned.
  fn settled(&mut self) -> String {
    let mut line = String::new();
    loop {
      line.clear();
      let read = self.stdout.read_line(&mut line).unwrap();
      assert!(read > 0, "the compiler closed its output without ending the batch");
      let text = line.trim_end().to_owned();
      if text == REBUILT || text == FAILED {
        return text;
      }
    }
  }
}

fn button(fixture: &Fixture) -> String {
  fs::read_to_string(fixture.root().join("dist/button.js")).unwrap()
}

fn panel(fixture: &Fixture) -> String {
  fs::read_to_string(fixture.root().join("dist/panel.js")).unwrap()
}

#[test]
fn test_driven_compiles_only_the_paths_it_is_given() {
  let fixture = Fixture::new("computed-root");
  let mut driven = Driven::start(fixture.root());

  fs::write(fixture.root().join("src/ui/button.ts"), "export const button = 42;\n").unwrap();
  fs::write(fixture.root().join("src/ui/panel.ts"), "export const panel = 7;\n").unwrap();

  assert_eq!(driven.rebuild(&["src/ui/button.ts"]), REBUILT);
  assert!(button(&fixture).contains("42"));
  assert!(!panel(&fixture).contains('7'), "panel was not named, so it must not have been compiled");
}

#[test]
fn test_driven_compiles_everything_for_an_empty_batch() {
  let fixture = Fixture::new("computed-root");
  let mut driven = Driven::start(fixture.root());

  fs::write(fixture.root().join("src/ui/button.ts"), "export const button = 42;\n").unwrap();
  fs::write(fixture.root().join("src/ui/panel.ts"), "export const panel = 7;\n").unwrap();

  assert_eq!(driven.rebuild(&[]), REBUILT);
  assert!(button(&fixture).contains("42"));
  assert!(panel(&fixture).contains('7'));
}

#[test]
fn test_driven_compiles_everything_for_a_path_outside_the_selection() {
  let fixture = Fixture::new("computed-root");
  let mut driven = Driven::start(fixture.root());

  fs::write(fixture.root().join("src/ui/extra.ts"), "export const extra = 1;\n").unwrap();

  assert_eq!(driven.rebuild(&["src/ui/extra.ts"]), REBUILT);
  assert!(fixture.root().join("dist/extra.js").is_file());
}

#[test]
fn test_driven_prunes_the_output_of_a_deleted_source() {
  let fixture = Fixture::new("computed-root");
  let mut driven = Driven::start(fixture.root());

  let orphan = fixture.root().join("dist/panel.js");
  assert!(orphan.is_file());
  fs::remove_file(fixture.root().join("src/ui/panel.ts")).unwrap();

  assert_eq!(driven.rebuild(&["src/ui/panel.ts"]), REBUILT);
  assert!(!orphan.exists());
}

#[test]
fn test_driven_reports_a_failed_batch_and_keeps_going() {
  let fixture = Fixture::new("computed-root");
  let mut driven = Driven::start(fixture.root());

  fs::write(fixture.root().join("src/ui/panel.ts"), "const broken = ;\n").unwrap();
  assert_eq!(driven.rebuild(&["src/ui/panel.ts"]), FAILED);

  fs::write(fixture.root().join("src/ui/panel.ts"), "export const panel = 7;\n").unwrap();
  assert_eq!(driven.rebuild(&["src/ui/panel.ts"]), REBUILT);
  assert!(panel(&fixture).contains('7'));
}

#[test]
fn test_driven_exits_when_its_input_closes() {
  let fixture = Fixture::new("computed-root");
  let mut driven = Driven::start(fixture.root());

  drop(driven.stdin.take());
  let status = driven.child.wait().unwrap();
  assert!(status.success());
}

/// A driver that answers the compiler's plugin requests itself, with a module it made up.
fn answering(root: &Path, seen: &mut Vec<String>) -> String {
  let mut cmd: Command = get_snapfirec_cmd();
  let mut child = cmd
    .arg("--root")
    .arg(root)
    .arg("--driven")
    .env("PATH", "/usr/bin:/bin")
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::null())
    .spawn()
    .expect("failed to start the compiler");
  let mut stdin = child.stdin.take().unwrap();
  let mut stdout = BufReader::new(child.stdout.take().unwrap());
  let mut line = String::new();
  let outcome = loop {
    line.clear();
    assert!(stdout.read_line(&mut line).unwrap() > 0, "the compiler closed its output without ending the batch");
    let text = line.trim_end().to_owned();
    if text == REBUILT || text == FAILED {
      break text;
    }
    if text != snapfire_compiler_wire::driven::PLUGIN {
      continue;
    }
    line.clear();
    stdout.read_line(&mut line).unwrap();
    let answer = match serde_json::from_str::<snapfire_compiler_wire::driven::PluginRequest>(&line).expect("a request on one line") {
      snapfire_compiler_wire::driven::PluginRequest::Hello { ext } => {
        seen.push(format!("hello {ext}"));
        serde_json::json!({ "answer": "hello", "hello": { "protocol": snapfire_compiler_wire::PROTOCOL, "name": "vue", "version": "9.9.9", "compiler": "the driver", "extensions": [".vue"] } })
      }
      snapfire_compiler_wire::driven::PluginRequest::Compile { ext, units } => {
        seen.push(format!("compile {ext} {}", units.iter().map(|u| u.filename.clone()).collect::<Vec<_>>().join(" ")));
        let outcomes: Vec<serde_json::Value> = units.iter().map(|_| serde_json::json!({ "status": "ok", "js": "export default { from: \"the driver\" };\n" })).collect();
        serde_json::json!({ "answer": "compiled", "outcomes": outcomes })
      }
    };
    writeln!(stdin, "{answer}").unwrap();
    stdin.flush().unwrap();
  };
  drop(stdin);
  let _ = child.wait();
  outcome
}

#[test]
fn a_driven_compiler_asks_its_driver_for_the_plugin_rather_than_starting_one() {
  let fixture = Fixture::new("vue-plugin");
  let mut seen = Vec::new();
  assert_eq!(answering(fixture.root(), &mut seen), REBUILT);
  assert_eq!(seen, ["hello vue", "compile vue src/Card.vue"], "one hello, then one batch per extension");
  let module = fs::read_to_string(fixture.root().join("dist/src/Card.js")).expect("the module");
  assert!(module.contains("the driver"), "the module is the driver's answer: {module}");
}
