use crate::compiler::{Compiler, Dialect, MapRequest, Markup, Minify, Output};
use crate::config::{MapMode, MapOptions, TsConfig};
use crate::graph::Graph;
use crate::importmap::ImportMap;
use crate::sources;
use crate::transforms::Import;
use anyhow::{Context, Result};
use base64::Engine;
use browserslist::{Opts, execute};
use lightningcss::targets::Browsers;
use rayon::prelude::*;
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

/// Records what this build produced so the next one can remove what it no longer produces. Only
/// paths listed here are ever deleted, so an output directory shared with hand-written files stays
/// safe.
const MANIFEST: &str = ".snapfirec-manifest";

/// Emitted for the page to consume, listing what each entry point statically depends on.
const PRELOAD_MANIFEST: &str = "preload-manifest.json";

pub struct Options {
  pub root: PathBuf,
  pub config_path: PathBuf,
  pub out_dir_flag: Option<PathBuf>,
  pub strip_log: bool,
  pub strip_debug: bool,
  pub copy_assets: bool,
  pub source_map: bool,
  pub inline_source_map: bool,
  pub minify: Option<Minify>,
  pub declaration: bool,
  /// Prefix for the URLs the preload manifest names. Absent keeps the manifest in the output
  /// directory's own terms, which is what keeps a build usable at any mount point.
  pub public_path: Option<String>,
  pub import_map: Option<PathBuf>,
}

/// Everything a rebuild needs to reuse without re-reading the config.
pub struct Build {
  pub out_dir: PathBuf,
  pub root_dir: PathBuf,
  pub files: Vec<PathBuf>,
  pub search_bases: Vec<PathBuf>,
  pub include_patterns: Vec<String>,
  pub map_options: MapOptions,
  /// Whether a `.d.ts` is emitted beside each TypeScript output, from the flag or from `tsconfig`.
  pub declaration: bool,
  pub compiler: Compiler,
  pub claimed: HashMap<PathBuf, PathBuf>,
  /// Bare specifiers the emitted modules carry. A browser cannot resolve these on its own, so the
  /// page has to supply an import map covering every one of them.
  pub externals: Vec<String>,
  pub graph: Graph,
  /// Every relative specifier the output carries, paired with the module that
  /// named it. Collected rather than checked in place, because whether a target
  /// was produced is only knowable once every job and every asset has landed.
  pub references: Vec<(PathBuf, Import)>,
  surfaces: HashMap<PathBuf, Surface>,
  pub emitted: usize,
  pub has_error: bool,
}

#[derive(Clone, Copy)]
enum Asset {
  Script {
    dialect: Dialect,
    markup: Markup,
    out_ext: &'static str,
  },
  Style,
}

pub fn classify(path: &Path) -> bool {
  asset(path).is_some()
}

fn asset(path: &Path) -> Option<Asset> {
  let name = path.file_name()?.to_str()?;

  if name.ends_with(".d.ts") || name.ends_with(".d.tsx") {
    return None;
  }

  let ext = path.extension()?.to_str()?.to_ascii_lowercase();

  match ext.as_str() {
    "ts" => Some(Asset::Script {
      dialect: Dialect::TypeScript,
      markup: Markup::Denied,
      out_ext: "js",
    }),
    "tsx" => Some(Asset::Script {
      dialect: Dialect::TypeScript,
      markup: Markup::Allowed,
      out_ext: "js",
    }),
    "js" | "jsx" => Some(Asset::Script {
      dialect: Dialect::JavaScript,
      markup: Markup::Allowed,
      out_ext: "js",
    }),
    "mjs" => Some(Asset::Script {
      dialect: Dialect::JavaScript,
      markup: Markup::Allowed,
      out_ext: "mjs",
    }),
    "css" => Some(Asset::Style),
    _ => None,
  }
}

