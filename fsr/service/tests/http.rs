use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::Arc;

use parking_lot::Mutex;
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_runtime::{FailureKind, Identity};
use snapfire_fsr_service::{
  Contract, CredentialInterceptor, Field, HttpTransport, IdentityInterceptor, Method, Route,
  Services, TraceInterceptor, Type,
};
use snapfire_fsr_session::TokenCell;

struct Recorded {
  request_line: String,
  headers: Vec<String>,
  body: String,
}

fn serve(responses: Vec<(u16, &'static str)>) -> (String, Arc<Mutex<Vec<Recorded>>>) {
  let listener = TcpListener::bind("127.0.0.1:0").unwrap();
  let base = format!("http://{}", listener.local_addr().unwrap());
  let seen = Arc::new(Mutex::new(Vec::new()));
  let recorder = seen.clone();

  std::thread::spawn(move || {
    for (status, body) in responses {
      let Ok((stream, _)) = listener.accept() else { return };
      let mut reader = BufReader::new(stream);
      let mut request_line = String::new();
      reader.read_line(&mut request_line).unwrap();

      let mut headers = Vec::new();
      let mut length = 0usize;
      loop {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        let line = line.trim_end().to_owned();
        if line.is_empty() {
          break;
        }
        if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
          length = value.trim().parse().unwrap_or(0);
        }
        headers.push(line);
      }
      let mut payload = vec![0u8; length];
      reader.read_exact(&mut payload).unwrap();

      recorder.lock().push(Recorded {
        request_line: request_line.trim_end().to_owned(),
        headers,
        body: String::from_utf8_lossy(&payload).into_owned(),
      });

      let mut stream = reader.into_inner();
      let response = format!(
        "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
      );
      let _ = stream.write_all(response.as_bytes());
      let _ = stream.flush();
    }
  });

  (base, seen)
}

fn contract() -> Contract {
  Contract::new()
    .record("Server", vec![Field::new("name", Type::Str), Field::new("load", Type::F64)])
    .service(
      "fleet",
      snapfire_fsr_service::Service::new()
        .method("get", Method::new(vec![Field::new("name", Type::Str)], Type::named("Server")))
        .method(
          "add",
          Method::new(vec![Field::new("name", Type::Str), Field::new("load", Type::F64)], Type::U32),
        ),
    )
}

fn args(pairs: Vec<(&str, Value)>) -> ValueMap {
  pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect()
}

fn header_of<'r>(recorded: &'r Recorded, name: &str) -> Option<&'r str> {
  recorded
    .headers
    .iter()
    .find(|h| h.to_ascii_lowercase().starts_with(&format!("{name}:")))
    .and_then(|h| h.split_once(':'))
    .map(|(_, value)| value.trim())
}

