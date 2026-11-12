//! `fsr-server`, the stock host in production, boots whatever `fsr serve`, the stock host in development, boots: both are built with the host's `stock` features.
#![cfg(feature = "server")]

use snapfire_fsr_host::Host;
use snapfire_fsr_host::config::Config;

fn project(tag: &str, extra: &str) -> std::path::PathBuf {
  let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
  let root = snapfire_fsr_test_internal::scratch_root().join(format!("fsr-stock-{tag}-{}-{nanos}", std::process::id()));
  std::fs::create_dir_all(root.join("app/generated")).unwrap();
  let plan = r#"{"version":2,"routes":[{"pattern":"/","plan":{"id":0,"module":"shell#document","children":[]}}],"sources":[],"actions":[],"handlers":[],"components":[]}"#;
  std::fs::write(root.join("app/generated/plan.sexp"), snapfire_fsr_plan::Manifest::from_json(plan).unwrap().to_sexpr()).unwrap();
  std::fs::write(root.join("app.toml"), format!("[app]\ndir = \"app\"\n[document]\ntitle = \"t\"\n[session]\nkey = \"k\"\n{extra}")).unwrap();
  root
}

#[test]
fn the_stock_server_reads_the_system_time_zone_database() {
  let root = project("zones", "[time]\nzones = \"system\"\n");
  let booted = Host::from_config(Config::load(&root).unwrap()).and_then(|builder| builder.build()).map(|_| ());
  assert!(booted.is_ok(), "{:?}", booted.err().map(|e| e.to_string()));
  let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_stock_server_holds_every_stock_feature() {
  let missing: Vec<&str> = snapfire_fsr_host::features::STOCK.iter().copied().filter(|feature| !snapfire_fsr_host::features::built(feature)).collect();
  assert!(missing.is_empty(), "{missing:?}");
}
