//! `matrix <label> <url> <seconds> <levels> -- <server command…>`: per concurrency level in `levels` (comma separated), a fresh server, the time to its first 200, a load run, then the server stopped for its peak RSS and CPU time. One JSON line per level, to stdout and `results/<label>.jsonl`.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::json;

const FIRST_200_LIMIT: Duration = Duration::from_secs(30);
const COOL_DOWN: Duration = Duration::from_secs(8);

fn session_key() -> String {
  let mut bytes = [0u8; 32];
  File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut bytes)).expect("/dev/urandom");
  bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn start(command: &[String], log: &File) -> Child {
  let mut server = Command::new(&command[0]);
  server.args(&command[1..]).stdin(Stdio::null()).stdout(log.try_clone().expect("the log")).stderr(log.try_clone().expect("the log"));
  if std::env::var_os("RELEASE_ENV").is_none() {
    server.env("RELEASE_ENV", "production");
  }
  if std::env::var_os("C5_SESSION__KEY").is_none() {
    server.env("C5_SESSION__KEY", session_key());
  }
  server.spawn().unwrap_or_else(|e| panic!("{}: {e}", command[0]))
}

async fn first_200(url: &str, started: Instant) -> Option<u128> {
  while started.elapsed() < FIRST_200_LIMIT {
    if load::status(url).await == Ok(200) {
      return Some(started.elapsed().as_millis());
    }
    tokio::time::sleep(Duration::from_millis(5)).await;
  }
  None
}

/// SIGTERM, then the server's peak RSS in MB and its user plus system CPU seconds.
fn stop(server: Child) -> (f64, f64) {
  let pid = server.id() as libc::pid_t;
  let mut status = 0;
  let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
  unsafe {
    libc::kill(pid, libc::SIGTERM);
    libc::wait4(pid, &mut status, 0, &mut usage);
  }
  // ru_maxrss is bytes on macOS, kilobytes on Linux.
  let rss_bytes = if cfg!(target_os = "macos") { usage.ru_maxrss as f64 } else { usage.ru_maxrss as f64 * 1024.0 };
  let seconds = |t: libc::timeval| t.tv_sec as f64 + t.tv_usec as f64 / 1e6;
  ((rss_bytes / 1048576.0 * 10.0).round() / 10.0, seconds(usage.ru_utime) + seconds(usage.ru_stime))
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() {
  let args: Vec<String> = std::env::args().collect();
  let Some(split) = args.iter().position(|a| a == "--").filter(|&at| at == 5) else {
    eprintln!("usage: matrix <label> <url> <seconds> <levels> -- <server command…>");
    std::process::exit(2);
  };
  let (label, url) = (&args[1], &args[2]);
  let seconds: u64 = args[3].parse().expect("seconds");
  let levels: Vec<usize> = args[4].split(',').map(|c| c.parse().expect("a concurrency")).collect();
  let command = &args[split + 1..];
  if command.is_empty() {
    eprintln!("no server command after --");
    std::process::exit(2);
  }

  let results = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("results");
  fs::create_dir_all(&results).expect("results/");
  let log = OpenOptions::new().create(true).append(true).open(results.join(format!("{label}-serve.log"))).expect("the server log");
  let mut rows = OpenOptions::new().create(true).append(true).open(results.join(format!("{label}.jsonl"))).expect("the results file");

  for (i, &c) in levels.iter().enumerate() {
    if i > 0 {
      tokio::time::sleep(COOL_DOWN).await;
    }
    let started = Instant::now();
    let server = start(command, &log);
    let cold_ms = first_200(url, started).await;
    let measured = Instant::now();
    let run = match cold_ms {
      Some(_) => load::run(url, c, seconds).await,
      None => Err(format!("no 200 from {url} within {}s", FIRST_200_LIMIT.as_secs())),
    };
    let wall = measured.elapsed().as_secs_f64();
    let (peak_rss_mb, cpu_s) = stop(server);
    let run = match run {
      Ok(run) => run,
      Err(why) => {
        eprintln!("c={c}: {why}");
        std::process::exit(1);
      }
    };
    for why in &run.errors {
      eprintln!("c={c}: {why}");
    }
    let mut row = json!({ "label": label, "c": c, "cold_ms": cold_ms });
    row.as_object_mut().unwrap().extend(run.json().as_object().unwrap().clone());
    row["peak_rss_mb"] = json!(peak_rss_mb);
    row["cpu_pct"] = json!((100.0 * cpu_s / wall).round());
    println!("{row}");
    writeln!(rows, "{row}").expect("the results file");
  }
}
