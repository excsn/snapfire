//! `fsr-server`, the production binary for an application with no crate of its own.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn dir(name: &str) -> PathBuf {
  let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
  let dir = snapfire_fsr_test_internal::scratch_root().join(format!("fsr-server-{name}-{}-{nanos}", std::process::id()));
  std::fs::create_dir_all(dir.join("app/generated")).unwrap();
  dir
}

fn application(root: &Path, config: &str) {
  let plan = r#"{"version":2,"routes":[{"pattern":"/","plan":{"id":0,"module":"shell#document","children":[{"slot":"content","node":{"id":1,"module":"routes/page.tsx#default"}}]}}],"sources":[],"actions":[],"handlers":[],"components":[]}"#;
  std::fs::write(root.join("app/generated/plan.sexp"), snapfire_fsr_plan::Manifest::from_json(plan).unwrap().to_sexpr()).unwrap();
  std::fs::write(root.join("app.toml"), format!("[app]\ndir = \"app\"\n[document]\ntitle = \"t\"\n[session]\nkey = \"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\"\n{config}")).unwrap();
}

fn free_address() -> String {
  let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
  listener.local_addr().unwrap().to_string()
}

fn server(root: &Path, listen: &str) -> Child {
  Command::new(env!("CARGO_BIN_EXE_fsr-server"))
    .arg(root.join("app"))
    .args(["--listen", listen])
    .env_remove("RELEASE_ENV")
    .env_remove("APP_ENV")
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .unwrap()
}

/// The status line of `GET path` once the server answers, within ten seconds.
fn status(listen: &str, path: &str) -> Option<String> {
  let deadline = Instant::now() + Duration::from_secs(10);
  while Instant::now() < deadline {
    if let Ok(mut stream) = TcpStream::connect(listen) {
      let _ = write!(stream, "GET {path} HTTP/1.1\r\nhost: {listen}\r\nconnection: close\r\n\r\n");
      let mut text = String::new();
      let _ = stream.read_to_string(&mut text);
      return text.lines().next().map(str::to_owned);
    }
    std::thread::sleep(Duration::from_millis(20));
  }
  None
}

#[test]
fn the_server_binary_serves_an_application_in_production_by_default() {
  let root = dir("serves");
  application(&root, "");
  let listen = free_address();
  let mut child = server(&root, &listen);
  let answered = status(&listen, "/health");
  let _ = child.kill();
  let out = child.wait_with_output().unwrap();
  assert_eq!(answered.as_deref(), Some("HTTP/1.1 200 OK"), "{}", String::from_utf8_lossy(&out.stderr));
  let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_server_binary_refuses_a_development_configuration() {
  let root = dir("refuses");
  application(&root, "[server]\ndev = true\n");
  let out = server(&root, &free_address()).wait_with_output().unwrap();
  assert!(!out.status.success());
  assert!(String::from_utf8_lossy(&out.stderr).contains("server.dev"), "{}", String::from_utf8_lossy(&out.stderr));
  let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_server_binary_carries_no_cli_and_no_test_engine() {
  let tree = Command::new(env!("CARGO"))
    .args(["tree", "--offline", "-p", "snapfire_fsr_sites", "--features", "server", "-e", "normal", "--prefix", "none"])
    .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
    .output()
    .unwrap();
  let tree = String::from_utf8_lossy(&tree.stdout);
  assert!(tree.contains("snapfire_fsr_host"), "{tree}");
  for absent in ["snapfire_fsr_cli", "snapfire_fsr_engine", "rquickjs"] {
    assert!(!tree.contains(absent), "{absent} is in the server's build");
  }
}
