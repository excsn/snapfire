//! `fsr add` against esm.sh and registry.npmjs.org. When either does not answer, the test says so on the terminal and passes; the line is written past libtest's capture, so a skip is never silent.

use std::io::Write;
use std::process::Command;

use snapfire_fsr_cli::new::{create, NewOptions};

const SERVICES: [&str; 2] = ["https://esm.sh/clsx@2.1.1", "https://registry.npmjs.org/clsx/2.1.1"];

fn services_answer(test: &str) -> bool {
  let client = reqwest::blocking::Client::builder().timeout(std::time::Duration::from_secs(10)).build().unwrap();
  for url in SERVICES {
    let why = match client.get(url).send() {
      Ok(response) if response.status().is_success() => continue,
      Ok(response) => format!("HTTP {}", response.status()),
      Err(e) => e.to_string(),
    };
    let _ = writeln!(std::io::stderr(), "SKIPPED {test}: {url} did not answer ({why})");
    return false;
  }
  true
}

#[test]
fn a_package_fsr_add_vendors_has_its_types_fetched_too() {
  if !services_answer("a_package_fsr_add_vendors_has_its_types_fetched_too") {
    return;
  }
  let root = std::env::temp_dir().join(format!("fsr-add-types-{}", std::process::id()));
  let _ = std::fs::remove_dir_all(&root);
  create(&root, NewOptions::default().fetch(false)).unwrap();
  let app = root.join("app");
  let output = Command::new(env!("CARGO_BIN_EXE_fsr")).arg("add").arg(&app).arg("clsx@2.1.1").output().unwrap();
  let printed = String::from_utf8_lossy(&output.stdout);
  assert!(output.status.success(), "{printed}{}", String::from_utf8_lossy(&output.stderr));
  assert!(printed.lines().any(|l| l.starts_with("types") && l.contains("clsx")), "fsr add names the types it fetched: {printed}");
  let manifest = std::fs::read_to_string(app.join("types").join(snapfire_fsr_cli::types::TYPES_MANIFEST)).unwrap_or_default();
  assert!(manifest.contains("\"clsx\""), "{manifest}");
  std::fs::remove_dir_all(&root).unwrap();
}
