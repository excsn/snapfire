//! `fsr build` and `fsr check` on a terminal application: its pages lower as `fsr dev --terminal` lowers them and its generated types follow its loader.

use snapfire_fsr_cli::{build, Options};

static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn app(files: &[(&str, &str)]) -> std::path::PathBuf {
  let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let app = std::env::temp_dir().join(format!("fsr-terminal-build-{}-{n}", std::process::id())).join("app");
  let _ = std::fs::remove_dir_all(&app);
  for (name, source) in files {
    let path = app.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, source).unwrap();
  }
  app
}

/// The import map `fsr new` writes: the client and no framework.
const IMPORT_MAP: &str = r#"{"imports":{"@snapfire/fsr-client":"/c","@snapfire/fsr-client/jsx-runtime":"/j","@snapfire/fsr-client/std":"/s","@snapfire/fsr-client/store":"/st"}}"#;

fn islands(built: &snapfire_fsr_cli::Built) -> Vec<String> {
  built.files.iter().map(|(f, _)| f.clone()).filter(|f| f.contains(".island")).collect()
}

#[test]
fn a_terminal_app_builds_with_its_loader_in_root_props() {
  let page = "import { useState } from \"react\";\nimport { Text, useInput } from \"ink\";\nimport type { RootProps } from \"@generated/client\";\n\nexport default function Hello({ greeting, count }: RootProps) {\n  const [more, setMore] = useState(0);\n  useInput(() => setMore(more + 1));\n  return <Text>{greeting} {count + more}</Text>;\n}\n";
  let loader = "export async function load() {\n  return { greeting: \"hello\", count: 3 };\n}\n";
  let app = app(&[("routes/page.tsx", page), ("routes/page.loader.ts", loader), ("importmap.json", IMPORT_MAP)]);
  let built = build(&app, &Options::default()).unwrap_or_else(|e| panic!("{e}"));
  let client = built.files.iter().find(|(f, _)| f == "generated/client.ts").map(|(_, text)| text.as_str()).expect("generated/client.ts");
  let root = client.lines().find(|line| line.contains("RootProps =")).unwrap_or_else(|| panic!("{client}"));
  assert!(root.contains("greeting: string") && root.contains("count: number"), "{root}");
  assert_eq!(islands(&built), Vec::<String>::new(), "an Ink page has no browser half");
}

#[test]
fn a_page_drawing_with_ink_through_its_own_component_is_a_terminal_page() {
  let page = "import { App } from \"../src/app\";\n\nexport default function Page() {\n  return <App />;\n}\n";
  let component = "import { useState } from \"react\";\nimport { Box, Text, useInput } from \"ink\";\n\nexport function App() {\n  const [keys, setKeys] = useState(0);\n  useInput(() => setKeys(keys + 1));\n  return <Box><Text>{keys} keys</Text></Box>;\n}\n";
  let app = app(&[("routes/page.tsx", page), ("src/app.tsx", component), ("importmap.json", IMPORT_MAP)]);
  let built = build(&app, &Options::default()).unwrap_or_else(|e| panic!("{e}"));
  assert_eq!(islands(&built), Vec::<String>::new());
}
