//! The wave's op rate: windows on one wave, each typing, over real sockets to
//! a host serving in this process. A keystroke is a `typing` row, an op the
//! field applies and a view sent to every window on the wave.
//!
//! Two sweeps. Paced: every window types at `OPS_RATE` keystrokes a second
//! (default 10). Flat out: every window sends as fast as its socket takes
//! frames. Each row runs for `OPS_SECONDS` (default 5). `OPS_SWEEP` picks
//! `paced`, `flat` or both (default).
//!
//! The field's settings: `OPS_TICK_HZ` sends keystroke views on a tick at that
//! rate (default 0, after every keystroke), `OPS_UNIFORM=1` builds one view
//! for everyone instead of one per window and `OPS_DEPTH` is how many ops may
//! wait for the controller before one is dropped (default 256).
//!
//! A body carries its sender, its sequence number and when it was sent, so a
//! receiver measures keystroke to screen and counts the keystrokes it never
//! saw. The clients share the machine and the runtime with the host.
//!
//! `cargo bench --bench ops`

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use bytes::Bytes;
use futures::{SinkExt, Stream, StreamExt};
use http::Request;
use http::header::{COOKIE, SET_COOKIE};
use plaza::{StateControllerBuilder, TickDriver};
use snapfire_fsr_core::Value;
use snapfire_fsr_host::Host;
use snapfire_fsr_host::socket::Sockets;
use tokio::sync::Barrier;
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::{Error, Message};
use wave_react_ts::backend;
use wave_react_ts::field::{Field, Rules, Views};
use wave_react_ts::wire::Wire;

const PACED: &[usize] = &[2, 4, 8, 16, 32, 64];
const FLAT_OUT: &[usize] = &[2, 4, 8, 16];
const WAVE: &str = "kickoff";
/// How long a reader waits for another frame, once typing has stopped, before
/// it counts the room as drained.
const QUIET: Duration = Duration::from_millis(500);

#[derive(Clone, Copy)]
struct Settings {
  tick_hz: u32,
  uniform: bool,
  depth: usize,
}

struct Served {
  url: String,
  cookie: String,
  tasks: Vec<JoinHandle<()>>,
}

/// The wave's host as `main` builds it, on a port of its own, with a session
/// that has opened the wave.
async fn serve(settings: Settings) -> Served {
  let sockets = Arc::new(Sockets::new());
  let wire = Wire::with_depth(sockets.clone(), settings.depth);
  let rules = Rules { on_tick: settings.tick_hz > 0, uniform: settings.uniform, ..Rules::new() };
  let (field, controller) = StateControllerBuilder::new(Arc::new(rules), wire.clone(), Arc::new(Views), Field::new(backend::seed())).build();
  let mut tasks = vec![tokio::spawn(async move {
    let _ = controller.run().await;
  })];
  if settings.tick_hz > 0 {
    let ticking = field.clone();
    tasks.push(tokio::spawn(async move {
      let _ = TickDriver::from_hz(settings.tick_hz).run(ticking).await;
    }));
  }
  let (service, kept) = backend::service(field);
  let listening = wire.clone();
  let host = Host::from(env!("CARGO_MANIFEST_DIR"))
    .expect("the wave's configuration loads")
    .services_over(service)
    .sockets(sockets)
    .topics(|topic, session, _| match topic.strip_prefix("wave/") {
      Some(wave) => matches!(session.get("waves"), Some(Value::Map(open)) if open.contains_key(wave)),
      None => false,
    })
    .socket(move |who, on| listening.on(who, on))
    .build()
    .expect("the host builds");
  let host = Arc::new(host);

  let page = host.handle(Request::get(format!("/wave/{WAVE}")).body(Bytes::new()).unwrap()).await;
  assert!(page.status().is_success(), "the wave page renders: {}", page.status());
  let cookie = page
    .headers()
    .get_all(SET_COOKIE)
    .iter()
    .filter_map(|value| value.to_str().ok()?.split(';').next().map(str::to_owned))
    .collect::<Vec<_>>()
    .join("; ");
  assert!(!cookie.is_empty(), "opening the wave sets a session cookie");

  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let url = format!("ws://{}/_sf/socket?topic=wave/{WAVE}", listener.local_addr().unwrap());
  tasks.push(tokio::spawn(async move {
    let _kept = kept;
    let _ = host.serve_listener(listener).await;
  }));
  Served { url, cookie, tasks }
}