pub fn full(opts: &Options, banner: bool) -> Result<Build> {
  let tsconfig = TsConfig::load(&opts.config_path)?;
  let compiler_options = tsconfig.compiler_options.unwrap_or_default();

  crate::config::check_target(compiler_options.target.as_deref())?;

  let map_options = MapOptions::resolve(&compiler_options, opts.source_map, opts.inline_source_map)?;
  let declaration = opts.declaration || compiler_options.declaration.unwrap_or(false);

  let config_dir = opts
    .config_path
    .parent()
    .filter(|p| !p.as_os_str().is_empty())
    .unwrap_or(Path::new("."))
    .canonicalize()
    .with_context(|| format!("Failed to resolve the directory holding {:?}", opts.config_path))?;

  let out_dir = match (&opts.out_dir_flag, compiler_options.out_dir) {
    (Some(flag), _) => opts.root.join(flag),
    (None, Some(configured)) => config_dir.join(configured),
    (None, None) => opts.root.join("dist"),
  };

  // Resolving the output directory must not create it, so a run with nothing to
  // compile leaves the tree as it found it. A directory that is not there yet
  // excludes nothing from the source walk, which is all `select` wants it for.
  let out_dir = match out_dir.canonicalize() {
    Ok(resolved) => resolved,
    Err(_) => std::path::absolute(&out_dir)
      .with_context(|| format!("Failed to resolve absolute path of {:?}", out_dir))?,
  };

  let selection = sources::select(sources::Request {
    config_dir: &config_dir,
    out_dir: &out_dir,
    files: tsconfig.files,
    include: tsconfig.include,
    exclude: tsconfig.exclude,
    root_dir: compiler_options.root_dir,
    is_input: &|path| classify(path),
  })?;

  let targets = Browsers::load_browserslist().context("Failed to resolve browser targets")?;

  if banner {
    let distribs = execute(&Opts::default()).context("Failed to execute browserslist")?;
    let query = distribs
      .iter()
      .map(|d| format!("{} {}", d.name(), d.version()))
      .collect::<Vec<_>>()
      .join(", ");

    println!("🔥 snapfirec started");
    println!("   Root:     {:?}", opts.root);
    println!("   Config:   {:?}", opts.config_path);
    println!("   Root Dir: {}", display(&selection.root_dir, &opts.root));
    println!("   Output:   {}", display(&out_dir, &opts.root));
    println!("   Sources:  {:?}", selection.include_patterns);
    println!("   Browser Targets: '{}'", query);

    if selection.include_defaulted {
      eprintln!(
        "⚠️  No 'include' or 'files' in {:?}: compiling every file under {:?}. Set 'include' to narrow it.",
        opts.config_path, opts.root
      );
    }

    for pattern in &selection.unmatched_patterns {
      eprintln!("⚠️  'include' pattern {:?} matched no files", pattern);
    }

    if targets.is_none() {
      eprintln!("⚠️  No browser targets resolved: CSS will be compiled without downlevelling or prefixing");
    }
  }

  let mut build = Build {
    out_dir,
    root_dir: selection.root_dir,
    files: selection.files,
    search_bases: selection.search_bases,
    include_patterns: selection.include_patterns,
    map_options,
    declaration,
    compiler: Compiler::new(targets),
    claimed: HashMap::new(),
    externals: Vec::new(),
    graph: Graph::default(),
    references: Vec::new(),
    surfaces: HashMap::new(),
    emitted: 0,
    has_error: false,
  };

  // `select` matches every file, compilable or not, so an output directory is
  // earned by what this build would actually write: something to compile, or
  // something to copy when `--copy-assets` asks for it. A referenced asset
  // cannot arrive on its own, since only a compiled module can name one.
  let produces = if opts.copy_assets {
    !build.files.is_empty()
  } else {
    build.files.iter().any(|path| classify(path))
  };

  // Nothing to write and no previous output: nothing to create, clean or
  // record. The caller reports it, and `--watch` still starts, since waiting
  // for files that are not there yet is the whole point of watching.
  if !produces && !build.out_dir.exists() {
    return Ok(build);
  }

  fs::create_dir_all(&build.out_dir).with_context(|| format!("Failed to create {:?}", build.out_dir))?;

  let jobs = plan(opts, &mut build);

  // Compiling is the expensive part and each job owns a distinct output path, so it fans out. The
  // planning pass above and the reporting pass below stay serial, which keeps collision detection,
  // progress order and error order identical to a single-threaded build.
  let results: Vec<JobResult> = jobs
    .par_iter()
    .map(|job| run(&build.compiler, opts, build.map_options, job))
    .collect();

  let mut referenced: Vec<PathBuf> = Vec::new();

  for result in results {
    report(&mut build, result, &mut referenced);
  }

  copy_assets(opts, &mut build, referenced);
  check_graph(opts, &mut build);

  build.externals.sort();
  build.externals.dedup();

  if !build.externals.is_empty() {
    let listed: Vec<String> = build.externals.iter().map(|e| format!("'{e}'")).collect();
    println!("   Externals: {}", listed.join(", "));

    match &opts.import_map {
      Some(path) => check_externals(opts, &mut build, path)?,
      None => println!("   These need an import map in the page; nothing in the output resolves them."),
    }
  }

  write_manifest(opts, &mut build);

  if !build.has_error {
    prune_stale(opts, &build)?;
  }

  Ok(build)
}

