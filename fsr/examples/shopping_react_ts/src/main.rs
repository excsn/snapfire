use std::path::Path;
use std::sync::Arc;

use shopping_react_ts::{backend, routes};
use snapfire_fsr_host::Host;

/// One application, two servers: the shopping service the browser never talks
/// to and the FSR host it does. The host is the stock one over `config/`; the
/// one route added here in Rust is the graduation path, not the norm.
#[actix_web::main]
async fn main() -> std::io::Result<()> {
  let logging = Path::new(env!("CARGO_MANIFEST_DIR")).join("fibre_logging.yaml");
  let _logging = fibre_logging::init::init_from_file(&logging)
    .map_err(|e| eprintln!("logging disabled: {e}"))
    .ok();

  let backend_addr = ("127.0.0.1", 8081);
  let inventory_addr: std::net::SocketAddr = "127.0.0.1:8082".parse().unwrap();
  let fsr_addr = ("127.0.0.1", 8080);

  let catalog = backend::Catalog::seed();
  let host = Host::from(env!("CARGO_MANIFEST_DIR"))
    .and_then(|builder| builder.route("/about", routes::about_plan()).build())
    .map_err(std::io::Error::other)?;
  let host = Arc::new(host);

  if std::env::args().any(|arg| arg == "--prerender") {
    let out = host.report().prerender.clone().ok_or_else(|| std::io::Error::other("server.prerender is not configured"))?;
    for (pattern, file) in host.prerender(&out).await.map_err(std::io::Error::other)? {
      println!("{pattern:<22} {}", file.display());
    }
    return Ok(());
  }

  print!("{}", host.report());
  println!("shopping backend on http://{}:{}/products", backend_addr.0, backend_addr.1);
  println!("inventory grpc on http://{inventory_addr}");
  println!("fsr server on http://{}:{}/", fsr_addr.0, fsr_addr.1);

  futures_util::try_join!(
    backend::shopping::serve(catalog, backend_addr),
    backend::inventory::serve(inventory_addr),
    snapfire_fsr_host::actix::serve(host, fsr_addr)
  )?;
  Ok(())
}