#[derive(Default)]
struct Tally {
  sent: u64,
  seen: u64,
  gaps: u64,
  frames: u64,
  latencies: Vec<u64>,
  began: Option<Instant>,
  last_frame: Option<Instant>,
}

/// One window: joins, names itself, waits for the whole room, then types for
/// `length` and reads until the room goes quiet.
async fn window(me: usize, room: usize, url: String, cookie: String, start: Instant, pace: Option<Duration>, length: Duration, ready: Arc<Barrier>) -> Tally {
  let mut request = url.into_client_request().unwrap();
  request.headers_mut().insert(COOKIE, cookie.parse().unwrap());
  let (socket, _) = tokio_tungstenite::connect_async(request).await.expect("the socket opens");
  let (mut tx, mut rx) = socket.split();
  tx.send(text(serde_json::json!({ "key": "named", "value": { "name": format!("w{me}") } }))).await.unwrap();

  let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
  loop {
    let frame = tokio::time::timeout_at(deadline, rx.next()).await.expect("the whole room arrives").expect("the socket stays open").unwrap();
    if let Message::Text(body) = frame {
      if row_value(body.as_str(), "wave/here").and_then(|here| here.as_array().map(Vec::len)).unwrap_or(0) >= room {
        break;
      }
    }
  }
  ready.wait().await;

  let stop = Arc::new(AtomicBool::new(false));
  let reader = tokio::spawn(read(rx, me, room, start, stop.clone()));
  let began = Instant::now();
  let mut ticks = pace.map(|every| {
    let mut ticks = tokio::time::interval(every);
    ticks.set_missed_tick_behavior(MissedTickBehavior::Delay);
    ticks
  });
  let mut sent = 0;
  while began.elapsed() < length {
    if let Some(ticks) = ticks.as_mut() {
      ticks.tick().await;
    }
    sent += 1;
    let body = format!("{me}:{sent}:{}", start.elapsed().as_micros());
    if tx.send(text(serde_json::json!({ "key": "typing", "value": { "parent": "", "body": body } }))).await.is_err() {
      break;
    }
  }
  stop.store(true, Ordering::Relaxed);
  let mut tally = reader.await.unwrap();
  tally.sent = sent;
  tally.began = Some(began);
  let _ = tx.send(Message::Close(None)).await;
  tally
}

/// Every view this window is sent: each keystroke of another window it has
/// not seen yet, how long it took and how many were skipped to reach it. A
/// uniform view carries this window's own draft too, which is not counted.
async fn read<S>(mut rx: S, me: usize, room: usize, start: Instant, stop: Arc<AtomicBool>) -> Tally
where
  S: Stream<Item = Result<Message, Error>> + Unpin,
{
  let mut tally = Tally::default();
  let mut last = vec![0u64; room];
  loop {
    let next = match tokio::time::timeout(QUIET, rx.next()).await {
      Ok(next) => next,
      Err(_) if stop.load(Ordering::Relaxed) => break,
      Err(_) => continue,
    };
    match next {
      Some(Ok(Message::Text(body))) => {
        tally.frames += 1;
        tally.last_frame = Some(Instant::now());
        let now = start.elapsed().as_micros() as u64;
        for (sender, seq, at) in drafts(body.as_str()) {
          if sender < room && sender != me && seq > last[sender] {
            tally.gaps += seq - last[sender] - 1;
            last[sender] = seq;
            tally.seen += 1;
            tally.latencies.push(now.saturating_sub(at));
          }
        }
      }
      Some(Ok(_)) => {}
      Some(Err(_)) | None => break,
    }
  }
  tally
}

fn text(json: serde_json::Value) -> Message {
  Message::Text(json.to_string().into())
}

fn row_value(body: &str, key: &str) -> Option<serde_json::Value> {
  let mut json: serde_json::Value = serde_json::from_str(body).ok()?;
  let rows = json.get_mut("rows")?.as_array_mut()?;
  let at = rows.iter().position(|row| row.get("key").and_then(serde_json::Value::as_str) == Some(key))?;
  rows.swap_remove(at).get_mut("value").map(serde_json::Value::take)
}

