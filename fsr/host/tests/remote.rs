//! A site in its own process: the shell renders its page from the rows the
//! site's host answers, forwards its statics, actions and handlers as bytes
//! and degrades to the error node when every address is down.

use std::path::PathBuf;
use std::sync::Arc;

use bytes::Bytes;
use http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use snapfire_fsr_host::{Body, Host, PAYLOAD_TYPE};

fn dir(tag: &str) -> PathBuf {
  static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
  let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
  let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let dir = std::env::temp_dir().join(format!("fsr-remote-{tag}-{}-{nanos}-{n}", std::process::id()));
  std::fs::create_dir_all(dir.join("generated/contracts")).unwrap();
  std::fs::create_dir_all(dir.join("public")).unwrap();
  std::fs::write(dir.join("importmap.json"), r#"{"imports":{}}"#).unwrap();
  dir
}

fn write_plan(dir: &std::path::Path, json: &str) {
  let manifest = snapfire_fsr_plan::Manifest::from_text(json).expect("a plan");
  std::fs::write(dir.join("generated/plan.sexp"), manifest.to_sexpr()).unwrap();
}

/// The site as `fsr build` leaves it: every id under its name, its routes
/// under its prefix, a page with a loader, a handler and an action.
const SITE_PLAN: &str = r#"{
  "version": 2,
  "routes": [
    { "pattern": "/reports", "plan": { "id": 0, "module": "shell#document", "children": [
      { "slot": "content", "node": { "id": 1, "module": "reports:routes/layout.tsx#default", "children": [
        { "slot": "content", "node": { "id": 2, "module": "reports:routes/page.tsx#default", "source": "reports:index" } } ] } } ] } },
    { "pattern": "/reports/late", "plan": { "id": 0, "module": "shell#document", "children": [
      { "slot": "content", "node": { "id": 1, "module": "reports:routes/layout.tsx#default", "children": [
        { "slot": "content", "node": { "id": 2, "module": "reports:routes/late/page.tsx#default", "source": "reports:late", "deferred": true, "fallback": "reports:routes/late/loading.tsx#default" } } ] } } ] } }
  ],
  "sources": [
    { "id": "reports:index", "owner": "lowered", "module": "routes/page.loader.ts",
      "body": [ { "return": { "object": [ { "field": [ "greeting", { "lit": { "str": "hello from reports" } } ] } ] } } ] },
    { "id": "reports:late", "owner": "lowered", "module": "routes/late/page.loader.ts",
      "body": [ { "return": { "object": [ { "field": [ "late", { "lit": { "str": "arrived" } } ] } ] } } ] }
  ],
  "handlers": [
    { "id": "reports:index.POST", "method": "POST", "pattern": "/reports/bump", "owner": "lowered", "module": "routes/route.ts", "input": "Bump",
      "body": [ { "return": { "object": [ { "field": [ "by", { "field": [ "input", "by" ] } ] } ] } } ] }
  ],
  "actions": [
    { "id": "reports:index.where", "owner": "lowered", "module": "routes/actions.ts",
      "body": [ { "return": "locale" } ] }
  ]
}"#;

const CONTRACT: &str = r#"{
  "types": { "Bump": { "record": { "fields": [ { "name": "by", "type": "i64" } ] } } },
  "services": {}
}"#;

const SHELL_PLAN: &str = r#"{
  "version": 2,
  "routes": [
    { "pattern": "/", "plan": { "id": 0, "module": "shell#document", "children": [
      { "slot": "content", "node": { "id": 1, "module": "routes/layout.tsx#default", "children": [
        { "slot": "content", "node": { "id": 2, "module": "routes/index/page.tsx#default" } } ] } } ] } }
  ]
}"#;

fn site_dir() -> PathBuf {
  let dir = dir("site");
  write_plan(&dir, SITE_PLAN);
  std::fs::write(dir.join("generated/contracts/reports.json"), CONTRACT).unwrap();
  std::fs::write(dir.join("public/app.js"), "console.log('reports')").unwrap();
  std::fs::write(
    dir.join("app.toml"),
    r#"
[app]
dir = "."

[server]
listen = "127.0.0.1:0"

[site]
name = "reports"
at = "/reports"

[document]
title = "Reports"
entry = "/reports/static/app.js"

[session]
key = "site-key"
ttl = "10m"

[[static]]
route = "/reports/static"
dir = "public"
"#,
  )
  .unwrap();
  dir
}

fn shell_dir(over: &[String]) -> PathBuf {
  let dir = dir("shell");
  write_plan(&dir, SHELL_PLAN);
  let addresses: Vec<String> = over.iter().map(|a| format!("\"{a}\"")).collect();
  std::fs::write(
    dir.join("app.toml"),
    format!(
      r#"
[app]
dir = "."

[server]
listen = "127.0.0.1:0"

[document]
title = "Acme"

[session]
key = "shell-key"
ttl = "10m"

[sites.reports]
at = "/reports"
over = [{}]
timeout = "2s"
retry = "1s"
"#,
      addresses.join(", ")
    ),
  )
  .unwrap();
  dir
}

