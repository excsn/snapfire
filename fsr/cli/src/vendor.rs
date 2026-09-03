//! `fsr add`: vendors a package's runtime modules from esm.sh into `vendor/`,
//! self-contained except for the externals named, and points the import map at
//! them. No npm and no conversion of our own: esm.sh rewrites the package to
//! ES modules and bundles its dependencies; what comes back is committed.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::xwpm::{self, Layout};
use crate::BuildError;

pub const ESM_HOST: &str = "https://esm.sh";
pub const VENDOR_MANIFEST: &str = ".fsr-vendor.json";

/// `react@18.3.1`, `react-dom@18.3.1/client`, `@scope/name@1.0.0/sub`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spec {
  pub package: String,
  pub version: String,
  pub subpath: Option<String>,
}

impl Spec {
  pub fn parse(raw: &str) -> Result<Self, BuildError> {
    let (head, rest) = match raw.strip_prefix('@') {
      Some(scoped) => {
        let slash = scoped.find('/').ok_or_else(|| BuildError::Spec(raw.to_owned()))?;
        (format!("@{}", &scoped[..slash]), &scoped[slash + 1..])
      }
      None => (String::new(), raw),
    };
    let at = rest.find('@').ok_or_else(|| BuildError::Spec(raw.to_owned()))?;
    let name = &rest[..at];
    let tail = &rest[at + 1..];
    let (version, subpath) = match tail.find('/') {
      Some(slash) => (&tail[..slash], Some(tail[slash + 1..].to_owned())),
      None => (tail, None),
    };
    if name.is_empty() || version.is_empty() || subpath.as_deref() == Some("") {
      return Err(BuildError::Spec(raw.to_owned()));
    }
    let package = if head.is_empty() { name.to_owned() } else { format!("{head}/{name}") };
    Ok(Self { package, version: version.to_owned(), subpath })
  }

