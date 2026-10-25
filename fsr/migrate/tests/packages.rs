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
    "import { text } from \"@snapfire/fsr/host\";\n\nexport const read = (s: string) => text.frontMatter(s, { delimiters: ['<!--', '-->'] }).data;\nexport const write = (content: string, data: Record<string, unknown>) =>\n  text.withFrontMatter(content, data, { delimiters: ['<!--', '-->'] });\nexport const again = (body: string, data: Record<string, unknown>) => text.withFrontMatter(body.trim(), data);\n",
    "{:?}",
    report.notes
  );
  assert_eq!(out.text("app/src/hash.ts").unwrap(), "import { text } from \"@snapfire/fsr/host\";\nexport async function hash(s: string) {\n  const parsed = text.frontMatter(s, { delimiters: ['<!--', '-->'] });\n  return parsed.content;\n}\n");
  assert_eq!(out.text("app/src/shadow.ts").unwrap(), "import { text as std_text } from \"@snapfire/fsr/host\";\n\nexport const body = (text: string) => std_text.frontMatter(text).content;\n");
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
    "import { log } from \"@snapfire/fsr/host\";\n\nexport function run(id: string, extra: object) {\n  log.info('starting');\n  log.info('running', { id });\n  log.warn('slow', { module: 'sync', id });\n  log.error('failed', { module: 'sync' });\n  log.info('more', { module: 'sync', ...extra });\n  log.debug(\"\", { id });\n}\n",
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
    "import { log as std_log } from \"@snapfire/fsr/host\";\n\nexport const save = (path: string, log: string[]) => {\n  std_log.info('saved', { path, n: log.length });\n  std_log.debug('detail');\n  std_log.debug('request');\n  std_log.trace('noise');\n};\n",
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

#[test]
fn ink_and_the_packages_around_it_are_fsrs_modules() {
  let picker = "import React from 'react';\nimport {Box, Text, useInput} from 'ink';\nimport Spinner from 'ink-spinner';\nimport TextInput, {UncontrolledTextInput} from 'ink-text-input';\nimport spinners from 'cli-spinners';\n\nexport default function Ui() {\n  return <Box><Spinner /><Text>{spinners.dots.interval}</Text></Box>;\n}\n";
  let t = tree(&[("package.json", PACKAGE), ("src/cli.tsx", ENTRY), ("src/ui.tsx", picker)]);
  let (out, _) = migrate(&t);
  assert_eq!(
    out.text("app/src/ui.tsx").unwrap(),
    "import React from 'react';\nimport {Box, Text, useInput} from \"@snapfire/fsr/terminal/ink\";\nimport Spinner from \"@snapfire/fsr/terminal/spinner\";\nimport TextInput, {UncontrolledTextInput} from \"@snapfire/fsr/terminal/text-input\";\nimport spinners from \"@snapfire/fsr/terminal/spinners\";\n\nexport default function Ui() {\n  return <Box><Spinner /><Text>{spinners.dots.interval}</Text></Box>;\n}\n"
  );
}

const FONT: &str = r#"{ "name": "f", "colors": 1, "lines": 1, "buffer": [""], "letterspace": [""], "chars": { "A": ["a"] } }"#;

#[test]
fn ink_big_text_draws_the_cfonts_font_files_the_application_now_ships() {
  let title = "import React from 'react';\nimport {Box} from 'ink';\nimport BigText from 'ink-big-text';\n\nexport default function Title() {\n  return <Box><BigText text=\"hi\" /><BigText text=\"hi\" font=\"simple3d\" colors={['red']} /></Box>;\n}\n";
  let t = tree(&[("package.json", PACKAGE), ("src/cli.tsx", ENTRY), ("src/ui.tsx", UI), ("src/title.tsx", title), (".cfonts/block.json", FONT), (".cfonts/simple3d.json", FONT), (".cfonts/tiny.json", FONT)]);
  let (out, report) = migrate(&t);
  assert_eq!(
    out.text("app/src/title.tsx").unwrap(),
    "import React from 'react';\nimport {Box} from \"@snapfire/fsr/terminal/ink\";\nimport BigText from \"@snapfire/fsr/terminal/big-text\";\nimport fontBlock from \"../fonts/block.json\";\nimport fontSimple3d from \"../fonts/simple3d.json\";\n\nexport default function Title() {\n  return <Box><BigText font={fontBlock} text=\"hi\" /><BigText text=\"hi\" font={fontSimple3d} colors={['red']} /></Box>;\n}\n"
  );
  assert_eq!(out.text("app/fonts/block.json"), Some(FONT));
  assert_eq!(out.text("app/fonts/simple3d.json"), Some(FONT));
  assert!(!out.contains("app/fonts/tiny.json") && out.paths().all(|p| !p.starts_with(".cfonts/")), "{:?}", out.paths().collect::<Vec<_>>());
  assert!(report.notes.iter().any(|n| n.file == "app/fonts/block.json" && n.message.contains("GPL-3.0-or-later")), "{:?}", report.notes);
}

