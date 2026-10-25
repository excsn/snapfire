//! The CommonJS a migration refuses: the files it would rewrite, followed from where each framework keeps them.

use snapfire_fsr_migrate::{migrate, Tree};

fn tree(files: &[(&str, &str)]) -> Tree {
  let mut t = Tree::new();
  for (p, s) in files {
    t.insert(*p, *s);
  }
  t
}

#[test]
fn a_next_app_with_a_commonjs_config_and_server_entry_is_migrated() {
  let t = tree(&[
    ("package.json", r#"{ "main": "firebaseFunctions.js", "dependencies": { "next": "15", "react": "19" } }"#),
    ("firebaseFunctions.js", "const { https } = require('firebase-functions');\nexports.nextjs = https.onRequest(() => {});\n"),
    ("src/next.config.js", "module.exports = { distDir: '../.next' };\n"),
    ("src/pages/index.jsx", "import Header from '../components/Header';\n\nexport default function Index() {\n  return <Header />;\n}\n"),
    ("src/components/Header.jsx", "export default function Header() {\n  return <h1>hi</h1>;\n}\n"),
  ]);
  let (_, report) = migrate(&t);
  assert!(report.refusals.is_empty(), "configuration and a server entry are not what the migration rewrites: {:?}", report.refusals);
  assert!(!report.passes.is_empty());
}

#[test]
fn a_next_page_reaching_a_commonjs_module_is_refused_naming_it() {
  let t = tree(&[
    ("package.json", r#"{ "dependencies": { "next": "15", "react": "19" } }"#),
    ("pages/index.jsx", "import format from '../lib/format';\n\nexport default function Index() {\n  return <p>{format(1)}</p>;\n}\n"),
    ("lib/format.js", "module.exports = (n) => String(n);\n"),
  ]);
  let (_, report) = migrate(&t);
  let refused: Vec<String> = report.refusals.iter().map(ToString::to_string).collect();
  assert_eq!(refused, ["lib/format.js:1: `module.exports =`, which is CommonJS; fsr migrate reads ES modules, `import` and `export`"]);
}
