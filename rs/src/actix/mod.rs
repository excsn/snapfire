use crate::core::app::{Template, TeraWeb};
use actix_web::{
  HttpRequest, HttpResponse, Responder,
  body::BoxBody,
  http::{StatusCode, header::ContentType},
  web,
  web::ServiceConfig,
};
use futures_util::stream;

pub mod dev;

impl Responder for Template {
  type Body = BoxBody;

  fn respond_to(self, _req: &HttpRequest) -> HttpResponse<Self::Body> {
    // This is a synchronous call, as required.
    let result = self.app_state.render_with_context(&self.template_name, self.context);

    // Create a single-item stream that will resolve immediately with the result.
    let body_stream = stream::once(async {
      result
        .map(|s| s.into()) // Convert String to Bytes
        .map_err(|e| {
          log::error!("Template rendering error: {:?}", e);
          // Convert our internal error into an Actix-compatible error.
          actix_web::error::ErrorInternalServerError(e)
        })
    });

    // Construct the response.
    HttpResponse::build(StatusCode::OK)
      .content_type(ContentType::html())
      .streaming(body_stream)
  }
}

// This block adds the `configure_routes` method.
// It is gated by the `devel` feature.
#[cfg(feature = "devel")]
impl TeraWeb {
  /// Configures Actix services needed by SnapFire for development.
  ///
  /// Currently, this adds the WebSocket route handler for live reloading.
  /// The route is determined by the `ws_path` set in the builder.
  pub fn configure_routes(&self, cfg: &mut ServiceConfig) {
    log::info!(
      "🔥 SnapFire devel enabled. Attaching WebSocket at {}",
      self.reloader.ws_path
    );

    let broadcaster = self.get_reloader_broadcaster();

    cfg.route(
      &self.reloader.ws_path,
      web::get().to(move |req, stream| {
        // We clone the broadcaster for each new connection.
        dev::ws::websocket_handler(req, stream, broadcaster.clone())
      }),
    );
  }
}

#[cfg(not(feature = "devel"))]
impl TeraWeb {
  /// In release builds, this is a no-op that allows user code to compile
  /// without having to add `#[cfg]` attributes.
  pub fn configure_routes(&self, _cfg: &mut ServiceConfig) {
    // Does nothing.
  }
}
