#[cfg(feature = "web")]
use std::path::Path;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};

use snapfire_fsr_cli::dev::DevOptions;
use snapfire_fsr_cli::doctor;
use snapfire_fsr_cli::new::{NewOptions, SiteScaffold};
use snapfire_fsr_cli::serve::ServeOptions;
use snapfire_fsr_cli::typecheck::{self, Typecheck};
#[cfg(feature = "web")]
use snapfire_fsr_cli::vendor::Spec;
#[cfg(feature = "web")]
use snapfire_fsr_cli::direction::{self, UseOptions};
use snapfire_fsr_cli::{check, dev, emit, ir, new, serve, Options};
#[cfg(feature = "web")]
use snapfire_fsr_cli::{sites, test, types};

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
  /// Scaffolds a bare project: configuration, routes and an import map, no framework; --with react adds React.
  New(New),
  /// Rewrites a Next.js, SvelteKit, Nuxt or Ink application into a new project, naming each line it left for a person.
  Migrate(Migrate),
  /// Builds and serves an application, rebuilding what changed as it changes.
  Dev(Dev),
  #[cfg(feature = "web")]
  /// Runs the application's tests.
  Test(Test),
  /// Serves an application that is already built.
  Serve(Serve),
  #[cfg(feature = "web")]
  /// Writes the documents every prerenderable route answers with.
  Prerender(Prerender),
  #[cfg(feature = "web")]
  /// Writes the deploy tree: everything the host reads, laid out as a server wants it.
  Bundle(Bundle),
  /// Builds an application without serving it.
  Build(Build),
  /// Reports what a deployment would find wrong before it ships.
  Doctor(Doctor),
  /// Builds and typechecks an application. It writes only what the checker reads (`generated/`, `tsconfig.json` and the type shims) and none of the deploy tree.
  Check(Check),
  #[cfg(feature = "web")]
  /// Counts what the build refuses or leaves to the browser across applications, by cause, writing nothing.
  Census(Census),
  #[cfg(feature = "web")]
  /// Gives an application a direction: react, vue, svelte, elements, htmx, tailwind, tera or mdx.
  Use(Use),
  #[cfg(feature = "web")]
  /// Vendors packages into the application and names them in its import map.
  Add(Add),
  #[cfg(feature = "web")]
  /// Writes the declarations for every package the import map names.
  Types(Types),
  #[cfg(feature = "web")]
  /// The sites a shell mounts.
  Sites(Sites),
  #[cfg(feature = "web")]
  /// The fonts an application serves.
  Fonts(Fonts),
  /// The lowered form the server runs, as the build wrote it.
  Ir(Ir),
}

#[derive(Args)]
struct Ir {
  #[command(subcommand)]
  command: IrCommand,
}

#[derive(Subcommand)]
enum IrCommand {
  /// Prints one body from the last build's plan: a source, action or handler by id, a component by module id or a derive by key.
  View {
    /// The application directory.
    app_dir: PathBuf,
    /// The row: `product.$id`, `cart.addToCart`, `api.cart.GET`, `routes/page.tsx#default` or `"order/total"`.
    id: String,
    /// One export of a source row: body, meta, store, defaults or paths.
    section: Option<String>,
  },
}

#[derive(Args)]
struct Migrate {
  /// The application to migrate: the directory holding its package.json.
  #[arg(required_unless_present = "schema")]
  from: Option<PathBuf>,
  /// The directory to create the project in.
  #[arg(required_unless_present = "schema")]
  project_dir: Option<PathBuf>,
  /// Skips vendoring packages and fetching declarations.
  #[arg(long)]
  no_fetch: bool,
  /// The migration map to read in place of the application's own `migrate.toml`.
  #[arg(long, value_name = "FILE")]
  map: Option<PathBuf>,
  /// Prints the JSON Schema of `migrate.toml` and exits.
  #[arg(long, conflicts_with_all = ["from", "project_dir", "map"])]
  schema: bool,
}

