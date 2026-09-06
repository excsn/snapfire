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
