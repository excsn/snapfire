use std::sync::Arc;

use futures_util::future::BoxFuture;
use http::StatusCode;
use snapfire_fsr_host::{Candidate, Host, ReloadCheck};
use snapfire_fsr_runtime::SessionCell;

/// The portal's own check over every candidate a reload produces: the
/// directory must still list teams on the home page.
struct TeamsListed;

impl ReloadCheck for TeamsListed {
  fn check<'a>(&'a self, candidate: &'a Candidate<'a>) -> BoxFuture<'a, Result<(), String>> {
    Box::pin(async move {
      let (status, html) = candidate.render("/", SessionCell::default()).await.map_err(|e| e.to_string())?;
      if status != StatusCode::OK || html.contains("0 teams") {
        return Err(format!("/ answered {status} listing no teams"));
      }
      Ok(())
    })
  }
}

/// The portal: its own routes and the sites its configuration mounts, all
/// served by one host. Everything under `app/` is TypeScript the build lowers.
#[tokio::main]
async fn main() -> std::io::Result<()> {
  // Events out to fibre_logging's appenders, spans kept for `/__fsr/traces`.
  let logging = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fibre_logging.yaml");
  let (traces, _logging, why) = snapfire_fsr_host::trace::observe(&logging);
  if let Some(why) = why {
    eprintln!("logging disabled: {why}");
  }
  let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
  let builder = Host::from(&root).map_err(std::io::Error::other)?.traces(traces).reload_check(Arc::new(TeamsListed));
  let builder = snapfire_fsr_sites::mount_all(snapfire_fsr_sites::mountable(builder)).map_err(std::io::Error::other)?;
  let host = builder.build().map_err(std::io::Error::other)?;
  let host = Arc::new(host);
  print!("{}", host.report());
  let listen = host.listen().to_owned();
  println!("portal on http://{listen}/");
  let poll = snapfire_fsr_host::Config::load(&root).ok().and_then(|c| snapfire_fsr_sites::poll_of(&c));
  snapfire_fsr_sites::watch(host.clone(), root, poll);
  host.serve(&listen).await
}
