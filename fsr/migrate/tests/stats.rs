//! A `file.stat` result is read as FSR answers it, `isFile` and `isDirectory` as fields.

use snapfire_fsr_migrate::{migrate, Tree};

fn tree(files: &[(&str, &str)]) -> Tree {
  let mut t = Tree::new();
  for (p, s) in files {
    t.insert(*p, *s);
  }
  t
}

const PACKAGE: &str = r#"{ "name": "tool", "type": "module", "dependencies": { "ink": "^5", "react": "^18" } }"#;
const ENTRY: &str = "import React from 'react';\nimport {render} from 'ink';\nimport Ui from './ui.js';\n\nrender(<Ui />);\n";
const UI: &str = "import React from 'react';\nimport {Text} from 'ink';\n\nexport default function Ui() {\n  return <Text>hi</Text>;\n}\n";

#[test]
fn a_stat_method_call_is_read_as_its_field() {
  let t = tree(&[
    ("package.json", PACKAGE),
    ("src/cli.tsx", ENTRY),
    ("src/ui.tsx", UI),
    ("src/files.ts", "import { file } from \"@snapfire/fsr/host\";\n\nexport async function kind(path: string) {\n  const s = await file.stat(path);\n  if (s.isDirectory()) return 'dir';\n  return file.stat(path).isFile() ? 'file' : 'other';\n}\n"),
  ]);
  let (out, report) = migrate(&t);
  assert_eq!(
    out.text("app/src/files.ts").unwrap(),
    "import { file } from \"@snapfire/fsr/host\";\n\nexport async function kind(path: string) {\n  const s = await file.stat(path);\n  if (s.isDirectory) return 'dir';\n  return file.stat(path).isFile ? 'file' : 'other';\n}\n",
    "{:?}",
    report.notes
  );
  assert!(report.notes.is_empty(), "{:?}", report.notes);
}

#[test]
fn a_stat_result_that_leaves_its_function_is_noted() {
  let source = "import { file } from \"@snapfire/fsr/host\";\n\nexport async function read(path: string, use: (s: unknown) => void) {\n  const s = await file.stat(path);\n  use(s);\n  return s.isFile();\n}\n";
  let t = tree(&[("package.json", PACKAGE), ("src/cli.tsx", ENTRY), ("src/ui.tsx", UI), ("src/files.ts", source)]);
  let (out, report) = migrate(&t);
  assert_eq!(out.text("app/src/files.ts").unwrap(), source.replace("s.isFile()", "s.isFile"));
  assert_eq!(report.notes.len(), 1, "{:?}", report.notes);
  assert!(report.notes[0].message.contains("use(s)"), "{:?}", report.notes);
}