#[derive(Args)]
struct New {
  /// The directory to create the project in.
  project_dir: PathBuf,
  /// Skips vendoring packages and fetching declarations.
  #[arg(long)]
  no_fetch: bool,
  /// A direction to adopt once the project is written; repeatable. Without one the application serves no framework: pass --with react for a React project.
  #[arg(long = "with", value_name = "DIRECTION")]
  with: Vec<String>,
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
  /// Refuses a component that renders in the browser alone, as `[build] strict` does.
  #[arg(long)]
  strict: bool,
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
  /// Prints what lowered as well as what renders in the browser.
  #[arg(long)]
  verbose: bool,
  #[command(flatten)]
  typecheck: TypecheckFlags,
}

#[derive(Args)]
struct Dev {
  #[command(flatten)]
  build: Build,
  /// A shell to serve this site under: its project directory, an unpacked artifact or an archive. Found from `[site] shell` when absent.
  #[arg(long, value_name = "SHELL")]
  under: Option<PathBuf>,
  /// Serves a site by itself even when `[site] shell` names a shell it could run under.
  #[arg(long)]
  alone: bool,
  /// Opens the application in a native window over the system webview.
  #[cfg(feature = "desktop")]
  #[arg(long)]
  desktop: bool,
  /// Runs the root page in this terminal, drawn with Ink's elements and stepped by its useInput handler.
  #[cfg(feature = "terminal")]
  #[arg(long)]
  terminal: bool,
  /// The terminal page's command line, after `--`: `--name value` and positional arguments reach its loader as `params`.
  #[cfg(feature = "terminal")]
  #[arg(last = true, value_name = "ARG")]
  args: Vec<String>,
}

#[derive(Args)]
struct Check {
  /// The application directory.
  app_dir: PathBuf,
  /// Refuses a component that renders in the browser alone, as `[build] strict` does.
  #[arg(long)]
  strict: bool,
  /// The module id of the shell the routes render into.
  #[arg(long)]
  shell: Option<String>,
  /// The slot of the shell the routes fill.
  #[arg(long)]
  slot: Option<String>,
  /// Prints what lowered as well as what renders in the browser.
  #[arg(long)]
  verbose: bool,
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

#[cfg(feature = "web")]
#[derive(Args)]
struct Test {
  /// The application directory.
  app_dir: PathBuf,
  /// Runs only the tests whose name holds this, specs and Rust tests alike.
  filter: Option<String>,
  /// Runs the specs and not the Rust tests of the crate beside the app.
  #[arg(long, conflicts_with = "rust_only")]
  specs_only: bool,
  /// Runs the Rust tests of the crate beside the app and not the specs.
  #[arg(long)]
  rust_only: bool,
  #[command(flatten)]
  typecheck: TypecheckFlags,
}

#[derive(Args)]
struct Serve {
  /// The application directory.
  app_dir: PathBuf,
  /// The address to listen on, over the configured one.
  #[arg(long)]
  listen: Option<String>,
  /// Mounts a site from a path for this run, replacing the table's row of that name; repeatable.
  #[arg(long = "mount", value_name = "NAME=PATH")]
  mount: Vec<String>,
  /// Prints the whole boot report rather than only what needs attention.
  #[arg(long)]
  verbose: bool,
  /// Opens the application in a native window over the system webview, on an ephemeral loopback port unless --listen names one.
  #[cfg(feature = "desktop")]
  #[arg(long)]
  desktop: bool,
}

#[cfg(feature = "web")]
#[derive(Args)]
struct Census {
  /// The application directories.
  #[arg(required = true)]
  app_dirs: Vec<PathBuf>,
}

#[cfg(feature = "web")]
#[derive(Args)]
struct Prerender {
  /// The application directory.
  app_dir: PathBuf,
  /// The directory to write the documents into.
  #[arg(long)]
  out: Option<PathBuf>,
}

#[cfg(feature = "web")]
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
  /// Lists every directory the tree serves and every file the host reads.
  #[arg(long)]
  verbose: bool,
}

