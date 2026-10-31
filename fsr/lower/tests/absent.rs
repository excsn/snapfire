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

#[test]
fn a_computed_key_of_any_type_reads_as_javascript_reads_it() {
  let loader = "export async function load(ctx) {\n  const absent = ctx.query.space;\n  const none = null;\n  const yes = true;\n  const zero = \"0\";\n  const named = \"x\";\n  const len = \"length\";\n  const counts: Record<string, number> = { a: 1, null: 2, true: 3, undefined: 4 };\n  const plain: Record<string, number> = { a: 1 };\n  const items = [\"first\"];\n  const text = \"abc\";\n  const count = 5;\n  return {\n    objectAbsent: plain[absent] === undefined,\n    objectAbsentFallback: (plain[absent] ?? \"fallback\") === \"fallback\",\n    objectUndefinedKey: counts[absent] === 4,\n    objectNullKey: counts[none] === 2,\n    objectBoolKey: counts[yes] === 3,\n    arrayAbsent: items[absent] === undefined,\n    arrayNull: items[none] === undefined,\n    arrayBool: items[yes] === undefined,\n    arrayNumericString: items[zero] === \"first\",\n    arrayName: items[named] === undefined,\n    arrayLength: items[len] === 1,\n    textAbsent: text[absent] === undefined,\n    textNumericString: text[zero] === \"a\",\n    textName: text[named] === undefined,\n    textLength: text[len] === 3,\n    numberAbsent: count[absent] === undefined,\n    boolName: yes[named] === undefined,\n  };\n}\n";
  let Value::Map(fields) = loaded(loader) else { panic!() };
  for name in [
    "objectAbsent", "objectAbsentFallback", "objectUndefinedKey", "objectNullKey", "objectBoolKey", "arrayAbsent", "arrayNull", "arrayBool", "arrayNumericString",
    "arrayName", "arrayLength", "textAbsent", "textNumericString", "textName", "textLength", "numberAbsent", "boolName",
  ] {
    assert_eq!(fields.get(name), Some(&Value::Bool(true)), "{name}: {fields:?}");
  }
}
