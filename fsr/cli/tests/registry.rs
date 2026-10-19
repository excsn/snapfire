//! `fsr add --published` against registry.npmjs.org. When the registry does
//! not answer, each test says so on the terminal and passes; the line is
//! written past libtest's capture, so a skip is never silent.

use std::io::Write;
use std::path::{Path, PathBuf};

use snapfire_fsr_cli::published::{add_published, imports_of};
use snapfire_fsr_cli::vendor::Spec;

const REGISTRY: &str = "https://registry.npmjs.org/svelte/5.57.2";

/// Whether the registry answers; when it does not, the reason on stderr.
fn registry_answers(test: &str) -> bool {
  let probe = reqwest::blocking::Client::builder().timeout(std::time::Duration::from_secs(10)).build().and_then(|client| client.get(REGISTRY).send());
  let why = match probe {
    Ok(response) if response.status().is_success() => return true,
    Ok(response) => format!("HTTP {}", response.status()),
    Err(e) => e.to_string(),
  };
  let _ = writeln!(std::io::stderr(), "SKIPPED {test}: {REGISTRY} did not answer ({why})");
  false
}

fn app(name: &str) -> PathBuf {
  let app = std::env::temp_dir().join(format!("fsr-registry-{name}-{}", std::process::id()));
  let _ = std::fs::remove_dir_all(&app);
  std::fs::create_dir_all(&app).unwrap();
  app
}

fn files(dir: &Path, into: &mut Vec<PathBuf>) {
  for path in std::fs::read_dir(dir).unwrap().flatten().map(|entry| entry.path()) {
    if path.is_dir() {
      files(&path, into);
    } else {
      into.push(path);
    }
  }
}

#[test]
fn svelte_from_the_registry_keeps_one_runtime_under_every_subpath() {
  if !registry_answers("svelte_from_the_registry_keeps_one_runtime_under_every_subpath") {
    return;
  }
  let app = app("svelte");
  let subpaths = ["", "/internal/client", "/internal/disclose-version", "/internal/flags/legacy", "/store", "/reactivity", "/legacy", "/transition", "/motion", "/events"];
  let specs: Vec<Spec> = subpaths.iter().map(|sub| Spec::parse(&format!("svelte@5.57.2{sub}")).unwrap()).collect();
  let report = add_published(&app, &specs).unwrap();
  let vendor = app.join("vendor");
  assert!(vendor.join("svelte/src/internal/client/runtime.js").is_file(), "the runtime is vendored once, where it was published");
  assert!(!vendor.join("svelte/compiler").exists(), "the compiler is a build tool, never vendored");
  let mut all = Vec::new();
  files(&vendor.join("svelte"), &mut all);
  let mut readers = 0;
  for file in &all {
    let text = std::fs::read_to_string(file).unwrap_or_default();
    let imports = imports_of(&text).unwrap_or_else(|why| panic!("{}: {why}", file.display()));
    assert!(!imports.iter().any(|(_, specifier)| specifier.starts_with('#')), "{} still imports an alias", file.display());
    if text.contains("/runtime.js'") {
      readers += 1;
    }
  }
  assert!(readers > 1, "more than one module reaches the one runtime file");
  let map: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(app.join("importmap.json")).unwrap()).unwrap();
  let imports = map["imports"].as_object().unwrap();
  for (specifier, _, _) in &report.added {
    assert!(imports.contains_key(specifier), "{specifier} is in the import map");
  }
  for dependency in ["esm-env", "clsx"] {
    assert!(imports.contains_key(dependency), "{dependency} came with Svelte: {imports:?}");
  }
  std::fs::remove_dir_all(&app).unwrap();
}
