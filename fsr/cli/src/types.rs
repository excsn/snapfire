//! `fsr types`: the declarations for every package the import map names, into
//! `types/`, and the `tsconfig.json` that points at them. Declarations are read
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

/// The packages generated code imports, so their declarations are written whether or not the import map names them.
const ALWAYS: &[&str] = &["@snapfire/fsr-authoring", "@snapfire/fsr-client"];

/// The declarations the fsr packages carry, written by `fsr types` without a
/// registry since the binary is the same version as the runtime they describe.
const FSR_CLIENT: &[(&str, &str)] = &[
  ("index.d.ts", include_str!("../../client/types/index.d.ts")),
  ("actions.d.ts", include_str!("../../client/types/actions.d.ts")),
  ("boot.d.ts", include_str!("../../client/types/boot.d.ts")),
  ("navigator.d.ts", include_str!("../../client/types/navigator.d.ts")),
  ("react.d.ts", include_str!("../../client/types/react.d.ts")),
  ("reader.d.ts", include_str!("../../client/types/reader.d.ts")),
  ("render.d.ts", include_str!("../../client/types/render.d.ts")),
  ("values.d.ts", include_str!("../../client/types/values.d.ts")),
];
const FSR_AUTHORING: &[(&str, &str)] = &[("index.d.ts", include_str!("../../authoring/index.d.ts"))];

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
  /// The xwpm commands run instead of fetching, when the application is xwpm's.
  pub delegated: Vec<String>,
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
  entry.contains("declare module \"") || entry.contains("declare module '")
}

fn semver(v: &str) -> Option<(u64, u64, u64)> {
  if v.contains('-') {
    return None;
  }
  let mut parts = v.split('.').map(|p| p.parse::<u64>().ok());
  Some((parts.next()??, parts.next()??, parts.next()??))
}

fn major_of(version: &str) -> Option<u64> {
  semver(version).map(|(m, _, _)| m)
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
  dependencies: BTreeMap<String, String>,
  dist: Dist,
}

#[derive(Deserialize)]
struct Dist {
  tarball: String,
}

fn encode(name: &str) -> String {
  name.replace('/', "%2f")
}

/// The version of `name` to take: the highest release sharing `major` when one is given and exists, else `latest`.
fn choose_version(client: &reqwest::blocking::Client, name: &str, major: Option<u64>) -> Result<Option<String>, BuildError> {
  let url = format!("{NPM_REGISTRY}/{}", encode(name));
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
  if let Some(major) = major {
    let best = doc.versions.keys().filter_map(|v| semver(v).filter(|s| s.0 == major).map(|s| (s, v.clone()))).max();
    if let Some((_, v)) = best {
      return Ok(Some(v));
    }
  }
  Ok(doc.dist_tags.get("latest").cloned())
}

fn version_doc(client: &reqwest::blocking::Client, name: &str, version: &str) -> Result<VersionDoc, BuildError> {
  let url = format!("{NPM_REGISTRY}/{}/{version}", encode(name));
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
  dependencies: Vec<String>,
}

