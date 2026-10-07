//! `[build] target` and `[build] lib`: the TypeScript an application's
//! modules are checked and compiled against, written into every tsconfig the
//! build generates.

use std::path::PathBuf;

use snapfire_fsr_cli::new::{create, NewOptions};
use snapfire_fsr_cli::{build, Options};

static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// A scaffold with `toml` appended to its configuration.
fn project(tag: &str, toml: &str) -> PathBuf {
  let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let root = std::env::temp_dir().join(format!("fsr-cli-tsconfig-{}-{n}-{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&root);
  create(&root, NewOptions { fetch: false, ..NewOptions::default() }).unwrap();
  let config = root.join("config/app.toml");
  let mut text = std::fs::read_to_string(&config).unwrap();
  text.push_str(toml);
  std::fs::write(&config, text).unwrap();
  root
}

fn compiler_options(built: &snapfire_fsr_cli::Built, file: &str) -> serde_json::Value {
  let text = &built.files.iter().find(|(name, _)| name == file).unwrap_or_else(|| panic!("{file} is written")).1;
  serde_json::from_str::<serde_json::Value>(text).unwrap_or_else(|e| panic!("{file}: {e}\n{text}"))["compilerOptions"].clone()
}

#[test]
fn an_application_without_the_keys_is_checked_against_es2022_and_its_default_lib() {
  let root = project("default", "");
  let app = root.join("app");
  let built = build(&app, &Options::beside(&app)).unwrap();
  for file in ["tsconfig.json", "tsconfig.build.json"] {
    let options = compiler_options(&built, file);
    assert_eq!(options["target"], "es2022", "{file}");
    assert!(options.get("lib").is_none(), "{file} leaves lib to the target: {options}");
  }
  std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn the_configuration_names_the_target_and_lib_every_tsconfig_carries() {
  let root = project("set", "\n[build]\ntarget = \"es2023\"\nlib = [\"es2023\", \"dom\", \"dom.iterable\"]\n");
  let app = root.join("app");
  let built = build(&app, &Options::beside(&app)).unwrap();
  for file in ["tsconfig.json", "tsconfig.build.json"] {
    let options = compiler_options(&built, file);
    assert_eq!(options["target"], "es2023", "{file}");
    assert_eq!(options["lib"], serde_json::json!(["es2023", "dom", "dom.iterable"]), "{file}");
  }
  std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn the_stylesheets_under_styles_compile_beside_the_modules() {
  let root = project("styles", "");
  let app = root.join("app");
  assert!(app.join("styles").is_dir(), "the scaffold has a styles directory");
  let built = build(&app, &Options::beside(&app)).unwrap();
  let text = &built.files.iter().find(|(name, _)| name == "tsconfig.build.json").expect("written").1;
  let config: serde_json::Value = serde_json::from_str(text).unwrap();
  let include: Vec<&str> = config["include"].as_array().unwrap().iter().filter_map(|v| v.as_str()).collect();
  assert!(include.contains(&"styles/**/*.css"), "{include:?}");
  std::fs::remove_dir_all(&root).unwrap();
}

/// DEFECTS 1.61: the checker reads the tsconfig of the build that wrote the declarations, on the first build as on every later one.
#[test]
fn the_first_build_maps_the_fsr_packages_it_writes() {
  let root = project("first", "");
  let app = root.join("app");
  assert!(!app.join("types/@snapfire").exists(), "a fresh checkout holds no fsr declarations");
  let built = build(&app, &Options::beside(&app)).unwrap();
  snapfire_fsr_cli::write(&app, &built).unwrap();
  let text = &built.files.iter().find(|(name, _)| name == "tsconfig.json").expect("written").1;
  let config: serde_json::Value = serde_json::from_str(text).unwrap();
  for package in ["@snapfire/fsr-client", "@snapfire/fsr-authoring"] {
    let targets = config["compilerOptions"]["paths"][package].as_array().unwrap_or_else(|| panic!("{package} is mapped: {text}"));
    for target in targets {
      let target = target.as_str().unwrap();
      assert!(app.join(target).is_file(), "{package} maps to {target}, which the build wrote");
    }
  }
  for (package, row) in built.report.types.iter().filter(|(name, _)| name.starts_with("@snapfire/")) {
    assert!(!row.starts_with("missing"), "{package} is reported as {row}, which this build writes");
  }
  std::fs::remove_dir_all(&root).unwrap();
}
