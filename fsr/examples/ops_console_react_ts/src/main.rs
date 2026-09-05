use std::path::Path;
use std::sync::Arc;

use ops_console_react_ts::backend;
use snapfire_fsr_host::Host;

/// One application, two servers: the fleet service the browser never talks to
/// and the FSR host it does. Everything under `app/` is TypeScript the build
/// lowers; there is no Rust route here at all.
#[actix_web::main]
async fn main() -> std::io::Result<()> {
  let logging = Path::new(env!("CARGO_MANIFEST_DIR")).join("fibre_logging.yaml");
  let _logging = fibre_logging::init::init_from_file(&logging).map_err(|e| eprintln!("logging disabled: {e}")).ok();

  let fleet_addr = ("127.0.0.1", 8091);
  let console_addr = ("127.0.0.1", 8090);

  let host = Host::from(env!("CARGO_MANIFEST_DIR")).and_then(|builder| builder.build()).map_err(std::io::Error::other)?;
  let host = Arc::new(host);

  if std::env::args().any(|arg| arg == "--prerender") {
    let out = host.report.prerender.clone().ok_or_else(|| std::io::Error::other("server.prerender is not configured"))?;
    for (pattern, file) in host.prerender(&out).await.map_err(std::io::Error::other)? {
      println!("{pattern:<22} {}", file.display());
    }
    return Ok(());
  }

  print!("{}", host.report);
  println!("fleet backend on http://{}:{}/agents", fleet_addr.0, fleet_addr.1);
  println!("ops console on http://{}:{}/", console_addr.0, console_addr.1);

  futures_util::try_join!(backend::fleet::serve(backend::fleet::Fleet::seed(), fleet_addr), snapfire_fsr_host::actix::serve(host, console_addr))?;
  Ok(())
}
