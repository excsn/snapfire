//! The one framework that needs a shim: actix has its own request and
//! response types. This maps them onto the `http` types the host speaks.

use std::sync::Arc;

use actix_web::web::{Bytes, Data};
use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer};
use futures_util::TryStreamExt;
use http::Request;
use http_body_util::BodyExt;

use crate::Host;

/// One actix handler over the host, for embedding in an existing actix `App`
/// as its default service.
pub async fn handle(req: HttpRequest, host: Data<Arc<Host>>, body: Bytes) -> HttpResponse {
  let mut builder = Request::builder().method(req.method().as_str()).uri(req.uri().to_string());
  for (name, value) in req.headers() {
    builder = builder.header(name.as_str(), value.as_bytes());
  }
  let request = match builder.body(bytes::Bytes::copy_from_slice(&body)) {
    Ok(request) => request,
    Err(e) => return HttpResponse::BadRequest().body(e.to_string()),
  };

  let response = host.handle(request).await;
  let (parts, body) = response.into_parts();
  let mut out = HttpResponse::build(actix_web::http::StatusCode::from_u16(parts.status.as_u16()).unwrap_or(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR));
  for (name, value) in &parts.headers {
    if let Ok(name) = actix_web::http::header::HeaderName::from_bytes(name.as_str().as_bytes()) {
      if let Ok(value) = actix_web::http::header::HeaderValue::from_bytes(value.as_bytes()) {
        out.append_header((name, value));
      }
    }
  }
  let stream = body.into_data_stream().map_ok(|chunk| Bytes::copy_from_slice(&chunk)).map_err(actix_web::error::ErrorInternalServerError);
  out.streaming(stream)
}

/// Serves the host with actix on `addr`.
pub async fn serve(host: Arc<Host>, addr: (&str, u16)) -> std::io::Result<()> {
  HttpServer::new(move || App::new().app_data(Data::new(host.clone())).default_service(web::to(handle)))
    .bind(addr)?
    .run()
    .await
}
