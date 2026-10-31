//! A helper's loop changes a collection it declared, as a body writes it.

use snapfire_fsr_core::Value;
use snapfire_fsr_ir::Interpreter;

fn loaded(helper: &str) -> Value {
  let loader = format!("{helper}\nexport async function load() {{\n  return f([\"a\", \"b\"]);\n}}\n");
  let body = snapfire_fsr_lower::lower_loader("page.loader.ts", &loader).unwrap_or_else(|e| panic!("{e}"));
  let ctx = snapfire_fsr_runtime::RequestCtx::anonymous(Default::default());
  futures::executor::block_on(Interpreter::default().run(&body, &ctx, None)).map(|outcome| outcome.value).unwrap_or_else(|e| panic!("{}", e.message))
}

fn refusal(helper: &str) -> String {
  let loader = format!("{helper}\nexport async function load() {{\n  return f([\"a\", \"b\"]);\n}}\n");
  snapfire_fsr_lower::lower_loader("page.loader.ts", &loader).err().map(|e| e.to_string()).unwrap_or_else(|| "lowered".to_owned())
}

fn text(value: &str) -> Value {
  Value::from(value.to_owned())
}

fn texts(items: &[&str]) -> Value {
  Value::Seq(items.iter().map(|item| text(item)).collect())
}

#[test]
fn a_map_set_in_a_loop_is_kept() {
  assert_eq!(loaded("function f(keys) {\n  const m = new Map();\n  for (const k of keys) {\n    m.set(k, k + \"!\");\n  }\n  return [m.get(\"a\"), m.get(\"b\")];\n}"), texts(&["a!", "b!"]));
}

#[test]
fn a_map_loop_can_read_what_it_wrote() {
  assert_eq!(loaded("function f(keys) {\n  const m = new Map();\n  for (const k of keys) {\n    m.set(\"last\", (m.get(\"last\") ?? \"\") + k);\n  }\n  return [m.get(\"last\")];\n}"), texts(&["ab"]));
}

#[test]
fn a_map_delete_in_a_loop_is_kept() {
  assert_eq!(loaded("function f(keys) {\n  const m = new Map([[\"a\", \"1\"], [\"c\", \"3\"]]);\n  for (const k of keys) {\n    m.delete(k);\n  }\n  return [...m.keys()];\n}"), texts(&["c"]));
}

#[test]
fn a_set_add_in_a_loop_is_kept() {
  assert_eq!(loaded("function f(keys) {\n  const s = new Set();\n  for (const k of keys) {\n    s.add(k);\n  }\n  return [...s];\n}"), texts(&["a", "b"]));
}

#[test]
fn a_set_delete_in_a_loop_is_kept() {
  assert_eq!(loaded("function f(keys) {\n  const s = new Set([\"a\", \"c\"]);\n  for (const k of keys) {\n    s.delete(k);\n  }\n  return [...s];\n}"), texts(&["c"]));
}

#[test]
fn two_collections_in_one_loop_are_both_kept() {
  assert_eq!(loaded("function f(keys) {\n  const s = new Set();\n  const m = new Map();\n  for (const k of keys) {\n    s.add(k);\n    m.set(k, k);\n  }\n  return [...s, m.get(\"a\")];\n}"), texts(&["a", "b", "a"]));
}

#[test]
fn a_counted_loop_keeps_its_map_writes() {
  assert_eq!(loaded("function f(keys) {\n  const m = new Map();\n  for (let i = 0; i < keys.length; i += 1) {\n    m.set(keys[i], keys[i]);\n  }\n  return [m.get(\"b\")];\n}"), texts(&["b"]));
}

#[test]
fn a_for_each_keeps_its_map_writes() {
  assert_eq!(loaded("function f(keys) {\n  const m = new Map();\n  keys.forEach((k) => {\n    m.set(k, k);\n  });\n  return [m.get(\"a\")];\n}"), texts(&["a"]));
}

#[test]
fn search_params_set_in_a_loop_is_kept() {
  assert_eq!(loaded("function f(keys) {\n  const q = new URLSearchParams();\n  for (const k of keys) {\n    q.set(k, \"1\");\n  }\n  return q.toString();\n}"), text("a=1&b=1"));
}

#[test]
fn a_url_search_params_set_in_a_loop_is_kept() {
  assert_eq!(loaded("function f(keys) {\n  const u = new URL(\"https://x.test/p\");\n  for (const k of keys) {\n    u.searchParams.set(k, \"1\");\n  }\n  return u.search;\n}"), text("?a=1&b=1"));
}

#[test]
fn a_collection_write_outside_a_loop_is_kept() {
  assert_eq!(loaded("function f(keys) {\n  const m = new Map();\n  m.set(\"one\", keys[0]);\n  return [m.get(\"one\")];\n}"), texts(&["a"]));
  assert_eq!(loaded("function f(keys) {\n  const s = new Set();\n  s.add(keys[1]);\n  return [...s];\n}"), texts(&["b"]));
}

#[test]
fn a_clear_in_a_loop_is_kept() {
  assert_eq!(loaded("function f(keys) {\n  const s = new Set([\"x\"]);\n  for (const k of keys) {\n    s.clear();\n    s.add(k);\n  }\n  return [...s];\n}"), texts(&["b"]));
  assert_eq!(loaded("function f(keys) {\n  const m = new Map([[\"x\", \"1\"]]);\n  for (const k of keys) {\n    m.clear();\n    m.set(k, k);\n  }\n  return [...m.keys()];\n}"), texts(&["b"]));
}

#[test]
fn an_array_push_and_an_object_write_in_a_loop_are_kept() {
  assert_eq!(loaded("function f(keys) {\n  const m = [];\n  for (const k of keys) {\n    m.push(k);\n  }\n  return m;\n}"), texts(&["a", "b"]));
  assert_eq!(loaded("function f(keys) {\n  const m = {};\n  for (const k of keys) {\n    m[k] = k;\n  }\n  return [m.a, m.b];\n}"), texts(&["a", "b"]));
}

#[test]
fn a_loop_whose_only_statement_is_an_unknown_call_is_refused() {
  let refused = refusal("function poke(k) {\n  globalThis.seen = k;\n}\nfunction f(keys) {\n  for (const k of keys) {\n    poke(k);\n  }\n  return keys;\n}");
  assert_ne!(refused, "lowered");
}

#[test]
fn a_loop_writing_a_collection_it_cannot_reach_is_refused() {
  let refused = refusal("function f(keys, m) {\n  for (const k of keys) {\n    m.nope.set(k, 1);\n  }\n  return keys;\n}");
  assert_ne!(refused, "lowered");
}

#[test]
fn a_loop_with_no_effect_at_all_still_lowers() {
  assert_eq!(loaded("function f(keys) {\n  for (const k of keys) {\n    const unused = k + \"!\";\n  }\n  return keys;\n}"), texts(&["a", "b"]));
}
