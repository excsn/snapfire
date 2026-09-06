use std::path::Path;
use std::sync::Arc;

use snapfire_fsr_host::Host;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
  let logging = Path::new(env!("CARGO_MANIFEST_DIR")).join("fibre_logging.yaml");
  let _logging = fibre_logging::init::init_from_file(&logging).map_err(|e| eprintln!("logging disabled: {e}")).ok();

  let host = Host::from(env!("CARGO_MANIFEST_DIR")).and_then(|builder| builder.build()).map_err(std::io::Error::other)?;
  let listen = host.listen().to_owned();
  let host = Arc::new(host);

  print!("{}", host.report());
  println!("www on http://{listen}/");

  host.serve(&listen).await
}
