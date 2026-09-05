use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use advanced_tera_app::build;

#[tokio::main]
async fn main() -> std::io::Result<()> {
  let config = Path::new(env!("CARGO_MANIFEST_DIR")).join("fibre_logging.yaml");
  let _logging = fibre_logging::init::init_from_file(&config)
    .map_err(|e| eprintln!("logging disabled: {e}"))
    .ok();
  let host = Arc::new(build(Duration::from_millis(1500)).map_err(std::io::Error::other)?);
  print!("{}", host.report());
  println!("advanced_tera_app on http://{}/dash/servers and /slow/servers", host.listen());
  let listen = host.listen().to_owned();
  host.serve(&listen).await
}
