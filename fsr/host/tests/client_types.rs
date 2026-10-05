//! The client this host embeds type-checks clean against the declarations
//! `examples/conformance/adapters_react18` fetched, through `client/tsconfig.check.json`.
//! A missing checker or missing declarations fail the test; `FSR_SKIP_CLIENT_TYPES=1`
//! skips it on purpose.

use std::path::{Path, PathBuf};
use std::process::Command;

#[test]
fn the_embedded_client_type_checks_clean() {
  if std::env::var_os("FSR_SKIP_CLIENT_TYPES").is_some_and(|v| v == "1") {
    eprintln!("skipped: FSR_SKIP_CLIENT_TYPES=1");
    return;
  }
  let fsr = Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("host sits in fsr/");
  let snapfiretc = std::env::var_os("SNAPFIRETC").map(PathBuf::from).unwrap_or_else(|| fsr.join("../target/debug/snapfiretc"));
  assert!(snapfiretc.is_file(), "no snapfiretc at {}; `cargo build -p snapfire_typecheck` in the snapfire root builds it, or set FSR_SKIP_CLIENT_TYPES=1 to skip this check", snapfiretc.display());
  let declarations = fsr.join("examples/conformance/adapters_react18/app/types/.fsr-types.json");
  assert!(declarations.is_file(), "no declarations at {}; `fsr types app` in examples/conformance/adapters_react18 fetches them, or set FSR_SKIP_CLIENT_TYPES=1 to skip this check", declarations.display());
  let client = fsr.join("client");
  let out = Command::new(&snapfiretc).arg("--root").arg(&client).args(["--config", "tsconfig.check.json"]).output().expect("snapfiretc runs");
  let report = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
  assert!(out.status.success() && !report.contains("error TS"), "fsr/client does not type-check:\n{report}");
}
