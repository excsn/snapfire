//! `fsr build` refuses an application whose host lacks a feature its configuration calls for, naming the feature, what calls for it and where to turn it on.

use std::path::{Path, PathBuf};

const HOST: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../host");
const LOCK: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../Cargo.lock");

fn project(tag: &str, cargo: Option<&str>) -> PathBuf {
  let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
  let root = snapfire_fsr_test_internal::scratch_root().join(format!("fsr-cli-features-{tag}-{}-{nanos}", std::process::id()));
  std::fs::create_dir_all(root.join("app")).unwrap();
  std::fs::write(root.join("app.toml"), "[app]\ndir = \"app\"\n[document]\ntitle = \"t\"\n[session]\nkey = \"k\"\n[server.tls]\ncert = \"cert.pem\"\nkey = \"key.pem\"\n").unwrap();
  if let Some(features) = cargo {
    let host = Path::new(HOST).canonicalize().unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
    std::fs::write(root.join("Cargo.toml"), format!("[package]\nname = \"probe\"\nversion = \"0.0.0\"\nedition = \"2024\"\npublish = false\n\n[workspace]\n\n[dependencies]\nsnapfire_fsr_host = {{ path = {:?}{features} }}\n", host.display().to_string())).unwrap();
    std::fs::copy(LOCK, root.join("Cargo.lock")).unwrap();
  }
  root
}

#[test]
fn a_crate_whose_host_lacks_a_feature_the_configuration_calls_for_is_refused() {
  let root = project("lacks", Some(""));
  let refused = snapfire_fsr_cli::features::check_with(&root.join("app"), false).err().map(|e| e.to_string()).unwrap_or_default();
  assert!(refused.contains("`tls` for [server.tls]") && refused.contains("Cargo.toml"), "{refused}");
  let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_crate_whose_host_holds_the_feature_builds() {
  let root = project("holds", Some(", features = [\"tls\"]"));
  let checked = snapfire_fsr_cli::features::check_with(&root.join("app"), false);
  assert!(checked.is_ok(), "{:?}", checked.err().map(|e| e.to_string()));
  let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn an_application_with_no_crate_is_checked_against_the_stock_host() {
  let root = project("stock", None);
  let checked = snapfire_fsr_cli::features::check_with(&root.join("app"), true);
  assert!(checked.as_ref().is_ok_and(|lines| lines.is_empty()), "{:?}", checked.err().map(|e| e.to_string()));
  let _ = std::fs::remove_dir_all(&root);
}
