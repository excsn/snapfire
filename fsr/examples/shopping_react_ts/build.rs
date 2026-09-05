fn main() {
  let app = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("app");
  for watched in ["routes", "src", "schemas", "clients", "importmap.json", "types"] {
    println!("cargo:rerun-if-changed={}", app.join(watched).display());
  }
  let mut options = snapfire_fsr_cli::DevOptions::beside(&app);
  options.snapfirec = snapfirec();
  snapfire_fsr_cli::emit(&app, options).unwrap_or_else(|e| panic!("fsr build app: {e}"));

  let clients = app.join("clients");
  let set = protox::compile([clients.join("inventory.proto")], [clients]).unwrap_or_else(|e| panic!("inventory.proto: {e}"));
  tonic_prost_build::configure().build_client(false).compile_fds(set).unwrap_or_else(|e| panic!("inventory.proto: {e}"));
}

/// The compiler that bundles the app, built from the snapfire workspace above
/// this one; `$SNAPFIREC` overrides it and `None` falls back to `PATH`.
fn snapfirec() -> Option<std::path::PathBuf> {
  if let Some(path) = std::env::var_os("SNAPFIREC") {
    return Some(path.into());
  }
  let root = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../../..");
  ["target/debug/snapfirec", "target/release/snapfirec"].iter().map(|p| root.join(p)).find(|p| p.is_file())
}
