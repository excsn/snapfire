//! `fsr types`: the declarations for every package the import map names, into
//! `types/` and the `tsconfig.json` that points at them. Declarations are read
//! by an editor and `tsc --noEmit`, never shipped, so `types/` is gitignored and
//! a fetch that fails is a warning rather than a build error.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::vendor::{self, get, import_map_packages, VendorManifest};
use crate::xwpm::{self, Layout};
use crate::BuildError;

pub const NPM_REGISTRY: &str = "https://registry.npmjs.org";
pub const TYPES_MANIFEST: &str = ".fsr-types.json";

/// The declaration file under `<types>/` that names what an import of a
/// foreign component is, `declare module "*.vue"`, for the typechecker.
pub const FOREIGN_SHIM: &str = "foreign.d.ts";

/// The extensions of components a snapfirec plugin compiles, found by
/// scanning the sources; a template's placements add whatever else they name.
const FOREIGN_COMPONENTS: &[&str] = &["vue"];

/// The directories the typechecker reads, of those the application has.
const SOURCE_DIRS: &[&str] = &["src", "ext", "elements", "routes", "schemas", "tests"];

/// The packages generated code imports, so their declarations are written whether or not the import map names them.
const ALWAYS: &[&str] = &["@snapfire/fsr-authoring", "@snapfire/fsr-client"];

/// The declarations the fsr packages carry, written by `fsr types` without a
/// registry since the binary is the same version as the runtime they describe.
/// The client's are the host's, which is what serves the modules they declare.
const FSR_CLIENT: &[(&str, &str)] = snapfire_fsr_host::client::TYPES;
const FSR_AUTHORING: &[(&str, &str)] = &[
  ("index.d.ts", include_str!("../embedded/authoring/index.d.ts")),
  ("template.d.ts", include_str!("../embedded/authoring/template.d.ts")),
  ("template.react.d.ts", include_str!("../embedded/authoring/template.react.d.ts")),
  ("jsx-runtime.d.ts", include_str!("../embedded/authoring/jsx-runtime.d.ts")),
];

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TypedPackage {
  pub version: String,
  /// The package the declarations came from: the package itself, `@types/<name>` or `fsr`.
  pub from: String,
  /// The declaration entry relative to `types/<name>/`.
  pub entry: String,
  /// True when the entry is a script of `declare module` blocks rather than a module, so it is included instead of path-mapped.
  #[serde(default)]
  pub ambient: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TypesManifest {
  #[serde(default)]
  pub packages: BTreeMap<String, TypedPackage>,
}

impl TypesManifest {
  pub fn read(app: &Path, layout: &Layout) -> Result<Self, BuildError> {
    let path = app.join(&layout.types).join(TYPES_MANIFEST);
    if !path.is_file() {
      return Ok(Self::default());
    }
    let text = std::fs::read_to_string(&path).map_err(|e| BuildError::Io(path.clone(), e))?;
    serde_json::from_str(&text).map_err(|e| BuildError::Manifest(path, e.to_string()))
  }

  pub fn write(&self, app: &Path, layout: &Layout) -> Result<(), BuildError> {
    let path = app.join(&layout.types).join(TYPES_MANIFEST);
    if let Some(parent) = path.parent() {
      std::fs::create_dir_all(parent).map_err(|e| BuildError::Io(parent.to_path_buf(), e))?;
    }
    let text = serde_json::to_string_pretty(self).map_err(|e| BuildError::Manifest(path.clone(), e.to_string()))?;
    std::fs::write(&path, text + "\n").map_err(|e| BuildError::Io(path, e))
  }
}

#[derive(Debug, Default)]
pub struct TypesReport {
  /// Package, version, source.
  pub fetched: Vec<(String, String, String)>,
  pub kept: Vec<String>,
  /// Package and why.
  pub missing: Vec<(String, String)>,
  /// Package, the version it holds and the range asked of it that refuses that version.
  pub unmet: Vec<(String, String, Asked)>,
  /// The xwpm commands run instead of fetching, when the application is xwpm's.
  pub delegated: Vec<String>,
  /// The files written beside the declarations: the foreign shim and the tsconfig.
  pub written: Vec<String>,
}

/// `@scope/name` is published to DefinitelyTyped as `@types/scope__name`.
pub fn definitely_typed(package: &str) -> String {
  match package.strip_prefix('@') {
    Some(scoped) => format!("@types/{}", scoped.replacen('/', "__", 1)),
    None => format!("@types/{package}"),
  }
}

fn from_definitely_typed(package: &str) -> String {
  match package.strip_prefix("@types/") {
    Some(rest) if rest.contains("__") => format!("@{}", rest.replacen("__", "/", 1)),
    Some(rest) => rest.to_owned(),
    None => package.to_owned(),
  }
}

pub fn is_ambient(entry: &str) -> bool {
  // A file with a top-level import or export is a module, and a `declare
  // module` in it augments another package, as Vue's runtime packages do.
  if entry.lines().any(|line| line.starts_with("import ") || line.starts_with("export ")) {
    return false;
  }
  // At the start of a line: React's own declarations mention `declare module
  // "react"` inside a doc comment and a path-mapped package must stay mapped.
  entry.lines().map(str::trim_start).any(|line| line.starts_with("declare module \"") || line.starts_with("declare module '"))
}

/// The major to take declarations at: the vendored version's, else the one a
/// site's shell contract names for the framework.
fn wanted_major(vendored: Option<&str>, shell: Option<&str>) -> Option<u64> {
  vendored.or(shell).and_then(range::Version::parse).filter(|v| v.pre.is_empty()).map(|v| v.major)
}

#[derive(Deserialize)]
struct Abbreviated {
  #[serde(default, rename = "dist-tags")]
  dist_tags: BTreeMap<String, String>,
  #[serde(default)]
  versions: BTreeMap<String, serde_json::Value>,
}

#[derive(Deserialize)]
struct VersionDoc {
  version: String,
  #[serde(default)]
  types: Option<String>,
  #[serde(default)]
  typings: Option<String>,
  #[serde(default)]
  exports: Option<serde_json::Value>,
  #[serde(default)]
  main: Option<String>,
  dist: Dist,
}

#[derive(Deserialize)]
struct Dist {
  tarball: String,
}

fn encode(name: &str) -> String {
  name.replace('/', "%2f")
}

/// The version of `name` to take: the highest release every range accepts,
/// else the highest the first accepts, else `latest`.
fn choose_version(client: &reqwest::blocking::Client, registry: &str, name: &str, ranges: &[range::Range]) -> Result<Option<String>, BuildError> {
  let url = format!("{registry}/{}", encode(name));
  let response = client
    .get(&url)
    .header("accept", "application/vnd.npm.install-v1+json")
    .send()
    .map_err(|e| BuildError::Http(url.clone(), e.to_string()))?;
  if response.status() == reqwest::StatusCode::NOT_FOUND {
    return Ok(None);
  }
  if !response.status().is_success() {
    return Err(BuildError::Http(url.clone(), format!("HTTP {}", response.status())));
  }
  let doc: Abbreviated = response.json().map_err(|e| BuildError::Http(url.clone(), e.to_string()))?;
  let published: Vec<(range::Version, &String)> = doc.versions.keys().filter_map(|key| range::Version::parse(key).map(|v| (v, key))).collect();
  let best = |accepts: &dyn Fn(&range::Version) -> bool| published.iter().filter(|(v, _)| accepts(v)).max_by(|a, b| a.0.cmp(&b.0)).map(|(_, key)| (*key).clone());
  if let Some(first) = ranges.first() {
    if let Some(version) = best(&|v| ranges.iter().all(|r| r.satisfies(v))).or_else(|| best(&|v| first.satisfies(v))) {
      return Ok(Some(version));
    }
  }
  Ok(doc.dist_tags.get("latest").cloned())
}

fn version_doc(client: &reqwest::blocking::Client, registry: &str, name: &str, version: &str) -> Result<VersionDoc, BuildError> {
  let url = format!("{registry}/{}/{version}", encode(name));
  let bytes = get(client, &url)?.ok_or_else(|| BuildError::Http(url.clone(), "HTTP 404".to_owned()))?;
  serde_json::from_slice(&bytes).map_err(|e| BuildError::Http(url, e.to_string()))
}

/// Unpacks every declaration file of a tarball under `dir`, paths relative to the package root.
fn unpack_declarations(bytes: &[u8], dir: &Path) -> Result<usize, BuildError> {
  let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
  let mut count = 0;
  let entries = archive.entries().map_err(|e| BuildError::Io(dir.to_path_buf(), e))?;
  for entry in entries {
    let mut entry = entry.map_err(|e| BuildError::Io(dir.to_path_buf(), e))?;
    let path = entry.path().map_err(|e| BuildError::Io(dir.to_path_buf(), e))?.into_owned();
    let name = path.to_string_lossy();
    let keep = name.ends_with(".d.ts") || name.ends_with(".d.mts") || name.ends_with(".d.cts") || name.ends_with("/package.json");
    if !keep {
      continue;
    }
    let rel: PathBuf = path.components().skip(1).collect();
    if rel.as_os_str().is_empty() || rel.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
      continue;
    }
    let target = dir.join(&rel);
    if let Some(parent) = target.parent() {
      std::fs::create_dir_all(parent).map_err(|e| BuildError::Io(parent.to_path_buf(), e))?;
    }
    let mut content = Vec::new();
    entry.read_to_end(&mut content).map_err(|e| BuildError::Io(target.clone(), e))?;
    std::fs::write(&target, content).map_err(|e| BuildError::Io(target, e))?;
    count += 1;
  }
  Ok(count)
}

