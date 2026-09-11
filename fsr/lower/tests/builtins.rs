use std::path::PathBuf;

use snapfire_fsr_ir::{Expr, Stmt};
use snapfire_fsr_lower::component::ComponentSet;

fn app(tag: &str, files: &[(&str, &str)]) -> PathBuf {
  let dir = std::env::temp_dir().join(format!("fsr_builtins_{}_{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  for (name, source) in files {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, source).unwrap();
  }
  dir
}

#[test]
fn find_index_lowers_beside_find() {
  let dir = app(
    "find_index",
    &[(
      "routes/a/page.loader.ts",
      "export async function load({ params }) {\n  const at = [\"a\", \"b\", \"c\"].findIndex((s) => s === params.slug);\n  const hit = [\"a\", \"b\", \"c\"].find((s) => s === params.slug);\n  return { at, hit };\n}\n",
    )],
  );
  let body = ComponentSet::new(&dir).lower_loader("routes/a/page.loader.ts").unwrap();
  assert!(matches!(&body[0], Stmt::Let { expr: Expr::FindIndex(..), .. }), "{body:?}");
  assert!(matches!(&body[1], Stmt::Let { expr: Expr::Find(..), .. }), "{body:?}");
}

#[test]
fn a_residue_names_the_rewrite_that_does_the_same_thing() {
  let dir = app(
    "hint",
    &[(
      "routes/a/page.loader.ts",
      "export async function load({ query }) {\n  while (query.more) {\n    poll();\n  }\n  return {};\n}\n",
    )],
  );
  let err = ComponentSet::new(&dir).lower_loader("routes/a/page.loader.ts").unwrap_err().to_string();
  let (first, hint) = err.split_once('\n').unwrap_or_else(|| panic!("a residue is two lines: {err}"));
  assert!(first.ends_with("`while`, a loop whose length the build cannot know"), "{first}");
  assert!(first.starts_with("routes/a/page.loader.ts:2:3:"), "the line and column stay first: {first}");
  assert!(hint.starts_with("  ") && hint.contains("`map`, `filter`, `reduce`"), "the hint is indented and names the rewrite: {hint:?}");
}

/// Weight enough to be worth a name: ten rows of two fields.
const BIG: &str = "export const ROWS = [\n  { slug: \"a\", n: 1, on: true },\n  { slug: \"b\", n: 2, on: true },\n  { slug: \"c\", n: 3, on: true },\n  { slug: \"d\", n: 4, on: true },\n  { slug: \"e\", n: 5, on: true },\n  { slug: \"f\", n: 6, on: true },\n  { slug: \"g\", n: 7, on: true },\n  { slug: \"h\", n: 8, on: true },\n  { slug: \"i\", n: 9, on: true },\n  { slug: \"j\", n: 10, on: true },\n  { slug: \"k\", n: 11, on: true },\n  { slug: \"l\", n: 12, on: true },\n  { slug: \"m\", n: 13, on: true },\n  { slug: \"n\", n: 14, on: true },\n  { slug: \"o\", n: 15, on: true },\n  { slug: \"p\", n: 16, on: true },\n];\nexport const ONE = { slug: \"a\" };\n";

#[test]
fn a_large_module_constant_is_named_once_however_many_bodies_read_it() {
  let dir = app(
    "consts",
    &[
      ("src/rows.ts", BIG),
      ("routes/a/page.loader.ts", "import { ROWS } from \"@src/rows\";\nexport async function load({ params }) {\n  return { at: ROWS.findIndex((r) => r.slug === params.slug), all: ROWS };\n}\n"),
      ("routes/b/page.loader.ts", "import { ROWS } from \"@src/rows\";\nexport async function load() {\n  return { n: ROWS.length };\n}\n"),
    ],
  );
  let mut set = ComponentSet::new(&dir);
  let a = set.lower_loader("routes/a/page.loader.ts").unwrap();
  let b = set.lower_loader("routes/b/page.loader.ts").unwrap();

  assert_eq!(set.consts.keys().collect::<Vec<_>>(), vec!["src/rows.ts#ROWS"], "one entry for one constant");
  let refs = |body: &snapfire_fsr_ir::Body| {
    let mut n = 0;
    for stmt in body {
      if let Stmt::Return(expr) = stmt {
        expr.visit(&mut |e| {
          if matches!(e, Expr::Const(key) if key == "src/rows.ts#ROWS") {
            n += 1;
          }
        });
      }
    }
    n
  };
  assert_eq!(refs(&a), 2, "both reads in one body are references: {a:?}");
  assert_eq!(refs(&b), 1, "{b:?}");
}

#[test]
fn a_small_constant_stays_inline() {
  let dir = app(
    "consts_small",
    &[
      ("src/rows.ts", BIG),
      ("routes/a/page.loader.ts", "import { ONE } from \"@src/rows\";\nexport async function load() {\n  return { one: ONE };\n}\n"),
    ],
  );
  let mut set = ComponentSet::new(&dir);
  let body = set.lower_loader("routes/a/page.loader.ts").unwrap();
  assert!(set.consts.is_empty(), "naming a small constant costs more than copying it: {:?}", set.consts.keys().collect::<Vec<_>>());
  let Stmt::Return(expr) = &body[0] else { panic!("{body:?}") };
  let mut named = false;
  expr.visit(&mut |e| named |= matches!(e, Expr::Const(_)));
  assert!(!named, "{expr:?}");
}

/// The four string builtins, each in a loader body so the assertion reads the
/// `Expr` rather than a render tree.
fn lowered(tag: &str, expr: &str) -> Result<Vec<Stmt>, String> {
  let dir = app(tag, &[("routes/a/page.loader.ts", &format!("export async function load({{ params }}) {{\n  const out = {expr};\n  return {{ out }};\n}}\n"))]);
  ComponentSet::new(&dir).lower_loader("routes/a/page.loader.ts").map_err(|e| e.to_string())
}

fn builtin_of(body: &[Stmt]) -> snapfire_fsr_ir::ast::Builtin {
  match &body[0] {
    Stmt::Let { expr: Expr::Builtin { name, .. }, .. } => *name,
    other => panic!("not a builtin: {other:?}"),
  }
}

#[test]
fn the_four_string_builtins_lower() {
  use snapfire_fsr_ir::ast::Builtin;
  assert_eq!(builtin_of(&lowered("split", "params.slug.split(\"-\")").unwrap()), Builtin::Split);
  assert_eq!(builtin_of(&lowered("starts", "params.slug.startsWith(\"/fr\")").unwrap()), Builtin::StartsWith);
  assert_eq!(builtin_of(&lowered("ends", "params.slug.endsWith(\".tsx\")").unwrap()), Builtin::EndsWith);
  assert_eq!(builtin_of(&lowered("replace", "params.slug.replace(\" \", \"-\")").unwrap()), Builtin::Replace);
}

#[test]
fn a_second_argument_is_residue_rather_than_one_the_interpreter_drops() {
  for (tag, expr, takes) in [
    ("split_limit", "params.slug.split(\",\", 2)", "`split` takes 1 argument, got 2"),
    ("starts_at", "params.slug.startsWith(\"a\", 3)", "`startsWith` takes 1 argument, got 2"),
    ("ends_at", "params.slug.endsWith(\"a\", 3)", "`endsWith` takes 1 argument, got 2"),
    ("replace_one", "params.slug.replace(\" \")", "`replace` takes 2 arguments, got 1"),
  ] {
    let err = lowered(tag, expr).unwrap_err();
    assert!(err.contains(takes), "{err}");
  }
}

#[test]
fn split_on_an_empty_separator_is_residue() {
  let err = lowered("split_empty", "params.slug.split(\"\")").unwrap_err();
  assert!(err.contains("`split` with an empty separator"), "{err}");
  assert!(err.contains("UTF-16 code units"), "the hint says why: {err}");
}

#[test]
fn replace_over_a_regular_expression_is_still_residue() {
  let err = lowered("replace_regex", "params.slug.replace(/ /g, \"-\")").unwrap_err();
  assert!(err.contains("a regular expression"), "{err}");
}

#[test]
fn the_residue_hint_names_the_four() {
  let err = lowered("hint_four", "params.slug.padStart(3, \"0\")").unwrap_err();
  assert!(err.contains("`.padStart()`, which is not a builtin"), "{err}");
  for name in ["`split`", "`startsWith`", "`endsWith`", "`replace`"] {
    assert!(err.contains(name), "the hint names {name}: {err}");
  }
}
