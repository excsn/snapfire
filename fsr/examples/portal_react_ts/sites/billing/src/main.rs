use std::sync::Arc;

use snapfire_fsr_host::Host;

/// The billing site on its own: every route under `/billing`, its ids
/// prefixed, served by the stock host with the site's own layout as the
/// page. Mounted by `portal_react_ts` it is the same artifact under the
/// portal's header.
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
  println!("billing on http://{listen}/billing");
  host.serve(&listen).await
}
