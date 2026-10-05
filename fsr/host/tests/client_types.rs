//! The client this host embeds type-checks clean against the declarations
//! `examples/adapters_react18_ts` fetched, through `client/tsconfig.check.json`.

use std::path::{Path, PathBuf};
use std::process::Command;

#[test]
fn the_embedded_client_type_checks_clean() {
  let fsr = Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("host sits in fsr/");
  let snapfiretc = std::env::var_os("SNAPFIRETC").map(PathBuf::from).unwrap_or_else(|| fsr.join("../target/debug/snapfiretc"));
  if !snapfiretc.is_file() {
    eprintln!("skipped: no snapfiretc at {}; `cargo build -p snapfire_typecheck` in the snapfire root builds it", snapfiretc.display());
    return;
  }
  let declarations = fsr.join("examples/adapters_react18_ts/app/types/.fsr-types.json");
  if !declarations.is_file() {
    eprintln!("skipped: no declarations at {}; `fsr types app` in examples/adapters_react18_ts fetches them", declarations.display());
    return;
  }
  let client = fsr.join("client");
  let out = Command::new(&snapfiretc).arg("--root").arg(&client).args(["--config", "tsconfig.check.json"]).output().expect("snapfiretc runs");
  let report = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
  assert!(out.status.success() && !report.contains("error TS"), "fsr/client does not type-check:\n{report}");
}
