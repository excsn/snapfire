use std::path::{Path, PathBuf};

use snapfire_fsr_host::Host;

/// The handbook has no server. This is its `[build] after`: it builds the host to render, writes
/// every fixed route and every static root into `site/` and exits, so what serves the directory
/// afterwards can be anything. The output sits beside the app rather than under it, since one of
/// the static roots is `app/dist` itself.
#[tokio::main]
async fn main() -> std::io::Result<()> {
  let root = project_root();
  // Events out to fibre_logging's appenders, spans kept for `/__fsr/traces`.
  let logging = root.join("fibre_logging.yaml");
  let (traces, _logging, why) = snapfire_fsr_host::trace::observe(&logging);
  if let Some(why) = why {
    eprintln!("logging disabled: {why}");
  }
  let host = Host::from(&root).and_then(|builder| builder.traces(traces).build()).map_err(std::io::Error::other)?;
  print!("{}", host.report());

  let out = root.join("site");
  let written = host.prerender(&out).await.map_err(std::io::Error::other)?;
  let client = written.iter().filter(|(pattern, _)| pattern.starts_with("/static/js/fsr/")).count();
  for (pattern, file) in written.iter().filter(|(pattern, _)| !pattern.starts_with("/static/js/fsr/")) {
    println!("wrote     {pattern:<22} {}", file.display());
  }
  if client > 0 {
    println!("wrote     {:<22} {client} modules the documents load, under {}", "/static/js/fsr", out.join("static/js/fsr").display());
  }

  for (route, dir) in &host.report().statics {
    let into = out.join(route.trim_start_matches('/'));
    let count = copy_into(dir, &into)?;
    println!("copied    {route:<22} {count} files from {}", dir.display());
  }

  let mut routes: Vec<&str> = written.iter().map(|(pattern, _)| pattern.as_str()).filter(|p| !p.starts_with("/static/js/fsr/") && !p.ends_with(".json")).collect();
  routes.dedup();
  println!("{} routes in {} files: serve {} with anything", routes.len(), written.len(), out.display());
  Ok(())
}

/// The handbook's root: `FSR_ROOT` when `fsr` runs this, else the directory above this crate.
fn project_root() -> PathBuf {
  std::env::var_os("FSR_ROOT").map(PathBuf::from).unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join(".."))
}

/// Every file under `from`, into `to` and how many. A static root the
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
