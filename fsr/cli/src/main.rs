use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};

use snapfire_fsr_cli::dev::DevOptions;
use snapfire_fsr_cli::doctor;
use snapfire_fsr_cli::new::{NewOptions, SiteScaffold};
use snapfire_fsr_cli::serve::ServeOptions;
use snapfire_fsr_cli::typecheck::{self, Typecheck};
use snapfire_fsr_cli::vendor::Spec;
use snapfire_fsr_cli::{build, dev, emit, new, serve, sites, test, types, vendor, Options};

#[derive(Parser)]
#[command(
  name = "fsr",
  about = "SnapFire FSR: TypeScript routes, loaders and components, built and served by Rust.",
  version,
  arg_required_else_help = true,
  subcommand_required = true,
  // `sites pack --version <version>` names a release rather than asking for
  // this binary's, so the flag stays off the subcommands.
  propagate_version = false
)]
struct Cli {
  #[command(subcommand)]
  command: Command,
}

#[derive(Subcommand)]
enum Command {
  /// Scaffolds a project: configuration, routes, an import map and the vendored packages.
  New(New),
  /// Builds and serves an application, rebuilding what changed as it changes.
  Dev(Build),
  /// Runs the application's tests.
  Test(Test),
  /// Serves an application that is already built.
  Serve(Serve),
  /// Writes the documents every prerenderable route answers with.
  Prerender(Prerender),
  /// Writes the deploy tree: everything the host reads, laid out as a server wants it.
  Bundle(Bundle),
  /// Builds an application without serving it.
  Build(Build),
  /// Reports what a deployment would find wrong before it ships.
  Doctor(Doctor),
  /// Builds and typechecks an application, writing nothing.
  Check(Check),
  /// Vendors packages into the application and names them in its import map.
  Add(Add),
  /// Writes the declarations for every package the import map names.
  Types(Types),
  /// The sites a shell mounts.
  Sites(Sites),
}

#[derive(Args)]
struct New {
  /// The directory to create the project in.
  project_dir: PathBuf,
  /// Skips vendoring packages and fetching declarations.
  #[arg(long)]
  no_fetch: bool,
  /// Scaffolds a shell, which is an application that mounts sites.
  #[arg(long)]
  shell: bool,
  /// Scaffolds a site, which is an application a shell mounts.
  #[arg(long)]
  site: bool,
  /// The prefix a shell mounts the site under. Belongs to --site.
  #[arg(long)]
  at: Option<String>,
  /// The name the mount is known by, defaulting to the project's. Belongs to --site.
  #[arg(long)]
  name: Option<String>,
  /// The shell to write the mount into. Belongs to --site.
  #[arg(long)]
  into: Option<PathBuf>,
}

#[derive(Args)]
struct Build {
  /// The application directory.
  app_dir: PathBuf,
  /// The module id of the shell the routes render into.
  #[arg(long)]
  shell: Option<String>,
  /// The slot of the shell the routes fill.
  #[arg(long)]
  slot: Option<String>,
  /// The URL prefix the built modules are served under.
  #[arg(long)]
  public_path: Option<String>,
  /// The compiler that bundles the application.
  #[arg(long)]
  snapfirec: Option<PathBuf>,
  #[command(flatten)]
  typecheck: TypecheckFlags,
}

#[derive(Args)]
struct Check {
  /// The application directory.
  app_dir: PathBuf,
  /// The module id of the shell the routes render into.
  #[arg(long)]
  shell: Option<String>,
  /// The slot of the shell the routes fill.
  #[arg(long)]
  slot: Option<String>,
  #[command(flatten)]
  typecheck: TypecheckFlags,
}

#[derive(Args)]
struct TypecheckFlags {
  /// Builds without typechecking.
  #[arg(long)]
  no_typecheck: bool,
  /// The TypeScript compiler to typecheck with.
  #[arg(long)]
  tsc: Option<PathBuf>,
  /// The TypeScript version to typecheck against.
  #[arg(long)]
  tsc_version: Option<String>,
  /// The SnapFire typechecker to typecheck with.
  #[arg(long)]
  snapfiretc: Option<PathBuf>,
}

#[derive(Args)]
struct Test {
  /// The application directory.
  app_dir: PathBuf,
  /// Runs only the tests whose name holds this.
  filter: Option<String>,
}

