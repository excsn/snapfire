//! Every way TypeScript lets a component reach a library construct, crossed
//! with each construct. A construct the lowerer recognises by one spelling
//! lowers to nothing, or to the wrong thing, for the others, so each spelling
//! must render what the plainest one renders. A name shadowing an import must
//! render as the local it is.

use std::path::PathBuf;
use std::sync::Arc;

use snapfire_fsr_core::ValueMap;
use snapfire_fsr_ir::ast::{Component, Entry, Expr, Tmpl};
use snapfire_fsr_ir::render::Components;
use snapfire_fsr_ir::{Frameworks, Interpreter, Owner, ReactMajor};
use snapfire_fsr_lower::component::ComponentSet;

static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

const PAGE: &str = "routes/a/page.tsx";

/// How a file reaches `export` of `module`: the lines it starts with, what it
/// calls the export and any file the spelling needs beside it.
#[derive(Clone, Copy, Debug)]
enum Spelling {
  Bare,
  Aliased,
  Namespace,
  Reexported,
  ConstAlias,
}

const SPELLINGS: [Spelling; 5] = [Spelling::Bare, Spelling::Aliased, Spelling::Namespace, Spelling::Reexported, Spelling::ConstAlias];

fn spell(spelling: Spelling, module: &str, export: &str) -> (String, String, Vec<(String, String)>) {
  match spelling {
    Spelling::Bare => (format!("import {{ {export} }} from \"{module}\";"), export.to_owned(), Vec::new()),
    Spelling::Aliased => (format!("import {{ {export} as Aliased }} from \"{module}\";"), "Aliased".to_owned(), Vec::new()),
    Spelling::Namespace => (format!("import * as Ns from \"{module}\";"), format!("Ns.{export}"), Vec::new()),
    Spelling::Reexported => (format!("import {{ {export} }} from \"@src/relay\";"), export.to_owned(), vec![("src/relay.ts".to_owned(), format!("export {{ {export} }} from \"{module}\";\n"))]),
    Spelling::ConstAlias => (format!("import {{ {export} }} from \"{module}\";\nconst Aliased = {export};"), "Aliased".to_owned(), Vec::new()),
  }
}

/// A construct: the module and export it is, and the files of an application
/// using it, `HEADER` and `REF` standing for the spelling.
struct Construct {
  name: &'static str,
  module: &'static str,
  export: &'static str,
  /// The file the spelling is written in, and the application's files.
  at: &'static str,
  files: &'static [(&'static str, &'static str)],
}

const THEME: &str = "import { createContext } from \"react\";\nexport const Theme = createContext(\"light\");\n";
const PROVIDING_PAGE: &str = "import { Theme } from \"@src/theme\";\nimport { Label } from \"@src/Label\";\nexport default function A() {\n  return <Theme.Provider value=\"dark\"><Label /></Theme.Provider>;\n}\n";
const STORE: &str = "import { key } from \"@snapfire/fsr-client/store\";\nexport const count = key<number>(\"c\");\n";
const CLOCK: &str = "import { useState } from \"react\";\nexport function Clock() {\n  const [n, setN] = useState(7);\n  return <button onClick={() => setN(n + 1)}>{n}</button>;\n}\n";

