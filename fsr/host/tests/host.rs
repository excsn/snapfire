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
      { "slot": "content", "node": { "id": 1, "module": "routes/hello/page.tsx#default", "source": "hello", "cache_key": "routes/hello/page.tsx#default" } } ] } }
  ],
  "sources": [
    { "id": "index", "owner": "lowered", "module": "routes/index/page.loader.ts",
      "body": [ { "return": { "object": [ { "field": [ "items", { "call": { "service": "shop", "method": "list", "args": [] } } ] } ] } } ] },
    { "id": "hello", "owner": "lowered", "module": "routes/hello/page.loader.ts",
      "body": [ { "return": { "object": [ { "field": [ "greeting", { "template": [ { "lit": { "str": "hi " } }, { "param": "name" }, { "lit": { "str": " via " } }, { "query": "from" } ] } ] } ] } } ] }
  ],
  "actions": [
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
  std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
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
  assert_eq!(host.report.app.sources.len(), 2);
  assert!(host.report.to_string().contains("lowered"), "{}", host.report);
  assert!(host.report.to_string().contains("/static"), "{}", host.report);
}

#[tokio::test]
async fn params_and_query_reach_a_lowered_loader() {
  let (host, _) = host();
  let payload = host.render_to_string("/hello/norm?from=test", RenderMode::Payload, SessionCell::default()).await.unwrap();
  assert!(payload.contains("hi norm via test"), "{payload}");
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
  assert!(host.report.to_string().contains("config"), "{}", host.report);
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
  let report = host.report.to_string();
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
  assert!(host.report.to_string().contains("cache     1000 entries, ttl 5m"), "{}", host.report);

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
  assert!(!host.report.to_string().contains("cache "), "{}", host.report);
  host.render_to_string("/hello/norm", RenderMode::Html, SessionCell::default()).await.unwrap();
  assert_eq!(host.invalidate("routes/hello/page.tsx#default").await, 0);
}