#[derive(Args)]
struct Serve {
  /// The application directory.
  app_dir: PathBuf,
  /// The address to listen on, over the configured one.
  #[arg(long)]
  listen: Option<String>,
}

#[derive(Args)]
struct Prerender {
  /// The application directory.
  app_dir: PathBuf,
  /// The directory to write the documents into.
  #[arg(long)]
  out: Option<PathBuf>,
}

#[derive(Args)]
struct Bundle {
  /// The application directory.
  app_dir: PathBuf,
  /// The directory to write the tree into, defaulting to the project's dist/.
  #[arg(long)]
  out: Option<PathBuf>,
  /// Bundles without running the checks first.
  #[arg(long)]
  no_doctor: bool,
}

#[derive(Args)]
struct Doctor {
  /// The application directory.
  app_dir: PathBuf,
}

#[derive(Args)]
struct Add {
  /// The application directory.
  app_dir: PathBuf,
  /// Packages to vendor, as name@version[/subpath].
  #[arg(required = true)]
  specs: Vec<String>,
  /// Packages to leave to the importing page, comma separated.
  #[arg(long)]
  external: Vec<String>,
}

#[derive(Args)]
struct Types {
  /// The application directory.
  app_dir: PathBuf,
  /// Rewrites the declarations a package already has.
  #[arg(long)]
  refresh: bool,
}

#[derive(Args)]
struct Sites {
  #[command(subcommand)]
  command: SitesCommand,
}

#[derive(Subcommand)]
enum SitesCommand {
  /// The sites a shell mounts, plus what every instance serves when hosts are given.
  List {
    /// The shell directory.
    shell_dir: PathBuf,
    #[command(flatten)]
    remote: Remote,
  },
  /// Tells every instance to mount its sites again.
  Reload {
    /// The shell directory, which names the hosts when none are given.
    shell_dir: Option<PathBuf>,
    #[command(flatten)]
    remote: Remote,
    /// Asks every host rather than stopping at the first refusal.
    #[arg(long)]
    all: bool,
  },
  /// What an artifact hashes to, which is what a mount is pinned against.
  Hash {
    /// The site directory.
    site_dir: PathBuf,
    /// Lists every file the hash covers.
    #[arg(long)]
    files: bool,
  },
  /// Packs an artifact into an archive a shell installs.
  Pack {
    /// The site directory.
    site_dir: PathBuf,
    /// The release this artifact is.
    #[arg(long)]
    version: Option<String>,
    /// The archive to write.
    #[arg(short = 'o', long)]
    out: Option<String>,
  },
  /// Unpacks an archive into a shell's site directory.
  Install {
    /// The shell directory.
    shell_dir: PathBuf,
    /// The archive to install.
    archive: PathBuf,
    /// The name to install it under, over the archive's own.
    #[arg(long = "as")]
    name: Option<String>,
    /// How many versions of the site to keep.
    #[arg(long)]
    keep: Option<String>,
    /// Installs without pinning the mount to the hash.
    #[arg(long)]
    no_pin: bool,
  },
  /// Pins every mount to the hash its artifact has now.
  Pin {
    /// The shell directory.
    shell_dir: PathBuf,
    /// Pins only this mount.
    name: Option<String>,
  },
  /// Writes both halves of a mount: the site's [site] and the shell's [sites.<name>].
  Link {
    /// The shell directory.
    shell_dir: PathBuf,
    /// The site directory.
    site_dir: PathBuf,
    /// The prefix the shell mounts the site under.
    #[arg(long)]
    at: Option<String>,
    /// The name the mount is known by, defaulting to the site's.
    #[arg(long)]
    name: Option<String>,
  },
  /// Removes a mount from a shell.
  Unlink {
    /// The shell directory.
    shell_dir: PathBuf,
    /// The mount to remove.
    name: String,
    /// Leaves the site's own [site] in place.
    #[arg(long)]
    keep_site: bool,
  },
}

#[derive(Args)]
struct Remote {
  /// An instance to ask, repeatable.
  #[arg(long)]
  host: Vec<String>,
  /// A header to send, as "K: V", repeatable.
  #[arg(long)]
  header: Vec<String>,
}

impl Remote {
  /// The headers to send. `$FSR_SITES_HEADER` joins them so a token stays out
  /// of history. A malformed one is an error rather than a usage dump.
  fn headers(&self) -> Result<Vec<(String, String)>, snapfire_fsr_cli::BuildError> {
    let mut raw = self.header.clone();
    if let Ok(from_env) = std::env::var("FSR_SITES_HEADER") {
      if !from_env.is_empty() {
        raw.push(from_env);
      }
    }
    raw.iter().map(|h| sites::header(h)).collect()
  }
}

