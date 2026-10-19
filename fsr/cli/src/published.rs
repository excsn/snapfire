//! Vendors a package as published on npm, its module tree kept, for a package
//! whose subpaths share state through relative imports. Svelte's
//! `svelte/store` and `svelte/internal/client` reach one runtime that way, so a
//! bundle per subpath, which is what [`crate::vendor::add`] makes, would give
//! each subpath a runtime of its own.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

use base64::Engine;
use serde::Deserialize;
use sha2::Digest;

use crate::types::{choose_version, encode, range};
use crate::vendor::{self, AddReport, Spec, VendorManifest};
use crate::xwpm::{self, Layout};
use crate::BuildError;

/// The export conditions an entry is chosen under, most specific first: a
/// browser and a production build. The first one a target names wins,
/// whatever order the package wrote them in.
const CONDITIONS: &[&str] = &["browser", "production", "import", "module", "default"];

#[derive(Deserialize)]
struct Published {
  version: String,
  #[serde(default)]
  exports: Option<serde_json::Value>,
  #[serde(default)]
  imports: Option<serde_json::Value>,
  #[serde(default)]
  module: Option<String>,
  #[serde(default)]
  main: Option<String>,
  #[serde(default)]
  dependencies: BTreeMap<String, String>,
  dist: Dist,
}

#[derive(Deserialize)]
struct Dist {
  tarball: String,
  #[serde(default)]
  integrity: Option<String>,
}

/// One package being vendored: what the registry said of it, the files of its
/// tarball and the ones the crawl kept, with their imports rewritten.
struct Package {
  doc: Published,
  files: BTreeMap<String, Vec<u8>>,
  kept: BTreeMap<String, String>,
}

/// Where a build keeps the framework compilers it fetched, under the application. Gitignored with the rest of `.fsr/`.
pub const COMPILERS: &str = ".fsr/compilers";

/// `file` of `name@version` under `<app>/.fsr/compilers/<name>/<version>/`,
/// fetched from the registry and checked against its integrity the first time
/// and read from there after. A build tool, so never vendored or shipped.
pub fn compiler(app: &Path, name: &str, version: &str, file: &str) -> Result<std::path::PathBuf, BuildError> {
  compiler_from(app, name, version, file, crate::types::NPM_REGISTRY)
}

fn compiler_from(app: &Path, name: &str, version: &str, file: &str, registry: &str) -> Result<std::path::PathBuf, BuildError> {
  if range::Version::parse(version).is_none() {
    return Err(BuildError::Http(format!("{name}@{version}"), format!("`{version}` is not a version, so no compiler is fetched for it")));
  }
  let base = Path::new(file).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| file.to_owned());
  let path = app.join(COMPILERS).join(name).join(version).join(base);
  if path.is_file() {
    return Ok(path);
  }
  let package = fetch(&vendor::client()?, registry, name, version)?;
  let bytes = package.files.get(file).ok_or_else(|| BuildError::Http(format!("{name}/{file}"), format!("{name} {version} has no file `{file}`")))?;
  if let Some(parent) = path.parent() {
    std::fs::create_dir_all(parent).map_err(|e| BuildError::Io(parent.to_path_buf(), e))?;
  }
  std::fs::write(&path, bytes).map_err(|e| BuildError::Io(path.clone(), e))?;
  Ok(path)
}

pub fn add_published(app: &Path, specs: &[Spec]) -> Result<AddReport, BuildError> {
  add_published_from(app, specs, crate::types::NPM_REGISTRY)
}

/// `specs` widened to what `.fsr-vendor.json` records for each package it
/// already holds as published, every recorded specifier at the version asked
/// for, so `svelte@5.60.0` alone updates the whole runtime. `None` when no
/// package `specs` names was vendored as published.
pub fn recorded(app: &Path, specs: &[Spec]) -> Result<Option<Vec<Spec>>, BuildError> {
  let layout = Layout::of(app)?;
  let manifest = VendorManifest::read(app, &layout)?;
  let mut widened: Vec<Spec> = Vec::new();
  let mut any = false;
  for spec in specs {
    match manifest.packages.get(&spec.package).filter(|record| record.published) {
      Some(record) => {
        any = true;
        for specifier in record.entries.keys().filter(|s| vendor::package_of(s) == spec.package) {
          let subpath = specifier[spec.package.len()..].trim_start_matches('/');
          widened.push(Spec { package: spec.package.clone(), version: spec.version.clone(), subpath: (!subpath.is_empty()).then(|| subpath.to_owned()) });
        }
      }
      None => widened.push(spec.clone()),
    }
  }
  widened.sort_by_key(Spec::specifier);
  widened.dedup_by_key(|s| s.specifier());
  Ok(any.then_some(widened))
}