/// Recompiles one already-selected file in place. Nothing structural is re-checked, so this never
/// prunes and never re-runs collision detection against entries the file already owns.
pub fn refresh(opts: &Options, build: &mut Build, path: &Path) {
  let Some(jobs) = jobs_for(opts, build, path, false) else {
    return;
  };

  let mut referenced = Vec::new();

  for job in &jobs {
    let result = run(&build.compiler, opts, build.map_options, job);
    report(build, result, &mut referenced);
  }

  check_graph(opts, build);
}

/// What one job emits from its source. The minified graph and the declarations are both further
/// outputs of a single input, so each is a variant of a job rather than a pass of its own.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Emit {
  Code(Option<Minify>),
  Declaration,
}

impl Emit {
  fn minify(self) -> Option<Minify> {
    match self {
      Self::Code(level) => level,
      Self::Declaration => None,
    }
  }
}

struct Job {
  source: PathBuf,
  relative: PathBuf,
  dest: PathBuf,
  asset: Asset,
  emit: Emit,
  source_name: String,
}

struct JobResult {
  emit: Emit,
  log: String,
  failure: Option<String>,
  referenced: Vec<PathBuf>,
  externals: Vec<String>,
  imports: Vec<Import>,
  surface: Surface,
  dest: PathBuf,
  written: bool,
}

/// What one emitted module offers importers, with `export *` left as edges to
/// follow rather than names, since the target may not have been compiled yet.
#[derive(Default, Clone)]
struct Surface {
  names: BTreeSet<String>,
  stars: Vec<PathBuf>,
  open: bool,
}

/// Resolves every output path, registers it and creates its directory. Serial on purpose: this is
/// where collisions are detected, and the answer must not depend on which thread got there first.
fn plan(opts: &Options, build: &mut Build) -> Vec<Job> {
  let mut jobs = Vec::new();

  for path in build.files.clone() {
    if let Some(mut planned) = jobs_for(opts, build, &path, true) {
      jobs.append(&mut planned);
    }
  }

  jobs
}

fn jobs_for(opts: &Options, build: &mut Build, path: &Path, check_collisions: bool) -> Option<Vec<Job>> {
  let asset = asset(path)?;

  let relative = path.strip_prefix(&build.root_dir).unwrap_or(path).to_path_buf();
  let mut dest = build.out_dir.join(&relative);

  if let Asset::Script { out_ext, .. } = &asset {
    dest.set_extension(out_ext);
  }

  let mut variants = vec![(dest.clone(), Emit::Code(None))];

  if let Some(level) = opts.minify {
    variants.push((with_min(&dest), Emit::Code(Some(level))));
  }

  // Declarations describe types, which the minified graph shares, so one file's worth is emitted
  // whatever else is asked for.
  if build.declaration
    && let Asset::Script {
      dialect: Dialect::TypeScript,
      ..
    } = asset
  {
    let mut declaration = dest.clone();
    declaration.set_extension("d.ts");
    variants.push((declaration, Emit::Declaration));
  }

  let mut jobs = Vec::new();

  for (dest, emit) in variants {
    if check_collisions
      && let Some(previous) = build.claimed.insert(dest.clone(), path.to_path_buf())
      && previous != path
    {
      eprintln!(
        "❌ Output collision on {}: {:?} and {:?} compile to the same file",
        display(&dest, &opts.root),
        previous,
        path
      );
      build.has_error = true;
      continue;
    }

    if build.map_options.mode == MapMode::External && emit != Emit::Declaration {
      build.claimed.insert(with_suffix(&dest, ".map"), path.to_path_buf());
    }

    if let Some(parent) = dest.parent()
      && let Err(e) = fs::create_dir_all(parent)
    {
      eprintln!(
        "❌ Error creating output directory {}: {}",
        display(parent, &opts.root),
        e
      );
      build.has_error = true;
      continue;
    }

    let source_name = relative_from(dest.parent().unwrap_or(&build.out_dir), path);

    jobs.push(Job {
      source: path.to_path_buf(),
      relative: relative.clone(),
      dest,
      asset,
      emit,
      source_name,
    });
  }

  Some(jobs)
}