/// Writes the declarations `fsr` carries for its own packages when the app's
/// copies are absent or differ from them, so an application cannot typecheck
/// against a client older than the binary that built it. Reads no network.
pub fn refresh_embedded(app: &Path) -> Result<Vec<PathBuf>, BuildError> {
  let layout = Layout::of(app)?;
  let root = app.join(&layout.types);
  let mut refreshed = Vec::new();
  let mut manifest = TypesManifest::read(app, &layout)?;
  let mut recorded = false;
  for (package, files) in [("@snapfire/fsr-client", FSR_CLIENT), ("@snapfire/fsr-authoring", FSR_AUTHORING)] {
    let dir = root.join(package);
    let stale = files.iter().any(|(name, content)| std::fs::read_to_string(dir.join(name)).map(|held| held != *content).unwrap_or(true));
    if stale {
      write_embedded(app, &layout, package, files)?;
      refreshed.push(dir);
    }
    if manifest.packages.get(package) != Some(&embedded_record()) {
      manifest.packages.insert(package.to_owned(), embedded_record());
      recorded = true;
    }
  }
  if recorded && root.join(TYPES_MANIFEST).is_file() {
    manifest.write(app, &layout)?;
  }
  Ok(refreshed)
}

/// The record of a package whose declarations this binary carries.
fn embedded_record() -> TypedPackage {
  TypedPackage { version: env!("CARGO_PKG_VERSION").to_owned(), from: "fsr".to_owned(), entry: "index.d.ts".to_owned(), ambient: false }
}

fn write_embedded(app: &Path, layout: &Layout, package: &str, files: &[(&str, &str)]) -> Result<(), BuildError> {
  let dir = app.join(&layout.types).join(package);
  std::fs::create_dir_all(&dir).map_err(|e| BuildError::Io(dir.clone(), e))?;
  for (name, content) in files {
    let path = dir.join(name);
    std::fs::write(&path, content).map_err(|e| BuildError::Io(path, e))?;
  }
  Ok(())
}

struct Fetched {
  from: String,
  version: String,
  entry: String,
  ambient: bool,
}

/// Declarations for `package` into `<types>/<package>/`: from the package
/// itself when it declares them, else from DefinitelyTyped, each at the
/// version [`choose_version`] picks for the ranges `asked` holds of it. The
/// candidate the first range names is tried first.
fn fetch_npm(client: &reqwest::blocking::Client, registry: &str, app: &Path, layout: &Layout, package: &str, asked: &[Asked]) -> Result<Option<Fetched>, BuildError> {
  let dir = app.join(&layout.types).join(package);
  let mut candidates: Vec<(String, bool)> = vec![(package.to_owned(), false)];
  if !package.starts_with("@types/") {
    candidates.push((definitely_typed(package), true));
  }
  if asked.iter().find_map(|a| a.of.as_deref()) == Some(definitely_typed(package).as_str()) {
    candidates.reverse();
  }
  for (name, from_dt) in candidates {
    let ranges: Vec<range::Range> = asked.iter().filter(|a| a.applies_to(&name)).filter_map(|a| range::Range::parse(&a.range)).collect();
    let Some(version) = choose_version(client, registry, &name, &ranges)? else { continue };
    let doc = version_doc(client, registry, &name, &version)?;
    let bytes = get(client, &doc.dist.tarball)?.ok_or_else(|| BuildError::Http(doc.dist.tarball.clone(), "HTTP 404".to_owned()))?;
    if dir.exists() {
      std::fs::remove_dir_all(&dir).map_err(|e| BuildError::Io(dir.clone(), e))?;
    }
    let count = unpack_declarations(&bytes, &dir)?;
    let entry = declaration_entries(&doc, from_dt).into_iter().find(|entry| dir.join(entry).is_file());
    let Some(entry) = entry.filter(|_| count > 0) else {
      let _ = std::fs::remove_dir_all(&dir);
      continue;
    };
    let entry_path = dir.join(&entry);
    let ambient = is_ambient(&std::fs::read_to_string(&entry_path).map_err(|e| BuildError::Io(entry_path.clone(), e))?);
    return Ok(Some(Fetched { from: name, version: doc.version, entry, ambient }));
  }
  Ok(None)
}

/// Where a package's declarations may start, in the order TypeScript would
/// look: `types`, `typings`, the `types` condition of `exports["."]`, a
/// `.d.ts` beside `main`, and `index.d.ts`.
fn declaration_entries(doc: &VersionDoc, definitely_typed: bool) -> Vec<String> {
  let mut out: Vec<String> = Vec::new();
  out.extend(doc.types.clone());
  out.extend(doc.typings.clone());
  out.extend(doc.exports.as_ref().and_then(exported_types));
  out.extend(doc.main.as_deref().and_then(|main| {
    [(".js", ".d.ts"), (".cjs", ".d.cts"), (".mjs", ".d.mts")].iter().find_map(|(js, dts)| main.strip_suffix(js).map(|stem| format!("{stem}{dts}")))
  }));
  if definitely_typed || out.is_empty() {
    out.push("index.d.ts".to_owned());
  }
  out.into_iter().map(|entry| entry.trim_start_matches("./").to_owned()).collect()
}

/// The `types` an `exports` map gives the package root: the `"."` entry when
/// the map has subpaths, conditions read in order, the first array item that has one.
fn exported_types(exports: &serde_json::Value) -> Option<String> {
  fn conditions(value: &serde_json::Value) -> Option<String> {
    match value {
      serde_json::Value::String(path) => [".d.ts", ".d.mts", ".d.cts"].iter().any(|ext| path.ends_with(ext)).then(|| path.clone()),
      serde_json::Value::Array(items) => items.iter().find_map(conditions),
      serde_json::Value::Object(map) => map.iter().find_map(|(condition, value)| match (condition.starts_with("types"), value) {
        (true, serde_json::Value::String(path)) => Some(path.clone()),
        _ => conditions(value),
      }),
      _ => None,
    }
  }
  match exports {
    serde_json::Value::Object(map) if map.keys().any(|k| k.starts_with('.')) => map.get(".").and_then(conditions),
    other => conditions(other),
  }
}

/// Fills the types directory for the fsr packages and every package the import
/// map names: the fsr packages from the binary, the rest from the npm registry
/// at the vendored major when `fsr add` recorded one, else for a site at the
/// major its shell's contract names for the framework. A package already present
/// is kept unless `refresh`. Under xwpm, `xwpm restore` and `xwpm types` fill
/// the directory and only the fsr packages are written here.
pub fn fetch(app: &Path, refresh: bool) -> Result<TypesReport, BuildError> {
  let layout = Layout::of(app)?;
  let mut report = TypesReport::default();
  let mut manifest = TypesManifest::read(app, &layout)?;

  if layout.xwpm {
    xwpm::run(app, &["restore"])?;
    xwpm::run(app, &["types"])?;
    report.delegated = vec!["xwpm restore".to_owned(), "xwpm types".to_owned()];
  }

  let mut queue: Vec<String> = ALWAYS.iter().map(|s| (*s).to_owned()).collect();
  queue.extend(import_map_packages(app, &layout)?);
  fill(app, &layout, &mut manifest, queue, refresh, NPM_REGISTRY, &mut report)?;
  manifest.write(app, &layout)?;
  let shim = write_foreign_shim(app, &layout, &[])?;
  if let Some(path) = &shim {
    report.written.push(path.clone());
  }
  let generated = app.join("generated").is_dir();
  let path = app.join("tsconfig.json");
  let declared = app.join(snapfire_fsr_lower::vue::PROPS_DECLARATION_DIR).is_dir();
  std::fs::write(&path, tsconfig(app, generated, shim.is_some(), declared)?).map_err(|e| BuildError::Io(path, e))?;
  report.written.push("tsconfig.json".to_owned());
  Ok(report)
}

