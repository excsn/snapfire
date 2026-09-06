use std::path::{Path, PathBuf};

use snapfire_fsr_host::Host;

/// The handbook has no server. It builds the host to render, writes every
/// fixed route and every static root into `site/` and exits, so what serves
/// the directory afterwards can be anything. The output sits beside the app
/// rather than under it, since one of the static roots is `app/dist` itself.
/// `fsr dev app` is still the authoring loop.
#[tokio::main]
async fn main() -> std::io::Result<()> {
  let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
  let host = Host::from(&root).and_then(|builder| builder.build()).map_err(std::io::Error::other)?;
  print!("{}", host.report());

  let out = root.join("site");
  let written = host.prerender(&out).await.map_err(std::io::Error::other)?;
  for (pattern, file) in &written {
    println!("wrote     {pattern:<22} {}", file.display());
  }

  for (route, dir) in &host.report().statics {
    let into = out.join(route.trim_start_matches('/'));
    let count = copy_into(dir, &into)?;
    println!("copied    {route:<22} {count} files from {}", dir.display());
  }

  let mut routes: Vec<&str> = written.iter().map(|(pattern, _)| pattern.as_str()).collect();
  routes.dedup();
  println!("{} routes in {} files: serve {} with anything", routes.len(), written.len(), out.display());
  Ok(())
}

/// Every file under `from`, into `to`, and how many. A static root the
/// configuration names but nothing wrote is nothing to copy.
fn copy_into(from: &Path, to: &Path) -> std::io::Result<usize> {
  if !from.is_dir() {
    return Ok(0);
  }
  std::fs::create_dir_all(to)?;
  let mut count = 0;
  for entry in std::fs::read_dir(from)? {
    let entry = entry?;
    let (source, target) = (entry.path(), to.join(entry.file_name()));
    if source.is_dir() {
      count += copy_into(&source, &target)?;
    } else {
      std::fs::copy(&source, &target)?;
      count += 1;
    }
  }
  Ok(count)
}
