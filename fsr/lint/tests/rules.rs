use std::path::Path;

use snapfire_lint::{Linter, Report};

fn lint(path: &str, source: &str) -> Report {
  let linter = Linter::new_with(snapfire_fsr_lint::base(), snapfire_fsr_lint::checks()).unwrap();
  linter.lint_source(Path::new(path), source)
}

fn fsr(report: &Report) -> Vec<(&str, usize)> {
  report.findings.iter().filter(|f| f.rule.starts_with("fsr/")).map(|f| (f.rule.as_str(), f.line)).collect()
}

#[test]
fn a_browser_global_read_while_rendering_is_found() {
  let cases = [
    ("export default function Page() {\n  const wide = window.innerWidth > 800;\n  return <p>{wide ? 'wide' : 'narrow'}</p>;\n}\n", 2),
    ("export const Size = () => {\n  const [w] = useState(() => window.innerWidth);\n  return <p>{w}</p>;\n};\n", 2),
    ("const theme = localStorage.getItem('theme');\nexport default function Page() {\n  return <p>{theme}</p>;\n}\n", 1),
  ];
  for (source, line) in cases {
    assert_eq!(fsr(&lint("routes/page.tsx", source)), [("fsr/no-browser-globals-in-render", line)], "{source}");
  }
}

#[test]
fn a_browser_global_where_the_browser_runs_it_is_not() {
  let cases = [
    "export default function Page() {\n  useEffect(() => { document.title = 'x'; }, []);\n  return <p />;\n}\n",
    "export default function Page() {\n  return <button onClick={() => alert('hi')}>hi</button>;\n}\n",
    "export default function Page() {\n  const wide = typeof window !== 'undefined' && window.innerWidth > 800;\n  return <p>{String(wide)}</p>;\n}\n",
    "function measure() {\n  return window.innerWidth;\n}\nexport default function Page() {\n  return <p onClick={measure} />;\n}\n",
  ];
  for source in cases {
    assert!(fsr(&lint("routes/page.tsx", source)).is_empty(), "{source}");
  }
  assert!(fsr(&lint("src/main.ts", "window.addEventListener('load', () => {});\n")).is_empty(), "the browser entry runs in the browser");
}

#[test]
fn a_browser_global_in_a_server_module_is_found_anywhere_in_it() {
  let source = "export async function load() {\n  const go = () => window.location.href;\n  return { href: go() };\n}\n";
  assert_eq!(fsr(&lint("routes/page.loader.ts", source)), [("fsr/no-browser-globals-in-render", 2)]);
  assert_eq!(fsr(&lint("routes/actions.ts", "export const save = () => document.cookie;\n")), [("fsr/no-browser-globals-in-render", 1)]);
}

#[test]
fn a_file_read_while_rendering_is_found() {
  let source = "import { file } from \"@snapfire/fsr/host\";\nexport default function Notes() {\n  const text = file.read(\"notes.txt\");\n  return <p>{text}</p>;\n}\n";
  assert_eq!(fsr(&lint("routes/page.tsx", source)), [("fsr/no-file-read-in-render", 3)]);
  let loader = "import { file } from \"@snapfire/fsr/host\";\nexport async function load() {\n  return { text: file.read(\"notes.txt\") };\n}\n";
  assert!(fsr(&lint("routes/page.loader.ts", loader)).is_empty(), "a loader reads once per request");
  let other = "const file = { read: () => '' };\nexport default function Notes() {\n  return <p>{file.read()}</p>;\n}\n";
  assert!(fsr(&lint("routes/page.tsx", other)).is_empty(), "only FSR's file");
}

#[test]
fn two_sources_that_compile_to_one_name_are_found() {
  let dir = tempfile::tempdir().unwrap();
  let ui = dir.path().join("src/ui");
  std::fs::create_dir_all(&ui).unwrap();
  for (name, text) in [("Card.svelte.ts", "export const a = 1;\n"), ("Card.svelte.js", "export const b = 1;\n"), ("format.ts", "export const c = 1;\n"), ("Format.js", "export const d = 1;\n"), ("Pantry.vue", "<template><p /></template>\n"), ("pantry.ts", "export const e = 1;\n"), ("types.d.ts", "export type T = 1;\n"), ("types.ts", "export const f = 1;\n")] {
    std::fs::write(ui.join(name), text).unwrap();
  }
  let linter = Linter::new_with(snapfire_fsr_lint::base(), snapfire_fsr_lint::checks()).unwrap();
  let report = linter.lint_dir(dir.path());
  let mut found: Vec<_> = report.findings.iter().filter(|f| f.rule == "fsr/output-name-clash").map(|f| f.path.file_name().unwrap().to_string_lossy().into_owned()).collect();
  found.sort();
  assert_eq!(found, ["Card.svelte.ts", "format.ts"], "{}", report.render());
}

#[test]
fn htmx_attributes_are_known_properties() {
  let report = lint("routes/page.tsx", "export default function P() {\n  return <div hx-get=\"/x\" hx-swap=\"outerHTML\">x</div>;\n}\n");
  assert!(report.findings.is_empty(), "{}", report.render());
}

#[test]
fn the_rules_are_listed() {
  let linter = Linter::new_with(snapfire_fsr_lint::base(), snapfire_fsr_lint::checks()).unwrap();
  let ids: Vec<_> = linter.rules().into_iter().map(|r| r.id).filter(|id| id.starts_with("fsr/")).collect();
  assert_eq!(ids, ["fsr/no-browser-globals-in-render", "fsr/no-file-read-in-render", "fsr/output-name-clash"]);
}
