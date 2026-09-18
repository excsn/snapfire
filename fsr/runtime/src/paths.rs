use futures_util::future::BoxFuture;
use snapfire_fsr_core::Params;

use crate::ctx::RequestCtx;
use crate::data::LoadError;

/// The parameter sets a route with a parameter is prerendered for, one per
/// path. Registered under the route's pattern; the host asks once per locale
/// with nothing of a request behind the context.
pub trait Paths: Send + Sync {
  fn paths(&self, ctx: &RequestCtx) -> BoxFuture<'static, Result<Vec<Params>, LoadError>>;
}
