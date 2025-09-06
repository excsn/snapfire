mod common;

use std::fs;
use std::time::Duration;

use actix_web::{App, HttpServer, rt, test, web};
use futures_util::{SinkExt, StreamExt};
use snapfire::{TeraWeb, actix::dev::InjectSnapfireScript};
use tempfile::tempdir;
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};

use crate::common::test_handler;

// Helper to create a fully configured dev-mode server for testing
async fn setup_dev_server() -> (
  impl actix_web::dev::Service<actix_http::Request, Response = actix_web::dev::ServiceResponse, Error = actix_web::Error>,
  String,            // base_url
  tempfile::TempDir, // temp_dir to keep it alive
) {
  let temp_dir = tempdir().unwrap();
  let template_path = temp_dir.path().join("index.html");
  fs::write(&template_path, "<html><body>Hello</body></html>").unwrap();
  let glob_path = temp_dir.path().join("*.html").to_str().unwrap().to_string();

  let static_dir = temp_dir.path().join("static");
  fs::create_dir(&static_dir).unwrap();
  fs::write(static_dir.join("style.css"), "body {}").unwrap();

  let snapfire_app = TeraWeb::builder(&glob_path)
    .watch_static(static_dir.to_str().unwrap())
    .build()
    .unwrap();

  // Note: We use a real listener to get a port for WS connection
  let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
  let addr = listener.local_addr().unwrap();
  let base_url = format!("http://{}", addr);

  let app_state_clone = snapfire_app.clone();
  let server = test::init_service(
    App::new()
      .app_data(web::Data::new(snapfire_app))
      .wrap(InjectSnapfireScript::default())
      .configure(move |cfg| app_state_clone.configure_routes(cfg))
      .route("/", web::get().to(test_handler)),
  )
  .await;

  (server, base_url, temp_dir)
}

#[actix_rt::test]
async fn test_middleware_injects_script() {
  let (server, _base_url, _temp_dir) = setup_dev_server().await;

  let req = test::TestRequest::get().uri("/").to_request();
  let resp = test::call_service(&server, req).await;
  assert!(resp.status().is_success());

  let body = test::read_body(resp).await;
  let body_str = std::str::from_utf8(&body).unwrap();

  assert!(body_str.contains("<html><body>Hello</body>"));
  // Check that our script was injected
  assert!(body_str.contains("window.location.reload()"));
  assert!(body_str.ends_with("</html>"));
}

// This test needs to spawn a real server to test the websocket connection.
#[actix_rt::test]
async fn test_full_reload_pipeline() {
  // 1. Setup server and templates
  let temp_dir = tempdir().unwrap();
  let template_path = temp_dir.path().join("index.html");
  fs::write(&template_path, "<html><body>Hello</body></html>").unwrap();
  let glob_path = temp_dir.path().join("*.html").to_str().unwrap().to_string();

  let static_dir = temp_dir.path().join("static");
  fs::create_dir(&static_dir).unwrap();
  let css_path = static_dir.join("style.css");
  fs::write(&css_path, "body {}").unwrap();

  let snapfire_app = TeraWeb::builder(&glob_path)
    .watch_static(static_dir.to_str().unwrap())
    .build()
    .unwrap();

  let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
  let addr = listener.local_addr().unwrap();
  let base_url = format!("http://{}", addr);

  let app_state_clone = snapfire_app.clone();
  let server = HttpServer::new(move || {
    App::new()
      .app_data(web::Data::new(app_state_clone.clone()))
      .wrap(InjectSnapfireScript::default())
      .configure({
        let value = app_state_clone.clone();
        move |cfg| value.clone().configure_routes(cfg)
      })
      .route("/", web::get().to(test_handler))
  })
  .listen(listener)
  .unwrap()
  .run();

  let server_handle = server.handle();
  rt::spawn(server);
  rt::time::sleep(Duration::from_millis(50)).await; // Give server a moment to start

  // 2. Connect WebSocket client
  let ws_url = format!("{}/_snapfire/ws", base_url).replace("http", "ws");
  let (mut ws_stream, _) = connect_async(&ws_url).await.expect("Failed to connect");

  // 3. Test template reload
  fs::write(&template_path, "new content").unwrap();
  let msg = timeout(Duration::from_secs(2), ws_stream.next())
    .await
    .expect("Timeout waiting for template reload message")
    .unwrap()
    .unwrap();
  assert_eq!(msg, Message::Text("reload".into()));

  // 4. Test CSS reload
  fs::write(&css_path, "new css").unwrap();
  let msg = timeout(Duration::from_secs(2), ws_stream.next())
    .await
    .expect("Timeout waiting for CSS reload message")
    .unwrap()
    .unwrap();
  assert_eq!(msg, Message::Text("reload-css".into()));

  // 5. Shutdown server
  server_handle.stop(true).await;
}
