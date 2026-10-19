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

use std::collections::{BTreeMap, HashMap, HashSet};
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
use crate::{BuildError, Built, Options, write};
use snapfire_compiler_wire::driven::{self as protocol, FAILED, MAPPED, PLUGIN, REBUILT, REFERENCES};

use crate::plugins::Plugins;

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

#[non_exhaustive]
pub struct DevOptions {
  pub build: Options,
  /// URL prefix the bundle is served under; the host infers its static root from it.
  pub public_path: String,
  /// The compiler to bundle with; beside this binary or on PATH when absent.
  pub snapfirec: Option<PathBuf>,
  /// Which TypeScript checks the application and whether it is checked at all.
  pub typecheck: Typecheck,
  /// A shell to serve the application under when it is a site: its project
  /// directory, an unpacked artifact or an archive. Absent, the shell `[site]
  /// shell` points into is used when it has a configuration.
  pub under: Option<PathBuf>,
  /// Serves a site by itself even when a shell is found for it.
  pub alone: bool,
  /// Prints the report's rows for what lowered as well, `--verbose`.
  pub verbose: bool,
}

impl Default for DevOptions {
  fn default() -> Self {
    Self { build: Options::default(), public_path: crate::BUNDLE_BASE.to_owned(), snapfirec: None, typecheck: Typecheck { enabled: true, ..Typecheck::default() }, under: None, alone: false, verbose: false }
  }
}

impl DevOptions {
  /// The defaults for `app`, its `[site]` read from the configuration beside
  /// it: a site's bundle is served under its prefix.
  pub fn beside(app: &Path) -> Self {
    let build = Options::beside(app);
    let public_path = crate::bundle_base(build.site.as_ref());
    Self { build, public_path, snapfirec: None, typecheck: Typecheck::beside(app), under: None, alone: false, verbose: false }
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
  /// Minified, with plugins compiling for production. [`emit`] sets it unless the configuration writes `server.dev = true`.
  production: bool,
  /// Served as it stands and never built or watched: a shell artifact a site runs under.
  served: bool,
}

impl App {
  fn open(dir: &Path, options: DevOptions) -> Result<Self, BuildError> {
    let dir = dir.canonicalize().map_err(|e| BuildError::Io(dir.to_path_buf(), e))?;
    let layout = Layout::of(&dir)?;
    let snapfirec = find_snapfirec(options.snapfirec.as_deref());
    Ok(Self { dir, layout, snapfirec, options, production: false, served: false })
  }

  /// A shell served from `root` as it stands, its app directory from its configuration.
  fn served(root: &Path) -> Result<Self, BuildError> {
    let config = Config::load(root).map_err(|e| BuildError::Dev(format!("{}: {e}", root.display())))?;
    let dir = config.app.canonicalize().map_err(|e| BuildError::Io(config.app.clone(), e))?;
    Ok(Self { dir, layout: Layout::default(), snapfirec: PathBuf::from("snapfirec"), options: DevOptions::default(), production: false, served: true })
  }

  /// Builds and writes, with `adopted` the asset paths earlier bundles reported, so the map the
  /// compiler reads already names them.
  fn generate(&self, adopted: &[String], plugins: &mut Plugins) -> Result<Built, BuildError> {
    let mut built = crate::build_with(&self.dir, &self.options.build, plugins)?;
    if !adopted.is_empty() {
      built.resolver.reconcile(adopted)?;
      built.refresh_assets();
    }
    write(&self.dir, &built)?;
    Ok(built)
  }

  /// The compiler invocation the driven child is built from.
  fn compiler(&self) -> Command {
    let mut command = Command::new(&self.snapfirec);
    command.arg("--root").arg(&self.dir).args([
      "--config",
      "tsconfig.build.json",
      "--source-map",
      "--public-path",
      &self.options.public_path,
      "--import-map",
      &self.layout.importmap,
      "--asset-map",
      crate::assets::MAP_FILE,
    ]);
    if self.production {
      command.args(["--minify", "compact"]);
    }
    if self.dir.join(crate::tailwind::CLASSES_FILE).is_file() {
      command.args(["--tailwind-classes", crate::tailwind::CLASSES_FILE]);
    }
    if let Some((vendor, base)) = self.vendor_tree() {
      command.arg("--tailwind-vendor").arg(vendor).args(["--tailwind-vendor-base", &base]);
    }
    // A missing compiler is refused by the build before this compiler runs.
    for (ext, path) in crate::plugin_compilers(&self.dir, &self.layout).unwrap_or_default() {
      command.arg("--plugin-compiler").arg(format!("{ext}={}", path.display()));
    }
    command
  }

  /// The vendor tree the import map names and its URL prefix: the app's own, else for a site
  /// the shell's, which the site's import map points at.
  fn vendor_tree(&self) -> Option<(PathBuf, String)> {
    if self.dir.join(&self.layout.vendor).is_dir() {
      return Some((PathBuf::from(&self.layout.vendor), self.layout.base.clone()));
    }
    let shell = crate::site_beside(&self.dir)?.shell?;
    let vendor = shell.parent()?.parent()?.join("vendor");
    vendor.is_dir().then(|| (vendor, crate::VENDOR_BASE.to_owned()))
  }

  /// One bundle of everything, through a driven child that is dropped once it has answered.
  fn bundle(&self, built: &Built, plugins: &mut Plugins) -> Result<(), BuildError> {
    let mut reconcile = reconciler(&self.dir, built, None);
    Driven::start(self, &mut reconcile, plugins)?;
    Ok(())
  }

  /// Tells the running server a bundle changed under it, so open documents
  /// refresh. Best effort: a server that is not up yet or has `dev` off
  /// simply does not hear it.
  fn notify_changed(&self) {
    self.tell("/__fsr/changed", "");
  }

  /// Tells the running server the last build was refused, with what refused
  /// it, so open documents show it. Best effort, as `notify_changed` is.
  fn notify_failed(&self, message: &str) {
    self.tell("/__fsr/failed", message);
  }

