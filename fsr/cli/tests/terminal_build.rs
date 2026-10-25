//! `fsr build` and `fsr check` on a terminal application: its pages lower as `fsr dev --terminal` lowers them and its generated types follow its loader.

use snapfire_fsr_cli::{build, Options};

static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// A project whose `config/app.toml` says `runs = "terminal"`, holding `files` under `app/`.
fn app(files: &[(&str, &str)]) -> std::path::PathBuf {
  project("[app]\nruns = \"terminal\"\n", files)
}

fn project(config: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
  let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let root = std::env::temp_dir().join(format!("fsr-terminal-build-{}-{n}", std::process::id()));
  let _ = std::fs::remove_dir_all(&root);
  std::fs::create_dir_all(root.join("config")).unwrap();
  std::fs::write(root.join("config/app.toml"), config).unwrap();
  let app = root.join("app");
  for (name, source) in files {
    let path = app.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, source).unwrap();
  }
  app
}

/// The import map `fsr new` writes: the client and no framework.
const IMPORT_MAP: &str = r#"{"imports":{"@snapfire/fsr/client":"/c","@snapfire/fsr/client/jsx-runtime":"/j","@snapfire/fsr":"/s","@snapfire/fsr/client/store":"/st"}}"#;

fn islands(built: &snapfire_fsr_cli::Built) -> Vec<String> {
  built.files.iter().map(|(f, _)| f.clone()).filter(|f| f.contains(".island")).collect()
}

#[test]
fn a_terminal_app_builds_with_its_loader_in_root_props() {
  let page = "import { useState } from \"react\";\nimport { Text, useInput } from \"@snapfire/fsr/terminal/ink\";\nimport type { RootProps } from \"@generated/client\";\n\nexport default function Hello({ greeting, count }: RootProps) {\n  const [more, setMore] = useState(0);\n  useInput(() => setMore(more + 1));\n  return <Text>{greeting} {count + more}</Text>;\n}\n";
  let loader = "export async function load() {\n  return { greeting: \"hello\", count: 3 };\n}\n";
  let app = app(&[("routes/page.tsx", page), ("routes/page.loader.ts", loader), ("importmap.json", IMPORT_MAP)]);
  let built = build(&app, &Options::default()).unwrap_or_else(|e| panic!("{e}"));
  let client = built.files.iter().find(|(f, _)| f == "generated/client.ts").map(|(_, text)| text.as_str()).expect("generated/client.ts");
  let root = client.lines().find(|line| line.contains("RootProps =")).unwrap_or_else(|| panic!("{client}"));
  assert_eq!(root, "export type RootProps = Wire<Awaited<ReturnType<typeof import(\"../routes/page.loader\")[\"load\"]>>> & HostProps;", "the loader's own return type");
  assert_eq!(islands(&built), Vec::<String>::new(), "an Ink page has no browser half");
}

const INK_PAGE: &str = "import { Text } from \"@snapfire/fsr/terminal/ink\";\n\nexport default function Page() {\n  return <Text>hi</Text>;\n}\n";

#[test]
fn an_app_runs_in_a_terminal_when_its_configuration_says_so() {
  let terminal = app(&[("routes/page.tsx", INK_PAGE)]);
  let web = project("", &[("routes/page.tsx", INK_PAGE)]);
  assert!(snapfire_fsr_cli::is_terminal_app(&terminal));
  assert!(!snapfire_fsr_cli::is_terminal_app(&web), "an Ink import alone does not make a terminal app");
}

#[test]
fn a_root_page_drawing_with_ink_in_an_app_that_runs_on_the_web_is_refused_naming_the_setting() {
  let web = project("", &[("routes/page.tsx", INK_PAGE), ("importmap.json", IMPORT_MAP)]);
  let error = build(&web, &Options::default()).err().map(|e| e.to_string()).unwrap_or_default();
  assert!(error.contains("routes/page.tsx") && error.contains("runs = \"terminal\"") && error.contains("[app]"), "{error}");
}

#[test]
fn a_page_drawing_with_ink_through_its_own_component_is_a_terminal_page() {
  let page = "import { App } from \"../src/app\";\n\nexport default function Page() {\n  return <App />;\n}\n";
  let component = "import { useState } from \"react\";\nimport { Box, Text, useInput } from \"@snapfire/fsr/terminal/ink\";\n\nexport function App() {\n  const [keys, setKeys] = useState(0);\n  useInput(() => setKeys(keys + 1));\n  return <Box><Text>{keys} keys</Text></Box>;\n}\n";
  let app = app(&[("routes/page.tsx", page), ("src/app.tsx", component), ("importmap.json", IMPORT_MAP)]);
  let built = build(&app, &Options::default()).unwrap_or_else(|e| panic!("{e}"));
  assert_eq!(islands(&built), Vec::<String>::new());
}

