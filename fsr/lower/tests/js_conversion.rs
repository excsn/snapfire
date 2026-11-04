//! A body converts a value as JavaScript does. Each expected value is what Node 26 printed for the same body through `JSON.stringify`, so NaN reads as null.

use snapfire_fsr_core::Value;
use snapfire_fsr_ir::Interpreter;
use snapfire_fsr_lower::component::ComponentSet;

static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn json(value: &Value) -> String {
  match value {
    Value::Null => "null".to_owned(),
    Value::Undefined => "undefined".to_owned(),
    Value::Bool(b) => b.to_string(),
    Value::Int(i) => i.to_string(),
    Value::F64(f) if !f.is_finite() => "null".to_owned(),
    Value::F64(f) if f.fract() == 0.0 => (*f as i64).to_string(),
    Value::F64(f) => f.to_string(),
    Value::Str(s) => format!("{:?}", s.to_string()),
    Value::Seq(items) => format!("[{}]", items.iter().map(json).collect::<Vec<_>>().join(",")),
    Value::Map(map) => format!("{{{}}}", map.iter().map(|(k, v)| format!("{:?}:{}", k.to_string(), json(v))).collect::<Vec<_>>().join(",")),
    other => format!("{other:?}"),
  }
}

fn agrees(source: String, node: &str) {
  let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let dir = snapfire_fsr_test_internal::scratch_root().join(format!("fsr_js_conversion_{}_{n}", std::process::id()));
  std::fs::create_dir_all(dir.join("routes/a")).unwrap();
  std::fs::write(dir.join("routes/a/page.loader.ts"), source).unwrap();
  let mut set = ComponentSet::new(&dir);
  let lowered = set.lower_loader("routes/a/page.loader.ts");
  let _ = std::fs::remove_dir_all(&dir);
  let body = lowered.unwrap_or_else(|e| panic!("refused: {e}"));
  let ctx = snapfire_fsr_runtime::RequestCtx::anonymous(Default::default());
  let interpreter = Interpreter::default().with_consts(Some(std::sync::Arc::new(set.consts.clone())));
  let got = futures::executor::block_on(interpreter.run(&body, &ctx, None)).map(|outcome| json(&outcome.value)).unwrap_or_else(|e| format!("THROW {}", e.message));
  assert_eq!(got, node);
}

fn in_a_helper(body: &str, node: &str) {
  agrees(format!("const f = () => {{ {body} }};\nexport async function load() {{\n  return f();\n}}\n"), node);
}

fn in_a_loader(body: &str, node: &str) {
  agrees(format!("export async function load() {{\n  {body}\n}}\n"), node);
}

