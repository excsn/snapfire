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

const LOGGING: &str = r#"{ "name": "tool", "type": "module", "dependencies": { "ink": "^5", "react": "^18", "pino": "^9", "winston": "^3" } }"#;

#[test]
fn a_pino_logger_is_the_log_member_with_the_message_first() {
  let t = tree(&[
    ("package.json", LOGGING),
    ("src/cli.tsx", ENTRY),
    ("src/ui.tsx", UI),
    ("src/run.ts", "import pino from 'pino';\n\nconst logger = pino({ level: 'info' });\nconst child = logger.child({ module: 'sync' });\n\nexport function run(id: string, extra: object) {\n  logger.info('starting');\n  logger.info({ id }, 'running');\n  child.warn({ id }, 'slow');\n  child.error('failed');\n  child.info(extra, 'more');\n  logger.debug({ id });\n}\n"),
  ]);
  let (out, report) = migrate(&t);
  assert_eq!(
    out.text("app/src/run.ts").unwrap(),
    "import { log } from \"@snapfire/fsr-client/std\";\n\nexport function run(id: string, extra: object) {\n  log.info('starting');\n  log.info('running', { id });\n  log.warn('slow', { module: 'sync', id });\n  log.error('failed', { module: 'sync' });\n  log.info('more', { module: 'sync', ...extra });\n  log.debug(\"\", { id });\n}\n",
    "{:?}",
    report.notes
  );
  assert_eq!(report.notes.len(), 1, "{:?}", report.notes);
  assert!(report.notes[0].message.contains("pino({ level: 'info' })"), "{:?}", report.notes);
  assert!(!report.packages.contains_key("pino"), "{:?}", report.packages);
}

#[test]
fn a_winston_logger_is_the_log_member_at_the_nearest_level() {
  let t = tree(&[
    ("package.json", LOGGING),
    ("src/cli.tsx", ENTRY),
    ("src/ui.tsx", UI),
    ("src/save.ts", "import winston from 'winston';\n\nconst logger = winston.createLogger({ level: 'info' });\n\nexport const save = (path: string, log: string[]) => {\n  logger.info('saved', { path, n: log.length });\n  logger.log('verbose', 'detail');\n  logger.http('request');\n  logger.silly('noise');\n};\n"),
  ]);
  let (out, report) = migrate(&t);
  assert_eq!(
    out.text("app/src/save.ts").unwrap(),
    "import { log as std_log } from \"@snapfire/fsr-client/std\";\n\nexport const save = (path: string, log: string[]) => {\n  std_log.info('saved', { path, n: log.length });\n  std_log.debug('detail');\n  std_log.debug('request');\n  std_log.trace('noise');\n};\n",
    "{:?}",
    report.notes
  );
  assert_eq!(report.notes.len(), 1, "{:?}", report.notes);
  assert!(!report.packages.contains_key("winston"), "{:?}", report.packages);
}

#[test]
fn a_logger_used_past_its_levels_is_noted_and_left() {
  let source = "import pino from 'pino';\n\nexport const logger = pino();\nlogger.flush();\n";
  let t = tree(&[("package.json", LOGGING), ("src/cli.tsx", ENTRY), ("src/ui.tsx", UI), ("src/shared.ts", source)]);
  let (out, report) = migrate(&t);
  assert_eq!(out.text("app/src/shared.ts").unwrap(), source);
  assert_eq!(report.packages.keys().collect::<Vec<_>>(), ["pino"]);
  assert!(report.notes.iter().any(|n| n.message.contains("export const logger")), "{:?}", report.notes);
  assert!(report.notes.iter().any(|n| n.message.contains("logger.flush")), "{:?}", report.notes);
}
