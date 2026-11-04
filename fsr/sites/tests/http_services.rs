//! An application that calls an HTTP service needs a host built with the `http` feature, and one built without it says so at boot. A mocked HTTP service needs no feature.

use snapfire_fsr_host::Host;
use snapfire_fsr_host::config::Config;

const SHOP: &str = r#"{"openapi":"3.0.0","info":{"title":"shop","version":"1"},"paths":{"/items":{"get":{"operationId":"list","responses":{"200":{"description":"ok","content":{"application/json":{"schema":{"type":"array","items":{"type":"string"}}}}}}}}}}"#;

fn project(tag: &str, clients: &str) -> std::path::PathBuf {
  let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
  let root = snapfire_fsr_test_internal::scratch_root().join(format!("fsr-http-{tag}-{}-{nanos}", std::process::id()));
  std::fs::create_dir_all(root.join("app/generated")).unwrap();
  std::fs::create_dir_all(root.join("app/clients")).unwrap();
  let plan = r#"{"version":2,"routes":[{"pattern":"/","plan":{"id":0,"module":"shell#document","children":[]}}],"sources":[],"actions":[],"handlers":[],"components":[]}"#;
  std::fs::write(root.join("app/generated/plan.sexp"), snapfire_fsr_plan::Manifest::from_json(plan).unwrap().to_sexpr()).unwrap();
  std::fs::write(root.join("app/clients/shop.openapi.json"), SHOP).unwrap();
  std::fs::write(root.join("app/clients/shop.mock.json"), r#"{"list":["a"]}"#).unwrap();
  std::fs::write(root.join("app.toml"), format!("[app]\ndir = \"app\"\n[document]\ntitle = \"t\"\n[session]\nkey = \"k\"\n{clients}")).unwrap();
  root
}

fn boot(root: &std::path::Path) -> Result<(), String> {
  Host::from_config(Config::load(root).unwrap()).and_then(|builder| builder.build()).map(|_| ()).map_err(|e| e.to_string())
}

#[test]
fn an_application_that_calls_an_http_service_boots_only_on_a_host_that_calls_out() {
  let root = project("live", "[clients.shop]\nbase_url = \"http://127.0.0.1:1\"\n");
  let booted = boot(&root);
  match snapfire_fsr_host::HTTP {
    true => assert!(booted.is_ok(), "{booted:?}"),
    false => assert!(booted.as_ref().is_err_and(|e| e.contains("`http`") && e.contains("shop")), "{booted:?}"),
  }
  let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_mocked_http_service_boots_on_any_host() {
  let root = project("mock", "[clients.shop]\ntransport = \"mock\"\n");
  let booted = boot(&root);
  assert!(booted.is_ok(), "{booted:?}");
  let _ = std::fs::remove_dir_all(&root);
}
