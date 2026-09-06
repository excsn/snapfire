//! The stock host over a small application written to a temporary directory:
//! a plan with one lowered loader and one lowered action, a contract, a
//! static root and an `app.toml`.

use std::path::PathBuf;
use std::sync::Arc;

use bytes::Bytes;
use http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_host::{Host, RenderMode};
use snapfire_fsr_runtime::SessionCell;
use snapfire_fsr_service::MockTransport;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const PLAN: &str = r#"{
  "version": 2,
  "routes": [
    { "pattern": "/", "plan": { "id": 0, "module": "shell#document", "children": [
      { "slot": "content", "node": { "id": 1, "module": "routes/index/page.tsx#default", "source": "index" } } ] } },
    { "pattern": "/hello/{name}", "plan": { "id": 0, "module": "shell#document", "children": [
      { "slot": "content", "node": { "id": 1, "module": "routes/hello/page.tsx#default", "source": "hello", "cache_key": "routes/hello/page.tsx#default" } } ] } },
    { "pattern": "/feed", "plan": { "id": 0, "module": "shell#document", "children": [
      { "slot": "content", "node": { "id": 1, "module": "routes/feed/layout.tsx#default", "children": [
        { "slot": "content", "node": { "id": 2, "module": "routes/feed/page.tsx#default" } } ] } } ] } },
    { "pattern": "/feed/photo/{id}", "plan": { "id": 0, "module": "shell#document", "children": [
      { "slot": "content", "node": { "id": 1, "module": "routes/feed/layout.tsx#default", "children": [
        { "slot": "content", "node": { "id": 2, "module": "routes/feed/photo/page.tsx#default" } } ] } } ] } }
    ,
    { "pattern": "/deck", "plan": { "id": 0, "module": "shell#document", "children": [
      { "slot": "content", "node": { "id": 1, "module": "routes/layout.tsx#default", "children": [
        { "slot": "content", "node": { "id": 2, "module": "routes/deck/layout.tsx#default", "children": [
          { "slot": "content", "node": { "id": 3, "module": "routes/deck/page.tsx#default" } } ] } } ] } } ] } },
    { "pattern": "/deck/card/{id}", "plan": { "id": 0, "module": "shell#document", "children": [
      { "slot": "content", "node": { "id": 1, "module": "routes/layout.tsx#default", "children": [
        { "slot": "content", "node": { "id": 2, "module": "routes/deck/layout.tsx#default", "children": [
          { "slot": "content", "node": { "id": 3, "module": "routes/deck/card/page.tsx#default" } } ] } } ] } } ] } }
  ],
  "intercepts": [
    { "pattern": "/feed/photo/{id}", "plan": { "id": 0, "module": "shell#document", "children": [
      { "slot": "content", "node": { "id": 1, "module": "routes/feed/layout.tsx#default", "keep": ["content", "drawer"], "children": [
        { "slot": "modal", "node": { "id": 2, "module": "routes/feed/photo/page.modal.tsx#default" } } ] } } ] } },
    { "pattern": "/feed/photo/{id}", "plan": { "id": 0, "module": "shell#document", "children": [
      { "slot": "content", "node": { "id": 1, "module": "routes/feed/layout.tsx#default", "keep": ["content", "modal"], "children": [
        { "slot": "drawer", "node": { "id": 2, "module": "routes/feed/photo/page.drawer.tsx#default" } } ] } } ] } },
    { "pattern": "/deck/card/{id}", "plan": { "id": 0, "module": "shell#document", "children": [
      { "slot": "content", "node": { "id": 1, "module": "routes/layout.tsx#default", "keep": ["side"], "children": [
        { "slot": "content", "node": { "id": 2, "module": "routes/deck/layout.tsx#default", "keep": ["content"], "children": [
          { "slot": "peek", "node": { "id": 3, "module": "routes/deck/card/page.peek.tsx#default" } } ] } } ] } } ] } }
  ],
  "sources": [
    { "id": "index", "owner": "lowered", "module": "routes/index/page.loader.ts",
      "body": [ { "return": { "object": [ { "field": [ "items", { "call": { "service": "shop", "method": "list", "args": [] } } ] } ] } } ] },
    { "id": "hello", "owner": "lowered", "module": "routes/hello/page.loader.ts",
      "body": [ { "return": { "object": [ { "field": [ "greeting", { "template": [ { "lit": { "str": "hi " } }, { "param": "name" }, { "lit": { "str": " via " } }, { "query": "from" } ] } ] } ] } } ] }
  ],
  "actions": [
    { "id": "index.where", "owner": "lowered", "module": "routes/index/actions.ts",
      "body": [ { "return": "locale" } ] },
    { "id": "index.bump", "owner": "lowered", "module": "routes/index/actions.ts", "input": "Bump",
      "body": [
        { "session_set": { "key": "count", "value": { "arith": [ "add", { "coalesce": [ { "session": "count" }, { "lit": { "int": 0 } } ] }, { "field": [ "input", "by" ] } ] } } },
        { "return": { "session": "count" } }
      ] }
  ]
}"#;

const CONTRACT: &str = r#"{
  "types": { "Bump": { "record": { "fields": [ { "name": "by", "type": "i64" } ] } } },
  "services": { "shop": { "methods": { "list": { "params": [], "returns": { "list": "str" } } } } }
}"#;

fn app_dir() -> PathBuf {
  let dir = std::env::temp_dir().join(format!("fsr-host-test-{}-{}", std::process::id(), rand_suffix()));
  std::fs::create_dir_all(dir.join("generated/contracts")).unwrap();
  std::fs::create_dir_all(dir.join("public")).unwrap();
  std::fs::write(dir.join("generated/plan.json"), PLAN).unwrap();
  std::fs::write(dir.join("generated/contracts/shop.json"), CONTRACT).unwrap();
  std::fs::write(dir.join("public/app.js"), "console.log('hello')").unwrap();
  std::fs::write(dir.join("importmap.json"), r#"{"imports":{}}"#).unwrap();
  std::fs::write(
    dir.join("app.toml"),
    r#"
[app]
dir = "."

[server]
listen = "127.0.0.1:0"

[document]
title = "Test <app>"
entry = "/static/app.js"

[session]
key = "test-key"
ttl = "10m"

[[static]]
route = "/static"
dir = "public"
"#,
  )
  .unwrap();
  dir
}

fn rand_suffix() -> u128 {
  static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
  let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
  nanos * 1000 + u128::from(NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed) % 1000)
}

fn host() -> (Arc<Host>, Arc<MockTransport>) {
  let transport = Arc::new(MockTransport::new().returns("shop.list", Value::Seq(vec![Value::str("a"), Value::str("b")])));
  let host = Host::from(app_dir().join("app.toml")).unwrap().services_over(transport.clone()).build().unwrap();
  (Arc::new(host), transport)
}

#[tokio::test]
async fn a_route_renders_through_the_stock_shell_with_the_configured_head() {
  let (host, _) = host();
  let html = host.render_to_string("/", RenderMode::Html, SessionCell::default()).await.unwrap();
  assert!(html.contains("<!doctype html>"), "{html}");
  assert!(html.contains("<title>Test &lt;app&gt;</title>"), "the title is escaped: {html}");
  assert!(html.contains("<script type=\"importmap\">{\"imports\":{}}</script>"), "{html}");
  assert!(html.contains("<script type=\"module\" src=\"/static/app.js\"></script>"), "{html}");
  assert!(html.contains("data-sf-module=\"routes/index/page.tsx#default\""), "{html}");
  assert!(html.contains("\"a\""), "the lowered loader ran: {html}");
  assert_eq!(host.report().app.sources.len(), 2);
  assert!(host.report().to_string().contains("lowered"), "{}", host.report());
  assert!(host.report().to_string().contains("/static"), "{}", host.report());
}

#[tokio::test]
async fn params_and_query_reach_a_lowered_loader() {
  let (host, _) = host();
  let payload = host.render_to_string("/hello/norm?from=test", RenderMode::Payload, SessionCell::default()).await.unwrap();
  assert!(payload.contains("hi norm via test"), "{payload}");
}

#[tokio::test]
async fn a_body_over_the_limit_is_refused_before_anything_reads_it() {
  let (host, _) = host();
  let response = host
    .handle(Request::post("/_sf/action/index.bump").header(header::CONTENT_TYPE, "application/json").body(Bytes::from(vec![b' '; (1 << 20) + 1])).unwrap())
    .await;
  assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
  assert!(response.headers().get(header::SET_COOKIE).is_none(), "no session was opened for it");
  let body = response.into_body().collect().await.unwrap().to_bytes();
  assert!(std::str::from_utf8(&body).unwrap().contains("server.max_body"), "{body:?}");
}

