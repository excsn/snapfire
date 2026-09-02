use std::path::PathBuf;

use snapfire_fsr_cli::types::{status, tsconfig, TypedPackage, TypesManifest};
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
  let ts = tsconfig(&dir).unwrap();
  assert!(ts.contains("\"@snapfire/fsr\": [\"./generated/fsr\"]"), "{ts}");
  assert!(ts.contains("\"react\": [\"./types/react/index.d.ts\"]"), "{ts}");
  assert!(ts.contains("\"react/*\": [\"./types/react/*\"]"), "a subpath such as react/jsx-runtime resolves under the package: {ts}");
  assert!(ts.contains("\"csstype\": [\"./types/csstype/index.d.ts\"]"), "a dependency with no import map entry is still mapped: {ts}");
  assert!(ts.contains("\"@snapfire/fsr-client\": [\"./types/@snapfire/fsr-client/index.d.ts\"]"), "{ts}");
  assert!(!ts.contains("\"sweetalert2\": ["), "an ambient entry is not path-mapped: {ts}");
  assert!(ts.contains("\"types/sweetalert2/sweetalert2.d.ts\"]"), "it is included instead: {ts}");
  assert!(ts.contains("\"strict\": true"));

  let rows = status(&dir).unwrap();
  let row = |name: &str| rows.iter().find(|(n, _)| n == name).map(|(_, s)| s.clone()).unwrap();
  assert_eq!(row("react"), "types/react  @types/react 18.3.31");
  assert!(!rows.iter().any(|(n, _)| n == "csstype"), "a dependency the import map does not name is not a row");
  assert_eq!(row("lodash"), "missing; run `fsr types`");
  assert!(row("@snapfire/fsr-authoring").starts_with("missing"), "the fsr packages are always listed");
  std::fs::remove_dir_all(&dir).unwrap();
}
