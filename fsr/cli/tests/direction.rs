use std::path::{Path, PathBuf};

use snapfire_fsr_cli::direction::{adopt, UseOptions};
use snapfire_fsr_cli::{build, Options};

static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

const BARE_MAP: &str = r#"{"imports":{"@snapfire/fsr-client":"/static/js/fsr/index.js","@snapfire/fsr-client/std":"/static/js/fsr/std.js","@snapfire/fsr-client/store":"/static/js/fsr/store.js"}}"#;

fn bare(files: &[(&str, &str)]) -> PathBuf {
  let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let dir = std::env::temp_dir().join(format!("fsr-cli-direction-{}-{n}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  std::fs::create_dir_all(dir.join("routes")).unwrap();
  std::fs::write(dir.join("importmap.json"), BARE_MAP).unwrap();
  std::fs::write(dir.join("routes/page.tsx"), "export default function Page() {\n  return <p>page</p>;\n}\n").unwrap();
  for (name, source) in files {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, source).unwrap();
  }
  dir
}

fn offline() -> UseOptions {
  UseOptions { fetch: false, example: false }
}

fn imports(dir: &Path) -> serde_json::Value {
  let map: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("importmap.json")).unwrap()).unwrap();
  map["imports"].clone()
}

#[test]
fn a_direction_writes_the_adapters_line_and_names_what_it_would_vendor() {
  let dir = bare(&[]);
  let adopted = adopt(&dir, &["vue".to_owned()], offline()).unwrap();
  assert_eq!(adopted.mapped, vec![("@snapfire/fsr-client/vue".to_owned(), "/static/js/fsr/vue.js".to_owned())]);
  let react = adopt(&dir, &["react".to_owned()], offline()).unwrap();
  assert_eq!(react.mapped, vec![("@snapfire/fsr-client/react".to_owned(), "/static/js/fsr/react.js".to_owned()), ("@snapfire/fsr-authoring/template".to_owned(), "/static/js/fsr/template.js".to_owned())]);
  assert!(react.edits.is_empty(), "a portable layout needs no edit: {:?}", react.edits);
  assert_eq!(imports(&dir)["@snapfire/fsr-client/vue"], "/static/js/fsr/vue.js");
  assert_eq!(imports(&dir)["@snapfire/fsr-client/std"], "/static/js/fsr/std.js", "the other lines survive");
  assert_eq!(adopted.next, vec![format!("fsr add {} vue@3.5.13", dir.display()), format!("fsr types {}", dir.display()), format!("fsr build {}", dir.display())]);
  std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn adopting_twice_changes_nothing() {
  let dir = bare(&[]);
  adopt(&dir, &["react".to_owned(), "htmx".to_owned()], offline()).unwrap();
  std::fs::create_dir_all(dir.join("vendor")).unwrap();
  std::fs::write(dir.join("vendor/.fsr-vendor.json"), r#"{"packages":{"react":{"version":"18.3.1"},"react-dom":{"version":"18.3.1"},"htmx.org":{"version":"2.0.10"}}}"#).unwrap();
  let before = std::fs::read(dir.join("importmap.json")).unwrap();
  let again = adopt(&dir, &["react".to_owned(), "htmx".to_owned()], offline()).unwrap();
  assert!(again.mapped.is_empty(), "{:?}", again.mapped);
  assert_eq!(again.present, ["@snapfire/fsr-client/react", "@snapfire/fsr-authoring/template", "@snapfire/fsr-client/htmx"]);
  assert_eq!(again.kept, ["react", "react/jsx-runtime", "react-dom/client", "htmx.org"]);
  assert!(!again.next.iter().any(|s| s.starts_with("fsr add")), "nothing is left to vendor: {:?}", again.next);
  assert_eq!(std::fs::read(dir.join("importmap.json")).unwrap(), before);
  std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_bare_application_given_vue_builds_a_vue_island() {
  let page = "import { Island } from \"@snapfire/fsr-authoring/template\";\nimport Chart from \"../src/ui/Chart.vue\";\nexport default function Page() {\n  return <Island><Chart /></Island>;\n}\n";
  let dir = bare(&[("routes/page.tsx", page), ("src/ui/Chart.vue", "<template><p /></template>\n")]);
  let refused = match build(&dir, &Options::default()) {
    Ok(_) => panic!("a .vue island built with no adapter in the map"),
    Err(e) => e.to_string(),
  };
  assert!(refused.contains("`fsr use <app dir> vue` writes it"), "{refused}");
  adopt(&dir, &["vue".to_owned()], offline()).unwrap();
  let mut map: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("importmap.json")).unwrap()).unwrap();
  map["imports"]["vue"] = serde_json::Value::String("/static/js/vendor/vue/vue.bundle.mjs".to_owned());
  std::fs::write(dir.join("importmap.json"), map.to_string()).unwrap();
  std::fs::create_dir_all(dir.join("vendor")).unwrap();
  std::fs::write(dir.join("vendor/.fsr-vendor.json"), r#"{"packages":{"vue":{"version":"3.5.13","entries":{"vue":"vue/vue.bundle.mjs"}}}}"#).unwrap();
  let built = build(&dir, &Options::default()).unwrap();
  let islands = built.files.iter().find(|(name, _)| name == "generated/islands.ts").map(|(_, text)| text.clone()).unwrap();
  assert!(islands.contains("registerIsland(\"src/ui/Chart.vue#default\""), "{islands}");
  std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_refusals_name_the_table_the_url_and_the_version() {
  let dir = bare(&[]);
  let unknown = adopt(&dir, &["svelte".to_owned()], offline()).unwrap_err().to_string();
  assert!(unknown.contains("`svelte` is not a direction") && unknown.contains("react, vue, elements, htmx, tera"), "{unknown}");

  std::fs::write(dir.join("importmap.json"), r#"{"imports":{"@snapfire/fsr-client/react":"/cdn/react.js"}}"#).unwrap();
  let url = adopt(&dir, &["react".to_owned()], offline()).unwrap_err().to_string();
  assert!(url.contains("`/cdn/react.js`") && url.contains("`/static/js/fsr/react.js`"), "{url}");

  std::fs::write(dir.join("importmap.json"), BARE_MAP).unwrap();
  std::fs::create_dir_all(dir.join("vendor")).unwrap();
  std::fs::write(dir.join("vendor/.fsr-vendor.json"), r#"{"packages":{"react":{"version":"19.1.0"}}}"#).unwrap();
  let pinned = adopt(&dir, &["react".to_owned()], offline()).unwrap_err().to_string();
  assert!(pinned.contains("react@19.1.0") && pinned.contains("react@18.3.1") && pinned.contains("`react` pins"), "{pinned}");
  std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn an_example_is_written_once_and_placed_by_the_next_lines() {
  let dir = bare(&[]);
  let adopted = adopt(&dir, &["elements".to_owned(), "htmx".to_owned(), "tera".to_owned()], UseOptions { fetch: false, example: true }).unwrap();
  for file in ["elements/hello-tag.tsx", "src/elements/hello-tag.ts", "routes/pulse/page.tsx", "routes/pulse/page.loader.ts", "routes/hello/page.tera", "routes/hello/page.loader.ts"] {
    assert!(dir.join(file).is_file(), "{file}");
  }
  assert!(adopted.edits.iter().any(|(file, line)| file == "src/main.ts" && line.contains("import \"./elements/hello-tag.js\"")), "{:?}", adopted.edits);
  assert!(adopted.edits.iter().any(|(_, line)| line.contains("hx-get=\"/pulse?__fragment\"")), "{:?}", adopted.edits);
  assert!(adopted.edits.iter().any(|(file, line)| file == "src/main.ts" && line.starts_with("bindHtmx(htmx)")), "{:?}", adopted.edits);
  let built = build(&dir, &Options::default()).unwrap();
  assert!(built.report.routes.iter().any(|(pattern, _)| pattern == "/pulse"), "{}", built.report);
  assert!(built.manifest.components.iter().any(|c| c.module == "elements/hello-tag.tsx#default"), "{}", built.report);
  assert!(built.report.routes.iter().any(|(pattern, _)| pattern == "/hello"), "{}", built.report);
  assert!(built.report.components.iter().any(|(module, how, _)| module == "routes/hello/page.tera#default" && how == "template"), "{}", built.report);
  assert!(imports(&dir).get("@snapfire/fsr-client/tera").is_none(), "a template direction maps nothing");

  let again = adopt(&dir, &["htmx".to_owned()], UseOptions { fetch: false, example: true }).unwrap_err().to_string();
  assert!(again.contains("routes/pulse/page.loader.ts already exists"), "{again}");
  std::fs::remove_dir_all(&dir).unwrap();
}
