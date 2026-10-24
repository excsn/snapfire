//! Node's modules and its `process` global written as FSR's `file`, `path` and `system` members.

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
fn node_modules_and_process_are_fsr_members() {
  let source = "import fs from 'node:fs/promises';\nimport { existsSync, readFileSync } from 'node:fs';\nimport path from 'node:path';\nimport os, { EOL } from 'node:os';\n\nexport async function load(dir: string) {\n  const at = path.join(os.homedir(), dir, 'config.json');\n  if (!existsSync(at)) return null;\n  const text = readFileSync(at, 'utf8');\n  await fs.writeFile(at + '.bak', text);\n  const s = await fs.stat(at);\n  return [text, EOL, process.env.HOME, process.env[dir], process.platform === 'darwin', process.cwd(), process.memoryUsage().rss, s.isFile()];\n}\n";
  let t = tree(&[("package.json", PACKAGE), ("src/cli.tsx", ENTRY), ("src/ui.tsx", UI), ("src/load.ts", source)]);
  let (out, report) = migrate(&t);
  assert_eq!(
    out.text("app/src/load.ts").unwrap(),
    "import { file, path, system } from \"@snapfire/fsr-client/std\";\n\nexport async function load(dir: string) {\n  const at = path.join(system.home(), dir, 'config.json');\n  if (!file.exists(at)) return null;\n  const text = file.read(at);\n  await file.write(at + '.bak', text);\n  const s = await file.stat(at);\n  return [text, system.lineEnd(), system.env(\"HOME\"), system.env(dir), system.platform() === 'macos', system.cwd(), system.memory(), s.isFile];\n}\n",
    "{:?}",
    report.notes
  );
  assert!(report.notes.is_empty(), "{:?}", report.notes);
}

#[test]
fn a_name_the_file_already_uses_takes_the_member_under_another() {
  let source = "import { readFileSync } from 'fs';\n\nexport const show = (file: string) => readFileSync(file, 'utf8');\n";
  let t = tree(&[("package.json", PACKAGE), ("src/cli.tsx", ENTRY), ("src/ui.tsx", UI), ("src/show.ts", source)]);
  let (out, report) = migrate(&t);
  assert_eq!(out.text("app/src/show.ts").unwrap(), "import { file as std_file } from \"@snapfire/fsr-client/std\";\n\nexport const show = (file: string) => std_file.read(file);\n", "{:?}", report.notes);
}

#[test]
fn a_node_use_with_no_fsr_member_is_noted_and_its_file_left() {
  let source = "import fs from 'node:fs';\n\nexport const open = (p: string) => {\n  fs.chmodSync(p, 0o755);\n  return fs.readFileSync(p, 'utf8');\n};\n";
  let t = tree(&[("package.json", PACKAGE), ("src/cli.tsx", ENTRY), ("src/ui.tsx", UI), ("src/open.ts", source)]);
  let (out, report) = migrate(&t);
  assert_eq!(out.text("app/src/open.ts").unwrap(), source);
  assert!(report.notes.iter().any(|n| n.message.contains("fs.chmodSync")), "{:?}", report.notes);
}

#[test]
fn a_process_use_with_no_fsr_member_is_noted() {
  let source = "export const quit = () => {\n  const dir = process.cwd();\n  process.exit(dir.length);\n};\n";
  let t = tree(&[("package.json", PACKAGE), ("src/cli.tsx", ENTRY), ("src/ui.tsx", UI), ("src/quit.ts", source)]);
  let (out, report) = migrate(&t);
  assert_eq!(out.text("app/src/quit.ts").unwrap(), "import { system } from \"@snapfire/fsr-client/std\";\nexport const quit = () => {\n  const dir = system.cwd();\n  process.exit(dir.length);\n};\n");
  assert!(report.notes.iter().any(|n| n.message.contains("process.exit") && n.message.contains("useApp")), "{:?}", report.notes);
}
