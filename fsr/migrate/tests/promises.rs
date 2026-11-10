//! A promise chain in an async function is written with `await`, each callback bound to a name the build calls.

use snapfire_fsr_migrate::{migrate, Tree};

fn tree(files: &[(&str, &str)]) -> Tree {
  let mut t = Tree::new();
  for (p, s) in files {
    t.insert(*p, *s);
  }
  t
}

const PACKAGE: &str = r#"{ "name": "tool", "type": "module", "dependencies": { "ink": "^5", "react": "^18" } }"#;
const ENTRY: &str = "import React from 'react';\nimport {render, Text} from 'ink';\n\nrender(<Text>hi</Text>);\n";

fn migrated(data: &str) -> (String, Vec<String>) {
  let t = tree(&[("package.json", PACKAGE), ("src/cli.tsx", ENTRY), ("src/data.ts", data)]);
  let (out, report) = migrate(&t);
  (out.text("app/src/data.ts").unwrap().to_owned(), report.notes.iter().map(|n| format!("{}:{}: {}", n.file, n.line, n.message)).collect())
}

#[test]
fn then_calls_its_callback_with_the_awaited_value() {
  let (out, notes) = migrated("export async function load(read) {\n  const text = await read().then((r) => r.trim()).then(parse);\n  return text;\n}\nconst parse = (s) => s.length;\n");
  assert_eq!(out, "export async function load(read) {\n  const $then1 = (r) => r.trim();\n  const text = await parse(await $then1(await read()));\n  return text;\n}\nconst parse = (s) => s.length;\n", "{notes:?}");
  assert!(notes.is_empty(), "{notes:?}");
}

#[test]
fn catch_and_finally_become_a_try() {
  let (out, notes) = migrated("export async function load(read, done) {\n  let n = await read().then(count).catch((e) => 0).finally(done);\n  return n;\n}\nconst count = (s) => s.length;\n");
  assert_eq!(
    out,
    "export async function load(read, done) {\n  let $settled1;\n  try {\n    let $settled2;\n    const $then3 = (e) => 0;\n    try {\n      $settled2 = await count(await read());\n    } catch ($caught4) {\n      $settled2 = await $then3($caught4);\n    }\n    $settled1 = $settled2;\n  } finally {\n    await done();\n  }\n  let n = $settled1;\n  return n;\n}\nconst count = (s) => s.length;\n",
    "{notes:?}"
  );
  assert!(notes.is_empty(), "{notes:?}");
}

#[test]
fn a_returned_chain_and_an_awaited_one_are_written_too() {
  let (out, notes) = migrated("export async function load(read, log) {\n  await read().then(log);\n  return read().then(String);\n}\n");
  assert_eq!(out, "export async function load(read, log) {\n  await log(await read());\n  return await String(await read());\n}\n", "{notes:?}");
  assert!(notes.is_empty(), "{notes:?}");
}

#[test]
fn a_chain_the_pass_cannot_write_is_left_with_a_note() {
  let source = "export function load(read) {\n  return read().then(String);\n}\nexport async function both(read) {\n  const v = await read().then(String, () => \"\");\n  read().then(String);\n  return v;\n}\n";
  let (out, notes) = migrated(source);
  assert_eq!(out, source);
  assert_eq!(notes.len(), 3, "{notes:?}");
  assert!(notes[0].contains("outside an async function"), "{notes:?}");
  assert!(notes[1].contains("rejection handler"), "{notes:?}");
  assert!(notes[2].contains("not awaited or returned whole"), "{notes:?}");
}