const CONSTRUCTS: &[Construct] = &[
  Construct { name: "useState", module: "react", export: "useState", at: PAGE, files: &[(PAGE, "HEADER\nexport default function A() {\n  const [n, setN] = REF(2);\n  return <button onClick={() => setN(n + 1)}>{n}</button>;\n}\n")] },
  Construct { name: "useMemo", module: "react", export: "useMemo", at: PAGE, files: &[(PAGE, "HEADER\nexport default function A() {\n  const v = REF(() => 3, []);\n  return <p>{v}</p>;\n}\n")] },
  Construct { name: "useRef", module: "react", export: "useRef", at: PAGE, files: &[(PAGE, "HEADER\nexport default function A() {\n  const r = REF(5);\n  return <p>{r.current}</p>;\n}\n")] },
  Construct { name: "useContext", module: "react", export: "useContext", at: "src/Label.tsx", files: &[("src/theme.ts", THEME), ("src/Label.tsx", "HEADER\nimport { Theme } from \"./theme\";\nexport function Label() {\n  return <i>{REF(Theme)}</i>;\n}\n"), (PAGE, PROVIDING_PAGE)] },
  Construct { name: "use", module: "react", export: "use", at: "src/Label.tsx", files: &[("src/theme.ts", THEME), ("src/Label.tsx", "HEADER\nimport { Theme } from \"./theme\";\nexport function Label() {\n  return <i>{REF(Theme)}</i>;\n}\n"), (PAGE, PROVIDING_PAGE)] },
  Construct { name: "createContext", module: "react", export: "createContext", at: "src/theme.ts", files: &[("src/theme.ts", "HEADER\nexport const Theme = REF(\"light\");\n"), ("src/Label.tsx", "import { useContext } from \"react\";\nimport { Theme } from \"./theme\";\nexport function Label() {\n  return <i>{useContext(Theme)}</i>;\n}\n"), (PAGE, PROVIDING_PAGE)] },
  Construct { name: "useStore", module: "@snapfire/fsr-client/react", export: "useStore", at: PAGE, files: &[("src/store.ts", STORE), (PAGE, "HEADER\nimport { count } from \"@src/store\";\nexport default function A() {\n  const [c, setC] = REF(count, 4);\n  return <button onClick={() => setC(c + 1)}>{c}</button>;\n}\n")] },
  Construct { name: "store key", module: "@snapfire/fsr-client/store", export: "key", at: "src/store.ts", files: &[("src/store.ts", "HEADER\nexport const count = REF<number>(\"c\");\n"), (PAGE, "import { useStore } from \"@snapfire/fsr-client/react\";\nimport { count } from \"@src/store\";\nexport default function A() {\n  const [c, setC] = useStore(count, 4);\n  return <button onClick={() => setC(c + 1)}>{c}</button>;\n}\n")] },
  Construct { name: "Link", module: "@snapfire/fsr-authoring/template", export: "Link", at: PAGE, files: &[(PAGE, "HEADER\nexport default function A() {\n  return <REF href=\"/x\">x</REF>;\n}\n")] },
  Construct { name: "Island", module: "@snapfire/fsr-client/react", export: "Island", at: PAGE, files: &[("src/Clock.tsx", CLOCK), (PAGE, "HEADER\nimport { Clock } from \"@src/Clock\";\nexport default function A() {\n  return <main><REF><Clock /></REF></main>;\n}\n")] },
  Construct { name: "template createContext", module: "@snapfire/fsr-authoring/template", export: "createContext", at: "src/theme.ts", files: &[("src/theme.ts", "HEADER\nexport const Theme = REF(\"light\");\n"), ("src/Label.tsx", "import { use } from \"@snapfire/fsr-authoring/template\";\nimport { Theme } from \"./theme\";\nexport function Label() {\n  return <i>{use(Theme)}</i>;\n}\n"), (PAGE, PROVIDING_PAGE)] },
  Construct { name: "template use", module: "@snapfire/fsr-authoring/template", export: "use", at: "src/Label.tsx", files: &[("src/theme.ts", "import { createContext } from \"@snapfire/fsr-authoring/template\";\nexport const Theme = createContext(\"light\");\n"), ("src/Label.tsx", "HEADER\nimport { Theme } from \"./theme\";\nexport function Label() {\n  return <i>{REF(Theme)}</i>;\n}\n"), (PAGE, PROVIDING_PAGE)] },
];

/// What lowering and rendering the page gave: its markup, or how it failed.
#[derive(Debug, Clone, PartialEq)]
enum Outcome {
  Rendered(String),
  Browser(String),
  Refused(String),
}