/// What `fsr build` fetches before it checks: when [`missing`] or [`unmet`]
/// names anything, what `fsr types` would fetch from `registry`, recorded in
/// the manifest. The build writes the tsconfig and the shim itself. Nothing is
/// fetched under xwpm, which supplies its own.
pub(crate) fn fetch_missing_from(app: &Path, registry: &str) -> Result<TypesReport, BuildError> {
  let layout = Layout::of(app)?;
  let mut report = TypesReport::default();
  if layout.xwpm || (missing(app)?.is_empty() && unmet(app)?.is_empty()) {
    return Ok(report);
  }
  let mut manifest = TypesManifest::read(app, &layout)?;
  fill(app, &layout, &mut manifest, wanted(app, &layout)?, false, registry, &mut report)?;
  manifest.write(app, &layout)?;
  Ok(report)
}

/// How many times [`fill`] takes one package, so ranges that keep moving each
/// other's versions settle.
const TAKES: usize = 3;

/// Fills the types directory for `roots` and each package their declarations
/// import: the fsr packages from the binary, the rest from `registry` at the
/// version every range asked of it accepts, the first of them the vendored or
/// a site's shell's major for a root. A package whose entry is on disk is kept
/// unless `refresh` or a range asked of it refuses the version it holds, and
/// one fetched again for a later range has its dependencies walked again.
fn fill(app: &Path, layout: &Layout, manifest: &mut TypesManifest, roots: Vec<String>, refresh: bool, registry: &str, report: &mut TypesReport) -> Result<(), BuildError> {
  let client = vendor::client()?;
  let mut asked = pins(app, layout)?;
  let mut queue: std::collections::VecDeque<String> = roots.into_iter().collect();
  let mut taken: BTreeMap<String, usize> = BTreeMap::new();
  while let Some(package) = queue.pop_front() {
    let times = taken.entry(package.clone()).or_insert(0);
    if *times >= TAKES {
      continue;
    }
    *times += 1;
    let asks = asked.get(&package).cloned().unwrap_or_default();
    let dir = app.join(&layout.types).join(&package);
    let embedded = match package.as_str() {
      "@snapfire/fsr-client" => Some(FSR_CLIENT),
      "@snapfire/fsr-authoring" => Some(FSR_AUTHORING),
      _ => None,
    };
    if let Some(files) = embedded {
      write_embedded(app, layout, &package, files)?;
      manifest.packages.insert(package.clone(), embedded_record());
      report.fetched.push((package, env!("CARGO_PKG_VERSION").to_owned(), "fsr".to_owned()));
      continue;
    }
    let entry = manifest.packages.get(&package).map(|typed| typed.entry.clone()).unwrap_or_else(|| "index.d.ts".to_owned());
    if dir.join(&entry).is_file() && !refresh && accepted(&asks, held(app, layout, &package, manifest).as_ref()) {
      report.kept.push(package.clone());
      if let Some(typed) = manifest.packages.get_mut(&package) {
        if let Ok(text) = std::fs::read_to_string(dir.join(&typed.entry)) {
          typed.ambient = is_ambient(&text);
        }
      }
    } else if package.starts_with("@snapfire/") {
      report.missing.push((package, "not a package fsr knows".to_owned()));
      continue;
    } else if layout.xwpm {
      report.missing.push((package, "xwpm supplies declarations for its modules and externals".to_owned()));
      continue;
    } else {
      match fetch_npm(&client, registry, app, layout, &package, &asks)? {
        Some(fetched) => {
          manifest.packages.insert(package.clone(), TypedPackage { version: fetched.version.clone(), from: fetched.from.clone(), entry: fetched.entry, ambient: fetched.ambient });
          report.fetched.push((package.clone(), fetched.version, fetched.from));
        }
        None => {
          report.missing.push((package, "no `types` in the package and nothing on DefinitelyTyped".to_owned()));
          continue;
        }
      }
    }
    for asks in asked.values_mut() {
      asks.retain(|a| a.by != package);
    }
    for (dependency, ask) in requirements_of(app, layout, &package) {
      asked.entry(dependency.clone()).or_default().push(ask);
      let refused = taken.contains_key(&dependency) && !accepted(&asked[&dependency], held(app, layout, &dependency, manifest).as_ref());
      if (!taken.contains_key(&dependency) || refused) && !queue.contains(&dependency) {
        queue.push_back(dependency);
      }
    }
  }
  for (package, asks) in &asked {
    if let Some(holding) = held(app, layout, package, manifest) {
      report.unmet.extend(refusing(asks, &holding).map(|ask| (package.clone(), holding.1.to_string(), ask.clone())));
    }
  }
  Ok(())
}

pub fn source_dirs(app: &Path) -> Vec<&'static str> {
  SOURCE_DIRS.iter().copied().filter(|dir| app.join(dir).is_dir()).collect()
}

/// `<types>/foreign.d.ts`: what an import of a component the build does not
/// read is, for the typechecker. Every `FOREIGN_COMPONENTS` file under the
/// source directories counts, plus the modules `placed` names, which is what
/// a build's templates import. `None` when there is nothing foreign.
pub fn foreign_shim(app: &Path, placed: &[String]) -> Option<String> {
  let mut extensions: Vec<String> = placed
    .iter()
    .filter_map(|m| Path::new(m.split_once('#').map(|(f, _)| f).unwrap_or(m)).extension().and_then(|e| e.to_str()).map(str::to_owned))
    .collect();
  for dir in source_dirs(app) {
    collect_foreign(&app.join(dir), &mut extensions);
  }
  extensions.sort();
  extensions.dedup();
  if extensions.is_empty() {
    return None;
  }
  let mut out = String::from("// Generated by fsr. Do not edit.\n");
  for ext in extensions {
    match ext.as_str() {
      // Loose on purpose: a template places the component with the props the
      // build reads; the browser types the component's own script itself.
      "vue" => out.push_str("\ndeclare module \"*.vue\" {\n  const component: (props: { [prop: string]: unknown }) => any;\n  export default component;\n}\n"),
      other => out.push_str(&format!("\ndeclare module \"*.{other}\" {{\n  const component: unknown;\n  export default component;\n}}\n")),
    }
  }
  Some(out)
}

fn collect_foreign(dir: &Path, into: &mut Vec<String>) {
  let Ok(entries) = std::fs::read_dir(dir) else { return };
  for entry in entries.flatten() {
    let path = entry.path();
    if path.is_dir() {
      collect_foreign(&path, into);
    } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
      if FOREIGN_COMPONENTS.contains(&ext) && !into.iter().any(|held| held == ext) {
        into.push(ext.to_owned());
      }
    }
  }
}

/// Writes the shim under `<types>/` or removes a stale one when there is
/// nothing foreign any more. The path written, relative to the app.
pub fn write_foreign_shim(app: &Path, layout: &Layout, placed: &[String]) -> Result<Option<String>, BuildError> {
  let rel = format!("{}/{FOREIGN_SHIM}", layout.types.trim_end_matches('/'));
  let path = app.join(&rel);
  match foreign_shim(app, placed) {
    Some(shim) => {
      if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| BuildError::Io(parent.to_path_buf(), e))?;
      }
      std::fs::write(&path, shim).map_err(|e| BuildError::Io(path, e))?;
      Ok(Some(rel))
    }
    None => {
      if path.is_file() {
        std::fs::remove_file(&path).map_err(|e| BuildError::Io(path, e))?;
      }
      Ok(None)
    }
  }
}

/// A range asked of a package's declarations: by a package whose own
/// declarations import it, or by the vendored version or a site's shell for a
/// package the import map names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asked {
  pub by: String,
  /// The npm package the range is a range of, the dependency or its
  /// `@types/` package; `None` for whichever supplies the declarations.
  pub of: Option<String>,
  pub range: String,
}

impl Asked {
  fn applies_to(&self, npm: &str) -> bool {
    self.of.as_deref().is_none_or(|of| of == npm)
  }
}

/// What the packages the import map names are held to before any package asks:
/// the vendored version's major, else the one a site's shell names.
fn pins(app: &Path, layout: &Layout) -> Result<BTreeMap<String, Vec<Asked>>, BuildError> {
  let vendored = VendorManifest::read(app, layout)?;
  let shell = match crate::site_beside(app).and_then(|site| site.shell) {
    Some(path) => crate::ShellContract::read(&path)?.frameworks,
    None => Default::default(),
  };
  let mut out = BTreeMap::new();
  for package in wanted(app, layout)? {
    let vendored_version = vendored.packages.get(&package).map(|p| p.version.as_str());
    if let Some(major) = wanted_major(vendored_version, shell.get(&package).map(String::as_str)) {
      let by = match vendored_version {
        Some(version) => format!("the vendored {version}"),
        None => "the shell".to_owned(),
      };
      out.insert(package, vec![Asked { by, of: None, range: format!("^{major}") }]);
    }
  }
  Ok(out)
}