impl TypecheckFlags {
  fn apply(&self, typecheck: &mut Typecheck) {
    if self.no_typecheck {
      typecheck.enabled = false;
    }
    if let Some(tsc) = &self.tsc {
      typecheck.tsc = Some(tsc.clone());
    }
    if let Some(version) = &self.tsc_version {
      typecheck.version = Some(version.clone());
    }
    if let Some(checker) = &self.snapfiretc {
      typecheck.checker = Some(checker.clone());
    }
  }
}

impl Build {
  fn options(&self) -> DevOptions {
    let mut options = DevOptions::beside(&self.app_dir);
    if let Some(shell) = &self.shell {
      options.build.shell = shell.clone();
    }
    if let Some(slot) = &self.slot {
      options.build.slot = slot.clone();
    }
    if let Some(public_path) = &self.public_path {
      options.public_path = public_path.clone();
    }
    if let Some(snapfirec) = &self.snapfirec {
      options.snapfirec = Some(snapfirec.clone());
    }
    self.typecheck.apply(&mut options.typecheck);
    options
  }
}

/// The shell's table against what every instance serves, which is what a fleet
/// is watched with.
fn list_against(shell: &Path, hosts: &[String], headers: &[(String, String)]) -> ExitCode {
  match sites::compare(shell, hosts, headers) {
    Ok(rows) => {
      let mut lagging = 0;
      for row in &rows {
        let at = row.row.at.as_deref().unwrap_or("-");
        println!("site      {:<20} {:<24} {:<8} {:<18} table", row.row.name, at, row.row.version, row.row.hash);
        for (host, mounted) in &row.against {
          match mounted {
            Some(m) if m.version == row.row.version && m.hash == row.row.hash => {
              println!("          {:<20} {:<24} {:<8} {:<18} ok", host, m.at, m.version, m.hash)
            }
            Some(m) => println!("          {:<20} {:<24} {:<8} {:<18} lags", host, m.at, m.version, m.hash),
            None => println!("          {:<20} {:<24} {:<8} {:<18} absent", host, "-", "-", "-"),
          }
        }
        if !row.agrees() {
          lagging += 1;
        }
      }
      if lagging > 0 { ExitCode::from(1) } else { ExitCode::SUCCESS }
    }
    Err(e) => {
      eprintln!("{e}");
      ExitCode::from(1)
    }
  }
}

/// The typecheck rows of a report, and the exit code the diagnostics call for.
fn types_row(checked: Option<&typecheck::Checked>) -> ExitCode {
  let Some(checked) = checked else { return ExitCode::SUCCESS };
  for diagnostic in &checked.diagnostics {
    println!("{diagnostic}");
  }
  println!("typecheck {}", checked.row());
  if let Some(path) = &checked.recorded {
    println!("recorded  typecheck.version = \"{}\" in {}", checked.version, path.display());
  }
  if checked.errors() > 0 { ExitCode::from(1) } else { ExitCode::SUCCESS }
}

fn failed(e: impl std::fmt::Display) -> ExitCode {
  eprintln!("{e}");
  ExitCode::from(1)
}

fn refused(message: &str) -> ExitCode {
  eprintln!("{message}");
  ExitCode::from(2)
}