  fn tell(&self, path: &str, body: &str) {
    let root = crate::serve::project_root(&self.dir);
    let Ok(config) = Config::load(&root) else { return };
    let listen = config.server.listen;
    let Ok(mut stream) = std::net::TcpStream::connect(&listen) else { return };
    let _ = stream.set_write_timeout(Some(Duration::from_secs(1)));
    let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
    let head = format!("POST {path} HTTP/1.1\r\nHost: {listen}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
    let _ = std::io::Write::write_all(&mut stream, head.as_bytes());
    let _ = std::io::Write::write_all(&mut stream, body.as_bytes());
    // A server drops a request whose client went away before it was
    // answered, so the answer is waited for.
    let _ = std::io::Read::read_to_end(&mut stream, &mut Vec::new());
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
  fn compile(&self, built: &Built, plugins: &mut Plugins) -> Result<Option<Checked>, BuildError> {
    self.compile_with(|| self.bundle(built, plugins))
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
  /// A site served under a shell artifact: the site's name and its project
  /// root, which the shell is told to mount for the run.
  mount: Option<(String, PathBuf)>,
  /// `[build] before` and `after`, resolved against the project root.
  programs: Programs,
  /// `--verbose`: the loop's own progress lines are printed too.
  verbose: bool,
}

impl Project {
  fn open(app: &Path, options: DevOptions) -> Result<Self, BuildError> {
    let verbose = options.verbose;
    if let Some(root) = shell_under(app, &options)? {
      let site = crate::site_beside(app).expect("a shell is found for a site alone");
      let site_root = crate::serve::project_root(app);
      let shell = App::served(&root)?;
      let mut site_options = options;
      site_options.under = None;
      let site_app = App::open(app, site_options)?;
      println!("dev: serving {} under the shell at {}", site.name, root.display());
      let mut programs = programs_of(app);
      programs.verbose = verbose;
      return Ok(Self { apps: vec![shell, site_app], cargo: None, mount: Some((site.name, site_root)), programs, verbose });
    }
    let shell = App::open(app, options)?;
    let cargo = shell.dir.parent().map(Path::to_path_buf).filter(|p| p.join("Cargo.toml").is_file());
    let sites = site_apps(&shell.dir);
    let mut apps = vec![shell];
    for dir in sites {
      let mut options = DevOptions::beside(&dir);
      options.snapfirec = Some(apps[0].snapfirec.clone());
      options.verbose = verbose;
      apps.push(App::open(&dir, options)?);
    }
    let mut programs = programs_of(&apps[0].dir);
    programs.verbose = verbose;
    Ok(Self { apps, cargo, mount: None, programs, verbose })
  }

  /// The arguments `fsr serve` takes for the run: the shell's app directory
  /// and, for a site under a shell, the mount that puts the site's tree in
  /// the shell's table.
  fn serve_args(&self) -> Vec<std::ffi::OsString> {
    let mut args = vec![std::ffi::OsString::from("serve"), self.shell().dir.clone().into_os_string()];
    if let Some((name, root)) = &self.mount {
      args.push("--mount".into());
      args.push(format!("{name}={}", root.display()).into());
    }
    if self.verbose {
      args.push("--verbose".into());
    }
    args
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
        Command::new(exe).args(self.serve_args()).spawn().map_err(|e| BuildError::Dev(format!("fsr serve: {e}")))
      }
    }
  }

  /// What to watch besides the applications: the Cargo project's sources or the configuration the stock host reads.
  fn watched(&self) -> Vec<PathBuf> {
    let mut watched: Vec<PathBuf> = match &self.cargo {
      Some(cargo) => ["src", "config", "build.rs", "Cargo.toml"].iter().map(|name| cargo.join(name)).collect(),
      None => {
        let root = crate::serve::project_root(&self.shell().dir);
        ["config", "app.toml", "app.yaml"].iter().map(|name| root.join(name)).collect()
      }
    };
    for program in self.programs.before.iter().chain(&self.programs.after) {
      watched.extend(["src", "build.rs", "Cargo.toml"].iter().map(|name| program.join(name)));
    }
    watched
  }

  /// Which application each changed path belongs to, else a program's declared input, else the project around them.
  fn classify(&self, changed: &[PathBuf], declared: &[PathBuf]) -> Change {
    let watched = self.watched();
    let mut change = Change { apps: vec![None; self.apps.len()], project: false, programs: false, counted: Vec::new() };
    for path in changed {
      let owner = self.apps.iter().enumerate().filter(|(_, app)| path.starts_with(&app.dir)).max_by_key(|(_, app)| app.dir.as_os_str().len());
      match owner {
        Some((_, app)) if app.served => {}
        Some((i, app)) => {
          let rel = path.strip_prefix(&app.dir).expect("a prefix");
          if !ignored(rel) {
            change.apps[i].get_or_insert_with(Vec::new).push(path.clone());
            change.counted.push(path.clone());
          }
        }
        None if !watched.iter().any(|root| path.starts_with(root)) && declared.iter().any(|input| path.starts_with(input)) => {
          change.programs = true;
          change.counted.push(path.clone());
        }
        None => {
          change.project = true;
          change.counted.push(path.clone());
        }
      }
    }
    // The build reads the configuration and the crate's own sources too, so a
    // change outside every application rebuilds each of them whole.
    if change.project {
      for (i, app) in self.apps.iter().enumerate() {
        if !app.served {
          change.apps[i] = Some(Vec::new());
        }
      }
    }
    change
  }
}

fn ignored(rel: &Path) -> bool {
  let first = rel.components().next().map(|c| c.as_os_str().to_string_lossy().into_owned()).unwrap_or_default();
  IGNORED.contains(&first.as_str()) || first.starts_with(".fsr-") || first == "tests" || rel.to_string_lossy().ends_with(".test.ts")
}

/// The shell a site runs under, when it runs under one: `--under` as given, a
/// directory holding a shell's configuration, which a project and a deploy
/// tree both are, or a shell's app directory; else the shell `[site] shell`
/// points into when that is a tree with a configuration. `None` for an
/// application that is not a site, for `--alone` and for a site whose shell is
/// a bare `shell.json`.
fn shell_under(app: &Path, options: &DevOptions) -> Result<Option<PathBuf>, BuildError> {
  if options.alone {
    return Ok(None);
  }
  let Some(site) = crate::site_beside(app) else {
    if options.under.is_some() {
      return Err(BuildError::Dev(format!("{} is not a site; --under runs a site under a shell", app.display())));
    }
    return Ok(None);
  };
  let given = match &options.under {
    Some(under) => under.clone(),
    None => {
      let Some(root) = site.shell.as_deref().and_then(|json| json.parent()).and_then(|generated| generated.parent()).map(crate::serve::project_root) else {
        return Ok(None);
      };
      let shell_root = root.canonicalize().unwrap_or(root);
      if !Config::load(&shell_root).is_ok() {
        return Ok(None);
      }
      return Ok(Some(shell_root));
    }
  };
  if !given.is_dir() {
    return Err(BuildError::Dev(format!("--under {}: not a directory; a shell's project or the deploy tree `fsr bundle` writes", given.display())));
  }
  let root = if given.join("config").is_dir() || given.join("app.toml").is_file() { given.clone() } else { crate::serve::project_root(&given) };
  Config::load(&root).map_err(|e| BuildError::Dev(format!("--under {}: {e}", given.display())))?;
  Ok(Some(root.canonicalize().unwrap_or(root)))
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
  /// A path a `[build] before` or `after` program declared it reads changed, and nothing else
  /// outside the applications did.
  programs: bool,
  /// The changed paths that caused the rest, build output and served applications left out.
  counted: Vec<PathBuf>,
}

impl Change {
  fn everything(apps: usize) -> Self {
    Self { apps: vec![Some(Vec::new()); apps], project: true, programs: false, counted: Vec::new() }
  }

  fn any(&self) -> bool {
    self.project || self.programs || self.apps.iter().any(Option::is_some)
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

/// What answers the compiler when it reports references the map does not name: defines each
/// on the resolver and rewrites the map, so the compiler reads it again.
type Reconcile<'a> = dyn FnMut(&[String]) -> Result<(), BuildError> + 'a;

/// The reconciler for one build: every path the compiler reports is defined on the build's
/// resolver, remembered in `adopted` when a loop keeps one; then the map is rewritten.
fn reconciler<'a>(app: &'a Path, built: &'a Built, mut adopted: Option<&'a mut std::collections::BTreeSet<String>>) -> impl FnMut(&[String]) -> Result<(), BuildError> + 'a {
  move |paths: &[String]| {
    built.resolver.reconcile(paths)?;
    if let Some(adopted) = adopted.as_deref_mut() {
      adopted.extend(paths.iter().cloned());
    }
    crate::assets::write_map(app, &built.resolver, &built.assets.fonts)
  }
}

/// The compiler held open for the life of `fsr dev`, told which paths changed and answering once it
/// has compiled them. One process means one resolved configuration and one selection, so a change
/// recompiles what changed instead of everything. Starting it is itself the first build. The
/// protocol is `snapfire_compiler_wire::driven`: the compiler announces its version first, and
/// when a batch named an asset the map does not define it lists the paths and waits for the map.
pub(crate) struct Driven {
  child: Child,
  /// Taken when the child is gone, which is what tells a failed batch from a lost compiler.
  stdin: Option<ChildStdin>,
  stdout: BufReader<ChildStdout>,
  snapfirec: PathBuf,
  /// The compiler's own lines, held rather than printed, for a caller that shows them only when a
  /// batch fails.
  held: Option<Vec<String>>,
}

impl Driven {
  fn start(app: &App, reconcile: &mut Reconcile<'_>, plugins: &mut Plugins) -> Result<Self, BuildError> {
    let mut command = app.compiler();
    if app.dir.join(BUNDLE_OVERLAY).is_dir() {
      command.args(["--overlay", BUNDLE_OVERLAY]);
    }
    Self::spawn(command, &app.snapfirec, reconcile, plugins, !app.options.verbose)
  }

  /// Starts `command`, a snapfirec invocation, under `--driven` and settles the build it starts
  /// with, answering its plugin requests from `plugins`.
  pub(crate) fn spawn(mut command: Command, snapfirec: &Path, reconcile: &mut Reconcile<'_>, plugins: &mut Plugins, quiet: bool) -> Result<Self, BuildError> {
    crate::install::ensure(&crate::install::COMPILER, snapfirec)?;
    command.arg("--driven");
    let mut child = command
      .stdin(Stdio::piped())
      .stdout(Stdio::piped())
      .spawn()
      .map_err(|e| BuildError::Dev(format!("{}: {e}; pass --snapfirec or put it on PATH", snapfirec.display())))?;
    let stdin = child.stdin.take();
    let stdout = BufReader::new(child.stdout.take().expect("a piped stdout"));
    let mut driven = Self { child, stdin, stdout, snapfirec: snapfirec.to_path_buf(), held: quiet.then(Vec::new) };
    driven.hello()?;
    match driven.settled(reconcile, plugins) {
      Ok(()) => Ok(driven),
      Err(e) if driven.alive() => Err(e),
      Err(e) => match driven.child.wait().ok().filter(|status| !status.success()) {
        Some(status) => Err(BuildError::Dev(format!("{} exited with {status} before its first build", snapfirec.display()))),
        None => Err(e),
      },
    }
  }

  /// The compiler's first line names the protocol it speaks; any other first line is an older
  /// compiler, which would compile without the map and place assets of its own.
  fn hello(&mut self) -> Result<(), BuildError> {
    let mut line = String::new();
    let read = self.stdout.read_line(&mut line).map_err(|e| BuildError::Dev(format!("snapfirec: {e}")))?;
    let theirs = if read == 0 { None } else { protocol::parse_hello(&line) };
    match theirs {
      Some(version) if version == protocol::PROTOCOL => Ok(()),
      Some(version) => Err(BuildError::Tool(format!(
        "{} speaks driven protocol {version} and this fsr needs {}; `{}`",
        self.snapfirec.display(),
        protocol::PROTOCOL,
        crate::install::COMPILER.command()
      ))),
      None => Err(BuildError::Tool(format!(
        "{} does not announce the driven protocol this fsr needs ({}), so it is older than the asset map; `{}`",
        self.snapfirec.display(),
        protocol::PROTOCOL,
        crate::install::COMPILER.command()
      ))),
    }
  }

