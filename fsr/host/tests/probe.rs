//! A reload probes its candidate before the swap: the routes that read
//! nothing render, each site's `[site] probe` list renders, the checks a
//! Rust host registered run. The first refusal keeps the running tables.

use std::path::PathBuf;
use std::sync::Arc;

use bytes::Bytes;
use http::{Request, StatusCode};
use snapfire_fsr_core::ext::FailureKind;
use snapfire_fsr_core::Value;
use snapfire_fsr_host::{Candidate, Host, HostBuilder, Loader, Mount, Probe, ReloadCheck};
use snapfire_fsr_service::MockTransport;
use snapfire_fsr_test_internal::body_of;

fn dir(tag: &str) -> PathBuf {
  static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
  let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
  let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let dir = std::env::temp_dir().join(format!("fsr-probe-{tag}-{}-{nanos}-{n}", std::process::id()));
  std::fs::create_dir_all(dir.join("generated/contracts")).unwrap();
  std::fs::write(dir.join("importmap.json"), r#"{"imports":{}}"#).unwrap();
  dir
}

const SHELL_PLAN: &str = r#"{ "version": 2, "routes": [
  { "pattern": "/", "plan": { "id": 0, "module": "shell#document", "children": [
    { "slot": "content", "node": { "id": 1, "module": "routes/layout.tsx#default", "children": [
      { "slot": "content", "node": { "id": 2, "module": "routes/index/page.tsx#default", "source": "index" } } ] } } ] } } ],
  "sources": [ { "id": "index", "owner": "lowered", "module": "routes/page.loader.ts",
    "body": [ { "return": { "object": [ { "field": [ "items", { "call": { "service": "shop", "method": "list", "args": [] } } ] } ] } } ] } ] }"#;
const SHELL_CONTRACT: &str = r#"{ "services": { "shop": { "methods": { "list": { "params": [], "returns": { "list": "str" } } } } } }"#;

/// A site whose index reads nothing and whose `/guarded` page fails with
/// `conflict` for a visitor with no identity.
const SITE_PLAN: &str = r#"{ "version": 2, "routes": [
  { "pattern": "/", "plan": { "id": 0, "module": "shell#document", "children": [
    { "slot": "content", "node": { "id": 1, "module": "routes/index/page.tsx#default", "source": "index" } } ] } },
  { "pattern": "/guarded", "plan": { "id": 0, "module": "shell#document", "children": [
    { "slot": "content", "node": { "id": 1, "module": "routes/guarded/page.tsx#default", "source": "guarded" } } ] } } ],
  "sources": [
    { "id": "index", "owner": "lowered", "module": "routes/page.loader.ts",
      "body": [ { "return": { "object": [ { "field": [ "rows", { "call": { "service": "ledger", "method": "rows", "args": [] } } ] } ] } } ] },
    { "id": "guarded", "owner": "lowered", "module": "routes/guarded/page.loader.ts",
      "body": [
        { "guard": { "cond": { "compare": [ "eq", { "coalesce": [ { "identity": [ "subject" ] }, { "lit": { "str": "" } } ] }, { "lit": { "str": "" } } ] }, "kind": "conflict", "message": { "lit": { "str": "no one is signed in" } } } },
        { "return": { "object": [ { "field": [ "who", { "identity": [ "subject" ] } ] } ] } } ] } ] }"#;
const SITE_CONTRACT: &str = r#"{ "services": { "ledger": { "methods": { "rows": { "params": [], "returns": { "list": "str" } } } } } }"#;

fn shell() -> PathBuf {
  let dir = dir("shell");
  std::fs::write(dir.join("generated/plan.sexp"), snapfire_fsr_plan::Manifest::from_json(SHELL_PLAN).unwrap().to_sexpr()).unwrap();
  std::fs::write(dir.join("generated/contracts/shell.json"), SHELL_CONTRACT).unwrap();
  std::fs::write(dir.join("app.toml"), "[app]\ndir = \".\"\n\n[server]\nlisten = \"127.0.0.1:0\"\n\n[session]\nkey = \"shell-key\"\n").unwrap();
  dir
}

fn site(probe: &str) -> PathBuf {
  let dir = dir("site");
  let manifest = snapfire_fsr_plan::Manifest::from_json(SITE_PLAN).unwrap().namespaced("billing", "/billing", "shell#document");
  std::fs::write(dir.join("generated/plan.sexp"), manifest.to_sexpr()).unwrap();
  let contract = snapfire_fsr_service::Contract::from_json(SITE_CONTRACT).unwrap().namespaced("billing");
  std::fs::write(dir.join("generated/contracts/billing.json"), contract.to_json()).unwrap();
  std::fs::write(
    dir.join("app.toml"),
    format!("[app]\ndir = \".\"\n\n[server]\nlisten = \"127.0.0.1:0\"\n\n[session]\nkey = \"site-key\"\n\n[site]\nname = \"billing\"\nat = \"/billing\"\n{probe}"),
  )
  .unwrap();
  dir
}