fn main() -> ExitCode {
  match Cli::parse().command {
    Command::New(args) => run_new(args),
    Command::Dev(args) => match dev::run(&args.app_dir, args.options()) {
      Ok(()) => ExitCode::SUCCESS,
      Err(e) => failed(e),
    },
    Command::Test(args) => match test::run(&args.app_dir, &Options::beside(&args.app_dir), args.filter.as_deref()) {
      Ok(summary) => {
        print!("{summary}");
        if summary.failed == 0 { ExitCode::SUCCESS } else { ExitCode::from(1) }
      }
      Err(e) => failed(e),
    },
    Command::Serve(args) => {
      let options = ServeOptions {
        listen: args.listen,
        ..ServeOptions::default()
      };
      match serve::run(&args.app_dir, options) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => failed(e),
      }
    }
    Command::Prerender(args) => match serve::prerender(&args.app_dir, args.out.as_deref()) {
      Ok(written) => {
        if written.is_empty() {
          println!("nothing to prerender or warm: every source reads the request");
        }
        for (pattern, file) in written {
          println!("{pattern:<22} {}", file.display());
        }
        ExitCode::SUCCESS
      }
      Err(e) => failed(e),
    },
    Command::Bundle(args) => run_bundle(args),
    Command::Build(args) => match emit(&args.app_dir, args.options()) {
      Ok(emitted) => {
        print!("{}", emitted.built.report);
        for path in emitted.written {
          println!("wrote {}", path.display());
        }
        types_row(emitted.checked.as_ref())
      }
      Err(e) => failed(e),
    },
    Command::Doctor(args) => match doctor::run(&snapfire_fsr_cli::serve::project_root(&args.app_dir)) {
      Ok(report) => {
        print!("{report}");
        if report.is_clean() { ExitCode::SUCCESS } else { ExitCode::from(1) }
      }
      Err(e) => {
        eprintln!("{e}");
        ExitCode::from(2)
      }
    },
    Command::Check(args) => run_check(args),
    Command::Add(args) => run_add(args),
    Command::Types(args) => match types::fetch(&args.app_dir, args.refresh) {
      Ok(report) => {
        for command in &report.delegated {
          println!("ran       {command}");
        }
        for (package, version, from) in &report.fetched {
          println!("types     {package:<28} {from} {version}");
        }
        for package in &report.kept {
          println!("kept      {package}");
        }
        for (package, why) in &report.missing {
          println!("missing   {package:<28} {why}");
        }
        ExitCode::SUCCESS
      }
      Err(e) => failed(e),
    },
    Command::Sites(args) => run_sites(args.command),
  }
}

fn run_new(args: New) -> ExitCode {
  let mut options = NewOptions {
    fetch: !args.no_fetch,
    shell: args.shell,
    ..NewOptions::default()
  };
  if args.site {
    let Some(at) = args.at else {
      return refused("fsr new --site needs --at <path>, the prefix a shell mounts it under");
    };
    options.site = Some(SiteScaffold {
      at,
      name: args.name,
      into: args.into,
    });
  } else if args.at.is_some() || args.name.is_some() || args.into.is_some() {
    return refused("--at, --name and --into belong to --site");
  }
  match new::create(&args.project_dir, options) {
    Ok(created) => {
      for path in &created.written {
        println!("wrote     {}", path.display());
      }
      for (specifier, file, bytes) in &created.vendored {
        println!("added     {specifier:<28} {file}  {bytes} bytes");
      }
      for (package, version, from) in &created.typed {
        println!("types     {package:<28} {from} {version}");
      }
      if let Some(linked) = &created.linked {
        println!("wrote     [site] {} at {} in {}", linked.name, linked.at, linked.site_config.display());
        println!("wrote     [sites.{}] in {}", linked.name, linked.shell_config.display());
      }
      for note in &created.notes {
        eprintln!("note      {note}");
      }
      for step in &created.next {
        println!("next      {step}");
      }
      ExitCode::SUCCESS
    }
    Err(e) => failed(e),
  }
}

fn run_bundle(args: Bundle) -> ExitCode {
  let out = args
    .out
    .unwrap_or_else(|| snapfire_fsr_cli::serve::project_root(&args.app_dir).join("dist"));
  match snapfire_fsr_cli::bundle::run_checked(&args.app_dir, &out, !args.no_doctor) {
    Ok(bundled) => {
      for (route, at) in &bundled.served {
        println!("{at:<32} serves {route}");
      }
      for path in &bundled.read {
        if !bundled.served.iter().any(|(_, at)| at == path) {
          println!("{path:<32} read by the host");
        }
      }
      println!("\n{} files, {} bytes under {}", bundled.files, bundled.bytes, bundled.out.display());
      println!("place beside it: {}", bundled.beside.join(", "));
      ExitCode::SUCCESS
    }
    Err(e) => failed(e),
  }
}

fn run_check(args: Check) -> ExitCode {
  let mut options = Options::beside(&args.app_dir);
  if let Some(shell) = &args.shell {
    options.shell = shell.clone();
  }
  if let Some(slot) = &args.slot {
    options.slot = slot.clone();
  }
  let mut typecheck = Typecheck::beside(&args.app_dir);
  args.typecheck.apply(&mut typecheck);
  match build(&args.app_dir, &options) {
    Ok(built) => {
      print!("{}", built.report);
      match typecheck::run(&args.app_dir, &typecheck) {
        Ok(checked) => types_row(checked.as_ref()),
        Err(e) => failed(e),
      }
    }
    Err(e) => failed(e),
  }
}

