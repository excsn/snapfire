fn main() {
  let clients = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("app/clients");
  println!("cargo:rerun-if-changed={}", clients.display());
  let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
  let fsr = snapfire_fsr_service::fsr_proto_include(&out).unwrap_or_else(|e| panic!("snapfire/fsr.proto: {e}"));
  let set = protox::compile([clients.join("inventory.proto")], [clients, fsr]).unwrap_or_else(|e| panic!("inventory.proto: {e}"));
  tonic_prost_build::configure().build_client(false).compile_fds(set).unwrap_or_else(|e| panic!("inventory.proto: {e}"));
}
