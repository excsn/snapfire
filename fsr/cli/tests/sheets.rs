//! A stylesheet imported for its effect: the plan names it under each route segment whose module reaches the importer and the bundle compiles it.

static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn app(files: &[(&str, &str)]) -> std::path::PathBuf {
  let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let dir = std::env::temp_dir().join(format!("fsr-cli-sheets-{}-{n}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  std::fs::create_dir_all(dir.join("vendor")).unwrap();
  std::fs::write(dir.join("importmap.json"), r#"{"imports":{"@snapfire/fsr-client/react":"/r","react":"/r","react-dom/client":"/d"}}"#).unwrap();
  std::fs::write(dir.join("vendor/.fsr-vendor.json"), r#"{"packages":{"react":{"version":"18.3.1"},"react-dom":{"version":"18.3.1"}}}"#).unwrap();
  for (name, source) in files {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, source).unwrap();
  }
  dir
}

#[test]
fn each_route_segment_names_the_sheets_its_module_reaches_and_the_bundle_compiles_them() {
  let dir = app(&[
    ("routes/layout.tsx", "import \"./globals.css\";\n\nexport default function Layout({ children }: { children: React.ReactNode }) {\n  return <main>{children}</main>;\n}\n"),
    ("routes/globals.css", "body { margin: 0; }\n"),
    ("routes/page.tsx", "import { Card } from \"@src/ui/Card\";\n\nexport default function Page() {\n  return <Card title=\"hi\" />;\n}\n"),
    ("routes/plain/page.tsx", "export default function Plain() {\n  return <p>plain</p>;\n}\n"),
    ("src/ui/Card.tsx", "import \"./card.css\";\nimport Badge from \"./Badge.vue\";\n\nexport function Card({ title }: { title: string }) {\n  return <div className=\"card\">{title}<Badge /></div>;\n}\n"),
    ("src/ui/card.css", ".card { color: red; }\n"),
    ("src/ui/Badge.vue", "<template><b class=\"badge\">new</b></template>\n\n<style>\n.badge { color: green; }\n</style>\n"),
  ]);
  let built = snapfire_fsr_cli::build(&dir, &snapfire_fsr_cli::Options::default()).expect("builds");
  let sheets: Vec<(&str, Vec<&str>)> = built.manifest.sheets.iter().map(|row| (row.module.as_str(), row.sheets.iter().map(String::as_str).collect())).collect();
  assert_eq!(sheets, [("routes/layout.tsx#default", vec!["routes/globals.css"]), ("routes/page.tsx#default", vec!["src/ui/card.css", "src/ui/Badge.vue.css"])]);
  let tsconfig = built.files.iter().find(|(f, _)| f == "tsconfig.build.json").map(|(_, text)| text.as_str()).unwrap();
  assert!(tsconfig.contains("\"routes/globals.css\"") && tsconfig.contains("\"src/ui/card.css\""), "{tsconfig}");
  assert!(built.browser_files.iter().any(|f| f == "src/ui/card.css"), "fsr test compiles it with the modules: {:?}", built.browser_files);
  std::fs::remove_dir_all(&dir).unwrap();
}
