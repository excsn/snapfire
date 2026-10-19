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
  UseOptions::default().fetch(false).example(false)
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
  let react = snapfire_fsr_cli::direction::REACT;
  std::fs::write(dir.join("vendor/.fsr-vendor.json"), format!(r#"{{"packages":{{"react":{{"version":"{react}"}},"react-dom":{{"version":"{react}"}},"htmx.org":{{"version":"4.0.0"}}}}}}"#)).unwrap();
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
  let unknown = adopt(&dir, &["solid".to_owned()], offline()).unwrap_err().to_string();
  assert!(unknown.contains("`solid` is not a direction") && unknown.contains("react, vue, svelte, elements, htmx, tailwind, tera"), "{unknown}");

  std::fs::write(dir.join("importmap.json"), r#"{"imports":{"@snapfire/fsr-client/react":"/cdn/react.js"}}"#).unwrap();
  let url = adopt(&dir, &["react".to_owned()], offline()).unwrap_err().to_string();
  assert!(url.contains("`/cdn/react.js`") && url.contains("`/static/js/fsr/react.js`"), "{url}");

  std::fs::write(dir.join("importmap.json"), BARE_MAP).unwrap();
  std::fs::create_dir_all(dir.join("vendor")).unwrap();
  std::fs::write(dir.join("vendor/.fsr-vendor.json"), r#"{"packages":{"react":{"version":"18.3.1"}}}"#).unwrap();
  let pinned = adopt(&dir, &["react".to_owned()], offline()).unwrap_err().to_string();
  assert!(pinned.contains(&format!("react@{}", snapfire_fsr_cli::direction::REACT)) && pinned.contains("react@18.3.1") && pinned.contains("`react` pins"), "{pinned}");
  std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn an_example_is_written_once_and_placed_by_the_next_lines() {
  let dir = bare(&[]);
  let adopted = adopt(&dir, &["elements".to_owned(), "htmx".to_owned(), "tera".to_owned()], UseOptions::default().fetch(false).example(true)).unwrap();
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

  let again = adopt(&dir, &["htmx".to_owned()], UseOptions::default().fetch(false).example(true)).unwrap_err().to_string();
  assert!(again.contains("routes/pulse/page.loader.ts already exists"), "{again}");
  std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn tailwind_adds_a_stylesheet_beside_the_applications_own_and_its_classes_build() {
  let page = "export default function Page() {\n  return <p className=\"rounded-md bg-sky-600 px-4\">page</p>;\n}\n";
  let dir = bare(&[("routes/page.tsx", page), ("styles/app.css", "body { margin: 0; }\n")]);
  let adopted = adopt(&dir, &["tailwind".to_owned()], offline()).unwrap();
  assert_eq!(adopted.written, [dir.join("styles/tailwind.css")]);
  let sheet = std::fs::read_to_string(dir.join("styles/tailwind.css")).unwrap();
  assert!(sheet.contains("@import \"tailwindcss/theme\" layer(theme.root);") && sheet.contains("@import \"tailwindcss/utilities\" layer(utilities);") && !sheet.contains("@import \"tailwindcss\";"), "no preflight reset over the application's own styles: {sheet}");
  assert_eq!(std::fs::read_to_string(dir.join("styles/app.css")).unwrap(), "body { margin: 0; }\n", "the application's sheet is its own");
  assert!(adopted.mapped.is_empty() && !adopted.next.iter().any(|s| s.starts_with("fsr add")), "nothing to map or vendor: {:?}", adopted.next);
  let built = build(&dir, &Options::default()).unwrap();
  let classes = built.files.iter().find(|(name, _)| name == "generated/tailwind.classes").map(|(_, text)| text.clone()).expect("the classes file");
  for class in ["rounded-md", "bg-sky-600", "px-4"] {
    assert!(classes.contains(&format!("{class}\t")), "{class}: {classes}");
  }
  let again = adopt(&dir, &["tailwind".to_owned()], offline()).unwrap();
  assert!(again.written.is_empty(), "{:?}", again.written);
  assert_eq!(again.present, ["tailwind in styles/tailwind.css"]);
  std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn tailwind_leaves_an_application_that_already_imports_it_alone() {
  let dir = bare(&[("styles/site.css", "@import \"tailwindcss\";\n")]);
  let adopted = adopt(&dir, &["tailwind".to_owned()], offline()).unwrap();
  assert!(adopted.written.is_empty() && !dir.join("styles/tailwind.css").exists(), "{:?}", adopted.written);
  assert_eq!(adopted.present, ["tailwind in styles/site.css"]);
  std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn tailwinds_example_is_a_styled_page_and_a_theme() {
  let dir = bare(&[]);
  let adopted = adopt(&dir, &["tailwind".to_owned()], UseOptions::default().fetch(false).example(true)).unwrap();
  assert!(dir.join("routes/styled/page.tsx").is_file() && dir.join("styles/themes/ocean.css").is_file(), "{:?}", adopted.written);
  assert!(adopted.edits.iter().any(|(file, _)| file == "routes/layout.tsx"), "{:?}", adopted.edits);
  let built = build(&dir, &Options::default()).unwrap();
  assert!(built.report.themes.contains(&"ocean".to_owned()), "{}", built.report);
  std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_react_a_direction_vendors_pauses_an_island_under_activity() {
  let pinned: Vec<u32> = snapfire_fsr_cli::direction::REACT.split('.').map(|part| part.parse().unwrap()).collect();
  assert!((pinned[0], pinned[1]) >= (19, 2), "React {} unmounts an island activity holds; <Activity> is 19.2 and later", snapfire_fsr_cli::direction::REACT);
}

#[test]
fn svelte_maps_its_adapter_and_names_a_published_add_for_its_runtime() {
  let dir = bare(&[]);
  let adopted = adopt(&dir, &["svelte".to_owned()], offline()).unwrap();
  assert_eq!(adopted.mapped, vec![("@snapfire/fsr-client/svelte".to_owned(), "/static/js/fsr/svelte.js".to_owned())]);
  let svelte = snapfire_fsr_cli::direction::SVELTE;
  let add = adopted.next.first().expect("an add line");
  assert!(add.starts_with(&format!("fsr add {} --published svelte@{svelte} ", dir.display())), "the runtime is vendored as published, one copy for every subpath: {add}");
  for subpath in ["internal/client", "internal/disclose-version", "store", "reactivity", "legacy"] {
    assert!(add.contains(&format!("svelte@{svelte}/{subpath}")), "{subpath}: {add}");
  }
  std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_bare_application_given_svelte_builds_a_svelte_island() {
  let page = "import { Island } from \"@snapfire/fsr-authoring/template\";\nimport Counter from \"../src/ui/Counter.svelte\";\nexport default function Page() {\n  return <Island><Counter start={0} /></Island>;\n}\n";
  let dir = bare(&[("routes/page.tsx", page), ("src/ui/Counter.svelte", "<script>\n  let { start } = $props();\n</script>\n<p>{start}</p>\n")]);
  let refused = match build(&dir, &Options::default()) {
    Ok(_) => panic!("a .svelte island built with no adapter in the map"),
    Err(e) => e.to_string(),
  };
  assert!(refused.contains("`fsr use <app dir> svelte` writes it"), "{refused}");
  adopt(&dir, &["svelte".to_owned()], offline()).unwrap();
  let svelte = snapfire_fsr_cli::direction::SVELTE;
  let mut map: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("importmap.json")).unwrap()).unwrap();
  map["imports"]["svelte"] = serde_json::Value::String("/static/js/vendor/svelte/src/index-client.js".to_owned());
  map["imports"]["svelte/internal/client"] = serde_json::Value::String("/static/js/vendor/svelte/src/internal/client/index.js".to_owned());
  std::fs::write(dir.join("importmap.json"), map.to_string()).unwrap();
  std::fs::create_dir_all(dir.join("vendor")).unwrap();
  std::fs::write(dir.join("vendor/.fsr-vendor.json"), format!(r#"{{"packages":{{"svelte":{{"version":"{svelte}","published":true,"entries":{{"svelte":"svelte/src/index-client.js","svelte/internal/client":"svelte/src/internal/client/index.js"}}}}}}}}"#)).unwrap();
  std::fs::create_dir_all(dir.join(format!(".fsr/compilers/svelte/{svelte}"))).unwrap();
  std::fs::write(dir.join(format!(".fsr/compilers/svelte/{svelte}/index.js")), "globalThis.svelte = {};\n").unwrap();
  let built = build(&dir, &Options::default()).unwrap();
  let islands = built.files.iter().find(|(name, _)| name == "generated/islands.ts").map(|(_, text)| text.clone()).unwrap();
  assert!(islands.contains("registerIsland(\"src/ui/Counter.svelte#default\""), "{islands}");
  assert!(islands.contains("svelteMounter"), "mounted by the Svelte adapter: {islands}");
  let shim = snapfire_fsr_cli::types::foreign_shim(&dir, &[]).expect("a shim");
  assert!(shim.contains("declare module \"*.svelte\""), "{shim}");
  std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_svelte_major_the_adapter_does_not_mount_is_refused_naming_the_ones_it_does() {
  let page = "import { Island } from \"@snapfire/fsr-authoring/template\";\nimport Counter from \"../src/ui/Counter.svelte\";\nexport default function Page() {\n  return <Island><Counter /></Island>;\n}\n";
  let dir = bare(&[("routes/page.tsx", page), ("src/ui/Counter.svelte", "<p>x</p>\n")]);
  adopt(&dir, &["svelte".to_owned()], offline()).unwrap();
  std::fs::create_dir_all(dir.join("vendor")).unwrap();
  std::fs::write(dir.join("vendor/.fsr-vendor.json"), r#"{"packages":{"svelte":{"version":"6.0.0","published":true,"entries":{"svelte":"svelte/src/index-client.js"}}}}"#).unwrap();
  let refused = match build(&dir, &Options::default()) {
    Ok(_) => panic!("built"),
    Err(e) => e.to_string(),
  };
  assert!(refused.contains("svelte 6.0.0") && refused.contains("5"), "{refused}");
  std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_direction_named_with_a_version_vendors_that_version() {
  let dir = bare(&[]);
  let adopted = adopt(&dir, &["svelte@5.60.0".to_owned()], offline()).unwrap();
  let add = adopted.next.first().expect("an add line");
  assert!(add.contains("svelte@5.60.0 ") && add.contains("svelte@5.60.0/internal/client") && !add.contains(snapfire_fsr_cli::direction::SVELTE), "{add}");
  let unknown = adopt(&dir, &["svelte@five".to_owned()], offline()).unwrap_err().to_string();
  assert!(unknown.contains("svelte@five"), "{unknown}");
  std::fs::remove_dir_all(&dir).unwrap();
}
