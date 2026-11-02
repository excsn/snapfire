//! Nothing `fsr dev` starts outlives it, however it stops.
#![cfg(unix)]

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use snapfire_fsr_cli::new::{NewOptions, create};
use snapfire_fsr_test_internal::Scratch;

const FINE: &str = "export async function load() {\n  return { held: \"fine\" };\n}\n";

fn free_port() -> u16 {
  std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

fn project(scratch: &Scratch, port: u16) -> PathBuf {
  let root = scratch.join("project");
  create(&root, NewOptions::default().fetch(false)).unwrap();
  let config = root.join("config/app.toml");
  let text = std::fs::read_to_string(&config).unwrap();
  let mut lines: Vec<String> = text.lines().filter(|line| !line.trim_start().starts_with("listen")).map(str::to_owned).collect();
  match lines.iter().position(|line| line.trim() == "[server]") {
    Some(at) => lines.insert(at + 1, format!("listen = \"127.0.0.1:{port}\"")),
    None => lines.push(format!("\n[server]\nlisten = \"127.0.0.1:{port}\"")),
  }
  std::fs::write(&config, lines.join("\n") + "\n").unwrap();
  std::fs::write(root.join("app/routes/page.loader.ts"), FINE).unwrap();
  root
}

/// `fsr dev` and, on a test's failure, what it started.
struct Dev {
  child: Child,
  started: Vec<u32>,
}

impl Drop for Dev {
  fn drop(&mut self) {
    let _ = self.child.kill();
    let _ = self.child.wait();
    for pid in &self.started {
      let _ = Command::new("kill").args(["-KILL", &pid.to_string()]).stderr(Stdio::null()).status();
    }
  }
}

fn fsr_dev(root: &Path, envs: &[(&str, &str)]) -> Dev {
  let snapfire = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/debug");
  let path = format!("{}:{}", snapfire.display(), std::env::var("PATH").unwrap_or_default());
  let child = Command::new(env!("CARGO_BIN_EXE_fsr")).arg("dev").arg("--no-typecheck").arg(root.join("app")).env("PATH", path).envs(envs.iter().copied()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
  Dev { child, started: Vec::new() }
}

fn answer(port: u16) -> Option<String> {
  let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).ok()?;
  stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
  write!(stream, "GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n").ok()?;
  let mut bytes = Vec::new();
  stream.read_to_end(&mut bytes).ok()?;
  Some(String::from_utf8_lossy(&bytes).into_owned())
}

fn await_success(port: u16, within: Duration) {
  let until = Instant::now() + within;
  let mut last = None;
  while Instant::now() < until {
    last = answer(port);
    if last.as_deref().is_some_and(|text| text.starts_with("HTTP/1.1 200")) {
      return;
    }
    std::thread::sleep(Duration::from_millis(200));
  }
  panic!("the app never answered: {last:?}");
}

/// Every process under `root`, however deep, from one reading of the process table.
fn descendants(root: u32) -> Vec<u32> {
  let table = Command::new("ps").args(["-axo", "pid=,ppid="]).output().unwrap();
  let rows: Vec<(u32, u32)> = String::from_utf8_lossy(&table.stdout).lines().filter_map(|line| {
    let mut fields = line.split_whitespace();
    Some((fields.next()?.parse().ok()?, fields.next()?.parse().ok()?))
  }).collect();
  let mut found = vec![root];
  let mut at = 0;
  while at < found.len() {
    let parent = found[at];
    found.extend(rows.iter().filter(|(_, ppid)| *ppid == parent).map(|(pid, _)| *pid));
    at += 1;
  }
  found.remove(0);
  found
}

fn alive(pid: u32) -> bool {
  Command::new("kill").args(["-0", &pid.to_string()]).stderr(Stdio::null()).status().unwrap().success()
}

fn assert_stops_everything(dev: &mut Dev, signal: &str, port: u16) {
  dev.started = descendants(dev.child.id());
  assert!(!dev.started.is_empty(), "fsr dev runs the server as a child");
  Command::new("kill").args([&format!("-{signal}"), &dev.child.id().to_string()]).status().unwrap();
  dev.child.wait().unwrap();
  let until = Instant::now() + Duration::from_secs(20);
  while Instant::now() < until && (answer(port).is_some() || dev.started.iter().any(|pid| alive(*pid))) {
    std::thread::sleep(Duration::from_millis(100));
  }
  let left: Vec<u32> = dev.started.iter().copied().filter(|pid| alive(*pid)).collect();
  assert!(left.is_empty(), "still running after SIG{signal}: {left:?}");
  assert_eq!(answer(port), None, "the port still answers after SIG{signal}");
}

fn stopped_by(signal: &str) {
  let scratch = Scratch::new("cli-dev-lifetime");
  let port = free_port();
  let root = project(&scratch, port);
  let mut dev = fsr_dev(&root, &[]);
  await_success(port, Duration::from_secs(120));
  assert_stops_everything(&mut dev, signal, port);
}

#[test]
fn an_interrupt_stops_everything_fsr_dev_started() {
  stopped_by("INT");
}

#[test]
fn a_terminate_stops_everything_fsr_dev_started() {
  stopped_by("TERM");
}

#[test]
fn a_closed_terminal_stops_everything_fsr_dev_started() {
  stopped_by("HUP");
}

#[test]
fn a_hard_kill_stops_everything_fsr_dev_started() {
  stopped_by("KILL");
}

/// A crate of the application's own, whose `main` is `body` under a host built from the project. `fsr dev` builds and runs it. Nothing in `body` listens for a signal or calls the host's `serve`, as an application with a server of its own does not.
fn own_server(root: &Path, body: &str) {
  let fsr = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
  let workspace = std::fs::read_to_string(fsr.join("Cargo.toml")).unwrap();
  let patches = workspace.split("[patch.crates-io]").nth(1).and_then(|rest| rest.split("\n[").next()).unwrap();
  std::fs::write(root.join("Cargo.toml"), format!("[package]\nname = \"own_server\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nsnapfire_fsr_host = {{ path = \"{}\" }}\n\n[workspace]\n\n[patch.crates-io]{patches}\n", fsr.join("host").display())).unwrap();
  std::fs::copy(fsr.join("Cargo.lock"), root.join("Cargo.lock")).unwrap();
  std::fs::create_dir_all(root.join("src")).unwrap();
  std::fs::write(root.join("src/main.rs"), body).unwrap();
}

const OWN_SERVER: &str = r#"use std::io::{Read, Write};

fn main() {
  let root = env!("CARGO_MANIFEST_DIR");
  let host = snapfire_fsr_host::Host::from(root).and_then(|builder| builder.build()).unwrap();
  let listener = std::net::TcpListener::bind(host.listen()).unwrap();
  for stream in listener.incoming() {
    let mut stream = stream.unwrap();
    let mut request = [0u8; 1024];
    let _ = stream.read(&mut request);
    let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
  }
}
"#;

#[test]
fn a_hard_kill_stops_a_server_the_application_runs_itself() {
  let scratch = Scratch::new("cli-dev-lifetime");
  let port = free_port();
  let root = project(&scratch, port);
  own_server(&root, OWN_SERVER);
  let target = Path::new(env!("CARGO_MANIFEST_DIR")).join("../target");
  let mut dev = fsr_dev(&root, &[("CARGO_TARGET_DIR", target.to_str().unwrap())]);
  await_success(port, Duration::from_secs(400));
  assert_stops_everything(&mut dev, "KILL", port);
}
