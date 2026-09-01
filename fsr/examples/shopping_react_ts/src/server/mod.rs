pub mod loaders;
pub mod render;
pub mod routes;
pub mod actions;
pub mod cart;
pub mod clients;
pub mod shell;

use std::sync::Arc;
use std::time::Duration;

use actix_web::http::header;
use actix_web::web::{Bytes, Data};
use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer};
use futures_util::StreamExt;
use snapfire_fsr::{App as FsrApp, Routes};
use snapfire_fsr_core::ModuleId;
use snapfire_fsr_runtime::{FibreCache, MatchitMatcher, NullEvaluator, Runtime, TableResolver};
use snapfire_fsr_service::Services;

pub use render::{call_action, render, respond_with, AppError, RenderMode};
use shell::{ShellEvaluator, SHELL};

pub struct AppCore {
  pub(crate) matcher: MatchitMatcher,
  pub(crate) resolver: TableResolver,
  pub(crate) runtime: Arc<Runtime>,
  pub(crate) services: Arc<Services>,
  pub(crate) actions: snapfire_fsr_runtime::ActionRegistry,
  pub(crate) sessions: snapfire_fsr_session::Sessions,
  pub report: snapfire_fsr::Report,
}

pub fn build_app(backend_url: &str) -> AppCore {
  build_app_over(clients::build(backend_url))
}

/// The seam the tests use: the same application over a transport that answers
/// without a backend.
pub fn build_app_over(services: Arc<Services>) -> AppCore {
  let routes = Routes::from_manifest(routes::PLAN).expect("the plan file parses");
  let app = actions::bind(loaders::bind(
    FsrApp::builder(routes).route("/about", routes::about_plan()),
  ))
    .evaluator(|m: &ModuleId| m.path == SHELL, Arc::new(ShellEvaluator))
    .evaluator(|_: &ModuleId| true, Arc::new(NullEvaluator))
    .cache(Arc::new(FibreCache::bounded(512, Duration::from_secs(30))))
    .services(services)
    .build()
    .expect("every name the plan file declares is bound");

  AppCore {
    matcher: app.matcher,
    resolver: app.resolver,
    runtime: app.runtime,
    services: app.services,
    actions: app.actions,
    sessions: snapfire_fsr_session::Sessions::new(
      Arc::new(snapfire_fsr_session::MemorySessionStore::new(4096, Duration::from_secs(3600))),
      b"shopping-example-dev-key-not-a-secret",
      snapfire_fsr_session::SessionConfig::default(),
    ),
    report: app.report,
  }
}

async fn handle(req: HttpRequest, app: Data<Arc<AppCore>>, body: Bytes) -> HttpResponse {
  let cookie = req.headers().get(header::COOKIE).and_then(|v| v.to_str().ok());
  let opened = app.sessions.open(cookie).await;

  if req.path().starts_with("/_sf/action/") && req.method() == actix_web::http::Method::POST {
    let id = req.path().trim_start_matches("/_sf/action/").to_owned();
    let input = match serde_json::from_slice(&body)
      .map_err(|e| e.to_string())
      .and_then(|json| snapfire_fsr_payload::json_to_value(&json).map_err(|e| e.to_string()))
    {
      Ok(value) => value,
      Err(e) => return HttpResponse::BadRequest().json(serde_json::json!({
        "kind": "invalid", "message": format!("invalid action input: {e}")
      })),
    };

    let result = call_action(&app, &id, opened.cell.clone(), input).await;
    let mut response = match result {
      Ok(value) => HttpResponse::Ok().json(snapfire_fsr_payload::value_to_json(&value)),
      Err(e) => HttpResponse::build(
        actix_web::http::StatusCode::from_u16(e.kind.http_status()).unwrap(),
      )
      .json(serde_json::json!({ "kind": e.kind.as_str(), "message": e.message })),
    };
    if let Some(set_cookie) = app.sessions.persist(&opened).await {
      if let Ok(value) = header::HeaderValue::from_str(&set_cookie) {
        response.headers_mut().append(header::SET_COOKIE, value);
      }
    }
    return response;
  }

  let query = req.query_string();
  let mode = if query.split('&').any(|p| p == "__payload") {
    RenderMode::Payload
  } else {
    RenderMode::Html
  };

  tracing::info!(target: "shopping::http", path = req.path(), payload = (mode == RenderMode::Payload), "request");
  match respond_with(&app, req.path(), mode, opened.cell.clone()).await {
    Ok(chunks) => {
      let content_type = match mode {
        RenderMode::Html => "text/html; charset=utf-8",
        RenderMode::Payload => "application/x-sf-payload+json; charset=utf-8",
      };
      let mut response = HttpResponse::Ok();
      response.content_type(content_type);
      if let Some(set_cookie) = app.sessions.persist(&opened).await {
        if let Ok(value) = header::HeaderValue::from_str(&set_cookie) {
          response.insert_header((header::SET_COOKIE, value));
        }
      }
      response.streaming(chunks.map(|c| Ok::<_, actix_web::Error>(Bytes::from(c))))
    }
    Err(AppError::NotFound(path)) => HttpResponse::NotFound().body(format!("no route: {path}")),
    Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
  }
}

/// The first server: the one a browser talks to.
pub async fn serve(app: Arc<AppCore>, addr: (&str, u16)) -> std::io::Result<()> {
  let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
  let client_dist = manifest.join("../../client/dist");
  let app_dist = manifest.join("app/dist");
  let vendor = manifest.join("vendor");
  HttpServer::new(move || {
    App::new()
      .app_data(Data::new(app.clone()))
      .service(actix_files::Files::new("/static/js/fsr", client_dist.clone()))
      .service(actix_files::Files::new("/static/js/app", app_dist.clone()))
      .service(actix_files::Files::new("/static/js/vendor", vendor.clone()))
      .default_service(web::to(handle))
  })
  .bind(addr)?
  .run()
  .await
}
