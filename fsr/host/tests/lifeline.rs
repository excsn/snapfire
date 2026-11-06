//! A host built while its lifeline is set stops when the lifeline's write end closes.
#![cfg(unix)]

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use snapfire_fsr_host::{Host, LIFELINE};
use snapfire_fsr_test_internal::Scratch;

/// Set on the copy of this test that plays the server: the directory holding its `app.toml`.
const SERVER: &str = "FSR_HOST_LIFELINE_TEST_SERVER";

const TEST: &str = "a_host_stops_when_its_lifeline_closes";

#[test]
fn a_host_stops_when_its_lifeline_closes() {
  if let Some(dir) = std::env::var_os(SERVER) {
    let _host = Host::from(Path::new(&dir).join("app.toml")).unwrap().build().unwrap();
    println!("built");
    loop {
      std::thread::sleep(Duration::from_secs(60));
    }
  }
  let scratch = Scratch::new("host-lifeline");
  let dir = scratch.join("app");
  std::fs::create_dir_all(dir.join("generated")).unwrap();
  let plan = snapfire_fsr_plan::Manifest::from_text(r#"{"version": 2, "routes": []}"#).unwrap();
  std::fs::write(dir.join("generated/plan.sexp"), plan.to_sexpr()).unwrap();
  std::fs::write(dir.join("app.toml"), "[app]\ndir = \".\"\n\n[server]\nlisten = \"127.0.0.1:0\"\n").unwrap();
  let mut server = Command::new(std::env::current_exe().unwrap())
    .args(["--exact", TEST, "--nocapture"])
    .env(SERVER, &dir)
    .env(LIFELINE, "1")
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .spawn()
    .unwrap();
  let stdout = BufReader::new(server.stdout.take().unwrap());
  let built = stdout.lines().map_while(Result::ok).any(|line| line == "built");
  assert!(built, "the server built its host: {:?}", server.wait());
  std::thread::sleep(Duration::from_millis(500));
  assert!(server.try_wait().unwrap().is_none(), "the server runs while its lifeline is held");
  drop(server.stdin.take());
  let until = Instant::now() + Duration::from_secs(10);
  while Instant::now() < until && server.try_wait().unwrap().is_none() {
    std::thread::sleep(Duration::from_millis(50));
  }
  let stopped = server.try_wait().unwrap();
  if stopped.is_none() {
    let _ = server.kill();
  }
  assert!(stopped.is_some(), "the server still runs after its lifeline closed");
}
