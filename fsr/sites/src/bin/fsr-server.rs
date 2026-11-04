//! `fsr-server [<app dir>] [--listen <addr>]`: the stock host over an application with no crate of its own, in production. The app directory defaults to `app` beside the binary, which is where `fsr bundle` lays it out.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

const USAGE: &str = "usage: fsr-server [<app dir>] [--listen <addr>]";

fn main() -> ExitCode {
  let mut app: Option<PathBuf> = None;
  let mut listen: Option<String> = None;
  let mut args = std::env::args().skip(1);
  while let Some(arg) = args.next() {
    match arg.as_str() {
      "--listen" => match args.next() {
        Some(addr) => listen = Some(addr),
        None => return refused("--listen needs an address"),
      },
      "--help" | "-h" => {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
      }
      other if other.starts_with('-') => return refused(&format!("unknown flag {other}")),
      other if app.is_none() => app = Some(PathBuf::from(other)),
      other => return refused(&format!("one app directory, not also {other}")),
    }
  }
  if std::env::var_os("RELEASE_ENV").is_none() {
    // Single-threaded here: nothing else reads the environment yet.
    unsafe { std::env::set_var("RELEASE_ENV", "production") };
  }
  let app = match app {
    Some(app) => app,
    None => match std::env::current_exe() {
      Ok(exe) => exe.parent().map(|dir| dir.join("app")).unwrap_or_else(|| PathBuf::from("app")),
      Err(_) => PathBuf::from("app"),
    },
  };
  let root = snapfire_fsr_sites::serve::project_root(&app);
  match snapfire_fsr_host::config::Config::load(&root) {
    Ok(config) if config.dev() => return refused("server.dev is on: fsr-server serves an application in production, and `fsr dev` or `fsr serve` serves one in development"),
    Ok(_) => {}
    Err(e) => return refused(&e.to_string()),
  }
  let (host, _traces, _logging) = match snapfire_fsr_sites::serve::host_of(&app, true, &[]) {
    Ok(built) => built,
    Err(e) => return refused(&e.to_string()),
  };
  let host = Arc::new(host);
  print!("{}", host.report().brief());
  let listen = listen.unwrap_or_else(|| host.listen().to_owned());
  let scheme = if host.report().tls.is_some() { "https" } else { "http" };
  let runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
    Ok(runtime) => runtime,
    Err(e) => return refused(&format!("runtime: {e}")),
  };
  let served = runtime.block_on(snapfire_fsr_sites::serve::listen(host, root, &listen, |at| {
    println!("fsr server on {scheme}://{at}/");
    Ok(())
  }));
  match served {
    Ok(()) => ExitCode::SUCCESS,
    Err(e) => refused(&format!("serve {listen}: {e}")),
  }
}

fn refused(message: &str) -> ExitCode {
  eprintln!("fsr-server: {message}");
  ExitCode::from(2)
}
