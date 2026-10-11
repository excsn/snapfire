//! A cache tag two sites share through `@`: a drop in one site reaches the
//! other's cached answers, a prefixed tag stays the site's own and the boot
//! report names the sharing.

use std::path::PathBuf;
use std::sync::Arc;

use snapfire_fsr_core::Value;
use snapfire_fsr_host::{Host, Loader, Mount, RenderMode};
use snapfire_fsr_runtime::SessionCell;
use snapfire_fsr_service::MockTransport;

fn dir(tag: &str) -> PathBuf {
  static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
  let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
  let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let dir = std::env::temp_dir().join(format!("fsr-tags-{tag}-{}-{nanos}-{n}", std::process::id()));
  std::fs::create_dir_all(dir.join("generated/contracts")).unwrap();
  std::fs::write(dir.join("importmap.json"), r#"{"imports":{}}"#).unwrap();
  dir
}

fn write_plan(dir: &std::path::Path, manifest: &snapfire_fsr_plan::Manifest) {
  std::fs::write(dir.join("generated/plan.sexp"), manifest.to_sexpr()).unwrap();
}

const SHELL_PLAN: &str = r#"{ "version": 2, "routes": [
  { "pattern": "/", "plan": { "id": 0, "module": "shell#document", "children": [
    { "slot": "content", "node": { "id": 1, "module": "routes/layout.tsx#default", "children": [
      { "slot": "content", "node": { "id": 2, "module": "routes/index/page.tsx#default" } } ] } } ] } } ] }"#;

/// Site a: a page whose loader reads a method cached under the shared tag.
const A_PLAN: &str = r#"{ "version": 2, "routes": [
  { "pattern": "/", "plan": { "id": 0, "module": "shell#document", "children": [
    { "slot": "content", "node": { "id": 1, "module": "routes/index/page.tsx#default", "source": "index" } } ] } } ],
  "sources": [ { "id": "index", "owner": "lowered", "module": "routes/page.loader.ts",
    "body": [ { "return": { "object": [ { "field": [ "items", { "call": { "service": "cat", "method": "list", "args": [] } } ] } ] } } ] } ] }"#;
const A_CONTRACT: &str = r#"{ "services": { "cat": { "methods": { "list": { "params": [], "returns": { "list": "str" },
  "cache": { "ttl": "5m", "scope": "shared", "tags": ["@catalog"] } } } } } }"#;

/// Site b: one action drops the shared tag, the other a tag of its own.
const B_PLAN: &str = r#"{ "version": 2, "routes": [
  { "pattern": "/", "plan": { "id": 0, "module": "shell#document", "children": [
    { "slot": "content", "node": { "id": 1, "module": "routes/index/page.tsx#default" } } ] } } ],
  "actions": [
    { "id": "index.drop_shared", "owner": "lowered", "module": "routes/actions.ts", "body": [ { "return": { "call": { "service": "ledger", "method": "pay", "args": [] } } } ] },
    { "id": "index.drop_own", "owner": "lowered", "module": "routes/actions.ts", "body": [ { "return": { "call": { "service": "ledger", "method": "touch", "args": [] } } } ] } ] }"#;
const B_CONTRACT: &str = r#"{ "services": { "ledger": { "methods": {
  "pay": { "params": [], "returns": "str", "writes": ["@catalog"] },
  "touch": { "params": [], "returns": "str", "writes": ["catalog"] } } } } }"#;

fn site(name: &str, plan: &str, contract: &str) -> PathBuf {
  let dir = dir(name);
  let manifest = snapfire_fsr_plan::Manifest::from_json(plan).unwrap().namespaced(name, &format!("/{name}"), "shell#document");
  write_plan(&dir, &manifest);
  let contract = snapfire_fsr_service::Contract::from_json(contract).unwrap().namespaced(name);
  std::fs::write(dir.join(format!("generated/contracts/{name}.json")), contract.to_json()).unwrap();
  std::fs::write(
    dir.join("app.toml"),
    format!("[app]\ndir = \".\"\n\n[server]\nlisten = \"127.0.0.1:0\"\n\n[session]\nkey = \"site-key\"\n\n[site]\nname = \"{name}\"\nat = \"/{name}\"\n"),
  )
  .unwrap();
  dir
}

fn shell() -> PathBuf {
  let dir = dir("shell");
  write_plan(&dir, &snapfire_fsr_plan::Manifest::from_json(SHELL_PLAN).unwrap());
  std::fs::write(dir.join("generated/contracts/shell.json"), "{}").unwrap();
  std::fs::write(
    dir.join("app.toml"),
    "[app]\ndir = \".\"\n\n[server]\nlisten = \"127.0.0.1:0\"\n\n[session]\nkey = \"shell-key\"\n\n[cache]\ncapacity = 100\nttl = \"1m\"\n\n[cache.data]\ncapacity = 50\n",
  )
  .unwrap();
  dir
}

#[tokio::test]
async fn a_drop_of_a_shared_tag_in_one_site_reaches_another_sites_cached_answer_and_a_prefixed_tag_does_not() {
  let transport = Arc::new(
    MockTransport::new()
      .returns("a:cat.list", Value::seq(vec![Value::str("socks")]))
      .returns("b:ledger.pay", Value::str("paid"))
      .returns("b:ledger.touch", Value::str("touched")),
  );
  let a = site("a", A_PLAN, A_CONTRACT);
  let b = site("b", B_PLAN, B_CONTRACT);
  let host = Host::from(shell().join("app.toml"))
    .unwrap()
    .services_over(transport.clone())
    .mount(Mount::new("a", "dev", "-", false, Loader::at(&a).load().unwrap()))
    .mount(Mount::new("b", "dev", "-", false, Loader::at(&b).load().unwrap()))
    .build()
    .unwrap();
  let report = host.report().to_string();
  assert!(report.contains("cached    a:cat.list             ttl 5m shared [@catalog]"), "{report}");
  assert!(report.contains("b:ledger.touch         [b:catalog]"), "a bare tag takes the site's prefix: {report}");
  assert!(report.contains("shared    @catalog               cached by [a:cat.list], dropped by [b:ledger.pay]"), "{report}");

  let list_calls = || transport.calls().iter().filter(|(p, _, _)| p == "a:cat.list").count();
  let render = || host.render_to_string("/a", RenderMode::Html, SessionCell::default());
  assert!(render().await.unwrap().contains("socks"));
  render().await.unwrap();
  assert_eq!(list_calls(), 1, "the second render read the entry");

  host.call_action("b:index.drop_own", SessionCell::default(), Value::Null).await.unwrap();
  render().await.unwrap();
  assert_eq!(list_calls(), 1, "b's own tag is b:catalog and touches nothing of a's");

  host.call_action("b:index.drop_shared", SessionCell::default(), Value::Null).await.unwrap();
  render().await.unwrap();
  assert_eq!(list_calls(), 2, "the shared tag dropped a's answer");
}
