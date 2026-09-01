use std::sync::Arc;

use shopping_react_ts::{backend, server};

/// One application, two servers: the shopping service the browser never talks
/// to, and the FSR server it does.
#[actix_web::main]
async fn main() -> std::io::Result<()> {
  let backend_addr = ("127.0.0.1", 8081);
  let fsr_addr = ("127.0.0.1", 8080);

  let catalog = backend::Catalog::seed();
  let app = Arc::new(server::build_app(&format!("http://{}:{}", backend_addr.0, backend_addr.1)));

  println!("shopping backend on http://{}:{}/products", backend_addr.0, backend_addr.1);
  println!("fsr server on http://{}:{}/", fsr_addr.0, fsr_addr.1);

  futures_util::try_join!(backend::serve(catalog, backend_addr), server::serve(app, fsr_addr))?;
  Ok(())
}