fn add_published_from(app: &Path, specs: &[Spec], registry: &str) -> Result<AddReport, BuildError> {
  let layout = Layout::of(app)?;
  if layout.xwpm {
    let mut delegated = Vec::new();
    for spec in specs {
      let arg = format!("{}@{}", spec.package, spec.version);
      if !delegated.contains(&arg) {
        xwpm::run(app, &["add", &arg])?;
        delegated.push(arg);
      }
    }
    return Ok(AddReport { added: Vec::new(), remapped: Vec::new(), from_shell: Vec::new(), delegated });
  }
  let client = vendor::client()?;
  let mut packages: BTreeMap<String, Package> = BTreeMap::new();
  let mut entries: BTreeMap<String, (String, String)> = BTreeMap::new();
  let mut wanted: Vec<(String, Option<String>, Option<String>)> = specs.iter().map(|s| (s.specifier(), Some(s.version.clone()), None)).collect();

  while let Some((specifier, version, wanted_by)) = wanted.pop() {
    if entries.contains_key(&specifier) {
      continue;
    }
    let name = vendor::package_of(&specifier);
    if !is_package_name(&name) {
      let by = match &wanted_by {
        Some(by) => format!("`{by}` imports `{specifier}`"),
        None => format!("`{specifier}` was asked for"),
      };
      return Err(BuildError::PackageName { name, by });
    }
    if !packages.contains_key(&name) {
      let version = match version {
        Some(version) => version,
        None => {
          let by = wanted_by.clone().unwrap_or_default();
          let declared = packages.get(&by).and_then(|p| p.doc.dependencies.get(&name)).cloned();
          let declared = declared.ok_or_else(|| BuildError::Dependency { package: by.clone(), wants: specifier.clone() })?;
          let ranges: Vec<range::Range> = range::Range::parse(&declared).into_iter().collect();
          choose_version(&client, registry, &name, &ranges)?.ok_or_else(|| BuildError::Http(format!("{registry}/{}", encode(&name)), format!("no version of {name} matches {declared}")))?
        }
      };
      packages.insert(name.clone(), fetch(&client, registry, &name, &version)?);
    }
    let package = packages.get_mut(&name).expect("inserted above");
    let subpath = specifier[name.len()..].trim_start_matches('/');
    let rel = entry(&package.doc, subpath).ok_or_else(|| BuildError::Http(specifier.clone(), format!("{name} {} exports no browser entry for `{specifier}`", package.doc.version)))?;
    let mut bare = Vec::new();
    crawl(&name, package, &rel, &mut bare)?;
    entries.insert(specifier, (name.clone(), rel));
    for import in bare {
      if !entries.contains_key(&import) {
        wanted.push((import, None, Some(name.clone())));
      }
    }
  }

  let mut manifest = VendorManifest::read(app, &layout)?;
  let mut map = vendor::read_import_map(app, &layout)?;
  let mut imports = map.get("imports").and_then(|v| v.as_object()).cloned().unwrap_or_default();
  let base = layout.base.trim_end_matches('/').to_owned();
  for (name, package) in &packages {
    // The tree is the package as published: a file an earlier vendoring wrote
    // that this one did not is not part of it any more.
    let dir = app.join(&layout.vendor).join(name);
    if dir.is_dir() {
      std::fs::remove_dir_all(&dir).map_err(|e| BuildError::Io(dir.clone(), e))?;
    }
    for (rel, text) in &package.kept {
      let file = dir.join(rel);
      if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|e| BuildError::Io(parent.to_path_buf(), e))?;
      }
      std::fs::write(&file, text).map_err(|e| BuildError::Io(file.clone(), e))?;
    }
    let record = manifest.packages.entry(name.clone()).or_default();
    record.version = package.doc.version.clone();
    record.entries.clear();
    record.published = true;
  }
  let mut added = Vec::new();
  for (specifier, (name, rel)) in &entries {
    let vendored = format!("{name}/{rel}");
    imports.insert(specifier.clone(), serde_json::Value::String(format!("{base}/{vendored}")));
    let size = packages[name].kept.get(rel).map_or(0, String::len);
    if let Some(record) = manifest.packages.get_mut(name) {
      record.entries.insert(specifier.clone(), vendored.clone());
    }
    added.push((specifier.clone(), vendored, size));
  }
  map.insert("imports".to_owned(), serde_json::Value::Object(imports));
  vendor::write_import_map(app, &layout, &map)?;
  manifest.write(app, &layout)?;
  Ok(AddReport { added, remapped: Vec::new(), from_shell: Vec::new(), delegated: Vec::new() })
}

