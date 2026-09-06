#![cfg(not(feature = "devel"))]

use actix_web::{App, HttpResponse, test, web};
use snapfire::{TeraWeb, actix::dev::InjectSnapFireScript};
use tempfile::tempdir;

const PAGE: &str = "<html><head></head><body>Hello</body></html>";

async fn simple_html_handler() -> HttpResponse {
  HttpResponse::Ok().content_type("text/html").body(PAGE)
}

#[actix_rt::test]
async fn test_middleware_does_not_inject_script() {
  let temp_dir = tempdir().unwrap();
  let glob_path = temp_dir.path().join("*.html").to_str().unwrap().to_string();
  let snapfire_app = TeraWeb::builder(&glob_path).build().unwrap();

  let app = test::init_service(
    App::new()
      .app_data(web::Data::new(snapfire_app))
      .wrap(InjectSnapFireScript::default())
      .route("/", web::get().to(simple_html_handler)),
  )
  .await;

  let req = test::TestRequest::get().uri("/").to_request();
  let resp = test::call_service(&app, req).await;
  assert!(resp.status().is_success());

  let body = test::read_body(resp).await;
  assert_eq!(std::str::from_utf8(&body).unwrap(), PAGE);
}

#[actix_rt::test]
async fn test_reload_script_is_empty_without_devel() {
  let temp_dir = tempdir().unwrap();
  let glob_path = temp_dir.path().join("*.html").to_str().unwrap().to_string();
  let snapfire_app = TeraWeb::builder(&glob_path).build().unwrap();

  assert_eq!(snapfire_app.reload_script(), "");
}

#[actix_rt::test]
async fn test_configure_routes_registers_no_websocket() {
  let temp_dir = tempdir().unwrap();
  let glob_path = temp_dir.path().join("*.html").to_str().unwrap().to_string();
  let snapfire_app = TeraWeb::builder(&glob_path).build().unwrap();

  let app_state_clone = snapfire_app.clone();
  let app = test::init_service(
    App::new()
      .app_data(web::Data::new(snapfire_app))
      .configure(move |cfg| app_state_clone.configure_routes(cfg))
      .route("/", web::get().to(simple_html_handler)),
  )
  .await;

  let req = test::TestRequest::get().uri("/_snapfire/ws").to_request();
  let resp = test::call_service(&app, req).await;
  assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_FOUND);
}
