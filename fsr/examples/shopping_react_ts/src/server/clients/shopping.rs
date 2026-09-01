//! The shopping service this application calls but does not own. Nothing here
//! implements shopping: it describes how to reach the server on the other
//! port, using only the document that server publishes.

use snapfire_fsr_service::openapi::{self, Imported};
use snapfire_fsr_service::HttpTransport;

pub const NAME: &str = "shopping";

pub fn import() -> Imported {
  openapi::import(crate::backend::openapi::DOCUMENT, NAME).expect("the published document imports")
}

/// The transport for this service alone. Its routes come from the document,
/// so no path or verb is written here.
pub fn transport(base_url: &str, imported: &Imported) -> HttpTransport {
  let mut transport = HttpTransport::new(base_url);
  for (path, route) in &imported.routes {
    transport = transport.route(path.clone(), route.clone());
  }
  transport
}
