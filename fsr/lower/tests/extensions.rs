use std::path::PathBuf;

use snapfire_fsr_ir::{Expr, Reach, Stmt, Tmpl};
use snapfire_fsr_lower::component::ComponentSet;
use snapfire_fsr_lower::LowerError;

fn app(tag: &str, files: &[(&str, &str)]) -> PathBuf {
  let dir = std::env::temp_dir().join(format!("fsr_extensions_{}_{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  for (name, source) in files {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, source).unwrap();
  }
  dir
}

fn exts(expr: &Expr) -> Vec<String> {
  let mut out = Vec::new();
  expr.visit(&mut |e| {
    if let Expr::Ext { module, name, args } = e {
      out.push(format!("{module}.{name}/{}", args.len()));
    }
  });
  out
}

fn tree_exts(tmpl: &Tmpl) -> Vec<String> {
  let mut out = Vec::new();
  tmpl.visit(&mut |e| {
    if let Expr::Ext { module, name, args } = e {
      out.push(format!("{module}.{name}/{}", args.len()));
    }
  });
  out
}

#[test]
fn a_standard_member_lowers_to_an_extension_call_in_a_body_and_on_a_render_path() {
  let dir = app(
    "std",
    &[
      ("routes/a/page.loader.ts", "import { intl, time, id } from \"@snapfire/fsr-client/std\";\nexport async function load({ params }) {\n  const when = time.now();\n  return { label: intl.number(Number(params.n)), id: id.new(), when };\n}\n"),
      ("routes/a/page.tsx", "import { intl, text } from \"@snapfire/fsr-client/std\";\nexport default function A({ n, name }: { n: number; name: string }) {\n  return <p title={text.slug(name)}>{intl.number(n)}</p>;\n}\n"),
    ],
  );
  let mut set = ComponentSet::new(&dir);
  let body = set.lower_loader("routes/a/page.loader.ts").unwrap();
  let Stmt::Let { expr, .. } = &body[0] else { panic!("{body:?}") };
  assert_eq!(exts(expr), vec!["time.now/0"]);
  let Stmt::Return(expr) = &body[1] else { panic!("{body:?}") };
  assert_eq!(exts(expr), vec!["intl.number/1", "id.new/0"]);

  set.lower("routes/a/page.tsx#default").unwrap();
  let (_, component) = &set.components[0];
  assert_eq!(tree_exts(&component.render), vec!["text.slug/1", "intl.number/1"]);
  let hoisted: Vec<u32> = {
    let mut ids = Vec::new();
    component.visit(&mut |e| {
      if let Expr::Hoist { id, .. } = e {
        ids.push(*id);
      }
    });
    ids
  };
  assert_eq!(hoisted.len(), 2, "both calls are props only, so both are hoisted: {component:?}");
}

#[test]
fn a_body_member_on_a_render_path_is_refused_and_not_downgraded() {
  let dir = app(
    "reach",
    &[
      ("routes/a/page.tsx", "import { time } from \"@snapfire/fsr-client/std\";\nexport default function A() {\n  return <p>{time.now()}</p>;\n}\n"),
      ("routes/b/page.tsx", "import { id } from \"@snapfire/fsr-client/std\";\nimport { useState } from \"react\";\nexport default function B() {\n  const [n, setN] = useState(0);\n  return <button onClick={() => setN(n + 1)} title={id.new()}>{n}</button>;\n}\n"),
      ("routes/c/page.tsx", "import { useState } from \"react\";\nimport { fresh } from \"@src/ids\";\nexport default function C() {\n  const [n, setN] = useState(0);\n  return <button onClick={() => setN(n + 1)}>{n}</button>;\n}\n"),
      ("routes/d/page.tsx", "import { fresh } from \"@src/ids\";\nexport default function D() {\n  return <p>{fresh()}</p>;\n}\n"),
      ("src/ids.ts", "import { id } from \"@snapfire/fsr-client/std\";\nexport function fresh() {\n  return id.new();\n}\n"),
    ],
  );
  let mut set = ComponentSet::new(&dir);
  match set.lower("routes/a/page.tsx#default") {
    Err(LowerError::Reach(residue)) => {
      assert_eq!((residue.file.as_str(), residue.line), ("routes/a/page.tsx", 3));
      assert!(residue.message.contains("`time.now` on a render path"), "{}", residue.message);
    }
    other => panic!("{other:?}"),
  }
  match set.lower("routes/b/page.tsx#default") {
    Err(LowerError::Reach(residue)) => assert!(residue.message.contains("`id.new`"), "{}", residue.message),
    other => panic!("{other:?}"),
  }
  set.lower("routes/c/page.tsx#default").unwrap();
  match set.lower("routes/d/page.tsx#default") {
    Err(LowerError::Reach(residue)) => assert!(residue.message.contains("`fresh` calls `id.new` on a render path"), "{}", residue.message),
    other => panic!("{other:?}"),
  }
  let unknown = app("unknown", &[("routes/a/page.tsx", "import { intl } from \"@snapfire/fsr-client/std\";\nexport default function A() {\n  return <p>{intl.money(1)}</p>;\n}\n")]);
  match ComponentSet::new(&unknown).lower("routes/a/page.tsx#default") {
    Err(LowerError::Residue(residue)) => assert!(residue.message.contains("`intl.money` is not a member of the standard library"), "{}", residue.message),
    other => panic!("{other:?}"),
  }
}

#[test]
fn a_handler_may_call_a_body_member() {
  let dir = app(
    "handler",
    &[("routes/a/page.tsx", "import { useState } from \"react\";\nimport { time } from \"@snapfire/fsr-client/std\";\nexport default function A() {\n  const [at, setAt] = useState(0);\n  return <button onClick={() => setAt(time.now())}>{at}</button>;\n}\n")],
  );
  let mut set = ComponentSet::new(&dir);
  set.lower("routes/a/page.tsx#default").unwrap();
  let (_, component) = &set.components[0];
  assert_eq!(component.handlers.len(), 1);
  let Stmt::Return(patch) = &component.handlers[0].body[0] else { panic!("{:?}", component.handlers) };
  assert_eq!(exts(patch), vec!["time.now/0"]);
}

#[test]
fn an_ext_module_lowers_every_export_or_fails_the_build() {
  let dir = app(
    "ext",
    &[
      ("ext/fmt.ts", "import { intl } from \"@snapfire/fsr-client/std\";\nexport const UNIT = \"kg\";\nexport function weight(grams: number): string {\n  return `${intl.number(grams / 1000)} ${UNIT}`;\n}\n"),
      ("ext/bad.ts", "export function stamp(): string {\n  return new Date().toISOString();\n}\n"),
      ("routes/a/page.loader.ts", "import { weight } from \"@ext/fmt\";\nexport async function load() {\n  return { label: weight(1500) };\n}\n"),
      ("routes/a/page.tsx", "import { weight } from \"@ext/fmt\";\nexport default function A({ grams }: { grams: number }) {\n  return <p>{weight(grams)}</p>;\n}\n"),
    ],
  );
  let mut set = ComponentSet::new(&dir);
  let rows = set.lower_extensions("ext/fmt.ts").unwrap();
  assert_eq!(rows, vec![("ext/fmt.ts#UNIT".to_owned(), "lowered".to_owned()), ("ext/fmt.ts#weight".to_owned(), "lowered".to_owned())]);
  match set.lower_extensions("ext/bad.ts") {
    Err(LowerError::Extension(residue)) => assert_eq!((residue.file.as_str(), residue.line), ("ext/bad.ts", 2)),
    other => panic!("{other:?}"),
  }
  let body = set.lower_loader("routes/a/page.loader.ts").unwrap();
  let Stmt::Return(expr) = &body[0] else { panic!("{body:?}") };
  assert!(matches!(expr, Expr::Object(_)));
  assert_eq!(exts(expr), vec!["intl.number/1"], "the helper is inlined into the body with its extension call: {expr:?}");
  set.lower("routes/a/page.tsx#default").unwrap();
  assert_eq!(tree_exts(&set.components[0].1.render), vec!["intl.number/1"]);
}

#[test]
fn a_native_pair_lowers_to_its_name_with_the_reach_its_declaration_gives() {
  let dir = app(
    "native",
    &[
      ("ext/fleet.ts", "import { native } from \"@snapfire/fsr-client/std\";\nexport const uptime = native(\"fleet.uptime\", (seconds: number) => `${Math.floor(seconds / 3600)}h`);\nexport const token = native<() => string>(\"fleet.token\");\n"),
      ("routes/a/page.tsx", "import { uptime } from \"@ext/fleet\";\nexport default function A({ seconds }: { seconds: number }) {\n  return <p>{uptime(seconds)}</p>;\n}\n"),
      ("routes/b/page.tsx", "import { token } from \"@ext/fleet\";\nexport default function B() {\n  return <p>{token()}</p>;\n}\n"),
      ("routes/b/page.loader.ts", "import { token, uptime } from \"@ext/fleet\";\nexport async function load() {\n  return { t: token(), u: uptime(7200) };\n}\n"),
      ("ext/broken.ts", "import { native } from \"@snapfire/fsr-client/std\";\nconst name = \"x.y\";\nexport const f = native(name);\n"),
    ],
  );
  let mut set = ComponentSet::new(&dir);
  let rows = set.lower_extensions("ext/fleet.ts").unwrap();
  assert_eq!(rows, vec![("ext/fleet.ts#uptime".to_owned(), "native render".to_owned()), ("ext/fleet.ts#token".to_owned(), "native body".to_owned())]);
  assert_eq!(set.natives, vec![("fleet.uptime".to_owned(), Reach::Render), ("fleet.token".to_owned(), Reach::Body)]);

  set.lower("routes/a/page.tsx#default").unwrap();
  let (_, component) = &set.components[0];
  assert_eq!(tree_exts(&component.render), vec!["fleet.uptime/1"]);
  let mut hoisted = 0;
  component.visit(&mut |e| {
    if matches!(e, Expr::Hoist { .. }) {
      hoisted += 1;
    }
  });
  assert_eq!(hoisted, 1, "a render native on props is hoisted like a helper");

  match set.lower("routes/b/page.tsx#default") {
    Err(LowerError::Reach(residue)) => assert!(residue.message.contains("`fleet.token` on a render path"), "{}", residue.message),
    other => panic!("{other:?}"),
  }
  let body = set.lower_loader("routes/b/page.loader.ts").unwrap();
  let Stmt::Return(expr) = &body[0] else { panic!("{body:?}") };
  assert_eq!(exts(expr), vec!["fleet.token/0", "fleet.uptime/1"]);

  match set.lower_extensions("ext/broken.ts") {
    Err(LowerError::Extension(residue)) => assert!(residue.message.contains("string literal"), "{}", residue.message),
    other => panic!("{other:?}"),
  }
}

#[test]
fn a_body_follows_a_local_helper_and_reports_the_line_that_stops_it() {
  let dir = app(
    "bodies",
    &[
      ("src/ui/money.ts", "export function money(cents: number): string {\n  return `$${(cents / 100).toFixed(2)}`;\n}\n"),
      ("routes/a/page.loader.ts", "import { money } from \"@src/ui/money\";\nexport async function load() {\n  return { total: money(1999) };\n}\n"),
      ("routes/a/actions.ts", "import { action } from \"@snapfire/fsr\";\nimport { money } from \"../../src/ui/money\";\nexport const label = action(async ({ input }) => {\n  return money(input.cents);\n});\n"),
      ("routes/b/page.loader.ts", "import { fetchIt } from \"@src/net\";\nexport async function load() {\n  return { x: fetchIt() };\n}\n"),
      ("src/net.ts", "export async function fetchIt() {\n  return await fetch(\"/x\");\n}\n"),
    ],
  );
  let mut set = ComponentSet::new(&dir);
  let body = set.lower_loader("routes/a/page.loader.ts").unwrap();
  let Stmt::Return(Expr::Object(entries)) = &body[0] else { panic!("{body:?}") };
  assert_eq!(entries.len(), 1);
  let actions = set.lower_actions("routes/a/actions.ts").unwrap();
  assert_eq!(actions.len(), 1);
  assert!(matches!(&actions[0].body[0], Stmt::Return(Expr::Apply { .. })), "{:?}", actions[0].body);
  match set.lower_loader("routes/b/page.loader.ts") {
    Err(LowerError::Residue(residue)) => assert_eq!((residue.file.as_str(), residue.line), ("src/net.ts", 2)),
    other => panic!("{other:?}"),
  }
}


#[test]
fn a_bare_t_from_the_standard_library_lowers_to_i18n_t_and_is_hoisted() {
  let dir = app(
    "t",
    &[
      ("routes/a/page.tsx", "import { t } from \"@snapfire/fsr-client/std\";\nexport default function A({ n }: { n: number }) {\n  return <p title={t(\"cart.title\")}>{t(\"cart.items\", { count: n })}</p>;\n}\n"),
      ("routes/a/page.loader.ts", "import { t } from \"@snapfire/fsr-client/std\";\nexport async function load() {\n  return { label: t(\"cart.title\") };\n}\n"),
      ("routes/b/page.tsx", "import { intl } from \"@snapfire/fsr-client/std\";\nexport default function B() {\n  return <p>{intl(1)}</p>;\n}\n"),
    ],
  );
  let mut set = ComponentSet::new(&dir);
  set.lower("routes/a/page.tsx#default").unwrap();
  assert_eq!(tree_exts(&set.components[0].1.render), vec!["i18n.t/1", "i18n.t/2"]);
  let mut hoisted = 0;
  set.components[0].1.visit(&mut |e| {
    if matches!(e, Expr::Hoist { .. }) {
      hoisted += 1;
    }
  });
  assert_eq!(hoisted, 2);
  let body = set.lower_loader("routes/a/page.loader.ts").unwrap();
  let Stmt::Return(expr) = &body[0] else { panic!("{body:?}") };
  assert_eq!(exts(expr), vec!["i18n.t/1"]);
  match set.lower("routes/b/page.tsx#default") {
    Err(LowerError::Residue(residue)) => assert!(residue.message.contains("`intl` from the standard library is a module"), "{}", residue.message),
    other => panic!("{other:?}"),
  }
}
