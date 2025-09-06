// TODO: Crate-level documentation will go here.

pub mod core;
pub mod error;

// The entire actix module is compiled only when the feature is enabled.
// For now, we assume an implicit 'actix' feature that is always on.
pub mod actix;

pub use crate::core::app::{Template, TeraWeb, TeraWebBuilder};
pub use crate::error::{Result, SnapfireError};