/// `(sender, seq, sent at)` for every draft in a view.
fn drafts(body: &str) -> Vec<(usize, u64, u64)> {
  let Some(serde_json::Value::Array(drafts)) = row_value(body, "wave/drafts") else { return Vec::new() };
  drafts
    .iter()
    .filter_map(|draft| {
      let mut parts = draft.get("body")?.as_str()?.split(':');
      Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
    })
    .collect()
}

async fn row(room: usize, pace: Option<Duration>, length: Duration, settings: Settings) {
  let served = serve(settings).await;
  let start = Instant::now();
  let ready = Arc::new(Barrier::new(room));
  let windows: Vec<_> = (0..room)
    .map(|me| tokio::spawn(window(me, room, served.url.clone(), served.cookie.clone(), start, pace, length, ready.clone())))
    .collect();
  let mut tallies = Vec::new();
  for window in windows {
    tallies.push(window.await.unwrap());
  }
  for task in served.tasks {
    task.abort();
  }

  let sent: u64 = tallies.iter().map(|t| t.sent).sum();
  let seen: u64 = tallies.iter().map(|t| t.seen).sum();
  let gaps: u64 = tallies.iter().map(|t| t.gaps).sum();
  let frames: u64 = tallies.iter().map(|t| t.frames).sum();
  let began = tallies.iter().filter_map(|t| t.began).min().unwrap();
  let active = tallies.iter().filter_map(|t| t.last_frame).max().map(|last| last.duration_since(began)).unwrap_or(length).as_secs_f64();
  let expected = sent * (room as u64 - 1);
  let mut latencies: Vec<u64> = tallies.into_iter().flat_map(|t| t.latencies).collect();
  latencies.sort_unstable();
  let at = |q: f64| latencies.get(((latencies.len() as f64 * q) as usize).min(latencies.len().saturating_sub(1))).map(|us| *us as f64 / 1000.0).unwrap_or(f64::NAN);
  println!(
    "{room:>7}  {:>9.0}  {:>9.0}  {:>9.0}  {:>6.1}%  {:>7}  {:>8.2}  {:>8.2}  {:>8.2}",
    sent as f64 / length.as_secs_f64(),
    frames as f64 / room as f64 / active,
    frames as f64 / active,
    100.0 * seen as f64 / expected.max(1) as f64,
    gaps,
    at(0.5),
    at(0.99),
    latencies.last().map(|us| *us as f64 / 1000.0).unwrap_or(f64::NAN),
  );
}

fn setting(name: &str, default: u64) -> u64 {
  std::env::var(name).ok().and_then(|value| value.parse().ok()).unwrap_or(default)
}

const HEADER: &str = "windows     sent/s  applied/s    views/s    seen     gaps   p50 ms    p99 ms    max ms";

#[tokio::main]
async fn main() {
  let length = Duration::from_secs(setting("OPS_SECONDS", 5));
  let rate = setting("OPS_RATE", 10);
  let sweep = std::env::var("OPS_SWEEP").unwrap_or_default();
  let settings = Settings { tick_hz: setting("OPS_TICK_HZ", 0) as u32, uniform: setting("OPS_UNIFORM", 0) == 1, depth: setting("OPS_DEPTH", 256) as usize };
  println!(
    "views {}, {}, queue {}",
    if settings.uniform { "uniform" } else { "per window" },
    if settings.tick_hz > 0 { format!("on a {} Hz tick", settings.tick_hz) } else { "after every keystroke".to_owned() },
    settings.depth
  );
  let rows = |label: String, rooms: &'static [usize], pace: Option<Duration>| async move {
    println!("\n{label}\n{HEADER}");
    for &room in rooms {
      if tokio::time::timeout(length + Duration::from_secs(60), row(room, pace, length, settings)).await.is_err() {
        println!("{room:>7}  timed out");
      }
    }
  };
  if sweep != "flat" {
    rows(format!("paced, {rate} keystrokes a second per window, {}s a row", length.as_secs()), PACED, Some(Duration::from_secs(1) / rate as u32)).await;
  }
  if sweep != "paced" {
    rows(format!("flat out, {}s a row", length.as_secs()), FLAT_OUT, None).await;
  }
}
