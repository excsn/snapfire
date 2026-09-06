#[cfg(feature = "devel")]
mod middleware;
#[cfg(feature = "devel")]
pub(crate) mod ws;
#[cfg(feature = "devel")]
pub use middleware::InjectSnapFireScript;

#[cfg(not(feature = "devel"))]
use actix_web::dev::{Service, ServiceRequest, ServiceResponse, Transform};
#[cfg(not(feature = "devel"))]
use std::future::{Ready, ready};

/// The release-build form of the live-reload middleware, which passes every response
/// through unchanged.
///
/// It exists so that an `App::wrap` call compiles with or without the `devel` feature.
/// See the `devel` build of this type for what the middleware actually does.
#[cfg(not(feature = "devel"))]
#[derive(Debug, Clone, Default)]
pub struct InjectSnapFireScript;

#[cfg(not(feature = "devel"))]
impl<S, B> Transform<S, ServiceRequest> for InjectSnapFireScript
where
  S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error>,
  B: actix_web::body::MessageBody,
{
  type Response = ServiceResponse<B>;
  type Error = actix_web::Error;
  type Transform = S;
  type InitError = ();
  type Future = Ready<Result<Self::Transform, Self::InitError>>;

  fn new_transform(&self, service: S) -> Self::Future {
    ready(Ok(service))
  }
}
