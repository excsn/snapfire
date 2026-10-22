use std::sync::Arc;

use snapfire_fsr_host::Host;

/// Four trails to read on the way, kept for when the signal goes.
#[tokio::main]
async fn main() -> std::io::Result<()> {
  // Events out to fibre_logging's appenders, spans kept for `/__fsr/traces`.
  let logging = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fibre_logging.yaml");
  let (traces, _logging, why) = snapfire_fsr_host::trace::observe(&logging);
  if let Some(why) = why {
    eprintln!("logging disabled: {why}");
  }
  let host = Arc::new(Host::from(env!("CARGO_MANIFEST_DIR")).and_then(|builder| builder.traces(traces).build()).map_err(std::io::Error::other)?);
  print!("{}", host.report());
  let listen = host.listen().to_owned();
  println!("trails on http://{listen}/; the worker registers outside development");
  host.serve(&listen).await
}