fn run(compiler: &Compiler, opts: &Options, map_options: MapOptions, job: &Job) -> JobResult {
  let map = MapRequest {
    options: map_options,
    source_name: &job.source_name,
  };

  let suffix = match job.emit {
    Emit::Code(Some(_)) => " (min)",
    Emit::Code(None) => "",
    Emit::Declaration => " (dts)",
  };

  let (label, compiled) = match (job.emit, job.asset) {
    (Emit::Declaration, _) => ("TS", crate::declarations::declare(&job.source).map(Output::text)),
    (emit, Asset::Script { dialect, markup, .. }) => {
      let label = match dialect {
        Dialect::TypeScript => "TS",
        Dialect::JavaScript => "JS",
      };
      (
        label,
        compiler.compile_script(
          &job.source,
          dialect,
          markup,
          opts.strip_log,
          opts.strip_debug,
          emit.minify(),
          map,
        ),
      )
    }
    (emit, Asset::Style) => ("CSS", compiler.compile_css(&job.source, emit.minify().is_some(), map)),
  };

  let log = format!("   Compiling {}{}: {:?}", label, suffix, job.relative);

  let mut output = match compiled {
    Ok(output) => output,
    Err(e) => {
      return JobResult {
        emit: job.emit,
        log,
        failure: Some(format!("❌ Error compiling {:?}: {:?}", job.source, e)),
        referenced: Vec::new(),
        externals: Vec::new(),
        imports: Vec::new(),
        surface: Surface::default(),
        dest: job.dest.clone(),
        written: false,
      };
    }
  };

  let referenced = output.referenced.clone();
  let externals = output.externals.clone();
  let imports = std::mem::take(&mut output.imports);

  let directory = job.dest.parent().unwrap_or(&job.dest).to_path_buf();
  let surface = Surface {
    names: std::mem::take(&mut output.exports).into_iter().collect(),
    stars: std::mem::take(&mut output.star_sources)
      .iter()
      .map(|specifier| crate::graph::normalise(&directory.join(specifier)))
      .collect(),
    open: output.open_exports,
  };

  match write_variant(map_options, &job.dest, &job.asset, output) {
    Ok(()) => JobResult {
      emit: job.emit,
      log,
      failure: None,
      referenced,
      externals,
      imports,
      surface: surface.clone(),
      dest: job.dest.clone(),
      written: true,
    },
    Err(e) => JobResult {
      emit: job.emit,
      log,
      failure: Some(format!("❌ Error writing {}: {}", display(&job.dest, &opts.root), e)),
      referenced,
      externals,
      imports,
      surface: surface.clone(),
      dest: job.dest.clone(),
      written: false,
    },
  }
}

fn report(build: &mut Build, result: JobResult, referenced: &mut Vec<PathBuf>) {
  println!("{}", result.log);
  referenced.extend(result.referenced);
  build.externals.extend(result.externals);
  // A declaration is never fetched by a browser, so it is not a node in the graph a page preloads.
  if result.emit != Emit::Declaration {
    build
    .references
    .extend(result.imports.iter().map(|import| (result.dest.clone(), import.clone())));

  build.surfaces.insert(result.dest.clone(), result.surface);

  build.graph.add(&result.dest, &result.imports);
  }

  if let Some(failure) = result.failure {
    eprintln!("{}", failure);
    build.has_error = true;
  }

  if result.written {
    build.emitted += 1;
  }
}

fn write_variant(map_options: MapOptions, dest: &Path, asset: &Asset, output: Output) -> Result<()> {
  let comment = match asset {
    Asset::Style => ("/*# sourceMappingURL=", " */\n"),
    _ => ("//# sourceMappingURL=", "\n"),
  };

  let mut code = output.code;

  match (map_options.mode, output.map) {
    (MapMode::External, Some(json)) => {
      let map_path = with_suffix(dest, ".map");
      let name = map_path.file_name().unwrap_or_default().to_string_lossy();
      code.push_str(&format!("{}{}{}", comment.0, name, comment.1));
      fs::write(&map_path, json).with_context(|| format!("Failed to write {:?}", map_path))?;
    }
    (MapMode::Inline, Some(json)) => {
      let encoded = base64::engine::general_purpose::STANDARD.encode(json);
      code.push_str(&format!(
        "{}data:application/json;base64,{}{}",
        comment.0, encoded, comment.1
      ));
    }
    _ => {}
  }

  fs::write(dest, code).with_context(|| format!("Failed to write {:?}", dest))
}