#[test]
fn ink_big_text_with_a_font_chosen_as_it_runs_draws_from_every_cfonts_font() {
  let title = "import React from 'react';\nimport BigText from 'ink-big-text';\n\nexport default function Title({ font }: { font: string }) {\n  return <BigText text=\"hi\" font={font} />;\n}\n";
  let t = tree(&[("package.json", PACKAGE), ("src/cli.tsx", ENTRY), ("src/ui.tsx", UI), ("src/title.tsx", title), (".cfonts/3d.json", FONT), (".cfonts/block.json", FONT)]);
  let (out, _) = migrate(&t);
  assert_eq!(
    out.text("app/src/title.tsx").unwrap(),
    "import React from 'react';\nimport BigText from \"@snapfire/fsr/terminal/big-text\";\nimport cfontsFonts from \"../fonts/cfonts\";\n\nexport default function Title({ font }: { font: string }) {\n  return <BigText text=\"hi\" font={cfontsFonts[font]} />;\n}\n"
  );
  assert_eq!(
    out.text("app/fonts/cfonts.ts").unwrap(),
    "import font3d from \"./3d.json\";\nimport fontBlock from \"./block.json\";\n\nconst fonts: Record<string, unknown> = { \"3d\": font3d, \"block\": fontBlock };\n\nexport default fonts;\n"
  );
  assert!(out.contains("app/fonts/3d.json") && out.contains("app/fonts/block.json"));
}

#[test]
fn ink_big_text_without_cfonts_installed_says_which_font_file_to_supply() {
  let title = "import React from 'react';\nimport BigText from 'ink-big-text';\n\nexport default function Title() {\n  return <BigText text=\"hi\" font=\"chrome\" />;\n}\n";
  let t = tree(&[("package.json", PACKAGE), ("src/cli.tsx", ENTRY), ("src/ui.tsx", UI), ("src/title.tsx", title)]);
  let (out, report) = migrate(&t);
  assert!(out.text("app/src/title.tsx").unwrap().contains("font={fontChrome}"));
  assert!(report.notes.iter().any(|n| n.file == "app/fonts/chrome.json" && n.message.contains("node_modules/cfonts/fonts/chrome.json")), "{:?}", report.notes);
}

#[test]
fn clipboardy_is_the_clipboard_member() {
  let copier = "import clip from 'clipboardy';\n\nexport async function copy(text: string) {\n  clip.writeSync(text);\n  await clip.write(text);\n  const back = await clip.read();\n  return back + clip.readSync();\n}\n\nexport const later = () => clip.read().then((t) => t.trim());\n";
  let t = tree(&[("package.json", PACKAGE), ("src/cli.tsx", ENTRY), ("src/ui.tsx", UI), ("src/copy.ts", copier)]);
  let (out, report) = migrate(&t);
  assert_eq!(
    out.text("app/src/copy.ts").unwrap(),
    "import clip from 'clipboardy';\n\nexport async function copy(text: string) {\n  clip.writeSync(text);\n  await clip.write(text);\n  const back = await clip.read();\n  return back + clip.readSync();\n}\n\nexport const later = () => clip.read().then((t) => t.trim());\n",
    "a file with a use the member does not cover is left whole"
  );
  assert!(report.notes.iter().any(|n| n.to_string() == "app/src/copy.ts:10: `clip.read`: `clipboardy` has no FSR counterpart for this use, so the file still imports it"), "{:?}", report.notes);
  let copier = copier.replace("\nexport const later = () => clip.read().then((t) => t.trim());\n", "");
  let t = tree(&[("package.json", PACKAGE), ("src/cli.tsx", ENTRY), ("src/ui.tsx", UI), ("src/copy.ts", &copier)]);
  let (out, _) = migrate(&t);
  assert_eq!(out.text("app/src/copy.ts").unwrap(), "import { clipboard } from \"@snapfire/fsr/terminal\";\n\nexport async function copy(text: string) {\n  clipboard.write(text);\n  clipboard.write(text);\n  const back = clipboard.read();\n  return back + clipboard.read();\n}\n");
}
