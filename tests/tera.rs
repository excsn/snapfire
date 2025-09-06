mod common;

use crate::common::test_handler;

use actix_web::{App, Responder, test, web};
use snapfire::{TeraWeb};
use std::fs;
use tempfile::tempdir;
use tera::Context;

#[actix_rt::test]
async fn test_render_in_actix_server() {
  // 1. Setup: Create a temporary template file
  let temp_dir = tempdir().unwrap();
  let template_path = temp_dir.path().join("index.html");
  let template_content = "<html><head><title>{{ site_name }} | {{ page_title }}</title></head></html>";
  fs::write(&template_path, template_content).unwrap();
  let glob_path = temp_dir.path().join("*.html").to_str().unwrap().to_string();

  // 2. Setup: Build the Snapfire app state
  let snapfire_app = TeraWeb::builder(&glob_path)
    .add_global("site_name", "Snapfire App")
    .build()
    .unwrap();

  // 3. Setup: Create the Actix test server
  let app = test::init_service(
    App::new()
      .app_data(web::Data::new(snapfire_app))
      .route("/", web::get().to(test_handler)),
  )
  .await;

  // 4. Execution: Make a request to the test server
  let req = test::TestRequest::get().uri("/").to_request();
  let resp = test::call_service(&app, req).await;

  // 5. Assertion: Check the response status and body
  assert!(resp.status().is_success());

  let body = test::read_body(resp).await;
  let body_str = std::str::from_utf8(&body).unwrap();

  let expected_html = "<html><head><title>Snapfire App | Integration Test</title></head></html>";
  assert_eq!(body_str, expected_html);
}