#[derive(Args)]
struct Doctor {
  /// The application directory.
  app_dir: PathBuf,
  /// Names each check that found nothing as well.
  #[arg(long)]
  verbose: bool,
  /// The release environment whose configuration ladder to check, over RELEASE_ENV.
  #[arg(long = "env", value_name = "RELEASE_ENV")]
  release_env: Option<String>,
  /// The application environment of the ladder, over APP_ENV.
  #[arg(long)]
  app_env: Option<String>,
  /// The region of the ladder, over APP_REGION.
  #[arg(long)]
  region: Option<String>,
  /// Runs in the deployment's own environment: a value the files leave to it is a finding when it is missing.
  #[arg(long)]
  runtime: bool,
}

#[cfg(feature = "web")]
#[derive(Args)]
struct Use {
  /// The application directory.
  app_dir: PathBuf,
  /// Directions to adopt, in order: react, vue, svelte, elements, htmx or tailwind; tera or mdx in an fsr built with that feature.
  #[arg(required = true)]
  directions: Vec<String>,
  /// Skips vendoring packages and fetching declarations.
  #[arg(long)]
  no_fetch: bool,
  /// Writes one example component per direction.
  #[arg(long)]
  example: bool,
}

#[cfg(feature = "web")]
#[derive(Args)]
struct Add {
  /// The application directory.
  app_dir: PathBuf,
  /// Packages to vendor, as name@version[/subpath]; none repairs the vendored modules' source maps alone.
  specs: Vec<String>,
  /// Packages to leave to the importing page, comma separated.
  #[arg(long)]
  external: Vec<String>,
  /// Vendors each package as published on npm, its module tree kept, rather than one bundle per subpath; for a package whose subpaths share state, as Svelte's do.
  #[arg(long, conflicts_with = "external")]
  published: bool,
}

#[cfg(feature = "web")]
#[derive(Args)]
struct Types {
  /// The application directory.
  app_dir: PathBuf,
  /// Rewrites the declarations a package already has.
  #[arg(long)]
  refresh: bool,
}

#[cfg(feature = "web")]
#[derive(Args)]
struct Sites {
  #[command(subcommand)]
  command: SitesCommand,
}

#[cfg(feature = "web")]
#[derive(Args)]
struct Fonts {
  #[command(subcommand)]
  command: FontsCommand,
}

#[cfg(feature = "web")]
#[derive(Subcommand)]
enum FontsCommand {
  /// Downloads a provider's faces into the font directory, as `google:Inter@400,700,400i`, so the build serves them as local files.
  Add {
    /// The application directory.
    app_dir: PathBuf,
    /// `google:<Family>@<weights>`, a weight ending in `i` for italic.
    spec: String,
  },
}