/// Whether `name` is one the npm registry accepts: an optional `@scope/` and
/// a name of URL-safe characters that starts with neither `.` nor `_`, so it
/// names one directory under the vendor tree. Upper case is kept for the
/// packages published before the registry refused it.
fn is_package_name(name: &str) -> bool {
  let part = |part: &str| !part.is_empty() && !part.starts_with(['.', '_']) && part.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_' | '~'));
  let unscoped = match name.strip_prefix('@') {
    Some(scoped) => match scoped.split_once('/') {
      Some((scope, rest)) if part(scope) => rest,
      _ => return false,
    },
    None => name,
  };
  name.len() <= 214 && part(unscoped)
}

/// The version document and the tarball's files, refused when the tarball does
/// not hash to the integrity the registry published for it.
fn fetch(client: &reqwest::blocking::Client, registry: &str, name: &str, version: &str) -> Result<Package, BuildError> {
  let url = format!("{registry}/{}/{version}", encode(name));
  let bytes = vendor::get(client, &url)?.ok_or_else(|| BuildError::Http(url.clone(), "HTTP 404: no such package or version".to_owned()))?;
  let doc: Published = serde_json::from_slice(&bytes).map_err(|e| BuildError::Http(url.clone(), e.to_string()))?;
  if doc.version != version {
    return Err(BuildError::Http(url, format!("the registry answered for `{}` where `{version}` was asked for", doc.version)));
  }
  let tarball = vendor::get(client, &doc.dist.tarball)?.ok_or_else(|| BuildError::Http(doc.dist.tarball.clone(), "HTTP 404".to_owned()))?;
  let published = doc.dist.integrity.as_deref().unwrap_or("").split_whitespace().find_map(|i| i.strip_prefix("sha512-")).map(str::to_owned);
  let actual = base64::engine::general_purpose::STANDARD.encode(sha2::Sha512::digest(&tarball));
  match published {
    Some(published) if published == actual => {}
    Some(_) => return Err(BuildError::Http(doc.dist.tarball.clone(), format!("{name}@{version}: the tarball does not match the integrity the registry published"))),
    None => return Err(BuildError::Http(doc.dist.tarball.clone(), format!("{name}@{version}: the registry published no sha512 integrity to check the tarball against"))),
  }
  let files = unpack(&tarball).map_err(|e| BuildError::Io(Path::new(&doc.dist.tarball).to_path_buf(), e))?;
  Ok(Package { doc, files, kept: BTreeMap::new() })
}

/// The regular files of a gzipped npm tarball by their path under its one top
/// directory. An entry is left out when its path climbs out or holds a `\`
/// or a `:`, which another platform reads as a separator or a drive.
fn unpack(tarball: &[u8]) -> std::io::Result<BTreeMap<String, Vec<u8>>> {
  let mut files = BTreeMap::new();
  let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(tarball));
  for entry in archive.entries()? {
    let mut entry = entry?;
    if !entry.header().entry_type().is_file() {
      continue;
    }
    let path = entry.path()?.into_owned();
    if path.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
      continue;
    }
    let rel: Vec<String> = path.components().filter_map(|c| match c {
      std::path::Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
      _ => None,
    }).skip(1).collect();
    if rel.is_empty() || rel.iter().any(|part| part.contains(['\\', ':'])) {
      continue;
    }
    let mut content = Vec::new();
    entry.read_to_end(&mut content)?;
    files.insert(rel.join("/"), content);
  }
  Ok(files)
}

/// The file a subpath names, through `exports` under [`CONDITIONS`], else `module` or `main` for the root.
fn entry(doc: &Published, subpath: &str) -> Option<String> {
  let key = match subpath {
    "" => ".".to_owned(),
    sub => format!("./{sub}"),
  };
  let target = match &doc.exports {
    Some(serde_json::Value::Object(map)) if map.keys().any(|k| k.starts_with('.')) => matching(map, &key),
    Some(other) if key == "." => target(other),
    Some(_) => None,
    None if key == "." => doc.module.clone().or_else(|| doc.main.clone()).or_else(|| Some("index.js".to_owned())),
    None => Some(subpath.to_owned()),
  }?;
  Some(target.trim_start_matches("./").to_owned())
}

/// The target `key` resolves to in an `exports` or `imports` map, a `*` pattern included.
fn matching(map: &serde_json::Map<String, serde_json::Value>, key: &str) -> Option<String> {
  if let Some(value) = map.get(key) {
    return target(value);
  }
  for (pattern, value) in map {
    let Some((head, tail)) = pattern.split_once('*') else { continue };
    if let Some(middle) = key.strip_prefix(head).and_then(|rest| rest.strip_suffix(tail)) {
      return target(value).map(|t| t.replace('*', middle));
    }
  }
  None
}

fn target(value: &serde_json::Value) -> Option<String> {
  match value {
    serde_json::Value::String(path) => Some(path.clone()),
    serde_json::Value::Object(map) => CONDITIONS.iter().find_map(|condition| map.get(*condition).and_then(target)),
    serde_json::Value::Array(choices) => choices.iter().find_map(target),
    _ => None,
  }
}

