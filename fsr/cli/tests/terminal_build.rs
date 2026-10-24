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
  let page = "import { useState } from \"react\";\nimport { Text, useInput } from \"@snapfire/fsr/terminal/ink\";\nimport type { RootProps } from \"@generated/client\";\n\nexport default function Hello({ greeting, count }: RootProps) {\n  const [more, setMore] = useState(0);\n  useInput(() => setMore(more + 1));\n  return <Text>{greeting} {count + more}</Text>;\n}\n";
  let loader = "export async function load() {\n  return { greeting: \"hello\", count: 3 };\n}\n";
  let app = app(&[("routes/page.tsx", page), ("routes/page.loader.ts", loader), ("importmap.json", IMPORT_MAP)]);
  let built = build(&app, &Options::default()).unwrap_or_else(|e| panic!("{e}"));
  let client = built.files.iter().find(|(f, _)| f == "generated/client.ts").map(|(_, text)| text.as_str()).expect("generated/client.ts");
  let root = client.lines().find(|line| line.contains("RootProps =")).unwrap_or_else(|| panic!("{client}"));
  assert_eq!(root, "export type RootProps = Wire<Awaited<ReturnType<typeof import(\"../routes/page.loader\")[\"load\"]>>> & HostProps;", "the loader's own return type");
  assert_eq!(islands(&built), Vec::<String>::new(), "an Ink page has no browser half");
}

#[test]
fn an_app_runs_in_a_terminal_when_its_root_page_draws_with_ink() {
  let ink = app(&[("routes/page.tsx", "import { Text } from \"@snapfire/fsr/terminal/ink\";\n\nexport default function Page() {\n  return <Text>hi</Text>;\n}\n")]);
  let web = app(&[("routes/page.tsx", "export default function Page() {\n  return <p>hi</p>;\n}\n")]);
  assert!(snapfire_fsr_cli::is_terminal_app(&ink));
  assert!(!snapfire_fsr_cli::is_terminal_app(&web));
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

/// The camelCase props `interface` declares in `core.d.ts`, as the kebab attributes the markup carries them in, beside `children`, `key` and `ref`.
fn declared_attributes(core: &str, interface: &str) -> std::collections::BTreeSet<String> {
  let body = core.split(&format!("export interface {interface} {{")).nth(1).and_then(|rest| rest.split("\n}").next()).unwrap_or_else(|| panic!("{interface} in core.d.ts"));
  body
    .lines()
    .filter_map(|line| line.trim().split_once("?:").map(|(name, _)| name.to_owned()))
    .filter(|name| !["children", "key", "ref"].contains(&name.as_str()))
    .map(|name| name.chars().flat_map(|c| if c.is_ascii_uppercase() { vec!['-', c.to_ascii_lowercase()] } else { vec![c] }).collect())
    .collect()
}

/// The attributes the markup reader's `fn` named `function` handles, the arms of its attribute match.
fn read_attributes(markup: &str, function: &str, end: &str) -> std::collections::BTreeSet<String> {
  let body = markup.split(&format!("fn {function}(")).nth(1).and_then(|rest| rest.split(end).next()).unwrap_or_else(|| panic!("fn {function} in markup.rs"));
  let arm = regex::Regex::new(r#"^ {6}("[a-z-]+"(?: \| "[a-z-]+")*) =>"#).unwrap();
  body
    .lines()
    .filter_map(|line| arm.captures(line).map(|c| c[1].to_owned()))
    .flat_map(|names| names.split(" | ").map(|name| name.trim_matches('"').to_owned()).collect::<Vec<_>>())
    .filter(|name| name != "ink-ref")
    .collect()
}

#[test]
fn the_box_and_text_props_declared_are_the_ones_the_terminal_reads() {
  let core = include_str!("../embedded/terminal/core.d.ts");
  let markup = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../terminal/src/markup.rs")).unwrap();
  assert_eq!(declared_attributes(core, "BoxProps"), read_attributes(&markup, "boxed", "\nfn kebab("));
  assert_eq!(declared_attributes(core, "TextProps"), read_attributes(&markup, "text", "for child in el.children"));
}
