mod common;

use std::time::Duration;
use std::{collections::HashSet, fs};

use actix_web::{App, HttpResponse, HttpServer, rt, test, web};
use futures_util::{SinkExt, StreamExt};
use snapfire::{TeraWeb, actix::dev::InjectSnapFireScript};
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
      .wrap(InjectSnapFireScript::default())
      .configure(move |cfg| app_state_clone.configure_routes(cfg))
      .route("/", web::get().to(test_handler)),
  )
  .await;

  (server, base_url, temp_dir)
}

// Helper function for the websocket test to get the next meaningful message
async fn get_next_text_message(ws_stream: &mut WebSocketStream<MaybeTlsStream<TcpStream>>) -> String {
  loop {
    let msg = timeout(Duration::from_secs(2), ws_stream.next())
      .await
      .expect("Timeout waiting for WS message")
      .expect("Stream ended unexpectedly")
      .expect("WS message error");

    if let Message::Text(text) = msg {
      return text.to_string();
    }
    // Ignore Ping, Pong, Binary, etc. and continue looping
  }
}
async fn simple_html_handler() -> HttpResponse {
  HttpResponse::Ok()
    .content_type("text/html")
    .body("<html><head></head><body>Hello</body></html>")
}

#[actix_rt::test]
async fn test_middleware_injects_script() {
  // Create a temporary directory so the watcher has a valid path.
  let temp_dir = tempdir().unwrap();
  let glob_path = temp_dir.path().join("*.html").to_str().unwrap().to_string();

  // Pass the valid path to the builder.
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
  let body_str = std::str::from_utf8(&body).unwrap();

  println!("--- TEST DEBUG ---");
  println!("Received Body (as string):");
  println!("{}", body_str);
  println!("Received Body Length: {} bytes", body.len());
  println!("--- END TEST DEBUG ---");

  // Check that the original content is still there, split by the injection.
  assert!(body_str.starts_with("<html><head></head><body>Hello"));
  assert!(body_str.ends_with("</body></html>"));

  // Check that the SCRIPT TAG is now present.
  assert!(body_str.contains("<script data-snapfire-reload=\"true\">"));
  assert!(body_str.contains("</script>"));

  // Check for a snippet of the JS content inside the tag.
  assert!(body_str.contains("window.location.reload()"));
}

// This helper now collects all available text messages for a short duration.
async fn collect_ws_messages(
  ws_stream: &mut WebSocketStream<MaybeTlsStream<TcpStream>>,
  duration: Duration,
) -> HashSet<String> {
  let mut received = HashSet::new();

  // Use `timeout` on the entire loop to act as a collection window.
  let _ = timeout(duration, async {
    loop {
      match ws_stream.next().await {
        Some(Ok(Message::Text(text))) => {
          received.insert(text.to_string());
        }
        Some(_) => {
          // Ignore other message types (pings, etc.)
        }
        None => break, // Stream closed
      }
    }
  })
  .await;

  received
}

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
  let configure_closure = {
    let app_state = app_state_clone.clone();
    move |cfg: &mut web::ServiceConfig| app_state.configure_routes(cfg)
  };
  let server = HttpServer::new(move || {
    App::new()
      .app_data(web::Data::new(app_state_clone.clone()))
      .wrap(InjectSnapFireScript::default())
      .configure(configure_closure.clone())
      .route("/", web::get().to(test_handler))
  })
  .listen(listener)
  .unwrap()
  .run();

  let server_handle = server.handle();
  rt::spawn(server);
  rt::time::sleep(Duration::from_millis(200)).await; // Give watcher a bit more time

  // 2. Connect WebSocket client
  let ws_url = format!("{}/_snapfire/ws", base_url).replace("http", "ws");
  let (mut ws_stream, _) = connect_async(&ws_url).await.expect("Failed to connect");

  // 3. Trigger both reloads in quick succession
  fs::write(&template_path, "new content").unwrap();
  fs::write(&css_path, "new css").unwrap();

  // 4. Collect all messages received over a short period
  let messages = collect_ws_messages(&mut ws_stream, Duration::from_secs(1)).await;

  // 5. Assert that both expected messages were received, ignoring order.
  assert!(messages.contains("reload"));
  assert!(messages.contains("reload-css"));

  // 6. Shutdown server
  server_handle.stop(true).await;
}
