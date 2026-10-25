use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use snapfire_fsr_cli::new::{create, NewOptions};
use snapfire_fsr_cli::{build, write, Options};

const PAGE: &str = "export default function Page({ notice }: { notice: { title: string } }) {\n  return <main>{notice.title}</main>;\n}\n";

const LOADER: &str = "import type { Ctx } from \"@snapfire/fsr/host\";\nexport async function load({ params }: Ctx<\"/notice/{id}\">) {\n  return { notice: { title: `Notice ${params.id}` } };\n}\n";

const IMAGE: &str = "export const size = { width: 300, height: 160 };\nexport const alt = \"A notice\";\n\nexport default function Image({ notice }: { notice: { title: string } }) {\n  return <div style={{ display: \"grid\", width: \"100%\", height: \"100%\", backgroundColor: \"#1e3a8a\", color: \"white\" }}>{notice.title}</div>;\n}\n";

fn project(tag: &str, logging: Option<&str>) -> PathBuf {
  let root = std::env::temp_dir().join(format!("fsr-cli-logging-{}-{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&root);
  create(&root, NewOptions::default().fetch(false)).unwrap();
  let app = root.join("app");
  for (path, text) in [("routes/notice/[id]/page.tsx", PAGE), ("routes/notice/[id]/page.loader.ts", LOADER), ("routes/notice/[id]/opengraph-image.tsx", IMAGE)] {
    let file = app.join(path);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
  }
  let config = root.join("config/app.toml");
  let text = std::fs::read_to_string(&config).unwrap().replacen("[document]\n", "[document]\norigin = \"https://board.example\"\n", 1);
  std::fs::write(&config, text).unwrap();
  if let Some(yaml) = logging {
    std::fs::write(root.join("fibre_logging.yaml"), yaml).unwrap();
  }
  let built = build(&app, &Options::default()).unwrap();
  write(&app, &built).unwrap();
  root
}

fn free_port() -> u16 {
  std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

/// `fsr serve` on the project, returned once it says it listens, with the address and a reader
/// collecting the rest of its stdout. A port `free_port` gave back can be bound by another test
/// before the server binds it, so a server that exits without listening is started again on
/// another port.
fn serving(root: &Path, envs: &[(&str, &str)]) -> (Child, String, std::thread::JoinHandle<String>) {
  serving_on(root, envs, &[], std::iter::repeat_with(free_port))
}

fn serving_on(root: &Path, envs: &[(&str, &str)], args: &[&str], ports: impl Iterator<Item = u16>) -> (Child, String, std::thread::JoinHandle<String>) {
  let mut failures = Vec::new();
  for port in ports.take(5) {
    let listen = format!("127.0.0.1:{port}");
    let mut child = Command::new(env!("CARGO_BIN_EXE_fsr")).arg("serve").arg(root.join("app")).args(["--listen", &listen]).args(args).envs(envs.iter().copied()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut seen = String::new();
    let mut line = String::new();
    while stdout.read_line(&mut line).unwrap_or(0) > 0 {
      seen.push_str(&line);
      if line.starts_with("fsr server on") {
        let rest = std::thread::spawn(move || {
          let _ = stdout.read_to_string(&mut seen);
          seen
        });
        return (child, listen, rest);
      }
      line.clear();
    }
    let output = child.wait_with_output().unwrap();
    failures.push(format!("{listen}: {seen}{}", String::from_utf8_lossy(&output.stderr)));
  }
  panic!("fsr serve did not listen: {failures:#?}");
}

/// Runs `fsr serve` on the project, asks for the card once and returns the status line, then what the process wrote to stdout and to stderr.
fn served(root: &Path) -> (String, String, String) {
  let (mut child, listen, stdout) = serving(root, &[]);
  let mut stream = TcpStream::connect(&listen).unwrap();
  stream.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
  write!(stream, "GET /notice/roof/opengraph-image.png HTTP/1.1\r\nHost: {listen}\r\nAccept: image/png\r\nConnection: close\r\n\r\n").unwrap();
  let mut response = Vec::new();
  let _ = stream.read_to_end(&mut response);
  let status = String::from_utf8_lossy(&response).lines().next().unwrap_or("").to_owned();
  std::thread::sleep(Duration::from_millis(300));
  let _ = child.kill();
  let output = child.wait_with_output().unwrap();
  (status, stdout.join().unwrap(), String::from_utf8_lossy(&output.stderr).into_owned())
}

#[test]
fn a_hosts_warning_reaches_stderr_without_a_logging_configuration() {
  let root = project("plain", None);
  let (status, _, stderr) = served(&root);
  assert!(status.contains("200"), "{status}\n{stderr}");
  assert!(stderr.contains("WARN") && stderr.contains("fsr::card") && stderr.contains("card image") && stderr.contains("display"), "{stderr}");
  let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_logging_configuration_beside_the_project_takes_the_events() {
  let broken = project("broken", Some("version: 1\nappenders: [not, a, map\n"));
  let (_, _, stderr) = served(&broken);
  assert!(stderr.contains("fibre_logging.yaml") && stderr.contains("warnings and errors go to stderr"), "a file that does not load is named: {stderr}");
  assert!(stderr.contains("card image"), "and the fallback still writes: {stderr}");
  let _ = std::fs::remove_dir_all(&broken);

  let console = "version: 1\n\nappenders:\n  console:\n    kind: console\n    encoder:\n      kind: pattern\n      pattern: \"%d [%p] %t - %m %X%n\"\n\nloggers:\n  root:\n    level: INFO\n    appenders:\n      - console\n";
  let configured = project("configured", Some(console));
  let (status, stdout, stderr) = served(&configured);
  assert!(status.contains("200"), "{status}");
  assert!(stdout.contains("card image"), "the console appender writes the warning: {stdout}");
  assert!(!stderr.contains("card image"), "and the fallback does not: {stderr}");
  let _ = std::fs::remove_dir_all(&configured);
}

#[test]
fn a_port_another_test_took_before_serve_bound_it_is_not_the_one_asked() {
  let root = project("raced", None);
  let taken = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
  let held = taken.local_addr().unwrap().port();
  let (mut child, listen, _) = serving_on(&root, &[], &[], [held].into_iter().chain(std::iter::repeat_with(free_port)));
  assert_ne!(listen, format!("127.0.0.1:{held}"));
  let response = get(&listen, "/notice/roof");
  let _ = child.kill();
  let _ = child.wait();
  assert!(response.starts_with("HTTP/1.1 200"), "{response}");
  drop(taken);
  let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn serve_prints_what_needs_attention_and_verbose_prints_the_boot_report() {
  let root = project("brief", None);
  let config = root.join("config/app.toml");
  let text = std::fs::read_to_string(&config).unwrap() + "\n[shop]\nlabel = \"x\"\n";
  std::fs::write(&config, text).unwrap();
  let boot = |args: &[&str]| {
    let (mut child, _, stdout) = serving_on(&root, &[], args, std::iter::repeat_with(free_port));
    let _ = child.kill();
    let _ = child.wait();
    stdout.join().unwrap()
  };
  let quiet = boot(&[]);
  assert!(quiet.contains("ignored   shop") && quiet.contains("fsr server on"), "{quiet}");
  assert!(!quiet.contains("config    ") && !quiet.contains("static    "), "the boot report's summary is printed under --verbose: {quiet}");
  let verbose = boot(&["--verbose"]);
  assert!(verbose.contains("config    ") && verbose.contains("ignored   shop"), "{verbose}");
  let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_port_already_taken_fails_serve_without_announcing_the_server() {
  let root = project("taken", None);
  let taken = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
  let listen = taken.local_addr().unwrap().to_string();
  let output = Command::new(env!("CARGO_BIN_EXE_fsr")).arg("serve").arg(root.join("app")).args(["--listen", &listen]).output().unwrap();
  let (stdout, stderr) = (String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
  assert!(!output.status.success());
  assert!(!stdout.contains("fsr server on"), "{stdout}");
  assert!(stderr.contains(&listen), "{stderr}");
  let _ = std::fs::remove_dir_all(&root);
}

fn get(listen: &str, path: &str) -> String {
  let mut stream = TcpStream::connect(listen).unwrap();
  stream.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
  write!(stream, "GET {path} HTTP/1.1\r\nHost: {listen}\r\nConnection: close\r\n\r\n").unwrap();
  let mut response = Vec::new();
  let _ = stream.read_to_end(&mut response);
  String::from_utf8_lossy(&response).into_owned()
}

#[test]
fn a_production_serve_reads_back_the_trace_of_an_exposed_path() {
  let root = project("expose", None);
  let config = root.join("config/app.toml");
  let text = std::fs::read_to_string(&config).unwrap() + "\n[trace]\nexpose = [\"/notice\"]\n";
  std::fs::write(&config, text).unwrap();
  let (mut child, listen, stdout) = serving(&root, &[("RELEASE_ENV", "production"), ("C5_SESSION__KEY", "a-test-session-key-of-at-least-32-bytes")]);
  let page = get(&listen, "/notice/roof");
  let token = page.lines().find_map(|line| line.to_ascii_lowercase().strip_prefix("x-sf-request:").map(|v| v.trim().to_owned()));
  let trace = token.as_ref().map(|token| get(&listen, &format!("/__fsr/trace/{token}")));
  let _ = child.kill();
  let _ = child.wait();
  let stdout = stdout.join().unwrap();
  let token = token.unwrap_or_else(|| panic!("an exposed page carries x-sf-request: {page}\n{stdout}"));
  let trace = trace.unwrap();
  assert!(trace.starts_with("HTTP/1.1 200"), "/__fsr/trace/{token}: {trace}");
  assert!(!stdout.contains("/__fsr/traces"), "the trace list is development only: {stdout}");
  let _ = std::fs::remove_dir_all(&root);
}
