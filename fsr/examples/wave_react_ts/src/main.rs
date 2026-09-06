use std::sync::Arc;

use snapfire_fsr_core::Value;
use snapfire_fsr_host::Host;
use wave_react_ts::{backend, presence::Field};

/// Waves, on both seams at once. A blip is durable: an action keeps it, the
/// wave's topic is published and every open page revalidates through the
/// loader. A keystroke is not: it goes over the socket, the field decides
/// what it means and what comes back is store rows, so a draft appears under
/// the blip it answers and is gone when its author stops.
///
/// A wave is not public. `wave/<id>` is opened only by a session that has
/// opened that wave, which the wave's loader records, and the same rule
/// guards the stream and the socket.
#[tokio::main]
async fn main() -> std::io::Result<()> {
  let (transport, waves) = backend::waves();
  let field = Arc::new(Field::new());
  let host = Host::from(env!("CARGO_MANIFEST_DIR"))
    .map(|builder| {
      builder
        .services_over(transport)
        .topics(|topic, session, _| match topic.strip_prefix("wave/") {
          Some(wave) => matches!(session.get("waves"), Some(Value::Map(open)) if open.contains_key(wave)),
          None => false,
        })
        .socket(move |who, on| field.on(who, on))
    })
    .and_then(|builder| builder.build())
    .map_err(std::io::Error::other)?;
  let host = Arc::new(host);
  print!("{}", host.report());

  let publishing = host.clone();
  let mut changes = waves.changes();
  tokio::spawn(async move {
    while let Ok(topic) = changes.recv().await {
      publishing.publish(topic);
    }
  });

  let listen = host.listen().to_owned();
  println!("waves on http://{listen}/");
  println!("open a wave in two windows, name yourself in each and type in one");
  host.serve(&listen).await
}
