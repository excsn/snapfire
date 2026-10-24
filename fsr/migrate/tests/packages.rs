//! Packages FSR has a standard member for are written as that member wherever an application imports them.

use snapfire_fsr_migrate::{migrate, Tree};

fn tree(files: &[(&str, &str)]) -> Tree {
  let mut t = Tree::new();
  for (p, s) in files {
    t.insert(*p, *s);
  }
  t
}

const PACKAGE: &str = r#"{ "name": "prompts", "type": "module", "dependencies": { "ink": "^5", "react": "^18", "gray-matter": "^4" } }"#;
const ENTRY: &str = "import React from 'react';\nimport {render} from 'ink';\nimport Ui from './ui.js';\n\nrender(<Ui />);\n";
const UI: &str = "import React from 'react';\nimport {Text} from 'ink';\n\nexport default function Ui() {\n  return <Text>hi</Text>;\n}\n";

#[test]
fn gray_matter_is_the_front_matter_members() {
  let t = tree(&[
    ("package.json", PACKAGE),
    ("src/cli.tsx", ENTRY),
    ("src/ui.tsx", UI),
    ("src/sync.ts", "import matter from 'gray-matter';\n\nexport const read = (s: string) => matter(s, { delimiters: ['<!--', '-->'] }).data;\nexport const write = (content: string, data: Record<string, unknown>) =>\n  matter.stringify({ content }, data, { delimiters: ['<!--', '-->'] });\nexport const again = (body: string, data: Record<string, unknown>) => matter.stringify({ content: body.trim() }, data);\n"),
    ("src/hash.ts", "export async function hash(s: string) {\n  const matter = await import('gray-matter');\n  const parsed = matter.default(s, { delimiters: ['<!--', '-->'] });\n  return parsed.content;\n}\n"),
    ("src/shadow.ts", "import matter from 'gray-matter';\n\nexport const body = (text: string) => matter(text).content;\n"),
  ]);
  let (out, report) = migrate(&t);
  assert_eq!(
    out.text("app/src/sync.ts").unwrap(),
    "import { text } from \"@snapfire/fsr-client/std\";\n\nexport const read = (s: string) => text.frontMatter(s, { delimiters: ['<!--', '-->'] }).data;\nexport const write = (content: string, data: Record<string, unknown>) =>\n  text.withFrontMatter(content, data, { delimiters: ['<!--', '-->'] });\nexport const again = (body: string, data: Record<string, unknown>) => text.withFrontMatter(body.trim(), data);\n",
    "{:?}",
    report.notes
  );
  assert_eq!(out.text("app/src/hash.ts").unwrap(), "import { text } from \"@snapfire/fsr-client/std\";\nexport async function hash(s: string) {\n  const parsed = text.frontMatter(s, { delimiters: ['<!--', '-->'] });\n  return parsed.content;\n}\n");
  assert_eq!(out.text("app/src/shadow.ts").unwrap(), "import { text as std_text } from \"@snapfire/fsr-client/std\";\n\nexport const body = (text: string) => std_text.frontMatter(text).content;\n");
  assert!(report.packages.is_empty(), "{:?}", report.packages);
  assert!(report.notes.is_empty(), "{:?}", report.notes);
}

#[test]
fn a_gray_matter_use_with_no_member_is_noted_and_left() {
  let t = tree(&[
    ("package.json", PACKAGE),
    ("src/cli.tsx", ENTRY),
    ("src/ui.tsx", UI),
    ("src/file.ts", "import matter from 'gray-matter';\n\nexport const read = (path: string) => matter.read(path);\n"),
  ]);
  let (out, report) = migrate(&t);
  assert_eq!(out.text("app/src/file.ts").unwrap(), "import matter from 'gray-matter';\n\nexport const read = (path: string) => matter.read(path);\n");
  assert_eq!(report.packages.keys().collect::<Vec<_>>(), ["gray-matter"]);
  assert_eq!(report.notes.len(), 1, "{:?}", report.notes);
  assert!(report.notes[0].message.contains("matter.read"), "{:?}", report.notes);
}