#[tokio::test]
async fn the_edge_serves_static_files_actions_and_pages_with_a_session_cookie() {
  let (host, _) = host();

  let response = host.handle(Request::get("/static/app.js").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.status(), StatusCode::OK);
  let body = response.into_body().collect().await.unwrap().to_bytes();
  assert_eq!(&body[..], b"console.log('hello')");

  let response = host.handle(Request::get("/static/missing.js").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.status(), StatusCode::NOT_FOUND);

  let response = host
    .handle(Request::post("/_sf/action/index.bump").header(header::CONTENT_TYPE, "application/json").body(Bytes::from(r#"{"by": 2}"#)).unwrap())
    .await;
  assert_eq!(response.status(), StatusCode::OK);
  let cookie = response.headers().get(header::SET_COOKIE).expect("a fresh session sets its cookie").to_str().unwrap().to_owned();
  let body = response.into_body().collect().await.unwrap().to_bytes();
  assert_eq!(&body[..], b"2");

  let cookie_value = cookie.split(';').next().unwrap().to_owned();
  let response = host
    .handle(Request::post("/_sf/action/index.bump").header(header::COOKIE, &cookie_value).body(Bytes::from(r#"{"by": 3}"#)).unwrap())
    .await;
  let body = response.into_body().collect().await.unwrap().to_bytes();
  assert_eq!(&body[..], b"5", "the session carried across requests");

  let response = host
    .handle(Request::post("/_sf/action/index.bump").body(Bytes::from(r#"{"by": "two"}"#)).unwrap())
    .await;
  assert_eq!(response.status(), StatusCode::BAD_REQUEST, "the input is checked against the contract");

  let response = host.handle(Request::get("/hello/x?from=y&__payload").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.headers().get(header::CONTENT_TYPE).unwrap(), "application/x-sf-payload+json; charset=utf-8");

  let response = host.handle(Request::get("/nope").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn hyper_serves_the_same_edge() {
  let (host, _) = host();
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let addr = listener.local_addr().unwrap();
  tokio::spawn(host.clone().serve_listener(listener));

  let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
  stream
    .write_all(b"GET /hello/hyper?from=tcp HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
    .await
    .unwrap();
  let mut response = String::new();
  stream.read_to_string(&mut response).await.unwrap();
  assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
  assert!(response.contains("text/html"), "{response}");
  assert!(response.contains("hi hyper via tcp"), "{response}");
}

/// One request over a prior-knowledge HTTP/2 connection, since the listener
/// carries no TLS and so no ALPN.
async fn over_h2(addr: std::net::SocketAddr, path: &str) -> Result<(http::Version, String), Box<dyn std::error::Error + Send + Sync>> {
  let io = hyper_util::rt::TokioIo::new(tokio::net::TcpStream::connect(addr).await?);
  let (mut sender, conn) = hyper::client::conn::http2::handshake(hyper_util::rt::TokioExecutor::new(), io).await?;
  tokio::spawn(conn);
  let request = Request::get(path).header(header::HOST, "localhost").body(http_body_util::Empty::<Bytes>::new())?;
  let response = sender.send_request(request).await?;
  let version = response.version();
  let body = response.into_body().collect().await?.to_bytes();
  Ok((version, String::from_utf8_lossy(&body).into_owned()))
}

#[tokio::test]
async fn http2_is_off_until_the_configuration_asks_for_it() {
  let (host, _) = host();
  assert!(!host.report().http2, "the default is http/1.1 alone");
  assert!(!host.report().to_string().contains("h2c"), "{}", host.report());

  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let addr = listener.local_addr().unwrap();
  tokio::spawn(host.clone().serve_listener(listener));

  let attempt = tokio::time::timeout(std::time::Duration::from_secs(5), over_h2(addr, "/hello/h2?from=two")).await;
  assert!(matches!(attempt, Ok(Err(_))), "an http/1.1 listener refuses the preface rather than answering it");
}

#[tokio::test]
async fn http2_serves_a_prior_knowledge_client_beside_http1() {
  let transport = Arc::new(MockTransport::new().returns("shop.list", Value::Seq(vec![Value::str("a")])));
  let host = Arc::new(Host::from(app_dir().join("app.toml")).unwrap().services_over(transport).http2(true).build().unwrap());
  assert!(host.report().http2);
  assert!(host.report().to_string().contains("h2c beside http/1.1"), "{}", host.report());

  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let addr = listener.local_addr().unwrap();
  tokio::spawn(host.clone().serve_listener(listener));

  let (version, body) = tokio::time::timeout(std::time::Duration::from_secs(5), over_h2(addr, "/hello/h2?from=two")).await.unwrap().unwrap();
  assert_eq!(version, http::Version::HTTP_2);
  assert!(body.contains("hi h2 via two"), "{body}");

  let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
  stream
    .write_all(b"GET /hello/one?from=tcp HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
    .await
    .unwrap();
  let mut response = String::new();
  stream.read_to_string(&mut response).await.unwrap();
  assert!(response.starts_with("HTTP/1.1 200 OK"), "the same listener still answers http/1.1: {response}");
  assert!(response.contains("hi one via tcp"), "{response}");
}

#[tokio::test]
async fn the_tower_service_answers_like_the_edge() {
  use tower::ServiceExt;

  let (host, _) = host();
  let response = host
    .service()
    .oneshot(Request::get("/hello/tower?from=svc").body(http_body_util::Full::new(Bytes::new())).unwrap())
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let body = response.into_body().collect().await.unwrap().to_bytes();
  assert!(String::from_utf8_lossy(&body).contains("hi tower via svc"));
}

#[tokio::test]
async fn call_action_runs_the_lowered_body_against_a_given_session() {
  let (host, _) = host();
  let session = SessionCell::default();
  let mut input = ValueMap::new();
  input.insert("by".to_owned(), Value::int(4i64));
  let out = host.call_action("index.bump", session.clone(), Value::Map(input)).await.unwrap();
  assert_eq!(out, Value::int(4i64));
  assert_eq!(session.get("count"), Some(Value::int(4i64)));
}

#[test]
fn a_missing_session_key_or_an_unknown_key_refuses_to_start() {
  let dir = app_dir();
  std::fs::write(dir.join("app.toml"), "[server]\nlisten = \"127.0.0.1:0\"\n").unwrap();
  let err = Host::from(dir.join("app.toml")).unwrap_err();
  assert!(err.to_string().contains("session"), "{err}");

  std::fs::write(dir.join("app.toml"), "[session]\nttl = \"1h\"\n").unwrap();
  let err = Host::from(dir.join("app.toml")).unwrap_err();
  assert!(err.to_string().contains("key"), "{err}");

  std::fs::write(dir.join("app.toml"), "[session]\nkey = \"k\"\n[server]\nlisten = \"x\"\nport = 1\n").unwrap();
  let err = Host::from(dir.join("app.toml")).unwrap_err();
  assert!(err.to_string().contains("port"), "an unknown key names itself: {err}");

  std::fs::write(dir.join("app.toml"), "[session]\nkey = \"k\"\n[nonsense]\nx = 1\n").unwrap();
  let err = Host::from(dir.join("app.toml")).unwrap_err();
  assert!(err.to_string().contains("nonsense"), "an unknown section names itself: {err}");
}

#[tokio::test]
async fn axum_nests_the_host_under_a_prefix() {
  use axum::Router;
  use tower::ServiceExt;

  let (host, _) = host();
  let app = Router::new().nest_service("/shop", host.service());
  let response = app
    .oneshot(Request::get("/shop/hello/axum?from=router").body(axum::body::Body::empty()).unwrap())
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
  assert!(String::from_utf8_lossy(&body).contains("hi axum via router"), "{}", String::from_utf8_lossy(&body));
}

#[test]
fn the_environment_overrides_the_file_and_the_report_says_where_config_came_from() {
  let dir = app_dir();
  unsafe { std::env::set_var("C5_SERVER__LISTEN", "127.0.0.1:9") };
  let host = Host::from(dir.join("app.toml")).unwrap().services_over(Arc::new(MockTransport::new())).build().unwrap();
  unsafe { std::env::remove_var("C5_SERVER__LISTEN") };
  assert_eq!(host.listen(), "127.0.0.1:9");
  assert!(host.report().to_string().contains("config"), "{}", host.report());
}

#[test]
fn a_project_root_with_a_config_directory_infers_the_rest_from_the_app() {
  let root = std::env::temp_dir().join(format!("fsr-host-root-{}-{}", std::process::id(), rand_suffix()));
  let app = root.join("app");
  std::fs::create_dir_all(root.join("config")).unwrap();
  std::fs::create_dir_all(app.join("generated/contracts")).unwrap();
  std::fs::create_dir_all(app.join("dist/src")).unwrap();
  std::fs::create_dir_all(app.join("vendor/react")).unwrap();
  std::fs::create_dir_all(app.join("clients")).unwrap();
  std::fs::create_dir_all(app.join("styles")).unwrap();
  std::fs::write(app.join("styles/app.css"), "body{}").unwrap();
  std::fs::write(app.join("styles/notes.txt"), "not a sheet").unwrap();
  std::fs::write(app.join("generated/plan.json"), PLAN).unwrap();
  std::fs::write(app.join("generated/contracts/shop.json"), CONTRACT).unwrap();
  std::fs::write(app.join("dist/src/main.js"), "boot()").unwrap();
  std::fs::write(app.join("dist/.snapfire-build.json"), r#"{"version":1,"entries":["src/main.js"],"publicPath":"/static/js/app/","outputs":[],"externals":[],"graph":{}}"#).unwrap();
  std::fs::write(app.join("vendor/react/react.js"), "react").unwrap();
  std::fs::write(app.join("importmap.json"), r#"{"imports":{"react":"/static/js/vendor/react/react.js"}}"#).unwrap();
  std::fs::write(app.join("clients/shop.openapi.json"), r#"{"openapi":"3.0.0","info":{"title":"shop","version":"1"},"paths":{"/items":{"get":{"operationId":"list","responses":{"200":{"description":"ok","content":{"application/json":{"schema":{"type":"array","items":{"type":"string"}}}}}}}}}}"#).unwrap();
  std::fs::write(root.join("config/app.toml"), "[server]\nlisten = \"127.0.0.1:0\"\n[document]\ntitle = \"Inferred\"\n[session]\nkey = \"k\"\n[clients.shop]\nbase_url = \"http://127.0.0.1:1\"\n").unwrap();
  std::fs::write(root.join("config/local.toml"), "[document]\ntitle = \"Layered\"\n").unwrap();

  let host = Host::from(&root).unwrap().build().unwrap();
  let report = host.report().to_string();
  assert!(report.contains("/static/js/app"), "dist is served at the build's publicPath: {report}");
  assert!(report.contains("/static/js/vendor"), "vendor/ is served by convention: {report}");
  assert!(report.contains("inferred"), "{report}");
  assert!(report.contains("shop"), "the client document was found under clients/: {report}");

  let rt = tokio::runtime::Runtime::new().unwrap();
  let html = rt.block_on(host.render_to_string("/hello/x?from=y", RenderMode::Html, SessionCell::default())).unwrap();
  assert!(html.contains("<title>Layered</title>"), "the later file in config/ wins: {html}");
  assert!(html.contains("src=\"/static/js/app/src/main.js\""), "the entry is inferred: {html}");
  assert!(html.contains("\"react\""), "the import map is inferred: {html}");
  assert!(html.contains("<link rel=\"stylesheet\" href=\"/static/css/app.css\">"), "the stylesheet is inferred: {html}");
  assert!(!html.contains("notes.txt"), "{html}");
  let response = rt.block_on(host.handle(Request::get("/static/css/app.css").body(Bytes::new()).unwrap()));
  assert_eq!(response.status(), StatusCode::OK);

  let response = rt.block_on(host.handle(Request::get("/static/js/vendor/react/react.js").body(Bytes::new()).unwrap()));
  assert_eq!(response.status(), StatusCode::OK);
  let response = rt.block_on(host.handle(Request::get("/static/js/app/src/main.js").body(Bytes::new()).unwrap()));
  assert_eq!(response.status(), StatusCode::OK);
}

#[test]
fn a_config_directory_loads_the_deployment_ladder_in_order_and_nothing_else() {
  use snapfire_fsr_host::config::{config_paths, locate_with, Config, Deployment};

  let root = std::env::temp_dir().join(format!("fsr-host-ladder-{}-{}", std::process::id(), rand_suffix()));
  let dir = root.join("config");
  std::fs::create_dir_all(&dir).unwrap();
  std::fs::write(dir.join("app.toml"), "[session]\nkey = \"k\"\nttl = \"1h\"\n[document]\ntitle = \"App\"\n").unwrap();
  std::fs::write(dir.join("staging.toml"), "[document]\ntitle = \"Staging\"\n[session]\nttl = \"2h\"\n").unwrap();
  std::fs::write(dir.join("local.yaml"), "document:\n  title: Local\n").unwrap();
  std::fs::write(dir.join("eu.toml"), "[session]\nttl = \"3h\"\n").unwrap();
  std::fs::write(dir.join("local-eu.toml"), "[session]\nttl = \"4h\"\n").unwrap();
  std::fs::write(dir.join("aaa.toml"), "[document]\ntitle = \"Alphabetical\"\n").unwrap();
  std::fs::write(dir.join("zzz.toml"), "[session]\nttl = \"9h\"\n").unwrap();
  std::fs::write(root.join("override.toml"), "[document]\ntitle = \"Extra\"\n").unwrap();

  let deployment = Deployment { release_env: "staging".to_owned(), app_env: "local".to_owned(), region: Some("eu".to_owned()) };
  let names: Vec<String> = config_paths(&dir, &deployment).iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
  assert_eq!(names, ["app.toml", "staging.toml", "local.yaml", "eu.toml", "local-eu.toml"]);

  let config = Config::load_located(locate_with(&root, &deployment).unwrap()).unwrap();
  assert_eq!(config.document.title, "Local", "the app env overlay loads after the release env overlay");
  assert_eq!(config.session.ttl, "4h", "the env-region file loads last");
  assert_eq!(config.sources.len(), 5);

  let config = Config::load_located(locate_with(&root, &Deployment::default()).unwrap()).unwrap();
  assert_eq!(config.document.title, "Local");
  assert_eq!(config.session.ttl, "1h", "no region and no development.toml, so app.toml's value stands");

  let config = Config::load_located(locate_with(&root, &deployment).unwrap().extra("../override.toml")).unwrap();
  assert_eq!(config.document.title, "Extra", "an extra file loads last, relative to the config directory");
  assert_eq!(config.session.ttl, "4h");

  std::fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn a_cache_section_installs_the_render_memo_and_the_report_says_so() {
  let dir = app_dir();
  let base = std::fs::read_to_string(dir.join("app.toml")).unwrap();
  std::fs::write(dir.join("app.toml"), format!("{base}\n[cache]\nttl = \"5m\"\n")).unwrap();
  let transport = Arc::new(MockTransport::new().returns("shop.list", Value::Seq(vec![Value::str("a")])));
  let host = Host::from(dir.join("app.toml")).unwrap().services_over(transport).build().unwrap();
  assert!(host.report().to_string().contains("cache     1000 entries, ttl 5m"), "{}", host.report());

  host.render_to_string("/hello/norm", RenderMode::Html, SessionCell::default()).await.unwrap();
  host.render_to_string("/hello/norm", RenderMode::Html, SessionCell::default()).await.unwrap();
  host.render_to_string("/hello/ada", RenderMode::Html, SessionCell::default()).await.unwrap();
  assert_eq!(host.invalidate("routes/hello/page.tsx#default").await, 2, "one entry per distinct params");
  assert_eq!(host.invalidate("routes/hello/page.tsx#default").await, 0);

  std::fs::write(dir.join("app.toml"), format!("{base}\n[cache]\nttl = \"soon\"\n")).unwrap();
  let Err(err) = Host::from(dir.join("app.toml")).unwrap().services_over(Arc::new(MockTransport::new())).build() else { panic!("a lifetime nobody can parse must refuse to start") };
  assert!(err.to_string().contains("cache.ttl"), "{err}");
}

#[tokio::test]
async fn without_a_cache_section_nothing_is_cached() {
  let (host, _) = host();
  assert!(!host.report().to_string().contains("cache "), "{}", host.report());
  host.render_to_string("/hello/norm", RenderMode::Html, SessionCell::default()).await.unwrap();
  assert_eq!(host.invalidate("routes/hello/page.tsx#default").await, 0);
}

#[tokio::test]
async fn development_documents_carry_the_refresh_script_and_the_host_announces_changes() {
  use http_body_util::BodyExt;

  let dir = app_dir();
  let transport = Arc::new(MockTransport::new().returns("shop.list", Value::Seq(vec![Value::str("a")])));
  let host = Host::from(dir.join("app.toml")).unwrap().services_over(transport).build().unwrap();
  assert!(host.report().dev, "RELEASE_ENV is unset, so this is development");
  assert!(host.report().to_string().contains("dev       live refresh on /__fsr/events"), "{}", host.report());
  let html = host.render_to_string("/hello/norm", RenderMode::Html, SessionCell::default()).await.unwrap();
  assert!(html.contains("new EventSource(\"/__fsr/events\")"), "{html}");
  assert!(html.contains("<title>Test &lt;app&gt;</title>"), "the head is otherwise the same: {html}");

  let response = host.handle(Request::get("/__fsr/events").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers().get("content-type").unwrap(), "text/event-stream");
  let mut body = response.into_body();
  let first = body.frame().await.unwrap().unwrap().into_data().unwrap();
  assert_eq!(&first[..], b"data: {\"bundle\":\"-\"}\n\n", "no bundle under this app, so its id is `-`");
  assert!(html.contains("var b=\"-\""), "the document carries the id it was rendered against: {html}");
  let told = host.handle(Request::post("/__fsr/changed").body(Bytes::new()).unwrap()).await;
  assert_eq!(told.status(), StatusCode::NO_CONTENT);
  let next = body.frame().await.unwrap().unwrap().into_data().unwrap();
  assert_eq!(&next[..], b"data: {\"bundle\":\"-\"}\n\n");
  std::fs::create_dir_all(dir.join("dist")).unwrap();
  std::fs::write(dir.join("dist/.snapfire-build.json"), "{\"outputs\":[\"main.js\",\"main.js.map\"]}").unwrap();
  std::fs::write(dir.join("dist/main.js"), "one").unwrap();
  host.changed();
  let with_bundle = String::from_utf8(body.frame().await.unwrap().unwrap().into_data().unwrap().to_vec()).unwrap();
  assert!(with_bundle.starts_with("data: {\"bundle\":\"") && !with_bundle.contains("\"-\""), "a bundle that appeared has an id: {with_bundle}");
  std::fs::write(dir.join("dist/main.js.map"), "a map").unwrap();
  host.changed();
  let same = String::from_utf8(body.frame().await.unwrap().unwrap().into_data().unwrap().to_vec()).unwrap();
  assert_eq!(same, with_bundle, "a source map is not part of the id");
  std::fs::write(dir.join("dist/main.js"), "two").unwrap();
  host.changed();
  let edited = String::from_utf8(body.frame().await.unwrap().unwrap().into_data().unwrap().to_vec()).unwrap();
  assert_ne!(edited, with_bundle, "an edited module changes the id");
  let served = host.handle(Request::get("/static/app.js").body(Bytes::new()).unwrap()).await;
  assert_eq!(served.headers().get("cache-control").unwrap(), "no-cache", "statics revalidate in development");
}

#[tokio::test]
async fn dev_off_in_the_configuration_drops_the_script_and_the_endpoints() {
  let dir = app_dir();
  let base = std::fs::read_to_string(dir.join("app.toml")).unwrap();
  std::fs::write(dir.join("app.toml"), base.replace("[server]\n", "[server]\ndev = false\n")).unwrap();
  let transport = Arc::new(MockTransport::new().returns("shop.list", Value::Seq(vec![Value::str("a")])));
  let host = Host::from(dir.join("app.toml")).unwrap().services_over(transport).build().unwrap();
  assert!(!host.report().dev);
  assert!(!host.report().to_string().contains("dev "), "{}", host.report());
  let html = host.render_to_string("/hello/norm", RenderMode::Html, SessionCell::default()).await.unwrap();
  assert!(!html.contains("EventSource"), "{html}");
  let response = host.handle(Request::get("/__fsr/events").body(Bytes::new()).unwrap()).await;
  assert_ne!(response.status(), StatusCode::OK);
  host.changed();
}

#[tokio::test]
async fn a_soft_navigation_is_intercepted_when_its_origin_shares_the_declaring_layout() {
  let (host, _) = host();
  assert!(host.intercept_for("/feed/photo/3", Some("/feed"), None).is_some(), "the feed sits under the layout declaring the slot");
  assert!(host.intercept_for("/feed/photo/3", Some("/feed/photo/2"), None).is_some(), "so does another photo");
  assert!(host.intercept_for("/feed/photo/3", Some("/"), None).is_none(), "the index has no such layout");
  assert!(host.intercept_for("/feed/photo/3", None, None).is_none(), "no origin is a document");
  assert!(host.intercept_for("/feed/photo/3", None, Some("modal")).is_some(), "`into` names the slot outright");
  assert!(host.intercept_for("/feed/photo/3", Some("/feed"), Some("panel")).is_none(), "a slot the route has no variant for");
  let (drawer, _) = host.intercept_for("/feed/photo/3", None, Some("drawer")).unwrap();
  assert_eq!(drawer.children[0].1.children[0].0 .0, "drawer", "`into` picks among a route's variants");
  let (first, _) = host.intercept_for("/feed/photo/3", Some("/feed"), None).unwrap();
  assert_eq!(first.children[0].1.children[0].0 .0, "modal", "without `into`, the first variant whose layout the origin shares");
  assert!(host.intercept_for("/feed", Some("/feed"), None).is_none(), "a route without a variant");

  let (peek, _) = host.intercept_for("/deck/card/3", None, Some("peek")).expect("a slot a nested layout declares");
  assert_eq!(peek.children[0].1.children[0].1.children[0].0 .0, "peek");
  assert!(host.intercept_for("/deck/card/3", Some("/deck"), None).is_some(), "the origin shares both layouts down to the declaring one");
  assert!(host.intercept_for("/deck/card/3", Some("/"), None).is_none(), "the index shares neither");
  assert!(host.intercept_for("/deck/card/3", None, Some("content")).is_none(), "the layout on the way down is not the declaring one");

  let payload = host.render_navigation_to_string("/feed/photo/3", Some("/feed"), None, SessionCell::default()).await.unwrap();
  let sidecar = payload.lines().find(|l| l.starts_with("G ")).unwrap();
  assert!(sidecar.contains("\"keep\":[\"content\",\"drawer\"]"), "{sidecar}");
  let plain = host.render_navigation_to_string("/feed/photo/3", Some("/"), None, SessionCell::default()).await.unwrap();
  assert!(!plain.contains("keep"), "{plain}");

  let response = host.handle(Request::get("/feed/photo/3?__payload").header("x-sf-from", "/feed?x=1").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.status(), StatusCode::OK);
  let body = String::from_utf8(response.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
  assert!(body.contains("\"keep\":[\"content\",\"drawer\"]"), "the edge reads the origin header: {body}");
  let response = host.handle(Request::get("/feed/photo/3?__payload").body(Bytes::new()).unwrap()).await;
  let body = String::from_utf8(response.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
  assert!(!body.contains("keep"), "without it the payload is the page's: {body}");
  let response = host.handle(Request::get("/feed/photo/3").header("x-sf-from", "/feed").body(Bytes::new()).unwrap()).await;
  let body = String::from_utf8(response.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
  assert!(body.contains("<!doctype html>") && !body.contains("page.modal"), "a document is never intercepted: {body}");
}

fn localised_dir() -> PathBuf {
  let dir = app_dir();
  std::fs::write(
    dir.join("app.toml"),
    r#"
[app]
dir = "."

[server]
listen = "127.0.0.1:0"

[document]
title = "Test <app>"

[session]
key = "test-key"
ttl = "10m"

[locales]
supported = ["en_US", "fr_FR", "de"]
default = "en_US"
remember = true

[[static]]
route = "/static"
dir = "public"
"#,
  )
  .unwrap();
  dir
}

fn localised() -> Arc<Host> {
  let transport = Arc::new(MockTransport::new().returns("shop.list", Value::Seq(vec![Value::str("a")])));
  let host = Host::from(localised_dir().join("app.toml"))
    .unwrap()
    .services_over(transport)
    .middleware(|ctx, input| async move {
      let path = match &input {
        Value::Map(map) => map.get("path").cloned().unwrap_or(Value::Null),
        _ => Value::Null,
      };
      let mut headers = ValueMap::new();
      headers.insert("x-locale".to_owned(), Value::Str(ctx.locale.tag.clone()));
      headers.insert("x-path".to_owned(), path);
      let mut out = ValueMap::new();
      out.insert("headers".to_owned(), Value::Map(headers));
      Ok(Value::Map(out))
    })
    .build()
    .unwrap();
  Arc::new(host)
}

async fn body_of(response: http::Response<snapfire_fsr_host::Body>) -> String {
  String::from_utf8(response.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap()
}

#[tokio::test]
async fn a_locale_prefix_is_stripped_and_marks_the_document_and_every_segment() {
  let host = localised();
  assert_eq!(host.report().locales, vec!["en_US".to_owned(), "fr_FR".to_owned(), "de".to_owned()]);
  assert!(host.report().to_string().contains("locales   en_US (default, unprefixed), fr_FR, de"), "{}", host.report());

  let html = host.render_to_string("/fr_FR/hello/norm?from=test", RenderMode::Html, SessionCell::default()).await.unwrap();
  assert!(html.contains("<html lang=\"fr-FR\" data-sf-locale=\"fr_FR\">"), "{html}");
  assert!(html.contains("hi norm via test"), "the route matched without its prefix: {html}");
  assert!(html.contains("<!--sf-g:shell#document@fr_FR-->"), "{html}");
  assert!(html.contains("<!--sf-g:routes/hello/page.tsx#default?name=norm&from=test@fr_FR-->"), "{html}");
  assert!(!html.contains("rel=\"canonical\""), "{html}");

  let html = host.render_to_string("/hello/norm?from=test", RenderMode::Html, SessionCell::default()).await.unwrap();
  assert!(html.contains("<html lang=\"en-US\" data-sf-locale=\"en_US\">"), "{html}");
  assert!(html.contains("<!--sf-g:shell#document-->"), "the default locale leaves the key bare: {html}");

  let html = host.render_to_string("/en-us/hello/norm?from=test", RenderMode::Html, SessionCell::default()).await.unwrap();
  assert!(html.contains("<html lang=\"en-US\" data-sf-locale=\"en_US\">"), "the prefix matches whatever its spelling: {html}");
  assert!(html.contains("<link rel=\"canonical\" href=\"/hello/norm\">"), "a prefixed default locale points at the bare path: {html}");
  assert!(html.contains("hi norm via test"), "{html}");

  let payload = host.render_to_string("/de/hello/norm", RenderMode::Payload, SessionCell::default()).await.unwrap();
  assert!(payload.contains("\nL \"de\"\n"), "{payload}");

  assert!(matches!(host.render_to_string("/ja/hello/norm", RenderMode::Html, SessionCell::default()).await, Err(snapfire_fsr_host::HostError::NotFound(_))), "an unsupported prefix is a path");
}

#[tokio::test]
async fn the_edge_takes_the_locale_from_the_prefix_the_cookie_then_the_header_and_remembers_a_prefix() {
  let host = localised();

  let response = host.handle(Request::get("/fr_FR/hello/x?from=y").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers().get("x-locale").unwrap(), "fr_FR", "the middleware reads the locale");
  assert_eq!(response.headers().get("x-path").unwrap(), "/hello/x", "and the stripped path");
  let cookies: Vec<String> = response.headers().get_all(header::SET_COOKIE).iter().map(|v| v.to_str().unwrap().to_owned()).collect();
  assert!(cookies.iter().any(|c| c.starts_with("sf_locale=fr_FR; Path=/; Max-Age=")), "the prefix is remembered: {cookies:?}");
  let html = body_of(response).await;
  assert!(html.contains("lang=\"fr-FR\""), "{html}");

  let response = host.handle(Request::get("/hello/x?from=y").header(header::COOKIE, "sf_locale=fr_FR").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.headers().get("x-locale").unwrap(), "fr_FR", "an unprefixed path follows the cookie");
  assert!(!response.headers().get_all(header::SET_COOKIE).iter().any(|v| v.to_str().unwrap().starts_with("sf_locale=")), "nothing to remember");

  let response = host.handle(Request::get("/hello/x?from=y").header(header::ACCEPT_LANGUAGE, "ja, de-AT;q=0.8, fr;q=0.5").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.headers().get("x-locale").unwrap(), "de", "the header's best supported language");

  let response = host.handle(Request::get("/hello/x?from=y").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.headers().get("x-locale").unwrap(), "en_US", "nothing says: the default");

  let response = host.handle(Request::get("/fr_FR/hello/x?from=y").header(header::COOKIE, "sf_locale=fr_FR").body(Bytes::new()).unwrap()).await;
  assert!(!response.headers().get_all(header::SET_COOKIE).iter().any(|v| v.to_str().unwrap().starts_with("sf_locale=")), "the cookie already holds the prefix's locale");

  let response = host.handle(Request::get("/fr_FR/static/app.js").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.status(), StatusCode::NOT_FOUND, "static roots are never prefixed");
  let response = host.handle(Request::post("/fr_FR/_sf/action/index.where").body(Bytes::from("{}")).unwrap()).await;
  assert_eq!(response.status(), StatusCode::NOT_FOUND, "nor is the action route");
}

#[tokio::test]
async fn an_action_runs_in_the_locale_of_the_document_that_called_it() {
  let host = localised();
  let response = host.handle(Request::post("/_sf/action/index.where").header("x-sf-from", "/fr_FR/hello/x?from=y").body(Bytes::from("{}")).unwrap()).await;
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(body_of(response).await, "\"fr_FR\"", "the document's prefix");

  let response = host.handle(Request::post("/_sf/action/index.where").header("x-sf-from", "/hello/x").header(header::COOKIE, "sf_locale=de").body(Bytes::from("{}")).unwrap()).await;
  assert_eq!(body_of(response).await, "\"de\"", "an unprefixed document's cookie");

  let response = host.handle(Request::post("/_sf/action/index.where").body(Bytes::from("{}")).unwrap()).await;
  assert!(!response.headers().get_all(header::SET_COOKIE).iter().any(|v| v.to_str().unwrap().starts_with("sf_locale=")), "an action never writes the locale cookie");
  assert_eq!(body_of(response).await, "\"en_US\"", "nothing says: the default");

  let value = host.call_action_in("index.where", SessionCell::default(), host.locales().locale("fr_FR"), Value::Map(ValueMap::new())).await.unwrap();
  assert_eq!(value, Value::str("fr_FR"));
  let value = host.call_action("index.where", SessionCell::default(), Value::Map(ValueMap::new())).await.unwrap();
  assert_eq!(value, Value::str("en_US"));
}

#[tokio::test]
async fn prerender_writes_every_locale_and_the_edge_serves_each_from_its_own_directory() {
  let out = std::env::temp_dir().join(format!("fsr-host-prerender-{}-{}", std::process::id(), rand_suffix()));
  let transport = Arc::new(MockTransport::new().returns("shop.list", Value::Seq(vec![Value::str("a")])));
  let host = Host::from(localised_dir().join("app.toml")).unwrap().services_over(transport).prerendered(&out).build().unwrap();
  assert!(host.prerenderable().contains(&"/".to_owned()), "{}", host.report());

  let written = host.prerender(&out).await.unwrap();
  let served: Vec<&str> = written.iter().map(|(p, _)| p.as_str()).collect();
  for path in ["/", "/fr_FR", "/de"] {
    assert_eq!(served.iter().filter(|p| **p == path).count(), 2, "a document and a payload for {path}: {served:?}");
  }
  assert!(out.join("index.html").is_file());
  assert!(out.join("fr_FR/index.html").is_file());
  assert!(out.join("de/index.payload").is_file());

  assert!(host.prerendered("/", RenderMode::Html).unwrap().contains("lang=\"en-US\""));
  assert!(host.prerendered("/fr_FR", RenderMode::Html).unwrap().contains("lang=\"fr-FR\""));
  assert!(host.prerendered("/fr-fr/", RenderMode::Html).unwrap().contains("lang=\"fr-FR\""));
  assert!(host.prerendered("/de", RenderMode::Payload).unwrap().contains("\nL \"de\"\n"));
  assert_eq!(host.prerendered("/ja", RenderMode::Html), None);

  let response = host.handle(Request::get("/fr_FR").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.headers().get("x-sf-prerendered").unwrap(), "1");
  assert!(body_of(response).await.contains("lang=\"fr-FR\""));
  let response = host.handle(Request::get("/").header(header::COOKIE, "sf_locale=de").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.headers().get("x-sf-prerendered").unwrap(), "1");
  assert!(body_of(response).await.contains("lang=\"de\""), "the cookie picks the prerendered locale");
  std::fs::remove_dir_all(&out).unwrap();
}

#[test]
fn a_bad_locales_section_refuses_to_start() {
  let dir = app_dir();
  std::fs::write(
    dir.join("app.toml"),
    "[app]\ndir = \".\"\n[session]\nkey = \"k\"\n[locales]\nsupported = [\"en\"]\ndefault = \"fr\"\n",
  )
  .unwrap();
  let err = match Host::from(dir.join("app.toml")).unwrap().build() {
    Ok(_) => panic!("a default outside the supported locales started"),
    Err(e) => e.to_string(),
  };
  assert!(err.contains("locales.default `fr` is not among locales.supported"), "{err}");
}

const WHO_PLAN: &str = r#"{
  "version": 2,
  "routes": [
    { "pattern": "/", "plan": { "id": 0, "module": "shell#document", "children": [
      { "slot": "content", "node": { "id": 1, "module": "routes/index/page.tsx#default" } } ] } },
    { "pattern": "/login", "plan": { "id": 0, "module": "shell#document", "children": [
      { "slot": "content", "node": { "id": 1, "module": "routes/login/page.tsx#default" } } ] } },
    { "pattern": "/who", "plan": { "id": 0, "module": "shell#document", "children": [
      { "slot": "content", "node": { "id": 1, "module": "routes/who/page.tsx#default", "source": "who" } } ] } }
  ],
  "sources": [
    { "id": "who", "owner": "lowered", "module": "routes/who/page.loader.ts",
      "body": [ { "return": { "object": [
        { "field": [ "subject", { "coalesce": [ { "identity": [ "subject" ] }, { "lit": { "str": "anonymous" } } ] } ] },
        { "field": [ "items", { "call": { "service": "shop", "method": "list", "args": [] } } ] } ] } } ] }
  ],
  "actions": []
}"#;

const SHOP_OPENAPI: &str = r#"{
  "openapi": "3.0.3",
  "info": { "title": "Shop", "version": "1.0.0" },
  "paths": { "/list": { "get": { "operationId": "list", "responses": { "200": { "description": "the items",
    "content": { "application/json": { "schema": { "type": "array", "items": { "type": "string" } } } } } } } } }
}"#;

fn identified_dir(users: &str) -> PathBuf {
  let dir = app_dir();
  std::fs::create_dir_all(dir.join("clients")).unwrap();
  std::fs::write(dir.join("generated/plan.json"), WHO_PLAN).unwrap();
  std::fs::write(dir.join("clients/shop.openapi.json"), SHOP_OPENAPI).unwrap();
  std::fs::write(dir.join("auth.toml"), users).unwrap();
  std::fs::write(
    dir.join("app.toml"),
    r#"
[app]
dir = "."

[server]
listen = "127.0.0.1:0"

[document]
title = "Test <app>"

[session]
key = "test-key"
ttl = "10m"

[locales]
supported = ["en_US", "fr_FR"]

[auth]
provider = "file"
login = "/login"

[clients.shop]
base_url = "http://127.0.0.1:1"
bearer = true
"#,
  )
  .unwrap();
  dir
}

const USERS: &str = r#"
[[users]]
name = "alice"
password = "wonder"
claims = { role = "admin" }

[[users]]
name = "bob"
password = "builder"
"#;

fn identified() -> (Arc<Host>, Arc<MockTransport>) {
  let transport = Arc::new(MockTransport::new().returns("shop.list", Value::Seq(vec![Value::str("a")])));
  let host = Host::from(identified_dir(USERS).join("app.toml")).unwrap().services_over(transport.clone()).build().unwrap();
  (Arc::new(host), transport)
}

fn location(response: &http::Response<snapfire_fsr_host::Body>) -> String {
  response.headers().get(header::LOCATION).expect("a location").to_str().unwrap().to_owned()
}

fn cookie_of(response: &http::Response<snapfire_fsr_host::Body>) -> String {
  let set = response.headers().get_all(header::SET_COOKIE).iter().find(|v| v.to_str().unwrap().starts_with("sf_session=")).expect("a session cookie");
  set.to_str().unwrap().split(';').next().unwrap().to_owned()
}

fn field(text: &str, name: &str) -> Option<String> {
  let start = text.find(&format!("\"{name}\":\"")).map(|i| i + name.len() + 4)?;
  let end = text[start..].find('"').map(|i| start + i)?;
  Some(text[start..end].to_owned())
}

#[tokio::test]
async fn the_login_flow_signs_a_session_in_through_the_file_provider() {
  let (host, transport) = identified();
  assert_eq!(host.report().auth, Some(("file".to_owned(), "/login".to_owned())));
  assert_eq!(host.report().bearer, vec![("shop".to_owned(), "access_token".to_owned())]);
  let report = host.report().to_string();
  assert!(report.contains("auth      file, login page /login"), "{report}");
  assert!(report.contains("bearer    shop                   access_token"), "{report}");

  let response = host.handle(Request::get("/who").body(Bytes::new()).unwrap()).await;
  let html = body_of(response).await;
  assert!(html.contains("anonymous"), "{html}");
  assert!(!html.contains("csrf_token"), "no token before sign-in: {html}");
  assert_eq!(transport.last_metadata("authorization"), None, "an anonymous call carries no bearer");

  let response = host.handle(Request::get("/auth/login?return_to=/who").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.status(), StatusCode::SEE_OTHER);
  assert_eq!(location(&response), "/login?return_to=%2Fwho");
  let cookie = cookie_of(&response);

  let response = host.handle(Request::get("/login?return_to=%2Fwho").header(header::COOKIE, &cookie).body(Bytes::new()).unwrap()).await;
  assert_eq!(response.status(), StatusCode::OK, "the login page is the application's route");

  let response = host
    .handle(
      Request::post("/auth/callback")
        .header(header::COOKIE, &cookie)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Bytes::from("user=alice&password=wonder"))
        .unwrap(),
    )
    .await;
  assert_eq!(response.status(), StatusCode::SEE_OTHER);
  assert_eq!(location(&response), "/who");

  let response = host.handle(Request::get("/who").header(header::COOKIE, &cookie).body(Bytes::new()).unwrap()).await;
  let html = body_of(response).await;
  assert!(html.contains("alice"), "the loader read the identity: {html}");
  assert_eq!(transport.last_metadata("authorization").as_deref(), Some("Bearer dev-token-alice"), "the loader's call carried the bearer");
  let token = field(&html, "csrf_token").expect("the token is a prop once signed in");
  assert!(!html.contains("dev-token-alice"), "custody never renders: {html}");

  let response = host
    .handle(Request::post("/auth/logout").header(header::COOKIE, &cookie).header(header::CONTENT_TYPE, "application/x-www-form-urlencoded").body(Bytes::from("_csrf=nope")).unwrap())
    .await;
  assert_eq!(response.status(), StatusCode::FORBIDDEN);

  let response = host
    .handle(
      Request::post("/auth/logout")
        .header(header::COOKIE, &cookie)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Bytes::from(format!("_csrf={token}")))
        .unwrap(),
    )
    .await;
  assert_eq!(response.status(), StatusCode::SEE_OTHER);
  assert_eq!(location(&response), "/");
  let expiring = response.headers().get(header::SET_COOKIE).unwrap().to_str().unwrap();
  assert!(expiring.starts_with("sf_session=;") && expiring.contains("Max-Age=0"), "{expiring}");

  let response = host.handle(Request::get("/who").header(header::COOKIE, &cookie).body(Bytes::new()).unwrap()).await;
  let html = body_of(response).await;
  assert!(html.contains("anonymous"), "signed out: {html}");
}

#[tokio::test]
async fn a_wrong_password_returns_to_the_login_page_and_a_replay_is_invalid() {
  let (host, _) = identified();
  let response = host.handle(Request::get("/auth/login?return_to=/who").body(Bytes::new()).unwrap()).await;
  let cookie = cookie_of(&response);
  let callback = |body: &str| Request::post("/auth/callback").header(header::COOKIE, &cookie).header(header::CONTENT_TYPE, "application/x-www-form-urlencoded").body(Bytes::from(body.to_owned())).unwrap();

  let response = host.handle(callback("user=alice&password=nope")).await;
  assert_eq!(response.status(), StatusCode::SEE_OTHER);
  assert_eq!(location(&response), "/login?error=denied&return_to=%2Fwho");

  let response = host.handle(callback("user=alice&password=wonder")).await;
  assert_eq!(response.status(), StatusCode::BAD_REQUEST, "the flow was consumed");

  let response = host.handle(Request::get("/login").header(header::COOKIE, &cookie).header(header::REFERER, "http://localhost/who?x=1").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.status(), StatusCode::OK);
  let response = host.handle(callback("user=alice&password=wonder")).await;
  assert_eq!(location(&response), "/who?x=1", "the login page reseeded the flow from the referer");

  let response = host.handle(Request::get("/auth/callback?user=bob&password=builder").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.status(), StatusCode::BAD_REQUEST, "a callback with no flow in progress");
}

#[tokio::test]
async fn return_to_never_leaves_the_origin_and_the_routes_are_never_prefixed() {
  let (host, _) = identified();
  for bad in ["https://evil.example/x", "//evil.example", "evil"] {
    let response = host.handle(Request::get(format!("/auth/login?return_to={bad}")).body(Bytes::new()).unwrap()).await;
    assert_eq!(location(&response), "/login?return_to=%2F", "{bad}");
  }
  let response = host.handle(Request::get("/auth/login").header(header::REFERER, "http://localhost/who").body(Bytes::new()).unwrap()).await;
  assert_eq!(location(&response), "/login?return_to=%2Fwho");

  let response = host.handle(Request::get("/fr_FR/auth/login").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.status(), StatusCode::NOT_FOUND);

  let (plain, _) = self::host();
  let response = plain.handle(Request::get("/auth/login").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.status(), StatusCode::NOT_FOUND, "no section, no routes");
}

#[test]
fn the_auth_section_and_the_users_file_are_checked() {
  let dir = identified_dir("");
  let err = Host::from(dir.join("app.toml")).unwrap().build().err().expect("an empty table is an error").to_string();
  assert!(err.contains("no [[users]] row"), "{err}");

  let dir = identified_dir(USERS);
  std::fs::write(dir.join("app.toml"), std::fs::read_to_string(dir.join("app.toml")).unwrap().replace("provider = \"file\"", "provider = \"oidc\"")).unwrap();
  let err = Host::from(dir.join("app.toml")).err().expect("an unknown provider is an error").to_string();
  assert!(err.contains("auth.provider `oidc` is not a provider"), "{err}");
}

fn formed_dir() -> PathBuf {
  let dir = app_dir();
  let toml = std::fs::read_to_string(dir.join("app.toml")).unwrap().replace("ttl = \"10m\"", "ttl = \"10m\"\ncsrf = \"always\"");
  std::fs::write(dir.join("app.toml"), toml).unwrap();
  dir
}

struct ItemsTitle;

impl snapfire_fsr_runtime::Metadata for ItemsTitle {
  fn describe(&self, ctx: &snapfire_fsr_runtime::RequestCtx, data: &snapfire_fsr_core::Data) -> futures::future::BoxFuture<'static, Result<snapfire_fsr_runtime::Meta, snapfire_fsr_runtime::LoadError>> {
    let count = match data.get("items") {
      Some(Value::Seq(items)) => items.len(),
      _ => 0,
    };
    let who = ctx.session.identity().map(|i| i.subject).unwrap_or_else(|| "nobody".to_owned());
    Box::pin(async move { Ok(snapfire_fsr_runtime::Meta { title: Some(format!("{count} items for {who}")), description: None, head: Vec::new() }) })
  }
}

fn formed() -> Arc<Host> {
  let transport = Arc::new(MockTransport::new().returns("shop.list", Value::Seq(vec![Value::str("a"), Value::str("b")])));
  let host = Host::from(formed_dir().join("app.toml"))
    .unwrap()
    .services_over(transport)
    .action("remember", |ctx, input| async move {
      let word = match &input {
        Value::Map(map) => map.get("word").cloned().unwrap_or(Value::Null),
        _ => Value::Null,
      };
      ctx.session.insert("word", word.clone());
      Ok(word)
    })
    .action("recall", |ctx, _input| async move { Ok(ctx.session.get("word").unwrap_or(Value::Null)) })
    .meta("index", Arc::new(ItemsTitle))
    .build()
    .unwrap();
  Arc::new(host)
}

#[tokio::test]
async fn a_form_posts_an_action_with_its_token_and_lands_back_on_the_referer() {
  let host = formed();
  let response = host.handle(Request::get("/").body(Bytes::new()).unwrap()).await;
  let cookie = cookie_of(&response);
  let html = body_of(response).await;
  let token = field(&html, "csrf_token").expect("csrf = always mints a token for an anonymous session");
  assert!(!cookie.is_empty(), "and establishes the session so the token verifies next time");
  assert!(html.contains("<title>2 items for nobody</title>"), "the Rust metadata titled the document: {html}");

  let form = |body: String| {
    Request::post("/_sf/action/remember")
      .header(header::COOKIE, &cookie)
      .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
      .header(header::REFERER, "http://localhost/hello/norm?from=form")
      .body(Bytes::from(body))
      .unwrap()
  };
  let response = host.handle(form("word=hi&_csrf=nope".to_owned())).await;
  assert_eq!(response.status(), StatusCode::FORBIDDEN);

  let response = host.handle(form(format!("word=hi&_csrf={token}"))).await;
  assert_eq!(response.status(), StatusCode::SEE_OTHER);
  assert_eq!(location(&response), "/hello/norm?from=form");

  let response = host
    .handle(Request::post("/_sf/action/recall").header(header::COOKIE, &cookie).header(header::CONTENT_TYPE, "application/json").body(Bytes::from("null")).unwrap())
    .await;
  assert_eq!(body_of(response).await, "\"hi\"", "the form's field reached the action as a string and the session kept it");

  let response = host.handle(form(format!("word=again&_csrf={token}"))).await;
  assert_eq!(response.status(), StatusCode::SEE_OTHER, "the token is good for the session's life");
}

#[tokio::test]
async fn a_payload_request_may_only_name_an_encoding_that_exists() {
  let host = formed();
  let response = host.handle(Request::get("/?__payload&enc=cbor").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.status(), StatusCode::NOT_ACCEPTABLE);
  assert!(body_of(response).await.contains("unsupported payload encoding `cbor`"));
  let response = host.handle(Request::get("/?__payload&enc=json").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.status(), StatusCode::OK);
  assert!(body_of(response).await.starts_with("V {\"fmt\":"));
  let response = host.handle(Request::get("/?enc=cbor").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.status(), StatusCode::OK, "a document request ignores enc");

  let plain = self::host().0;
  let response = plain.handle(Request::get("/").body(Bytes::new()).unwrap()).await;
  assert!(!body_of(response).await.contains("csrf_token"), "csrf = identified is the default");
}

fn mocked_dir(responses: &str, transport: &str) -> PathBuf {
  let dir = app_dir();
  std::fs::create_dir_all(dir.join("clients")).unwrap();
  std::fs::write(dir.join("generated/plan.json"), WHO_PLAN).unwrap();
  std::fs::write(dir.join("clients/shop.openapi.json"), SHOP_OPENAPI).unwrap();
  std::fs::write(dir.join("clients/shop.mock.json"), responses).unwrap();
  std::fs::write(
    dir.join("app.toml"),
    format!(
      r#"
[app]
dir = "."

[server]
listen = "127.0.0.1:0"

[document]
title = "Mocked"

[session]
key = "test-key"

[clients.shop]
transport = "{transport}"
"#
    ),
  )
  .unwrap();
  dir
}

#[tokio::test]
async fn a_mock_client_answers_from_its_responses_file_and_reaches_nothing() {
  let host = Host::from(mocked_dir(r#"{"list": ["socks", "hat"]}"#, "mock").join("app.toml")).unwrap().build().unwrap();
  assert_eq!(host.report().services, vec![("shop".to_owned(), "mock".to_owned(), "clients/shop.mock.json".to_owned())]);
  let report = host.report().to_string();
  assert!(report.contains("services  shop                   mock        clients/shop.mock.json"), "{report}");

  let response = host.handle(Request::get("/who").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.status(), StatusCode::OK);
  let html = body_of(response).await;
  assert!(html.contains("socks") && html.contains("hat"), "the loader's call answered from the file: {html}");
}

#[tokio::test]
async fn a_mock_response_can_be_a_failure_and_the_transport_name_is_checked() {
  let host = Host::from(mocked_dir(r#"{"list": {"$fail": {"kind": "unavailable", "message": "the shop is closed"}}}"#, "mock").join("app.toml")).unwrap().build().unwrap();
  let response = host.handle(Request::get("/who").body(Bytes::new()).unwrap()).await;
  let html = body_of(response).await;
  assert!(html.contains("the shop is closed"), "the failure reached the render: {html}");

  let error = Host::from(mocked_dir("{}", "smtp").join("app.toml")).err().expect("an unknown transport is refused").to_string();
  assert!(error.contains("clients.shop.transport") && error.contains("smtp"), "{error}");
}

const IDENTITY_OPENAPI: &str = r##"{
  "openapi": "3.0.3",
  "info": { "title": "Identity", "version": "1.0.0" },
  "paths": {
    "/authenticate": { "post": { "operationId": "authenticate",
      "requestBody": { "required": true, "content": { "application/json": { "schema": { "$ref": "#/components/schemas/Credentials" } } } },
      "responses": { "200": { "description": "signed", "content": { "application/json": { "schema": { "$ref": "#/components/schemas/Signed" } } } } } } },
    "/sessions/{id}": {
      "get": { "operationId": "getSession", "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "type": "string" } }],
        "responses": { "200": { "description": "stored", "content": { "application/json": { "schema": { "$ref": "#/components/schemas/Stored" } } } } } },
      "put": { "operationId": "putSession", "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "type": "string" } }],
        "requestBody": { "required": true, "content": { "application/json": { "schema": { "$ref": "#/components/schemas/Stored" } } } },
        "responses": { "204": { "description": "stored" } } },
      "delete": { "operationId": "deleteSession", "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "type": "string" } }],
        "responses": { "204": { "description": "forgotten" } } }
    }
  },
  "components": { "schemas": {
    "Credentials": { "type": "object", "required": ["user", "password"], "properties": { "user": { "type": "string" }, "password": { "type": "string" } } },
    "Signed": { "type": "object", "required": ["subject", "claims", "access_token"], "properties": { "subject": { "type": "string" }, "claims": { "type": "object", "additionalProperties": { "type": "string" } }, "access_token": { "type": "string" } } },
    "Stored": { "type": "object", "required": ["record"], "properties": { "record": { "type": "string" } } }
  } }
}"##;

/// The identity service as a transport with state: accounts and the sessions
/// the host hands it, plus the last bearer the shop saw.
#[derive(Default)]
struct IdentityService {
  sessions: parking_lot::Mutex<std::collections::HashMap<String, String>>,
  shop_bearer: parking_lot::Mutex<Option<String>>,
}

fn str_arg(args: &ValueMap, key: &str) -> String {
  match args.get(key) {
    Some(Value::Str(s)) => s.clone(),
    other => panic!("{key} is not a string: {other:?}"),
  }
}

impl snapfire_fsr_service::Transport for IdentityService {
  fn call(&self, call: snapfire_fsr_service::Call) -> futures::future::BoxFuture<'static, Result<Value, snapfire_fsr_runtime::ServiceError>> {
    use snapfire_fsr_runtime::{FailureKind, ServiceError};
    let path = format!("{}.{}", call.service, call.method);
    let result = match path.as_str() {
      "shop.list" => {
        *self.shop_bearer.lock() = match call.metadata.get("authorization") {
          Some(Value::Str(s)) => Some(s.clone()),
          _ => None,
        };
        Ok(Value::Seq(vec![Value::str("a")]))
      }
      "identity.authenticate" => {
        if str_arg(&call.args, "user") == "alice" && str_arg(&call.args, "password") == "wonder" {
          let mut claims = ValueMap::new();
          claims.insert("role".to_owned(), Value::str("admin"));
          let mut signed = ValueMap::new();
          signed.insert("subject".to_owned(), Value::str("alice"));
          signed.insert("claims".to_owned(), Value::Map(claims));
          signed.insert("access_token".to_owned(), Value::str("svc-token-alice"));
          Ok(Value::Map(signed))
        } else {
          Err(ServiceError::new(FailureKind::Unauthorized, "identity", "authenticate", "unknown user or wrong password"))
        }
      }
      "identity.getSession" => match self.sessions.lock().get(&str_arg(&call.args, "id")) {
        Some(record) => {
          let mut stored = ValueMap::new();
          stored.insert("record".to_owned(), Value::Str(record.clone()));
          Ok(Value::Map(stored))
        }
        None => Err(ServiceError::new(FailureKind::NotFound, "identity", "getSession", "no such session")),
      },
      "identity.putSession" => {
        self.sessions.lock().insert(str_arg(&call.args, "id"), str_arg(&call.args, "record"));
        Ok(Value::Null)
      }
      "identity.deleteSession" => {
        self.sessions.lock().remove(&str_arg(&call.args, "id"));
        Ok(Value::Null)
      }
      other => Err(ServiceError::new(FailureKind::Internal, call.service.clone(), call.method.clone(), format!("unexpected {other}"))),
    };
    Box::pin(futures::future::ready(result))
  }
}

fn remote_dir() -> PathBuf {
  let dir = app_dir();
  std::fs::create_dir_all(dir.join("clients")).unwrap();
  std::fs::write(dir.join("generated/plan.json"), WHO_PLAN).unwrap();
  std::fs::write(dir.join("clients/shop.openapi.json"), SHOP_OPENAPI).unwrap();
  std::fs::write(dir.join("clients/identity.openapi.json"), IDENTITY_OPENAPI).unwrap();
  std::fs::write(
    dir.join("app.toml"),
    r#"
[app]
dir = "."

[server]
listen = "127.0.0.1:0"

[document]
title = "Remote"

[session]
key = "test-key"
store = "service"
client = "identity"

[auth]
provider = "service"
client = "identity"
login = "/login"

[clients.shop]
base_url = "http://127.0.0.1:1"
bearer = true

[clients.identity]
base_url = "http://127.0.0.1:1"
"#,
  )
  .unwrap();
  dir
}

#[tokio::test]
async fn sessions_and_sign_in_live_behind_the_identity_client() {
  let identity = Arc::new(IdentityService::default());
  let host = Host::from(remote_dir().join("app.toml")).unwrap().services_over(identity.clone()).build().unwrap();
  let report = host.report().to_string();
  assert!(report.contains("session   service via identity"), "{report}");
  assert!(report.contains("auth      service via identity, login page /login"), "{report}");

  let response = host.handle(Request::get("/auth/login?return_to=/who").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.status(), StatusCode::SEE_OTHER);
  let cookie = cookie_of(&response);
  assert_eq!(identity.sessions.lock().len(), 1, "the flow state was stored through putSession");

  let response = host
    .handle(Request::post("/auth/callback").header(header::COOKIE, &cookie).header(header::CONTENT_TYPE, "application/x-www-form-urlencoded").body(Bytes::from("user=alice&password=nope")).unwrap())
    .await;
  assert_eq!(location(&response), "/login?error=denied&return_to=%2Fwho", "the service's 401 is a denial");

  let response = host.handle(Request::get("/login?return_to=%2Fwho").header(header::COOKIE, &cookie).body(Bytes::new()).unwrap()).await;
  assert_eq!(response.status(), StatusCode::OK);
  let response = host
    .handle(Request::post("/auth/callback").header(header::COOKIE, &cookie).header(header::CONTENT_TYPE, "application/x-www-form-urlencoded").body(Bytes::from("user=alice&password=wonder")).unwrap())
    .await;
  assert_eq!(location(&response), "/who");
  let stored = identity.sessions.lock().values().next().cloned().expect("the session is in the service");
  assert!(stored.contains("\"alice\"") && stored.contains("svc-token-alice"), "identity and custody travel in the record: {stored}");

  let response = host.handle(Request::get("/who").header(header::COOKIE, &cookie).body(Bytes::new()).unwrap()).await;
  let html = body_of(response).await;
  assert!(html.contains("alice"), "the loader read the identity the service holds: {html}");
  assert_eq!(identity.shop_bearer.lock().as_deref(), Some("Bearer svc-token-alice"), "the token the service issued rides the shop call");
  let token = field(&html, "csrf_token").expect("a token once signed in");

  let response = host
    .handle(Request::post("/auth/logout").header(header::COOKIE, &cookie).header(header::CONTENT_TYPE, "application/x-www-form-urlencoded").body(Bytes::from(format!("_csrf={token}"))).unwrap())
    .await;
  assert_eq!(response.status(), StatusCode::SEE_OTHER);
  assert_eq!(identity.sessions.lock().len(), 0, "logout deleted the session in the service");
}

#[test]
fn a_service_store_or_provider_needs_a_client_that_exists() {
  let dir = remote_dir();
  let text = std::fs::read_to_string(dir.join("app.toml")).unwrap().replace("client = \"identity\"\n\n[auth]", "client = \"vault\"\n\n[auth]");
  std::fs::write(dir.join("app.toml"), text).unwrap();
  let error = Host::from(dir.join("app.toml")).err().expect("a client that is not declared is refused").to_string();
  assert!(error.contains("session.client") && error.contains("vault"), "{error}");
}

const CACHED_OPENAPI: &str = r##"{
  "openapi": "3.0.3",
  "info": { "title": "Shop", "version": "1.0.0" },
  "paths": {
    "/list": { "get": { "operationId": "list", "x-sf-cache": { "ttl": "1m", "tags": ["items"], "scope": "shared" },
      "responses": { "200": { "description": "the items", "content": { "application/json": { "schema": { "type": "array", "items": { "type": "string" } } } } } } } },
    "/add": { "post": { "operationId": "add", "x-sf-writes": ["items"], "responses": { "204": { "description": "added" } } } }
  }
}"##;

fn cached_dir() -> PathBuf {
  let dir = app_dir();
  std::fs::create_dir_all(dir.join("clients")).unwrap();
  std::fs::write(dir.join("generated/plan.json"), WHO_PLAN).unwrap();
  std::fs::write(dir.join("clients/shop.openapi.json"), CACHED_OPENAPI).unwrap();
  std::fs::write(
    dir.join("app.toml"),
    r#"
[app]
dir = "."

[server]
listen = "127.0.0.1:0"

[document]
title = "Cached"

[session]
key = "test-key"

[cache]
capacity = 100
ttl = "1m"

[cache.data]
capacity = 50

[clients.shop]
base_url = "http://127.0.0.1:1"
"#,
  )
  .unwrap();
  dir
}

#[tokio::test]
async fn a_cached_method_answers_renders_from_memory_until_a_write_or_a_drop() {
  let transport = Arc::new(MockTransport::new().returns("shop.list", Value::Seq(vec![Value::str("socks")])).returns("shop.add", Value::Null));
  let host = Host::from(cached_dir().join("app.toml")).unwrap().services_over(transport.clone()).build().unwrap();
  let report = host.report().to_string();
  assert!(report.contains("cached    shop.list              ttl 1m shared [items]"), "{report}");
  assert!(report.contains("writes    shop.add               [items]"), "{report}");

  let list_calls = || transport.calls().iter().filter(|(p, _, _)| p == "shop.list").count();
  for _ in 0..2 {
    let response = host.handle(Request::get("/who").body(Bytes::new()).unwrap()).await;
    assert!(body_of(response).await.contains("socks"));
  }
  assert_eq!(list_calls(), 1, "the second render read the entry");

  host.invalidate_tags(["items"]);
  host.handle(Request::get("/who").body(Bytes::new()).unwrap()).await;
  assert_eq!(list_calls(), 2, "an out-of-band drop");

  host.services().bind_anonymous().call("shop", "add", ValueMap::new()).await.unwrap();
  host.handle(Request::get("/who").body(Bytes::new()).unwrap()).await;
  assert_eq!(list_calls(), 3, "a write dropped the tag it names");
}

#[test]
fn without_cache_data_no_method_is_cached_whatever_the_contract_says() {
  let dir = cached_dir();
  let text = std::fs::read_to_string(dir.join("app.toml")).unwrap().replace("[cache.data]\ncapacity = 50\n", "");
  std::fs::write(dir.join("app.toml"), text).unwrap();
  let transport = Arc::new(MockTransport::new().returns("shop.list", Value::Seq(vec![Value::str("socks")])));
  let host = Host::from(dir.join("app.toml")).unwrap().services_over(transport.clone()).build().unwrap();
  assert!(host.report().cached.is_empty());
  assert!(host.services().data_cache().is_none());
}

#[tokio::test]
async fn a_reload_swaps_the_tables_in_place_and_keeps_the_sessions() {
  let dir = app_dir();
  let transport = Arc::new(MockTransport::new().returns("shop.list", Value::Seq(vec![Value::str("a")])));
  let reload_from = dir.clone();
  let reload_with = transport.clone();
  let host = Host::from(dir.join("app.toml"))
    .unwrap()
    .services_over(transport.clone())
    .reloader(move || Host::from(reload_from.join("app.toml")).map(|b| b.services_over(reload_with.clone())))
    .build()
    .unwrap();

  let response = host
    .handle(Request::post("/_sf/action/index.bump").header(header::CONTENT_TYPE, "application/json").body(Bytes::from(r#"{"by": 2}"#)).unwrap())
    .await;
  let cookie = response.headers().get(header::SET_COOKIE).unwrap().to_str().unwrap().split(';').next().unwrap().to_owned();
  let html = host.render_to_string("/hello/x?from=y", RenderMode::Html, SessionCell::default()).await.unwrap();
  assert!(html.contains("hi x via y") && html.contains("<title>Test &lt;app&gt;</title>"), "{html}");

  std::fs::write(dir.join("generated/plan.json"), PLAN.replace("\"hi \"", "\"hey \"")).unwrap();
  let toml = std::fs::read_to_string(dir.join("app.toml")).unwrap();
  std::fs::write(dir.join("app.toml"), toml.replace("Test <app>", "Reloaded")).unwrap();
  let response = host.handle(Request::post("/__fsr/reload").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.status(), StatusCode::OK);
  let printed = body_of(response).await;
  assert!(printed.contains("lowered"), "the reload answers with the new report: {printed}");

  let html = host.render_to_string("/hello/x?from=y", RenderMode::Html, SessionCell::default()).await.unwrap();
  assert!(html.contains("hey x via y") && html.contains("<title>Reloaded</title>"), "the new plan and head serve: {html}");
  let response = host
    .handle(Request::post("/_sf/action/index.bump").header(header::COOKIE, &cookie).body(Bytes::from(r#"{"by": 3}"#)).unwrap())
    .await;
  assert_eq!(body_of(response).await, "5", "the session outlived the reload");

  std::fs::write(dir.join("app.toml"), toml.replace("test-key", "another-key")).unwrap();
  let refused = host.reload().unwrap_err().to_string();
  assert!(refused.contains("`session`") && refused.contains("restart"), "{refused}");
  let html = host.render_to_string("/hello/x?from=y", RenderMode::Html, SessionCell::default()).await.unwrap();
  assert!(html.contains("hey x via y"), "a refused reload leaves the tables alone: {html}");

  let (plain, _) = self::host();
  let none = plain.reload().unwrap_err().to_string();
  assert!(none.contains("no reloader"), "{none}");
}

#[test]
fn a_bundle_carrying_a_server_module_refuses_to_start() {
  let dir = app_dir();
  std::fs::create_dir_all(dir.join("dist")).unwrap();
  let build = |facts: &str| {
    std::fs::write(dir.join("dist/.snapfire-build.json"), facts).unwrap();
    Host::from(dir.join("app.toml")).and_then(|b| b.build()).map(|_| ()).map_err(|e| e.to_string())
  };
  let leaked = build(r#"{"outputs": ["routes/index/page.js", "routes/index/page.loader.js"], "graph": {}}"#).unwrap_err();
  assert!(leaked.contains("server modules in the bundle: routes/index/page.loader.js is a loader, routes/index/page.loader.ts"), "{leaked}");
  let imported = build(r#"{"outputs": ["routes/index/page.js"], "graph": {"routes/index/page.js": ["routes/index/actions.js"]}}"#).unwrap_err();
  assert!(imported.contains("routes/index/page.js imports routes/index/actions.js"), "{imported}");
  build(r#"{"outputs": ["routes/index/page.js"], "graph": {"routes/index/page.js": []}}"#).unwrap();
}

fn site_dir() -> PathBuf {
  let dir = app_dir();
  let manifest = snapfire_fsr_plan::Manifest::from_json(PLAN).unwrap().namespaced("shop", "/shop", "shell#document");
  std::fs::write(dir.join("generated/plan.json"), manifest.to_json()).unwrap();
  let contract = snapfire_fsr_service::Contract::from_json(CONTRACT).unwrap().namespaced("shop");
  std::fs::write(dir.join("generated/contracts/shop.json"), contract.to_json()).unwrap();
  std::fs::create_dir_all(dir.join("styles")).unwrap();
  std::fs::write(dir.join("styles/site.css"), "body{}").unwrap();
  let toml = std::fs::read_to_string(dir.join("app.toml")).unwrap();
  std::fs::write(dir.join("app.toml"), toml + "\n[site]\nname = \"shop\"\nat = \"/shop\"\n").unwrap();
  dir
}

#[tokio::test]
async fn a_site_serves_standalone_under_its_prefix_with_its_ids_prefixed() {
  let dir = site_dir();
  let transport = Arc::new(MockTransport::new().returns("shop:shop.list", Value::Seq(vec![Value::str("a")])));
  let host = Host::from(dir.join("app.toml")).unwrap().services_over(transport.clone()).build().unwrap();
  let report = host.report().to_string();
  assert!(report.contains("site      shop                   at /shop"), "{report}");
  assert!(report.contains("/shop/static/css"), "{report}");
  assert!(report.contains("/shop/hello/{name}") && report.contains("shop:index.bump"), "{report}");

  let html = host.render_to_string("/shop", RenderMode::Html, SessionCell::default()).await.unwrap();
  assert!(html.contains("data-sf-module=\"shop:routes/index/page.tsx#default\"") && html.contains("\"a\""), "{html}");
  assert!(html.contains("href=\"/shop/static/css/site.css\""), "{html}");
  assert_eq!(transport.calls().len(), 1, "the body called the prefixed client");
  let html = host.render_to_string("/shop/hello/x?from=y", RenderMode::Html, SessionCell::default()).await.unwrap();
  assert!(html.contains("hi x via y"), "{html}");
  assert!(host.render_to_string("/hello/x", RenderMode::Html, SessionCell::default()).await.is_err(), "nothing serves outside the prefix");

  let response = host
    .handle(Request::post("/_sf/action/shop:index.bump").header(header::CONTENT_TYPE, "application/json").body(Bytes::from(r#"{"by": 2}"#)).unwrap())
    .await;
  assert_eq!(response.status(), StatusCode::OK, "{}", body_of(response).await);
}

#[test]
fn a_bad_site_section_refuses_to_start() {
  let dir = site_dir();
  let toml = std::fs::read_to_string(dir.join("app.toml")).unwrap();
  std::fs::write(dir.join("app.toml"), toml.replace("at = \"/shop\"", "at = \"shop/\"")).unwrap();
  let e = Host::from(dir.join("app.toml")).map(|_| ()).unwrap_err().to_string();
  assert!(e.contains("site.at"), "{e}");
  std::fs::write(dir.join("app.toml"), toml.replace("name = \"shop\"", "name = \"Shop\"")).unwrap();
  let e = Host::from(dir.join("app.toml")).map(|_| ()).unwrap_err().to_string();
  assert!(e.contains("site.name"), "{e}");
}

const SITE_MIDDLEWARE: &str = r#"[ { "return": { "object": [ { "field": [ "headers", { "object": [ { "field": [ "x-site", { "lit": { "str": "shop" } } ] } ] } ] } ] } } ]"#;

const SHELL_LAYOUT: &str = r#"{ "module": "routes/layout.tsx#default", "body": { "render": { "element": { "tag": "main", "attrs": [ { "field": [ "class", { "lit": { "str": "shell" } } ] } ], "children": [ { "element": { "tag": "header", "children": [ { "text": "the shell" } ] } }, { "slot": "content" } ] } } } }"#;

/// A shell over the fixture whose root layout renders in Rust, so a mounted
/// site's pages land inside it.
fn shell_dir() -> PathBuf {
  let dir = app_dir();
  let plan = std::fs::read_to_string(dir.join("generated/plan.json")).unwrap();
  let mut json: serde_json::Value = serde_json::from_str(&plan).unwrap();
  json["components"] = serde_json::json!([serde_json::from_str::<serde_json::Value>(SHELL_LAYOUT).unwrap()]);
  std::fs::write(dir.join("generated/plan.json"), json.to_string()).unwrap();
  dir
}

fn shell_with(site: &std::path::Path) -> Arc<Host> {
  let transport = Arc::new(MockTransport::new().returns("shop.list", Value::Seq(vec![Value::str("a")])).returns("shop:shop.list", Value::Seq(vec![Value::str("b")])));
  let mount = snapfire_fsr_host::Mount::load("shop", site, "dev", "deadbeef", false).unwrap();
  let host = Host::from(shell_dir().join("app.toml"))
    .unwrap()
    .services_over(transport)
    .middleware(|_ctx, input| async move {
      let site = match &input {
        Value::Map(map) => match map.get("site") {
          Some(Value::Str(name)) => name.clone(),
          _ => "none".to_owned(),
        },
        _ => "none".to_owned(),
      };
      let mut headers = ValueMap::new();
      headers.insert("x-shell".to_owned(), Value::Str(site));
      let mut out = ValueMap::new();
      out.insert("headers".to_owned(), Value::Map(headers));
      Ok(Value::Map(out))
    })
    .mount(mount)
    .build()
    .unwrap();
  Arc::new(host)
}

#[tokio::test]
async fn a_mounted_site_serves_under_the_shells_root_layout_with_its_own_middleware_head_and_clients() {
  let site = site_dir();
  let plan = std::fs::read_to_string(site.join("generated/plan.json")).unwrap();
  let mut json: serde_json::Value = serde_json::from_str(&plan).unwrap();
  json["middleware"] = serde_json::from_str(SITE_MIDDLEWARE).unwrap();
  std::fs::write(site.join("generated/plan.json"), json.to_string()).unwrap();
  let toml = std::fs::read_to_string(site.join("app.toml")).unwrap();
  std::fs::write(site.join("app.toml"), toml.replace("entry = \"/static/app.js\"", "entry = \"/shop/static/js/app/src/main.js\"")).unwrap();
  let host = shell_with(&site);

  let report = host.report().to_string();
  assert!(report.contains("sites     shop                   at /shop from") && report.contains("dev deadbeef"), "{report}");
  assert!(report.contains("ignored [static /static, session]"), "{report}");
  assert!(report.contains("/shop/hello/{name}") && report.contains("shop:index") && report.contains("static    /static") && report.contains("/shop/static/css"), "{report}");

  let response = host.handle(Request::get("/shop").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers().get("x-shell").unwrap(), "shop", "the shell's middleware saw the site");
  assert_eq!(response.headers().get("x-site").unwrap(), "shop", "the site's middleware ran after it");
  let html = body_of(response).await;
  assert!(html.contains("<header>the shell</header>"), "the shell's root layout wraps the site: {html}");
  assert!(html.contains("data-sf-module=\"shop:routes/index/page.tsx#default\"") && html.contains("\"b\""), "the site's page rendered inside it through its own client: {html}");
  assert!(html.contains("<link rel=\"stylesheet\" href=\"/shop/static/css/site.css\">") && html.contains("<script type=\"module\" src=\"/shop/static/js/app/src/main.js\"></script>"), "{html}");

  let response = host.handle(Request::get("/").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.headers().get("x-shell").unwrap(), "none");
  assert!(response.headers().get("x-site").is_none(), "the site's middleware stays under its prefix");
  let html = body_of(response).await;
  assert!(!html.contains("/shop/static/js/app/src/main.js") && html.contains("\"a\""), "the shell's page renders through the shell's client: {html}");

  let payload = body_of(host.handle(Request::get("/shop/hello/x?from=y&__payload").body(Bytes::new()).unwrap()).await).await;
  assert!(payload.contains("E \"/shop/static/js/app/src/main.js\"") && payload.contains("hi x via y"), "{payload}");
  let payload = body_of(host.handle(Request::get("/hello/x?from=y&__payload").body(Bytes::new()).unwrap()).await).await;
  assert!(!payload.contains("\nE "), "{payload}");

  let response = host.handle(Request::get("/shop/static/css/site.css").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.status(), StatusCode::OK, "the site's stylesheet is served under its prefix");
  let response = host
    .handle(Request::post("/_sf/action/shop:index.bump").header(header::CONTENT_TYPE, "application/json").body(Bytes::from(r#"{"by": 4}"#)).unwrap())
    .await;
  assert_eq!(body_of(response).await, "4", "the site's action runs under its prefixed id");
  let status = body_of(host.handle(Request::get("/__fsr/sites").body(Bytes::new()).unwrap()).await).await;
  assert!(status.contains("\"name\":\"shop\"") && status.contains("\"hash\":\"deadbeef\""), "{status}");
}

#[test]
fn a_mount_is_refused_when_its_name_differs_it_carries_engine_rows_or_the_shell_is_a_site() {
  let site = site_dir();
  let mount = snapfire_fsr_host::Mount::load("billing", &site, "dev", "-", false).unwrap();
  let e = Host::from(app_dir().join("app.toml")).unwrap().mount(mount).build().map(|_| ()).unwrap_err().to_string();
  assert!(e.contains("site `billing`") && e.contains("is the site `shop`"), "{e}");

  let plan = std::fs::read_to_string(site.join("generated/plan.json")).unwrap();
  let mut json: serde_json::Value = serde_json::from_str(&plan).unwrap();
  json["sources"][0]["owner"] = serde_json::Value::String("engine".to_owned());
  json["sources"][0]["export"] = serde_json::Value::String("load".to_owned());
  std::fs::write(site.join("generated/plan.json"), json.to_string()).unwrap();
  let mount = snapfire_fsr_host::Mount::load("shop", &site, "dev", "-", false).unwrap();
  let e = Host::from(app_dir().join("app.toml")).unwrap().mount(mount).build().map(|_| ()).unwrap_err().to_string();
  assert!(e.contains("engine-owned rows shop:index"), "{e}");

  let shell = app_dir();
  let toml = std::fs::read_to_string(shell.join("app.toml")).unwrap();
  std::fs::write(shell.join("app.toml"), toml + "\n[sites]\nroot = \"sites\"\n[sites.shop]\nartifact = \"shop@1\"\n[site]\nname = \"x\"\nat = \"/x\"\n").unwrap();
  let e = Host::from(shell.join("app.toml")).map(|_| ()).unwrap_err().to_string();
  assert!(e.contains("cannot mount sites"), "{e}");
}

const EXT_PLAN: &str = r#"{
  "version": 2,
  "routes": [
    { "pattern": "/", "plan": { "id": 0, "module": "shell#document", "children": [
      { "slot": "content", "node": { "id": 1, "module": "routes/index/page.tsx#default", "source": "index" } } ] } }
  ],
  "sources": [
    { "id": "index", "owner": "lowered", "module": "routes/index/page.loader.ts",
      "body": [ { "return": { "object": [
        { "field": [ "label", { "ext": { "module": "fmt", "name": "pretty", "args": [ { "lit": { "float": 1234.5 } } ] } } ] },
        { "field": [ "n", { "ext": { "module": "intl", "name": "number", "args": [ { "lit": { "float": 1234.5 } } ] } } ] }
      ] } } ] }
  ]
}"#;

#[tokio::test]
async fn a_registered_native_pair_answers_a_lowered_body_and_an_unregistered_one_refuses_to_build() {
  let dir = app_dir();
  std::fs::write(dir.join("generated/plan.json"), EXT_PLAN).unwrap();
  let err = Host::from(dir.join("app.toml")).unwrap().build().err().map(|e| e.to_string()).unwrap_or_default();
  assert!(err.contains("`index` calls extension `fmt.pretty`, which nothing registers"), "{err}");

  let host = Host::from(dir.join("app.toml"))
    .unwrap()
    .extension("fmt.pretty", snapfire_fsr_ir::Reach::Render, |ambient, args| Ok(Value::Str(format!("{}:{}", ambient.bcp47(), args.len()))))
    .build()
    .unwrap();
  let payload = host.render_to_string("/", RenderMode::Payload, SessionCell::default()).await.unwrap();
  assert!(payload.contains("en:1"), "the pair ran under the default locale: {payload}");
  assert!(payload.contains("1,234.5"), "the standard library is there too: {payload}");
  assert_eq!(host.report().extensions, vec!["fmt.pretty".to_owned()]);
  assert!(host.report().to_string().contains("natives   fmt.pretty             rust"), "{}", host.report());
}

#[tokio::test]
async fn catalogs_under_locales_reach_the_document_the_payload_and_t() {
  let dir = app_dir();
  std::fs::create_dir_all(dir.join("locales")).unwrap();
  std::fs::write(dir.join("locales/en.toml"), "[hello]\nworld = \"Hello {name}\"\nnum = 3\n[items]\none = \"{count} item\"\nother = \"{count} items\"\n").unwrap();
  std::fs::write(
    dir.join("generated/plan.json"),
    r#"{ "version": 2, "routes": [ { "pattern": "/", "plan": { "id": 0, "module": "shell#document", "children": [ { "slot": "content", "node": { "id": 1, "module": "routes/index/page.tsx#default", "source": "index" } } ] } } ],
      "sources": [ { "id": "index", "owner": "lowered", "module": "routes/index/page.loader.ts", "body": [ { "return": { "object": [
        { "field": [ "hi", { "ext": { "module": "i18n", "name": "t", "args": [ { "lit": { "str": "hello.world" } }, { "object": [ { "field": [ "name", { "lit": { "str": "Norm" } } ] } ] } ] } } ] },
        { "field": [ "n", { "ext": { "module": "i18n", "name": "t", "args": [ { "lit": { "str": "items" } }, { "object": [ { "field": [ "count", { "lit": { "float": 2.0 } } ] } ] } ] } } ] }
      ] } } ] } ] }"#,
  )
  .unwrap();
  let host = Host::from(dir.join("app.toml")).unwrap().build().unwrap();
  assert_eq!(host.report().catalogs, vec![("en".to_owned(), 4)]);
  assert!(host.report().to_string().contains("catalogs  en 4 keys"), "{}", host.report());
  assert_eq!(host.catalogs().unwrap().lookup("en", "hello.num"), Some("3"));

  let html = host.render_to_string("/", RenderMode::Html, SessionCell::default()).await.unwrap();
  assert!(html.contains("<script type=\"application/json\" data-sf-i18n=\"en\">{\"hello.num\":\"3\",\"hello.world\":\"Hello {name}\",\"items.one\":\"{count} item\",\"items.other\":\"{count} items\"}</script>"), "{html}");
  assert!(html.contains("Hello Norm") && html.contains("2 items"), "t ran in the loader: {html}");

  let payload = host.render_to_string("/", RenderMode::Payload, SessionCell::default()).await.unwrap();
  assert!(payload.contains("\nD {\"hello.num\""), "a payload carries the catalog when nothing says it is held: {payload}");

  let host = Arc::new(host);
  let held = host.handle(Request::get("/?__payload").header("x-sf-catalog", "en").body(Bytes::new()).unwrap()).await;
  let text = body_of(held).await;
  assert!(text.contains("\nL \"en\"") && !text.contains("\nD "), "the row is dropped for a navigator holding it: {text}");
  let missing = host.handle(Request::get("/?__payload").header("x-sf-catalog", "fr").body(Bytes::new()).unwrap()).await;
  assert!(body_of(missing).await.contains("\nD {"), "a navigator holding another locale gets it");
}

#[tokio::test]
async fn a_route_reading_only_the_identity_prerenders_for_anonymous_visitors_and_renders_live_for_a_signed_in_one() {
  let transport = Arc::new(MockTransport::new().returns("shop.list", Value::Seq(vec![Value::str("a")])));
  let out = std::env::temp_dir().join(format!("fsr-host-prerender-{}-{}", std::process::id(), rand_suffix()));
  let host = Host::from(identified_dir(USERS).join("app.toml")).unwrap().services_over(transport).prerendered(&out).build().unwrap();
  assert_eq!(host.prerenderable(), vec!["/".to_owned(), "/login".to_owned()]);
  assert_eq!(host.prerenderable_anonymous(), vec!["/who".to_owned()], "the loader reads identity and calls a bearer client, nothing else");
  assert!(host.report().to_string().contains("/who                   ") && host.report().to_string().contains("for anonymous visitors"), "{}", host.report());

  let written = host.prerender(&out).await.unwrap();
  assert!(written.iter().any(|(path, _)| path == "/who"), "{written:?}");
  let host = Arc::new(host);

  let response = host.handle(Request::get("/who").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.headers().get("x-sf-prerendered").map(|v| v.to_str().unwrap()), Some("1"));
  assert!(body_of(response).await.contains("anonymous"));

  let response = host.handle(Request::get("/auth/login?return_to=/who").body(Bytes::new()).unwrap()).await;
  let cookie = cookie_of(&response);
  let response = host
    .handle(Request::post("/auth/callback").header(header::COOKIE, &cookie).header(header::CONTENT_TYPE, "application/x-www-form-urlencoded").body(Bytes::from("user=alice&password=wonder")).unwrap())
    .await;
  assert_eq!(response.status(), StatusCode::SEE_OTHER);
  let response = host.handle(Request::get("/who").header(header::COOKIE, &cookie).body(Bytes::new()).unwrap()).await;
  assert!(response.headers().get("x-sf-prerendered").is_none(), "a signed-in visitor is rendered live");
  let html = body_of(response).await;
  assert!(html.contains("alice") && !html.contains("anonymous"), "{html}");

  let response = host.handle(Request::get("/login").header(header::COOKIE, &cookie).body(Bytes::new()).unwrap()).await;
  assert_eq!(response.headers().get("x-sf-prerendered").map(|v| v.to_str().unwrap()), Some("1"), "a route reading nothing serves its file to everyone");
  let _ = std::fs::remove_dir_all(&out);
}
