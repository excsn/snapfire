use std::sync::Arc;

use snapfire_fsr_host::Host;

/// Four evenings of phone photos on a wall. Everything on show is in the
/// images and fonts: the photos are stored on their side with an EXIF
/// orientation, the wall's texture and a caption face come from a
/// stylesheet, the zoom overlay's close icon is imported by code only the
/// browser runs, the photographer's avatar comes from a remote source, one
/// family is vendored from a provider and one is linked from it; every
/// asset is served from a second origin standing in for a CDN.
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
  println!("gallery on http://{listen}/; its images and fonts are served from http://localhost:8200/, the same process under the other name");
  host.serve(&listen).await
}