/// Declarations for `package` into `<types>/<package>/`: from the package itself when it declares `types`, else from DefinitelyTyped.
fn fetch_npm(client: &reqwest::blocking::Client, app: &Path, layout: &Layout, package: &str, major: Option<u64>) -> Result<Option<Fetched>, BuildError> {
  let dir = app.join(&layout.types).join(package);
  let mut candidates: Vec<(String, bool)> = vec![(package.to_owned(), false)];
  if !package.starts_with("@types/") {
    candidates.push((definitely_typed(package), true));
  }
  for (name, from_dt) in candidates {
    let Some(version) = choose_version(client, &name, major)? else { continue };
    let doc = version_doc(client, &name, &version)?;
    let entry = doc.types.clone().or(doc.typings.clone()).or_else(|| from_dt.then(|| "index.d.ts".to_owned()));
    let Some(entry) = entry else { continue };
    let entry = entry.trim_start_matches("./").to_owned();
    let bytes = get(client, &doc.dist.tarball)?.ok_or_else(|| BuildError::Http(doc.dist.tarball.clone(), "HTTP 404".to_owned()))?;
    if dir.exists() {
      std::fs::remove_dir_all(&dir).map_err(|e| BuildError::Io(dir.clone(), e))?;
    }
    let count = unpack_declarations(&bytes, &dir)?;
    let entry_path = dir.join(&entry);
    if count == 0 || !entry_path.is_file() {
      let _ = std::fs::remove_dir_all(&dir);
      continue;
    }
    let ambient = is_ambient(&std::fs::read_to_string(&entry_path).map_err(|e| BuildError::Io(entry_path.clone(), e))?);
    let dependencies = if from_dt { doc.dependencies.keys().cloned().collect() } else { Vec::new() };
    return Ok(Some(Fetched { from: name, version: doc.version, entry, ambient, dependencies }));
  }
  Ok(None)
}

/// Fills the types directory for the fsr packages and every package the import
/// map names: the fsr packages from the binary, the rest from the npm registry
/// at the vendored major when `fsr add` recorded one. A package already present
/// is kept unless `refresh`. Under xwpm, `xwpm restore` and `xwpm types` fill
/// the directory and only the fsr packages are written here.
pub fn fetch(app: &Path, refresh: bool) -> Result<TypesReport, BuildError> {
  let layout = Layout::of(app)?;
  let mut report = TypesReport::default();
  let mut manifest = TypesManifest::read(app, &layout)?;
  let vendored = VendorManifest::read(app, &layout)?;
  let client = vendor::client()?;

  if layout.xwpm {
    xwpm::run(app, &["restore"])?;
    xwpm::run(app, &["types"])?;
    report.delegated = vec!["xwpm restore".to_owned(), "xwpm types".to_owned()];
  }

  let mut queue: Vec<String> = ALWAYS.iter().map(|s| (*s).to_owned()).collect();
  queue.extend(import_map_packages(app, &layout)?);
  let mut seen: Vec<String> = Vec::new();
  while let Some(package) = queue.first().cloned() {
    queue.remove(0);
    if seen.contains(&package) {
      continue;
    }
    seen.push(package.clone());
    let dir = app.join(&layout.types).join(&package);
    if dir.is_dir() && !refresh {
      report.kept.push(package.clone());
      if let Some(typed) = manifest.packages.get(&package) {
        if typed.from.starts_with("@types/") {
          queue.extend(dependencies_of(app, &layout, &package));
        }
      }
      continue;
    }
    let embedded = match package.as_str() {
      "@snapfire/fsr-client" => Some(FSR_CLIENT),
      "@snapfire/fsr-authoring" => Some(FSR_AUTHORING),
      _ => None,
    };
    if let Some(files) = embedded {
      write_embedded(app, &layout, &package, files)?;
      manifest.packages.insert(package.clone(), TypedPackage { version: env!("CARGO_PKG_VERSION").to_owned(), from: "fsr".to_owned(), entry: "index.d.ts".to_owned(), ambient: false });
      report.fetched.push((package, env!("CARGO_PKG_VERSION").to_owned(), "fsr".to_owned()));
      continue;
    }
    if package.starts_with("@snapfire/") {
      report.missing.push((package, "not a package fsr knows".to_owned()));
      continue;
    }
    if layout.xwpm {
      report.missing.push((package, "xwpm supplies declarations for its modules and externals".to_owned()));
      continue;
    }
    let major = vendored.packages.get(&package).and_then(|p| major_of(&p.version));
    match fetch_npm(&client, app, &layout, &package, major)? {
      Some(fetched) => {
        for dependency in fetched.dependencies {
          queue.push(from_definitely_typed(&dependency));
        }
        manifest.packages.insert(package.clone(), TypedPackage { version: fetched.version.clone(), from: fetched.from.clone(), entry: fetched.entry, ambient: fetched.ambient });
        report.fetched.push((package, fetched.version, fetched.from));
      }
      None => report.missing.push((package, "no `types` in the package and nothing on DefinitelyTyped".to_owned())),
    }
  }
  manifest.write(app, &layout)?;
  Ok(report)
}