#[test]
fn a_terminal_app_has_declarations_for_fsrs_terminal_modules_and_asks_for_react() {
  let page = "import { useState } from \"react\";\nimport { Box, Text, useInput } from \"@snapfire/fsr/terminal/ink\";\nimport Spinner from \"@snapfire/fsr/terminal/spinner\";\n\nexport default function Page() {\n  const [n, setN] = useState(0);\n  useInput(() => setN(n + 1));\n  return <Box><Spinner /><Text>{n}</Text></Box>;\n}\n";
  let app = app(&[("routes/page.tsx", page)]);
  let built = build(&app, &Options::default()).unwrap_or_else(|e| panic!("{e}"));
  snapfire_fsr_cli::write(&app, &built).unwrap();
  for file in ["index.d.ts", "core.d.ts", "ink.d.ts", "hooks.d.ts", "spinner.d.ts", "spinners.d.ts", "text-input.d.ts"] {
    assert!(app.join("types/@snapfire/fsr/terminal").join(file).is_file(), "{file} under {}: {:?}", app.display(), std::fs::read_dir(&app).map(|d| d.flatten().map(|e| e.file_name()).collect::<Vec<_>>()));
  }
  let tsconfig = std::fs::read_to_string(app.join("tsconfig.json")).unwrap();
  assert!(tsconfig.contains("\"@snapfire/fsr/terminal\": [\"./types/@snapfire/fsr/terminal/index.d.ts\"]") && tsconfig.contains("\"@snapfire/fsr/terminal/*\": [\"./types/@snapfire/fsr/terminal/*\"]"), "{tsconfig}");
  assert!(!tsconfig.contains("jsxImportSource"), "a terminal page's JSX is React's: {tsconfig}");
  let index = std::fs::read_to_string(app.join("types/@snapfire/fsr/terminal/index.d.ts")).unwrap();
  for member in snapfire_fsr_ir::STANDARD.iter().filter(|member| member.module == "terminal") {
    for signature in member.signatures {
      assert!(index.contains(&signature.declare(member.name)), "{} in {index}", member.name);
    }
  }
  assert!(index.contains("export * from \"@snapfire/fsr/client/terminal/index.js\";"), "system, clipboard and Scope under the terminal's root: {index}");
  assert!(std::fs::read_to_string(app.join("types/@snapfire/fsr/client/terminal/index.d.ts")).unwrap().contains("export * as system from"));
  let core = std::fs::read_to_string(app.join("types/@snapfire/fsr/terminal/core.d.ts")).unwrap();
  assert!(core.contains(&snapfire_fsr_terminal::prop_declarations()), "{core}");
  let ink = std::fs::read_to_string(app.join("types/@snapfire/fsr/terminal/ink.d.ts")).unwrap();
  for (name, signatures) in snapfire_fsr_lower::meaning::terminal_primitives() {
    for signature in signatures {
      assert!(core.contains(&signature.declare(name)), "{name} in {core}");
    }
    assert!(ink.contains(&format!(" {name},")) || ink.contains(&format!(" {name} }}")), "{name} in {ink}");
  }
  assert!(snapfire_fsr_cli::types::missing(&app).unwrap().contains(&"react".to_owned()));
}

#[test]
fn the_declarations_of_fsrs_terminal_typescript_follow_its_source() {
  let source = format!("{}\nexport function useExtra(): number {{\n  return 1;\n}}\n", snapfire_fsr_lower::terminal::INK_SOURCE);
  let declared = snapfire_fsr_cli::types::declarations_of(&[("hooks.tsx".to_owned(), source.as_str())]).unwrap_or_else(|e| panic!("{e}"));
  let (name, text) = &declared[0];
  assert_eq!(name, "hooks.d.ts");
  assert!(text.contains("export declare function useExtra(): number;") && text.contains("export declare function useWindowSize(): WindowSize;"), "{text}");
}

#[test]
fn the_key_a_handler_gets_is_declared_from_the_fields_the_terminal_sets() {
  let declared = snapfire_fsr_cli::terminal::key_declaration();
  for field in snapfire_fsr_cli::terminal::KEYS {
    assert!(declared.contains(&format!("  {field}: boolean;\n")), "{field} in {declared}");
  }
  use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
  let codes = [KeyCode::Up, KeyCode::Down, KeyCode::Left, KeyCode::Right, KeyCode::PageUp, KeyCode::PageDown, KeyCode::Enter, KeyCode::Esc, KeyCode::Tab, KeyCode::BackTab, KeyCode::Backspace, KeyCode::Delete, KeyCode::Char('x')];
  for code in codes {
    let Some(snapfire_fsr_cli::terminal::Press::Input(snapfire_fsr_core::Value::Map(press))) = snapfire_fsr_cli::terminal::key_event(KeyEvent::new(code, KeyModifiers::all())) else { panic!("{code:?} is input") };
    let Some(snapfire_fsr_core::Value::Map(key)) = press.get("key") else { panic!("a key") };
    let set: Vec<&String> = key.iter().filter(|(_, on)| **on == snapfire_fsr_core::Value::Bool(true)).map(|(name, _)| name).collect();
    let own = if matches!(code, KeyCode::Char(_)) { 0 } else { 1 };
    assert_eq!(set.len(), 3 + own, "{code:?} sets ctrl, shift and meta and a field of its own in KEYS: {set:?}");
    assert_eq!(key.len(), snapfire_fsr_cli::terminal::KEYS.len(), "{code:?}");
  }
}

#[test]
fn a_terminal_app_reports_no_call_left_to_a_browser() {
  let page = "import { useState } from \"react\";\nimport { Text, useInput } from \"@snapfire/fsr/terminal/ink\";\n\nfunction clock(seconds: number) {\n  return `${String(Math.floor(seconds / 60)).padStart(2, \"0\")}:${String(seconds % 60).padStart(2, \"0\")}`;\n}\n\nexport default function Page() {\n  const [left, setLeft] = useState(90);\n  useInput(() => setLeft(left - 1));\n  return <Text>{clock(left)}</Text>;\n}\n";
  let app = app(&[("routes/page.tsx", page)]);
  let built = build(&app, &Options::default()).unwrap_or_else(|e| panic!("{e}"));
  assert_eq!(built.report.browser, Vec::<(String, String)>::new(), "a terminal app has no browser half");
}