#[test]
fn string_of_an_array_in_a_helper() {
  in_a_helper(r#"return String([1, 2]);"#, r#""1,2""#);
}

#[test]
fn string_of_an_array_in_a_loader() {
  in_a_loader(r#"return String([1, 2]);"#, r#""1,2""#);
}

#[test]
fn string_of_a_nested_array_in_a_helper() {
  in_a_helper(r#"return String([1, [2, 3]]);"#, r#""1,2,3""#);
}

#[test]
fn string_of_a_nested_array_in_a_loader() {
  in_a_loader(r#"return String([1, [2, 3]]);"#, r#""1,2,3""#);
}

#[test]
fn string_of_an_empty_array_in_a_helper() {
  in_a_helper(r#"return String([]);"#, r#""""#);
}

#[test]
fn string_of_an_empty_array_in_a_loader() {
  in_a_loader(r#"return String([]);"#, r#""""#);
}

#[test]
fn string_of_an_array_holding_null_in_a_helper() {
  in_a_helper(r#"return String([null, undefined, 1]);"#, r#"",,1""#);
}

#[test]
fn string_of_an_array_holding_null_in_a_loader() {
  in_a_loader(r#"return String([null, undefined, 1]);"#, r#"",,1""#);
}

#[test]
fn string_of_an_object_in_a_helper() {
  in_a_helper(r#"return String({ a: 1 });"#, r#""[object Object]""#);
}

#[test]
fn string_of_an_object_in_a_loader() {
  in_a_loader(r#"return String({ a: 1 });"#, r#""[object Object]""#);
}

#[test]
fn template_of_an_array_in_a_helper() {
  in_a_helper(r#"const a = [1, 2]; return `x${a}`;"#, r#""x1,2""#);
}

#[test]
fn template_of_an_array_in_a_loader() {
  in_a_loader(r#"const a = [1, 2]; return `x${a}`;"#, r#""x1,2""#);
}

#[test]
fn template_of_an_object_in_a_helper() {
  in_a_helper(r#"const o = { a: 1 }; return `x${o}`;"#, r#""x[object Object]""#);
}

#[test]
fn template_of_an_object_in_a_loader() {
  in_a_loader(r#"const o = { a: 1 }; return `x${o}`;"#, r#""x[object Object]""#);
}

#[test]
fn add_array_to_string_in_a_helper() {
  in_a_helper(r#"return [1] + "x";"#, r#""1x""#);
}

#[test]
fn add_array_to_string_in_a_loader() {
  in_a_loader(r#"return [1] + "x";"#, r#""1x""#);
}

#[test]
fn add_string_to_array_in_a_helper() {
  in_a_helper(r#"return "x" + [1];"#, r#""x1""#);
}

#[test]
fn add_string_to_array_in_a_loader() {
  in_a_loader(r#"return "x" + [1];"#, r#""x1""#);
}

#[test]
fn add_object_to_string_in_a_helper() {
  in_a_helper(r#"return ({}) + "x";"#, r#""[object Object]x""#);
}

#[test]
fn add_object_to_string_in_a_loader() {
  in_a_loader(r#"return ({}) + "x";"#, r#""[object Object]x""#);
}

#[test]
fn add_array_to_number_in_a_helper() {
  in_a_helper(r#"return [1] + 1;"#, r#""11""#);
}

#[test]
fn add_array_to_number_in_a_loader() {
  in_a_loader(r#"return [1] + 1;"#, r#""11""#);
}

#[test]
fn add_arrays_in_a_helper() {
  in_a_helper(r#"return [1] + [2];"#, r#""12""#);
}

#[test]
fn add_arrays_in_a_loader() {
  in_a_loader(r#"return [1] + [2];"#, r#""12""#);
}

#[test]
fn join_of_nested_arrays_in_a_helper() {
  in_a_helper(r#"return [[1, 2], [3]].join("|");"#, r#""1,2|3""#);
}

#[test]
fn join_of_nested_arrays_in_a_loader() {
  in_a_loader(r#"return [[1, 2], [3]].join("|");"#, r#""1,2|3""#);
}

#[test]
fn join_of_objects_in_a_helper() {
  in_a_helper(r#"return [{}, { a: 1 }].join("|");"#, r#""[object Object]|[object Object]""#);
}

#[test]
fn join_of_objects_in_a_loader() {
  in_a_loader(r#"return [{}, { a: 1 }].join("|");"#, r#""[object Object]|[object Object]""#);
}

#[test]
fn array_to_string_method_in_a_helper() {
  in_a_helper(r#"return [1, [2]].toString();"#, r#""1,2""#);
}

#[test]
fn array_to_string_method_in_a_loader() {
  in_a_loader(r#"return [1, [2]].toString();"#, r#""1,2""#);
}

#[test]
fn default_sort_of_arrays_in_a_helper() {
  in_a_helper(r#"const a = [[2], [1]]; return [...a].sort();"#, r#"[[1],[2]]"#);
}

#[test]
fn default_sort_of_arrays_in_a_loader() {
  in_a_loader(r#"const a = [[2], [1]]; return [...a].sort();"#, r#"[[1],[2]]"#);
}

#[test]
fn default_sort_of_objects_in_a_helper() {
  in_a_helper(r#"return [{ a: 1 }, { a: 2 }].toSorted().length;"#, r#"2"#);
}

#[test]
fn default_sort_of_objects_in_a_loader() {
  in_a_loader(r#"return [{ a: 1 }, { a: 2 }].toSorted().length;"#, r#"2"#);
}

#[test]
fn number_of_one_item_array_in_a_helper() {
  in_a_helper(r#"return Number([5]);"#, r#"5"#);
}

#[test]
fn number_of_one_item_array_in_a_loader() {
  in_a_loader(r#"return Number([5]);"#, r#"5"#);
}

#[test]
fn number_of_empty_array_in_a_helper() {
  in_a_helper(r#"return Number([]);"#, r#"0"#);
}

#[test]
fn number_of_empty_array_in_a_loader() {
  in_a_loader(r#"return Number([]);"#, r#"0"#);
}

#[test]
fn number_of_two_item_array_in_a_helper() {
  in_a_helper(r#"return Number([1, 2]);"#, r#"null"#);
}

#[test]
fn number_of_two_item_array_in_a_loader() {
  in_a_loader(r#"return Number([1, 2]);"#, r#"null"#);
}

#[test]
fn number_of_an_object_in_a_helper() {
  in_a_helper(r#"return Number({});"#, r#"null"#);
}

#[test]
fn number_of_an_object_in_a_loader() {
  in_a_loader(r#"return Number({});"#, r#"null"#);
}

#[test]
fn multiply_an_array_in_a_helper() {
  in_a_helper(r#"return [2] * 3;"#, r#"6"#);
}

#[test]
fn multiply_an_array_in_a_loader() {
  in_a_loader(r#"return [2] * 3;"#, r#"6"#);
}

#[test]
fn subtract_an_object_in_a_helper() {
  in_a_helper(r#"return ({}) - 1;"#, r#"null"#);
}

#[test]
fn subtract_an_object_in_a_loader() {
  in_a_loader(r#"return ({}) - 1;"#, r#"null"#);
}

#[test]
fn array_greater_than_number_in_a_helper() {
  in_a_helper(r#"return [2] > 1;"#, r#"true"#);
}

#[test]
fn array_greater_than_number_in_a_loader() {
  in_a_loader(r#"return [2] > 1;"#, r#"true"#);
}

#[test]
fn string_greater_than_array_in_a_helper() {
  in_a_helper(r#"return "b" > ["a"];"#, r#"true"#);
}

#[test]
fn string_greater_than_array_in_a_loader() {
  in_a_loader(r#"return "b" > ["a"];"#, r#"true"#);
}

#[test]
fn object_less_than_number_in_a_helper() {
  in_a_helper(r#"return ({}) < 1;"#, r#"false"#);
}

#[test]
fn object_less_than_number_in_a_loader() {
  in_a_loader(r#"return ({}) < 1;"#, r#"false"#);
}

#[test]
fn computed_key_number_in_a_helper() {
  in_a_helper(r#"const k = 1; return { [k]: "a" };"#, r#"{"1":"a"}"#);
}

#[test]
fn computed_key_number_in_a_loader() {
  in_a_loader(r#"const k = 1; return { [k]: "a" };"#, r#"{"1":"a"}"#);
}

#[test]
fn computed_key_bool_in_a_helper() {
  in_a_helper(r#"return { [true]: "a" };"#, r#"{"true":"a"}"#);
}

#[test]
fn computed_key_bool_in_a_loader() {
  in_a_loader(r#"return { [true]: "a" };"#, r#"{"true":"a"}"#);
}

#[test]
fn computed_key_null_in_a_helper() {
  in_a_helper(r#"return { [null]: "a" };"#, r#"{"null":"a"}"#);
}

#[test]
fn computed_key_null_in_a_loader() {
  in_a_loader(r#"return { [null]: "a" };"#, r#"{"null":"a"}"#);
}

#[test]
fn computed_key_array_in_a_helper() {
  in_a_helper(r#"return { [[1, 2]]: "a" };"#, r#"{"1,2":"a"}"#);
}

#[test]
fn computed_key_array_in_a_loader() {
  in_a_loader(r#"return { [[1, 2]]: "a" };"#, r#"{"1,2":"a"}"#);
}

#[test]
fn computed_key_object_in_a_helper() {
  in_a_helper(r#"return { [{}]: "a" };"#, r#"{"[object Object]":"a"}"#);
}

#[test]
fn computed_key_object_in_a_loader() {
  in_a_loader(r#"return { [{}]: "a" };"#, r#"{"[object Object]":"a"}"#);
}

#[test]
fn number_key_write_in_a_helper() {
  in_a_helper(r#"const o = {}; const k = 1; o[k] = "a"; return o;"#, r#"{"1":"a"}"#);
}

#[test]
fn number_key_write_in_a_loader() {
  in_a_loader(r#"const o = {}; const k = 1; o[k] = "a"; return o;"#, r#"{"1":"a"}"#);
}

#[test]
fn number_key_read_in_a_helper() {
  in_a_helper(r#"const o = { 1: "a" }; const k = 1; return o[k];"#, r#""a""#);
}

#[test]
fn number_key_read_in_a_loader() {
  in_a_loader(r#"const o = { 1: "a" }; const k = 1; return o[k];"#, r#""a""#);
}

#[test]
fn string_key_of_an_array_in_a_helper() {
  in_a_helper(r#"const k = "0"; return [5][k];"#, r#"5"#);
}

#[test]
fn string_key_of_an_array_in_a_loader() {
  in_a_loader(r#"const k = "0"; return [5][k];"#, r#"5"#);
}

#[test]
fn array_as_a_key_in_a_helper() {
  in_a_helper(r#"const o = { a: 1 }; return o[["a"]];"#, r#"1"#);
}

#[test]
fn array_as_a_key_in_a_loader() {
  in_a_loader(r#"const o = { a: 1 }; return o[["a"]];"#, r#"1"#);
}

#[test]
fn reduce_with_number_keys_in_a_helper() {
  in_a_helper(r#"return [1, 2].reduce((acc, x) => ({ ...acc, [x]: x }), {});"#, r#"{"1":1,"2":2}"#);
}

#[test]
fn reduce_with_number_keys_in_a_loader() {
  in_a_loader(r#"return [1, 2].reduce((acc, x) => ({ ...acc, [x]: x }), {});"#, r#"{"1":1,"2":2}"#);
}

#[test]
fn loose_number_string_in_a_helper() {
  in_a_helper(r#"const a = 1; const b = "1"; return a == b;"#, r#"true"#);
}

#[test]
fn loose_number_string_in_a_loader() {
  in_a_loader(r#"const a = 1; const b = "1"; return a == b;"#, r#"true"#);
}

#[test]
fn loose_string_number_not_in_a_helper() {
  in_a_helper(r#"const a = "x"; const b = 1; return a == b;"#, r#"false"#);
}

#[test]
fn loose_string_number_not_in_a_loader() {
  in_a_loader(r#"const a = "x"; const b = 1; return a == b;"#, r#"false"#);
}

#[test]
fn loose_not_equal_in_a_helper() {
  in_a_helper(r#"const a = 1; const b = "1"; return a != b;"#, r#"false"#);
}

#[test]
fn loose_not_equal_in_a_loader() {
  in_a_loader(r#"const a = 1; const b = "1"; return a != b;"#, r#"false"#);
}

#[test]
fn loose_bool_number_in_a_helper() {
  in_a_helper(r#"const a = true; const b = 1; return a == b;"#, r#"true"#);
}

#[test]
fn loose_bool_number_in_a_loader() {
  in_a_loader(r#"const a = true; const b = 1; return a == b;"#, r#"true"#);
}

#[test]
fn loose_bool_string_in_a_helper() {
  in_a_helper(r#"const a = false; const b = ""; return a == b;"#, r#"true"#);
}

#[test]
fn loose_bool_string_in_a_loader() {
  in_a_loader(r#"const a = false; const b = ""; return a == b;"#, r#"true"#);
}

#[test]
fn loose_zero_empty_in_a_helper() {
  in_a_helper(r#"const a = 0; const b = ""; return a == b;"#, r#"true"#);
}

#[test]
fn loose_zero_empty_in_a_loader() {
  in_a_loader(r#"const a = 0; const b = ""; return a == b;"#, r#"true"#);
}

#[test]
fn loose_array_string_in_a_helper() {
  in_a_helper(r#"return [1] == "1";"#, r#"true"#);
}

#[test]
fn loose_array_string_in_a_loader() {
  in_a_loader(r#"return [1] == "1";"#, r#"true"#);
}

#[test]
fn loose_array_number_in_a_helper() {
  in_a_helper(r#"return [1] == 1;"#, r#"true"#);
}

#[test]
fn loose_array_number_in_a_loader() {
  in_a_loader(r#"return [1] == 1;"#, r#"true"#);
}

#[test]
fn loose_array_false_in_a_helper() {
  in_a_helper(r#"return [] == false;"#, r#"true"#);
}

#[test]
fn loose_array_false_in_a_loader() {
  in_a_loader(r#"return [] == false;"#, r#"true"#);
}

#[test]
fn loose_object_string_in_a_helper() {
  in_a_helper(r#"return ({}) == "[object Object]";"#, r#"true"#);
}

#[test]
fn loose_object_string_in_a_loader() {
  in_a_loader(r#"return ({}) == "[object Object]";"#, r#"true"#);
}

#[test]
fn loose_null_undefined_in_a_helper() {
  in_a_helper(r#"const a = null; const b = undefined; return a == b;"#, r#"true"#);
}

#[test]
fn loose_null_undefined_in_a_loader() {
  in_a_loader(r#"const a = null; const b = undefined; return a == b;"#, r#"true"#);
}

#[test]
fn loose_null_zero_in_a_helper() {
  in_a_helper(r#"const a = null; const b = 0; return a == b;"#, r#"false"#);
}

#[test]
fn loose_null_zero_in_a_loader() {
  in_a_loader(r#"const a = null; const b = 0; return a == b;"#, r#"false"#);
}

#[test]
fn loose_string_string_in_a_helper() {
  in_a_helper(r#"const a = "a"; const b = "a"; return a == b;"#, r#"true"#);
}

#[test]
fn loose_string_string_in_a_loader() {
  in_a_loader(r#"const a = "a"; const b = "a"; return a == b;"#, r#"true"#);
}

#[test]
fn strict_stays_strict_in_a_helper() {
  in_a_helper(r#"const a = 1; const b = "1"; return a === b;"#, r#"false"#);
}

#[test]
fn strict_stays_strict_in_a_loader() {
  in_a_loader(r#"const a = 1; const b = "1"; return a === b;"#, r#"false"#);
}

#[test]
fn strict_array_string_stays_false_in_a_helper() {
  in_a_helper(r#"return [1] === "1";"#, r#"false"#);
}

#[test]
fn strict_array_string_stays_false_in_a_loader() {
  in_a_loader(r#"return [1] === "1";"#, r#"false"#);
}

#[test]
fn string_of_a_set_in_a_helper() {
  in_a_helper(r#"return String(new Set([1]));"#, r#""[object Set]""#);
}

#[test]
fn string_of_a_set_in_a_loader() {
  in_a_loader(r#"return String(new Set([1]));"#, r#""[object Set]""#);
}

#[test]
fn string_of_a_map_in_a_helper() {
  in_a_helper(r#"return String(new Map());"#, r#""[object Map]""#);
}

#[test]
fn string_of_a_map_in_a_loader() {
  in_a_loader(r#"return String(new Map());"#, r#""[object Map]""#);
}

#[test]
fn template_of_a_set_in_a_helper() {
  in_a_helper(r#"const s = new Set([1]); return `x${s}`;"#, r#""x[object Set]""#);
}

#[test]
fn template_of_a_set_in_a_loader() {
  in_a_loader(r#"const s = new Set([1]); return `x${s}`;"#, r#""x[object Set]""#);
}

#[test]
fn template_of_a_map_in_a_helper() {
  in_a_helper(r#"const m = new Map(); return `x${m}`;"#, r#""x[object Map]""#);
}

#[test]
fn template_of_a_map_in_a_loader() {
  in_a_loader(r#"const m = new Map(); return `x${m}`;"#, r#""x[object Map]""#);
}

#[test]
fn add_set_to_string_in_a_helper() {
  in_a_helper(r#"const s = new Set([1]); return s + "x";"#, r#""[object Set]x""#);
}

#[test]
fn add_set_to_string_in_a_loader() {
  in_a_loader(r#"const s = new Set([1]); return s + "x";"#, r#""[object Set]x""#);
}



#[test]
fn loose_set_string_in_a_helper() {
  in_a_helper(r#"const s = new Set([1]); return s == "[object Set]";"#, r#"true"#);
}

#[test]
fn loose_set_string_in_a_loader() {
  in_a_loader(r#"const s = new Set([1]); return s == "[object Set]";"#, r#"true"#);
}

#[test]
fn key_of_a_set_in_a_helper() {
  in_a_helper(r#"const s = new Set([1]); return { [s]: 1 };"#, r#"{"[object Set]":1}"#);
}

#[test]
fn key_of_a_set_in_a_loader() {
  in_a_loader(r#"const s = new Set([1]); return { [s]: 1 };"#, r#"{"[object Set]":1}"#);
}

#[test]
fn an_object_with_its_own_to_string_converts_through_it() {
  in_a_loader(r#"const o = { toString: () => "own" }; return String(o) + "|" + `${o}`;"#, r#""own|own""#);
}

#[test]
fn number_of_a_set_in_a_loader() {
  in_a_loader(r#"const s = new Set([1]); return [Number(s), +s, s * 2];"#, r#"[null,null,null]"#);
}

#[test]
fn two_sets_are_not_compared_as_their_text() {
  in_a_loader(r#"const s = new Set([1]); const t = new Set([1]); return s == "[object Set]" && t == "[object Set]";"#, r#"true"#);
}

#[test]
fn an_object_as_a_map_key_is_keyed_by_its_identity() {
  in_a_loader(r#"const a = { x: 1 }; const b = { y: 2 }; const m = new Map(); m.set(a, 1); return [m.get(b) === undefined, m.get(a)];"#, "[true,1]");
}

#[test]
fn an_object_as_a_key_of_a_new_map_is_keyed_by_its_identity() {
  in_a_loader(r#"const a = { x: 1 }; const b = { y: 2 }; const m = new Map([[a, 1]]); return [m.has(b), m.has(a), m.get(a)];"#, "[false,true,1]");
}

#[test]
fn a_set_inside_an_array_joins_as_its_tag_in_a_helper() {
  in_a_helper(r#"return [new Set([1])].join();"#, r#""[object Set]""#);
}

#[test]
fn a_set_inside_an_array_joins_as_its_tag_in_a_loader() {
  in_a_loader(r#"return [new Set([1])].join();"#, r#""[object Set]""#);
}

#[test]
fn a_set_passed_to_a_function_converts_as_a_set_in_a_helper() {
  in_a_helper(r#"const show = (s) => String(s); return show(new Set([1, 2]));"#, r#""[object Set]""#);
}

#[test]
fn a_set_passed_to_a_function_converts_as_a_set_in_a_loader() {
  in_a_loader(r#"const show = (s) => String(s); return show(new Set([1, 2]));"#, r#""[object Set]""#);
}

#[test]
fn a_map_in_a_template_converts_as_a_map_in_a_helper() {
  in_a_helper(r#"const m = new Map(); m.set("a", 1); const box = { m }; return `${box.m}`;"#, r#""[object Map]""#);
}

#[test]
fn a_map_in_a_template_converts_as_a_map_in_a_loader() {
  in_a_loader(r#"const m = new Map(); m.set("a", 1); const box = { m }; return `${box.m}`;"#, r#""[object Map]""#);
}

#[test]
fn a_set_serializes_as_an_empty_object_in_a_helper() {
  in_a_helper(r#"return JSON.stringify([new Set([1]), new Map([["a", 1]])]);"#, r#""[{},{}]""#);
}

#[test]
fn a_set_serializes_as_an_empty_object_in_a_loader() {
  in_a_loader(r#"return JSON.stringify([new Set([1]), new Map([["a", 1]])]);"#, r#""[{},{}]""#);
}

#[test]
fn a_set_is_not_an_array_in_a_helper() {
  in_a_helper(r#"const s = new Set([1]); const box = [s]; return [Array.isArray(box[0]), Array.isArray([...box[0]])];"#, r#"[false,true]"#);
}

#[test]
fn a_set_is_not_an_array_in_a_loader() {
  in_a_loader(r#"const s = new Set([1]); const box = [s]; return [Array.isArray(box[0]), Array.isArray([...box[0]])];"#, r#"[false,true]"#);
}

const CLASSES: &str = "class P { constructor(n) { this.n = n; } toString() { return \"P\" + this.n; } }\nclass M { constructor(v) { this.v = v; } valueOf() { return this.v; } }\nclass Q { constructor() { this.q = 1; } }\n";

#[test]
fn an_instance_with_to_string_converts_through_it() {
  agrees(format!("{CLASSES}{}", r#"export async function load() {
  return `${new P(2)}|${String(new P(3))}|${"x" + new P(4)}`;
}
"#), r#""P2|P3|xP4""#);
}

#[test]
fn an_instance_with_value_of_adds_and_compares_through_it() {
  agrees(format!("{CLASSES}{}", r#"export async function load() {
  return [new M(3) + 1, new M(3) > 2, new M(3) == 3];
}
"#), r#"[4,true,true]"#);
}

#[test]
fn an_instance_of_a_plain_class_converts_as_an_object() {
  agrees(format!("{CLASSES}{}", r#"export async function load() {
  return String(new Q());
}
"#), r#""[object Object]""#);
}

#[test]
fn instances_in_an_array_join_through_their_to_string() {
  agrees(format!("{CLASSES}{}", r#"export async function load() {
  return [new P(1), new P(2)].join("-");
}
"#), r#""P1-P2""#);
}