fn app(files: &[(String, String)]) -> PathBuf {
  let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let dir = std::env::temp_dir().join(format!("fsr-spellings-{}-{n}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  for (name, source) in files {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, source).unwrap();
  }
  dir
}

fn host(module: &str) -> Component {
  Component { body: Vec::new(), render: Tmpl::Island { module: module.to_owned(), props: vec![Entry::Spread(Expr::var("$props"))], children: Vec::new(), when: None, mode: None, id: 0, define: false }, state: Vec::new(), stores: Vec::new(), handlers: Vec::new(), owner: Owner::Fsr, shadow: None, scope: None }
}

fn outcome(files: &[(String, String)]) -> Outcome {
  let dir = app(files);
  let module = format!("{PAGE}#default");
  let mut set = ComponentSet::new(&dir);
  let lowered = set.lower(&module);
  let _ = std::fs::remove_dir_all(&dir);
  if let Err(e) = lowered {
    return Outcome::Refused(e.to_string());
  }
  if let Some((module, residue)) = set.browser_only.first() {
    return Outcome::Browser(format!("{module}: {}", residue.message));
  }
  let library: Components = set.components.iter().map(|(m, c)| (m.clone(), Arc::new(snapfire_fsr_ir::render::prepare(c)))).collect();
  let interpreter = Interpreter::default().with_frameworks(Frameworks { react: Some(ReactMajor::V19), vue: None });
  match interpreter.render(&host(&module), &ValueMap::default(), &library) {
    Ok(rendered) => Outcome::Rendered(rendered.islands.first().map(|island| island.body.html.clone()).unwrap_or_default()),
    Err(e) => Outcome::Refused(format!("render: {e:?}")),
  }
}

fn filled(construct: &Construct, spelling: Spelling) -> Vec<(String, String)> {
  let (header, reference, extra) = spell(spelling, construct.module, construct.export);
  let mut files: Vec<(String, String)> = construct.files.iter().map(|(name, source)| ((*name).to_owned(), if *name == construct.at { source.replace("HEADER", &header).replace("REF", &reference) } else { (*source).to_owned() })).collect();
  files.extend(extra);
  files
}

fn short(outcome: &Outcome) -> String {
  let text = match outcome {
    Outcome::Rendered(html) => format!("rendered {html}"),
    Outcome::Browser(why) => format!("browser: {why}"),
    Outcome::Refused(why) => format!("refused: {why}"),
  };
  text.chars().take(220).collect()
}

#[test]
fn every_spelling_of_a_construct_renders_what_the_plainest_renders() {
  let mut differ = Vec::new();
  let mut rows = 0;
  for construct in CONSTRUCTS {
    let reference = outcome(&filled(construct, Spelling::Bare));
    if !matches!(reference, Outcome::Rendered(_)) {
      differ.push(format!("{} / Bare (the reference): {}", construct.name, short(&reference)));
      continue;
    }
    for spelling in SPELLINGS.iter().skip(1) {
      rows += 1;
      let got = outcome(&filled(construct, *spelling));
      if got != reference {
        differ.push(format!("{} / {spelling:?}: {} (Bare {})", construct.name, short(&got), short(&reference)));
      }
    }
  }
  assert!(differ.is_empty(), "{} of {rows} spellings:\n{}", differ.len(), differ.join("\n"));
}

/// A provider written each way React and TypeScript allow, against `<Theme.Provider>`.
#[test]
fn every_spelling_of_a_provider_renders_what_the_plainest_renders() {
  let label = "import { useContext } from \"react\";\nimport { Theme } from \"./theme\";\nexport function Label() {\n  return <i>{useContext(Theme)}</i>;\n}\n";
  let page = |header: &str, open: &str, close: &str| format!("{header}\nimport {{ Label }} from \"@src/Label\";\nexport default function A() {{\n  return {open}<Label />{close};\n}}\n");
  let cases = [
    ("X.Provider", page("import { Theme } from \"@src/theme\";", "<Theme.Provider value=\"dark\">", "</Theme.Provider>")),
    ("X (React 19)", page("import { Theme } from \"@src/theme\";", "<Theme value=\"dark\">", "</Theme>")),
    ("const P = X.Provider", page("import { Theme } from \"@src/theme\";\nconst P = Theme.Provider;", "<P value=\"dark\">", "</P>")),
    ("Ns.X.Provider", page("import * as T from \"@src/theme\";", "<T.Theme.Provider value=\"dark\">", "</T.Theme.Provider>")),
    ("Ns.X", page("import * as T from \"@src/theme\";", "<T.Theme value=\"dark\">", "</T.Theme>")),
  ];
  let run = |source: &str| outcome(&[("src/theme.ts".to_owned(), THEME.to_owned()), ("src/Label.tsx".to_owned(), label.to_owned()), (PAGE.to_owned(), source.to_owned())]);
  let reference = run(&cases[0].1);
  assert!(matches!(reference, Outcome::Rendered(ref html) if html.contains("dark")), "{reference:?}");
  let differ: Vec<String> = cases.iter().skip(1).filter_map(|(what, source)| Some(run(source)).filter(|got| *got != reference).map(|got| format!("{what}: {}", short(&got)))).collect();
  assert!(differ.is_empty(), "{} of {}:\n{}", differ.len(), cases.len() - 1, differ.join("\n"));
}

/// A local binding with an imported name is the local, wherever it shadows the
/// import: it lowers exactly as the same local under a name no import has.
#[test]
fn a_local_that_shadows_an_import_lowers_as_the_local_it_is() {
  let cases = [
    ("use", "import { use } from \"react\";\nexport default function A() {\n  const use = (v: number) => v * 2;\n  return <p>{use(2)}</p>;\n}\n", "import { use } from \"react\";\nexport default function A() {\n  const twice = (v: number) => v * 2;\n  return <p>{twice(2)}</p>;\n}\n"),
    ("useState", "import { useState } from \"react\";\nexport default function A() {\n  const useState = (v: number) => [v + 1];\n  const [n] = useState(2);\n  return <p>{n}</p>;\n}\n", "import { useState } from \"react\";\nexport default function A() {\n  const twice = (v: number) => [v + 1];\n  const [n] = twice(2);\n  return <p>{n}</p>;\n}\n"),
    ("key", "import { key } from \"@snapfire/fsr-client/store\";\nexport const k = key<number>(\"k\");\nexport default function A() {\n  return <ul>{[\"a\", \"b\"].map((key) => <li key={key}>{key}</li>)}</ul>;\n}\n", "import { key } from \"@snapfire/fsr-client/store\";\nexport const k = key<number>(\"k\");\nexport default function A() {\n  return <ul>{[\"a\", \"b\"].map((twice) => <li key={twice}>{twice}</li>)}</ul>;\n}\n"),
    ("Link", "import { Link } from \"@snapfire/fsr-authoring/template\";\nfunction Row({ Link }: { Link: string }) {\n  return <p>{Link}</p>;\n}\nexport default function A() {\n  return <Row Link=\"here\" />;\n}\n", "import { Link } from \"@snapfire/fsr-authoring/template\";\nfunction Row({ twice }: { twice: string }) {\n  return <p>{twice}</p>;\n}\nexport default function A() {\n  return <Row twice=\"here\" />;\n}\n"),
    ("Island", "import { Island } from \"@snapfire/fsr-client/react\";\nexport default function A() {\n  const Island = ({ children }: { children: unknown }) => <section>{children}</section>;\n  return <Island><b>x</b></Island>;\n}\n", "import { Island } from \"@snapfire/fsr-client/react\";\nexport default function A() {\n  const Twice = ({ children }: { children: unknown }) => <section>{children}</section>;\n  return <Twice><b>x</b></Twice>;\n}\n"),
  ];
  let differ: Vec<String> = cases
    .iter()
    .filter_map(|(name, source, renamed)| {
      let normal = |o: Outcome, local: &str| match o {
        Outcome::Refused(why) => Outcome::Refused(why.replace(local, "LOCAL")),
        Outcome::Browser(why) => Outcome::Browser(why.replace(local, "LOCAL")),
        rendered => rendered,
      };
      let got = normal(outcome(&[(PAGE.to_owned(), (*source).to_owned())]), name);
      let twin = if *name == "Island" { "Twice" } else { "twice" };
      let reference = normal(outcome(&[(PAGE.to_owned(), (*renamed).to_owned())]), twin);
      (got != reference).then(|| format!("a local `{name}`: {} (renamed {})", short(&got), short(&reference)))
    })
    .collect();
  assert!(differ.is_empty(), "{} of {}:\n{}", differ.len(), cases.len(), differ.join("\n"));
}
