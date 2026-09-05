use std::path::PathBuf;

use snapfire_fsr_host::config::Config;
use snapfire_fsr_sites::{hash_dir, resolve};

fn dir(name: &str) -> PathBuf {
  let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
  let dir = std::env::temp_dir().join(format!("fsr-sites-{name}-{}-{nanos}", std::process::id()));
  std::fs::create_dir_all(&dir).unwrap();
  dir
}

fn shell(sites: &str) -> PathBuf {
  let root = dir("shell");
  std::fs::create_dir_all(root.join("app/generated")).unwrap();
  std::fs::write(root.join("app/generated/plan.json"), r#"{"version":2,"routes":[]}"#).unwrap();
  std::fs::write(root.join("app.toml"), format!("[app]\ndir = \"app\"\n[session]\nkey = \"k\"\n{sites}")).unwrap();
  root
}

#[test]
fn a_version_resolves_under_the_root_and_a_path_stands_alone() {
  let root = shell("[sites]\nroot = \"sites\"\n[sites.billing]\nartifact = \"billing@1.2.0\"\n[sites.reports]\nartifact = \"elsewhere/reports\"\n");
  std::fs::create_dir_all(root.join("sites/billing/1.2.0")).unwrap();
  std::fs::write(root.join("sites/billing/1.2.0/a.txt"), "a").unwrap();
  std::fs::create_dir_all(root.join("elsewhere/reports")).unwrap();
  std::fs::write(root.join("elsewhere/reports/b.txt"), "b").unwrap();
  let config = Config::load(&root).unwrap();
  let resolved = resolve(&config).unwrap();
  assert_eq!(resolved.len(), 2);
  assert_eq!(resolved[0].name, "billing");
  assert_eq!(resolved[0].version, "1.2.0");
  assert!(resolved[0].artifact.ends_with("sites/billing/1.2.0"));
  assert_eq!(resolved[0].hash, hash_dir(&root.join("sites/billing/1.2.0")).unwrap());
  assert_eq!(resolved[1].version, "path");
  assert!(resolved[1].artifact.ends_with("elsewhere/reports"));
}

#[test]
fn a_pinned_hash_refuses_an_artifact_that_differs_and_a_version_needs_a_root() {
  let root = shell("[sites.billing]\nartifact = \"billing\"\nhash = \"0000000000000000\"\n");
  std::fs::create_dir_all(root.join("billing")).unwrap();
  std::fs::write(root.join("billing/a.txt"), "a").unwrap();
  let config = Config::load(&root).unwrap();
  let e = resolve(&config).unwrap_err().to_string();
  assert!(e.contains("sites.billing") && e.contains("pinned 0000000000000000"), "{e}");
  let root = shell("[sites.billing]\nartifact = \"billing@1\"\n");
  let e = Config::load(&root).map(|_| ()).unwrap_err().to_string();
  assert!(e.contains("needs sites.root"), "{e}");
}

#[test]
fn the_hash_follows_content_and_ignores_dot_entries() {
  let a = dir("hash-a");
  std::fs::write(a.join("x.txt"), "one").unwrap();
  std::fs::create_dir_all(a.join("sub")).unwrap();
  std::fs::write(a.join("sub/y.txt"), "two").unwrap();
  let first = hash_dir(&a).unwrap();
  std::fs::write(a.join(".hidden"), "ignored").unwrap();
  assert_eq!(hash_dir(&a).unwrap(), first);
  std::fs::write(a.join("sub/y.txt"), "three").unwrap();
  assert_ne!(hash_dir(&a).unwrap(), first);
}
