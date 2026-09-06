use std::sync::Arc;
use std::time::Duration;

use arrivals_react_ts::backend;
use snapfire_fsr_host::Host;

/// The arrivals board over three services, one of which takes a second and
/// one of which takes two. The document goes out with the board rendered and
/// a skeleton where each panel will be, and each panel fills as its service
/// answers, so a `Pending` slot is something to watch rather than something
/// to take on faith.
#[tokio::main]
async fn main() -> std::io::Result<()> {
  let pause = Duration::from_millis(std::env::var("ARRIVALS_PAUSE_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(1000));
  let host = Host::from(env!("CARGO_MANIFEST_DIR"))
    .map(|builder| builder.services_over(backend::board(pause)))
    .and_then(|builder| builder.build())
    .map_err(std::io::Error::other)?;
  let host = Arc::new(host);
  print!("{}", host.report());
  let listen = host.listen().to_owned();
  println!("arrivals on http://{listen}/ with the field {}ms behind and the gates {}ms", pause.as_millis(), pause.as_millis() * 2);
  host.serve(&listen).await
}