#[cfg(feature = "web")]
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
    /// The archive to write or the directory to write `<name>-<version>.tar.gz` into.
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
    /// Writes the pin into `config/<layer>.toml`, created when absent, instead of `app.toml`.
    #[arg(long)]
    layer: Option<String>,
  },
  /// Pins every mount to the hash its artifact has now.
  Pin {
    /// The shell directory.
    shell_dir: PathBuf,
    /// Pins only this mount.
    name: Option<String>,
    /// Writes the pin into `config/<layer>.toml`, created when absent, instead of `app.toml`.
    #[arg(long)]
    layer: Option<String>,
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

#[cfg(feature = "web")]
#[derive(Args)]
struct Remote {
  /// An instance to ask, repeatable.
  #[arg(long)]
  host: Vec<String>,
  /// A header to send, as "K: V", repeatable.
  #[arg(long)]
  header: Vec<String>,
}

#[cfg(feature = "web")]
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

#[cfg(feature = "web")]
impl Test {
  fn typechecked(&self) -> Typecheck {
    let mut typecheck = Typecheck::beside(&self.app_dir);
    self.typecheck.apply(&mut typecheck);
    typecheck
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

impl Dev {
  fn options(&self) -> DevOptions {
    let mut options = self.build.options();
    options.under = self.under.clone();
    options.alone = self.alone;
    #[cfg(feature = "desktop")]
    {
      options.desktop = self.desktop;
    }
    #[cfg(feature = "terminal")]
    {
      options.terminal = self.terminal;
      options.args = self.args.clone();
    }
    options
  }
}

impl Build {
  fn options(&self) -> DevOptions {
    let mut options = DevOptions::beside(&self.app_dir);
    options.verbose = self.verbose;
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
    options.build.strict |= self.strict;
    options
  }
}

#[cfg(feature = "web")]
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

/// The typecheck rows of a report and the exit code the diagnostics call for.
fn types_row(checked: Option<&typecheck::Checked>, verbose: bool) -> ExitCode {
  let Some(checked) = checked else { return ExitCode::SUCCESS };
  for diagnostic in &checked.diagnostics {
    println!("{diagnostic}");
  }
  if verbose || !checked.diagnostics.is_empty() {
    println!("typecheck {}", checked.row());
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

#[cfg(feature = "web")]
fn run_test(args: Test) -> ExitCode {
  let project = test::rust_project(&args.app_dir);
  if args.rust_only && project.is_none() {
    return refused(&format!("--rust-only: no Cargo.toml beside {}", args.app_dir.display()));
  }
  let mut passed = true;
  if !args.rust_only {
    match test::run(&args.app_dir, &Options::beside(&args.app_dir), &args.typechecked(), args.filter.as_deref()) {
      Ok(summary) => {
        print!("{summary}");
        passed &= summary.failed == 0;
      }
      Err(e) => return failed(e),
    }
  }
  if let Some(project) = project.filter(|_| !args.specs_only) {
    println!("\nrust tests: {}", std::fs::canonicalize(&project).unwrap_or_else(|_| project.clone()).display());
    let _ = std::io::Write::flush(&mut std::io::stdout());
    match test::run_rust(&project, args.filter.as_deref()) {
      Ok(run) => {
        println!("{}: {}", run.runner, if run.passed { "ok" } else { "FAILED" });
        passed &= run.passed;
      }
      Err(e) => return failed(e),
    }
  }
  if passed { ExitCode::SUCCESS } else { ExitCode::from(1) }
}

fn main() -> ExitCode {
  match Cli::parse().command {
    Command::New(args) => run_new(args),
    Command::Migrate(args) => run_migrate(args),
    Command::Dev(args) => match dev::run(&args.build.app_dir, args.options()) {
      Ok(()) => ExitCode::SUCCESS,
      Err(e) => failed(e),
    },
    #[cfg(feature = "web")]
    Command::Test(args) => run_test(args),
    Command::Serve(args) => {
      let mut mounts = Vec::new();
      for spec in &args.mount {
        let Some((name, path)) = spec.split_once('=').filter(|(name, path)| !name.is_empty() && !path.is_empty()) else {
          return refused(&format!("--mount takes NAME=PATH, not `{spec}`"));
        };
        mounts.push((name.to_owned(), PathBuf::from(path)));
      }
      let options = ServeOptions::default().listen(args.listen).mounts(mounts).verbose(args.verbose);
      #[cfg(feature = "desktop")]
      let options = options.desktop(args.desktop);
      match serve::run(&args.app_dir, options) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => failed(e),
      }
    }
    #[cfg(feature = "web")]
    Command::Census(args) => {
      print!("{}", snapfire_fsr_cli::census::run(&args.app_dirs));
      ExitCode::SUCCESS
    }
    #[cfg(feature = "web")]
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
    #[cfg(feature = "web")]
    Command::Bundle(args) => run_bundle(args),
    Command::Build(args) => {
      let mut programs = snapfire_fsr_cli::dev::programs_of(&args.app_dir);
      programs.verbose = args.verbose;
      let mut declared = std::collections::BTreeMap::new();
      if let Err(e) = snapfire_fsr_cli::dev::run_programs("before", &programs.before, &programs, &mut declared) {
        return failed(e);
      }
      match emit(&args.app_dir, args.options()) {
        Ok(emitted) => {
          snapfire_fsr_cli::dev::print_report(&emitted.built.report, args.verbose);
          if args.verbose {
            for path in emitted.written {
              println!("wrote {}", path.display());
            }
          }
          let code = types_row(emitted.checked.as_ref(), args.verbose);
          match snapfire_fsr_cli::dev::run_programs("after", &programs.after, &programs, &mut declared) {
            Ok(()) => code,
            Err(e) => failed(e),
          }
        }
        Err(e) => failed(e),
      }
    }
    Command::Doctor(args) => match doctor::run_with(&snapfire_fsr_cli::serve::project_root(&args.app_dir), &doctor_options(&args)) {
      Ok(report) => {
        if args.verbose {
          print!("{}", report.verbose());
        } else {
          print!("{report}");
        }
        if report.is_clean() { ExitCode::SUCCESS } else { ExitCode::from(1) }
      }
      Err(e) => {
        eprintln!("{e}");
        ExitCode::from(2)
      }
    },
    Command::Check(args) => run_check(args),
    #[cfg(feature = "web")]
    Command::Use(args) => run_use(args),
    #[cfg(feature = "web")]
    Command::Add(args) => run_add(args),
    #[cfg(feature = "web")]
    Command::Types(args) => match types::fetch(&args.app_dir, args.refresh) {
      Ok(report) => {
        for command in &report.delegated {
          println!("ran       {command}");
        }
        for (package, version, from) in &report.fetched {
          println!("types     {package:<28} {from} {version}");
        }
        if let Some((path, version)) = &report.pinned {
          println!("recorded  typecheck.version = \"{version}\" in {}", path.display());
        }
        for package in &report.kept {
          println!("kept      {package}");
        }
        for (package, why) in &report.missing {
          println!("missing   {package:<28} {why}");
        }
        for (package, version, ask) in &report.unmet {
          println!("unmet     {package:<28} {version}; {} asks {}", ask.by, ask.range);
        }
        for path in &report.written {
          println!("wrote     {path}");
        }
        ExitCode::SUCCESS
      }
      Err(e) => failed(e),
    },
    #[cfg(feature = "web")]
    Command::Sites(args) => run_sites(args.command),
    Command::Ir(args) => match args.command {
      IrCommand::View { app_dir, id, section } => match ir::view(&app_dir, &id, section.as_deref()) {
        Ok(row) => {
          print!("{row}");
          ExitCode::SUCCESS
        }
        Err(e) => failed(e),
      },
    },
    #[cfg(feature = "web")]
    Command::Fonts(args) => match args.command {
      FontsCommand::Add { app_dir, spec } => match snapfire_fsr_cli::assets::add(&app_dir, &spec) {
        Ok(written) => {
          for path in &written {
            println!("{:<9} {}", "fetched", path.strip_prefix(&app_dir).unwrap_or(path).display());
          }
          println!("{:<9} {} files; `fsr build` serves them from the font directory", "fonts", written.len());
          ExitCode::SUCCESS
        }
        Err(e) => refused(&e.to_string()),
      },
    },
  }
}

fn run_new(args: New) -> ExitCode {
  let mut options = NewOptions::default().fetch(!args.no_fetch).with(args.with).shell(args.shell);
  if args.site {
    let Some(at) = args.at else {
      return refused("fsr new --site needs --at <path>, the prefix a shell mounts it under");
    };
    options.site = Some(SiteScaffold::new(at).name(args.name).into(args.into));
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
      if let Some((path, version)) = &created.pinned {
        println!("recorded  typecheck.version = \"{version}\" in {}", path.display());
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

fn doctor_options(args: &Doctor) -> doctor::DoctorOptions {
  let options = doctor::DoctorOptions::default().runtime(args.runtime);
  if args.release_env.is_none() && args.app_env.is_none() && args.region.is_none() {
    return options;
  }
  let mut deployment = snapfire_fsr_host::config::Deployment::from_env();
  if let Some(release_env) = &args.release_env {
    deployment = deployment.release_env(release_env.clone());
  }
  if let Some(app_env) = &args.app_env {
    deployment = deployment.app_env(app_env.clone());
  }
  if args.region.is_some() {
    deployment = deployment.region(args.region.clone());
  }
  options.deployment(deployment)
}

fn run_migrate(args: Migrate) -> ExitCode {
  if args.schema {
    println!("{}", serde_json::to_string_pretty(&snapfire_fsr_migrate::map_schema()).expect("a schema serializes"));
    return ExitCode::SUCCESS;
  }
  let (Some(from), Some(project_dir)) = (args.from.as_deref(), args.project_dir.clone()) else { return ExitCode::FAILURE };
  match snapfire_fsr_cli::migrate::run(from, &project_dir, !args.no_fetch, args.map.as_deref()) {
    Ok(migrated) => {
      if let Some(source) = migrated.report.source {
        println!("from      {}", source.name());
      }
      println!("wrote     {} files under {}", migrated.written.len(), project_dir.display());
      for (specifier, file, bytes) in &migrated.created.vendored {
        println!("added     {specifier:<28} {file}  {bytes} bytes");
      }
      for note in &migrated.report.notes {
        eprintln!("note      {note}");
      }
      let app = project_dir.join("app");
      if !migrated.report.packages.is_empty() {
        let specs: Vec<String> = migrated
          .report
          .packages
          .iter()
          .map(|(name, range)| match range.as_str() {
            "" | "*" | "latest" => format!("{name}@latest"),
            range => format!("{name}@{range}"),
          })
          .collect();
        println!("next      fsr add {} {}", app.display(), specs.join(" "));
      }
      for command in &migrated.report.commands {
        println!("next      {command}");
      }
      println!("next      fsr check {}", app.display());
      ExitCode::SUCCESS
    }
    Err(e) => failed(e),
  }
}

#[cfg(feature = "web")]
fn run_bundle(args: Bundle) -> ExitCode {
  let out = args
    .out
    .unwrap_or_else(|| snapfire_fsr_cli::serve::project_root(&args.app_dir).join("dist"));
  match snapfire_fsr_cli::bundle::run_checked(&args.app_dir, &out, !args.no_doctor) {
    Ok(bundled) => {
      if args.verbose {
        for (route, at) in &bundled.served {
          println!("{at:<32} serves {route}");
        }
        for path in &bundled.read {
          if !bundled.served.iter().any(|(_, at)| at == path) {
            println!("{path:<32} read by the host");
          }
        }
        println!();
      }
      println!("{} files, {} bytes under {}", bundled.files, bundled.bytes, bundled.out.display());
      println!("place beside it: {}", bundled.beside.join(", "));
      ExitCode::SUCCESS
    }
    Err(e) => failed(e),
  }
}

fn run_check(args: Check) -> ExitCode {
  let mut options = Options::beside(&args.app_dir);
  options.strict |= args.strict;
  if let Some(shell) = &args.shell {
    options.shell = shell.clone();
  }
  if let Some(slot) = &args.slot {
    options.slot = slot.clone();
  }
  let mut typecheck = Typecheck::beside(&args.app_dir);
  args.typecheck.apply(&mut typecheck);
  match check(&args.app_dir, &options, &typecheck, |built| snapfire_fsr_cli::dev::print_report(&built.report, args.verbose)) {
    Ok((_, checked)) => types_row(checked.as_ref(), args.verbose),
    Err(e) => failed(e),
  }
}

#[cfg(feature = "web")]
fn run_use(args: Use) -> ExitCode {
  match direction::adopt(&args.app_dir, &args.directions, UseOptions::default().fetch(!args.no_fetch).example(args.example)) {
    Ok(adopted) => {
      for (specifier, url) in &adopted.mapped {
        println!("mapped    {specifier:<28} {url}");
      }
      for specifier in &adopted.present {
        println!("present   {specifier}");
      }
      for spec in &adopted.delegated {
        println!("xwpm add  {spec}");
      }
      for (specifier, file, bytes) in &adopted.vendored {
        println!("added     {specifier:<28} {file}  {bytes} bytes");
      }
      for (specifier, url) in &adopted.from_shell {
        println!("shell     {specifier:<28} {url}");
      }
      for specifier in &adopted.kept {
        println!("kept      {specifier}");
      }
      for (package, version, from) in &adopted.typed {
        println!("types     {package:<28} {from} {version}");
      }
      if let Some((path, version)) = &adopted.pinned {
        println!("recorded  typecheck.version = \"{version}\" in {}", path.display());
      }
      for path in &adopted.written {
        println!("wrote     {}", path.display());
      }
      for note in &adopted.notes {
        eprintln!("note      {note}");
      }
      for (file, line) in &adopted.edits {
        println!("edit      {file}: {line}");
      }
      for step in &adopted.next {
        println!("next      {step}");
      }
      ExitCode::SUCCESS
    }
    Err(e) => failed(e),
  }
}

#[cfg(feature = "web")]
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
  let added = match snapfire_fsr_cli::add::add(&args.app_dir, specs, &externals, args.published) {
    Ok(added) => added,
    Err(e) => return failed(e),
  };
  for report in added.groups {
    if !externals.is_empty() && !report.delegated.is_empty() {
      eprintln!("note: xwpm carries dependencies itself; --external was not used");
    }
    for spec in &report.delegated {
      println!("xwpm add  {spec}");
    }
    for (specifier, file, bytes) in report.added {
      println!("added     {specifier:<28} {file}  {bytes} bytes");
    }
    for (specifier, url) in report.from_shell {
      println!("shell     {specifier:<28} {url}");
    }
    for (specifier, url) in report.remapped {
      println!("remapped  {specifier:<28} {url}");
    }
    for module in &report.repaired {
      println!("map       {module}");
    }
  }
  for (package, version, from) in &added.typed {
    println!("types     {package:<28} {from} {version}");
  }
  if let Some((path, version)) = &added.pinned {
    println!("recorded  typecheck.version = \"{version}\" in {}", path.display());
  }
  for path in &added.written {
    println!("wrote     {}", path.display());
  }
  for note in &added.notes {
    eprintln!("note      {note}");
  }
  ExitCode::SUCCESS
}

#[cfg(feature = "web")]
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
      layer,
    } => {
      let keep = keep.and_then(|n| n.parse::<usize>().ok());
      match sites::install(&shell_dir, &archive, name.as_deref(), keep, !no_pin, layer.as_deref()) {
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
            Some(pinned) if pinned.moved() => println!("pinned    [sites.{}] hash = \"{}\" in {}", pinned.name, pinned.hash, pinned.file.display()),
            Some(pinned) => println!("pinned    [sites.{}] already {}", pinned.name, pinned.hash),
            None => println!("next      artifact = \"{}@{}\" in [sites.{}]", installed.name, installed.version, installed.name),
          }
          ExitCode::SUCCESS
        }
        Err(e) => failed(e),
      }
    }
    SitesCommand::Pin { shell_dir, name, layer } => match sites::pin(&shell_dir, name.as_deref(), layer.as_deref()) {
      Ok(pinned) => {
        if pinned.is_empty() {
          println!("pinned    nothing: a mount naming a path is a working tree and is never pinned");
        }
        for entry in &pinned {
          match (&entry.was, entry.moved()) {
            (Some(was), true) => println!("repinned  [sites.{}] {was} -> {} in {}", entry.name, entry.hash, entry.file.display()),
            (_, true) => println!("pinned    [sites.{}] hash = \"{}\" in {}", entry.name, entry.hash, entry.file.display()),
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

#[cfg(feature = "web")]
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