fn transport(shop_up: bool, ledger_up: bool) -> Arc<MockTransport> {
  let mut t = MockTransport::new();
  t = if shop_up { t.returns("shop.list", Value::seq(vec![Value::str("socks")])) } else { t.fails("shop.list", FailureKind::Unavailable, "the shop is down") };
  t = if ledger_up { t.returns("billing:ledger.rows", Value::seq(vec![Value::str("row")])) } else { t.fails("billing:ledger.rows", FailureKind::Unavailable, "the ledger is down") };
  Arc::new(t)
}

fn builder(shell: &std::path::Path, site: Option<&std::path::Path>, transport: Arc<MockTransport>) -> HostBuilder {
  let mut builder = Host::from(shell.join("app.toml")).unwrap().services_over(transport);
  if let Some(site) = site {
    builder = builder.mount(Mount::new("billing", "dev", "-", false, Loader::at(site).load().unwrap()));
  }
  builder
}

#[tokio::test]
async fn a_candidate_whose_route_that_reads_nothing_fails_is_refused_and_the_old_tables_keep_serving() {
  let shell = shell();
  let host = builder(&shell, None, transport(true, true)).build().unwrap();
  assert!(host.prerenderable().contains(&"/".to_owned()), "{}", host.report());

  let e = host.reload_with_checked(builder(&shell, None, transport(false, true))).await.unwrap_err().to_string();
  assert!(e.contains("reload refused by a probe") && e.contains("index") && e.contains("unavailable"), "the warm pass names the source: {e}");
  let response = host.handle(Request::get("/").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.status(), StatusCode::OK);
  assert!(body_of(response).await.contains("socks"), "the running tables still answer from the backend that was up");

  host.reload_with_checked(builder(&shell, None, transport(true, true))).await.unwrap();
  assert!(host.reload_with(builder(&shell, None, transport(false, true))).is_ok(), "the unchecked reload swaps whatever assembles");
}

#[tokio::test]
async fn a_sites_probe_list_renders_each_path_as_the_subject_it_names_and_a_failing_one_refuses_the_candidate() {
  let shell = shell();
  let host = builder(&shell, Some(&site("")), transport(true, true)).build().unwrap();

  let signed = site("probe = [\"/billing\", { path = \"/billing/guarded\", as = \"probe\" }]\n");
  let report = host.reload_with_checked(builder(&shell, Some(&signed), transport(true, true))).await.unwrap();
  assert_eq!(report.sites.len(), 1);

  let anonymous = site("probe = [\"/billing/guarded\"]\n");
  let e = host.reload_with_checked(builder(&shell, Some(&anonymous), transport(true, true))).await.unwrap_err().to_string();
  assert!(e.contains("site billing, /billing/guarded") && e.contains("conflict"), "the guard refuses a visitor with no identity: {e}");

  let down = site("probe = [\"/billing\"]\n");
  let e = host.reload_with_checked(builder(&shell, Some(&down), transport(true, false))).await.unwrap_err().to_string();
  assert!(e.contains("billing") && e.contains("unavailable"), "the warm pass names the site's source before the probe list runs: {e}");
  assert_eq!(host.handle(Request::get("/billing").body(Bytes::new()).unwrap()).await.status(), StatusCode::OK, "the old site serves on");

  let config = snapfire_fsr_host::Config::load(&signed).unwrap();
  assert_eq!(config.site.unwrap().probe, [Probe::Path("/billing".to_owned()), Probe::As { path: "/billing/guarded".to_owned(), subject: "probe".to_owned() }]);
}

struct TeamsListed;

impl ReloadCheck for TeamsListed {
  fn check<'a>(&'a self, candidate: &'a Candidate<'a>) -> futures_util::future::BoxFuture<'a, Result<(), String>> {
    Box::pin(async move {
      let (status, html) = candidate.render("/", Default::default()).await.map_err(|e| e.to_string())?;
      if status != StatusCode::OK || !html.contains("socks") {
        return Err(format!("/ answered {status} without the shop's items"));
      }
      let (status, _) = candidate.render_as("/billing/guarded", "ops").await.map_err(|e| e.to_string())?;
      (status == StatusCode::OK).then_some(()).ok_or_else(|| format!("/billing/guarded as ops answered {status}"))
    })
  }
}

#[tokio::test]
async fn a_rust_check_runs_over_the_candidate_and_its_reason_names_the_refusal() {
  let shell = shell();
  let host = builder(&shell, Some(&site("")), transport(true, true)).reload_check(Arc::new(TeamsListed)).build().unwrap();
  host.reload_with_checked(builder(&shell, Some(&site("")), transport(true, true))).await.unwrap();

  let e = host.reload_with_checked(builder(&shell, None, transport(true, true))).await.unwrap_err().to_string();
  assert!(e.contains("/billing/guarded as ops answered") || e.contains("no route"), "a candidate without the site fails the check: {e}");
  assert_eq!(host.report().sites.len(), 1, "the running tables still hold the site");
}
