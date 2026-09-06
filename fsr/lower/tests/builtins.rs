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
