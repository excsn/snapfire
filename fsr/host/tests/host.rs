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
  ],
  "intercepts": [
    { "pattern": "/feed/photo/{id}", "plan": { "id": 0, "module": "shell#document", "children": [
      { "slot": "content", "node": { "id": 1, "module": "routes/feed/layout.tsx#default", "keep": ["content"], "children": [
        { "slot": "modal", "node": { "id": 2, "module": "routes/feed/photo/page.modal.tsx#default" } } ] } } ] } }
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

#[tokio::test]
async fn development_documents_carry_the_refresh_script_and_the_host_announces_changes() {
  use http_body_util::BodyExt;

  let dir = app_dir();
  let transport = Arc::new(MockTransport::new().returns("shop.list", Value::Seq(vec![Value::str("a")])));
  let host = Host::from(dir.join("app.toml")).unwrap().services_over(transport).build().unwrap();
  assert!(host.report.dev, "RELEASE_ENV is unset, so this is development");
  assert!(host.report.to_string().contains("dev       live refresh on /__fsr/events"), "{}", host.report);
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
  assert!(!host.report.dev);
  assert!(!host.report.to_string().contains("dev "), "{}", host.report);
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
  assert!(host.intercept_for("/feed/photo/3", Some("/feed"), Some("drawer")).is_none(), "a slot the route has no variant for");
  assert!(host.intercept_for("/feed", Some("/feed"), None).is_none(), "a route without a variant");

  let payload = host.render_navigation_to_string("/feed/photo/3", Some("/feed"), None, SessionCell::default()).await.unwrap();
  let sidecar = payload.lines().find(|l| l.starts_with("G ")).unwrap();
  assert!(sidecar.contains("\"keep\":[\"content\"]"), "{sidecar}");
  let plain = host.render_navigation_to_string("/feed/photo/3", Some("/"), None, SessionCell::default()).await.unwrap();
  assert!(!plain.contains("keep"), "{plain}");

  let response = host.handle(Request::get("/feed/photo/3?__payload").header("x-sf-from", "/feed?x=1").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.status(), StatusCode::OK);
  let body = String::from_utf8(response.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
  assert!(body.contains("\"keep\":[\"content\"]"), "the edge reads the origin header: {body}");
  let response = host.handle(Request::get("/feed/photo/3?__payload").body(Bytes::new()).unwrap()).await;
  let body = String::from_utf8(response.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
  assert!(!body.contains("keep"), "without it the payload is the page's: {body}");
  let response = host.handle(Request::get("/feed/photo/3").header("x-sf-from", "/feed").body(Bytes::new()).unwrap()).await;
  let body = String::from_utf8(response.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
  assert!(body.contains("<!doctype html>") && !body.contains("page.modal"), "a document is never intercepted: {body}");
}
