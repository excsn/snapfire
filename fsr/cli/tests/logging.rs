//! DEFECTS 1.96: `fsr serve`, which `fsr dev` runs, installed the trace
//! collector and nothing that writes events, so every `tracing::warn!` the host
//! emits reached no log. A card property the renderer leaves out drew with a
//! 200 and said nothing.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use snapfire_fsr_cli::new::{create, NewOptions};
use snapfire_fsr_cli::{build, write, Options};

const PAGE: &str = "export default function Page({ notice }: { notice: { title: string } }) {\n  return <main>{notice.title}</main>;\n}\n";

const LOADER: &str = "import type { Ctx } from \"@snapfire/fsr\";\nexport async function load({ params }: Ctx<\"/notice/{id}\">) {\n  return { notice: { title: `Notice ${params.id}` } };\n}\n";

const IMAGE: &str = "export const size = { width: 300, height: 160 };\nexport const alt = \"A notice\";\n\nexport default function Image({ notice }: { notice: { title: string } }) {\n  return <div style={{ display: \"grid\", width: \"100%\", height: \"100%\", backgroundColor: \"#1e3a8a\", color: \"white\" }}>{notice.title}</div>;\n}\n";

fn project(tag: &str, logging: Option<&str>) -> PathBuf {
  let root = std::env::temp_dir().join(format!("fsr-cli-logging-{}-{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&root);
  create(&root, NewOptions { fetch: false, ..NewOptions::default() }).unwrap();
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

/// Runs `fsr serve` on the project, asks for the card once and returns the status line, then what the process wrote to stdout and to stderr.
fn served(root: &Path) -> (String, String, String) {
  let port = free_port();
  let listen = format!("127.0.0.1:{port}");
  let mut child: Child = Command::new(env!("CARGO_BIN_EXE_fsr")).arg("serve").arg(root.join("app")).args(["--listen", &listen]).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
  let deadline = Instant::now() + Duration::from_secs(30);
  let mut stream = loop {
    match TcpStream::connect(&listen) {
      Ok(stream) => break stream,
      Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
      Err(e) => {
        let _ = child.kill();
        panic!("fsr serve did not listen on {listen}: {e}");
      }
    }
  };
  stream.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
  write!(stream, "GET /notice/roof/opengraph-image.png HTTP/1.1\r\nHost: {listen}\r\nAccept: image/png\r\nConnection: close\r\n\r\n").unwrap();
  let mut response = Vec::new();
  let _ = stream.read_to_end(&mut response);
  let status = String::from_utf8_lossy(&response).lines().next().unwrap_or("").to_owned();
  std::thread::sleep(Duration::from_millis(300));
  let _ = child.kill();
  let output = child.wait_with_output().unwrap();
  (status, String::from_utf8_lossy(&output.stdout).into_owned(), String::from_utf8_lossy(&output.stderr).into_owned())
}

#[test]
fn a_hosts_warning_reaches_stderr_without_a_logging_configuration() {
  let root = project("plain", None);
  let (status, _, stderr) = served(&root);
  assert!(status.contains("200"), "{status}");
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
