use std::sync::Arc;

use shopping_react_ts::{backend, server::routes};
use snapfire_fsr_host::Host;

/// One application, two servers: the shopping service the browser never talks
/// to and the FSR host it does. The host is the stock one over `config/`; the
/// one route added here in Rust is the graduation path, not the norm.
#[actix_web::main]
async fn main() -> std::io::Result<()> {
  let backend_addr = ("127.0.0.1", 8081);
  let fsr_addr = ("127.0.0.1", 8080);

  let catalog = backend::Catalog::seed();
  let host = Host::from(env!("CARGO_MANIFEST_DIR"))
    .and_then(|builder| builder.route("/about", routes::about_plan()).build())
    .map_err(std::io::Error::other)?;
  let host = Arc::new(host);

  print!("{}", host.report);
  println!("shopping backend on http://{}:{}/products", backend_addr.0, backend_addr.1);
  println!("fsr server on http://{}:{}/", fsr_addr.0, fsr_addr.1);

  futures_util::try_join!(backend::serve(catalog, backend_addr), snapfire_fsr_host::actix::serve(host, fsr_addr))?;
  Ok(())
}
