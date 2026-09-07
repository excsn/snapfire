use std::sync::Arc;

use plaza::StateControllerBuilder;
use snapfire_fsr_core::Value;
use snapfire_fsr_host::Host;
use wave_react_ts::backend;
use wave_react_ts::field::{Field, Rules, Views};
use wave_react_ts::wire::Wire;

/// Waves, on one controller. Every wave's state lives in one plaza
/// `StateController`, mutated only by its rules and only from its own task,
/// so nothing here holds a lock: a keystroke, a kept blip, an arrival and a
/// departure are all operations, applied one at a time.
///
/// The transport is the socket the host already terminates. `Wire` turns a
/// connection into an agent and a row into an operation, and turns the view
/// the controller builds for each recipient back into the two store rows the
/// page reads, so the browser never learns that any of this changed.
///
/// A blip is still durable and still arrives through a loader: the action
/// submits an operation, waits for it to apply and publishes the wave's
/// topic, and every open page revalidates. What the socket carries is what is
/// not worth keeping.
#[tokio::main]
async fn main() -> std::io::Result<()> {
  // Events out to fibre_logging's appenders, spans kept for `/__fsr/traces`.
  let logging = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fibre_logging.yaml");
  let (traces, _logging, why) = snapfire_fsr_host::trace::observe(&logging);
  if let Some(why) = why {
    eprintln!("logging disabled: {why}");
  }
  let host = Host::from(env!("CARGO_MANIFEST_DIR")).map(|b| b.traces(traces)).map_err(std::io::Error::other)?;
  let sockets = Arc::new(snapfire_fsr_host::socket::Sockets::new());
  let wire = Wire::new(sockets.clone());

  let (field, controller) = StateControllerBuilder::new(Arc::new(Rules::new()), wire.clone(), Arc::new(Views), Field::new(backend::seed())).build();
  tokio::spawn(async move {
    if let Err(e) = controller.run().await {
      eprintln!("the field stopped: {e}");
    }
  });

  let (service, mut kept) = backend::service(field);
  let listening = wire.clone();
  let host = host
    .services_over(service)
    .sockets(sockets)
    .topics(|topic, session, _| match topic.strip_prefix("wave/") {
      Some(wave) => matches!(session.get("waves"), Some(Value::Map(open)) if open.contains_key(wave)),
      None => false,
    })
    .socket(move |who, on| listening.on(who, on))
    .build()
    .map_err(std::io::Error::other)?;
  let host = Arc::new(host);
  print!("{}", host.report());

  let publishing = host.clone();
  tokio::spawn(async move {
    while let Ok(topic) = kept.recv().await {
      publishing.publish(topic);
    }
  });

  let listen = host.listen().to_owned();
  println!("waves on http://{listen}/");
  println!("open a wave in two windows, name yourself in each and type in one");
  host.serve(&listen).await
}