fn dependencies_of(app: &Path, layout: &Layout, package: &str) -> Vec<String> {
  let path = app.join(&layout.types).join(package).join("package.json");
  let Ok(text) = std::fs::read_to_string(path) else { return Vec::new() };
  let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else { return Vec::new() };
  value
    .get("dependencies")
    .and_then(|d| d.as_object())
    .map(|d| d.keys().map(|k| from_definitely_typed(k)).collect())
    .unwrap_or_default()
}

/// Every package directory under the types directory, with what the manifest knows about it.
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
    out.push((name, typed));
  }
  out.sort_by(|a, b| a.0.cmp(&b.0));
  Ok(out)
}

/// The `types` rows of the build report: one per import map package.
pub fn status(app: &Path) -> Result<Vec<(String, String)>, BuildError> {
  let layout = Layout::of(app)?;
  let present = present(app, &layout)?;
  let mut rows = Vec::new();
  let mut packages: Vec<String> = ALWAYS.iter().map(|s| (*s).to_owned()).collect();
  packages.extend(import_map_packages(app, &layout)?);
  packages.dedup();
  for package in packages {
    let row = match present.iter().find(|(n, _)| *n == package) {
      Some((_, typed)) if typed.from.is_empty() => format!("{}/{package}", layout.types),
      Some((_, typed)) => format!("{}/{package}  {} {}", layout.types, typed.from, typed.version),
      None => "missing; run `fsr types`".to_owned(),
    };
    rows.push((package, row));
  }
  Ok(rows)
}

/// `tsconfig.json` for the editor and `tsc --noEmit`: every package under
/// `types/` path-mapped, ambient entries included, the generated context module
/// under its package name.
pub fn tsconfig(app: &Path) -> Result<String, BuildError> {
  let layout = Layout::of(app)?;
  let types = layout.types.trim_end_matches('/');
  let mut paths: Vec<(String, String)> = vec![("@snapfire/fsr".to_owned(), "./generated/fsr".to_owned())];
  let mut include: Vec<String> = vec!["src/**/*".to_owned(), "routes/**/*".to_owned(), "schemas/**/*".to_owned(), "generated/**/*".to_owned()];
  for (name, typed) in present(app, &layout)? {
    if typed.ambient {
      include.push(format!("{types}/{name}/{}", typed.entry));
    } else {
      paths.push((name.clone(), format!("./{types}/{name}/{}", typed.entry)));
    }
    paths.push((format!("{name}/*"), format!("./{types}/{name}/*")));
  }
  let mut out = String::from("{\n  \"compilerOptions\": {\n    \"target\": \"es2022\",\n    \"module\": \"esnext\",\n    \"moduleResolution\": \"bundler\",\n    \"jsx\": \"react-jsx\",\n    \"strict\": true,\n    \"noEmit\": true,\n    \"skipLibCheck\": true,\n    \"paths\": {\n");
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
pub fn tsconfig_build() -> String {
  "{\n  \"compilerOptions\": {\n    \"target\": \"es2022\",\n    \"outDir\": \"dist\",\n    \"rootDir\": \".\",\n    \"sourceMap\": true,\n    \"jsx\": \"react-jsx\"\n  },\n  \"include\": [\"src/**/*\", \"routes/**/*.tsx\", \"generated/islands.ts\", \"generated/client.ts\"]\n}\n".to_owned()
}

#[cfg(test)]
mod tests {
  use super::*;

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
    assert_eq!(semver("18.3.1"), Some((18, 3, 1)));
    assert_eq!(semver("19.0.0-rc.1"), None);
  }
}
