//! `fsr dev`: generate, bundle and run, then redo whichever of those a change
//! calls for. A change under an application regenerates and rebundles it;
//! when the generated files differ the running server reloads its tables in
//! place and restarts only when it cannot, since the bundle's output names
//! are stable and the host reads it from disk. A change to the project around
//! the applications rebuilds and restarts. A failed step keeps the running
//! server, so a typo never takes the page down.
//!
//! The loop builds every application the shell mounts from a working tree as
//! well as the shell and the project's build script is told so through
//! [`OWNS_BUILD`], since a script that built them too would build each one
//! twice per change and write over what the loop wrote.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use fibre::{RecvErrorTimeout, TryRecvError};
use fibre::mpsc::{UnboundedSyncReceiver, UnboundedSyncSender, unbounded};
use std::time::Duration;

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use snapfire_fsr_host::config::Config;

use crate::typecheck::{self, Checked, Typecheck};
use crate::xwpm::Layout;
use crate::{BuildError, Built, Options, build, write};

/// Editors save in bursts; batching until this much quiet has passed turns one save into one rebuild.
const SETTLE: Duration = Duration::from_millis(120);
const POLL: Duration = Duration::from_secs(1);

/// Written by the build or the bundle, never a reason to run either.
const IGNORED: &[&str] = &["generated", "dist", "types", "tsconfig.json", "tsconfig.build.json"];

/// Set in the environment of every cargo command `fsr dev` runs. A build script that calls
/// [`emit`] skips it while this is set: the loop has already built every application and a
/// script building them again would write over that and cost a second bundle per change.
pub const OWNS_BUILD: &str = "FSR_DEV_OWNS_BUILD";

/// Whether the build this process runs under is driven by `fsr dev`.
pub fn owns_build() -> bool {
  std::env::var_os(OWNS_BUILD).is_some_and(|v| !v.is_empty())
}

/// Rebuilds in a row caused by nothing the loop did not write itself before it stops and waits
/// for an edit instead.
const SELF_TRIGGERED_LIMIT: u32 = 3;

pub struct DevOptions {
  pub build: Options,
  /// URL prefix the bundle is served under; the host infers its static root from it.
  pub public_path: String,
  /// The compiler to bundle with; beside this binary or on PATH when absent.
  pub snapfirec: Option<PathBuf>,
  /// Which TypeScript checks the application and whether it is checked at all.
  pub typecheck: Typecheck,
}

impl Default for DevOptions {
  fn default() -> Self {
    Self { build: Options::default(), public_path: "/static/js/app".to_owned(), snapfirec: None, typecheck: Typecheck { enabled: true, ..Typecheck::default() } }
  }
}

impl DevOptions {
  /// The defaults for `app`, its `[site]` read from the configuration beside
  /// it: a site's bundle is served under its prefix.
  pub fn beside(app: &Path) -> Self {
    let build = Options::beside(app);
    let public_path = match &build.site {
      Some(site) => format!("{}/static/js/app", site.at.trim_end_matches('/')),
      None => "/static/js/app".to_owned(),
    };
    Self { build, public_path, snapfirec: None, typecheck: Typecheck::beside(app) }
  }
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

/// One application the loop builds: the shell or a site mounted into it from a working tree.
struct App {
  dir: PathBuf,
  layout: Layout,
  snapfirec: PathBuf,
  options: DevOptions,
}

impl App {
  fn open(dir: &Path, options: DevOptions) -> Result<Self, BuildError> {
    let dir = dir.canonicalize().map_err(|e| BuildError::Io(dir.to_path_buf(), e))?;
    let layout = Layout::of(&dir)?;
    let snapfirec = find_snapfirec(options.snapfirec.as_deref());
    Ok(Self { dir, layout, snapfirec, options })
  }

  fn generate(&self) -> Result<Built, BuildError> {
    let built = build(&self.dir, &self.options.build)?;
    write(&self.dir, &built)?;
    Ok(built)
  }

  /// The compiler invocation both the one-shot bundle and the driven child are built from.
  fn compiler(&self) -> Command {
    let mut command = Command::new(&self.snapfirec);
    command
      .arg("--root")
      .arg(&self.dir)
      .args(["--config", "tsconfig.build.json", "--source-map", "--public-path", &self.options.public_path, "--import-map", &self.layout.importmap]);
    command
  }

