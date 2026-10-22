//! An Ink application migrates into FSR's terminal layout: its component
//! files under `app/src/`, a `routes/page.tsx` that renders the component the
//! entry rendered and every import written the way an FSR application writes
//! it.

use snapfire_fsr_migrate::{detect, migrate, Source, Tree};

fn tree(files: &[(&str, &str)]) -> Tree {
  let mut t = Tree::new();
  for (p, s) in files {
    t.insert(*p, *s);
  }
  t
}

const PACKAGE: &str = r#"{ "name": "emoj", "type": "module", "dependencies": { "ink": "^5", "react": "^18", "meow": "^13", "ink-spinner": "^5", "conf": "^12" } }"#;

#[test]
fn an_application_depending_on_ink_is_an_ink_application() {
  let t = tree(&[("package.json", PACKAGE), ("source/cli.tsx", "")]);
  assert_eq!(detect(&t), Some(Source::Ink));
  assert_eq!(Source::Ink.direction(), "react");
}

#[test]
fn the_entry_that_renders_becomes_a_page_rendering_the_same_component() {
  let t = tree(&[
    ("package.json", PACKAGE),
    ("source/cli.tsx", "#!/usr/bin/env node\nimport React from 'react';\nimport {render} from 'ink';\nimport Ui from './ui.js';\n\nrender(<Ui />);\n"),
    ("source/ui.tsx", "import React from 'react';\nimport {Text} from 'ink';\nimport Conf from 'conf';\nimport {search} from './index.js';\n\nexport default function Ui() {\n  return <Text>{search('a')}</Text>;\n}\n"),
    ("source/index.ts", "export const search = (q: string) => q;\n"),
    ("readme.md", "# emoj\n"),
    ("test.ts", "import test from 'ava';\n"),
  ]);
  let (out, report) = migrate(&t);
  let mut paths: Vec<&str> = out.paths().collect();
  paths.sort();
  assert_eq!(paths, ["app/routes/page.tsx", "app/src/index.ts", "app/src/ui.tsx", "app/tests/test.ts"], "{:?}", report.notes);
  assert_eq!(out.text("app/routes/page.tsx").unwrap(), "import Ui from \"@src/ui\";\n\nexport default function Page() {\n  return <Ui />;\n}\n");
  assert_eq!(out.text("app/src/ui.tsx").unwrap(), "import React from 'react';\nimport {Text} from 'ink';\nimport Conf from 'conf';\nimport {search} from './index.js';\n\nexport default function Ui() {\n  return <Text>{search('a')}</Text>;\n}\n");
  assert_eq!(report.packages.keys().collect::<Vec<_>>(), ["conf"], "ink, react, the packages FSR provides and what only the entry imported are not carried");
  assert!(report.notes.is_empty(), "{:?}", report.notes);
}

#[test]
fn an_alias_from_tsconfig_paths_is_written_as_the_src_alias() {
  let t = tree(&[
    ("package.json", PACKAGE),
    ("tsconfig.json", "{ \"compilerOptions\": { \"baseUrl\": \"./src\", \"paths\": { \"~/*\": [\"./*\"] } } }"),
    ("src/index.ts", "import { render } from 'ink';\nimport { App } from '~/app';\nimport { init } from '~/core/init';\n\ninit();\nrender(App(), { exitOnCtrlC: true });\n"),
    ("src/app.tsx", "import React from 'react';\nimport { Board } from '~/components/board';\nimport { solve } from '~/core/solve';\n\nexport function App() {\n  return <Board cells={solve()} />;\n}\n"),
    ("src/components/board.tsx", "import React from 'react';\nimport { CELLS } from '~/constants';\n\nexport function Board({ cells }: { cells: number[] }) {\n  return null;\n}\n"),
    ("src/core/solve.ts", "export const solve = () => [1];\n"),
    ("src/core/init.ts", "export const init = () => {};\n"),
    ("src/constants.ts", "export const CELLS = 81;\n"),
    ("src/types.d.ts", "import type { Board } from '~/components/board';\n\nexport type Shown = Board;\n"),
  ]);
  let (out, report) = migrate(&t);
  assert_eq!(out.text("app/src/types.d.ts").unwrap(), "import type { Board } from '@src/components/board';\n\nexport type Shown = Board;\n");
  assert_eq!(out.text("app/src/app.tsx").unwrap(), "import React from 'react';\nimport { Board } from '@src/components/board';\nimport { solve } from '@src/core/solve';\n\nexport function App() {\n  return <Board cells={solve()} />;\n}\n", "{:?}", report.notes);
  assert_eq!(out.text("app/src/components/board.tsx").unwrap(), "import React from 'react';\nimport { CELLS } from '@src/constants';\n\nexport function Board({ cells }: { cells: number[] }) {\n  return null;\n}\n");
  assert_eq!(out.text("app/routes/page.tsx").unwrap(), "import { App } from \"@src/app\";\n\nexport default function Page() {\n  return <App />;\n}\n");
  assert!(!out.contains("app/src/index.ts") && !out.contains("tsconfig.json"));
}

#[test]
fn what_the_entry_passed_the_component_is_a_note() {
  let t = tree(&[
    ("package.json", PACKAGE),
    ("source/cli.tsx", "import React from 'react';\nimport {render} from 'ink';\nimport meow from 'meow';\nimport Ui from './ui.js';\n\nconst cli = meow('', {importMeta: import.meta, flags: {limit: {type: 'number'}}});\n\nrender(<Ui limit={cli.flags.limit ?? 7} skinNumber={0} />);\n"),
    ("source/ui.tsx", "export default function Ui({ limit, skinNumber }: { limit: number; skinNumber: number }) {\n  return null;\n}\n"),
  ]);
  let (out, report) = migrate(&t);
  assert_eq!(out.text("app/routes/page.tsx").unwrap(), "import Ui from \"@src/ui\";\n\nexport default function Page() {\n  return <Ui />;\n}\n");
  assert_eq!(report.notes.iter().map(ToString::to_string).collect::<Vec<_>>(), ["source/cli.tsx:8: `render` passed `Ui` the props `limit` and `skinNumber`, read from the command line; `routes/page.tsx` renders it without them"]);
}

#[test]
fn an_entry_that_declares_the_component_it_renders_is_kept_and_exports_it() {
  let t = tree(&[
    ("package.json", PACKAGE),
    ("lrn.tsx", "import React, { useState } from 'react';\nimport { render, Text, useInput } from 'ink';\nimport { readFileSync } from 'node:fs';\n\nconst cards = readFileSync('cards.txt', 'utf8');\n\nfunction AppMatch() {\n  const [n] = useState(0);\n  return <Text>{cards} {n}</Text>;\n}\n\nrender(<AppMatch />);\n"),
  ]);
  let (out, report) = migrate(&t);
  assert_eq!(out.text("app/src/lrn.tsx").unwrap(), "import React, { useState } from 'react';\nimport { Text, useInput } from 'ink';\nimport { readFileSync } from 'node:fs';\n\nconst cards = readFileSync('cards.txt', 'utf8');\n\nexport function AppMatch() {\n  const [n] = useState(0);\n  return <Text>{cards} {n}</Text>;\n}\n", "{:?}", report.notes);
  assert_eq!(out.text("app/routes/page.tsx").unwrap(), "import { AppMatch } from \"@src/lrn\";\n\nexport default function Page() {\n  return <AppMatch />;\n}\n");
  assert!(report.notes.is_empty(), "{:?}", report.notes);
}
