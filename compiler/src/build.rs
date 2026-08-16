use crate::compiler::{Compiler, Dialect, MapRequest, Minify, Output};
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
use std::collections::HashMap;
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
  pub compiler: Compiler,
  pub claimed: HashMap<PathBuf, PathBuf>,
  /// Bare specifiers the emitted modules carry. A browser cannot resolve these on its own, so the
  /// page has to supply an import map covering every one of them.
  pub externals: Vec<String>,
  pub graph: Graph,
  pub emitted: usize,
  pub has_error: bool,
}

#[derive(Clone, Copy)]
enum Asset {
  Script { dialect: Dialect, out_ext: &'static str },
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
    "ts" | "tsx" => Some(Asset::Script {
      dialect: Dialect::TypeScript,
      out_ext: "js",
    }),
    "js" | "jsx" => Some(Asset::Script {
      dialect: Dialect::JavaScript,
      out_ext: "js",
    }),
    "mjs" => Some(Asset::Script {
      dialect: Dialect::JavaScript,
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

  fs::create_dir_all(&out_dir).with_context(|| format!("Failed to create {:?}", out_dir))?;
  let out_dir = out_dir
    .canonicalize()
    .context("Failed to resolve absolute path of output directory")?;

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
    compiler: Compiler::new(targets),
    claimed: HashMap::new(),
    externals: Vec::new(),
    graph: Graph::default(),
    emitted: 0,
    has_error: false,
  };

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
}

struct Job {
  source: PathBuf,
  relative: PathBuf,
  dest: PathBuf,
  asset: Asset,
  minify: Option<Minify>,
  source_name: String,
}

struct JobResult {
  log: String,
  failure: Option<String>,
  referenced: Vec<PathBuf>,
  externals: Vec<String>,
  imports: Vec<Import>,
  dest: PathBuf,
  written: bool,
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

  let mut variants = vec![(dest.clone(), None)];
  if let Some(level) = opts.minify {
    variants.push((with_min(&dest), Some(level)));
  }

  let mut jobs = Vec::new();

  for (dest, minify) in variants {
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

    if build.map_options.mode == MapMode::External {
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
      minify,
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

  let suffix = if job.minify.is_some() { " (min)" } else { "" };

  let (label, compiled) = match job.asset {
    Asset::Script { dialect, .. } => {
      let label = match dialect {
        Dialect::TypeScript => "TS",
        Dialect::JavaScript => "JS",
      };
      (
        label,
        compiler.compile_script(&job.source, dialect, opts.strip_log, opts.strip_debug, job.minify, map),
      )
    }
    Asset::Style => ("CSS", compiler.compile_css(&job.source, job.minify.is_some(), map)),
  };

  let log = format!("   Compiling {}{}: {:?}", label, suffix, job.relative);

  let mut output = match compiled {
    Ok(output) => output,
    Err(e) => {
      return JobResult {
        log,
        failure: Some(format!("❌ Error compiling {:?}: {:?}", job.source, e)),
        referenced: Vec::new(),
        externals: Vec::new(),
        imports: Vec::new(),
        dest: job.dest.clone(),
        written: false,
      };
    }
  };

  let referenced = output.referenced.clone();
  let externals = output.externals.clone();
  let imports = std::mem::take(&mut output.imports);

  match write_variant(map_options, &job.dest, &job.asset, output) {
    Ok(()) => JobResult {
      log,
      failure: None,
      referenced,
      externals,
      imports,
      dest: job.dest.clone(),
      written: true,
    },
    Err(e) => JobResult {
      log,
      failure: Some(format!("❌ Error writing {}: {}", display(&job.dest, &opts.root), e)),
      referenced,
      externals,
      imports,
      dest: job.dest.clone(),
      written: false,
    },
  }
}

fn report(build: &mut Build, result: JobResult, referenced: &mut Vec<PathBuf>) {
  println!("{}", result.log);
  referenced.extend(result.referenced);
  build.externals.extend(result.externals);
  build.graph.add(&result.dest, &result.imports);

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