/// Two failures the emitted graph cannot report for itself, over one pass of
/// every specifier the output carries.
///
/// A specifier naming something the build did not produce is a 404. A name the
/// target does not export is a `SyntaxError` raised before either module runs,
/// and it survives a rename that the exporting module compiled cleanly through.
///
/// `export *` defers to another module, so the names a module offers are the
/// fixed point of following those edges. A star from a bare specifier leaves the
/// set open, since only the page supplying that module knows what it carries.
fn check_graph(opts: &Options, build: &mut Build) {
  let references = std::mem::take(&mut build.references);
  let offered = resolve_surfaces(&build.surfaces);

  for (module, import) in references {
    let dir = module.parent().unwrap_or(&build.out_dir).to_path_buf();
    let target = crate::graph::normalise(&dir.join(&import.specifier));

    if !build.claimed.contains_key(&target) && !target.is_file() {
      eprintln!(
        "❌ {} imports '{}', which resolves to nothing",
        display(&module, &opts.root),
        import.specifier
      );
      build.has_error = true;
      continue;
    }

    let Some((names, open)) = offered.get(&target) else {
      continue;
    };

    if *open {
      continue;
    }

    for name in &import.names {
      if names.contains(name) {
        continue;
      }

      eprintln!(
        "❌ {} imports '{}' from '{}', which does not export it",
        display(&module, &opts.root),
        name,
        import.specifier
      );
      build.has_error = true;
    }
  }
}

/// Follows every `export *` to a fixed point. Modules may import each other in a
/// cycle, so this repeats until nothing new arrives rather than recursing.
fn resolve_surfaces(surfaces: &HashMap<PathBuf, Surface>) -> HashMap<PathBuf, (BTreeSet<String>, bool)> {
  let mut offered: HashMap<PathBuf, (BTreeSet<String>, bool)> = surfaces
    .iter()
    .map(|(path, surface)| (path.clone(), (surface.names.clone(), surface.open)))
    .collect();

  loop {
    let mut changed = false;

    for (path, surface) in surfaces {
      for star in &surface.stars {
        // A star at something this build did not compile is another module's
        // business, and its names cannot be enumerated from here.
        let Some((names, open)) = offered.get(star).cloned() else {
          let entry = offered.get_mut(path).expect("every surface is seeded");
          changed |= !entry.1;
          entry.1 = true;
          continue;
        };

        let entry = offered.get_mut(path).expect("every surface is seeded");

        for name in names {
          changed |= entry.0.insert(name);
        }

        if open && !entry.1 {
          entry.1 = true;
          changed = true;
        }
      }
    }

    if !changed {
      return offered;
    }
  }
}

fn copy_assets(opts: &Options, build: &mut Build, referenced: Vec<PathBuf>) {
  let mut to_copy: Vec<PathBuf> = Vec::new();

  // An emitted module naming a file the compiler does not produce would resolve to nothing in the
  // browser, so those files ship whether or not `--copy-assets` was passed.
  for path in referenced {
    let Ok(path) = path.canonicalize() else {
      continue;
    };

    if path.is_file() && !classify(&path) {
      to_copy.push(path);
    }
  }

  if opts.copy_assets {
    to_copy.extend(build.files.iter().filter(|p| !classify(p)).cloned());
  }

  to_copy.sort();
  to_copy.dedup();

  for path in to_copy {
    let Ok(relative_path) = path.strip_prefix(&build.root_dir) else {
      eprintln!(
        "⚠️  {:?} is referenced but sits outside the root directory, so it has no place in the output",
        path
      );
      continue;
    };

    let dest_path = build.out_dir.join(relative_path);

    if let Some(previous) = build.claimed.insert(dest_path.clone(), path.clone())
      && previous != path
    {
      eprintln!(
        "❌ Output collision on {}: {:?} and {:?} both claim it",
        display(&dest_path, &opts.root),
        previous,
        path
      );
      build.has_error = true;
      continue;
    }

    if let Some(parent) = dest_path.parent()
      && let Err(e) = fs::create_dir_all(parent)
    {
      eprintln!(
        "❌ Error creating output directory {}: {}",
        display(parent, &opts.root),
        e
      );
      build.has_error = true;
      continue;
    }

    println!("   Copying: {:?}", relative_path);

    match fs::copy(&path, &dest_path) {
      Ok(_) => build.emitted += 1,
      Err(e) => {
        eprintln!("❌ Error copying {:?}: {}", path, e);
        build.has_error = true;
      }
    }
  }
}

