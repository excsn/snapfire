use std::path::PathBuf;

use snapfire_fsr_cli::types::{foreign_shim, status, tsconfig, write_foreign_shim, TypedPackage, TypesManifest};
use snapfire_fsr_cli::xwpm::Layout;

fn app() -> PathBuf {
  let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
  let dir = std::env::temp_dir().join(format!("fsr-cli-types-{}-{nanos}", std::process::id()));
  std::fs::create_dir_all(dir.join("types/react")).unwrap();
  std::fs::create_dir_all(dir.join("types/@snapfire/fsr-client")).unwrap();
  std::fs::create_dir_all(dir.join("types/sweetalert2")).unwrap();
  std::fs::create_dir_all(dir.join("types/csstype")).unwrap();
  std::fs::write(dir.join("types/react/index.d.ts"), "export = React;").unwrap();
  std::fs::write(dir.join("types/@snapfire/fsr-client/index.d.ts"), "export {};").unwrap();
  std::fs::write(dir.join("types/sweetalert2/sweetalert2.d.ts"), "declare module 'sweetalert2' {}").unwrap();
  std::fs::write(dir.join("types/csstype/index.d.ts"), "export {};").unwrap();
  std::fs::write(dir.join("importmap.json"), r#"{"imports":{"react":"/static/js/vendor/react/react.bundle.mjs","react/jsx-runtime":"/x","sweetalert2":"/y","@snapfire/fsr-client":"/z","lodash":"/w"}}"#).unwrap();
  let mut manifest = TypesManifest::default();
  manifest.packages.insert("react".into(), TypedPackage { version: "18.3.31".into(), from: "@types/react".into(), entry: "index.d.ts".into(), ambient: false });
  manifest.packages.insert("sweetalert2".into(), TypedPackage { version: "11.26.25".into(), from: "sweetalert2".into(), entry: "sweetalert2.d.ts".into(), ambient: true });
  manifest.write(&dir, &Layout::default()).unwrap();
  dir
}

#[test]
fn the_tsconfig_maps_every_typed_package_and_includes_ambient_entries() {
  let dir = app();
  let ts = tsconfig(&dir, true, false).unwrap();
  assert!(ts.contains("\"@snapfire/fsr\": [\"./generated/fsr\"]"), "{ts}");
  assert!(ts.contains("\"react\": [\"./types/react/index.d.ts\"]"), "{ts}");
  assert!(ts.contains("\"react/*\": [\"./types/react/*\"]"), "a subpath such as react/jsx-runtime resolves under the package: {ts}");
  assert!(ts.contains("\"csstype\": [\"./types/csstype/index.d.ts\"]"), "a dependency with no import map entry is still mapped: {ts}");
  assert!(ts.contains("\"@snapfire/fsr-client\": [\"./types/@snapfire/fsr-client/index.d.ts\"]"), "{ts}");
  assert!(!ts.contains("\"sweetalert2\": ["), "an ambient entry is not path-mapped: {ts}");
  assert!(ts.contains("\"types/sweetalert2/sweetalert2.d.ts\"]"), "it is included instead: {ts}");
  assert!(ts.contains("\"strict\": true"));
  assert!(ts.contains("\"include\": [\"generated/**/*\", \"types/sweetalert2/sweetalert2.d.ts\"]"), "only the directories the app has, plus generated when the build is writing it: {ts}");

  let rows = status(&dir).unwrap();
  let row = |name: &str| rows.iter().find(|(n, _)| n == name).map(|(_, s)| s.clone()).unwrap();
  assert_eq!(row("react"), "types/react  @types/react 18.3.31");
  assert!(!rows.iter().any(|(n, _)| n == "csstype"), "a dependency the import map does not name is not a row");
  assert_eq!(row("lodash"), "missing; run `fsr types`");
  assert!(row("@snapfire/fsr-authoring").starts_with("missing"), "the fsr packages are always listed");
  std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_foreign_shim_comes_from_the_sources_and_the_placements_and_lands_under_types() {
  let dir = app();
  assert!(foreign_shim(&dir, &[]).is_none(), "nothing foreign, no shim");
  assert!(foreign_shim(&dir, &["src/ui/Chart.svelte#default".to_owned()]).is_some_and(|s| s.contains("declare module \"*.svelte\"")), "a placement names its own extension");

  std::fs::create_dir_all(dir.join("src/ui")).unwrap();
  std::fs::write(dir.join("src/ui/Holdings.vue"), "<template><table /></template>").unwrap();
  std::fs::write(dir.join("src/main.ts"), "export {};").unwrap();
  let shim = foreign_shim(&dir, &[]).expect("a .vue under src is foreign");
  assert!(shim.contains("declare module \"*.vue\""), "{shim}");

  let written = write_foreign_shim(&dir, &Layout::default(), &[]).unwrap();
  assert_eq!(written.as_deref(), Some("types/foreign.d.ts"));
  assert!(dir.join("types/foreign.d.ts").is_file());
  let ts = tsconfig(&dir, false, written.is_some()).unwrap();
  assert!(ts.contains("\"include\": [\"src/**/*\", \"types/foreign.d.ts\", \"types/sweetalert2/sweetalert2.d.ts\"]"), "a Rust-hosted app: its sources, the shim, no generated: {ts}");

  std::fs::remove_file(dir.join("src/ui/Holdings.vue")).unwrap();
  assert!(write_foreign_shim(&dir, &Layout::default(), &[]).unwrap().is_none());
  assert!(!dir.join("types/foreign.d.ts").exists(), "a stale shim is taken away");
  std::fs::remove_dir_all(&dir).unwrap();
}