#[tokio::test]
async fn a_method_posts_its_arguments_and_decodes_the_response() {
  let (base, seen) = serve(vec![(200, r#"{"name":"web-1","load":0.5}"#)]);
  let services = Services::builder()
    .contract(contract())
    .default_transport(Arc::new(HttpTransport::new(&base)))
    .build();

  let got = services
    .bind_anonymous()
    .call("fleet", "get", args(vec![("name", Value::str("web-1"))]))
    .await
    .unwrap();
  assert_eq!(got, Value::Map(args(vec![("name", Value::str("web-1")), ("load", Value::F64(0.5))])));

  let seen = seen.lock();
  assert_eq!(seen[0].request_line.split(' ').take(2).collect::<Vec<_>>(), vec!["POST", "/fleet/get"]);
  assert_eq!(seen[0].body, r#"{"name":"web-1"}"#);
}

#[tokio::test]
async fn an_unpaired_surrogate_in_a_response_decodes_as_the_replacement_character() {
  let (base, _) = serve(vec![
    (200, r#"{"name":"a\ud800b \udc00 \\ud800 \ud83d\ude00","load":0.5}"#),
  ]);
  let services = Services::builder()
    .contract(contract())
    .default_transport(Arc::new(HttpTransport::new(&base)))
    .build();

  let got = services
    .bind_anonymous()
    .call("fleet", "get", args(vec![("name", Value::str("web-1"))]))
    .await
    .unwrap();
  assert_eq!(got, Value::Map(args(vec![("name", Value::str("a\u{FFFD}b \u{FFFD} \\ud800 \u{1F600}")), ("load", Value::F64(0.5))])));
}

#[tokio::test]
async fn every_call_one_request_makes_carries_the_same_request_id() {
  let body = r#"{"name":"web-1","load":0.5}"#;
  let (base, seen) = serve(vec![(200, body), (200, body), (200, body)]);
  let services = Services::builder()
    .contract(contract())
    .intercept(Arc::new(TraceInterceptor::new()))
    .default_transport(Arc::new(HttpTransport::new(&base)))
    .build();

  let request = services.bind_anonymous();
  for _ in 0..2 {
    request.call("fleet", "get", args(vec![("name", Value::str("web-1"))])).await.unwrap();
  }
  services.bind_anonymous().call("fleet", "get", args(vec![("name", Value::str("web-1"))])).await.unwrap();

  let seen = seen.lock();
  let ids: Vec<&str> = seen.iter().map(|r| header_of(r, "x-sf-request-id").unwrap()).collect();
  assert_eq!(ids[0], ids[1], "{ids:?}");
  assert_ne!(ids[1], ids[2], "{ids:?}");
}

#[tokio::test]
async fn interceptor_metadata_becomes_request_headers() {
  let (base, seen) = serve(vec![(200, r#"{"name":"web-1","load":0.5}"#)]);
  let services = Services::builder()
    .contract(contract())
    .intercept(Arc::new(IdentityInterceptor::new()))
    .intercept(Arc::new(CredentialInterceptor::bearer("access_token")))
    .default_transport(Arc::new(HttpTransport::new(&base)))
    .build();

  let tokens = TokenCell::default();
  tokens.set("access_token", Value::str("secret-abc"));
  let identity = Identity::new("alice".into(), Default::default());

  services
    .bind(Some(identity), Arc::new(tokens))
    .call("fleet", "get", args(vec![("name", Value::str("web-1"))]))
    .await
    .unwrap();

  let seen = seen.lock();
  assert_eq!(header_of(&seen[0], "authorization"), Some("Bearer secret-abc"));
  assert_eq!(header_of(&seen[0], "x-sf-subject"), Some("alice"));
}

#[tokio::test]
async fn a_route_template_consumes_the_argument_it_names() {
  let (base, seen) = serve(vec![(200, r#"{"name":"web-1","load":0.5}"#)]);
  let transport = HttpTransport::new(&base).route("fleet.get", Route::get("/servers/{name}"));
  let services = Services::builder()
    .contract(contract())
    .default_transport(Arc::new(transport))
    .build();

  services
    .bind_anonymous()
    .call("fleet", "get", args(vec![("name", Value::str("web-1"))]))
    .await
    .unwrap();

  let seen = seen.lock();
  assert_eq!(seen[0].request_line.split(' ').take(2).collect::<Vec<_>>(), vec!["GET", "/servers/web-1"]);
  assert!(seen[0].body.is_empty(), "a consumed argument is not repeated in the body");
}

#[tokio::test]
async fn statuses_map_onto_the_failure_kinds_a_ui_renders() {
  let (base, _) = serve(vec![(409, "already exists"), (503, "down"), (404, "nope")]);
  let services = Services::builder()
    .contract(contract())
    .default_transport(Arc::new(HttpTransport::new(&base)))
    .build();
  let handle = services.bind_anonymous();
  let call = || handle.call("fleet", "add", args(vec![("name", Value::str("x")), ("load", Value::F64(0.1))]));

  let conflict = call().await.unwrap_err();
  assert_eq!(conflict.kind, FailureKind::Conflict);
  assert!(conflict.message.contains("already exists"), "{conflict}");
  assert_eq!(call().await.unwrap_err().kind, FailureKind::Unavailable);
  assert_eq!(call().await.unwrap_err().kind, FailureKind::NotFound);
}

#[tokio::test]
async fn a_backend_403_429_or_410_reaches_the_page_as_its_own_kind() {
  let (base, _) = serve(vec![(403, "not yours"), (429, "slow down"), (410, "gone for good"), (502, "upstream"), (418, "teapot")]);
  let services = Services::builder()
    .contract(contract())
    .default_transport(Arc::new(HttpTransport::new(&base)))
    .build();
  let handle = services.bind_anonymous();
  let call = || handle.call("fleet", "add", args(vec![("name", Value::str("x")), ("load", Value::F64(0.1))]));

  assert_eq!(call().await.unwrap_err().kind, FailureKind::Forbidden);
  let limited = call().await.unwrap_err();
  assert_eq!(limited.kind, FailureKind::TooManyRequests);
  assert!(limited.message.contains("slow down"), "a 4xx other than 401, 403, 404 and 408 carries the body: {limited}");
  assert_eq!(call().await.unwrap_err().kind, FailureKind::Gone);
  assert_eq!(call().await.unwrap_err().kind, FailureKind::BadGateway);
  assert_eq!(call().await.unwrap_err().kind, FailureKind::Invalid, "a 4xx no kind names is invalid");
}

#[tokio::test]
async fn an_unreachable_backend_is_unavailable_not_a_panic() {
  let listener = TcpListener::bind("127.0.0.1:0").unwrap();
  let base = format!("http://{}", listener.local_addr().unwrap());
  drop(listener);

  let services = Services::builder()
    .contract(contract())
    .default_transport(Arc::new(HttpTransport::new(&base)))
    .build();

  let err = services
    .bind_anonymous()
    .call("fleet", "get", args(vec![("name", Value::str("web-1"))]))
    .await
    .unwrap_err();
  assert_eq!(err.kind, FailureKind::Unavailable);
}

#[tokio::test]
async fn a_server_failure_or_an_unreachable_backend_keeps_its_details_out_of_the_message() {
  let (base, _) = serve(vec![(503, "stack trace at db-7.internal:5432"), (409, "already exists")]);
  let services = Services::builder().contract(contract()).default_transport(Arc::new(HttpTransport::new(&base))).build();
  let handle = services.bind_anonymous();
  let call = || handle.call("fleet", "add", args(vec![("name", Value::str("x")), ("load", Value::F64(0.1))]));
  let down = call().await.unwrap_err();
  assert_eq!(down.kind, FailureKind::Unavailable);
  assert!(!down.message.contains("db-7"), "a 5xx body is logged, not carried to the page: {down}");
  assert!(call().await.unwrap_err().message.contains("already exists"), "a conflict's body is for the user");

  let gone = Services::builder().contract(contract()).default_transport(Arc::new(HttpTransport::new("http://127.0.0.1:9"))).build();
  let unreachable = gone.bind_anonymous().call("fleet", "add", args(vec![("name", Value::str("x")), ("load", Value::F64(0.1))])).await.unwrap_err();
  assert_eq!(unreachable.kind, FailureKind::Unavailable);
  assert!(!unreachable.message.contains("127.0.0.1"), "the backend's address stays in the log: {unreachable}");
}

#[tokio::test]
async fn a_header_the_transport_carries_goes_out_on_every_call() {
  let body = r#"{"name":"web-1","load":0.5}"#;
  let (base, seen) = serve(vec![(200, body), (200, body)]);
  let transport = HttpTransport::new(&base).header("x-api-key", "app-secret").route("fleet.get", Route::get("/servers/{name}"));
  let services = Services::builder().contract(contract()).default_transport(Arc::new(transport)).build();
  let binding = services.bind_anonymous();
  binding.call("fleet", "get", args(vec![("name", Value::str("web-1"))])).await.unwrap();
  binding.call("fleet", "add", args(vec![("name", Value::str("web-2")), ("load", Value::F64(0.1))])).await.unwrap_or(Value::Null);
  let seen = seen.lock();
  assert_eq!(seen.len(), 2);
  for recorded in seen.iter() {
    assert_eq!(header_of(recorded, "x-api-key"), Some("app-secret"), "{}", recorded.request_line);
  }
}

#[tokio::test]
async fn a_query_parameter_the_transport_carries_goes_out_on_every_call_beside_the_arguments() {
  let body = r#"{"name":"web-1","load":0.5}"#;
  let (base, seen) = serve(vec![(200, body), (200, "1")]);
  let transport = HttpTransport::new(&base).query("key", "app secret").route("fleet.get", Route::get("/servers"));
  let services = Services::builder().contract(contract()).default_transport(Arc::new(transport)).build();
  let binding = services.bind_anonymous();
  binding.call("fleet", "get", args(vec![("name", Value::str("web-1"))])).await.unwrap();
  binding.call("fleet", "add", args(vec![("name", Value::str("web-2")), ("load", Value::F64(0.1))])).await.unwrap();
  let seen = seen.lock();
  let target = |i: usize| seen[i].request_line.split(' ').nth(1).unwrap().to_owned();
  assert_eq!(target(0), "/servers?name=web-1&key=app+secret");
  assert_eq!(target(1), "/fleet/add?key=app+secret");
  assert_eq!(seen[1].body, r#"{"name":"web-2","load":0.1}"#, "the parameter is not an argument");
}

/// A backend speaks plain JSON: a whole-number double goes out as `2`, the way JavaScript writes it, and a key named `$` comes back as a key.
#[tokio::test]
async fn a_backend_reads_and_writes_plain_json() {
  let (base, seen) = serve(vec![(200, "7"), (200, r#"{"$":"x","v":"y"}"#)]);
  let contract = contract().service("labels", snapfire_fsr_service::Service::new().method("of", Method::new(vec![], Type::map(Type::Str))));
  let services = Services::builder().contract(contract).default_transport(Arc::new(HttpTransport::new(&base))).build();
  let bound = services.bind_anonymous();
  bound.call("fleet", "add", args(vec![("name", Value::str("web-2")), ("load", Value::F64(2.0))])).await.unwrap();
  assert_eq!(seen.lock()[0].body, r#"{"name":"web-2","load":2}"#);
  let got = bound.call("labels", "of", ValueMap::default()).await.unwrap();
  assert_eq!(got, Value::Map(args(vec![("$", Value::str("x")), ("v", Value::str("y"))])));
}
