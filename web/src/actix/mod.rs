use crate::core::app::{Template, TeraWeb};
use actix_web::{
  HttpRequest, HttpResponse, Responder,
  body::BoxBody,
  http::{StatusCode, header::ContentType},
  web::ServiceConfig,
};
use futures_util::stream;

#[cfg(feature = "devel")]
use actix_web::web;

/// Development-only Actix pieces. The items here exist in every build; without the
/// `devel` feature they are inert, so user code needs no `#[cfg]` attributes.
pub mod dev;

impl Responder for Template {
  type Body = BoxBody;

  fn respond_to(self, _req: &HttpRequest) -> HttpResponse<Self::Body> {
    let result = self.app_state.render_with_context(&self.template_name, self.context);

    let body_stream = stream::once(async {
      result.map(|s| s.into()).map_err(|e| {
        log::error!("Template rendering error: {:?}", e);
        actix_web::error::ErrorInternalServerError(e)
      })
    });

    HttpResponse::build(StatusCode::OK)
      .content_type(ContentType::html())
      .streaming(body_stream)
  }
}

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
      web::get().to(move |req, stream| dev::ws::websocket_handler(req, stream, broadcaster.clone())),
    );
  }
}

#[cfg(not(feature = "devel"))]
impl TeraWeb {
  /// In release builds, this is a no-op that allows user code to compile
  /// without having to add `#[cfg]` attributes.
  pub fn configure_routes(&self, _cfg: &mut ServiceConfig) {}
}
