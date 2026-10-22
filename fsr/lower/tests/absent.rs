//! What JavaScript reads as `undefined` compares strictly equal to `undefined` and unequal to `null`.

use snapfire_fsr_core::Value;
use snapfire_fsr_ir::Interpreter;

fn loaded(loader: &str) -> Value {
  let body = snapfire_fsr_lower::lower_loader("page.loader.ts", loader).unwrap_or_else(|e| panic!("{e}"));
  let ctx = snapfire_fsr_runtime::RequestCtx::anonymous(Default::default());
  futures::executor::block_on(Interpreter::default().run(&body, &ctx, None)).map(|outcome| outcome.value).unwrap_or_else(|e| panic!("{}", e.message))
}

#[test]
fn an_absent_value_is_strictly_undefined() {
  let loader = "export async function load() {\n  const books = [{ id: \"a\" }];\n  const counts: Record<string, number> = { a: 1 };\n  const found = books.find((b) => b.id === \"z\");\n  return {\n    find: found === undefined,\n    findNull: found === null,\n    index: books[3] === undefined,\n    at: books.at(5) === undefined,\n    key: counts[\"z\"] === undefined,\n    field: books[0].title === undefined,\n  };\n}\n";
  let Value::Map(fields) = loaded(loader) else { panic!() };
  for (name, expected) in [("find", true), ("findNull", false), ("index", true), ("at", true), ("key", true), ("field", true)] {
    assert_eq!(fields.get(name), Some(&Value::Bool(expected)), "{name}: {fields:?}");
  }
}