/// What the declarations under `<types>/<package>/` need beside them, read
/// from the `package.json` they came with, each with the range asked of it:
/// every dependency of a DefinitelyTyped package, and of a package carrying
/// its own declarations only those they import, `vue` re-exporting
/// `@vue/runtime-dom`.
fn requirements_of(app: &Path, layout: &Layout, package: &str) -> Vec<(String, Asked)> {
  let dir = app.join(&layout.types).join(package);
  let Ok(text) = std::fs::read_to_string(dir.join("package.json")) else { return Vec::new() };
  let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else { return Vec::new() };
  let Some(dependencies) = value.get("dependencies").and_then(|d| d.as_object()) else { return Vec::new() };
  let range = |name: &str| dependencies.get(name).and_then(|r| r.as_str()).unwrap_or("*").to_owned();
  let ask = |name: &str| Asked { by: package.to_owned(), of: Some(name.to_owned()), range: range(name) };
  match value.get("name").and_then(|n| n.as_str()).is_some_and(|n| n.starts_with("@types/")) {
    true => dependencies.keys().map(|k| (from_definitely_typed(k), ask(k))).collect(),
    false => imported_dependencies(&dir, dependencies.keys()).into_iter().map(|name| (name.clone(), ask(&name))).collect(),
  }
}

/// The npm package the declarations under `<types>/<package>/` came from and
/// its version: the manifest's record, else their own `package.json`.
fn held(app: &Path, layout: &Layout, package: &str, manifest: &TypesManifest) -> Option<(String, range::Version)> {
  if let Some(typed) = manifest.packages.get(package).filter(|t| !t.from.is_empty() && t.from != "fsr") {
    return Some((typed.from.clone(), range::Version::parse(&typed.version)?));
  }
  let text = std::fs::read_to_string(app.join(&layout.types).join(package).join("package.json")).ok()?;
  let value: serde_json::Value = serde_json::from_str(&text).ok()?;
  Some((value.get("name")?.as_str()?.to_owned(), range::Version::parse(value.get("version")?.as_str()?)?))
}

/// The ranges in `asks` that apply to what `holding` came from and refuse its
/// version. A range that does not parse, a dist-tag or a URL, refuses nothing.
fn refusing<'a>(asks: &'a [Asked], holding: &'a (String, range::Version)) -> impl Iterator<Item = &'a Asked> {
  asks.iter().filter(|a| a.applies_to(&holding.0) && range::Range::parse(&a.range).is_some_and(|r| !r.satisfies(&holding.1)))
}

/// Whether what is held meets every range asked of it; a package with no version to judge does.
fn accepted(asks: &[Asked], holding: Option<&(String, range::Version)>) -> bool {
  holding.is_none_or(|holding| refusing(asks, holding).next().is_none())
}

/// The names among `dependencies` that a declaration file under `dir` imports,
/// re-exports or references by a bare specifier.
fn imported_dependencies<'a>(dir: &Path, dependencies: impl Iterator<Item = &'a String>) -> Vec<String> {
  let mut imported = std::collections::BTreeSet::new();
  let mut pending = vec![dir.to_path_buf()];
  while let Some(at) = pending.pop() {
    let Ok(entries) = std::fs::read_dir(&at) else { continue };
    for entry in entries.flatten() {
      let path = entry.path();
      if path.is_dir() {
        pending.push(path);
      } else if [".d.ts", ".d.mts", ".d.cts"].iter().any(|ext| path.to_string_lossy().ends_with(ext)) {
        if let Ok(text) = std::fs::read_to_string(&path) {
          imported.extend(bare_imports(&text));
        }
      }
    }
  }
  dependencies.filter(|name| imported.contains(name.as_str())).cloned().collect()
}

