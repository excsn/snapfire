use std::sync::Arc;
use std::time::Duration;

use arrivals_react_ts::backend;
use snapfire_fsr_host::Host;

/// The arrivals board over three services, one of which takes a second and
/// one of which takes two, on a field whose clock runs fast. The document
/// goes out with the board rendered and a skeleton where each panel will be,
/// each panel fills as its service answers, and from then on the page follows
/// the field: a tick publishes `board`, every open `/_sf/live` stream hears
/// it and the client revalidates the route in place.
#[tokio::main]
async fn main() -> std::io::Result<()> {
  let pause = env_ms("ARRIVALS_PAUSE_MS", 900);
  let tick = env_ms("ARRIVALS_TICK_MS", 3000);
  let speed: f64 = std::env::var("ARRIVALS_SPEED").ok().and_then(|v| v.parse().ok()).unwrap_or(2.0);

  let host = Host::from(env!("CARGO_MANIFEST_DIR"))
    .map(|builder| builder.services_over(backend::running(pause, speed)))
    .and_then(|builder| builder.build())
    .map_err(std::io::Error::other)?;
  let host = Arc::new(host);
  print!("{}", host.report());

  let ticking = host.clone();
  tokio::spawn(async move {
    let mut ticker = tokio::time::interval(tick);
    loop {
      ticker.tick().await;
      ticking.publish("board");
    }
  });

  let listen = host.listen().to_owned();
  println!("arrivals on http://{listen}/");
  println!("the field is {}ms behind and the gates {}ms; {speed} minutes pass a second, the morning repeats every 200 of them and `board` is published every {}ms", pause.as_millis(), pause.as_millis() * 2, tick.as_millis());
  host.serve(&listen).await
}

fn env_ms(name: &str, default: u64) -> Duration {
  Duration::from_millis(std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default))
}
