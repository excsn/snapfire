use std::path::{Path, PathBuf};

use snapfire_fsr_cli::typecheck::{self, Typecheck};

static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn root(tag: &str) -> PathBuf {
  let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let dir = std::env::temp_dir().join(format!("fsr-cli-typecheck-{}-{n}-{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  std::fs::create_dir_all(&dir).unwrap();
  dir
}

/// A stand-in checker: prints one report and exits.
#[cfg(unix)]
fn fake_checker(dir: &Path, report: &str) -> PathBuf {
  use std::os::unix::fs::PermissionsExt;
  let path = dir.join("snapfiretc");
  std::fs::write(&path, format!("#!/bin/sh\ncat <<'END'\n{report}\nEND\n")).unwrap();
  std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
  path
}

#[cfg(unix)]
fn app_with_tsconfig(dir: &Path) -> PathBuf {
  let app = dir.join("app");
  std::fs::create_dir_all(&app).unwrap();
  std::fs::write(app.join("tsconfig.json"), "{}\n").unwrap();
  app
}

#[cfg(unix)]
#[test]
fn a_report_is_read_back_and_the_version_recorded_where_the_configuration_names_none() {
  let dir = root("records");
  let app = app_with_tsconfig(&dir);
  let config = dir.join("app.toml");
  std::fs::write(&config, "[server]\nlisten = \"127.0.0.1:8080\"\n").unwrap();
  let checker = fake_checker(&dir, r#"{"tsc":"/c/tsc","version":"7.0.2","source":"cache","sha512":null,"pinned":true,"diagnostics":[{"file":"routes/page.tsx","line":1,"column":15,"code":"TS2305","severity":"error","message":"no such member"}]}"#);
  let options = Typecheck::default().enabled(true).checker(Some(checker)).record(Some(config.clone()));

  let checked = typecheck::run(&app, &options).unwrap().expect("the checker ran");
  assert_eq!(checked.errors(), 1);
  assert_eq!(checked.row(), "tsc 7.0.2 from cache, 1 error");
  assert_eq!(checked.diagnostics[0].to_string(), "routes/page.tsx(1,15): error TS2305: no such member");
  assert_eq!(checked.recorded.as_deref(), Some(config.as_path()));
  assert!(std::fs::read_to_string(&config).unwrap().contains("[typecheck]\nversion = \"7.0.2\""));

  let again = typecheck::run(&app, &options).unwrap().expect("the checker ran");
  assert_eq!(again.recorded, None, "a version already recorded is not written twice");
}

#[cfg(unix)]
#[test]
fn nothing_runs_when_typechecking_is_off_or_no_checker_is_installed() {
  let dir = root("off");
  let app = app_with_tsconfig(&dir);
  let off = Typecheck::default().enabled(false);
  assert!(typecheck::run(&app, &off).unwrap().is_none());

  let missing = Typecheck::default().enabled(true).checker(Some(dir.join("nothing-here")));
  assert!(typecheck::run(&app, &missing).unwrap().is_none());

  let bare = dir.join("bare");
  std::fs::create_dir_all(&bare).unwrap();
  let checker = fake_checker(&dir, "{}");
  let no_tsconfig = Typecheck::default().enabled(true).checker(Some(checker));
  assert!(typecheck::run(&bare, &no_tsconfig).unwrap().is_none());
}

/// `check` typechecks the `generated/` its own build produced, not what an earlier command left on disk.
#[cfg(unix)]
#[test]
fn check_typechecks_the_generated_files_of_the_build_it_holds() {
  use std::os::unix::fs::PermissionsExt;
  use snapfire_fsr_cli::{build, check, write_generated, Options};
  let dir = root("fresh");
  let app = app_with_tsconfig(&dir);
  std::fs::create_dir_all(app.join("routes/index")).unwrap();
  std::fs::write(app.join("importmap.json"), r#"{"imports":{}}"#).unwrap();
  std::fs::write(app.join("routes/index/page.tsx"), "export default function Page() {\n  return <p>page</p>;\n}\n").unwrap();
  let loader = app.join("routes/index/page.loader.ts");
  std::fs::write(&loader, "export async function load() {\n  return { count: 2 };\n}\n").unwrap();
  let earlier = build(&app, &Options::default()).unwrap();
  write_generated(&app, &earlier).unwrap();
  assert!(std::fs::read_to_string(app.join("generated/client.ts")).unwrap().contains("count"));

  std::fs::write(&loader, "export async function load() {\n  return { total: 2 };\n}\n").unwrap();
  let checker = dir.join("snapfiretc");
  std::fs::write(&checker, "#!/bin/sh\ncp \"$2/generated/client.ts\" \"$2/seen.ts\"\necho '{\"tsc\":\"/c/tsc\",\"version\":\"7.0.2\",\"source\":\"cache\",\"sha512\":null,\"pinned\":true,\"diagnostics\":[]}'\n").unwrap();
  std::fs::set_permissions(&checker, std::fs::Permissions::from_mode(0o755)).unwrap();
  let options = Typecheck::default().enabled(true).checker(Some(checker));
  let (built, checked) = check(&app, &Options::default(), &options, |_| {}).unwrap();
  assert!(built.files.iter().any(|(name, text)| name == "generated/client.ts" && text.contains("total")));
  assert_eq!(checked.expect("the checker ran").errors(), 0);
  let seen = std::fs::read_to_string(app.join("seen.ts")).unwrap();
  assert!(seen.contains("total") && !seen.contains("count"), "the checker read the build's own types: {seen}");
}


fn shadowed_app(tag: &str, page: &str) -> PathBuf {
  use snapfire_fsr_cli::new::{create, NewOptions};
  let dir = root(tag);
  std::fs::remove_dir_all(&dir).unwrap();
  create(&dir, NewOptions::default().fetch(false).with(vec!["svelte".to_owned()])).unwrap();
  let app = dir.join("app");
  let svelte = snapfire_fsr_cli::direction::SVELTE;
  for (name, text) in [
    ("vendor/.fsr-vendor.json", format!(r#"{{"packages":{{"svelte":{{"version":"{svelte}","published":true,"entries":{{"svelte":"svelte/src/index-client.js","svelte/internal/client":"svelte/src/internal/client/index.js"}}}}}}}}"#)),
    (&format!(".fsr/compilers/svelte/{svelte}/index.js"), "globalThis.svelte = {};\n".to_owned()),
    ("tsconfig.json", "{}\n".to_owned()),
    ("routes/page.tsx", page.to_owned()),
    ("src/ui/Card.svelte", "<script lang=\"ts\">\n  import { held } from \"./Card.svelte.js\";\n</script>\n\n<p>{held.n}</p>\n".to_owned()),
    ("src/ui/Card.svelte.ts", "export const held = { n: 1 };\n".to_owned()),
  ] {
    let path = app.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
  }
  let mut map: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(app.join("importmap.json")).unwrap()).unwrap();
  map["imports"]["svelte"] = serde_json::Value::String("/static/js/vendor/svelte/src/index-client.js".to_owned());
  map["imports"]["svelte/internal/client"] = serde_json::Value::String("/static/js/vendor/svelte/src/internal/client/index.js".to_owned());
  std::fs::write(app.join("importmap.json"), map.to_string()).unwrap();
  snapfire_fsr_cli::types::refresh_embedded(&app).unwrap();
  app
}

/// Needs `snapfiretc` and `snapfirec-svelte` on PATH, as `fsr build` does.
#[test]
fn a_component_beside_a_module_of_its_own_name_typechecks_as_the_component() {
  use snapfire_fsr_cli::{build, write, Options};
  let page = "import { Island } from \"@snapfire/fsr-authoring/template\";\nimport Card from \"@src/ui/Card.svelte\";\n\nexport default function Page() {\n  return (\n    <Island when=\"load\">\n      <Card />\n    </Island>\n  );\n}\n";
  let app = shadowed_app("shadowed", page);
  let built = build(&app, &Options::default()).unwrap();
  write(&app, &built).unwrap();
  let checked = typecheck::run(&app, &Typecheck::default().enabled(true)).unwrap().expect("snapfiretc is on PATH");
  let errors: Vec<String> = checked.diagnostics.iter().map(|d| d.to_string()).collect();
  assert_eq!(checked.errors(), 0, "{errors:#?}");
  let islands = built.files.iter().find(|(name, _)| name == "generated/islands.ts").map(|(_, text)| text.clone()).unwrap();
  assert!(islands.contains("import(\"@app/src/ui/Card.svelte\")"), "{islands}");
  std::fs::remove_dir_all(app.parent().unwrap()).unwrap();
}

#[test]
fn a_relative_import_of_a_component_beside_a_module_of_its_own_name_is_refused_naming_the_alias() {
  use snapfire_fsr_cli::{build, Options};
  let page = "import { Island } from \"@snapfire/fsr-authoring/template\";\nimport Card from \"../src/ui/Card.svelte\";\n\nexport default function Page() {\n  return (\n    <Island when=\"load\">\n      <Card />\n    </Island>\n  );\n}\n";
  let app = shadowed_app("shadowed-relative", page);
  let refused = match build(&app, &Options::default()) {
    Err(e) => e.to_string(),
    Ok(_) => panic!("built"),
  };
  assert!(refused.contains("routes/page.tsx imports `../src/ui/Card.svelte`"), "{refused}");
  assert!(refused.contains("`src/ui/Card.svelte.ts`"), "{refused}");
  assert!(refused.contains("`@src/ui/Card.svelte`"), "{refused}");
  std::fs::remove_dir_all(app.parent().unwrap()).unwrap();
}
