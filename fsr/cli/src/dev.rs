//! `fsr dev`: generate, bundle and run, then redo whichever of those a change
//! calls for. A change under the app regenerates and rebundles; the server
//! restarts only when the generated files differ, since the bundle's output
//! names are stable and the host reads it from disk. A change to the project
//! around the app rebuilds and restarts. A failed step keeps the running
//! server, so a typo never takes the page down.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::time::Duration;

use notify::{RecommendedWatcher, RecursiveMode, Watcher};

use crate::xwpm::Layout;
use crate::{BuildError, Built, Options, build, write};

/// Editors save in bursts; batching until this much quiet has passed turns one save into one rebuild.
const SETTLE: Duration = Duration::from_millis(120);
const POLL: Duration = Duration::from_secs(1);

/// Written by the build or the bundle, never a reason to run either.
const IGNORED: &[&str] = &["generated", "dist", "types", "tsconfig.json", "tsconfig.build.json"];

pub struct DevOptions {
  pub build: Options,
  /// URL prefix the bundle is served under; the host infers its static root from it.
  pub public_path: String,
  /// The compiler to bundle with; beside this binary or on PATH when absent.
  pub snapfirec: Option<PathBuf>,
}

impl Default for DevOptions {
  fn default() -> Self {
    Self { build: Options::default(), public_path: "/static/js/app".to_owned(), snapfirec: None }
  }
}

/// The application and, when one wraps it, the Cargo project whose binary serves it.
struct Project {
  app: PathBuf,
  cargo: Option<PathBuf>,
  layout: Layout,
  snapfirec: PathBuf,
  options: DevOptions,
}

/// The compiler: as given, else `$SNAPFIREC`, else beside this binary, else on `PATH`.
pub(crate) fn find_snapfirec(explicit: Option<&Path>) -> PathBuf {
  match explicit {
    Some(path) => path.to_path_buf(),
    None if std::env::var_os("SNAPFIREC").is_some_and(|v| !v.is_empty()) => PathBuf::from(std::env::var_os("SNAPFIREC").unwrap()),
    None => {
      let beside = std::env::current_exe().ok().and_then(|exe| exe.parent().map(|d| d.join("snapfirec")));
      beside.filter(|p| p.is_file()).unwrap_or_else(|| PathBuf::from("snapfirec"))
    }
  }
}

impl Project {
  fn open(app: &Path, options: DevOptions) -> Result<Self, BuildError> {
    let app = app.canonicalize().map_err(|e| BuildError::Io(app.to_path_buf(), e))?;
    let cargo = app.parent().map(Path::to_path_buf).filter(|p| p.join("Cargo.toml").is_file());
    let layout = Layout::of(&app)?;
    let snapfirec = find_snapfirec(options.snapfirec.as_deref());
    Ok(Self { app, cargo, layout, snapfirec, options })
  }

  fn generate(&self) -> Result<Built, BuildError> {
    let built = build(&self.app, &self.options.build)?;
    write(&self.app, &built)?;
    Ok(built)
  }

  fn bundle(&self) -> Result<(), BuildError> {
    let status = Command::new(&self.snapfirec)
      .arg("--root")
      .arg(&self.app)
      .args(["--config", "tsconfig.build.json", "--source-map", "--public-path", &self.options.public_path, "--import-map", &self.layout.importmap])
      .status()
      .map_err(|e| BuildError::Dev(format!("{}: {e}; pass --snapfirec or put it on PATH", self.snapfirec.display())))?;
    if !status.success() {
      return Err(BuildError::Dev(format!("snapfirec exited with {status}")));
    }
    Ok(())
  }

