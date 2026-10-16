use std::path::PathBuf;

use snapfire_fsr_lower::component::ComponentSet;

fn app(tag: &str, files: &[(&str, &[u8])]) -> PathBuf {
  let dir = std::env::temp_dir().join(format!("fsr_imports_{}_{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  for (name, bytes) in files {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
  }
  dir
}

#[test]
fn the_reach_follows_value_imports_and_re_exports_to_scripts_only() {
  let dir = app(
    "reach",
    &[
      ("src/ui/Card.tsx", b"import type { Shape } from \"./shape\";\nimport { Label } from \"./parts\";\nimport logo from \"./logo.png\";\nexport function Card(s: Shape) {\n  return <Label text={logo} />;\n}\n"),
      ("src/ui/shape.ts", b"export type Shape = { n: number };\n"),
      ("src/ui/parts.ts", b"export { Label } from \"./Label\";\nexport * from \"./more\";\n"),
      ("src/ui/Label.tsx", b"export function Label({ text }: { text: string }) {\n  return <b>{text}</b>;\n}\n"),
      ("src/ui/more.ts", b"export const unit = \"px\";\n"),
      ("src/ui/logo.png", &[0x89, 0x50, 0x4e, 0x47, 0xff, 0xfe]),
    ],
  );
  let mut set = ComponentSet::new(&dir);
  assert_eq!(set.local_reach("src/ui/Card.tsx"), ["src/ui/Card.tsx", "src/ui/parts.ts", "src/ui/Label.tsx", "src/ui/more.ts"], "a type-only import and an asset are not followed");
  std::fs::remove_dir_all(&dir).unwrap();
}

/// TypeScript parses JSX in every file but a `.ts`, where `<T>x` is an
/// assertion; react.dev and Create React App write components in `.js`.
#[test]
fn a_component_in_a_js_or_jsx_file_lowers() {
  let dir = app(
    "jsx_in_js",
    &[
      ("routes/a/page.tsx", b"import App from \"@src/App.js\";\nexport default function Page() {\n  return <App />;\n}\n"),
      ("src/App.js", b"import { Title } from './Title.jsx';\nexport default function App() {\n  return <main><Title text=\"hi\" /></main>;\n}\n"),
      ("src/Title.jsx", b"export function Title({ text }) {\n  return <h1>{text}</h1>;\n}\n"),
    ],
  );
  let mut set = ComponentSet::new(&dir);
  set.lower("routes/a/page.tsx#default").unwrap();
  assert!(set.components.iter().any(|(m, _)| m == "src/App.js#default"), "{:?}", set.components.iter().map(|(m, _)| m).collect::<Vec<_>>());
  assert!(set.components.iter().any(|(m, _)| m == "src/Title.jsx#Title"));
  std::fs::remove_dir_all(&dir).unwrap();
}

/// A JavaScript project names `./Title` for `Title.jsx` and `./util.js` for
/// `util.jsx`, which TypeScript resolves with `allowJs`.
#[test]
fn an_import_resolves_to_a_js_or_jsx_file() {
  let dir = app(
    "resolve_js",
    &[
      ("src/Card.jsx", b"import { Title } from './Title';\nimport { shout } from './util.js';\nimport { Badge } from './badge';\nexport function Card() {\n  return <Title text={shout('hi')} />;\n}\n"),
      ("src/Title.jsx", b"export function Title({ text }) {\n  return <h1>{text}</h1>;\n}\n"),
      ("src/util.jsx", b"export function shout(s) {\n  return s.toUpperCase();\n}\n"),
      ("src/badge/index.js", b"export function Badge() {\n  return null;\n}\n"),
    ],
  );
  let mut set = ComponentSet::new(&dir);
  assert_eq!(set.local_reach("src/Card.jsx"), ["src/Card.jsx", "src/Title.jsx", "src/util.jsx", "src/badge/index.js"]);
  std::fs::remove_dir_all(&dir).unwrap();
}
