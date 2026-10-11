use std::sync::Arc;

use snapfire_fsr_host::Host;

/// The reports site in its own process: every route under `/reports`, its
/// ids prefixed, reading a metrics client the portal does not carry. The
/// portal mounts it over this address and renders its pages inside the
/// portal's document.
#[tokio::main]
async fn main() -> std::io::Result<()> {
  // Events out to fibre_logging's appenders, spans kept for `/__fsr/traces`.
  let logging = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fibre_logging.yaml");
  let (traces, _logging, why) = snapfire_fsr_host::trace::observe(&logging);
  if let Some(why) = why {
    eprintln!("logging disabled: {why}");
  }
  let host = Host::from(env!("CARGO_MANIFEST_DIR")).map(|b| b.traces(traces)).and_then(|builder| builder.build()).map_err(std::io::Error::other)?;
  let host = Arc::new(host);
  print!("{}", host.report());
  let listen = host.listen().to_owned();
  println!("reports on http://{listen}/reports");
  host.serve(&listen).await
}
