use futures_util::future::BoxFuture;
use snapfire_fsr_core::Data;

use crate::ctx::RequestCtx;
use crate::data::LoadError;

/// What a segment seeds the browser's store with once its data is known.
/// Registered under the data source's id; every seeding segment of a route
/// contributes, an inner one winning a key an outer one also sets.
pub trait Seeds: Send + Sync {
  fn seed(&self, ctx: &RequestCtx, data: &Data) -> BoxFuture<'static, Result<Data, LoadError>>;
}
