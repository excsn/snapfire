/// The document the backend publishes and the FSR side imports. It is the only
/// description of this API that anything else is allowed to read.
pub const DOCUMENT: &str = include_str!("../../app/clients/shopping.openapi.json");