fn run_add(args: Add) -> ExitCode {
  let mut specs = Vec::new();
  for arg in &args.specs {
    match Spec::parse(arg) {
      Ok(spec) => specs.push(spec),
      Err(e) => return refused(&e.to_string()),
    }
  }
  let externals: Vec<String> = args
    .external
    .iter()
    .flat_map(|value| value.split(','))
    .map(|s| s.trim().to_owned())
    .filter(|s| !s.is_empty())
    .collect();
  match vendor::add(&args.app_dir, &specs, &externals) {
    Ok(report) => {
      if !externals.is_empty() && !report.delegated.is_empty() {
        eprintln!("note: xwpm carries dependencies itself; --external was not used");
      }
      for spec in &report.delegated {
        println!("xwpm add  {spec}");
      }
      for (specifier, file, bytes) in report.added {
        println!("added     {specifier:<28} {file}  {bytes} bytes");
      }
      ExitCode::SUCCESS
    }
    Err(e) => failed(e),
  }
}

fn run_sites(command: SitesCommand) -> ExitCode {
  match command {
    SitesCommand::List { shell_dir, remote } => {
      let headers = match remote.headers() {
        Ok(headers) => headers,
        Err(e) => return refused(&e.to_string()),
      };
      if !remote.host.is_empty() {
        return list_against(&shell_dir, &remote.host, &headers);
      }
      match sites::list(&shell_dir) {
        Ok(rows) => {
          if rows.is_empty() {
            println!("no sites mounted");
          }
          for row in &rows {
            let at = row.at.as_deref().unwrap_or("-");
            println!("site      {:<20} {:<24} {:<8} {}", row.name, at, row.version, row.hash);
            println!("          {}", row.artifact);
            if let Some(note) = &row.note {
              println!("          {note}");
            }
          }
          match sites::cached(&shell_dir) {
            Ok(cached) => {
              for (name, versions) in &cached {
                println!("cached    {:<20} {}", name, versions.join(" "));
              }
            }
            Err(e) => eprintln!("{e}"),
          }
          ExitCode::SUCCESS
        }
        Err(e) => failed(e),
      }
    }
    SitesCommand::Reload { shell_dir, remote, all } => {
      let headers = match remote.headers() {
        Ok(headers) => headers,
        Err(e) => return refused(&e.to_string()),
      };
      let hosts = match sites::hosts_for(shell_dir.as_deref(), &remote.host) {
        Ok(hosts) => hosts,
        Err(e) => return refused(&e.to_string()),
      };
      match sites::reload(&hosts, &headers, all) {
        Ok(answers) => {
          for answer in &answers {
            match (&answer.refused, &answer.sites) {
              (Some(why), _) => println!("refused   {:<24} {why}", answer.host),
              (None, Some(sites)) => {
                println!("reloaded  {:<24} {} sites", answer.host, sites.len());
                for site in sites {
                  println!("          {:<20} {:<24} {:<8} {}", site.name, site.at, site.version, site.hash);
                }
              }
              (None, None) => println!("reloaded  {}", answer.host),
            }
          }
          if answers.iter().any(|a| !a.ok()) {
            if !all && answers.len() < hosts.len() {
              println!("stopped   {} of {} asked; what was published is refused", answers.len(), hosts.len());
            }
            return ExitCode::from(1);
          }
          ExitCode::SUCCESS
        }
        Err(e) => failed(e),
      }
    }
    SitesCommand::Hash { site_dir, files } => match sites::hash(&site_dir) {
      Ok(hashed) => {
        println!("site      {} at {}", hashed.name, hashed.at);
        println!("hash      {}", hashed.hash);
        println!("ships     {} files, {}", hashed.files.len(), bytes(hashed.bytes));
        for part in &hashed.parts {
          println!("          {part}");
        }
        if files {
          for file in &hashed.files {
            println!("file      {:<12} {:<64} {}", file.size, file.sha256, file.path);
          }
        }
        ExitCode::SUCCESS
      }
      Err(e) => failed(e),
    },
    SitesCommand::Pack { site_dir, version, out } => {
      let Some(version) = version else {
        return refused("fsr sites pack needs --version <version>, the release this artifact is");
      };
      match sites::pack(&site_dir, &version, out.as_deref().map(Path::new)) {
        Ok(packed) => {
          println!("packed    {} {}", packed.manifest.name, packed.manifest.version);
          println!("hash      {}", packed.manifest.hash);
          println!(
            "wrote     {} ({} of {} in {} files)",
            packed.out.display(),
            bytes(packed.bytes),
            bytes(packed.unpacked),
            packed.manifest.files.len()
          );
          ExitCode::SUCCESS
        }
        Err(e) => failed(e),
      }
    }
    SitesCommand::Install {
      shell_dir,
      archive,
      name,
      keep,
      no_pin,
    } => {
      let keep = keep.and_then(|n| n.parse::<usize>().ok());
      match sites::install(&shell_dir, &archive, name.as_deref(), keep, !no_pin) {
        Ok(out) => {
          let installed = &out.installed;
          if installed.held {
            println!("held      {} {} already at {}", installed.name, installed.version, installed.path.display());
          } else {
            println!("installed {} {}", installed.name, installed.version);
            println!("hash      {}", installed.hash);
            println!("at        {}", installed.path.display());
          }
          for version in &installed.swept {
            println!("removed   {} {version}", installed.name);
          }
          match &out.pinned {
            Some(pinned) if pinned.moved() => println!("pinned    [sites.{}] hash = \"{}\"", pinned.name, pinned.hash),
            Some(pinned) => println!("pinned    [sites.{}] already {}", pinned.name, pinned.hash),
            None => println!("next      artifact = \"{}@{}\" in [sites.{}]", installed.name, installed.version, installed.name),
          }
          ExitCode::SUCCESS
        }
        Err(e) => failed(e),
      }
    }
    SitesCommand::Pin { shell_dir, name } => match sites::pin(&shell_dir, name.as_deref()) {
      Ok(pinned) => {
        if pinned.is_empty() {
          println!("pinned    nothing: a mount naming a path is a working tree and is never pinned");
        }
        for entry in &pinned {
          match (&entry.was, entry.moved()) {
            (Some(was), true) => println!("repinned  [sites.{}] {was} -> {}", entry.name, entry.hash),
            (_, true) => println!("pinned    [sites.{}] hash = \"{}\"", entry.name, entry.hash),
            (_, false) => println!("held      [sites.{}] already {}", entry.name, entry.hash),
          }
        }
        ExitCode::SUCCESS
      }
      Err(e) => failed(e),
    },
    SitesCommand::Link {
      shell_dir,
      site_dir,
      at,
      name,
    } => {
      let Some(at) = at else {
        return refused("fsr sites link needs --at <path>, the prefix the shell mounts the site under");
      };
      match sites::link(&shell_dir, &site_dir, &at, name.as_deref()) {
        Ok(linked) => {
          if linked.site_kept {
            println!("kept      [site] {} at {} in {}", linked.name, linked.at, linked.site_config.display());
          } else {
            println!("wrote     [site] {} at {} in {}", linked.name, linked.at, linked.site_config.display());
            println!("          shell = {}", linked.shell_json);
          }
          println!("wrote     [sites.{}] in {}", linked.name, linked.shell_config.display());
          println!("          artifact = {}", linked.artifact);
          for command in &linked.next {
            println!("next      {command}");
          }
          ExitCode::SUCCESS
        }
        Err(e) => failed(e),
      }
    }
    SitesCommand::Unlink {
      shell_dir,
      name,
      keep_site,
    } => match sites::unlink(&shell_dir, &name, keep_site) {
      Ok(unlinked) => {
        println!("removed   [sites.{}] from {}", unlinked.name, unlinked.shell_config.display());
        match &unlinked.site_config {
          Some(path) => println!("removed   [site] from {}", path.display()),
          None => println!("kept      the site's own [site]"),
        }
        ExitCode::SUCCESS
      }
      Err(e) => failed(e),
    },
  }
}

/// A byte count for a report line, three significant figures and a unit.
fn bytes(count: u64) -> String {
  const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB"];
  let mut size = count as f64;
  let mut unit = 0;
  while size >= 1024.0 && unit + 1 < UNITS.len() {
    size /= 1024.0;
    unit += 1;
  }
  if unit == 0 {
    format!("{count} B")
  } else {
    format!("{size:.1} {}", UNITS[unit])
  }
}
