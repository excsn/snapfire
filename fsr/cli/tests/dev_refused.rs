//! `fsr dev` when the first build is refused, before any server exists to carry the refusal.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use snapfire_fsr_cli::new::{NewOptions, create};

const REFUSED: &str = "export async function load({ query }) {\n  while (query.more) {\n    query.more = false;\n  }\n  return { held: \"<b>\" };\n}\n";
const FINE: &str = "export async function load() {\n  return { held: \"fine\" };\n}\n";

fn free_port() -> u16 {
  std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

fn project(tag: &str, port: u16) -> PathBuf {
  let root = snapfire_fsr_test_internal::scratch_root().join(format!("fsr-cli-dev-refused-{}-{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&root);
  create(&root, NewOptions::default().fetch(false)).unwrap();
  let config = root.join("config/app.toml");
  let text = std::fs::read_to_string(&config).unwrap();
  let mut lines: Vec<String> = text.lines().filter(|line| !line.trim_start().starts_with("listen")).map(str::to_owned).collect();
  let server = lines.iter().position(|line| line.trim() == "[server]");
  match server {
    Some(at) => lines.insert(at + 1, format!("listen = \"127.0.0.1:{port}\"")),
    None => lines.push(format!("\n[server]\nlisten = \"127.0.0.1:{port}\"")),
  }
  std::fs::write(&config, lines.join("\n") + "\n").unwrap();
  std::fs::write(root.join("app/routes/page.loader.ts"), REFUSED).unwrap();
  root
}

struct Dev(Child);

impl Drop for Dev {
  fn drop(&mut self) {
    let _ = self.0.kill();
    let _ = self.0.wait();
  }
}

fn fsr_dev(root: &Path) -> Dev {
  let snapfire = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/debug");
  let path = format!("{}:{}", snapfire.display(), std::env::var("PATH").unwrap_or_default());
  Dev(Command::new(env!("CARGO_BIN_EXE_fsr")).arg("dev").arg("--no-typecheck").arg(root.join("app")).env("PATH", path).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap())
}

fn get(port: u16) -> Option<String> {
  let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).ok()?;
  stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
  write!(stream, "GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n").ok()?;
  let mut answer = Vec::new();
  stream.read_to_end(&mut answer).ok()?;
  Some(String::from_utf8_lossy(&answer).into_owned())
}

fn answer_where(port: u16, within: Duration, wanted: impl Fn(&str) -> bool) -> Result<String, Option<String>> {
  let until = Instant::now() + within;
  let mut last = None;
  while Instant::now() < until {
    if let Some(answer) = get(port) {
      if wanted(&answer) {
        return Ok(answer);
      }
      last = Some(answer);
    }
    std::thread::sleep(Duration::from_millis(200));
  }
  Err(last)
}

#[test]
fn a_refused_first_build_answers_on_the_apps_port_with_what_refused_it() {
  let port = free_port();
  let root = project("refused", port);
  let _dev = fsr_dev(&root);
  let answer = answer_where(port, Duration::from_secs(60), |_| true).unwrap_or_else(|last| panic!("nothing answered: {last:?}"));
  assert!(answer.starts_with("HTTP/1.1 503"), "{answer}");
  assert!(answer.contains("the last build was refused"), "{answer}");
  assert!(answer.contains("routes/page.loader.ts"), "names the file: {answer}");
  assert!(answer.contains("`while`"), "says why: {answer}");
}

#[test]
fn the_refusal_page_escapes_what_it_shows() {
  let port = free_port();
  let root = project("escape", port);
  std::fs::write(root.join("app/routes/page.loader.ts"), "export async function load() {\n  while (1) {\n    <script>;\n  }\n}\n").unwrap();
  let _dev = fsr_dev(&root);
  let answer = answer_where(port, Duration::from_secs(60), |_| true).unwrap_or_else(|last| panic!("nothing answered: {last:?}"));
  assert!(answer.starts_with("HTTP/1.1 503"), "{answer}");
  assert!(!answer.contains("<script>"), "{answer}");
}

#[test]
fn fixing_the_refusal_gives_the_port_to_the_app() {
  let port = free_port();
  let root = project("fixed", port);
  let _dev = fsr_dev(&root);
  answer_where(port, Duration::from_secs(60), |answer| answer.starts_with("HTTP/1.1 503")).unwrap_or_else(|last| panic!("no refusal: {last:?}"));
  std::fs::write(root.join("app/routes/page.loader.ts"), FINE).unwrap();
  let answer = answer_where(port, Duration::from_secs(120), |answer| !answer.starts_with("HTTP/1.1 503")).unwrap_or_else(|last| panic!("still refused: {last:?}"));
  assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
}

#[test]
fn a_server_that_dies_on_start_answers_on_the_apps_port_with_that() {
  let port = free_port();
  let root = project("crash", port);
  std::fs::write(root.join("app/routes/page.loader.ts"), FINE).unwrap();
  std::fs::write(root.join("Cargo.toml"), "[package]\nname = \"crashes\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n").unwrap();
  std::fs::create_dir_all(root.join("src")).unwrap();
  std::fs::write(root.join("src/main.rs"), "fn main() {\n  std::process::exit(7);\n}\n").unwrap();
  let _dev = fsr_dev(&root);
  let answer = answer_where(port, Duration::from_secs(180), |_| true).unwrap_or_else(|last| panic!("nothing answered: {last:?}"));
  assert!(answer.starts_with("HTTP/1.1 503"), "{answer}");
  assert!(answer.contains("server exited"), "{answer}");
}
