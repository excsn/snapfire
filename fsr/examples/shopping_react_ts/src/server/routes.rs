use std::path::PathBuf;

use snapfire_fsr::Plan;

/// Where `fsr build app` writes the plan file: routes, lowered loaders and
/// lowered actions. `build.rs` runs that build, so the file exists whenever
/// the crate compiles.
pub fn plan_path() -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("app/generated/plan.json")
}

pub fn plan() -> String {
  let path = plan_path();
  std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}; run `fsr build app`", path.display()))
}

/// A route the file system convention does not describe, added in Rust beside
/// the ones the plan file carries.
pub fn about_plan() -> Plan {
  Plan::of("shell#document").slot("content", Plan::of("src/About.tsx#default"))
}