/// Keeps `rel` and every file it reaches by a relative import or a package
/// alias, rewriting an alias to the relative path it names. The bare imports
/// it meets are pushed onto `bare` for the caller to vendor.
fn crawl(name: &str, package: &mut Package, rel: &str, bare: &mut Vec<String>) -> Result<(), BuildError> {
  let mut queue = vec![rel.to_owned()];
  while let Some(rel) = queue.pop() {
    if package.kept.contains_key(&rel) {
      continue;
    }
    let bytes = package.files.get(&rel).ok_or_else(|| BuildError::Http(format!("{name}/{rel}"), format!("{name} {} has no file `{rel}`", package.doc.version)))?;
    let text = String::from_utf8_lossy(bytes).into_owned();
    let mut rewrites: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    let imports = imports_of(&text).map_err(|why| BuildError::Http(format!("{name}/{rel}"), format!("{name} {} does not parse as a module: {why}", package.doc.version)))?;
    for (span, specifier) in imports {
      if specifier.starts_with("./") || specifier.starts_with("../") {
        let resolved = resolve(&rel, &specifier, &package.files).ok_or_else(|| BuildError::Http(format!("{name}/{rel}"), format!("imports `{specifier}`, which the package does not hold")))?;
        queue.push(resolved);
      } else if specifier.starts_with('#') {
        let aliases = package.doc.imports.as_ref().and_then(|v| v.as_object());
        let aliased = aliases.and_then(|map| matching(map, &specifier)).ok_or_else(|| BuildError::Dependency { package: name.to_owned(), wants: specifier.clone() })?;
        let aliased = aliased.trim_start_matches("./").to_owned();
        rewrites.push((span, relative(&rel, &aliased)));
        queue.push(aliased);
      } else if specifier.starts_with("node:") || specifier.contains(':') {
        return Err(BuildError::Dependency { package: format!("{name}/{rel}"), wants: specifier });
      } else if !bare.contains(&specifier) {
        bare.push(specifier);
      }
    }
    let mut text = text;
    for (span, replacement) in rewrites.into_iter().rev() {
      text.replace_range(span, &replacement);
    }
    package.kept.insert(rel, text);
  }
  Ok(())
}

/// Each static or dynamic import's and each re-export's specifier with the
/// byte range between its quotes, in source order, as SWC's parser reads the
/// module. A file that does not parse as a module is refused with the reason.
fn imports_of(text: &str) -> Result<Vec<(std::ops::Range<usize>, String)>, String> {
  use swc_core::common::{sync::Lrc, FileName, SourceMap, Spanned};
  use swc_core::ecma::ast::{CallExpr, Callee, ExportAll, Expr, ImportDecl, Lit, NamedExport, Str};
  use swc_core::ecma::parser::{lexer::Lexer, EsSyntax, Parser, StringInput, Syntax};
  use swc_core::ecma::visit::{Visit, VisitWith};

  struct Found<'t> {
    start: u32,
    text: &'t str,
    out: Vec<(std::ops::Range<usize>, String)>,
  }
  impl Found<'_> {
    fn add(&mut self, src: &Str) {
      let span = src.span();
      let range = (span.lo.0 - self.start) as usize + 1..(span.hi.0 - self.start) as usize - 1;
      self.out.push((range.clone(), self.text[range].to_owned()));
    }
  }
  impl Visit for Found<'_> {
    fn visit_import_decl(&mut self, n: &ImportDecl) {
      self.add(&n.src);
    }
    fn visit_export_all(&mut self, n: &ExportAll) {
      self.add(&n.src);
    }
    fn visit_named_export(&mut self, n: &NamedExport) {
      if let Some(src) = &n.src {
        self.add(src);
      }
    }
    fn visit_call_expr(&mut self, n: &CallExpr) {
      if let (Callee::Import(_), Some(arg)) = (&n.callee, n.args.first()) {
        if let (None, Expr::Lit(Lit::Str(src))) = (arg.spread, &*arg.expr) {
          self.add(src);
        }
      }
      n.visit_children_with(self);
    }
  }

  let cm: Lrc<SourceMap> = Default::default();
  let fm = cm.new_source_file(Lrc::new(FileName::Anon), text.to_owned());
  let lexer = Lexer::new(Syntax::Es(EsSyntax::default()), Default::default(), StringInput::from(&*fm), None);
  let mut parser = Parser::new_from(lexer);
  let module = parser.parse_module().map_err(|e| e.kind().msg().into_owned())?;
  if let Some(error) = parser.take_errors().first() {
    return Err(error.kind().msg().into_owned());
  }
  let mut found = Found { start: fm.start_pos.0, text, out: Vec::new() };
  module.visit_with(&mut found);
  found.out.sort_by_key(|(span, _)| span.start);
  Ok(found.out)
}

