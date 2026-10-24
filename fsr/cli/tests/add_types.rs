//! `fsr add` against esm.sh and registry.npmjs.org. When either does not answer, the test says so on the terminal and passes; the line is written past libtest's capture, so a skip is never silent.

use std::process::Command;

use snapfire_fsr_cli::new::{create, NewOptions};
use snapfire_fsr_test_internal::Probe;

const SERVICES: [&str; 2] = ["https://esm.sh/clsx@2.1.1", "https://registry.npmjs.org/clsx/2.1.1"];

fn services_answer(test: &str) -> bool {
  snapfire_fsr_test_internal::answers(test, &SERVICES.map(Probe::Get))
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