  fn alive(&self) -> bool {
    self.stdin.is_some()
  }

  fn rebuild(&mut self, paths: &[PathBuf], reconcile: &mut Reconcile<'_>, plugins: &mut Plugins) -> Result<(), BuildError> {
    let mut batch: String = paths.iter().map(|path| format!("{}\n", path.display())).collect();
    batch.push('\n');
    self.send(&batch)?;
    self.settled(reconcile, plugins)
  }

  fn send(&mut self, text: &str) -> Result<(), BuildError> {
    let Some(stdin) = self.stdin.as_mut() else {
      return Err(BuildError::Dev("snapfirec is gone".to_owned()));
    };
    if let Err(e) = stdin.write_all(text.as_bytes()).and_then(|()| stdin.flush()) {
      self.stdin = None;
      return Err(BuildError::Dev(format!("snapfirec: {e}")));
    }
    Ok(())
  }

  fn line(&mut self, line: &mut String) -> Result<(), BuildError> {
    line.clear();
    match self.stdout.read_line(line) {
      Ok(0) => {
        self.stdin = None;
        Err(BuildError::Dev("snapfirec exited".to_owned()))
      }
      Ok(_) => Ok(()),
      Err(e) => {
        self.stdin = None;
        Err(BuildError::Dev(format!("snapfirec: {e}")))
      }
    }
  }

