use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use advanced_tera_app::{build_app, http};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
  let config = Path::new(env!("CARGO_MANIFEST_DIR")).join("fibre_logging.yaml");
  let _logging = fibre_logging::init::init_from_file(&config)
    .map_err(|e| eprintln!("logging disabled: {e}"))
    .ok();
  let app = Arc::new(build_app(Duration::from_millis(1500)));
  println!("advanced_tera_app on http://127.0.0.1:8080/dash/servers and /slow/servers");
  http::serve(app, ("127.0.0.1", 8080)).await
}
