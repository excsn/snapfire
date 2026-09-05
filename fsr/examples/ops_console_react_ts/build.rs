fn main() {
  let app = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("app");
  for watched in ["routes", "schemas", "clients", "importmap.json", "types"] {
    println!("cargo:rerun-if-changed={}", app.join(watched).display());
  }
  let built = snapfire_fsr_cli::build(&app, &snapfire_fsr_cli::Options::default()).unwrap_or_else(|e| panic!("fsr build app: {e}"));
  snapfire_fsr_cli::write(&app, &built).unwrap_or_else(|e| panic!("fsr build app: {e}"));
}
