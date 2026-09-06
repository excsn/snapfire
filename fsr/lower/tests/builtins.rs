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