/// Turns a bare specifier the page cannot resolve into a build failure rather than a runtime one.
fn check_externals(opts: &Options, build: &mut Build, path: &Path) -> Result<()> {
  let map = ImportMap::load(path)?;

  if map.uses_scopes() && opts.public_path.is_none() {
    eprintln!(
      "⚠️  {:?} defines scopes, which are keyed by the importing module's URL. Without --public-path only 'imports' can be checked.",
      path
    );
  }

  // A scope is selected by the importing module's URL, so every module has to satisfy the map, not
  // just one of them.
  let importers: Vec<Option<String>> = match &opts.public_path {
    Some(base) => build
      .graph
      .entry_points()
      .iter()
      .filter_map(|m| m.strip_prefix(&build.out_dir).ok())
      .filter_map(|m| m.to_str())
      .map(|m| Some(format!("{}{}", base, m.replace('\\', "/"))))
      .collect(),
    None => vec![None],
  };

  for external in &build.externals {
    let unresolved = importers
      .iter()
      .any(|importer| !map.resolves(external, importer.as_deref()));

    if unresolved {
      eprintln!("❌ '{}' is not resolved by {:?}", external, path);
      build.has_error = true;
    }
  }

  if !build.has_error {
    println!("   All externals resolve through {:?}", path);
  }

  Ok(())
}

/// Records what each entry point pulls in, so the page can preload the graph in one round trip
/// instead of discovering it one hop at a time.
fn write_manifest(opts: &Options, build: &mut Build) {
  let Some(body) = build.graph.manifest(&build.out_dir, opts.public_path.as_deref()) else {
    return;
  };

  let path = build.out_dir.join(PRELOAD_MANIFEST);
  build.claimed.insert(path.clone(), path.clone());

  match fs::write(&path, body) {
    Ok(()) => println!("   Preload manifest: {}", display(&path, &opts.root)),
    Err(e) => {
      eprintln!("❌ Error writing {}: {}", display(&path, &opts.root), e);
      build.has_error = true;
    }
  }
}

fn prune_stale(opts: &Options, build: &Build) -> Result<()> {
  let manifest_path = build.out_dir.join(MANIFEST);

  if let Ok(previous) = fs::read_to_string(&manifest_path) {
    for line in previous.lines().filter(|l| !l.is_empty()) {
      let stale = build.out_dir.join(line);

      if build.claimed.contains_key(&stale) || !stale.is_file() {
        continue;
      }

      match fs::remove_file(&stale) {
        Ok(()) => println!("   Removed: {}", display(&stale, &opts.root)),
        Err(e) => eprintln!("⚠️  Could not remove {}: {}", display(&stale, &opts.root), e),
      }
    }
  }

  let mut listed: Vec<String> = build
    .claimed
    .keys()
    .filter_map(|p| p.strip_prefix(&build.out_dir).ok())
    .map(|p| p.to_string_lossy().replace('\\', "/"))
    .collect();
  listed.sort();

  fs::write(&manifest_path, listed.join("\n")).with_context(|| format!("Failed to write {:?}", manifest_path))
}

fn with_min(path: &Path) -> PathBuf {
  let Some(ext) = path.extension().map(|e| e.to_os_string()) else {
    return with_suffix(path, ".min");
  };

  let stem = path.file_stem().unwrap_or_default().to_string_lossy().into_owned();
  path.with_file_name(format!("{}.min.{}", stem, ext.to_string_lossy()))
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
  let mut name = path.file_name().unwrap_or_default().to_os_string();
  name.push(suffix);
  path.with_file_name(name)
}

/// Path of `to` as seen from inside `from_dir`, so a map written next to the output can point back
/// at a source that lives outside the output tree.
fn relative_from(from_dir: &Path, to: &Path) -> String {
  let mut from = from_dir.components().peekable();
  let mut target = to.components().peekable();

  while from.peek().is_some() && from.peek() == target.peek() {
    from.next();
    target.next();
  }

  let mut parts: Vec<String> = from.map(|_| "..".to_string()).collect();
  parts.extend(target.map(|c| c.as_os_str().to_string_lossy().into_owned()));
  parts.join("/")
}

pub fn display(path: &Path, base: &Path) -> String {
  let relative = relative_from(base, path);

  if relative.is_empty() {
    return format!("{:?}", ".");
  }

  format!("{:?}", relative)
}