  /// The bare specifier the import map answers.
  pub fn specifier(&self) -> String {
    match &self.subpath {
      Some(sub) => format!("{}/{sub}", self.package),
      None => self.package.clone(),
    }
  }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct VendoredPackage {
  pub version: String,
  #[serde(default)]
  pub externals: Vec<String>,
  /// Specifier to the file under `vendor/`, for example `react/jsx-runtime` to `react/jsx-runtime.bundle.mjs`.
  #[serde(default)]
  pub entries: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct VendorManifest {
  #[serde(default)]
  pub packages: BTreeMap<String, VendoredPackage>,
}

impl VendorManifest {
  pub fn read(app: &Path, layout: &Layout) -> Result<Self, BuildError> {
    let path = app.join(&layout.vendor).join(VENDOR_MANIFEST);
    if !path.is_file() {
      return Ok(Self::default());
    }
    let text = std::fs::read_to_string(&path).map_err(|e| BuildError::Io(path.clone(), e))?;
    serde_json::from_str(&text).map_err(|e| BuildError::Manifest(path, e.to_string()))
  }

  pub fn write(&self, app: &Path, layout: &Layout) -> Result<(), BuildError> {
    let path = app.join(&layout.vendor).join(VENDOR_MANIFEST);
    if let Some(parent) = path.parent() {
      std::fs::create_dir_all(parent).map_err(|e| BuildError::Io(parent.to_path_buf(), e))?;
    }
    let text = serde_json::to_string_pretty(self).map_err(|e| BuildError::Manifest(path.clone(), e.to_string()))?;
    std::fs::write(&path, text + "\n").map_err(|e| BuildError::Io(path, e))
  }
}

pub struct AddReport {
  /// Specifier, file written relative to the vendor directory, bytes.
  pub added: Vec<(String, String, usize)>,
  /// The `xwpm add` invocations run instead, when the application is xwpm's.
  pub delegated: Vec<String>,
}

/// The import map's `imports` table, read and written whole; other keys survive.
pub fn read_import_map(app: &Path, layout: &Layout) -> Result<serde_json::Map<String, serde_json::Value>, BuildError> {
  let path = app.join(&layout.importmap);
  if !path.is_file() {
    return Ok(serde_json::Map::new());
  }
  let text = std::fs::read_to_string(&path).map_err(|e| BuildError::Io(path.clone(), e))?;
  let value: serde_json::Value = serde_json::from_str(&text).map_err(|e| BuildError::Manifest(path, e.to_string()))?;
  Ok(value.as_object().cloned().unwrap_or_default())
}

pub fn write_import_map(app: &Path, layout: &Layout, map: &serde_json::Map<String, serde_json::Value>) -> Result<(), BuildError> {
  let path = app.join(&layout.importmap);
  let text = serde_json::to_string_pretty(&serde_json::Value::Object(map.clone())).map_err(|e| BuildError::Manifest(path.clone(), e.to_string()))?;
  std::fs::write(&path, text + "\n").map_err(|e| BuildError::Io(path, e))
}

/// Bare specifiers in the import map's `imports`, as package names: `react/jsx-runtime` counts as `react`.
pub fn import_map_packages(app: &Path, layout: &Layout) -> Result<Vec<String>, BuildError> {
  let map = read_import_map(app, layout)?;
  let mut names: Vec<String> = map
    .get("imports")
    .and_then(|v| v.as_object())
    .map(|imports| imports.keys().filter(|k| !k.starts_with('/') && !k.starts_with('.')).map(|k| package_of(k)).collect())
    .unwrap_or_default();
  names.sort();
  names.dedup();
  Ok(names)
}

/// The package part of a bare specifier.
pub fn package_of(specifier: &str) -> String {
  let parts: Vec<&str> = specifier.split('/').collect();
  if specifier.starts_with('@') && parts.len() >= 2 {
    format!("{}/{}", parts[0], parts[1])
  } else {
    parts[0].to_owned()
  }
}

pub fn client() -> Result<reqwest::blocking::Client, BuildError> {
  reqwest::blocking::Client::builder()
    .user_agent(concat!("fsr/", env!("CARGO_PKG_VERSION")))
    .build()
    .map_err(|e| BuildError::Http(ESM_HOST.to_owned(), e.to_string()))
}

pub fn get(client: &reqwest::blocking::Client, url: &str) -> Result<Option<Vec<u8>>, BuildError> {
  let response = client.get(url).send().map_err(|e| BuildError::Http(url.to_owned(), e.to_string()))?;
  if response.status() == reqwest::StatusCode::NOT_FOUND {
    return Ok(None);
  }
  if !response.status().is_success() {
    return Err(BuildError::Http(url.to_owned(), format!("HTTP {}", response.status())));
  }
  response.bytes().map(|b| Some(b.to_vec())).map_err(|e| BuildError::Http(url.to_owned(), e.to_string()))
}

/// The absolute `/…` paths an esm.sh stub re-exports or imports, in order.
pub(crate) fn stub_paths(stub: &str) -> Vec<String> {
  let mut paths = Vec::new();
  for line in stub.lines() {
    let line = line.trim();
    let Some(start) = line.find("\"/") else { continue };
    let rest = &line[start + 1..];
    let Some(end) = rest.find('"') else { continue };
    let path = &rest[..end];
    if !paths.iter().any(|p| p == path) {
      paths.push(path.to_owned());
    }
  }
  paths
}

/// Every quoted absolute `/…` path a module imports.
pub(crate) fn absolute_imports(module: &str) -> Vec<String> {
  let mut found = Vec::new();
  for quote in ['"', '\''] {
    let needle = format!("{quote}/");
    let mut rest = module;
    while let Some(i) = rest.find(&needle) {
      let after = &rest[i + 1..];
      if let Some(end) = after.find(quote) {
        let path = &after[..end];
        if path.ends_with(".mjs") || path.ends_with(".js") || path.contains('@') {
          if !found.iter().any(|p| p == path) {
            found.push(path.to_owned());
          }
        }
        rest = &after[end..];
      } else {
        break;
      }
    }
  }
  found
}

pub(crate) fn file_name(path: &str) -> String {
  path.rsplit('/').next().unwrap_or(path).to_owned()
}

/// Fetches `specs` from esm.sh with `externals` left bare, writes each entry's
/// bundled module under `<vendor>/<package>/`, records it in the vendor
/// manifest and points the import map at it. Under xwpm the same specs go to
/// `xwpm add`, which converts and carries dependencies itself, so `externals`
/// has nothing to say and a subpath is the package's own `exports` business.
pub fn add(app: &Path, specs: &[Spec], externals: &[String]) -> Result<AddReport, BuildError> {
  let layout = Layout::of(app)?;
  if layout.xwpm {
    let mut delegated = Vec::new();
    for spec in specs {
      let arg = format!("{}@{}", spec.package, spec.version);
      if delegated.contains(&arg) {
        continue;
      }
      xwpm::run(app, &["add", &arg])?;
      delegated.push(arg);
    }
    return Ok(AddReport { added: Vec::new(), delegated });
  }
  let client = client()?;
  let mut manifest = VendorManifest::read(app, &layout)?;
  let mut map = read_import_map(app, &layout)?;
  let mut imports = map.get("imports").and_then(|v| v.as_object()).cloned().unwrap_or_default();
  let mut added = Vec::new();

  for spec in specs {
    let mut url = format!("{ESM_HOST}/{}@{}", spec.package, spec.version);
    if let Some(sub) = &spec.subpath {
      url.push('/');
      url.push_str(sub);
    }
    url.push_str("?target=es2022&bundle");
    if !externals.is_empty() {
      url.push_str("&external=");
      url.push_str(&externals.join(","));
    }
    let stub = get(&client, &url)?.ok_or_else(|| BuildError::Http(url.clone(), "HTTP 404: no such package or version".to_owned()))?;
    let stub = String::from_utf8_lossy(&stub).into_owned();
    let paths = stub_paths(&stub);
    let entry_path = paths
      .iter()
      .find(|p| stub.lines().any(|l| l.contains("export * from") && l.contains(p.as_str())))
      .or_else(|| paths.first())
      .ok_or_else(|| BuildError::Http(url.clone(), format!("esm.sh answered with no module path:\n{stub}")))?
      .clone();

    let dir = app.join(&layout.vendor).join(&spec.package);
    std::fs::create_dir_all(&dir).map_err(|e| BuildError::Io(dir.clone(), e))?;
    let mut queue: Vec<String> = paths.clone();
    let mut written: Vec<String> = Vec::new();
    while let Some(path) = queue.pop() {
      let name = file_name(&path);
      if written.contains(&name) {
        continue;
      }
      let module_url = format!("{ESM_HOST}{path}");
      let bytes = get(&client, &module_url)?.ok_or_else(|| BuildError::Http(module_url.clone(), "HTTP 404".to_owned()))?;
      let mut text = String::from_utf8(bytes).map_err(|e| BuildError::Http(module_url.clone(), e.to_string()))?;
      for import in absolute_imports(&text) {
        let same_package = import.starts_with(&format!("/{}@", spec.package));
        if !same_package {
          return Err(BuildError::Dependency { package: spec.specifier(), wants: import });
        }
        let sibling = file_name(&import);
        text = text.replace(&format!("\"{import}\""), &format!("\"./{sibling}\"")).replace(&format!("'{import}'"), &format!("'./{sibling}'"));
        queue.push(import);
      }
      let file = dir.join(&name);
      let size = text.len();
      std::fs::write(&file, text).map_err(|e| BuildError::Io(file.clone(), e))?;
      written.push(name.clone());
      if path == entry_path {
        let rel = format!("{}/{name}", spec.package);
        imports.insert(spec.specifier(), serde_json::Value::String(format!("{}/{rel}", layout.base.trim_end_matches('/'))));
        let entry = manifest.packages.entry(spec.package.clone()).or_default();
        entry.version = spec.version.clone();
        entry.externals = externals.to_vec();
        entry.entries.insert(spec.specifier(), rel.clone());
        added.push((spec.specifier(), rel, size));
      }
    }
  }

  map.insert("imports".to_owned(), serde_json::Value::Object(imports));
  write_import_map(app, &layout, &map)?;
  manifest.write(app, &layout)?;
  Ok(AddReport { added, delegated: Vec::new() })
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn specs_parse_scoped_names_versions_and_subpaths() {
    assert_eq!(Spec::parse("react@18.3.1").unwrap(), Spec { package: "react".into(), version: "18.3.1".into(), subpath: None });
    assert_eq!(Spec::parse("react-dom@18.3.1/client").unwrap().specifier(), "react-dom/client");
    let scoped = Spec::parse("@scope/name@1.0.0/deep/sub").unwrap();
    assert_eq!(scoped.package, "@scope/name");
    assert_eq!(scoped.subpath.as_deref(), Some("deep/sub"));
    assert!(Spec::parse("react").is_err(), "a version is required");
    assert!(Spec::parse("@scope@1").is_err());
  }

  #[test]
  fn stubs_and_modules_are_read_for_their_paths() {
    let stub = "/* esm.sh - react@18.3.1/jsx-runtime */\nimport \"/react@18.3.1/es2022/react.mjs\";\nexport * from \"/react@18.3.1/es2022/jsx-runtime.mjs\";\nexport { default } from \"/react@18.3.1/es2022/jsx-runtime.mjs\";\n";
    assert_eq!(stub_paths(stub), ["/react@18.3.1/es2022/react.mjs", "/react@18.3.1/es2022/jsx-runtime.mjs"]);
    let module = "import*as a from\"./react-dom.mjs\";import b from\"react\";import\"/scheduler@^0.23.2?target=es2022\";";
    assert_eq!(absolute_imports(module), ["/scheduler@^0.23.2?target=es2022"]);
    assert_eq!(package_of("@snapfire/fsr-client/react"), "@snapfire/fsr-client");
    assert_eq!(package_of("react/jsx-runtime"), "react");
  }
}