  fn bundle(&self) -> Result<(), BuildError> {
    crate::install::ensure(&crate::install::COMPILER, &self.snapfirec)?;
    let mut command = self.compiler();
    if self.dir.join(BUNDLE_OVERLAY).is_dir() {
      command.args(["--overlay", BUNDLE_OVERLAY]);
    }
    let status = command
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
    let root = crate::serve::project_root(&self.dir);
    let Ok(config) = Config::load(&root) else { return };
    let listen = config.server.listen;
    let Ok(mut stream) = std::net::TcpStream::connect(&listen) else { return };
    let _ = stream.set_write_timeout(Some(Duration::from_secs(1)));
    let _ = std::io::Write::write_all(&mut stream, format!("POST /__fsr/changed HTTP/1.1\r\nHost: {listen}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes());
  }

  /// Asks the running server to rebuild its tables from disk. `Err` when it
  /// is not up, has `dev` off, has no reloader or refused and the caller
  /// restarts it instead. A server still coming up is given [`REACH`] to
  /// start listening.
  fn reload(&self) -> Result<String, String> {
    let root = crate::serve::project_root(&self.dir);
    let config = Config::load(&root).map_err(|e| e.to_string())?;
    let listen = config.server.listen;
    let mut stream = connect(&listen).map_err(|e| e.to_string())?;
    let _ = stream.set_write_timeout(Some(Duration::from_secs(1)));
    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
    std::io::Write::write_all(&mut stream, format!("POST /__fsr/reload HTTP/1.1\r\nHost: {listen}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes()).map_err(|e| e.to_string())?;
    let mut response = String::new();
    std::io::Read::read_to_string(&mut stream, &mut response).map_err(|e| e.to_string())?;
    let (head, body) = response.split_once("\r\n\r\n").unwrap_or((response.as_str(), ""));
    let status = head.split_whitespace().nth(1).unwrap_or("");
    if status == "200" {
      Ok(body.to_owned())
    } else {
      Err(body.trim().to_owned())
    }
  }

  /// The bundle and the typecheck, which read none of each other's output
  /// and so run at once. A checker that is not installed is not an error;
  /// its absence is reported by the caller.
  fn compile(&self) -> Result<Option<Checked>, BuildError> {
    self.compile_with(|| self.bundle())
  }

  fn compile_with(&self, bundle: impl FnOnce() -> Result<(), BuildError>) -> Result<Option<Checked>, BuildError> {
    let checker = typecheck::spawn(&self.dir, &self.options.typecheck)?;
    let bundled = bundle();
    let checked = typecheck::finish(checker, &self.options.typecheck);
    bundled?;
    checked
  }
}

/// How long a reload waits for a server that was just started to listen.
const REACH: Duration = Duration::from_secs(10);

fn connect(listen: &str) -> std::io::Result<std::net::TcpStream> {
  let until = std::time::Instant::now() + REACH;
  loop {
    match std::net::TcpStream::connect(listen) {
      Err(e) if e.kind() == std::io::ErrorKind::ConnectionRefused && std::time::Instant::now() < until => {
        std::thread::sleep(Duration::from_millis(250));
      }
      outcome => return outcome,
    }
  }
}

/// The shell, the sites its configuration mounts from a working tree and, when one wraps them,
/// the Cargo project whose binary serves them.
struct Project {
  /// The shell first, then each site.
  apps: Vec<App>,
  cargo: Option<PathBuf>,
}

impl Project {
  fn open(app: &Path, options: DevOptions) -> Result<Self, BuildError> {
    let shell = App::open(app, options)?;
    let cargo = shell.dir.parent().map(Path::to_path_buf).filter(|p| p.join("Cargo.toml").is_file());
    let sites = site_apps(&shell.dir);
    let mut apps = vec![shell];
    for dir in sites {
      let mut options = DevOptions::beside(&dir);
      options.snapfirec = Some(apps[0].snapfirec.clone());
      apps.push(App::open(&dir, options)?);
    }
    Ok(Self { apps, cargo })
  }

  fn shell(&self) -> &App {
    &self.apps[0]
  }

  /// Where a changed path is reported relative to.
  fn root(&self) -> PathBuf {
    self.cargo.clone().unwrap_or_else(|| crate::serve::project_root(&self.shell().dir))
  }

  /// Builds the project and returns what its build scripts declared they read and the binary it
  /// produced. Cargo renders the diagnostics itself, so the JSON on stdout is the artifact
  /// messages alone.
  fn cargo_build(&self) -> Result<CargoBuilt, BuildError> {
    let Some(cargo) = &self.cargo else { return Ok(CargoBuilt::default()) };
    let mut child = Command::new("cargo")
      .args(["build", "--message-format=json-render-diagnostics"])
      .env(OWNS_BUILD, "1")
      .current_dir(cargo)
      .stdout(Stdio::piped())
      .spawn()
      .map_err(|e| BuildError::Dev(format!("cargo build: {e}")))?;
    let stdout = child.stdout.take().expect("a piped stdout");
    let mut built = CargoBuilt::default();
    let mut executables = Vec::new();
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
      built.declared.extend(inputs_of(cargo, &line));
      executables.extend(executable_of(&line));
    }
    let status = child.wait().map_err(|e| BuildError::Dev(format!("cargo build: {e}")))?;
    if !status.success() {
      return Err(BuildError::Dev(format!("cargo build exited with {status}")));
    }
    if let [executable] = executables.as_slice() {
      built.executable = Some(executable.clone());
    }
    Ok(built)
  }

  /// The binary cargo just built when it built exactly one, else `cargo run` over the project,
  /// else this binary's `serve` over the shell. Running the binary itself keeps cargo from
  /// looking at the build script's inputs a second time, which a script that wrote one of them
  /// makes a rebuild.
  fn spawn(&self, executable: Option<&Path>) -> Result<Child, BuildError> {
    match (&self.cargo, executable) {
      (Some(cargo), Some(executable)) => Command::new(executable).current_dir(cargo).spawn().map_err(|e| BuildError::Dev(format!("{}: {e}", executable.display()))),
      (Some(cargo), None) => Command::new("cargo").arg("run").env(OWNS_BUILD, "1").current_dir(cargo).spawn().map_err(|e| BuildError::Dev(format!("cargo run: {e}"))),
      (None, _) => {
        let exe = std::env::current_exe().map_err(|e| BuildError::Dev(format!("fsr serve: {e}")))?;
        Command::new(exe).arg("serve").arg(&self.shell().dir).spawn().map_err(|e| BuildError::Dev(format!("fsr serve: {e}")))
      }
    }
  }

  /// What to watch besides the applications: the Cargo project's sources or the configuration the stock host reads.
  fn watched(&self) -> Vec<PathBuf> {
    match &self.cargo {
      Some(cargo) => ["src", "config", "build.rs", "Cargo.toml"].iter().map(|name| cargo.join(name)).collect(),
      None => {
        let root = crate::serve::project_root(&self.shell().dir);
        ["config", "app.toml", "app.yaml"].iter().map(|name| root.join(name)).collect()
      }
    }
  }

  /// Which application each changed path belongs to, else that it is the project around them.
  fn classify(&self, changed: &[PathBuf]) -> Change {
    let mut change = Change { apps: vec![None; self.apps.len()], project: false };
    for path in changed {
      let owner = self.apps.iter().enumerate().filter(|(_, app)| path.starts_with(&app.dir)).max_by_key(|(_, app)| app.dir.as_os_str().len());
      match owner {
        Some((i, app)) => {
          let rel = path.strip_prefix(&app.dir).expect("a prefix");
          if !ignored(rel) {
            change.apps[i].get_or_insert_with(Vec::new).push(path.clone());
          }
        }
        None => change.project = true,
      }
    }
    change
  }
}

fn ignored(rel: &Path) -> bool {
  let first = rel.components().next().map(|c| c.as_os_str().to_string_lossy().into_owned()).unwrap_or_default();
  IGNORED.contains(&first.as_str()) || first.starts_with(".fsr-") || first == "tests" || rel.to_string_lossy().ends_with(".test.ts")
}

/// The app directory of every site the shell's configuration mounts from a path: those are
/// working trees the loop builds. A `name@version` row is a release and is served as it is.
fn site_apps(shell: &Path) -> Vec<PathBuf> {
  let root = crate::serve::project_root(shell);
  let Ok(config) = Config::load(&root) else { return Vec::new() };
  let resolved = match snapfire_fsr_sites::resolve(&config) {
    Ok(resolved) => resolved,
    Err(e) => {
      eprintln!("dev: sites not built: {e}");
      return Vec::new();
    }
  };
  resolved
    .into_iter()
    .filter(|site| site.version == "path")
    .filter_map(|site| match Config::load(&site.artifact) {
      Ok(config) => Some(config.app),
      Err(e) => {
        eprintln!("dev: site {} not built: {e}", site.name);
        None
      }
    })
    .collect()
}

enum Msg {
  Fs(notify::Result<notify::Event>),
  Stop,
}

#[derive(Default)]
struct CargoBuilt {
  /// What the project's build scripts declared they read.
  declared: Vec<PathBuf>,
  /// The one binary the build produced, when it produced exactly one.
  executable: Option<PathBuf>,
}

#[derive(Default)]
struct Change {
  /// Per application, index aligned with `Project::apps`: untouched or the sources that
  /// changed, none of them meaning everything.
  apps: Vec<Option<Vec<PathBuf>>>,
  project: bool,
}

impl Change {
  fn everything(apps: usize) -> Self {
    Self { apps: vec![Some(Vec::new()); apps], project: true }
  }

  fn any(&self) -> bool {
    self.project || self.apps.iter().any(Option::is_some)
  }
}

/// What every watched file held the last time the loop looked, so an event for a file whose
/// content is what it was is not a change: a build that rewrote its own output byte for byte,
/// an editor that touched a file it did not alter. A file the loop has never looked at counts
/// as changed; so does one that is gone.
#[derive(Default)]
struct Seen {
  hashes: HashMap<PathBuf, u64>,
}

impl Seen {
  fn changed(&mut self, paths: Vec<PathBuf>) -> Vec<PathBuf> {
    paths.into_iter().filter(|path| self.differs(path)).collect()
  }

  fn differs(&mut self, path: &Path) -> bool {
    match std::fs::read(path) {
      Ok(bytes) => {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        bytes.hash(&mut hasher);
        let hash = hasher.finish();
        self.hashes.insert(path.to_path_buf(), hash) != Some(hash)
      }
      Err(_) if path.is_dir() => false,
      Err(_) => {
        self.hashes.remove(path);
        true
      }
    }
  }
}

/// The two lines of snapfirec's `--driven` protocol: one of them ends every batch.
const REBUILT: &str = "snapfirec: rebuilt";
const FAILED: &str = "snapfirec: failed";

/// The compiler held open for the life of `fsr dev`, told which paths changed and answering once it
/// has compiled them. One process means one resolved configuration and one selection, so a change
/// recompiles what changed instead of everything. Starting it is itself the first build.
struct Driven {
  child: Child,
  /// Taken when the child is gone, which is what tells a failed batch from a lost compiler.
  stdin: Option<ChildStdin>,
  stdout: BufReader<ChildStdout>,
}

impl Driven {
  fn start(app: &App) -> Result<Self, BuildError> {
    crate::install::ensure(&crate::install::COMPILER, &app.snapfirec)?;
    let mut child = app
      .compiler()
      .args(["--driven", "--overlay", BUNDLE_OVERLAY])
      .stdin(Stdio::piped())
      .stdout(Stdio::piped())
      .spawn()
      .map_err(|e| BuildError::Dev(format!("{}: {e}; pass --snapfirec or put it on PATH", app.snapfirec.display())))?;
    let stdin = child.stdin.take();
    let stdout = BufReader::new(child.stdout.take().expect("a piped stdout"));
    let mut driven = Self { child, stdin, stdout };
    match driven.settled() {
      Ok(()) => Ok(driven),
      Err(e) if driven.alive() => Err(e),
      Err(e) => match driven.child.wait().ok().filter(|status| !status.success()) {
        Some(status) => Err(BuildError::Dev(format!(
          "{} exited with {status} before its first build; `--driven` needs a newer snapfire_compiler than this one",
          app.snapfirec.display()
        ))),
        None => Err(e),
      },
    }
  }

  fn alive(&self) -> bool {
    self.stdin.is_some()
  }

  fn rebuild(&mut self, paths: &[PathBuf]) -> Result<(), BuildError> {
    let mut batch: String = paths.iter().map(|path| format!("{}\n", path.display())).collect();
    batch.push('\n');
    let Some(stdin) = self.stdin.as_mut() else {
      return Err(BuildError::Dev("snapfirec is gone".to_owned()));
    };
    if let Err(e) = stdin.write_all(batch.as_bytes()).and_then(|()| stdin.flush()) {
      self.stdin = None;
      return Err(BuildError::Dev(format!("snapfirec: {e}")));
    }
    self.settled()
  }

  /// Forwards the compiler's own output until it says the batch is compiled.
  fn settled(&mut self) -> Result<(), BuildError> {
    let mut line = String::new();
    loop {
      line.clear();
      match self.stdout.read_line(&mut line) {
        Ok(0) => {
          self.stdin = None;
          return Err(BuildError::Dev("snapfirec exited".to_owned()));
        }
        Ok(_) => match line.trim_end() {
          REBUILT => return Ok(()),
          FAILED => return Err(BuildError::Dev("snapfirec failed; see the errors above".to_owned())),
          text => println!("{text}"),
        },
        Err(e) => {
          self.stdin = None;
          return Err(BuildError::Dev(format!("snapfirec: {e}")));
        }
      }
    }
  }
}

impl Drop for Driven {
  fn drop(&mut self) {
    drop(self.stdin.take());
    let _ = self.child.kill();
    let _ = self.child.wait();
  }
}

/// The compiler, started on the first bundle so the overlay it reads is already written.
#[derive(Default)]
enum Bundler {
  #[default]
  Idle,
  Running(Driven),
}

impl Bundler {
  fn bundle(&mut self, app: &App, paths: &[PathBuf]) -> Result<(), BuildError> {
    if let Self::Running(driven) = self {
      if driven.alive() {
        let outcome = driven.rebuild(paths);
        if driven.alive() {
          return outcome;
        }
      }
    }
    *self = Self::Running(Driven::start(app)?);
    Ok(())
  }
}

/// An application with what the loop knows of its last build.
struct Tracked<'a> {
  app: &'a App,
  /// The generated files of the last build that generated, which tell a change the host has to
  /// reload for from one it only has to be told about.
  files: Option<Vec<(String, String)>>,
  bundler: Bundler,
  /// Sources the compiler has not compiled yet, kept across a failed batch so the next carries them.
  pending: HashSet<PathBuf>,
}

impl<'a> Tracked<'a> {
  fn new(app: &'a App) -> Self {
    Self { app, files: None, bundler: Bundler::default(), pending: HashSet::new() }
  }

  /// Generates and bundles, `sources` naming what changed and none of them meaning everything.
  /// `Ok(true)` when the generated files differ from the last build's.
  fn rebuild(&mut self, sources: Vec<PathBuf>) -> Result<bool, BuildError> {
    self.pending.extend(sources);
    let built = self.app.generate()?;
    let changed = self.files.as_ref() != Some(&built.files);
    if changed {
      print!("{}", built.report);
    }
    self.pending.extend(rewritten(&self.app.dir, self.files.as_deref(), &built.files));
    if changed {
      self.files = Some(built.files);
    }
    let batch: Vec<PathBuf> = self.pending.iter().cloned().collect();
    let checked = self.app.compile_with(|| self.bundler.bundle(self.app, &batch))?;
    self.pending.clear();
    report_types(checked.as_ref());
    Ok(changed)
  }
}

struct Server {
  child: Option<Child>,
}

impl Server {
  fn restart(&mut self, project: &Project, executable: Option<&Path>) {
    self.stop();
    match project.spawn(executable) {
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

/// A complete artifact: what `build` and `write` produced, plus the paths
/// they wrote. The browser bundle is in `dist/` beside them.
pub struct Emitted {
  pub built: Built,
  pub written: Vec<PathBuf>,
  /// What the typechecker found, when one ran and found no error.
  pub checked: Option<Checked>,
}

/// The directory under the app holding the sources the build rewrote for the
/// browser, read by snapfirec in place of the originals at the same path.
pub const BUNDLE_OVERLAY: &str = ".fsr-bundle";

/// Everything a host reads: the plan and the generated modules, then the
/// browser bundle. `build` and `write` alone leave `dist/` at whatever the
/// last bundle wrote, which a host cannot tell from a current one, so this
/// is what a build script and `fsr build` call. A build script under `fsr dev`
/// leaves it to the loop; see [`owns_build`].
pub fn emit(app: &Path, options: DevOptions) -> Result<Emitted, BuildError> {
  let app = App::open(app, options)?;
  let built = build(&app.dir, &app.options.build)?;
  let written = write(&app.dir, &built)?;
  let missing = crate::types::missing(&app.dir)?;
  if !missing.is_empty() {
    return Err(BuildError::Types(format!("no declarations for {}; run `fsr types`", missing.join(", "))));
  }
  let checked = app.compile()?;
  if let Some(checked) = &checked {
    if checked.errors() > 0 {
      let lines: Vec<String> = checked.diagnostics.iter().map(|d| d.to_string()).collect();
      return Err(BuildError::Typecheck(format!("{}\n{}", checked.row(), lines.join("\n"))));
    }
  }
  Ok(Emitted { built, written, checked })
}

pub fn run(app: &Path, options: DevOptions) -> Result<(), BuildError> {
  let project = Project::open(app, options)?;
  let (tx, rx) = unbounded::<Msg>();
  let mut stop: UnboundedSyncSender<Msg> = tx.clone();
  ctrlc::set_handler(move || {
    let _ = stop.send(Msg::Stop);
  })
  .map_err(|e| BuildError::Dev(format!("signal handler: {e}")))?;
  let mut fs = tx;
  let mut watcher = RecommendedWatcher::new(move |event| {
    let _ = fs.send(Msg::Fs(event));
  }, notify::Config::default())
  .map_err(|e| BuildError::Dev(format!("watcher: {e}")))?;
  let mut watching: HashSet<PathBuf> = HashSet::new();
  for app in &project.apps {
    watch(&mut watcher, &mut watching, app.dir.clone())?;
  }
  for path in project.watched() {
    watch(&mut watcher, &mut watching, path)?;
  }

  let mut server = Server { child: None };
  let root = project.root();
  let sites: Vec<String> = project.apps[1..].iter().map(|app| app.dir.strip_prefix(&root).unwrap_or(&app.dir).display().to_string()).collect();
  let shell = project.shell().dir.display();
  match (&project.cargo, sites.is_empty()) {
    (Some(cargo), true) => println!("dev: watching {shell} and the project at {}; press Ctrl-C to stop", cargo.display()),
    (Some(cargo), false) => println!("dev: watching {shell}, the sites {} and the project at {}; press Ctrl-C to stop", sites.join(", "), cargo.display()),
    (None, true) => println!("dev: watching {shell} and its configuration, served by the stock host; press Ctrl-C to stop"),
    (None, false) => println!("dev: watching {shell}, the sites {} and its configuration, served by the stock host; press Ctrl-C to stop", sites.join(", ")),
  }

  let mut tracked: Vec<Tracked> = project.apps.iter().map(Tracked::new).collect();
  let mut seen = Seen::default();
  let mut self_triggered = 0u32;
  let mut want = Change::everything(project.apps.len());
  loop {
    if want.any() {
      let mut restart = want.project || server.child.is_none();
      let mut reload = false;
      let mut failed = false;
      for (state, sources) in tracked.iter_mut().zip(want.apps.iter_mut()) {
        let Some(sources) = sources.take() else { continue };
        match state.rebuild(sources) {
          Ok(changed) => reload |= changed,
          Err(e) => {
            eprintln!("{e}");
            failed = true;
          }
        }
      }
      if !failed && reload && !restart {
        match project.shell().reload() {
          Ok(report) => print!("{report}"),
          Err(e) => {
            println!("dev: reload refused ({e}); restarting");
            restart = true;
          }
        }
      }
      if !failed && restart {
        match project.cargo_build() {
          Ok(built) => {
            for path in built.declared {
              watch(&mut watcher, &mut watching, path)?;
            }
            server.restart(&project, built.executable.as_deref());
          }
          Err(e) => {
            eprintln!("{e}");
            failed = true;
          }
        }
      } else if !failed && !reload {
        project.shell().notify_changed();
      }
      if failed {
        println!("dev: waiting for changes");
      }
    }
    let Some(batch) = collect(&rx, &mut server) else {
      println!("dev: stopping");
      return Ok(());
    };
    let changed = seen.changed(batch.paths);
    want = project.classify(&changed);
    if !want.any() {
      continue;
    }
    self_triggered = if batch.queued { self_triggered + 1 } else { 0 };
    if self_triggered >= SELF_TRIGGERED_LIMIT {
      if self_triggered == SELF_TRIGGERED_LIMIT {
        eprintln!("dev: rebuilt {SELF_TRIGGERED_LIMIT} times in a row on nothing but its own writes, last {}; waiting for an edit", describe(&root, &changed));
      }
      want = Change::default();
      continue;
    }
    println!("dev: changed {}", describe(&root, &changed));
  }
}

/// Up to a few of the changed paths, relative to the project, plus how many more there were.
fn describe(root: &Path, changed: &[PathBuf]) -> String {
  const SHOWN: usize = 6;
  let shown: Vec<String> = changed.iter().take(SHOWN).map(|path| path.strip_prefix(root).unwrap_or(path).display().to_string()).collect();
  match changed.len().saturating_sub(SHOWN) {
    0 => shown.join(", "),
    more => format!("{} and {more} more", shown.join(", ")),
  }
}

/// Watches a path once, recursively. Anything under a watched path is already covered and a
/// path that is not there yet is nothing to watch.
fn watch(watcher: &mut RecommendedWatcher, watching: &mut HashSet<PathBuf>, path: PathBuf) -> Result<(), BuildError> {
  if !path.exists() || watching.iter().any(|root| path.starts_with(root)) {
    return Ok(());
  }
  watcher
    .watch(&path, RecursiveMode::Recursive)
    .map_err(|e| BuildError::Dev(format!("watch {}: {e}", path.display())))?;
  watching.insert(path);
  Ok(())
}

/// What one cargo message says a build script reads, when it is a build script of the project
/// itself rather than of a dependency. Cargo writes the declarations beside the script's output
/// directory, so the message's `out_dir` is what leads to them.
fn inputs_of(cargo: &Path, message: &str) -> Vec<PathBuf> {
  let Ok(json) = serde_json::from_str::<serde_json::Value>(message) else { return Vec::new() };
  if json["reason"].as_str() != Some("build-script-executed") {
    return Vec::new();
  }
  // A dependency's build script reads files under the registry, which no edit ever touches. Only a
  // path source is the project's own.
  if !json["package_id"].as_str().is_some_and(|id| id.starts_with("path+")) {
    return Vec::new();
  }
  let Some(out_dir) = json["out_dir"].as_str() else { return Vec::new() };
  let Some(declarations) = Path::new(out_dir).parent().map(|dir| dir.join("output")) else { return Vec::new() };
  let Ok(text) = std::fs::read_to_string(declarations) else { return Vec::new() };
  text
    .lines()
    .filter_map(|line| line.strip_prefix("cargo:rerun-if-changed=").or_else(|| line.strip_prefix("cargo::rerun-if-changed=")))
    .map(|path| cargo.join(path))
    .collect()
}

/// The binary one cargo message says the project's own package produced.
fn executable_of(message: &str) -> Option<PathBuf> {
  let json = serde_json::from_str::<serde_json::Value>(message).ok()?;
  if json["reason"].as_str() != Some("compiler-artifact") || !json["package_id"].as_str()?.starts_with("path+") {
    return None;
  }
  json["executable"].as_str().map(PathBuf::from)
}

/// The sources whose rewritten copy under the overlay differs from the last build's, each named by
/// the path the compiler knows it as. A build can rewrite a module no edit touched, so the overlay
/// is what says which ones the compiler has to read again.
fn rewritten(app: &Path, before: Option<&[(String, String)]>, after: &[(String, String)]) -> Vec<PathBuf> {
  let after = overlay(after);
  let before = before.map(overlay).unwrap_or_default();
  let gone = before.keys().filter(|rel| !after.contains_key(*rel));
  let differs = after.iter().filter(|(rel, body)| before.get(*rel) != Some(body)).map(|(rel, _)| rel);
  differs.chain(gone).map(|rel| app.join(rel)).collect()
}

fn overlay(files: &[(String, String)]) -> HashMap<&str, &str> {
  files
    .iter()
    .filter_map(|(rel, body)| {
      let source = Path::new(rel).strip_prefix(BUNDLE_OVERLAY).ok()?;
      Some((source.to_str()?, body.as_str()))
    })
    .collect()
}

/// A type error is printed and the server keeps running: the bundle carries
/// no types, so what is served is what the sources say either way.
fn report_types(checked: Option<&Checked>) {
  let Some(checked) = checked else { return };
  for diagnostic in &checked.diagnostics {
    eprintln!("{diagnostic}");
  }
  println!("typecheck {}", checked.row());
}

/// One batch of changed paths. `queued` when the events were already waiting when the loop came
/// back for them, which is what a batch the loop's own build produced looks like: an edit arrives
/// while the loop is blocked, a build's writes arrive while it is building.
struct Batch {
  paths: Vec<PathBuf>,
  queued: bool,
}

/// Takes whatever is already queued, else blocks for the first event, polling the server
/// meanwhile, then keeps draining until the filesystem has been quiet for `SETTLE`. `None` on a
/// stop signal or once the watcher has hung up.
fn collect(rx: &UnboundedSyncReceiver<Msg>, server: &mut Server) -> Option<Batch> {
  let mut paths: HashSet<PathBuf> = HashSet::new();
  loop {
    match rx.try_recv() {
      Ok(Msg::Fs(event)) => absorb(event, &mut paths),
      Ok(Msg::Stop) | Err(TryRecvError::Disconnected) => return None,
      Err(TryRecvError::Empty) => break,
    }
  }
  let queued = !paths.is_empty();
  while paths.is_empty() {
    match rx.recv_timeout(POLL) {
      Ok(Msg::Fs(event)) => absorb(event, &mut paths),
      Ok(Msg::Stop) | Err(RecvErrorTimeout::Disconnected) => return None,
      Err(RecvErrorTimeout::Timeout) => server.poll(),
    }
  }
  loop {
    match rx.recv_timeout(SETTLE) {
      Ok(Msg::Fs(event)) => absorb(event, &mut paths),
      Ok(Msg::Stop) | Err(RecvErrorTimeout::Disconnected) => return None,
      Err(RecvErrorTimeout::Timeout) => break,
    }
  }
  Some(Batch { paths: paths.into_iter().collect(), queued })
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

  fn app(dir: &Path) -> App {
    App { dir: dir.to_path_buf(), layout: Layout::default(), snapfirec: PathBuf::from("snapfirec"), options: DevOptions::default() }
  }

  fn project(shell: &Path) -> Project {
    Project { apps: vec![app(shell)], cargo: Some(shell.parent().unwrap().to_path_buf()) }
  }

  fn with_site(shell: &Path, site: &Path) -> Project {
    Project { apps: vec![app(shell), app(site)], cargo: Some(shell.parent().unwrap().to_path_buf()) }
  }

  #[test]
  fn generated_output_is_never_a_reason_to_rebuild() {
    let p = project(Path::new("/p/app"));
    let change = p.classify(&[PathBuf::from("/p/app/generated/plan.json"), PathBuf::from("/p/app/dist/src/main.js"), PathBuf::from("/p/app/types/react/index.d.ts"), PathBuf::from("/p/app/tsconfig.json"), PathBuf::from("/p/app/.fsr-dev"), PathBuf::from("/p/app/tests/cart/loader.test.ts"), PathBuf::from("/p/app/routes/cart/loader.test.ts")]);
    assert!(!change.any());
  }

  #[test]
  fn app_sources_and_project_sources_are_told_apart() {
    let p = project(Path::new("/p/app"));
    let change = p.classify(&[PathBuf::from("/p/app/routes/index/page.tsx")]);
    assert!(change.apps[0].is_some() && !change.project);
    let change = p.classify(&[PathBuf::from("/p/src/main.rs"), PathBuf::from("/p/config/app.toml")]);
    assert!(change.apps[0].is_none() && change.project);
    let change = p.classify(&[PathBuf::from("/p/app/.fsr-something"), PathBuf::from("/p/app/importmap.json")]);
    assert!(change.apps[0].is_some());
  }

  #[test]
  fn a_changed_source_is_named_for_the_compiler() {
    let p = project(Path::new("/p/app"));
    let change = p.classify(&[PathBuf::from("/p/app/routes/index/page.tsx"), PathBuf::from("/p/app/dist/src/main.js")]);
    assert_eq!(change.apps[0], Some(vec![PathBuf::from("/p/app/routes/index/page.tsx")]));
  }

  #[test]
  fn a_site_source_is_the_site_s_change_and_its_output_is_nobody_s() {
    let p = with_site(Path::new("/p/app"), Path::new("/p/sites/learn/app"));
    let change = p.classify(&[PathBuf::from("/p/sites/learn/app/src/ui/Prose.tsx")]);
    assert_eq!(change.apps, vec![None, Some(vec![PathBuf::from("/p/sites/learn/app/src/ui/Prose.tsx")])]);
    assert!(!change.project);
    let change = p.classify(&[PathBuf::from("/p/sites/learn/app/types/react/index.d.ts"), PathBuf::from("/p/sites/learn/app/generated/plan.sexp"), PathBuf::from("/p/sites/learn/app/dist/src/main.js")]);
    assert!(!change.any());
    let change = p.classify(&[PathBuf::from("/p/sites/learn/content/010.md")]);
    assert!(change.project && change.apps.iter().all(Option::is_none));
  }

  #[test]
  fn everything_touches_every_app_with_no_source_named() {
    let change = Change::everything(3);
    assert!(change.project);
    assert_eq!(change.apps, vec![Some(Vec::new()); 3]);
    assert!(!Change::default().any());
  }

  #[test]
  fn a_file_that_holds_what_it_held_is_not_a_change() {
    let dir = std::env::temp_dir().join(format!("fsr-seen-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("a.ts");
    std::fs::write(&file, "one").unwrap();

    let mut seen = Seen::default();
    assert_eq!(seen.changed(vec![file.clone()]), vec![file.clone()]);
    std::fs::write(&file, "one").unwrap();
    assert!(seen.changed(vec![file.clone()]).is_empty());
    std::fs::write(&file, "two").unwrap();
    assert_eq!(seen.changed(vec![file.clone()]), vec![file.clone()]);
    assert!(seen.changed(vec![dir.clone()]).is_empty());
    std::fs::remove_file(&file).unwrap();
    assert_eq!(seen.changed(vec![file.clone()]), vec![file.clone()]);
    let never = dir.join("never.ts");
    assert_eq!(seen.changed(vec![never.clone()]), vec![never]);

    std::fs::remove_dir_all(&dir).unwrap();
  }

  #[test]
  fn a_change_is_described_relative_to_the_project_and_capped() {
    let root = Path::new("/p");
    let few = [PathBuf::from("/p/app/a.ts"), PathBuf::from("/elsewhere/b.ts")];
    assert_eq!(describe(root, &few), "app/a.ts, /elsewhere/b.ts");
    let many: Vec<PathBuf> = (0..8).map(|i| PathBuf::from(format!("/p/{i}.ts"))).collect();
    assert_eq!(describe(root, &many), "0.ts, 1.ts, 2.ts, 3.ts, 4.ts, 5.ts and 2 more");
  }

  fn overlay_file(rel: &str, body: &str) -> (String, String) {
    (format!("{BUNDLE_OVERLAY}/{rel}"), body.to_owned())
  }

  #[test]
  fn a_rewritten_module_is_named_by_its_source_path() {
    let app = Path::new("/p/app");
    let before = vec![overlay_file("src/a.ts", "one"), overlay_file("src/b.ts", "two"), overlay_file("src/c.ts", "three")];
    let after = vec![overlay_file("src/a.ts", "one"), overlay_file("src/b.ts", "changed")];
    let mut changed = rewritten(app, Some(&before), &after);
    changed.sort();
    assert_eq!(changed, vec![app.join("src/b.ts"), app.join("src/c.ts")]);
  }

  fn message(reason: &str, package: &str, out_dir: &Path) -> String {
    format!(r#"{{"reason":"{reason}","package_id":"{package}","out_dir":"{}"}}"#, out_dir.display())
  }

  #[test]
  fn a_build_script_declares_what_the_watcher_has_to_watch() {
    let dir = std::env::temp_dir().join(format!("fsr-declared-{}", std::process::id()));
    let out = dir.join("build/www-abc/out");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(
      dir.join("build/www-abc/output"),
      "cargo:rerun-if-changed=build.rs\ncargo::rerun-if-changed=/abs/sites/learn/content\ncargo:rustc-env=X=1\n",
    )
    .unwrap();

    let cargo = Path::new("/p");
    let declared = inputs_of(cargo, &message("build-script-executed", "path+file:///p#www@0.1.0", &out));
    assert_eq!(declared, vec![PathBuf::from("/p/build.rs"), PathBuf::from("/abs/sites/learn/content")]);

    assert!(inputs_of(cargo, &message("build-script-executed", "registry+https://x#serde@1.0.0", &out)).is_empty());
    assert!(inputs_of(cargo, &message("compiler-artifact", "path+file:///p#www@0.1.0", &out)).is_empty());
    assert!(inputs_of(cargo, "not json at all").is_empty());

    std::fs::remove_dir_all(&dir).unwrap();
  }

  #[test]
  fn the_binary_cargo_built_is_what_runs() {
    let own = r#"{"reason":"compiler-artifact","package_id":"path+file:///p#www@0.1.0","executable":"/p/target/debug/www"}"#;
    assert_eq!(executable_of(own), Some(PathBuf::from("/p/target/debug/www")));
    let lib = r#"{"reason":"compiler-artifact","package_id":"path+file:///p#www@0.1.0","executable":null}"#;
    assert_eq!(executable_of(lib), None);
    let dependency = r#"{"reason":"compiler-artifact","package_id":"registry+https://x#serde@1.0.0","executable":"/p/target/debug/build/serde"}"#;
    assert_eq!(executable_of(dependency), None);
    assert_eq!(executable_of(r#"{"reason":"build-script-executed","package_id":"path+file:///p#www@0.1.0"}"#), None);
  }

  #[test]
  fn only_the_overlay_says_a_module_was_rewritten() {
    let app = Path::new("/p/app");
    let after = vec![("generated/plan.sexp".to_owned(), "(plan)".to_owned()), overlay_file("src/a.ts", "one")];
    assert_eq!(rewritten(app, None, &after), vec![app.join("src/a.ts")]);
  }
}