  /// Tells the running server a bundle changed under it, so open documents
  /// refresh. Best effort: a server that is not up yet or has `dev` off
  /// simply does not hear it.
  fn notify_changed(&self) {
    let root = crate::serve::project_root(&self.app);
    let Ok(config) = snapfire_fsr_host::config::Config::load(&root) else { return };
    let listen = config.server.listen;
    let Ok(mut stream) = std::net::TcpStream::connect(&listen) else { return };
    let _ = stream.set_write_timeout(Some(Duration::from_secs(1)));
    let _ = std::io::Write::write_all(&mut stream, format!("POST /__fsr/changed HTTP/1.1\r\nHost: {listen}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes());
  }

  fn cargo_build(&self) -> Result<(), BuildError> {
    let Some(cargo) = &self.cargo else { return Ok(()) };
    let status = Command::new("cargo").arg("build").current_dir(cargo).status().map_err(|e| BuildError::Dev(format!("cargo build: {e}")))?;
    if !status.success() {
      return Err(BuildError::Dev(format!("cargo build exited with {status}")));
    }
    Ok(())
  }

  /// The project's own binary when there is one, else this binary's `serve` over the app.
  fn spawn(&self) -> Result<Child, BuildError> {
    match &self.cargo {
      Some(cargo) => Command::new("cargo").arg("run").current_dir(cargo).spawn().map_err(|e| BuildError::Dev(format!("cargo run: {e}"))),
      None => {
        let exe = std::env::current_exe().map_err(|e| BuildError::Dev(format!("fsr serve: {e}")))?;
        Command::new(exe).arg("serve").arg(&self.app).spawn().map_err(|e| BuildError::Dev(format!("fsr serve: {e}")))
      }
    }
  }

  /// What to watch besides the app: the Cargo project's sources, or the configuration the stock host reads.
  fn watched(&self) -> Vec<PathBuf> {
    match &self.cargo {
      Some(cargo) => ["src", "config", "build.rs", "Cargo.toml"].iter().map(|name| cargo.join(name)).collect(),
      None => {
        let root = crate::serve::project_root(&self.app);
        ["config", "app.toml", "app.yaml"].iter().map(|name| root.join(name)).collect()
      }
    }
  }

  /// Which of the three steps a set of changed paths calls for.
  fn classify(&self, changed: &[PathBuf]) -> Change {
    let mut change = Change::default();
    for path in changed {
      if let Ok(rel) = path.strip_prefix(&self.app) {
        let first = rel.components().next().map(|c| c.as_os_str().to_string_lossy().into_owned()).unwrap_or_default();
        if IGNORED.contains(&first.as_str()) || first.starts_with(".fsr-") || first == "tests" || rel.to_string_lossy().ends_with(".test.ts") {
          continue;
        }
        change.app = true;
      } else {
        change.project = true;
      }
    }
    change
  }
}

enum Msg {
  Fs(notify::Result<notify::Event>),
  Stop,
}

#[derive(Default)]
struct Change {
  app: bool,
  project: bool,
}

struct Server {
  child: Option<Child>,
}

impl Server {
  fn restart(&mut self, project: &Project) {
    self.stop();
    match project.spawn() {
      Ok(child) => {
        println!("dev: server started, pid {}", child.id());
        self.child = Some(child);
      }
      Err(e) => eprintln!("dev: {e}"),
    }
  }

  fn stop(&mut self) {
    if let Some(mut child) = self.child.take() {
      let _ = child.kill();
      let _ = child.wait();
    }
  }

  /// Notices a server that exited on its own, so the next change starts one.
  fn poll(&mut self) {
    if let Some(child) = &mut self.child {
      if let Ok(Some(status)) = child.try_wait() {
        eprintln!("dev: server exited with {status}; the next change starts it again");
        self.child = None;
      }
    }
  }
}

impl Drop for Server {
  fn drop(&mut self) {
    self.stop();
  }
}

pub fn run(app: &Path, options: DevOptions) -> Result<(), BuildError> {
  let project = Project::open(app, options)?;
  let (tx, rx) = channel::<Msg>();
  let stop: Sender<Msg> = tx.clone();
  ctrlc::set_handler(move || {
    let _ = stop.send(Msg::Stop);
  })
  .map_err(|e| BuildError::Dev(format!("signal handler: {e}")))?;
  let fs = tx;
  let mut watcher = RecommendedWatcher::new(move |event| {
    let _ = fs.send(Msg::Fs(event));
  }, notify::Config::default())
  .map_err(|e| BuildError::Dev(format!("watcher: {e}")))?;
  watcher.watch(&project.app, RecursiveMode::Recursive).map_err(|e| BuildError::Dev(format!("watch {}: {e}", project.app.display())))?;
  for path in project.watched() {
    if path.exists() {
      watcher.watch(&path, RecursiveMode::Recursive).map_err(|e| BuildError::Dev(format!("watch {}: {e}", path.display())))?;
    }
  }

  let mut server = Server { child: None };
  let mut files: Option<Vec<(String, String)>> = None;
  match &project.cargo {
    Some(cargo) => println!("dev: watching {} and the project at {}; press Ctrl-C to stop", project.app.display(), cargo.display()),
    None => println!("dev: watching {} and its configuration, served by the stock host; press Ctrl-C to stop", project.app.display()),
  }

  let mut want = Change { app: true, project: true };
  loop {
    if want.app || want.project {
      let mut restart = want.project || server.child.is_none();
      let mut failed = false;
      if want.app {
        match project.generate() {
          Ok(built) => {
            let changed = files.as_ref() != Some(&built.files);
            if changed {
              print!("{}", built.report);
              files = Some(built.files);
            }
            restart |= changed;
          }
          Err(e) => {
            eprintln!("{e}");
            failed = true;
          }
        }
        if !failed {
          if let Err(e) = project.bundle() {
            eprintln!("{e}");
            failed = true;
          }
        }
      }
      if !failed && restart {
        match project.cargo_build() {
          Ok(()) => server.restart(&project),
          Err(e) => {
            eprintln!("{e}");
            failed = true;
          }
        }
      } else if !failed {
        project.notify_changed();
      }
      if failed {
        println!("dev: waiting for changes");
      }
    }
    match collect(&rx, &mut server) {
      Some(changed) => want = project.classify(&changed),
      None => {
        println!("dev: stopping");
        return Ok(());
      }
    }
  }
}

/// Blocks for the first event, polling the server meanwhile, then keeps
/// draining until the filesystem has been quiet for `SETTLE`. `None` on a
/// stop signal or once the watcher has hung up.
fn collect(rx: &Receiver<Msg>, server: &mut Server) -> Option<Vec<PathBuf>> {
  let mut paths: HashSet<PathBuf> = HashSet::new();
  loop {
    match rx.recv_timeout(POLL) {
      Ok(Msg::Fs(event)) => {
        absorb(event, &mut paths);
        break;
      }
      Ok(Msg::Stop) | Err(RecvTimeoutError::Disconnected) => return None,
      Err(RecvTimeoutError::Timeout) => server.poll(),
    }
  }
  loop {
    match rx.recv_timeout(SETTLE) {
      Ok(Msg::Fs(event)) => absorb(event, &mut paths),
      Ok(Msg::Stop) | Err(RecvTimeoutError::Disconnected) => return None,
      Err(RecvTimeoutError::Timeout) => break,
    }
  }
  Some(paths.into_iter().collect())
}

fn absorb(event: notify::Result<notify::Event>, paths: &mut HashSet<PathBuf>) {
  match event {
    Ok(event) => paths.extend(event.paths),
    Err(e) => eprintln!("dev: watch error: {e}"),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn project(app: &Path) -> Project {
    Project { app: app.to_path_buf(), cargo: Some(app.parent().unwrap().to_path_buf()), layout: Layout::default(), snapfirec: PathBuf::from("snapfirec"), options: DevOptions::default() }
  }

  #[test]
  fn generated_output_is_never_a_reason_to_rebuild() {
    let p = project(Path::new("/p/app"));
    let change = p.classify(&[PathBuf::from("/p/app/generated/plan.json"), PathBuf::from("/p/app/dist/src/main.js"), PathBuf::from("/p/app/types/react/index.d.ts"), PathBuf::from("/p/app/tsconfig.json"), PathBuf::from("/p/app/.fsr-dev"), PathBuf::from("/p/app/tests/cart/loader.test.ts"), PathBuf::from("/p/app/routes/cart/loader.test.ts")]);
    assert!(!change.app && !change.project);
  }

  #[test]
  fn app_sources_and_project_sources_are_told_apart() {
    let p = project(Path::new("/p/app"));
    let change = p.classify(&[PathBuf::from("/p/app/routes/index/page.tsx")]);
    assert!(change.app && !change.project);
    let change = p.classify(&[PathBuf::from("/p/src/main.rs"), PathBuf::from("/p/config/app.toml")]);
    assert!(!change.app && change.project);
    let change = p.classify(&[PathBuf::from("/p/app/.fsr-something"), PathBuf::from("/p/app/importmap.json")]);
    assert!(change.app);
  }
}