/// The package file a relative specifier in `from` names, as written or with `.js` or `/index.js` added.
fn resolve(from: &str, specifier: &str, files: &BTreeMap<String, Vec<u8>>) -> Option<String> {
  let joined = normalize(&format!("{}/{specifier}", from.rsplit_once('/').map_or("", |(dir, _)| dir)));
  [joined.clone(), format!("{joined}.js"), format!("{joined}/index.js")].into_iter().find(|candidate| files.contains_key(candidate))
}

fn normalize(path: &str) -> String {
  let mut parts: Vec<&str> = Vec::new();
  for part in path.split('/') {
    match part {
      "" | "." => {}
      ".." => {
        parts.pop();
      }
      part => parts.push(part),
    }
  }
  parts.join("/")
}

/// `to` as a specifier relative to the file `from`, both package-relative.
fn relative(from: &str, to: &str) -> String {
  let from_dir: Vec<&str> = from.split('/').collect::<Vec<_>>().split_last().map_or(Vec::new(), |(_, dir)| dir.to_vec());
  let to_parts: Vec<&str> = to.split('/').collect();
  let shared = from_dir.iter().zip(&to_parts).take_while(|(a, b)| a == b).count();
  let ups = from_dir.len() - shared;
  let rest = to_parts[shared..].join("/");
  match ups {
    0 => format!("./{rest}"),
    n => format!("{}{rest}", "../".repeat(n)),
  }
}

#[cfg(test)]
mod properties;

#[cfg(test)]
mod tests {
  use super::*;
  use crate::xwpm::Layout;
  use base64::Engine;
  use sha2::Digest;
  use std::collections::BTreeMap;