  /// Forwards the compiler's own output until it says the batch is compiled. A references list
  /// is answered by defining each path and rewriting the map, a plugin request from `plugins`.
  fn settled(&mut self, reconcile: &mut Reconcile<'_>, plugins: &mut Plugins) -> Result<(), BuildError> {
    let mut line = String::new();
    loop {
      self.line(&mut line)?;
      match line.trim_end() {
        REBUILT => {
          if let Some(held) = self.held.as_mut() {
            held.clear();
          }
          return Ok(());
        }
        FAILED => {
          return Err(BuildError::Dev(match self.held.as_mut() {
            Some(held) => format!("snapfirec failed:\n{}", std::mem::take(held).join("\n")),
            None => "snapfirec failed; see the errors above".to_owned(),
          }))
        }
        REFERENCES => {
          let mut paths = Vec::new();
          loop {
            self.line(&mut line)?;
            let path = line.trim_end();
            if path.is_empty() {
              break;
            }
            paths.push(path.to_owned());
          }
          reconcile(&paths)?;
          self.send(&format!("{MAPPED}\n"))?;
        }
        PLUGIN => {
          self.line(&mut line)?;
          let request = serde_json::from_str(line.trim_end()).map_err(|e| BuildError::Dev(format!("snapfirec asked for a plugin in a shape this fsr does not read: {e}")))?;
          let answer = serde_json::to_string(&plugins.answer(request)).expect("an answer serializes");
          self.send(&format!("{answer}\n"))?;
        }
        text => match self.held.as_mut() {
          Some(held) => held.push(text.to_owned()),
          None => println!("{text}"),
        },
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
  fn bundle(&mut self, app: &App, paths: &[PathBuf], reconcile: &mut Reconcile<'_>, plugins: &mut Plugins) -> Result<(), BuildError> {
    if let Self::Running(driven) = self {
      if driven.alive() {
        let outcome = driven.rebuild(paths, reconcile, plugins);
        if driven.alive() {
          return outcome;
        }
      }
    }
    *self = Self::Running(Driven::start(app, reconcile, plugins)?);
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
  /// The plugin workers the build reads components through and the compiler compiles them with.
  plugins: Plugins,
  /// Sources the compiler has not compiled yet, kept across a failed batch so the next carries them.
  pending: HashSet<PathBuf>,
  /// The asset manifest of the last build, derived under `dist/` once the bundle has run.
  last: Option<snapfire_fsr_host::assets::AssetsManifest>,
  /// Asset paths the compiler reported and the loop defined, carried into every later build so
  /// the map names them before the compiler asks again.
  adopted: std::collections::BTreeSet<String>,
  /// The stylesheets the last bundle wrote, which the host links from the head it infers at boot.
  sheets: Option<Vec<String>>,
}

impl<'a> Tracked<'a> {
  fn new(app: &'a App) -> Self {
    Self { app, files: None, bundler: Bundler::default(), plugins: Plugins::new(), pending: HashSet::new(), last: None, adopted: std::collections::BTreeSet::new(), sheets: None }
  }

  /// Generates and bundles, `sources` naming what changed and none of them meaning everything.
  /// `Ok(true)` when the generated files differ from the last build's.
  fn rebuild(&mut self, sources: Vec<PathBuf>) -> Result<bool, BuildError> {
    let catalogs = self.app.dir.join("locales");
    let recatalogued = sources.iter().any(|source| source.starts_with(&catalogs));
    self.pending.extend(sources);
    let adopted: Vec<String> = self.adopted.iter().cloned().collect();
    let mut built = self.app.generate(&adopted, &mut self.plugins)?;
    if self.files.as_ref() != Some(&built.files) {
      print_report(&built.report, self.app.options.verbose);
    }
    self.pending.extend(rewritten(&self.app.dir, self.files.as_deref(), &built.files));
    let batch: Vec<PathBuf> = self.pending.iter().cloned().collect();
    let checked = {
      let mut reconcile = reconciler(&self.app.dir, &built, Some(&mut self.adopted));
      self.app.compile_with(|| self.bundler.bundle(self.app, &batch, &mut reconcile, &mut self.plugins))?
    };
    self.pending.clear();
    report_types(checked.as_ref(), self.app.options.verbose);
    for (rel, content) in built.refresh_assets() {
      let path = self.app.dir.join(&rel);
      std::fs::write(&path, content).map_err(|e| BuildError::Io(path, e))?;
    }
    let sheets = bundled_sheets(&self.app.dir.join("dist"));
    let restyled = self.sheets.as_ref().is_some_and(|before| *before != sheets);
    self.sheets = Some(sheets);
    let changed = self.files.as_ref() != Some(&built.files) || restyled || recatalogued;
    self.last = Some(built.assets.clone());
    if changed {
      self.files = Some(built.files);
    }
    if let Some(built) = self.last.as_ref() {
      report_derived(crate::assets::derive(&self.app.dir, &self.app.dir.join("dist"), built)?, self.app.options.verbose);
    }
    Ok(changed)
  }
}

/// The `.css` outputs the bundle's facts file lists, sorted; none when it has not run.
fn bundled_sheets(dist: &Path) -> Vec<String> {
  let Ok(text) = std::fs::read_to_string(dist.join(".snapfire-build.json")) else { return Vec::new() };
  let Ok(facts) = serde_json::from_str::<serde_json::Value>(&text) else { return Vec::new() };
  let mut sheets: Vec<String> = facts["outputs"].as_array().into_iter().flatten().filter_map(|o| o.as_str()).filter(|o| o.ends_with(".css")).map(str::to_owned).collect();
  sheets.sort();
  sheets
}

struct Server {
  child: Option<Child>,
}

impl Server {
  fn restart(&mut self, project: &Project, executable: Option<&Path>) {
    self.stop();
    match project.spawn(executable) {
      Ok(child) => {
        if project.verbose {
          println!("dev: server started, pid {}", child.id());
        }
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

  /// Notices a server that exited on its own. One that failed is started again by the next
  /// change, so a fix brings it back. One that exited 0 finished as a program does, which a
  /// server never does, so `fsr dev` stops on it.
  fn poll(&mut self) -> Result<(), BuildError> {
    if let Some(child) = &mut self.child {
      if let Ok(Some(status)) = child.try_wait() {
        self.child = None;
        if status.success() {
          return Err(BuildError::Dev("dev: the app exited 0; fsr dev runs it as the server and a server listens until it is stopped. A program that writes files and exits is named in `[build] before` or `after`, not `src/main.rs`".to_owned()));
        }
        eprintln!("dev: server exited with {status}; the next change starts it again");
      }
    }
    Ok(())
  }
}

impl Drop for Server {
  fn drop(&mut self) {
    self.stop();
  }
}

/// A complete artifact: what `build` and `write` produced, plus the paths
/// they wrote. The browser bundle is in `dist/` beside them.
#[non_exhaustive]
pub struct Emitted {
  pub built: Built,
  pub written: Vec<PathBuf>,
  /// What the typechecker found, when one ran and found no error.
  pub checked: Option<Checked>,
}

/// The directory under the app holding the sources the build rewrote for the
/// browser, read by snapfirec in place of the originals at the same path.
pub const BUNDLE_OVERLAY: &str = ".fsr-bundle";

/// Whether `emit` bundles for production: unless `app`'s configuration writes `server.dev = true`.
fn production(app: &Path) -> bool {
  crate::config_beside(app).is_none_or(|config| config.server.dev != Some(true))
}

/// Everything a host reads: the plan and the generated modules, then the
/// browser bundle. `build` and `write` alone leave `dist/` at whatever the
/// last bundle wrote, which a host cannot tell from a current one, so this
/// is what a build script and `fsr build` call. A build script under `fsr dev`
/// leaves it to the loop; see [`owns_build`].
pub fn emit(app: &Path, options: DevOptions) -> Result<Emitted, BuildError> {
  emit_from(app, options, crate::types::NPM_REGISTRY)
}

/// `emit` with the declarations it lacks fetched from `registry`.
/// The report as `--verbose` asks for it.
pub fn print_report(report: &crate::Report, verbose: bool) {
  if verbose {
    print!("{}", report.verbose());
  } else {
    print!("{report}");
  }
}

pub(crate) fn emit_from(app: &Path, options: DevOptions, registry: &str) -> Result<Emitted, BuildError> {
  let mut app = App::open(app, options)?;
  app.production = production(&app.dir);
  let fetched = crate::types::fetch_missing_from(&app.dir, registry);
  let mut plugins = Plugins::new();
  let mut built = crate::build_with(&app.dir, &app.options.build, &mut plugins)?;
  let rest = (|| -> Result<(Vec<PathBuf>, Option<Checked>), BuildError> {
    let mut written = write(&app.dir, &built)?;
    let missing = built.missing_declarations.clone();
    if !missing.is_empty() {
      let failure = match &fetched {
        Err(e) => format!(" ({e})"),
        Ok(_) => String::new(),
      };
      return Err(BuildError::Types(format!("no declarations for {}; run `fsr types`{failure}", missing.join(", "))));
    }
    let checked = app.compile(&built, &mut plugins)?;
    if let Some(checked) = &checked {
      if checked.errors() > 0 {
        let lines: Vec<String> = checked.diagnostics.iter().map(|d| d.to_string()).collect();
        return Err(BuildError::Typecheck(format!("{}\n{}", checked.row(), lines.join("\n"))));
      }
    }
    for (rel, content) in built.refresh_assets() {
      let path = app.dir.join(&rel);
      std::fs::write(&path, content).map_err(|e| BuildError::Io(path.clone(), e))?;
      if !written.contains(&path) {
        written.push(path);
      }
    }
    report_derived(crate::assets::derive(&app.dir, &app.dir.join("dist"), &built.assets)?, app.options.verbose);
    Ok((written, checked))
  })();
  match rest {
    Ok((written, checked)) => Ok(Emitted { built, written, checked }),
    // The report is complete before the bundle runs and it holds the rows that
    // say why: a missing plugin is a `plugins` row here and an unresolvable
    // import there. Returning the error alone loses the half that names a fix.
    Err(e) => {
      print_report(&built.report, app.options.verbose);
      Err(e)
    }
  }
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
  for app in project.apps.iter().filter(|app| !app.served) {
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
    _ if project.mount.is_some() => println!("dev: watching {}, served under the shell at {shell} by the stock host; press Ctrl-C to stop", sites.join(", ")),
    (Some(cargo), true) => println!("dev: watching {shell} and the project at {}; press Ctrl-C to stop", cargo.display()),
    (Some(cargo), false) => println!("dev: watching {shell}, the sites {} and the project at {}; press Ctrl-C to stop", sites.join(", "), cargo.display()),
    (None, true) => println!("dev: watching {shell} and its configuration, served by the stock host; press Ctrl-C to stop"),
    (None, false) => println!("dev: watching {shell}, the sites {} and its configuration, served by the stock host; press Ctrl-C to stop", sites.join(", ")),
  }

  let mut tracked: Vec<Tracked> = project.apps.iter().map(Tracked::new).collect();
  let mut seen = Seen::default();
  let mut self_triggered = 0u32;
  let mut want = Change::everything(project.apps.len());
  let mut declared: BTreeMap<PathBuf, Vec<PathBuf>> = BTreeMap::new();
  let mut declared_watched: HashSet<PathBuf> = HashSet::new();
  loop {
    if want.any() {
      let mut restart = want.project || server.child.is_none();
      let rebuilds = want.project || want.apps.iter().any(Option::is_some);
      let mut reload = false;
      let mut failed = false;
      let mut refusals: Vec<String> = Vec::new();
      if let Err(e) = run_programs("before", &project.programs.before, &project.programs, &mut declared) {
        eprintln!("{e}");
        refusals.push(e.to_string());
        failed = true;
      }
      for (state, sources) in tracked.iter_mut().zip(want.apps.iter_mut()) {
        let Some(sources) = sources.take() else { continue };
        if state.app.served {
          continue;
        }
        if failed {
          state.pending.extend(sources);
          continue;
        }
        match state.rebuild(sources) {
          Ok(changed) => reload |= changed,
          Err(e) => {
            eprintln!("{e}");
            refusals.push(e.to_string());
            failed = true;
          }
        }
      }
      if !failed && reload && !restart {
        match project.shell().reload() {
          Ok(report) if project.verbose => print!("{report}"),
          Ok(_) => {}
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
            refusals.push(e.to_string());
            failed = true;
          }
        }
      } else if !failed && !reload && rebuilds {
        project.shell().notify_changed();
      }
      if !failed {
        if let Err(e) = run_programs("after", &project.programs.after, &project.programs, &mut declared) {
          eprintln!("{e}");
          refusals.push(e.to_string());
          failed = true;
        }
      }
      rewatch(&mut watcher, &mut watching, &mut declared_watched, declared.values().flatten())?;
      if failed {
        project.shell().notify_failed(&refusals.join("\n\n"));
        println!("dev: waiting for changes");
      }
    }
    let Some(batch) = collect(&rx, &mut server)? else {
      if project.verbose {
        println!("dev: stopping");
      }
      return Ok(());
    };
    let changed = seen.changed(batch.paths);
    let inputs: Vec<PathBuf> = declared.values().flatten().cloned().collect();
    want = project.classify(&changed, &inputs);
    if !want.any() {
      continue;
    }
    self_triggered = if batch.queued { self_triggered + 1 } else { 0 };
    if self_triggered >= SELF_TRIGGERED_LIMIT {
      if self_triggered == SELF_TRIGGERED_LIMIT {
        eprintln!("dev: rebuilt {SELF_TRIGGERED_LIMIT} times in a row on nothing but its own writes, last {}; waiting for an edit", describe(&root, &want.counted));
      }
      want = Change::default();
      continue;
    }
    if project.verbose {
      println!("dev: changed {}", describe(&root, &want.counted));
    }
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
fn watch(watcher: &mut RecommendedWatcher, watching: &mut HashSet<PathBuf>, path: PathBuf) -> Result<bool, BuildError> {
  if !path.exists() || watching.iter().any(|root| path.starts_with(root)) {
    return Ok(false);
  }
  watcher
    .watch(&path, RecursiveMode::Recursive)
    .map_err(|e| BuildError::Dev(format!("watch {}: {e}", path.display())))?;
  watching.insert(path);
  Ok(true)
}

/// Makes the programs' declared inputs the watched set they add: watches each new one and stops
/// watching each that no program declares any more. `added` holds the paths watched for a
/// declaration alone, so a path the project watches anyway is never unwatched. A declared path
/// that does not exist yet is watched once a later run declares it again and it does.
fn rewatch<'a>(watcher: &mut RecommendedWatcher, watching: &mut HashSet<PathBuf>, added: &mut HashSet<PathBuf>, declared: impl Iterator<Item = &'a PathBuf>) -> Result<(), BuildError> {
  let wanted: HashSet<&PathBuf> = declared.collect();
  for gone in added.iter().filter(|path| !wanted.contains(path)).cloned().collect::<Vec<_>>() {
    let _ = watcher.unwatch(&gone);
    watching.remove(&gone);
    added.remove(&gone);
  }
  for path in wanted {
    if watch(watcher, watching, path.clone())? {
      added.insert(path.clone());
    }
  }
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

/// One line for what the derive pass wrote under `dist/`, when it wrote anything.
fn report_derived(derived: crate::assets::Derived, verbose: bool) {
  if verbose && !derived.written.is_empty() {
    println!("{:<9} {} files under dist/, {} already there", "derived", derived.written.len(), derived.kept);
  }
}

/// A type error is printed and the server keeps running: the bundle carries
/// no types, so what is served is what the sources say either way.
fn report_types(checked: Option<&Checked>, verbose: bool) {
  let Some(checked) = checked else { return };
  for diagnostic in &checked.diagnostics {
    eprintln!("{diagnostic}");
  }
  if verbose || !checked.diagnostics.is_empty() {
    println!("typecheck {}", checked.row());
  }
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
fn collect(rx: &UnboundedSyncReceiver<Msg>, server: &mut Server) -> Result<Option<Batch>, BuildError> {
  let mut paths: HashSet<PathBuf> = HashSet::new();
  loop {
    match rx.try_recv() {
      Ok(Msg::Fs(event)) => absorb(event, &mut paths),
      Ok(Msg::Stop) | Err(TryRecvError::Disconnected) => return Ok(None),
      Err(TryRecvError::Empty) => break,
    }
  }
  let queued = !paths.is_empty();
  while paths.is_empty() {
    match rx.recv_timeout(POLL) {
      Ok(Msg::Fs(event)) => absorb(event, &mut paths),
      Ok(Msg::Stop) | Err(RecvErrorTimeout::Disconnected) => return Ok(None),
      Err(RecvErrorTimeout::Timeout) => server.poll()?,
    }
  }
  loop {
    match rx.recv_timeout(SETTLE) {
      Ok(Msg::Fs(event)) => absorb(event, &mut paths),
      Ok(Msg::Stop) | Err(RecvErrorTimeout::Disconnected) => return Ok(None),
      Err(RecvErrorTimeout::Timeout) => break,
    }
  }
  Ok(Some(Batch { paths: paths.into_iter().collect(), queued }))
}

fn absorb(event: notify::Result<notify::Event>, paths: &mut HashSet<PathBuf>) {
  match event {
    Ok(event) => paths.extend(event.paths),
    Err(e) => eprintln!("dev: watch error: {e}"),
  }
}

/// The `[build] before` and `after` projects of one application, resolved against its project
/// root, with the root they run from and the application they are told about.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Programs {
  pub root: PathBuf,
  pub app: PathBuf,
  pub before: Vec<PathBuf>,
  pub after: Vec<PathBuf>,
  /// `--verbose`: each program's `before` or `after` row is printed before it runs.
  pub verbose: bool,
}

/// `[build] before` and `after` of the configuration beside `app`; both empty when there is no
/// `[build]` section. The root and the application are absolute, since a program runs from the
/// root and a relative `app` names a path from where `fsr` ran.
pub fn programs_of(app: &Path) -> Programs {
  let root = crate::serve::project_root(app);
  let app = &app.canonicalize().unwrap_or_else(|_| app.to_path_buf());
  let Some(build) = crate::config_beside(app).and_then(|config| config.build) else { return Programs { root, app: app.to_path_buf(), ..Programs::default() } };
  Programs {
    before: build.before.iter().map(|project| root.join(project)).collect(),
    after: build.after.iter().map(|project| root.join(project)).collect(),
    root,
    app: app.to_path_buf(),
    verbose: false,
  }
}

/// The line a program prints to declare a path it reads, relative to `FSR_ROOT` or absolute.
pub const DECLARE: &str = "fsr:watch=";

/// Runs each of `projects`, the `[build] <key>` list of `programs`, with [`run_program`] in order,
/// stopping at the first that fails. Each that succeeds replaces what it declared in `declared`;
/// one that fails keeps what it declared last.
pub fn run_programs(key: &str, projects: &[PathBuf], programs: &Programs, declared: &mut BTreeMap<PathBuf, Vec<PathBuf>>) -> Result<(), BuildError> {
  for project in projects {
    if programs.verbose {
      println!("{key:<9} {}", project.strip_prefix(&programs.root).unwrap_or(project).display());
    }
    let inputs = run_program(&programs.root, key, project, &programs.app)?;
    declared.insert(project.clone(), inputs);
  }
  Ok(())
}

/// Builds `project`, named in `[build] <key>`, and runs its binary to completion from `root`,
/// with `FSR_ROOT` and `FSR_APP` naming the project and the application. The build runs with
/// [`OWNS_BUILD`] set, since `fsr` builds the application itself. Returns the paths the program
/// declared with [`DECLARE`] lines, which are not printed; every other line is.
pub fn run_program(root: &Path, key: &str, project: &Path, app: &Path) -> Result<Vec<PathBuf>, BuildError> {
  let shown = project.strip_prefix(root).unwrap_or(project).display().to_string();
  let fail = |why: String| BuildError::Dev(format!("[build] {key} {shown}: {why}"));
  if !project.join("Cargo.toml").is_file() {
    return Err(fail("no Cargo.toml there".to_owned()));
  }
  let mut child = Command::new("cargo")
    .args(["build", "--message-format=json-render-diagnostics"])
    .env(OWNS_BUILD, "1")
    .current_dir(project)
    .stdout(Stdio::piped())
    .spawn()
    .map_err(|e| fail(format!("cargo build: {e}")))?;
  let stdout = child.stdout.take().expect("a piped stdout");
  let executables: Vec<PathBuf> = BufReader::new(stdout).lines().map_while(Result::ok).filter_map(|line| executable_of(&line)).collect();
  let status = child.wait().map_err(|e| fail(format!("cargo build: {e}")))?;
  if !status.success() {
    return Err(fail(format!("cargo build exited with {status}")));
  }
  let [executable] = executables.as_slice() else {
    return Err(fail(format!("cargo build made {} binaries; a program is one", executables.len())));
  };
  let mut child = Command::new(executable)
    .current_dir(root)
    .env("FSR_ROOT", root)
    .env("FSR_APP", app)
    .stdout(Stdio::piped())
    .spawn()
    .map_err(|e| fail(format!("{}: {e}", executable.display())))?;
  let mut stdout = BufReader::new(child.stdout.take().expect("a piped stdout"));
  let mut inputs = Vec::new();
  let mut line = Vec::new();
  while stdout.read_until(b'\n', &mut line).map_err(|e| fail(format!("reading its output: {e}")))? > 0 {
    let text = String::from_utf8_lossy(&line);
    match declared_input(root, text.trim_end_matches(['\n', '\r'])) {
      Some(input) => inputs.push(input),
      None => print!("{text}"),
    }
    line.clear();
  }
  let status = child.wait().map_err(|e| fail(format!("{}: {e}", executable.display())))?;
  if !status.success() {
    return Err(fail(format!("exited with {status}")));
  }
  Ok(inputs)
}

/// The path a [`DECLARE`] line names, resolved against `root`.
fn declared_input(root: &Path, line: &str) -> Option<PathBuf> {
  let path = line.strip_prefix(DECLARE)?.trim();
  (!path.is_empty()).then(|| root.join(path))
}

#[cfg(test)]
mod tests {
  use super::*;

  fn app(dir: &Path) -> App {
    App { dir: dir.to_path_buf(), layout: Layout::default(), snapfirec: PathBuf::from("snapfirec"), options: DevOptions::default(), production: false, served: false }
  }

  fn scaffold(tag: &str, server: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("fsr-cli-dev-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    crate::new::create(&root, crate::new::NewOptions { fetch: false, ..crate::new::NewOptions::default() }).unwrap();
    let config = root.join("config/app.toml");
    let text = std::fs::read_to_string(&config).unwrap().replacen("[server]\n", &format!("[server]\n{server}"), 1);
    std::fs::write(&config, text).unwrap();
    std::fs::remove_file(root.join("config/development.toml")).unwrap();
    root
  }

  #[test]
  fn a_build_is_for_production_unless_server_dev_is_written_true() {
    for (tag, server, expected) in [("false", "dev = false\n", true), ("true", "dev = true\n", false), ("unwritten", "", true)] {
      let root = scaffold(tag, server);
      assert_eq!(production(&root.join("app")), expected, "server.dev {tag}");
      std::fs::remove_dir_all(&root).unwrap();
    }
  }

  fn args(app: &App) -> Vec<String> {
    app.compiler().get_args().map(|a| a.to_string_lossy().into_owned()).collect()
  }

  #[test]
  fn a_production_bundle_is_minified_and_any_other_is_not() {
    let mut built = app(Path::new("/p/app"));
    assert!(!args(&built).iter().any(|a| a == "--minify"));
    built.production = true;
    let args = args(&built);
    let at = args.iter().position(|a| a == "--minify").expect("--minify is passed");
    assert_eq!(args[at + 1], "compact");
  }

  #[test]
  fn a_vendor_tree_is_named_to_the_compiler_for_tailwind_and_none_is_not() {
    let root = std::env::temp_dir().join(format!("fsr-cli-dev-vendor-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    assert!(!args(&app(&root)).iter().any(|a| a == "--tailwind-vendor"), "no vendor directory, no flag");
    std::fs::create_dir_all(root.join("vendor")).unwrap();
    let args = args(&app(&root));
    let at = args.iter().position(|a| a == "--tailwind-vendor").expect("--tailwind-vendor is passed");
    assert_eq!(&args[at + 1..at + 4], ["vendor", "--tailwind-vendor-base", "/static/js/vendor"]);
    std::fs::remove_dir_all(&root).unwrap();
  }

  #[test]
  fn a_vendored_svelte_compiler_is_named_to_the_compiler() {
    let root = std::env::temp_dir().join(format!("fsr-cli-dev-svelte-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("vendor/svelte/compiler")).unwrap();
    assert!(!args(&app(&root)).iter().any(|a| a == "--plugin-compiler"), "nothing vendored, no flag");
    std::fs::write(root.join("vendor/svelte/compiler/index.js"), "globalThis.svelte = {};\n").unwrap();
    std::fs::write(root.join("vendor/.fsr-vendor.json"), r#"{"packages":{"svelte":{"version":"5.57.2","published":true,"files":["compiler/index.js"]}}}"#).unwrap();
    let args = args(&app(&root));
    let at = args.iter().position(|a| a == "--plugin-compiler").expect("--plugin-compiler is passed");
    assert_eq!(args[at + 1], format!("svelte={}", root.join("vendor/svelte/compiler/index.js").display()));
    std::fs::remove_dir_all(&root).unwrap();
  }

  fn project(shell: &Path) -> Project {
    Project { apps: vec![app(shell)], cargo: Some(shell.parent().unwrap().to_path_buf()), mount: None, programs: Programs::default(), verbose: false }
  }

  fn with_site(shell: &Path, site: &Path) -> Project {
    Project { apps: vec![app(shell), app(site)], cargo: Some(shell.parent().unwrap().to_path_buf()), mount: None, programs: Programs::default(), verbose: false }
  }

  #[test]
  fn generated_output_is_never_a_reason_to_rebuild() {
    let p = project(Path::new("/p/app"));
    let change = p.classify(&[PathBuf::from("/p/app/generated/plan.json"), PathBuf::from("/p/app/dist/src/main.js"), PathBuf::from("/p/app/types/react/index.d.ts"), PathBuf::from("/p/app/tsconfig.json"), PathBuf::from("/p/app/.fsr-dev"), PathBuf::from("/p/app/tests/cart/loader.test.ts"), PathBuf::from("/p/app/routes/cart/loader.test.ts")], &[]);
    assert!(!change.any());
  }

  #[test]
  fn app_sources_and_project_sources_are_told_apart() {
    let p = project(Path::new("/p/app"));
    let change = p.classify(&[PathBuf::from("/p/app/routes/index/page.tsx")], &[]);
    assert!(change.apps[0].is_some() && !change.project);
    let change = p.classify(&[PathBuf::from("/p/src/main.rs"), PathBuf::from("/p/config/app.toml")], &[]);
    assert!(change.apps[0] == Some(Vec::new()) && change.project);
    let change = p.classify(&[PathBuf::from("/p/app/.fsr-something"), PathBuf::from("/p/app/importmap.json")], &[]);
    assert!(change.apps[0].is_some());
  }

  #[test]
  fn a_batch_names_only_the_paths_that_rebuild() {
    let p = project(Path::new("/p/app"));
    let change = p.classify(&[PathBuf::from("/p/app/dist/src/main.js"), PathBuf::from("/p/app/.fsr-bundle/routes/page.tsx"), PathBuf::from("/p/app/routes/index/page.tsx"), PathBuf::from("/p/config/app.toml")], &[]);
    assert_eq!(change.counted, vec![PathBuf::from("/p/app/routes/index/page.tsx"), PathBuf::from("/p/config/app.toml")]);
  }

  #[test]
  fn a_changed_source_is_named_for_the_compiler() {
    let p = project(Path::new("/p/app"));
    let change = p.classify(&[PathBuf::from("/p/app/routes/index/page.tsx"), PathBuf::from("/p/app/dist/src/main.js")], &[]);
    assert_eq!(change.apps[0], Some(vec![PathBuf::from("/p/app/routes/index/page.tsx")]));
  }

  #[test]
  fn a_site_source_is_the_site_s_change_and_its_output_is_nobody_s() {
    let p = with_site(Path::new("/p/app"), Path::new("/p/sites/learn/app"));
    let change = p.classify(&[PathBuf::from("/p/sites/learn/app/src/ui/Prose.tsx")], &[]);
    assert_eq!(change.apps, vec![None, Some(vec![PathBuf::from("/p/sites/learn/app/src/ui/Prose.tsx")])]);
    assert!(!change.project);
    let change = p.classify(&[PathBuf::from("/p/sites/learn/app/types/react/index.d.ts"), PathBuf::from("/p/sites/learn/app/generated/plan.sexp"), PathBuf::from("/p/sites/learn/app/dist/src/main.js")], &[]);
    assert!(!change.any());
    let change = p.classify(&[PathBuf::from("/p/sites/learn/content/010.md")], &[]);
    assert!(change.project && change.apps.iter().all(|a| *a == Some(Vec::new())));
  }

  #[test]
  fn a_path_a_program_declared_reruns_the_programs_and_rebuilds_nothing() {
    let p = project(Path::new("/p/app"));
    let declared = [PathBuf::from("/p/guide")];
    let change = p.classify(&[PathBuf::from("/p/guide/010.md")], &declared);
    assert!(change.programs && !change.project && change.apps.iter().all(Option::is_none));
    let change = p.classify(&[PathBuf::from("/p/guide/010.md")], &[]);
    assert!(change.project, "undeclared, it is the project's");
    let change = p.classify(&[PathBuf::from("/p/config/app.toml")], &[PathBuf::from("/p/config")]);
    assert!(change.project, "a path the project watches anyway rebuilds the project");
  }

  #[test]
  fn a_declaration_is_a_path_against_the_root_and_any_other_line_is_output() {
    assert_eq!(declared_input(Path::new("/p"), "fsr:watch=guide"), Some(PathBuf::from("/p/guide")));
    assert_eq!(declared_input(Path::new("/p"), "fsr:watch=/elsewhere/guide"), Some(PathBuf::from("/elsewhere/guide")));
    assert_eq!(declared_input(Path::new("/p"), "fsr:watch="), None);
    assert_eq!(declared_input(Path::new("/p"), "wrote site/index.html"), None);
  }

  #[test]
  fn a_configuration_change_rebuilds_every_application_whole() {
    let p = with_site(Path::new("/p/app"), Path::new("/p/sites/learn/app"));
    let change = p.classify(&[PathBuf::from("/p/config/app.toml")], &[]);
    assert!(change.project);
    assert_eq!(change.apps, vec![Some(Vec::new()), Some(Vec::new())], "every application, no source named, which is a whole build");
    let change = p.classify(&[PathBuf::from("/p/app/routes/page.tsx"), PathBuf::from("/p/config/production.toml")], &[]);
    assert_eq!(change.apps[0], Some(Vec::new()), "a source saved beside it does not narrow the build");
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

  /// The npm registry as `fsr types` reads it, holding `htmx.org` 2.0.4 alone, whose tarball carries `dist/htmx.d.ts`.
  /// A registry serving `htmx.org`; `@htmx/core` at 1.0.0, 1.2.0 and 2.0.0, `latest` 2.0.0; and `@htmx/ext`, typed only through `exports` and asking for `@htmx/core` at `~1.0.0`.
  fn npm_stub() -> String {
    use std::io::{BufRead, BufReader, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let host = format!("http://{}", listener.local_addr().unwrap());
    let pack = |files: &[(&str, String)]| {
      let mut tarball = tar::Builder::new(flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default()));
      for (path, text) in files {
        let mut header = tar::Header::new_gnu();
        header.set_size(text.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        tarball.append_data(&mut header, format!("package/{path}"), text.as_bytes()).unwrap();
      }
      tarball.into_inner().unwrap().finish().unwrap()
    };
    let core = |version: &str| (version.to_owned(), serde_json::json!({ "name": "@htmx/core", "types": "index.d.ts" }), vec![("index.d.ts", format!("export declare const core: \"{version}\";\n"))]);
    let registry: Vec<(&str, &str, Vec<(String, serde_json::Value, Vec<(&str, String)>)>)> = vec![
      ("htmx.org", "2.0.4", vec![("2.0.4".to_owned(), serde_json::json!({ "name": "htmx.org", "types": "dist/htmx.d.ts" }), vec![("dist/htmx.d.ts", "export declare const version: string;\n".to_owned())])]),
      ("@htmx/core", "2.0.0", vec![core("1.0.0"), core("1.2.0"), core("2.0.0")]),
      (
        "@htmx/ext",
        "1.0.0",
        vec![(
          "1.0.0".to_owned(),
          serde_json::json!({ "name": "@htmx/ext", "exports": { ".": { "types": "./lib/index.d.ts", "default": "./lib/index.js" } }, "dependencies": { "@htmx/core": "~1.0.0" } }),
          vec![("lib/index.d.ts", "export * from '@htmx/core';\n".to_owned())],
        )],
      ),
    ];
    let mut routes: std::collections::HashMap<String, Vec<u8>> = std::collections::HashMap::new();
    for (name, latest, versions) in registry {
      let encoded = name.replace('/', "%2f");
      let listed: serde_json::Map<String, serde_json::Value> = versions.iter().map(|(v, _, _)| (v.clone(), serde_json::json!({}))).collect();
      routes.insert(format!("/{encoded}"), serde_json::json!({ "dist-tags": { "latest": latest }, "versions": listed }).to_string().into_bytes());
      for (version, manifest, files) in versions {
        let tarball = format!("/tgz/{encoded}-{version}.tgz");
        let mut doc = manifest.clone();
        doc["version"] = serde_json::Value::String(version.clone());
        doc["dist"] = serde_json::json!({ "tarball": format!("{host}{tarball}") });
        routes.insert(format!("/{encoded}/{version}"), doc.to_string().into_bytes());
        let mut packed = vec![("package.json", doc.to_string())];
        packed.extend(files);
        routes.insert(tarball, pack(&packed));
      }
    }
    std::thread::spawn(move || {
      for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        let mut line = String::new();
        let mut reader = BufReader::new(&stream);
        if reader.read_line(&mut line).is_err() {
          continue;
        }
        loop {
          let mut header = String::new();
          if reader.read_line(&mut header).map(|n| n == 0).unwrap_or(true) || header == "\r\n" {
            break;
          }
        }
        let path = line.split(' ').nth(1).unwrap_or("").to_owned();
        let head = match routes.get(&path) {
          Some(body) => format!("HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n", body.len()),
          None => "HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n".to_owned(),
        };
        let _ = stream.write_all(head.as_bytes());
        if let Some(body) = routes.get(&path) {
          let _ = stream.write_all(body);
        }
      }
    });
    host
  }

  /// A scaffold whose import map names `htmx.org`, built with no typecheck and a compiler that is not there, so the build stops at the bundle once the declarations are settled.
  fn htmx_app(tag: &str) -> (PathBuf, DevOptions) {
    let root = scaffold(tag, "");
    let app = root.join("app");
    let map = app.join("importmap.json");
    let mut json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&map).unwrap()).unwrap();
    json["imports"]["htmx.org"] = serde_json::Value::String("/static/js/vendor/htmx.org/htmx.mjs".to_owned());
    std::fs::write(&map, json.to_string()).unwrap();
    let mut options = DevOptions::beside(&app);
    options.typecheck.enabled = false;
    options.snapfirec = Some(root.join("no-snapfirec"));
    (app, options)
  }

  fn declares_htmx(app: &Path) -> bool {
    app.join("types/htmx.org/dist/htmx.d.ts").is_file() && crate::types::missing(app).unwrap().is_empty()
  }

  #[test]
  fn a_build_fetches_the_declarations_a_clean_checkout_lacks() {
    let (app, options) = htmx_app("fetch-clean");
    let _ = std::fs::remove_dir_all(app.join("types/htmx.org"));
    let result = emit_from(&app, options, &npm_stub());
    assert!(!matches!(&result, Err(BuildError::Types(_))), "{:?}", result.err());
    assert!(declares_htmx(&app), "the build fetched and recorded htmx.org");
  }

  /// A tree with the package directory committed and its entry missing.
  #[test]
  fn a_build_fetches_a_package_whose_directory_lacks_its_entry() {
    let (app, options) = htmx_app("fetch-entryless");
    std::fs::create_dir_all(app.join("types/htmx.org")).unwrap();
    std::fs::write(app.join("types/htmx.org/package.json"), r#"{"name":"htmx.org","types":"dist/htmx.d.ts"}"#).unwrap();
    let result = emit_from(&app, options, &npm_stub());
    assert!(!matches!(&result, Err(BuildError::Types(_))), "{:?}", result.err());
    assert!(declares_htmx(&app), "the build fetched htmx.org again");
  }

  /// `htmx.org`'s declarations kept from a fetch that did not follow what they import, as `vue`'s re-export of `@vue/runtime-dom` was before 0b1ccf9.
  fn keep_htmx_without_its_dependency(app: &Path) {
    keep_htmx_importing(app, &[("@htmx/core", "1")]);
  }

  /// `htmx.org`'s declarations kept, re-exporting each of `dependencies` at the range given, with none of them under `types/`.
  fn keep_htmx_importing(app: &Path, dependencies: &[(&str, &str)]) {
    let dir = app.join("types/htmx.org");
    std::fs::create_dir_all(dir.join("dist")).unwrap();
    let declared: serde_json::Map<String, serde_json::Value> = dependencies.iter().map(|(name, range)| ((*name).to_owned(), serde_json::Value::String((*range).to_owned()))).collect();
    std::fs::write(dir.join("package.json"), serde_json::json!({ "name": "htmx.org", "types": "dist/htmx.d.ts", "dependencies": declared }).to_string()).unwrap();
    std::fs::write(dir.join("dist/htmx.d.ts"), dependencies.iter().map(|(name, _)| format!("export * from '{name}';\n")).collect::<String>()).unwrap();
    let _ = std::fs::remove_dir_all(app.join("types/@htmx"));
    let mut manifest = crate::types::TypesManifest::read(app, &crate::xwpm::Layout::of(app).unwrap()).unwrap();
    manifest.packages.insert("htmx.org".to_owned(), crate::types::TypedPackage { version: "2.0.4".to_owned(), from: "htmx.org".to_owned(), entry: "dist/htmx.d.ts".to_owned(), ambient: false });
    manifest.write(app, &crate::xwpm::Layout::of(app).unwrap()).unwrap();
  }

  fn recorded(app: &Path, package: &str) -> Option<crate::types::TypedPackage> {
    crate::types::TypesManifest::read(app, &crate::xwpm::Layout::of(app).unwrap()).unwrap().packages.get(package).cloned()
  }

  /// `vue` 3.5.42 pins `@vue/runtime-dom` at 3.5.42 and was given 3.5.43, `latest`.
  #[test]
  fn a_build_fetches_a_dependency_at_the_range_its_dependent_declares() {
    let (app, options) = htmx_app("fetch-range");
    keep_htmx_importing(&app, &[("@htmx/core", "^1")]);
    let result = emit_from(&app, options, &npm_stub());
    assert!(!matches!(&result, Err(BuildError::Types(_))), "{:?}", result.err());
    assert_eq!(recorded(&app, "@htmx/core").map(|t| t.version), Some("1.2.0".to_owned()), "the highest ^1, not latest 2.0.0");
  }

  #[test]
  fn a_build_takes_the_highest_version_every_dependent_accepts() {
    let (app, options) = htmx_app("fetch-ranges");
    keep_htmx_importing(&app, &[("@htmx/core", "^1"), ("@htmx/ext", "1")]);
    let result = emit_from(&app, options, &npm_stub());
    assert!(!matches!(&result, Err(BuildError::Types(_))), "{:?}", result.err());
    assert_eq!(recorded(&app, "@htmx/core").map(|t| t.version), Some("1.0.0".to_owned()), "^1 from htmx.org and ~1.0.0 from @htmx/ext");
  }

  #[test]
  fn a_build_fetches_again_a_kept_dependency_its_dependent_does_not_accept() {
    let (app, options) = htmx_app("fetch-stale");
    keep_htmx_importing(&app, &[("@htmx/core", "^1")]);
    let core = app.join("types/@htmx/core");
    std::fs::create_dir_all(&core).unwrap();
    std::fs::write(core.join("package.json"), r#"{"name":"@htmx/core","version":"2.0.0","types":"index.d.ts"}"#).unwrap();
    std::fs::write(core.join("index.d.ts"), "export declare const core: \"2.0.0\";\n").unwrap();
    let layout = crate::xwpm::Layout::of(&app).unwrap();
    let mut manifest = crate::types::TypesManifest::read(&app, &layout).unwrap();
    manifest.packages.insert("@htmx/core".to_owned(), crate::types::TypedPackage { version: "2.0.0".to_owned(), from: "@htmx/core".to_owned(), entry: "index.d.ts".to_owned(), ambient: false });
    manifest.write(&app, &layout).unwrap();
    let result = emit_from(&app, options, &npm_stub());
    assert!(!matches!(&result, Err(BuildError::Types(_))), "{:?}", result.err());
    assert_eq!(recorded(&app, "@htmx/core").map(|t| t.version), Some("1.2.0".to_owned()));
    assert!(std::fs::read_to_string(core.join("index.d.ts")).unwrap().contains("1.2.0"));
  }

  #[test]
  fn a_range_no_version_meets_is_named_in_the_types_rows() {
    let (app, options) = htmx_app("fetch-unmet");
    keep_htmx_importing(&app, &[("@htmx/core", "^3")]);
    let result = emit_from(&app, options, &npm_stub());
    assert!(!matches!(&result, Err(BuildError::Types(_))), "{:?}", result.err());
    assert_eq!(recorded(&app, "@htmx/core").map(|t| t.version), Some("2.0.0".to_owned()), "latest, since nothing meets ^3");
    let rows = crate::types::status(&app).unwrap();
    assert!(rows.iter().any(|(package, row)| package == "@htmx/core" && row.contains("htmx.org asks ^3")), "{rows:?}");
  }

  /// `@babel/parser` 8 names its declarations only in `exports`.
  #[test]
  fn a_build_fetches_a_package_whose_types_only_exports_names() {
    let (app, options) = htmx_app("fetch-exports");
    keep_htmx_importing(&app, &[("@htmx/ext", "1")]);
    let result = emit_from(&app, options, &npm_stub());
    assert!(!matches!(&result, Err(BuildError::Types(_))), "{:?}", result.err());
    assert_eq!(recorded(&app, "@htmx/ext").map(|t| t.entry), Some("lib/index.d.ts".to_owned()));
    assert!(app.join("types/@htmx/core/index.d.ts").is_file(), "and what its declarations import");
  }

  #[test]
  fn a_build_fetches_what_the_declarations_it_keeps_import() {
    let (app, options) = htmx_app("fetch-dependency");
    keep_htmx_without_its_dependency(&app);
    let result = emit_from(&app, options, &npm_stub());
    assert!(!matches!(&result, Err(BuildError::Types(_))), "{:?}", result.err());
    assert!(app.join("types/@htmx/core/index.d.ts").is_file(), "the build fetched the package htmx.org's declarations re-export");
    assert_eq!(crate::types::missing(&app).unwrap(), Vec::<String>::new());
  }

  /// A package placed by hand has no manifest record to say where its declarations came from.
  #[test]
  fn a_build_fetches_what_an_unrecorded_packages_declarations_import() {
    let (app, options) = htmx_app("fetch-dependency-unrecorded");
    keep_htmx_without_its_dependency(&app);
    let layout = crate::xwpm::Layout::of(&app).unwrap();
    let mut manifest = crate::types::TypesManifest::read(&app, &layout).unwrap();
    manifest.packages.remove("htmx.org");
    manifest.write(&app, &layout).unwrap();
    std::fs::rename(app.join("types/htmx.org/dist/htmx.d.ts"), app.join("types/htmx.org/index.d.ts")).unwrap();
    let result = emit_from(&app, options, &npm_stub());
    assert!(!matches!(&result, Err(BuildError::Types(_))), "{:?}", result.err());
    assert!(app.join("types/@htmx/core/index.d.ts").is_file(), "the build fetched the package htmx.org's declarations re-export");
  }

  #[test]
  fn the_types_rows_name_a_dependency_a_kept_package_imports() {
    let (app, _) = htmx_app("status-dependency");
    keep_htmx_without_its_dependency(&app);
    let rows = crate::types::status(&app).unwrap();
    assert!(rows.iter().any(|(package, row)| package == "@htmx/core" && row.starts_with("missing")), "{rows:?}");
  }

  #[test]
  fn a_build_that_cannot_fetch_a_kept_packages_dependency_names_it() {
    let (app, options) = htmx_app("fetch-dependency-offline");
    keep_htmx_without_its_dependency(&app);
    match emit_from(&app, options, "http://127.0.0.1:1") {
      Err(BuildError::Types(message)) => assert!(message.contains("no declarations for @htmx/core; run `fsr types`"), "{message}"),
      other => panic!("the build went on to the typecheck: {:?}", other.err()),
    }
  }

  #[test]
  fn a_build_that_cannot_fetch_the_declarations_names_fsr_types_and_the_failure() {
    let (app, options) = htmx_app("fetch-offline");
    let _ = std::fs::remove_dir_all(app.join("types/htmx.org"));
    match emit_from(&app, options, "http://127.0.0.1:1") {
      Err(BuildError::Types(message)) => assert!(message.contains("no declarations for htmx.org; run `fsr types`") && message.contains("127.0.0.1:1"), "{message}"),
      other => panic!("{:?}", other.err()),
    }
  }

}

#[cfg(test)]
mod under_tests {
  use super::*;

  fn fresh(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("fsr-cli-under-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    root
  }

  /// A shell scaffold and a site scaffold linked into it, both with no fetch.
  fn shell_and_site(tag: &str) -> (PathBuf, PathBuf) {
    let shell = fresh(&format!("{tag}-shell"));
    crate::new::create(&shell, crate::new::NewOptions { fetch: false, shell: true, ..crate::new::NewOptions::default() }).unwrap();
    std::fs::create_dir_all(shell.join("app/generated")).unwrap();
    std::fs::write(shell.join("app/generated/shell.json"), r#"{"version":1}"#).unwrap();
    let site = fresh(&format!("{tag}-site"));
    crate::new::create(
      &site,
      crate::new::NewOptions { fetch: false, site: Some(crate::new::SiteScaffold { at: "/billing".to_owned(), name: Some("billing".to_owned()), into: Some(shell.clone()) }), ..crate::new::NewOptions::default() },
    )
    .unwrap();
    (shell, site)
  }

  #[test]
  fn a_site_finds_its_shell_through_the_contract_it_was_built_against_unless_told_to_run_alone() {
    let (shell, site) = shell_and_site("infer");
    let app = site.join("app");
    let found = shell_under(&app, &DevOptions::default()).unwrap().expect("the shell beside the contract");
    assert_eq!(found, shell.canonicalize().unwrap());
    let alone = DevOptions { alone: true, ..DevOptions::default() };
    assert_eq!(shell_under(&app, &alone).unwrap(), None);
    assert_eq!(shell_under(&shell.join("app"), &DevOptions::default()).unwrap(), None, "a shell is not under anything");
    let e = shell_under(&shell.join("app"), &DevOptions { under: Some(shell.clone()), ..DevOptions::default() }).unwrap_err().to_string();
    assert!(e.contains("is not a site"), "{e}");
  }

  #[test]
  fn under_takes_a_shells_root_or_its_app_directory_and_refuses_anything_else() {
    let (shell, site) = shell_and_site("given");
    let app = site.join("app");
    let by_root = shell_under(&app, &DevOptions { under: Some(shell.clone()), ..DevOptions::default() }).unwrap().unwrap();
    let by_app = shell_under(&app, &DevOptions { under: Some(shell.join("app")), ..DevOptions::default() }).unwrap().unwrap();
    assert_eq!(by_root, shell.canonicalize().unwrap());
    assert_eq!(by_app, by_root);
    let e = shell_under(&app, &DevOptions { under: Some(shell.join("app/importmap.json")), ..DevOptions::default() }).unwrap_err().to_string();
    assert!(e.contains("not a directory"), "{e}");
    let e = shell_under(&app, &DevOptions { under: Some(fresh("given-empty")), ..DevOptions::default() }).unwrap_err().to_string();
    assert!(e.contains("--under"), "{e}");
  }

  #[test]
  fn a_site_under_a_shell_runs_its_own_programs() {
    let (shell, site) = shell_and_site("programs");
    let config = site.join("config/app.toml");
    let text = std::fs::read_to_string(&config).unwrap();
    std::fs::write(&config, format!("{text}\n[build]\nbefore = [\"contents\"]\n")).unwrap();
    let project = Project::open(&site.join("app"), DevOptions { under: Some(shell.clone()), ..DevOptions::default() }).unwrap();
    let root = crate::serve::project_root(&site.join("app"));
    assert_eq!(project.programs.before, [root.join("contents")]);
    assert!(project.watched().contains(&root.join("contents/src")));
  }

  #[test]
  fn a_site_s_programs_run_from_the_site_and_are_told_the_site() {
    let (shell, site) = shell_and_site("programs-app");
    let app = site.join("app");
    let project = Project::open(&app, DevOptions { under: Some(shell.clone()), ..DevOptions::default() }).unwrap();
    assert_eq!(project.programs.root, crate::serve::project_root(&app));
    assert_eq!(project.programs.app, app.canonicalize().unwrap());
  }

  #[test]
  fn a_site_under_a_shell_is_the_one_app_built_and_the_shell_is_told_to_mount_it() {
    let (shell, site) = shell_and_site("project");
    let project = Project::open(&site.join("app"), DevOptions { under: Some(shell.clone()), ..DevOptions::default() }).unwrap();
    assert_eq!(project.apps.len(), 2);
    assert!(project.apps[0].served && !project.apps[1].served);
    assert_eq!(project.apps[0].dir, shell.join("app").canonicalize().unwrap());
    assert!(project.cargo.is_none());
    let args: Vec<String> = project.serve_args().iter().map(|a| a.to_string_lossy().into_owned()).collect();
    assert_eq!(args[0], "serve");
    assert_eq!(args[2], "--mount");
    assert_eq!(args[3], format!("billing={}", site.canonicalize().unwrap().display()));
    let change = project.classify(&[project.apps[0].dir.join("routes/page.tsx"), project.apps[1].dir.join("routes/page.tsx")], &[]);
    assert!(change.apps[0].is_none(), "nothing under a served shell is a change");
    assert!(change.apps[1].is_some() && !change.project);
  }
}

#[cfg(test)]
mod server_tests {
  use super::*;

  /// Polls until the child has been seen to exit, returning what `poll` said then.
  fn polled(command: &str) -> Result<(), BuildError> {
    let mut server = Server { child: Some(Command::new(command).spawn().unwrap()) };
    for _ in 0..500 {
      let result = server.poll();
      if server.child.is_none() {
        return result;
      }
      std::thread::sleep(Duration::from_millis(10));
    }
    panic!("{command} did not exit");
  }

  #[test]
  fn an_app_that_exits_0_stops_dev() {
    let e = polled("true").expect_err("an exit 0 is an error");
    assert!(e.to_string().contains("exited 0") && e.to_string().contains("`[build] before` or `after`"), "{e}");
  }

  #[test]
  fn an_app_that_fails_waits_for_the_next_change() {
    polled("false").expect("a failure is started again by the next change");
  }
}