/// The site served on a port of its own, as its own process would.
async fn site_host() -> (Arc<Host>, String) {
  let host = Arc::new(Host::from(site_dir().join("app.toml")).unwrap().build().unwrap());
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let address = format!("http://{}", listener.local_addr().unwrap());
  tokio::spawn(host.clone().serve_listener(listener));
  (host, address)
}

fn shell_over(addresses: &[String]) -> Arc<Host> {
  Arc::new(Host::from(shell_dir(addresses).join("app.toml")).unwrap().build().unwrap())
}

async fn text(response: http::Response<Body>) -> String {
  String::from_utf8(response.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap()
}

fn get(path: &str, accept: &str) -> Request<Bytes> {
  Request::builder().uri(path).header(header::ACCEPT, accept).body(Bytes::new()).unwrap()
}

fn post(path: &str, json: &str) -> Request<Bytes> {
  Request::builder()
    .method("POST")
    .uri(path)
    .header(header::CONTENT_TYPE, "application/json")
    .header(header::ACCEPT, "application/json")
    .body(Bytes::from(json.to_owned()))
    .unwrap()
}

#[tokio::test]
async fn the_shell_renders_a_remote_sites_page_inside_its_own_document() {
  let (_site, address) = site_host().await;
  let shell = shell_over(&[address]);
  let response = shell.handle(get("/reports", "text/html")).await;
  assert_eq!(response.status(), StatusCode::OK);
  let html = text(response).await;
  assert!(html.contains("<title>Reports</title>"), "the site's title: {html}");
  assert!(html.contains("routes/layout.tsx#default"), "the shell's root layout: {html}");
  assert!(html.contains("reports:routes/page.tsx#default"), "the site's page under it: {html}");
  assert!(html.contains("hello from reports"), "rendered from the site's loader: {html}");
  assert!(html.contains("<script type=\"module\" src=\"/reports/static/app.js\"></script>"), "the site's entry in the shell's head: {html}");
  assert!(html.contains("data-sf-segments"), "{html}");
  assert!(html.contains("\"k\":\"reports:routes/layout.tsx#default\""), "the site's keys in the sidecar: {html}");
}

#[tokio::test]
async fn a_payload_navigation_into_the_site_carries_its_entry_and_its_fill_streams_through() {
  let (_site, address) = site_host().await;
  let shell = shell_over(&[address]);
  let response = shell.handle(get("/reports/late?__payload", "*/*")).await;
  assert_eq!(response.status(), StatusCode::OK);
  let rows = text(response).await;
  assert!(rows.starts_with("V {\"fmt\":2"), "{rows}");
  assert!(rows.contains("E \"/reports/static/app.js\""), "{rows}");
  assert!(rows.contains("reports:routes/late/page.tsx#default"), "{rows}");
  assert!(rows.contains("\nS 1 "), "the site's deferred page fills under the shell's slot: {rows}");
  assert!(rows.contains("arrived"), "{rows}");
}

#[tokio::test]
async fn the_sites_statics_actions_and_handlers_are_forwarded_as_bytes() {
  let (_site, address) = site_host().await;
  let shell = shell_over(&[address]);
  let file = shell.handle(get("/reports/static/app.js", "*/*")).await;
  assert_eq!(file.status(), StatusCode::OK);
  assert_eq!(text(file).await, "console.log('reports')");

  let action = shell.handle(post("/_sf/action/reports:index.where", "{}")).await;
  assert_eq!(action.status(), StatusCode::OK, "{}", text(action).await);
  assert!(text(action).await.contains("en"));

  let handler = shell.handle(post("/reports/bump", r#"{"by":2}"#)).await;
  assert_eq!(handler.status(), StatusCode::OK, "{}", text(handler).await);
  assert!(text(handler).await.contains("\"by\":2"));
}

#[tokio::test]
async fn a_site_whose_every_address_is_down_renders_the_error_node_and_answers_503() {
  let shell = shell_over(&["http://127.0.0.1:1".to_owned()]);
  let response = shell.handle(get("/reports", "text/html")).await;
  assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
  let html = text(response).await;
  assert!(html.contains("data-sf-error"), "{html}");
  assert!(html.contains("routes/layout.tsx#default"), "the shell's own layout still renders: {html}");
  let status = text(shell.handle(get("/__fsr/sites", "*/*")).await).await;
  assert!(status.contains("\"up\":false"), "{status}");
}

#[tokio::test]
async fn a_site_host_describes_itself_at_its_status_route() {
  let (site, _address) = site_host().await;
  let status = text(site.handle(get("/__fsr/sites", "*/*")).await).await;
  let json: serde_json::Value = serde_json::from_str(&status).unwrap();
  assert_eq!(json["site"]["name"], "reports");
  assert_eq!(json["site"]["at"], "/reports");
  assert_eq!(json["site"]["entry"], "/reports/static/app.js");
  let payload = site.handle(get("/reports?__payload", "*/*")).await;
  assert!(payload.headers().get(header::CONTENT_TYPE).unwrap().to_str().unwrap().starts_with(PAYLOAD_TYPE));
}