  fn tarball(files: &[(&str, &str)]) -> Vec<u8> {
    let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default()));
    for (path, text) in files {
      let mut header = tar::Header::new_gnu();
      header.set_size(text.len() as u64);
      header.set_mode(0o644);
      header.set_cksum();
      builder.append_data(&mut header, format!("package/{path}"), text.as_bytes()).unwrap();
    }
    builder.into_inner().unwrap().finish().unwrap()
  }

  fn integrity(bytes: &[u8]) -> String {
    format!("sha512-{}", base64::engine::general_purpose::STANDARD.encode(sha2::Sha512::digest(bytes)))
  }

  const PKG_FILES: &[(&str, &str)] = &[
    ("package.json", "{}"),
    ("src/index-client.js", "import { state } from './shared/state.js';\nimport { DEV } from 'dep';\nexport const client = state;\n"),
    ("src/index-server.js", "export const server = 1;\n"),
    ("src/store/index.js", "import { state } from '../shared/state.js';\nimport { K } from '#shared/constants';\nexport const store = state;\n"),
    ("src/shared/state.js", "/** @import { X } from '#nowhere' */\nexport const state = {};\n"),
    ("src/shared/constants.js", "export const K = 1;\n"),
    ("src/unused.js", "export const u = 1;\n"),
    ("compiler/index.js", "globalThis.pkg = { VERSION: \"1.0.0\" };\n"),
  ];

  const DEP_FILES: &[(&str, &str)] = &[("package.json", "{}"), ("false.js", "export const DEV = false;\n"), ("true.js", "export const DEV = true;\n"), ("fallback.js", "export const DEV = false;\n")];

  /// A registry answering `pkg@1.0.0`, which depends on `dep@^1.0.0`, plus `dep` at 1.0.0, 1.2.0 and 2.0.0.
  /// `corrupt` serves `pkg`'s tarball under an integrity that does not match it.
  fn registry(corrupt: bool) -> String {
    serve(|host| {
    let pkg = tarball(PKG_FILES);
    let dep = tarball(DEP_FILES);
    let pkg_integrity = if corrupt { integrity(b"something else") } else { integrity(&pkg) };
    let mut routes: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    routes.insert(
      "/pkg/1.0.0".to_owned(),
      serde_json::json!({
        "name": "pkg", "version": "1.0.0",
        "exports": { ".": { "types": "./index.d.ts", "browser": "./src/index-client.js", "default": "./src/index-server.js" }, "./store": { "default": "./src/store/index.js" } },
        "imports": { "#shared/constants": "./src/shared/constants.js" },
        "dependencies": { "dep": "^1.0.0" },
        "dist": { "tarball": format!("{host}/pkg/-/pkg-1.0.0.tgz"), "integrity": pkg_integrity }
      })
      .to_string()
      .into_bytes(),
    );
    routes.insert("/pkg/-/pkg-1.0.0.tgz".to_owned(), pkg);
    routes.insert("/dep".to_owned(), serde_json::json!({ "dist-tags": { "latest": "2.0.0" }, "versions": { "1.0.0": {}, "1.2.0": {}, "2.0.0": {} } }).to_string().into_bytes());
    routes.insert(
      "/dep/1.2.0".to_owned(),
      serde_json::json!({
        "name": "dep", "version": "1.2.0",
        "exports": { ".": { "production": "./false.js", "development": "./true.js", "default": "./fallback.js" } },
        "dist": { "tarball": format!("{host}/dep/-/dep-1.2.0.tgz"), "integrity": integrity(&dep) }
      })
      .to_string()
      .into_bytes(),
    );
    routes.insert("/dep/-/dep-1.2.0.tgz".to_owned(), dep);
    routes
    })
  }

  /// An HTTP server on a free port answering each path in the table `routes` builds from the host, 404 otherwise.
  fn serve(routes: impl FnOnce(&str) -> BTreeMap<String, Vec<u8>>) -> String {
    use std::io::{BufRead, BufReader, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let host = format!("http://{}", listener.local_addr().unwrap());
    let routes = routes(&host);
    std::thread::spawn(move || {
      for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        let mut reader = BufReader::new(&stream);
        let mut line = String::new();
        if reader.read_line(&mut line).is_err() {
          continue;
        }
        loop {
          let mut header = String::new();
          if reader.read_line(&mut header).is_err() || header.trim().is_empty() {
            break;
          }
        }
        let path = line.split(' ').nth(1).unwrap_or("").to_owned();
        let mut response = match routes.get(&path) {
          Some(body) => format!("HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n", body.len()).into_bytes(),
          None => b"HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n".to_vec(),
        };
        if let Some(body) = routes.get(&path) {
          response.extend_from_slice(body);
        }
        let _ = stream.write_all(&response);
      }
    });
    host
  }

  fn app(name: &str) -> std::path::PathBuf {
    let app = std::env::temp_dir().join(format!("fsr-published-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&app);
    std::fs::create_dir_all(&app).unwrap();
    app
  }

  fn specs() -> Vec<Spec> {
    vec![Spec::parse("pkg@1.0.0").unwrap(), Spec::parse("pkg@1.0.0/store").unwrap()]
  }

  fn imports(app: &Path) -> serde_json::Map<String, serde_json::Value> {
    let layout = Layout::of(app).unwrap();
    crate::vendor::read_import_map(app, &layout).unwrap().get("imports").and_then(|v| v.as_object()).cloned().unwrap_or_default()
  }

  #[test]
  fn two_subpaths_import_one_shared_module_from_the_tree_as_published() {
    let app = app("tree");
    add_published_from(&app, &specs(), &registry(false)).unwrap();
    let layout = Layout::of(&app).unwrap();
    let vendor = app.join(&layout.vendor).join("pkg");
    for kept in ["src/index-client.js", "src/store/index.js", "src/shared/state.js", "src/shared/constants.js"] {
      assert!(vendor.join(kept).is_file(), "{kept} is vendored where it was published");
    }
    for left in ["src/index-server.js", "src/unused.js"] {
      assert!(!vendor.join(left).exists(), "{left} is reached by no browser entry, so it is left out");
    }
    let base = layout.base.trim_end_matches('/').to_owned();
    let map = imports(&app);
    assert_eq!(map.get("pkg").and_then(|v| v.as_str()), Some(format!("{base}/pkg/src/index-client.js").as_str()), "{map:?}");
    assert_eq!(map.get("pkg/store").and_then(|v| v.as_str()), Some(format!("{base}/pkg/src/store/index.js").as_str()), "{map:?}");
    std::fs::remove_dir_all(&app).unwrap();
  }

  #[test]
  fn a_package_alias_is_rewritten_to_the_file_it_names() {
    let app = app("alias");
    add_published_from(&app, &specs(), &registry(false)).unwrap();
    let store = std::fs::read_to_string(app.join(Layout::of(&app).unwrap().vendor).join("pkg/src/store/index.js")).unwrap();
    assert!(store.contains("from '../shared/constants.js'"), "{store}");
    assert!(!store.contains('#'), "{store}");
    std::fs::remove_dir_all(&app).unwrap();
  }

  #[test]
  fn a_dependency_is_vendored_at_the_highest_version_its_range_accepts_under_production_conditions() {
    let app = app("dep");
    add_published_from(&app, &specs(), &registry(false)).unwrap();
    let layout = Layout::of(&app).unwrap();
    let dep = app.join(&layout.vendor).join("dep");
    assert!(dep.join("false.js").is_file());
    assert!(!dep.join("true.js").exists() && !dep.join("fallback.js").exists(), "only the production entry");
    let base = layout.base.trim_end_matches('/').to_owned();
    assert_eq!(imports(&app).get("dep").and_then(|v| v.as_str()), Some(format!("{base}/dep/false.js").as_str()));
    let manifest = crate::vendor::VendorManifest::read(&app, &layout).unwrap();
    assert_eq!(manifest.packages["dep"].version, "1.2.0", "the highest 1.x, not the latest 2.0.0");
    assert_eq!(manifest.packages["pkg"].version, "1.0.0");
    std::fs::remove_dir_all(&app).unwrap();
  }

  /// Against registry.npmjs.org: `cargo test -p snapfire_fsr_cli --lib svelte_from_the_registry -- --ignored`.
  #[test]
  #[ignore = "reads the npm registry"]
  fn svelte_from_the_registry_keeps_one_runtime_under_every_subpath() {
    let app = app("svelte");
    let subpaths = ["", "/internal/client", "/internal/disclose-version", "/internal/flags/legacy", "/store", "/reactivity", "/legacy", "/transition", "/motion", "/events"];
    let specs: Vec<Spec> = subpaths.iter().map(|sub| Spec::parse(&format!("svelte@5.57.2{sub}")).unwrap()).collect();
    let report = add_published(&app, &specs).unwrap();
    let layout = Layout::of(&app).unwrap();
    let vendor = app.join(&layout.vendor);
    let runtime = vendor.join("svelte/src/internal/client/runtime.js");
    assert!(runtime.is_file(), "the runtime is vendored once, where it was published");
    assert!(!vendor.join("svelte/compiler").exists(), "the compiler is a build tool, never vendored");
    let mut readers = Vec::new();
    fn walk(dir: &Path, into: &mut Vec<std::path::PathBuf>) {
      for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
          walk(&path, into);
        } else {
          into.push(path);
        }
      }
    }
    let mut all = Vec::new();
    walk(&vendor.join("svelte"), &mut all);
    for file in &all {
      let text = std::fs::read_to_string(file).unwrap_or_default();
      assert!(!imports_of(&text).unwrap().iter().any(|(_, s)| s.starts_with('#')), "{} still imports an alias", file.display());
      if text.contains("internal/client/runtime.js'") || text.contains("/runtime.js'") {
        readers.push(file.clone());
      }
    }
    assert!(readers.len() > 1, "more than one module reaches the one runtime file");
    let map = imports(&app);
    for (specifier, _, _) in &report.added {
      assert!(map.contains_key(specifier), "{specifier} is in the import map");
    }
    for dependency in ["esm-env", "clsx"] {
      assert!(map.contains_key(dependency), "{dependency} came with Svelte: {map:?}");
    }
    eprintln!("{} files, {} specifiers: {:?}", all.len(), report.added.len(), report.added.iter().map(|(s, r, _)| format!("{s} -> {r}")).collect::<Vec<_>>());
    std::fs::remove_dir_all(&app).unwrap();
  }

  #[test]
  fn a_new_version_of_a_package_vendored_as_published_names_every_recorded_specifier() {
    let app = app("recorded");
    add_published_from(&app, &specs(), &registry(false)).unwrap();
    let specs = recorded(&app, &[Spec::parse("pkg@1.1.0").unwrap()]).unwrap().expect("pkg was vendored as published");
    let named: Vec<String> = specs.iter().map(|s| format!("{}@{}", s.specifier(), s.version)).collect();
    assert_eq!(named, ["pkg@1.1.0", "pkg/store@1.1.0"]);
    assert!(recorded(&app, &[Spec::parse("other@1.0.0").unwrap()]).unwrap().is_none(), "a package never vendored as published is not widened");
    std::fs::remove_dir_all(&app).unwrap();
  }

  #[test]
  fn a_compiler_is_fetched_into_the_build_directory_once_and_read_from_there_after() {
    let app = app("compiler");
    let path = compiler_from(&app, "pkg", "1.0.0", "compiler/index.js", &registry(false)).unwrap();
    assert_eq!(path, app.join(".fsr/compilers/pkg/1.0.0/index.js"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "globalThis.pkg = { VERSION: \"1.0.0\" };\n");
    assert!(!app.join(Layout::of(&app).unwrap().vendor).exists(), "nothing is vendored for a compiler");
    let again = compiler_from(&app, "pkg", "1.0.0", "compiler/index.js", "http://127.0.0.1:9").unwrap();
    assert_eq!(again, path, "the second build reads what the first fetched, with no registry");
    std::fs::remove_dir_all(&app).unwrap();
  }

  #[test]
  fn a_compiler_whose_tarball_does_not_match_its_integrity_is_refused_and_nothing_is_written() {
    let app = app("compiler-corrupt");
    let refused = compiler_from(&app, "pkg", "1.0.0", "compiler/index.js", &registry(true)).err().expect("refused");
    assert!(refused.to_string().contains("integrity"), "{refused}");
    assert!(!app.join(".fsr/compilers").exists());
    std::fs::remove_dir_all(&app).unwrap();
  }

  #[test]
  fn vendoring_again_replaces_the_package_tree_so_nothing_from_before_is_left() {
    let app = app("again");
    add_published_from(&app, &specs(), &registry(false)).unwrap();
    let dir = app.join(Layout::of(&app).unwrap().vendor).join("pkg");
    std::fs::create_dir_all(dir.join("compiler")).unwrap();
    std::fs::write(dir.join("compiler/index.js"), "left by an earlier vendoring\n").unwrap();
    add_published_from(&app, &specs(), &registry(false)).unwrap();
    assert!(!dir.join("compiler").exists(), "a file the new vendoring did not write is gone");
    assert!(dir.join("src/store/index.js").is_file());
    std::fs::remove_dir_all(&app).unwrap();
  }

  #[test]
  fn a_tarball_whose_integrity_does_not_match_is_refused_and_nothing_is_written() {
    let app = app("corrupt");
    let refused = add_published_from(&app, &specs(), &registry(true)).err().expect("refused");
    assert!(refused.to_string().contains("integrity"), "{refused}");
    assert!(!app.join(Layout::of(&app).unwrap().vendor).join("pkg").exists());
    std::fs::remove_dir_all(&app).unwrap();
  }

  #[test]
  fn a_package_name_that_is_not_one_is_refused_before_anything_under_the_vendor_tree_is_touched() {
    let app = app("hostile-name");
    let vendor = app.join(Layout::of(&app).unwrap().vendor);
    std::fs::create_dir_all(vendor.join("kept")).unwrap();
    std::fs::write(vendor.join("kept/index.js"), "export const kept = 1;\n").unwrap();
    let host = serve(|host| {
      let pkg = tarball(&[("package.json", "{}"), ("index.js", "import '@x/..';\nexport const pkg = 1;\n")]);
      let hostile = tarball(&[("package.json", "{}"), ("index.js", "export const x = 1;\n")]);
      let mut routes = BTreeMap::new();
      routes.insert("/pkg/1.0.0".to_owned(), serde_json::json!({ "name": "pkg", "version": "1.0.0", "main": "index.js", "dependencies": { "@x/..": "1.0.0" }, "dist": { "tarball": format!("{host}/pkg.tgz"), "integrity": integrity(&pkg) } }).to_string().into_bytes());
      routes.insert("/pkg.tgz".to_owned(), pkg);
      routes.insert("/@x%2f..".to_owned(), serde_json::json!({ "dist-tags": { "latest": "1.0.0" }, "versions": { "1.0.0": {} } }).to_string().into_bytes());
      routes.insert("/@x%2f../1.0.0".to_owned(), serde_json::json!({ "name": "@x/..", "version": "1.0.0", "main": "index.js", "dist": { "tarball": format!("{host}/x.tgz"), "integrity": integrity(&hostile) } }).to_string().into_bytes());
      routes.insert("/x.tgz".to_owned(), hostile);
      routes
    });
    let refused = add_published_from(&app, &[Spec::parse("pkg@1.0.0").unwrap()], &host).err().expect("refused");
    assert!(refused.to_string().contains("`@x/..` is not a package name"), "{refused}");
    assert!(vendor.join("kept/index.js").is_file(), "the rest of the vendor tree is untouched");
    assert!(!vendor.join("pkg").exists());
    std::fs::remove_dir_all(&app).unwrap();
  }

  #[test]
  fn a_compiler_version_that_is_not_a_version_is_refused_and_nothing_is_written() {
    let app = app("compiler-version");
    let refused = compiler_from(&app, "pkg", "../../../escaped", "compiler/index.js", &registry(false)).err().expect("refused");
    assert!(refused.to_string().contains("`../../../escaped` is not a version"), "{refused}");
    assert!(!app.join(".fsr").exists());
    assert!(!app.parent().unwrap().join("escaped").exists());
    std::fs::remove_dir_all(&app).unwrap();
  }

  #[test]
  fn a_version_document_for_another_version_than_the_one_asked_for_is_refused() {
    let app = app("other-version");
    let host = serve(|host| {
      let pkg = tarball(&[("package.json", "{}"), ("index.js", "export const pkg = 1;\n")]);
      let mut routes = BTreeMap::new();
      routes.insert("/pkg/1.0.0".to_owned(), serde_json::json!({ "name": "pkg", "version": "../../../escaped", "main": "index.js", "dist": { "tarball": format!("{host}/pkg.tgz"), "integrity": integrity(&pkg) } }).to_string().into_bytes());
      routes.insert("/pkg.tgz".to_owned(), pkg);
      routes
    });
    let refused = add_published_from(&app, &[Spec::parse("pkg@1.0.0").unwrap()], &host).err().expect("refused");
    assert!(refused.to_string().contains("answered for `../../../escaped`"), "{refused}");
    assert!(!app.join(Layout::of(&app).unwrap().vendor).join("pkg").exists());
    std::fs::remove_dir_all(&app).unwrap();
  }
}
