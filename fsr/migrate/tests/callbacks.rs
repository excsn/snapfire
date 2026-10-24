//! A callback that changes the object it is handed returns the changed object instead; the function that calls it takes what it returns.

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
const CONFIG: &str = "type Config = { n: number; name: string };\n\nlet config: Config = { n: 0, name: '' };\n\nexport const update = async (fn: (c: Config) => void) => {\n  fn(config);\n  await save(config);\n};\n\nconst save = async (c: Config) => {};\n";

#[test]
fn a_callback_that_writes_fields_of_its_parameter_returns_the_changed_object() {
  let ui = "import React from 'react';\nimport {Text, useInput} from 'ink';\nimport {update} from './config.js';\n\nexport default function Ui() {\n  useInput((input) => {\n    update(cfg => {\n      cfg.n = 1;\n      cfg.name = input;\n    });\n    update((c) => {\n      c.n = 2;\n    });\n  });\n  return <Text>hi</Text>;\n}\n";
  let t = tree(&[("package.json", PACKAGE), ("src/cli.tsx", ENTRY), ("src/ui.tsx", ui), ("src/config.ts", CONFIG)]);
  let (out, report) = migrate(&t);
  assert_eq!(
    out.text("app/src/config.ts").unwrap(),
    "type Config = { n: number; name: string };\n\nlet config: Config = { n: 0, name: '' };\n\nexport const update = async (fn: (c: Config) => Config) => {\n  config = fn(config);\n  await save(config);\n};\n\nconst save = async (c: Config) => {};\n",
    "{:?}",
    report.notes
  );
  let ui = out.text("app/src/ui.tsx").unwrap();
  assert!(ui.contains("    update(cfg => ({ ...cfg, n: 1, name: input }));\n    update((c) => ({ ...c, n: 2 }));\n"), "{ui}");
  assert!(report.notes.iter().all(|n| !n.message.contains("update")), "{:?}", report.notes);
}

#[test]
fn a_callback_that_reads_a_field_it_wrote_leaves_every_call_as_written() {
  let ui = "import React from 'react';\nimport {Text, useInput} from 'ink';\nimport {update} from './config.js';\n\nexport default function Ui() {\n  useInput(() => {\n    update(cfg => {\n      cfg.n = 1;\n    });\n    update(cfg => {\n      cfg.n = 2;\n      cfg.name = String(cfg.n);\n    });\n  });\n  return <Text>hi</Text>;\n}\n";
  let t = tree(&[("package.json", PACKAGE), ("src/cli.tsx", ENTRY), ("src/ui.tsx", ui), ("src/config.ts", CONFIG)]);
  let (out, report) = migrate(&t);
  assert_eq!(out.text("app/src/config.ts").unwrap(), CONFIG);
  assert!(out.text("app/src/ui.tsx").unwrap().contains("update(cfg => {\n      cfg.n = 1;\n    });"));
  assert!(report.notes.iter().any(|n| n.message.contains("cfg.name = String(cfg.n)")), "{:?}", report.notes);
}