/// The package each bare specifier in a declaration file names: `from`,
/// `import(...)` and `/// <reference types>`, a scoped name kept whole.
fn bare_imports(text: &str) -> Vec<String> {
  static SPECIFIER: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
  let specifier = SPECIFIER.get_or_init(|| regex::Regex::new(r#"(?:\bfrom\s*|\bimport\s*\(\s*|<reference\s+types\s*=\s*)["']([^"'./][^"']*)["']"#).expect("the pattern compiles"));
  specifier
    .captures_iter(text)
    .map(|c| {
      let spec = &c[1];
      let mut parts = spec.split('/');
      match (parts.next(), parts.next()) {
        (Some(scope), Some(name)) if scope.starts_with('@') => format!("{scope}/{name}"),
        (Some(name), _) => name.to_owned(),
        _ => spec.to_owned(),
      }
    })
    .collect()
}

/// Every package under the types directory whose declaration entry is on disk, with what the manifest knows about it: the entry it records, else `index.d.ts`.
pub fn present(app: &Path, layout: &Layout) -> Result<Vec<(String, TypedPackage)>, BuildError> {
  let manifest = TypesManifest::read(app, layout)?;
  let root = app.join(&layout.types);
  let mut out = Vec::new();
  if !root.is_dir() {
    return Ok(out);
  }
  let mut dirs: Vec<PathBuf> = Vec::new();
  for entry in std::fs::read_dir(&root).map_err(|e| BuildError::Io(root.clone(), e))?.flatten() {
    let path = entry.path();
    if !path.is_dir() {
      continue;
    }
    if path.file_name().is_some_and(|n| n.to_string_lossy().starts_with('@')) {
      for scoped in std::fs::read_dir(&path).map_err(|e| BuildError::Io(path.clone(), e))?.flatten() {
        if scoped.path().is_dir() {
          dirs.push(scoped.path());
        }
      }
    } else {
      dirs.push(path);
    }
  }
  for dir in dirs {
    let name = dir.strip_prefix(&root).unwrap_or(&dir).to_string_lossy().replace('\\', "/");
    let typed = manifest.packages.get(&name).cloned().unwrap_or_else(|| TypedPackage { entry: "index.d.ts".to_owned(), ..TypedPackage::default() });
    if dir.join(&typed.entry).is_file() {
      out.push((name, typed));
    }
  }
  out.sort_by(|a, b| a.0.cmp(&b.0));
  Ok(out)
}

/// Every package the application needs declarations for: the fsr packages and
/// whatever the import map names.
fn wanted(app: &Path, layout: &Layout) -> Result<Vec<String>, BuildError> {
  let mut packages: Vec<String> = ALWAYS.iter().map(|s| (*s).to_owned()).collect();
  for package in import_map_packages(app, layout)? {
    if !packages.contains(&package) {
      packages.push(package);
    }
  }
  Ok(packages)
}

/// The packages the application needs declarations for, [`wanted`] and what
/// the declarations on disk import, walked as [`fill`] walks them, with every
/// range asked of each. A package's own needs are known only once it is
/// present, so one that is missing adds none until it is fetched.
struct Closure {
  order: Vec<String>,
  asked: BTreeMap<String, Vec<Asked>>,
}

fn closure(app: &Path, layout: &Layout, present: &[(String, TypedPackage)]) -> Result<Closure, BuildError> {
  let mut asked = pins(app, layout)?;
  let mut order: Vec<String> = Vec::new();
  let mut queue: std::collections::VecDeque<String> = wanted(app, layout)?.into_iter().collect();
  while let Some(package) = queue.pop_front() {
    if order.contains(&package) {
      continue;
    }
    if present.iter().any(|(name, _)| *name == package) {
      for (dependency, ask) in requirements_of(app, layout, &package) {
        asked.entry(dependency.clone()).or_default().push(ask);
        queue.push_back(dependency);
      }
    }
    order.push(package);
  }
  Ok(Closure { order, asked })
}

/// What [`present`] finds plus the packages this binary carries, which every
/// build writes after its tsconfig and report are computed.
pub fn declared_packages(app: &Path, layout: &Layout) -> Result<Vec<(String, TypedPackage)>, BuildError> {
  let mut out = present(app, layout)?;
  for package in ALWAYS {
    if !out.iter().any(|(name, _)| name == package) {
      out.push(((*package).to_owned(), embedded_record()));
    }
  }
  out.sort_by(|a, b| a.0.cmp(&b.0));
  Ok(out)
}

/// The [`needed`] packages with no declarations under `types/`. tsc reports a
/// wanted one as an unresolved module per import and then hundreds of untyped
/// JSX elements; under `skipLibCheck` it reports nothing for a dependency, only
/// the names it would have supplied once the application imports them.
pub fn missing(app: &Path) -> Result<Vec<String>, BuildError> {
  let layout = Layout::of(app)?;
  let present = declared_packages(app, &layout)?;
  Ok(closure(app, &layout, &present)?.order.into_iter().filter(|package| !present.iter().any(|(name, _)| name == package)).collect())
}

/// Each package on disk whose version a range asked of it refuses: the package, the version it holds and the range.
pub fn unmet(app: &Path) -> Result<Vec<(String, String, Asked)>, BuildError> {
  let layout = Layout::of(app)?;
  let present = declared_packages(app, &layout)?;
  let manifest = TypesManifest::read(app, &layout)?;
  let closure = closure(app, &layout, &present)?;
  let mut out = Vec::new();
  for package in &closure.order {
    let (Some(asks), Some(holding)) = (closure.asked.get(package), held(app, &layout, package, &manifest)) else { continue };
    out.extend(refusing(asks, &holding).map(|ask| (package.clone(), holding.1.to_string(), ask.clone())));
  }
  Ok(out)
}

/// The `types` rows of the build report: one per package [`closure`] needs,
/// each naming a range asked of it that the version it holds does not meet.
pub fn status(app: &Path) -> Result<Vec<(String, String)>, BuildError> {
  let layout = Layout::of(app)?;
  let present = declared_packages(app, &layout)?;
  let unmet = unmet(app)?;
  let mut rows = Vec::new();
  for package in closure(app, &layout, &present)?.order {
    let mut row = match present.iter().find(|(n, _)| *n == package) {
      Some((_, typed)) if typed.from.is_empty() => format!("{}/{package}", layout.types),
      Some((_, typed)) => format!("{}/{package}  {} {}", layout.types, typed.from, typed.version),
      None => "missing; run `fsr types`".to_owned(),
    };
    for (_, _, ask) in unmet.iter().filter(|(p, _, _)| *p == package) {
      row.push_str(&format!("; {} asks {}", ask.by, ask.range));
    }
    rows.push((package, row));
  }
  Ok(rows)
}

/// What `[build] target` and `[build] lib` in the configuration beside an application ask every generated tsconfig to carry.
pub(crate) struct CompileOptions {
  target: String,
  lib: Option<Vec<String>>,
}

impl CompileOptions {
  /// The configuration's keys, `es2022` and the target's own library where it sets none.
  pub(crate) fn of(app: &Path) -> Self {
    let build = crate::config_beside(app).and_then(|config| config.build);
    Self {
      target: build.as_ref().and_then(|build| build.target.clone()).unwrap_or_else(|| snapfire_fsr_host::config::DEFAULT_TARGET.to_owned()),
      lib: build.and_then(|build| build.lib),
    }
  }

  /// The `target` line and the `lib` line when there is one, each indented as a compiler option.
  pub(crate) fn lines(&self) -> String {
    let mut out = format!("    \"target\": {},\n", serde_json::to_string(&self.target).expect("a string serializes"));
    if let Some(lib) = &self.lib {
      out.push_str(&format!("    \"lib\": {},\n", serde_json::to_string(lib).expect("strings serialize")));
    }
    out
  }
}

/// `tsconfig.json` for the editor and `tsc --noEmit`: every package under
/// `types/` path-mapped, ambient entries included, the generated context module
/// under its package name. `generated` says the build's directory is or is
/// about to be there, `shim` that `<types>/foreign.d.ts` is, `declared` that
/// the build writes a declaration of a `.vue` component's props under
/// `generated/vue/`, which an import of the component then resolves to ahead of
/// the shim, through `rootDirs` from a relative specifier and a second `paths`
/// target from an alias.
pub fn tsconfig(app: &Path, generated: bool, shim: bool, declared: bool) -> Result<String, BuildError> {
  let layout = Layout::of(app)?;
  let types = layout.types.trim_end_matches('/');
  let mut paths: Vec<(String, String)> = vec![
    ("@snapfire/fsr".to_owned(), "./generated/fsr".to_owned()),
    ("@snapfire/fsr/head".to_owned(), "./generated/head".to_owned()),
    ("@snapfire/fsr/testing".to_owned(), "./generated/testing".to_owned()),
  ];
  match declared {
    true => paths.extend(snapfire_fsr_lower::ALIASES.iter().map(|(alias, dir)| (format!("{alias}*"), format!("./{dir}*\", \"./{}/{dir}*", snapfire_fsr_lower::vue::PROPS_DECLARATION_DIR)))),
    false => paths.extend(alias_paths()),
  }
  let mut include: Vec<String> = source_dirs(app).iter().map(|dir| format!("{dir}/**/*")).collect();
  if generated {
    include.push("generated/**/*".to_owned());
  }
  if shim {
    include.push(format!("{types}/{FOREIGN_SHIM}"));
  }
  for (name, typed) in declared_packages(app, &layout)? {
    if typed.ambient {
      include.push(format!("{types}/{name}/{}", typed.entry));
    } else {
      paths.push((name.clone(), format!("./{types}/{name}/{}", typed.entry)));
    }
    paths.push((format!("{name}/*"), format!("./{types}/{name}/*")));
  }
  // Templates are JSX. An application with React reads them as React
  // components, since its browser mounts the ones with state that way; one
  // without React reads them through the dialect's own declarations.
  let react = serves_react(app, &layout)?;
  if react {
    paths.push(("@snapfire/fsr-authoring/template".to_owned(), format!("./{types}/@snapfire/fsr-authoring/template.react")));
  }
  let jsx_source = match react {
    true => String::new(),
    false => "    \"jsxImportSource\": \"@snapfire/fsr-authoring\",\n".to_owned(),
  };
  let mut compile = CompileOptions::of(app).lines();
  if declared {
    compile.push_str(&format!("    \"allowArbitraryExtensions\": true,\n    \"rootDirs\": [\".\", \"./{}\"],\n", snapfire_fsr_lower::vue::PROPS_DECLARATION_DIR));
  }
  let mut out = format!("{{\n  \"compilerOptions\": {{\n{compile}    \"module\": \"esnext\",\n    \"moduleResolution\": \"bundler\",\n    \"jsx\": \"react-jsx\",\n{jsx_source}    \"strict\": true,\n    \"noEmit\": true,\n    \"skipLibCheck\": true,\n    \"paths\": {{\n");
  let last = paths.len() - 1;
  for (i, (from, to)) in paths.iter().enumerate() {
    out.push_str(&format!("      \"{from}\": [\"{to}\"]{}\n", if i == last { "" } else { "," }));
  }
  out.push_str("    }\n  },\n  \"include\": [");
  out.push_str(&include.iter().map(|i| format!("\"{i}\"")).collect::<Vec<_>>().join(", "));
  out.push_str("]\n}\n");
  Ok(out)
}

/// `tsconfig.build.json` for snapfirec: the browser modules only, so the
/// server-side bodies and their `@snapfire/fsr` import stay out of the bundle.
/// `files` are the files the browser loads, by name or by import, so a
/// template nothing mounts is never compiled and never asks the import map for
/// a framework. `generated` are the ones the bundle overlay alone holds. `jsx` is the runtime the application's JSX compiles against.
pub fn tsconfig_build(app: &Path, files: &[String], generated: &[String], jsx: &str) -> String {
  let compile = CompileOptions::of(app).lines();
  let mut out = format!("{{\n  \"compilerOptions\": {{\n{compile}    \"outDir\": \"dist\",\n    \"rootDir\": \".\",\n    \"sourceMap\": true,\n    \"jsx\": \"react-jsx\",\n    \"jsxImportSource\": \"{jsx}\",\n    \"paths\": {{\n");
  let paths = alias_paths();
  let last = paths.len() - 1;
  for (i, (from, to)) in paths.iter().enumerate() {
    out.push_str(&format!("      \"{from}\": [\"{to}\"]{}\n", if i == last { "" } else { "," }));
  }
  let mut include: Vec<String> = files.iter().filter(|file| !generated.contains(file)).cloned().collect();
  // The stylesheets a page links compile beside the modules, so they are
  // minified with them and their `url()`s take the asset map's URLs.
  let styles = crate::assets::Sections::of(app).dirs.styles;
  if app.join(&styles).is_dir() {
    include.push(format!("{styles}/**/*.css"));
  }
  include.extend(["generated/islands.ts".to_owned(), "generated/client.ts".to_owned()]);
  let include: Vec<String> = include.into_iter().map(|i| format!("\"{i}\"")).collect();
  out.push_str(&format!("    }}\n  }},\n  \"include\": [{}]", include.join(", ")));
  // A module the build generated exists in the bundle overlay alone, which
  // only `files` can name.
  if !generated.is_empty() {
    let files: Vec<String> = generated.iter().map(|f| format!("\"{f}\"")).collect();
    out.push_str(&format!(",\n  \"files\": [{}]", files.join(", ")));
  }
  out.push_str("\n}\n");
  out
}

/// The `paths` entries for `snapfire_fsr_lower::ALIASES`, the same in both tsconfigs.
fn alias_paths() -> Vec<(String, String)> {
  snapfire_fsr_lower::ALIASES.iter().map(|(alias, dir)| (format!("{alias}*"), format!("./{dir}*"))).collect()
}

/// The `jsxImportSource` TSX compiles against in the browser bundle and in
/// specs: React's in an application that serves React, else FSR's element
/// builder, which a spec's JSX and a template the browser never loads need
/// and which mounts nothing.
pub(crate) fn jsx_source(app: &Path, layout: &Layout) -> Result<&'static str, BuildError> {
  Ok(if serves_react(app, layout)? { "react" } else { "@snapfire/fsr-client" })
}

/// Whether the import map serves React, whose declarations then type the templates' JSX.
pub(crate) fn serves_react(app: &Path, layout: &Layout) -> Result<bool, BuildError> {
  Ok(import_map_packages(app, layout)?.iter().any(|p| p == "react"))
}

/// `generated/elements.d.ts`: each tag in `elements`, as `element_templates`
/// found them, typed with its template's props plus any attribute the template
/// does not take. With React it augments `React.JSX.IntrinsicElements`, which
/// then also takes any other hyphenated tag; without it, it fills the
/// dialect's `ElementTemplates`.
pub fn element_declarations(app: &Path, layout: &Layout, elements: &[(String, String)]) -> Result<String, BuildError> {
  let react = serves_react(app, layout)?;
  let mut out = String::from("// Generated by fsr build from `elements/`. Do not edit.\n\n");
  if !react && elements.is_empty() {
    out.push_str("export {};\n");
    return Ok(out);
  }
  out.push_str(match react {
    true => "import type { DetailedHTMLProps, HTMLAttributes } from \"react\";\n",
    false => "import type { Attributes } from \"@snapfire/fsr-authoring/template\";\n",
  });
  for (i, (_, module)) in elements.iter().enumerate() {
    let file = module.split_once('#').map_or(module.as_str(), |(file, _)| file).trim_end_matches(".tsx");
    out.push_str(&format!("import type Template{i} from \"../{file}\";\n"));
  }
  let host = if react { "DetailedHTMLProps<HTMLAttributes<HTMLElement>, HTMLElement>" } else { "Attributes" };
  out.push_str(&format!("\ntype Host = {host};\n"));
  out.push_str("type Loose = { [attribute: string]: any };\n");
  out.push_str("type Props<F> = F extends (props: infer P) => any ? NonNullable<P> : {};\n");
  out.push_str("type Placed<F> = Omit<Host, keyof Props<F>> & Props<F> & Loose;\n");
  if react {
    let taken = (0..elements.len()).map(|i| format!("keyof Props<typeof Template{i}>")).collect::<Vec<_>>().join(" | ");
    out.push_str(&format!("type Taken = {};\n", if taken.is_empty() { "never" } else { &taken }));
  }
  out.push('\n');
  let entries = |indent: &str| elements.iter().enumerate().map(|(i, (tag, _))| format!("{indent}\"{tag}\": Placed<typeof Template{i}>;\n")).collect::<String>();
  match react {
    true => out.push_str(&format!("declare module \"react\" {{\n  namespace JSX {{\n    interface IntrinsicElements {{\n      [custom: `${{string}}-${{string}}`]: Omit<Host, Taken> & Loose;\n{}    }}\n  }}\n}}\n", entries("      "))),
    false => out.push_str(&format!("declare module \"@snapfire/fsr-authoring/template\" {{\n  interface ElementTemplates {{\n{}  }}\n}}\n", entries("    "))),
  }
  Ok(out)
}

/// npm's version ranges, read the way node-semver reads them with neither
/// `loose` nor `includePrerelease` (<https://github.com/npm/node-semver#ranges>).
pub mod range {
  use std::cmp::Ordering;
  use std::fmt;

  /// node-semver refuses a number above `Number.MAX_SAFE_INTEGER`.
  const MAX_SAFE: u64 = 9_007_199_254_740_991;

  #[derive(Debug, Clone, PartialEq, Eq, Hash)]
  pub enum Ident {
    Num(u64),
    Alpha(String),
  }

  impl Ord for Ident {
    fn cmp(&self, other: &Self) -> Ordering {
      match (self, other) {
        (Ident::Num(a), Ident::Num(b)) => a.cmp(b),
        (Ident::Num(_), Ident::Alpha(_)) => Ordering::Less,
        (Ident::Alpha(_), Ident::Num(_)) => Ordering::Greater,
        (Ident::Alpha(a), Ident::Alpha(b)) => a.cmp(b),
      }
    }
  }

  impl PartialOrd for Ident {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
      Some(self.cmp(other))
    }
  }

  /// A version without its build metadata, which takes no part in precedence.
  #[derive(Debug, Clone, PartialEq, Eq, Hash)]
  pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub pre: Vec<Ident>,
  }

  impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
      (self.major, self.minor, self.patch).cmp(&(other.major, other.minor, other.patch)).then_with(|| match (self.pre.is_empty(), other.pre.is_empty()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => self.pre.cmp(&other.pre),
      })
    }
  }

  impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
      Some(self.cmp(other))
    }
  }

  impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
      write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
      for (i, ident) in self.pre.iter().enumerate() {
        f.write_str(if i == 0 { "-" } else { "." })?;
        match ident {
          Ident::Num(n) => write!(f, "{n}")?,
          Ident::Alpha(s) => f.write_str(s)?,
        }
      }
      Ok(())
    }
  }

  impl Version {
    pub fn new(major: u64, minor: u64, patch: u64) -> Self {
      Self { major, minor, patch, pre: Vec::new() }
    }

    /// `major.minor.patch`, an optional `-prerelease` and `+build`, after an optional `v`.
    pub fn parse(text: &str) -> Option<Self> {
      let text = text.trim();
      let text = text.strip_prefix('v').unwrap_or(text);
      let (main, build) = match text.split_once('+') {
        Some((main, build)) => (main, Some(build)),
        None => (text, None),
      };
      if build.is_some_and(|build| !build_metadata(build)) {
        return None;
      }
      let (core, pre) = match main.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (main, None),
      };
      let mut parts = core.split('.');
      let (major, minor, patch) = (number(parts.next()?)?, number(parts.next()?)?, number(parts.next()?)?);
      if parts.next().is_some() {
        return None;
      }
      let pre = match pre {
        Some(pre) => prerelease(pre)?,
        None => Vec::new(),
      };
      Some(Self { major, minor, patch, pre })
    }

    fn lowest(major: u64, minor: u64, patch: u64) -> Self {
      Self { major, minor, patch, pre: vec![Ident::Num(0)] }
    }
  }

  /// A numeric identifier: `0` or digits with no leading zero, at most `MAX_SAFE`.
  fn number(text: &str) -> Option<u64> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) || (text.len() > 1 && text.starts_with('0')) {
      return None;
    }
    text.parse::<u64>().ok().filter(|n| *n <= MAX_SAFE)
  }

  fn build_metadata(text: &str) -> bool {
    text.split('.').all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'))
  }

  fn prerelease(text: &str) -> Option<Vec<Ident>> {
    text
      .split('.')
      .map(|part| {
        if part.is_empty() || !part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
          return None;
        }
        match part.bytes().all(|b| b.is_ascii_digit()) {
          true => number(part).map(Ident::Num),
          false => Some(Ident::Alpha(part.to_owned())),
        }
      })
      .collect()
  }

  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  enum Op {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
  }

  #[derive(Debug, Clone, PartialEq, Eq)]
  enum Comparator {
    Any,
    None,
    Cmp(Op, Version),
  }

  impl Comparator {
    fn test(&self, v: &Version) -> bool {
      match self {
        Comparator::Any => true,
        Comparator::None => false,
        Comparator::Cmp(op, c) => match op {
          Op::Lt => v < c,
          Op::Le => v <= c,
          Op::Gt => v > c,
          Op::Ge => v >= c,
          Op::Eq => v == c,
        },
      }
    }
  }

  /// A version with any of its three numbers written `x`, `X` or `*`, or left off.
  #[derive(Debug, Clone, PartialEq, Eq)]
  struct Partial {
    major: Option<u64>,
    minor: Option<u64>,
    patch: Option<u64>,
    pre: Vec<Ident>,
  }

  impl Partial {
    fn parse(text: &str) -> Option<Self> {
      let text = text.strip_prefix('v').unwrap_or(text);
      let text = text.strip_prefix('=').unwrap_or(text);
      let main = match text.split_once('+') {
        Some((main, build)) => build_metadata(build).then_some(main)?,
        None => text,
      };
      let (core, pre) = match main.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (main, None),
      };
      let xr = |part: &str| -> Option<Option<u64>> {
        match part {
          "x" | "X" | "*" => Some(None),
          _ => number(part).map(Some),
        }
      };
      let parts: Vec<&str> = core.split('.').collect();
      if parts.is_empty() || parts.len() > 3 {
        return None;
      }
      let major = xr(parts[0])?;
      let minor = match parts.get(1) {
        Some(p) => xr(p)?,
        None => None,
      };
      let patch = match parts.get(2) {
        Some(p) => xr(p)?,
        None => None,
      };
      let pre = match pre {
        Some(pre) if parts.len() == 3 => prerelease(pre)?,
        Some(_) => return None,
        None => Vec::new(),
      };
      let (minor, patch) = match (major, minor) {
        (None, _) => (None, None),
        (_, None) => (None, None),
        _ => (minor, patch),
      };
      Some(Self { major, minor, patch, pre })
    }

    fn full(&self) -> Option<Version> {
      Some(Version { major: self.major?, minor: self.minor?, patch: self.patch?, pre: self.pre.clone() })
    }
  }

  /// Any number of comparator sets, one of which a version must satisfy whole.
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct Range {
    sets: Vec<Vec<Comparator>>,
  }

  impl Range {
    /// `None` for what is not a range: a dist-tag, a URL, a path, a git or
    /// `npm:` alias, `workspace:`, or a malformed comparator.
    pub fn parse(text: &str) -> Option<Self> {
      let sets = text.split("||").map(|part| set(part.trim())).collect::<Option<Vec<_>>>()?;
      Some(Self { sets })
    }

    /// Whether `v` satisfies one comparator set whole. A prerelease does only
    /// when a comparator in that set names the same `major.minor.patch` with a
    /// prerelease of its own.
    pub fn satisfies(&self, v: &Version) -> bool {
      self.sets.iter().any(|set| {
        set.iter().all(|c| c.test(v))
          && (v.pre.is_empty() || set.iter().any(|c| matches!(c, Comparator::Cmp(_, c) if !c.pre.is_empty() && (c.major, c.minor, c.patch) == (v.major, v.minor, v.patch))))
      })
    }
  }

  fn set(text: &str) -> Option<Vec<Comparator>> {
    let mut tokens: Vec<String> = Vec::new();
    let mut pending: Option<String> = None;
    for word in text.split_whitespace() {
      let joined = match pending.take() {
        Some(op) => format!("{op}{word}"),
        None => word.to_owned(),
      };
      if matches!(joined.as_str(), "<" | "<=" | ">" | ">=" | "=" | "^" | "~" | "~>") {
        pending = Some(joined);
      } else {
        tokens.push(joined);
      }
    }
    if pending.is_some() {
      return None;
    }
    if tokens.is_empty() {
      return Some(vec![Comparator::Any]);
    }
    if tokens.len() == 3 && tokens[1] == "-" {
      return hyphen(&Partial::parse(&tokens[0])?, &Partial::parse(&tokens[2])?);
    }
    let mut out = Vec::new();
    for token in &tokens {
      out.extend(comparator(token)?);
    }
    Some(out)
  }

  fn comparator(token: &str) -> Option<Vec<Comparator>> {
    if let Some(rest) = token.strip_prefix("~>").or_else(|| token.strip_prefix('~')) {
      return Some(tilde(&Partial::parse(rest)?));
    }
    if let Some(rest) = token.strip_prefix('^') {
      return Some(caret(&Partial::parse(rest)?));
    }
    let (op, rest) = [("<=", Some(Op::Le)), (">=", Some(Op::Ge)), ("<", Some(Op::Lt)), (">", Some(Op::Gt)), ("=", None)]
      .into_iter()
      .find_map(|(prefix, op)| token.strip_prefix(prefix).map(|rest| (op, rest)))
      .unwrap_or((None, token));
    Some(xrange(op, &Partial::parse(rest)?))
  }

  fn between(from: Version, to: Version) -> Vec<Comparator> {
    vec![Comparator::Cmp(Op::Ge, from), Comparator::Cmp(Op::Lt, to)]
  }

  fn tilde(p: &Partial) -> Vec<Comparator> {
    match (p.major, p.minor, p.patch) {
      (None, _, _) => vec![Comparator::Any],
      (Some(m), None, _) => between(Version::new(m, 0, 0), Version::lowest(m + 1, 0, 0)),
      (Some(m), Some(n), None) => between(Version::new(m, n, 0), Version::lowest(m, n + 1, 0)),
      (Some(m), Some(n), Some(_)) => between(p.full().expect("all three are numbers"), Version::lowest(m, n + 1, 0)),
    }
  }

  fn caret(p: &Partial) -> Vec<Comparator> {
    match (p.major, p.minor, p.patch) {
      (None, _, _) => vec![Comparator::Any],
      (Some(m), None, _) => between(Version::new(m, 0, 0), Version::lowest(m + 1, 0, 0)),
      (Some(0), Some(n), None) => between(Version::new(0, n, 0), Version::lowest(0, n + 1, 0)),
      (Some(m), Some(n), None) => between(Version::new(m, n, 0), Version::lowest(m + 1, 0, 0)),
      (Some(0), Some(0), Some(q)) => between(p.full().expect("all three are numbers"), Version::lowest(0, 0, q + 1)),
      (Some(0), Some(n), Some(_)) => between(p.full().expect("all three are numbers"), Version::lowest(0, n + 1, 0)),
      (Some(m), Some(_), Some(_)) => between(p.full().expect("all three are numbers"), Version::lowest(m + 1, 0, 0)),
    }
  }

  /// A bare partial, or one after `<`, `<=`, `>`, `>=` or `=` (`op` `None`).
  fn xrange(op: Option<Op>, p: &Partial) -> Vec<Comparator> {
    let Some(m) = p.major else {
      return vec![match op {
        Some(Op::Lt | Op::Gt) => Comparator::None,
        _ => Comparator::Any,
      }];
    };
    match (op, p.minor, p.patch) {
      (_, Some(_), Some(_)) => vec![Comparator::Cmp(op.unwrap_or(Op::Eq), p.full().expect("all three are numbers"))],
      (None, None, _) => between(Version::new(m, 0, 0), Version::lowest(m + 1, 0, 0)),
      (None, Some(n), None) => between(Version::new(m, n, 0), Version::lowest(m, n + 1, 0)),
      (Some(op), minor, _) => {
        let n = minor.unwrap_or(0);
        let next = || match minor {
          None => (m + 1, 0),
          Some(n) => (m, n + 1),
        };
        vec![match op {
          Op::Gt => {
            let (m, n) = next();
            Comparator::Cmp(Op::Ge, Version::new(m, n, 0))
          }
          Op::Ge => Comparator::Cmp(Op::Ge, Version::new(m, n, 0)),
          Op::Lt => Comparator::Cmp(Op::Lt, Version::lowest(m, n, 0)),
          Op::Le => {
            let (m, n) = next();
            Comparator::Cmp(Op::Lt, Version::lowest(m, n, 0))
          }
          Op::Eq => unreachable!("`=` reaches here as `None`"),
        }]
      }
    }
  }

  fn hyphen(from: &Partial, to: &Partial) -> Option<Vec<Comparator>> {
    let mut out = Vec::new();
    if let Some(m) = from.major {
      out.push(Comparator::Cmp(Op::Ge, from.full().unwrap_or(Version::new(m, from.minor.unwrap_or(0), 0))));
    }
    if let Some(m) = to.major {
      out.push(match (to.minor, to.patch) {
        (None, _) => Comparator::Cmp(Op::Lt, Version::lowest(m + 1, 0, 0)),
        (Some(n), None) => Comparator::Cmp(Op::Lt, Version::lowest(m, n + 1, 0)),
        _ => Comparator::Cmp(Op::Le, to.full().expect("all three are numbers")),
      });
    }
    if out.is_empty() {
      out.push(Comparator::Any);
    }
    Some(out)
  }
}

