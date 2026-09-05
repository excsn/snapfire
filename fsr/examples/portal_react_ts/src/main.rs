use std::sync::Arc;

use snapfire_fsr_host::Host;

/// The portal: its own routes, and the sites its configuration mounts, all
/// served by one host. Everything under `app/` is TypeScript the build lowers.
#[tokio::main]
async fn main() -> std::io::Result<()> {
  let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
  let builder = Host::from(&root).map_err(std::io::Error::other)?;
  let builder = snapfire_fsr_sites::mount_all(builder).map_err(std::io::Error::other)?;
  let reload_root = root.clone();
  let host = builder
    .reloader(move || snapfire_fsr_sites::mount_all(Host::from(&reload_root)?).map_err(|e| snapfire_fsr_host::HostError::Value("sites".to_owned(), e.to_string())))
    .build()
    .map_err(std::io::Error::other)?;
  let host = Arc::new(host);
  print!("{}", host.report());
  let listen = host.listen().to_owned();
  println!("portal on http://{listen}/");
  let poll = snapfire_fsr_host::Config::load(&root).ok().and_then(|c| snapfire_fsr_sites::poll_of(&c));
  snapfire_fsr_sites::watch(host.clone(), root, poll);
  host.serve(&listen).await
}
