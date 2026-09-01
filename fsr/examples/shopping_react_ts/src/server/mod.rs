pub mod loaders;
pub mod render;
pub mod routes;
pub mod services;
pub mod shell;

use std::sync::Arc;
use std::time::Duration;

use actix_web::web::{Bytes, Data};
use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer};
use futures_util::StreamExt;
use snapfire_fsr::{App as FsrApp, Routes};
use snapfire_fsr_core::ModuleId;
use snapfire_fsr_runtime::{FibreCache, MatchitMatcher, NullEvaluator, Runtime, TableResolver};
use snapfire_fsr_service::Services;

pub use render::{render, respond_with, AppError, RenderMode};
use shell::{ShellEvaluator, SHELL};

pub struct AppCore {
  pub(crate) matcher: MatchitMatcher,
  pub(crate) resolver: TableResolver,
  pub(crate) runtime: Arc<Runtime>,
  pub(crate) services: Arc<Services>,
  pub report: snapfire_fsr::Report,
}

pub fn build_app(backend_url: &str) -> AppCore {
  build_app_over(services::build(backend_url))
}

/// The seam the tests use: the same application over a transport that answers
/// without a backend.
pub fn build_app_over(services: Arc<Services>) -> AppCore {
  let routes = Routes::from_manifest(routes::PLAN).expect("the plan file parses");
  let app = loaders::bind(FsrApp::builder(routes).route("/about", routes::about_plan()))
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
    report: app.report,
  }
}

async fn handle(req: HttpRequest, app: Data<Arc<AppCore>>, _body: Bytes) -> HttpResponse {
  let query = req.query_string();
  let mode = if query.split('&').any(|p| p == "__payload") {
    RenderMode::Payload
  } else {
    RenderMode::Html
  };

  tracing::info!(target: "shopping::http", path = req.path(), payload = (mode == RenderMode::Payload), "request");
  match respond_with(&app, req.path(), mode, Default::default()).await {
    Ok(chunks) => {
      let content_type = match mode {
        RenderMode::Html => "text/html; charset=utf-8",
        RenderMode::Payload => "application/x-sf-payload+json; charset=utf-8",
      };
      HttpResponse::Ok()
        .content_type(content_type)
        .streaming(chunks.map(|c| Ok::<_, actix_web::Error>(Bytes::from(c))))
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
