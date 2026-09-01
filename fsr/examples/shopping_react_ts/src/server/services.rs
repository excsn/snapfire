use std::sync::Arc;

use snapfire_fsr_service::openapi::{self, Imported};
use snapfire_fsr_service::{HttpTransport, IdentityInterceptor, Services, TraceInterceptor};

pub const SHOPPING: &str = "shopping";

/// The document the backend publishes is the only description of it anything
/// here reads. Nothing hand-writes a client.
pub fn import() -> Imported {
  openapi::import(crate::backend::openapi::DOCUMENT, SHOPPING).expect("the published document imports")
}

pub fn build(base_url: &str) -> Arc<Services> {
  let imported = import();
  let mut transport = HttpTransport::new(base_url);
  for (path, route) in imported.routes {
    transport = transport.route(path, route);
  }

  Services::builder()
    .contract(imported.contract)
    .intercept(Arc::new(TraceInterceptor::new()))
    .intercept(Arc::new(IdentityInterceptor::new()))
    .default_transport(Arc::new(transport))
    .build()
}
