use std::path::Path;
use std::sync::Arc;

use bytes::Bytes;
use http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use ops_console_react_ts::backend::identity::{self, Identity};
use snapfire_fsr_host::{Config, Host};

async fn text(response: http::Response<snapfire_fsr_host::Body>) -> String {
  String::from_utf8(response.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap()
}

fn cookie_of(response: &http::Response<snapfire_fsr_host::Body>) -> String {
  let set = response.headers().get(header::SET_COOKIE).expect("a session cookie").to_str().unwrap();
  set.split(';').next().unwrap().to_owned()
}

fn location(response: &http::Response<snapfire_fsr_host::Body>) -> String {
  response.headers().get(header::LOCATION).unwrap().to_str().unwrap().to_owned()
}

/// The console over the real identity service on a port of its own, the fleet
/// answering from its mock file: every session and every sign-in is a call
/// out, and the host holds neither.
#[actix_web::test]
async fn sessions_and_sign_in_are_calls_to_the_identity_service() {
  let service = Identity::seed();
  let (port, server) = identity::bind(service.clone(), ("127.0.0.1", 0)).unwrap();
  actix_web::rt::spawn(server);

  let mut config = Config::load(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
  config.clients.get_mut("identity").unwrap().base_url = Some(format!("http://127.0.0.1:{port}"));
  config.clients.get_mut("fleet").unwrap().transport = Some("mock".to_owned());
  let host = Arc::new(Host::from_config(config).unwrap().build().unwrap());
  assert!(host.report().to_string().contains("session   service via identity"));

  let response = host.handle(Request::get("/auth/login?return_to=/account").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.status(), StatusCode::SEE_OTHER);
  let cookie = cookie_of(&response);
  assert_eq!(service.session_count(), 1, "the flow state went to the service");

  let response = host
    .handle(Request::post("/auth/callback").header(header::COOKIE, &cookie).header(header::CONTENT_TYPE, "application/x-www-form-urlencoded").body(Bytes::from("user=bob&password=wrong")).unwrap())
    .await;
  assert!(location(&response).starts_with("/login?error=denied"), "a 401 from the service is a denial: {}", location(&response));
  let response = host.handle(Request::get("/login?return_to=%2Faccount").header(header::COOKIE, &cookie).body(Bytes::new()).unwrap()).await;
  assert_eq!(response.status(), StatusCode::OK, "the login page reseeds the flow");

  let response = host
    .handle(Request::post("/auth/callback").header(header::COOKIE, &cookie).header(header::CONTENT_TYPE, "application/x-www-form-urlencoded").body(Bytes::from("user=alice&password=wonder")).unwrap())
    .await;
  if response.status() != StatusCode::SEE_OTHER {
    let status = response.status();
    panic!("callback answered {status}: {}", text(response).await);
  }
  assert_eq!(location(&response), "/account", "the service accepted the password");
  let id = cookie.split('=').nth(1).unwrap().split('.').next().unwrap().to_owned();
  let stored = service.session(&id).unwrap_or_else(|| service.sessions_dump());
  assert!(stored.contains("\"alice\""), "the identity is in the service's record: {stored}");

  let response = host.handle(Request::get("/account").header(header::COOKIE, &cookie).body(Bytes::new()).unwrap()).await;
  assert_eq!(response.status(), StatusCode::OK);
  let html = text(response).await;
  assert!(html.contains("alice") && html.contains("admin"), "{html}");

}