#[cfg(test)]
mod tests {
  #[test]
  fn a_package_with_its_own_declarations_brings_the_dependencies_they_import() {
    let dir = std::env::temp_dir().join(format!("fsr-types-imports-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("dist")).unwrap();
    std::fs::write(dir.join("dist/vue.d.ts"), "import { CompilerOptions } from '@vue/compiler-dom';\nexport * from '@vue/runtime-dom';\nexport type X = import(\"@vue/shared/dist\").Y;\nimport './local';\n").unwrap();
    let dependencies = ["@vue/compiler-dom", "@vue/runtime-dom", "@vue/shared", "@vue/compiler-sfc", "@vue/server-renderer"].map(String::from);
    assert_eq!(super::imported_dependencies(&dir, dependencies.iter()), ["@vue/compiler-dom", "@vue/runtime-dom", "@vue/shared"]);
    std::fs::remove_dir_all(&dir).unwrap();
  }

  use super::*;

  /// Cases from node-semver's README and its range tests, `loose` ones left out.
  #[test]
  fn a_range_is_satisfied_as_node_semver_satisfies_it() {
    use range::{Range, Version};
    let yes = [
      ("1.0.0 - 2.0.0", "1.2.3"), ("^1.2.3+build", "1.2.3"), ("^1.2.3+build", "1.3.0"), ("1.2.3-pre+asdf - 2.4.3-pre+asdf", "1.2.3"), ("*", "1.2.3"), ("", "1.0.0"),
      (">=1.0.0", "1.0.0"), (">1.0.0", "1.0.1"), ("<=2.0.0", "2.0.0"), ("<2.0.0", "1.9999.9999"), (">= 1.0.0", "1.0.0"), ("<    2.0.0", "1.9999.9999"),
      ("0.1.20 || 1.2.4", "1.2.4"), (">=0.2.3 || <0.0.1", "0.2.4"), ("||", "1.3.4"), ("2.x.x", "2.1.3"), ("1.2.x", "1.2.3"), ("1.2.x || 2.x", "2.1.3"), ("x", "1.2.3"),
      ("2.*.*", "2.1.3"), ("2", "2.1.2"), ("2.3", "2.3.1"), ("~0.0.1", "0.0.2"), ("~x", "0.0.9"), ("~2", "2.0.9"), ("~2.4", "2.4.5"), ("~>3.2.1", "3.2.2"),
      ("~> 1", "1.2.3"), ("~1.0", "1.0.2"), ("~ 1.0.3", "1.0.12"), (">=1", "1.0.0"), ("< 1.2", "1.1.1"), ("~v0.5.4-pre", "0.5.5"), ("~v0.5.4-pre", "0.5.4"),
      ("=0.7.x", "0.7.2"), ("<=0.7.x", "0.7.2"), (">=0.7.x", "0.7.2"), ("<=0.7.x", "0.6.2"), ("~1.2.1 >=1.2.3", "1.2.3"), ("~1.2.1 =1.2.3", "1.2.3"),
      ("~1.2.1 1.2.3", "1.2.3"), (">=1.2.1 1.2.3", "1.2.3"), (">=1.2", "1.2.8"), ("^1.2.3", "1.8.1"), ("^0.1.2", "0.1.2"), ("^0.1", "0.1.2"), ("^0.0.1", "0.0.1"),
      ("^1.2", "1.4.2"), ("^1.2 ^1", "1.4.2"), ("^1.2.3-alpha", "1.2.3-pre"), ("^0.0.1-alpha", "0.0.1-beta"), ("^0.0.1-alpha", "0.0.1"), ("^x", "1.2.3"),
      ("x - 1.0.0", "0.9.7"), ("x - 1.x", "0.9.7"), ("1.0.0 - x", "1.9.7"), ("1.x - x", "1.9.7"), ("<=7.x", "7.9.9"), ("3.5.42", "3.5.42"), ("^7.29.8", "7.29.9"),
    ];
    let no = [
      ("1.0.0 - 2.0.0", "2.2.3"), ("1.2.3+asdf - 2.4.3+asdf", "1.2.3-pre.2"), ("^1.2.3+build", "2.0.0"), ("^1.2.3+build", "1.2.0"), ("^1.2.3", "1.2.3-pre"),
      ("^1.2", "1.2.0-pre"), (">1.2", "1.3.0-beta"), ("<=1.2.3", "1.2.3-beta"), ("=0.7.x", "0.7.0-asdf"), (">=0.7.x", "0.7.0-asdf"), ("1.0.0", "1.0.1"),
      (">=1.0.0", "0.1.0"), (">1.0.0", "0.0.1"), ("<=2.0.0", "2.2.9"), ("<2.0.0", "2.9999.9999"), (">=0.1.97", "0.1.93"), ("0.1.20 || 1.2.4", "1.2.3"),
      (">=0.2.3 || <0.0.1", "0.0.3"), ("2.x.x", "3.1.3"), ("1.2.x || 2.x", "1.1.3"), ("2.*.*", "1.1.3"), ("2", "1.1.2"), ("2.3", "2.4.1"), ("~0.0.1", "0.1.0-alpha"),
      ("~0.0.1", "0.1.0"), ("~2.4", "2.5.0"), ("~2.4", "2.3.9"), ("~>3.2.1", "3.3.2"), ("~>3.2.1", "3.2.0"), ("~1", "0.2.3"), ("~>1", "2.2.3"), ("~1.0", "1.1.0"),
      ("<1", "1.0.0"), (">=1.2", "1.1.1"), ("=0.7.x", "0.8.2"), ("<0.7.x", "0.7.2"), ("<1.2.3", "1.2.3-beta"), ("=1.2.3", "1.2.3-beta"), (">1.2", "1.2.8"),
      ("^0.0.1", "0.0.2"), ("^1.2.3", "2.0.0-alpha"), ("^1.2.3", "1.2.2"), ("^1.2", "1.1.9"), ("*", "1.2.3-foo"), ("^1.0.0", "2.0.0-rc1"), ("^1.2.3-rc2", "2.0.0"),
      ("1 - 2", "3.0.0-pre"), ("1 - 2", "2.0.0-pre"), ("1 - 2", "1.0.0-pre"), ("1.1.x", "1.1.0-a"), ("1.x", "1.2.0-a"), (">=1.0.0 <1.1.0", "1.1.0"),
      (">=1.0.0 <1.1.0-pre", "1.1.0-pre"), ("< 1", "1.0.0-beta"), ("<=0.7.x", "0.8.0"), (">7.x", "7.9.9"), ("3.5.42", "3.5.43"), ("^7.29.8", "8.0.7"),
    ];
    for (range, version) in yes {
      assert!(Range::parse(range).unwrap().satisfies(&Version::parse(version).unwrap()), "{version} satisfies {range:?}");
    }
    for (range, version) in no {
      assert!(!Range::parse(range).unwrap().satisfies(&Version::parse(version).unwrap()), "{version} does not satisfy {range:?}");
    }
  }

  #[test]
  fn what_is_not_a_range_parses_to_none() {
    for text in ["latest", "next", "workspace:*", "npm:foo@^1", "file:../x", "git+https://x.org/a.git", "github:a/b", "https://x.org/a.tgz", ">=", "1.2.3.4", "01.2.3", "^", "~>", "1.2.3 -", "blerg", "1.2.3-", "1.2-pre"] {
      assert_eq!(range::Range::parse(text), None, "{text:?}");
    }
  }

  /// The order semver 2.0.0 section 11 lists.
  #[test]
  fn versions_order_by_semver_precedence() {
    let order = ["1.0.0-alpha", "1.0.0-alpha.1", "1.0.0-alpha.beta", "1.0.0-beta", "1.0.0-beta.2", "1.0.0-beta.11", "1.0.0-rc.1", "1.0.0", "1.0.1", "1.1.0", "2.0.0"];
    let parsed: Vec<range::Version> = order.iter().map(|v| range::Version::parse(v).unwrap()).collect();
    for pair in parsed.windows(2) {
      assert!(pair[0] < pair[1], "{} < {}", pair[0], pair[1]);
    }
    assert_eq!(range::Version::parse("1.0.0+build.7"), range::Version::parse("1.0.0"), "build metadata takes no part");
  }

  #[test]
  fn a_site_with_nothing_vendored_takes_declarations_at_its_shells_major() {
    assert_eq!(wanted_major(None, Some("18.3.1")), Some(18));
    assert_eq!(wanted_major(Some("19.0.0"), Some("18.3.1")), Some(19));
    assert_eq!(wanted_major(None, None), None);
  }

  #[test]
  fn definitely_typed_names_mangle_scopes() {
    assert_eq!(definitely_typed("react"), "@types/react");
    assert_eq!(definitely_typed("@scope/name"), "@types/scope__name");
    assert_eq!(from_definitely_typed("@types/scope__name"), "@scope/name");
    assert_eq!(from_definitely_typed("@types/prop-types"), "prop-types");
    assert_eq!(from_definitely_typed("csstype"), "csstype");
  }

  #[test]
  fn ambient_entries_are_told_from_modules() {
    assert!(is_ambient("declare module 'sweetalert2' { const Swal: any; export default Swal }"));
    assert!(!is_ambient("export = React;\nexport as namespace React;"));
    assert!(!is_ambient("import { App } from '@vue/runtime-core';\nexport * from '@vue/runtime-core';\ndeclare module '@vue/runtime-core' {\n  interface GlobalComponents {}\n}\n"), "an augmentation in a module leaves the package path-mapped");
    assert_eq!(wanted_major(Some("19.0.0-rc.1"), None), None, "a prerelease holds no major");
  }
}
