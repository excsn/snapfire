//! A body lowers to what JavaScript computes. Each expected value is what Node 23 printed for the same body through `JSON.stringify`, with `c` true.
//! `u()` is `crypto.randomUUID()`, so two reads of one computed value compare equal only when it ran once. A build refusal passes only where the test names it with `Refusal::Named`.

use snapfire_fsr_core::Value;
use snapfire_fsr_ir::Interpreter;
use snapfire_fsr_lower::component::ComponentSet;

const PRELUDE: &str = "const u = () => crypto.randomUUID();\nconst walk = (n: number): string => { if (n === 0) return u(); const s = walk(n - 1); return s === s ? s : \"no\"; };\n";

static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn json(value: &Value) -> String {
  match value {
    Value::Null => "null".to_owned(),
    Value::Undefined => "undefined".to_owned(),
    Value::Bool(b) => b.to_string(),
    Value::Int(i) => i.to_string(),
    Value::F64(f) if f.fract() == 0.0 => (*f as i64).to_string(),
    Value::F64(f) => f.to_string(),
    Value::Str(s) => format!("{:?}", s.to_string()),
    Value::Seq(items) => format!("[{}]", items.iter().map(json).collect::<Vec<_>>().join(",")),
    Value::Map(map) => format!("{{{}}}", map.iter().map(|(k, v)| format!("{:?}:{}", k.to_string(), json(v))).collect::<Vec<_>>().join(",")),
    other => format!("{other:?}"),
  }
}

enum Refusal {
  Never,
  Named(&'static str),
}

fn scratch_app(file: &str, source: String) -> std::path::PathBuf {
  let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let dir = snapfire_fsr_test_internal::scratch_root().join(format!("fsr_js_semantics_{}_{n}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  let path = dir.join(file);
  std::fs::create_dir_all(path.parent().unwrap()).unwrap();
  std::fs::write(path, source).unwrap();
  dir
}

fn agrees(source: String, sites: usize, node: &str, refusal: Refusal) {
  let dir = scratch_app("routes/a/page.loader.ts", source);
  let mut set = ComponentSet::new(&dir);
  let lowered = set.lower_loader("routes/a/page.loader.ts");
  let _ = std::fs::remove_dir_all(&dir);
  let body = match (lowered, refusal) {
    (Ok(body), _) => body,
    (Err(e), Refusal::Named(reason)) => {
      assert!(e.to_string().contains(reason), "refused for another reason: {e}");
      return;
    }
    (Err(e), Refusal::Never) => panic!("refused: {e}"),
  };
  assert_eq!(format!("{body:?}").matches("name: \"uuid\"").count(), sites, "calls in the plan: {body:?}");
  let ctx = snapfire_fsr_runtime::RequestCtx::anonymous(Default::default());
  let interpreter = Interpreter::default().with_consts(Some(std::sync::Arc::new(set.consts.clone())));
  let got = futures::executor::block_on(interpreter.run(&body, &ctx, None)).map(|outcome| json(&outcome.value)).unwrap_or_else(|e| format!("THROW {}", e.message));
  assert_eq!(got, node);
}

fn in_a_helper(body: &str, sites: usize, node: &str, refusal: Refusal) {
  agrees(format!("{PRELUDE}const f = (c: boolean) => {{ {body} }};\nexport async function load() {{\n  return f(true);\n}}\n"), sites, node, refusal);
}

fn in_a_loader(body: &str, sites: usize, node: &str, refusal: Refusal) {
  agrees(format!("{PRELUDE}export async function load() {{\n  const c = Math.abs(1) === 1;\n  {body}\n}}\n"), sites, node, refusal);
}

// `u` here is a function the lowerer keeps, so its parameter name counts the sites of the call in a component's plan.
fn in_a_page(source: &str) {
  let dir = scratch_app("routes/page.tsx", format!("const u = () => [1].map((zz) => zz);\n{source}\n"));
  let mut set = ComponentSet::new(&dir);
  let lowered = set.lower("routes/page.tsx#default");
  set.settle_proofs();
  set.settle_fills();
  let _ = std::fs::remove_dir_all(&dir);
  lowered.unwrap();
  let (_, component) = set.components.iter().find(|(module, _)| module == "routes/page.tsx#default").unwrap();
  assert_eq!(format!("{component:?}").matches("params: [\"zz\"").count(), 1, "calls in the plan: {component:?}");
}

mod computed_once {
  use super::*;

  #[test]
  fn decl_in_a_helper() {
    in_a_helper(r#"const a = u(); return a === a;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn decl_in_a_loader() {
    in_a_loader(r#"const a = u(); return a === a;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn decl_two_consts_in_a_helper() {
    in_a_helper(r#"const a = u(); const b = a + a; return b === b;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn decl_two_consts_in_a_loader() {
    in_a_loader(r#"const a = u(); const b = a + a; return b === b;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn destructure_array_in_a_helper() {
    in_a_helper(r#"const [a] = [u()]; return a === a;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn destructure_array_in_a_loader() {
    in_a_loader(r#"const [a] = [u()]; return a === a;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn destructure_object_in_a_helper() {
    in_a_helper(r#"const { a } = { a: u() }; return a === a;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn destructure_object_in_a_loader() {
    in_a_loader(r#"const { a } = { a: u() }; return a === a;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn destructure_of_const_in_a_helper() {
    in_a_helper(r#"const o = { a: u() }; const { a } = o; return a === a;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn destructure_of_const_in_a_loader() {
    in_a_loader(r#"const o = { a: u() }; const { a } = o; return a === a;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn destructure_default_in_a_helper() {
    in_a_helper(r#"const { a = u() } = {}; return a === a;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn destructure_default_in_a_loader() {
    in_a_loader(r#"const { a = u() } = {}; return a === a;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn destructure_rest_in_a_helper() {
    in_a_helper(r#"const [x, ...r] = [1, u()]; return r[0] === r[0];"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn destructure_rest_in_a_loader() {
    in_a_loader(r#"const [x, ...r] = [1, u()]; return r[0] === r[0];"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn let_reassign_in_a_helper() {
    in_a_helper(r#"let a = ""; a = u(); return a === a;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn let_reassign_in_a_loader() {
    in_a_loader(r#"let a = ""; a = u(); return a === a;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn let_reassign_chain_in_a_helper() {
    in_a_helper(r#"let a = u(); a = a + "x"; a = a + a; return a === a;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn let_reassign_chain_in_a_loader() {
    in_a_loader(r#"let a = u(); a = a + "x"; a = a + a; return a === a;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn let_compound_in_a_helper() {
    in_a_helper(r#"let a = "x"; a += u(); return a === a;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn let_compound_in_a_loader() {
    in_a_loader(r#"let a = "x"; a += u(); return a === a;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn field_write_in_a_helper() {
    in_a_helper(r#"const o = { a: "" }; o.a = u(); return o.a === o.a;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn field_write_in_a_loader() {
    in_a_loader(r#"const o = { a: "" }; o.a = u(); return o.a === o.a;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn field_write_computed_key_in_a_helper() {
    in_a_helper(r#"const o = {}; o[u()] = 1; return Object.keys(o).length === 1;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn field_write_computed_key_in_a_loader() {
    in_a_loader(r#"const o = {}; o[u()] = 1; return Object.keys(o).length === 1;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn array_push_in_a_helper() {
    in_a_helper(r#"const xs = []; xs.push(u()); return xs[0] === xs[0];"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn array_push_in_a_loader() {
    in_a_loader(r#"const xs = []; xs.push(u()); return xs[0] === xs[0];"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn map_set_in_a_helper() {
    in_a_helper(r#"const m = new Map(); m.set("a", u()); return m.get("a") === m.get("a");"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn map_set_in_a_loader() {
    in_a_loader(r#"const m = new Map(); m.set("a", u()); return m.get("a") === m.get("a");"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn set_of_in_a_helper() {
    in_a_helper(r#"const s = new Set([u()]); const l = [...s]; return l[0] === l[0];"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn set_of_in_a_loader() {
    in_a_loader(r#"const s = new Set([u()]); const l = [...s]; return l[0] === l[0];"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn url_write_in_a_helper() {
    in_a_helper(r#"const url = new URL("https://x.test/"); url.pathname = "/" + u(); return url.pathname === url.pathname;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn url_write_in_a_loader() {
    in_a_loader(r#"const url = new URL("https://x.test/"); url.pathname = "/" + u(); return url.pathname === url.pathname;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn if_write_in_a_helper() {
    in_a_helper(r#"let a = ""; if (c) { a = u(); } return a === a;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn if_write_in_a_loader() {
    in_a_loader(r#"let a = ""; if (c) { a = u(); } return a === a;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn if_condition_two_writes_in_a_helper() {
    in_a_helper(r#"let a = 0; let b = 0; if (u().length > 0) { a = 1; b = 2; } return a + b;"#, 1, r#"3"#, Refusal::Never);
  }

  #[test]
  fn if_condition_two_writes_in_a_loader() {
    in_a_loader(r#"let a = 0; let b = 0; if (u().length > 0) { a = 1; b = 2; } return a + b;"#, 1, r#"3"#, Refusal::Never);
  }

  #[test]
  fn if_const_inside_in_a_helper() {
    in_a_helper(r#"let a = ""; if (c) { const k = u(); a = k + k; } return a === a;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn if_const_inside_in_a_loader() {
    in_a_loader(r#"let a = ""; if (c) { const k = u(); a = k + k; } return a === a;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn if_else_writes_in_a_helper() {
    in_a_helper(r#"let a = ""; if (!c) { a = "x"; } else { a = u(); } return a === a;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn if_else_writes_in_a_loader() {
    in_a_loader(r#"let a = ""; if (!c) { a = "x"; } else { a = u(); } return a === a;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn if_not_taken_runs_nothing_in_a_helper() {
    in_a_helper(r#"let a = 0; if (!c) { const k = JSON.parse("{"); a = k.b; } return a;"#, 0, r#"0"#, Refusal::Never);
  }

  #[test]
  fn if_not_taken_runs_nothing_in_a_loader() {
    in_a_loader(r#"let a = 0; if (!c) { const k = JSON.parse("{"); a = k.b; } return a;"#, 0, r#"0"#, Refusal::Never);
  }

  #[test]
  fn if_return_condition_in_a_helper() {
    in_a_helper(r#"if (u().length === 0) { return 1; } return 2;"#, 1, r#"2"#, Refusal::Never);
  }

  #[test]
  fn if_return_condition_in_a_loader() {
    in_a_loader(r#"if (u().length === 0) { return 1; } return 2;"#, 1, r#"2"#, Refusal::Never);
  }

  #[test]
  fn return_after_const_in_a_helper() {
    in_a_helper(r#"const k = u(); if (!c) { return k; } return k === k;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn return_after_const_in_a_loader() {
    in_a_loader(r#"const k = u(); if (!c) { return k; } return k === k;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn ternary_in_a_helper() {
    in_a_helper(r#"const a = c ? u() : "x"; return a === a;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn ternary_in_a_loader() {
    in_a_loader(r#"const a = c ? u() : "x"; return a === a;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn logical_or_in_a_helper() {
    in_a_helper(r#"const a = u() || "x"; return a === a;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn logical_or_in_a_loader() {
    in_a_loader(r#"const a = u() || "x"; return a === a;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn logical_and_in_a_helper() {
    in_a_helper(r#"const a = u() && 1; return a === a;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn logical_and_in_a_loader() {
    in_a_loader(r#"const a = u() && 1; return a === a;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn nullish_in_a_helper() {
    in_a_helper(r#"const a = u() ?? "x"; return a === a;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn nullish_in_a_loader() {
    in_a_loader(r#"const a = u() ?? "x"; return a === a;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn optional_field_in_a_helper() {
    in_a_helper(r#"const a = ({ b: u() })?.b; return a === a;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn optional_field_in_a_loader() {
    in_a_loader(r#"const a = ({ b: u() })?.b; return a === a;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn optional_on_const_in_a_helper() {
    in_a_helper(r#"const o = { b: u() }; return o?.b === o?.b;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn optional_on_const_in_a_loader() {
    in_a_loader(r#"const o = { b: u() }; return o?.b === o?.b;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn optional_nested_call_in_a_helper() {
    in_a_helper(r#"const g = () => ({ b: u() }); const a = g()?.b; return a === a;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn optional_nested_call_in_a_loader() {
    in_a_loader(r#"const g = () => ({ b: u() }); const a = g()?.b; return a === a;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn template_in_a_helper() {
    in_a_helper(r#"const a = `${u()}`; return a === a;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn template_in_a_loader() {
    in_a_loader(r#"const a = `${u()}`; return a === a;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn sequence_in_a_helper() {
    in_a_helper(r#"const a = (1, u()); return a === a;"#, 1, r#"true"#, Refusal::Named(r#"a comma expression"#));
  }

  #[test]
  fn sequence_in_a_loader() {
    in_a_loader(r#"const a = (1, u()); return a === a;"#, 1, r#"true"#, Refusal::Named(r#"a comma expression"#));
  }

  #[test]
  fn object_spread_in_a_helper() {
    in_a_helper(r#"const o = { ...{ a: u() } }; return o.a === o.a;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn object_spread_in_a_loader() {
    in_a_loader(r#"const o = { ...{ a: u() } }; return o.a === o.a;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn array_spread_in_a_helper() {
    in_a_helper(r#"const a = [...[u()]]; return a[0] === a[0];"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn array_spread_in_a_loader() {
    in_a_loader(r#"const a = [...[u()]]; return a[0] === a[0];"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn switch_writes_in_a_helper() {
    in_a_helper(r#"let r = 0; switch (u()) { case "a": r = 1; break; case "b": r = 2; break; default: r = 3; } return r;"#, 1, r#"3"#, Refusal::Never);
  }

  #[test]
  fn switch_writes_in_a_loader() {
    in_a_loader(r#"let r = 0; switch (u()) { case "a": r = 1; break; case "b": r = 2; break; default: r = 3; } return r;"#, 1, r#"3"#, Refusal::Never);
  }

  #[test]
  fn switch_returns_in_a_helper() {
    in_a_helper(r#"switch (u()) { case "a": return 1; case "b": return 2; default: return 3; }"#, 1, r#"3"#, Refusal::Never);
  }

  #[test]
  fn switch_returns_in_a_loader() {
    in_a_loader(r#"switch (u()) { case "a": return 1; case "b": return 2; default: return 3; }"#, 1, r#"3"#, Refusal::Never);
  }

  #[test]
  fn for_of_iterable_in_a_helper() {
    in_a_helper(r#"let n = 0; for (const x of [u()]) { n += 1; } return n;"#, 1, r#"1"#, Refusal::Never);
  }

  #[test]
  fn for_of_iterable_in_a_loader() {
    in_a_loader(r#"let n = 0; for (const x of [u()]) { n += 1; } return n;"#, 1, r#"1"#, Refusal::Never);
  }

  #[test]
  fn for_of_push_in_a_helper() {
    in_a_helper(r#"const out = []; for (const y of [1]) { out.push(u()); } return out[0] === out[0];"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn for_of_push_in_a_loader() {
    in_a_loader(r#"const out = []; for (const y of [1]) { out.push(u()); } return out[0] === out[0];"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn for_of_const_body_in_a_helper() {
    in_a_helper(r#"let r = ""; for (const y of [1]) { const k = u(); r = k + k; } return r === r;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn for_of_const_body_in_a_loader() {
    in_a_loader(r#"let r = ""; for (const y of [1]) { const k = u(); r = k + k; } return r === r;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn for_of_two_names_in_a_helper() {
    in_a_helper(r#"let a = ""; let b = ""; for (const y of [1]) { a = u(); b = a; } return a === b;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn for_of_two_names_in_a_loader() {
    in_a_loader(r#"let a = ""; let b = ""; for (const y of [1]) { a = u(); b = a; } return a === b;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn for_counted_in_a_helper() {
    in_a_helper(r#"let s = ""; for (let i = 0; i < 1; i++) { s = u(); } return s === s;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn for_counted_in_a_loader() {
    in_a_loader(r#"let s = ""; for (let i = 0; i < 1; i++) { s = u(); } return s === s;"#, 1, r#"true"#, Refusal::Named(r#"a `for` loop other than `for...of`"#));
  }

  #[test]
  fn map_callback_in_a_helper() {
    in_a_helper(r#"const a = [1].map(() => u()); return a[0] === a[0];"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn map_callback_in_a_loader() {
    in_a_loader(r#"const a = [1].map(() => u()); return a[0] === a[0];"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn filter_then_read_in_a_helper() {
    in_a_helper(r#"const a = [u()].filter((x) => x.length > 0); return a[0] === a[0];"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn filter_then_read_in_a_loader() {
    in_a_loader(r#"const a = [u()].filter((x) => x.length > 0); return a[0] === a[0];"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn argument_to_arrow_in_a_helper() {
    in_a_helper(r#"const g = (x) => x === x; return g(u());"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn argument_to_arrow_in_a_loader() {
    in_a_loader(r#"const g = (x) => x === x; return g(u());"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn argument_to_function_in_a_helper() {
    in_a_helper(r#"function g(x) { return x === x; } return g(u());"#, 1, r#"true"#, Refusal::Named(r#"a statement a straight-line body cannot hold"#));
  }

  #[test]
  fn argument_to_function_in_a_loader() {
    in_a_loader(r#"function g(x) { return x === x; } return g(u());"#, 1, r#"true"#, Refusal::Named(r#"a function declared inside a body"#));
  }

  #[test]
  fn arrow_body_const_in_a_helper() {
    in_a_helper(r#"const g = () => { const k = u(); return k === k; }; return g();"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn arrow_body_const_in_a_loader() {
    in_a_loader(r#"const g = () => { const k = u(); return k === k; }; return g();"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn closure_capture_in_a_helper() {
    in_a_helper(r#"const k = u(); const g = () => k; return g() === g();"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn closure_capture_in_a_loader() {
    in_a_loader(r#"const k = u(); const g = () => k; return g() === g();"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn object_of_functions_in_a_helper() {
    in_a_helper(r#"const o = { g: () => 1 }; const k = u(); return o.g() === 1 && k === k;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn object_of_functions_in_a_loader() {
    in_a_loader(r#"const o = { g: () => 1 }; const k = u(); return o.g() === 1 && k === k;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn function_result_in_a_helper() {
    in_a_helper(r#"const g = () => u(); const a = g(); return a === a;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn function_result_in_a_loader() {
    in_a_loader(r#"const g = () => u(); const a = g(); return a === a;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn recursion_in_a_helper() {
    in_a_helper(r#"const a = walk(2); return a === a;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn recursion_in_a_loader() {
    in_a_loader(r#"const a = walk(2); return a === a;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn try_write_in_a_helper() {
    in_a_helper(r#"let a = ""; try { a = u(); } catch (e) { a = "f"; } return a === a;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn try_write_in_a_loader() {
    in_a_loader(r#"let a = ""; try { a = u(); } catch (e) { a = "f"; } return a === a;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn try_return_in_a_helper() {
    in_a_helper(r#"try { const k = u(); return k === k; } catch (e) { return false; }"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn try_return_in_a_loader() {
    in_a_loader(r#"try { const k = u(); return k === k; } catch (e) { return false; }"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn try_const_after_in_a_helper() {
    in_a_helper(r#"let a = ""; try { a = "x"; } catch (e) { a = "f"; } const k = u(); return k === k;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn try_const_after_in_a_loader() {
    in_a_loader(r#"let a = ""; try { a = "x"; } catch (e) { a = "f"; } const k = u(); return k === k;"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn two_calls_stay_two_in_a_helper() {
    in_a_helper(r#"const a = u(); const b = u(); return a !== b;"#, 2, r#"true"#, Refusal::Never);
  }

  #[test]
  fn two_calls_stay_two_in_a_loader() {
    in_a_loader(r#"const a = u(); const b = u(); return a !== b;"#, 2, r#"true"#, Refusal::Never);
  }

  #[test]
  fn map_each_item_differs_in_a_helper() {
    in_a_helper(r#"const a = [1, 2].map(() => u()); return a[0] !== a[1];"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn map_each_item_differs_in_a_loader() {
    in_a_loader(r#"const a = [1, 2].map(() => u()); return a[0] !== a[1];"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn loop_each_pass_differs_in_a_helper() {
    in_a_helper(r#"const out = []; for (const y of [1, 2]) { out.push(u()); } return out[0] !== out[1];"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn loop_each_pass_differs_in_a_loader() {
    in_a_loader(r#"const out = []; for (const y of [1, 2]) { out.push(u()); } return out[0] !== out[1];"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn loop_const_each_pass_differs_in_a_helper() {
    in_a_helper(r#"const out = []; for (const y of [1, 2]) { const k = u(); out.push(k + k); } return out[0] !== out[1];"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn loop_const_each_pass_differs_in_a_loader() {
    in_a_loader(r#"const out = []; for (const y of [1, 2]) { const k = u(); out.push(k + k); } return out[0] !== out[1];"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn function_called_twice_differs_in_a_helper() {
    in_a_helper(r#"const g = () => u(); return g() !== g();"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn function_called_twice_differs_in_a_loader() {
    in_a_loader(r#"const g = () => u(); return g() !== g();"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn swap_in_a_helper() {
    in_a_helper(r#"let a = 1; let b = 2; const t = a; a = b; b = t; return [a, b];"#, 0, r#"[2,1]"#, Refusal::Never);
  }

  #[test]
  fn swap_in_a_loader() {
    in_a_loader(r#"let a = 1; let b = 2; const t = a; a = b; b = t; return [a, b];"#, 0, r#"[2,1]"#, Refusal::Never);
  }

  #[test]
  fn const_keeps_old_value_in_a_helper() {
    in_a_helper(r#"let a = 1; const b = a; a = 2; return [a, b];"#, 0, r#"[2,1]"#, Refusal::Never);
  }

  #[test]
  fn const_keeps_old_value_in_a_loader() {
    in_a_loader(r#"let a = 1; const b = a; a = 2; return [a, b];"#, 0, r#"[2,1]"#, Refusal::Never);
  }

  #[test]
  fn const_keeps_old_array_in_a_helper() {
    in_a_helper(r#"let a = [1]; const b = a; a = [2]; return [a[0], b[0]];"#, 0, r#"[2,1]"#, Refusal::Never);
  }

  #[test]
  fn const_keeps_old_array_in_a_loader() {
    in_a_loader(r#"let a = [1]; const b = a; a = [2]; return [a[0], b[0]];"#, 0, r#"[2,1]"#, Refusal::Never);
  }

  #[test]
  fn nested_field_write_in_a_helper() {
    in_a_helper(r#"const o = { p: { q: 1 } }; o.p.q = 2; return o.p.q;"#, 0, r#"2"#, Refusal::Never);
  }

  #[test]
  fn nested_field_write_in_a_loader() {
    in_a_loader(r#"const o = { p: { q: 1 } }; o.p.q = 2; return o.p.q;"#, 0, r#"2"#, Refusal::Never);
  }

  #[test]
  fn write_reads_itself_in_a_helper() {
    in_a_helper(r#"let a = [1]; a = [...a, a.length]; a = [...a, a.length]; return a;"#, 0, r#"[1,1,2]"#, Refusal::Never);
  }

  #[test]
  fn write_reads_itself_in_a_loader() {
    in_a_loader(r#"let a = [1]; a = [...a, a.length]; a = [...a, a.length]; return a;"#, 0, r#"[1,1,2]"#, Refusal::Never);
  }

  #[test]
  fn if_else_not_taken_throws_in_a_helper() {
    in_a_helper(r#"let a = 0; if (c) { a = 1; } else { a = JSON.parse("{"); } return a;"#, 0, r#"1"#, Refusal::Never);
  }

  #[test]
  fn if_else_not_taken_throws_in_a_loader() {
    in_a_loader(r#"let a = 0; if (c) { a = 1; } else { a = JSON.parse("{"); } return a;"#, 0, r#"1"#, Refusal::Never);
  }

  #[test]
  fn nested_if_not_taken_throws_in_a_helper() {
    in_a_helper(r#"let a = 0; if (c) { if (!c) { a = JSON.parse("{"); } } return a;"#, 0, r#"0"#, Refusal::Never);
  }

  #[test]
  fn nested_if_not_taken_throws_in_a_loader() {
    in_a_loader(r#"let a = 0; if (c) { if (!c) { a = JSON.parse("{"); } } return a;"#, 0, r#"0"#, Refusal::Never);
  }

  #[test]
  fn loop_if_not_taken_throws_in_a_helper() {
    in_a_helper(r#"let a = 0; for (const y of [1]) { if (!c) { a = JSON.parse("{"); } } return a;"#, 0, r#"0"#, Refusal::Never);
  }

  #[test]
  fn loop_if_not_taken_throws_in_a_loader() {
    in_a_loader(r#"let a = 0; for (const y of [1]) { if (!c) { a = JSON.parse("{"); } } return a;"#, 0, r#"0"#, Refusal::Never);
  }

  #[test]
  fn switch_not_taken_throws_in_a_helper() {
    in_a_helper(r#"let a = 0; switch (1) { case 2: a = JSON.parse("{"); break; default: a = 5; } return a;"#, 0, r#"5"#, Refusal::Never);
  }

  #[test]
  fn switch_not_taken_throws_in_a_loader() {
    in_a_loader(r#"let a = 0; switch (1) { case 2: a = JSON.parse("{"); break; default: a = 5; } return a;"#, 0, r#"5"#, Refusal::Never);
  }

  #[test]
  fn try_catches_write_in_a_helper() {
    in_a_helper(r#"let a = 0; try { a = JSON.parse("{"); } catch (e) { a = 1; } return a;"#, 0, r#"1"#, Refusal::Never);
  }

  #[test]
  fn try_catches_write_in_a_loader() {
    in_a_loader(r#"let a = 0; try { a = JSON.parse("{"); } catch (e) { a = 1; } return a;"#, 0, r#"1"#, Refusal::Never);
  }

  #[test]
  fn try_catches_const_in_a_helper() {
    in_a_helper(r#"let a = 0; try { const k = JSON.parse("{"); a = k; } catch (e) { a = 1; } return a;"#, 0, r#"1"#, Refusal::Never);
  }

  #[test]
  fn try_catches_const_in_a_loader() {
    in_a_loader(r#"let a = 0; try { const k = JSON.parse("{"); a = k; } catch (e) { a = 1; } return a;"#, 0, r#"1"#, Refusal::Never);
  }

  #[test]
  fn try_write_before_throw_in_a_helper() {
    in_a_helper(r#"let a = 0; try { a = 1; const k = JSON.parse("{"); a = 2; } catch (e) { a = a + 10; } return a;"#, 0, r#"11"#, Refusal::Never);
  }

  #[test]
  fn try_write_before_throw_in_a_loader() {
    in_a_loader(r#"let a = 0; try { a = 1; const k = JSON.parse("{"); a = 2; } catch (e) { a = a + 10; } return a;"#, 0, r#"11"#, Refusal::Never);
  }

  #[test]
  fn throw_untaken_in_a_helper() {
    in_a_helper(r#"if (!c) { throw new Error("x"); } return 1;"#, 0, r#"1"#, Refusal::Never);
  }

  #[test]
  fn throw_untaken_in_a_loader() {
    in_a_loader(r#"if (!c) { throw new Error("x"); } return 1;"#, 0, r#"1"#, Refusal::Never);
  }

  #[test]
  fn continue_in_a_helper() {
    in_a_helper(r#"let n = 0; for (const y of [1, 2, 3]) { if (y === 2) continue; n += y; } return n;"#, 0, r#"4"#, Refusal::Never);
  }

  #[test]
  fn continue_in_a_loader() {
    in_a_loader(r#"let n = 0; for (const y of [1, 2, 3]) { if (y === 2) continue; n += y; } return n;"#, 0, r#"4"#, Refusal::Named(r#"`break` or `continue`"#));
  }

  #[test]
  fn loop_accumulate_object_in_a_helper() {
    in_a_helper(r#"const m = {}; for (const y of ["a", "b"]) { m[y] = y + y; } return m;"#, 0, r#"{"a":"aa","b":"bb"}"#, Refusal::Never);
  }

  #[test]
  fn loop_accumulate_object_in_a_loader() {
    in_a_loader(r#"const m = {}; for (const y of ["a", "b"]) { m[y] = y + y; } return m;"#, 0, r#"{"a":"aa","b":"bb"}"#, Refusal::Never);
  }

  #[test]
  fn loop_nested_in_a_helper() {
    in_a_helper(r#"let n = 0; for (const x of [1, 2]) { for (const y of [1, 2]) { n += x * y; } } return n;"#, 0, r#"9"#, Refusal::Never);
  }

  #[test]
  fn loop_nested_in_a_loader() {
    in_a_loader(r#"let n = 0; for (const x of [1, 2]) { for (const y of [1, 2]) { n += x * y; } } return n;"#, 0, r#"9"#, Refusal::Never);
  }

  #[test]
  fn loop_two_names_reads_in_a_helper() {
    in_a_helper(r#"let a = 0; let b = 1; for (const y of [1, 2, 3]) { const t = a + b; a = b; b = t; } return [a, b];"#, 0, r#"[2,3]"#, Refusal::Never);
  }

  #[test]
  fn loop_two_names_reads_in_a_loader() {
    in_a_loader(r#"let a = 0; let b = 1; for (const y of [1, 2, 3]) { const t = a + b; a = b; b = t; } return [a, b];"#, 0, r#"[2,3]"#, Refusal::Never);
  }

  #[test]
  fn if_merge_two_names_in_a_helper() {
    in_a_helper(r#"let a = 1; let b = 2; if (c) { a = b + 1; b = a + 1; } return [a, b];"#, 0, r#"[3,4]"#, Refusal::Never);
  }

  #[test]
  fn if_merge_two_names_in_a_loader() {
    in_a_loader(r#"let a = 1; let b = 2; if (c) { a = b + 1; b = a + 1; } return [a, b];"#, 0, r#"[3,4]"#, Refusal::Never);
  }

  #[test]
  fn if_reads_write_after_in_a_helper() {
    in_a_helper(r#"let a = 1; if (c) { a = 2; } const b = a + 1; a = 5; return [a, b];"#, 0, r#"[5,3]"#, Refusal::Never);
  }

  #[test]
  fn if_reads_write_after_in_a_loader() {
    in_a_loader(r#"let a = 1; if (c) { a = 2; } const b = a + 1; a = 5; return [a, b];"#, 0, r#"[5,3]"#, Refusal::Never);
  }

  #[test]
  fn switch_fallthrough_in_a_helper() {
    in_a_helper(r#"let a = 0; switch (1) { case 1: a += 1; case 2: a += 2; break; default: a = 9; } return a;"#, 0, r#"3"#, Refusal::Named(r#"a `case` that falls through"#));
  }

  #[test]
  fn switch_fallthrough_in_a_loader() {
    in_a_loader(r#"let a = 0; switch (1) { case 1: a += 1; case 2: a += 2; break; default: a = 9; } return a;"#, 0, r#"3"#, Refusal::Named(r#"a `case` that falls through"#));
  }

  #[test]
  fn destructure_defaults_order_in_a_helper() {
    in_a_helper(r#"const { a = 1, b = a + 1 } = {}; return [a, b];"#, 0, r#"[1,2]"#, Refusal::Never);
  }

  #[test]
  fn destructure_defaults_order_in_a_loader() {
    in_a_loader(r#"const { a = 1, b = a + 1 } = {}; return [a, b];"#, 0, r#"[1,2]"#, Refusal::Never);
  }

  #[test]
  fn destructure_nested_in_a_helper() {
    in_a_helper(r#"const { p: { q }, ...rest } = { p: { q: 1 }, z: 2 }; return [q, rest];"#, 0, r#"[1,{"z":2}]"#, Refusal::Never);
  }

  #[test]
  fn destructure_nested_in_a_loader() {
    in_a_loader(r#"const { p: { q }, ...rest } = { p: { q: 1 }, z: 2 }; return [q, rest];"#, 0, r#"[1,{"z":2}]"#, Refusal::Never);
  }

  #[test]
  fn object_method_const_in_a_helper() {
    in_a_helper(r#"const repo = { find: (id) => id + 1 }; return repo.find(1);"#, 0, r#"2"#, Refusal::Never);
  }

  #[test]
  fn object_method_const_in_a_loader() {
    in_a_loader(r#"const repo = { find: (id) => id + 1 }; return repo.find(1);"#, 0, r#"2"#, Refusal::Never);
  }

  #[test]
  fn object_computed_method_in_a_helper() {
    in_a_helper(r#"const ops = { add: (x) => x + 1 }; const key = "add"; return ops[key](1);"#, 0, r#"2"#, Refusal::Never);
  }

  #[test]
  fn object_computed_method_in_a_loader() {
    in_a_loader(r#"const ops = { add: (x) => x + 1 }; const key = "add"; return ops[key](1);"#, 0, r#"2"#, Refusal::Never);
  }

  #[test]
  fn array_of_fns_in_a_helper() {
    in_a_helper(r#"const fs = [(x) => x + 1]; return fs[0](1);"#, 0, r#"2"#, Refusal::Never);
  }

  #[test]
  fn array_of_fns_in_a_loader() {
    in_a_loader(r#"const fs = [(x) => x + 1]; return fs[0](1);"#, 0, r#"2"#, Refusal::Never);
  }

  #[test]
  fn fn_in_ternary_then_call_in_a_helper() {
    in_a_helper(r#"const g = c ? (x) => x + 1 : (x) => x; return g(1);"#, 0, r#"2"#, Refusal::Never);
  }

  #[test]
  fn fn_in_ternary_then_call_in_a_loader() {
    in_a_loader(r#"const g = c ? (x) => x + 1 : (x) => x; return g(1);"#, 0, r#"2"#, Refusal::Never);
  }

  #[test]
  fn map_write_read_in_a_helper() {
    in_a_helper(r#"const m = new Map(); m.set("a", 1); m.set("b", 2); return [...m.keys()];"#, 0, r#"["a","b"]"#, Refusal::Never);
  }

  #[test]
  fn map_write_read_in_a_loader() {
    in_a_loader(r#"const m = new Map(); m.set("a", 1); m.set("b", 2); return [...m.keys()];"#, 0, r#"["a","b"]"#, Refusal::Never);
  }

  #[test]
  fn delete_field_in_a_helper() {
    in_a_helper(r#"const o = { a: 1, b: 2 }; delete o.a; return o;"#, 0, r#"{"b":2}"#, Refusal::Never);
  }

  #[test]
  fn delete_field_in_a_loader() {
    in_a_loader(r#"const o = { a: 1, b: 2 }; delete o.a; return o;"#, 0, r#"{"b":2}"#, Refusal::Never);
  }

  #[test]
  fn early_return_in_loop_free_if_in_a_helper() {
    in_a_helper(r#"let a = 0; if (c) { a = 1; if (a === 1) { return a + 1; } } return a;"#, 0, r#"2"#, Refusal::Never);
  }

  #[test]
  fn early_return_in_loop_free_if_in_a_loader() {
    in_a_loader(r#"let a = 0; if (c) { a = 1; if (a === 1) { return a + 1; } } return a;"#, 0, r#"2"#, Refusal::Never);
  }

  #[test]
  fn param_default_in_a_helper() {
    in_a_helper(r#"const g = (x = u()) => x === x; return g();"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn param_default_in_a_loader() {
    in_a_loader(r#"const g = (x = u()) => x === x; return g();"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn param_object_default_in_a_helper() {
    in_a_helper(r#"const g = ({ x = u() }) => x === x; return g({});"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn param_object_default_in_a_loader() {
    in_a_loader(r#"const g = ({ x = u() }) => x === x; return g({});"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn param_array_default_in_a_helper() {
    in_a_helper(r#"const g = ([x = u()]) => x === x; return g([]);"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn param_array_default_in_a_loader() {
    in_a_loader(r#"const g = ([x = u()]) => x === x; return g([]);"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn param_default_per_call_in_a_helper() {
    in_a_helper(r#"const g = (x = u()) => x; return g() !== g();"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn param_default_per_call_in_a_loader() {
    in_a_loader(r#"const g = (x = u()) => x; return g() !== g();"#, 1, r#"true"#, Refusal::Never);
  }

  #[test]
  fn try_effect_only_call_in_a_helper() {
    in_a_helper(r#"let a = 0; try { JSON.parse("{"); a = 1; } catch (e) { a = 2; } return a;"#, 0, r#"2"#, Refusal::Never);
  }

  #[test]
  fn try_effect_only_call_in_a_loader() {
    in_a_loader(r#"let a = 0; try { JSON.parse("{"); a = 1; } catch (e) { a = 2; } return a;"#, 0, r#"2"#, Refusal::Never);
  }

  #[test]
  fn component_const_in_a_page() {
    in_a_page(r#"export default function P() { const a = u(); return <p title={a}>{a}</p>; }"#);
  }

  #[test]
  fn component_const_nested_in_a_page() {
    in_a_page(r#"export default function P() { const a = u(); const b = a + a; return <p title={b}>{b}</p>; }"#);
  }

  #[test]
  fn component_helper_in_a_page() {
    in_a_page(r#"const h = () => { const k = u(); return k + k; }; export default function P() { const a = h(); return <p title={a}>{a}</p>; }"#);
  }

  #[test]
  fn component_destructure_in_a_page() {
    in_a_page(r#"export default function P() { const { a } = { a: u() }; return <p title={a}>{a}</p>; }"#);
  }

  #[test]
  fn custom_hook_argument_in_a_page() {
    in_a_page(r#"function useH(x) { const y = x; return [y, y]; } export default function P() { const [a, b] = useH(u()); return <p title={a}>{b}</p>; }"#);
  }

  #[test]
  fn custom_hook_const_in_a_page() {
    in_a_page(r#"function useH() { const y = u(); return [y, y]; } export default function P() { const [a, b] = useH(); return <p title={a}>{b}</p>; }"#);
  }

  #[test]
  fn use_memo_const_in_a_page() {
    in_a_page(r#"import { useMemo } from "react"; export default function P() { const a = useMemo(() => { const k = u(); return k + k; }, []); return <p title={a}>{a}</p>; }"#);
  }

  #[test]
  fn use_state_const_in_a_page() {
    in_a_page(r#"import { useState } from "react"; export default function P() { const [a, set] = useState(() => { const k = u(); return k + k; }); return <button title={a} onClick={() => set("x")}>{a}</button>; }"#);
  }

  #[test]
  fn handler_const_two_sets_in_a_page() {
    in_a_page(r#"import { useState } from "react"; export default function P() { const [a, setA] = useState(""); const [b, setB] = useState(""); return <button title={a} onClick={() => { const k = u(); setA(k); setB(k); }}>{b}</button>; }"#);
  }

  #[test]
  fn handler_if_const_in_a_page() {
    in_a_page(r#"import { useState } from "react"; export default function P() { const [a, setA] = useState(""); const [b, setB] = useState(""); return <button title={a} onClick={() => { if (a === "") { const k = u(); setA(k); setB(k); } }}>{b}</button>; }"#);
  }

  #[test]
  fn handler_let_reassign_in_a_page() {
    in_a_page(r#"import { useState } from "react"; export default function P() { const [a, setA] = useState(""); const [b, setB] = useState(""); return <button title={a} onClick={() => { let k = ""; k = u(); setA(k); setB(k); }}>{b}</button>; }"#);
  }

  #[test]
  fn map_in_markup_in_a_page() {
    in_a_page(r#"export default function P() { const xs = [1].map(() => u()); return <ul>{xs.map((x) => <li key={x}>{x}</li>)}{xs.length}</ul>; }"#);
  }
}

mod one_variable {
  use super::*;

  #[test]
  fn closure_sees_later_write_in_a_helper() {
    in_a_helper(r#"let a = 1; const g = () => a; a = 2; return g();"#, 0, r#"2"#, Refusal::Never);
  }

  #[test]
  fn closure_sees_later_write_in_a_loader() {
    in_a_loader(r#"let a = 1; const g = () => a; a = 2; return g();"#, 0, r#"2"#, Refusal::Never);
  }

  #[test]
  fn try_in_loop_in_a_helper() {
    in_a_helper(r#"let a = 0; for (const y of [1, 2]) { try { if (y === 1) { JSON.parse("{"); } a += y; } catch (e) { a += 10; } } return a;"#, 0, r#"12"#, Refusal::Never);
  }

  #[test]
  fn try_in_loop_in_a_loader() {
    in_a_loader(r#"let a = 0; for (const y of [1, 2]) { try { if (y === 1) { JSON.parse("{"); } a += y; } catch (e) { a += 10; } } return a;"#, 0, r#"12"#, Refusal::Never);
  }

  #[test]
  fn try_in_loop_no_if_in_a_helper() {
    in_a_helper(r#"let a = 0; for (const y of [1, 2]) { try { a += y; } catch (e) { a += 10; } } return a;"#, 0, r#"3"#, Refusal::Never);
  }

  #[test]
  fn try_in_loop_no_if_in_a_loader() {
    in_a_loader(r#"let a = 0; for (const y of [1, 2]) { try { a += y; } catch (e) { a += 10; } } return a;"#, 0, r#"3"#, Refusal::Never);
  }

  #[test]
  fn try_in_loop_throw_in_a_helper() {
    in_a_helper(r#"let a = 0; for (const y of [1]) { try { JSON.parse("{"); a += y; } catch (e) { a += 10; } } return a;"#, 0, r#"10"#, Refusal::Never);
  }

  #[test]
  fn try_in_loop_throw_in_a_loader() {
    in_a_loader(r#"let a = 0; for (const y of [1]) { try { JSON.parse("{"); a += y; } catch (e) { a += 10; } } return a;"#, 0, r#"10"#, Refusal::Never);
  }

  #[test]
  fn try_in_loop_if_no_throw_in_a_helper() {
    in_a_helper(r#"let a = 0; for (const y of [1, 2]) { try { if (y === 9) { a = 100; } a += y; } catch (e) { a += 10; } } return a;"#, 0, r#"3"#, Refusal::Never);
  }

  #[test]
  fn try_in_loop_if_no_throw_in_a_loader() {
    in_a_loader(r#"let a = 0; for (const y of [1, 2]) { try { if (y === 9) { a = 100; } a += y; } catch (e) { a += 10; } } return a;"#, 0, r#"3"#, Refusal::Never);
  }

  #[test]
  fn closure_reads_after_loop_in_a_helper() {
    in_a_helper(r#"let a = 0; const g = () => a; for (const y of [1, 2]) { a += y; } return g();"#, 0, r#"3"#, Refusal::Never);
  }

  #[test]
  fn closure_reads_after_loop_in_a_loader() {
    in_a_loader(r#"let a = 0; const g = () => a; for (const y of [1, 2]) { a += y; } return g();"#, 0, r#"3"#, Refusal::Never);
  }

  #[test]
  fn closure_reads_after_if_in_a_helper() {
    in_a_helper(r#"let a = 0; const g = () => a; if (c) { a = 1; } return g();"#, 0, r#"1"#, Refusal::Never);
  }

  #[test]
  fn closure_reads_after_if_in_a_loader() {
    in_a_loader(r#"let a = 0; const g = () => a; if (c) { a = 1; } return g();"#, 0, r#"1"#, Refusal::Never);
  }

  #[test]
  fn closure_before_field_write_in_a_helper() {
    in_a_helper(r#"const o = { a: 1 }; const g = () => o.a; o.a = 2; return g();"#, 0, r#"2"#, Refusal::Never);
  }

  #[test]
  fn closure_before_field_write_in_a_loader() {
    in_a_loader(r#"const o = { a: 1 }; const g = () => o.a; o.a = 2; return g();"#, 0, r#"2"#, Refusal::Never);
  }

  #[test]
  fn closure_before_push_in_a_helper() {
    in_a_helper(r#"const xs = []; const g = () => xs.length; xs.push(1); return g();"#, 0, r#"1"#, Refusal::Never);
  }

  #[test]
  fn closure_before_push_in_a_loader() {
    in_a_loader(r#"const xs = []; const g = () => xs.length; xs.push(1); return g();"#, 0, r#"1"#, Refusal::Never);
  }
}

mod one_identity {
  use super::*;

  #[test]
  fn alias_object_write_in_a_helper() {
    in_a_helper(r#"const o = { a: 1 }; const p = o; p.a = 2; return o.a;"#, 0, r#"2"#, Refusal::Never);
  }

  #[test]
  fn alias_object_write_in_a_loader() {
    in_a_loader(r#"const o = { a: 1 }; const p = o; p.a = 2; return o.a;"#, 0, r#"2"#, Refusal::Never);
  }

  #[test]
  fn alias_array_push_in_a_helper() {
    in_a_helper(r#"const xs = []; const ys = xs; ys.push(1); return xs.length;"#, 0, r#"1"#, Refusal::Never);
  }

  #[test]
  fn alias_array_push_in_a_loader() {
    in_a_loader(r#"const xs = []; const ys = xs; ys.push(1); return xs.length;"#, 0, r#"1"#, Refusal::Never);
  }

  #[test]
  fn alias_field_array_push_in_a_helper() {
    in_a_helper(r#"const o = { xs: [] }; const ys = o.xs; ys.push(1); return o.xs.length;"#, 0, r#"1"#, Refusal::Never);
  }

  #[test]
  fn alias_field_array_push_in_a_loader() {
    in_a_loader(r#"const o = { xs: [] }; const ys = o.xs; ys.push(1); return o.xs.length;"#, 0, r#"1"#, Refusal::Never);
  }

  #[test]
  fn sort_in_place_in_a_helper() {
    in_a_helper(r#"const xs = [2, 1]; xs.sort(); return xs;"#, 0, r#"[1,2]"#, Refusal::Never);
  }

  #[test]
  fn sort_in_place_in_a_loader() {
    in_a_loader(r#"const xs = [2, 1]; xs.sort(); return xs;"#, 0, r#"[1,2]"#, Refusal::Never);
  }

  #[test]
  fn reverse_in_place_in_a_helper() {
    in_a_helper(r#"const xs = [1, 2]; xs.reverse(); return xs;"#, 0, r#"[2,1]"#, Refusal::Never);
  }

  #[test]
  fn reverse_in_place_in_a_loader() {
    in_a_loader(r#"const xs = [1, 2]; xs.reverse(); return xs;"#, 0, r#"[2,1]"#, Refusal::Never);
  }

  #[test]
  fn pop_in_place_in_a_helper() {
    in_a_helper(r#"const xs = [1, 2]; xs.pop(); return xs;"#, 0, r#"[1]"#, Refusal::Never);
  }

  #[test]
  fn pop_in_place_in_a_loader() {
    in_a_loader(r#"const xs = [1, 2]; xs.pop(); return xs;"#, 0, r#"[1]"#, Refusal::Never);
  }

  #[test]
  fn alias_then_read_alias_in_a_helper() {
    in_a_helper(r#"const o = { a: 1 }; const p = o; o.a = 2; return p.a;"#, 0, r#"2"#, Refusal::Never);
  }

  #[test]
  fn alias_then_read_alias_in_a_loader() {
    in_a_loader(r#"const o = { a: 1 }; const p = o; o.a = 2; return p.a;"#, 0, r#"2"#, Refusal::Never);
  }

  #[test]
  fn alias_param_write_in_a_helper() {
    in_a_helper(r#"const w = (o) => { o.a = 2; }; const o = { a: 1 }; w(o); return o.a;"#, 0, r#"2"#, Refusal::Never);
  }

  #[test]
  fn alias_param_write_in_a_loader() {
    in_a_loader(r#"const w = (o) => { o.a = 2; }; const o = { a: 1 }; w(o); return o.a;"#, 0, r#"2"#, Refusal::Never);
  }

  #[test]
  fn alias_param_push_in_a_helper() {
    in_a_helper(r#"const w = (xs) => { xs.push(1); }; const xs = []; w(xs); return xs.length;"#, 0, r#"1"#, Refusal::Never);
  }

  #[test]
  fn alias_param_push_in_a_loader() {
    in_a_loader(r#"const w = (xs) => { xs.push(1); }; const xs = []; w(xs); return xs.length;"#, 0, r#"1"#, Refusal::Never);
  }

  #[test]
  fn alias_in_array_in_a_helper() {
    in_a_helper(r#"const o = { a: 1 }; const l = [o]; l[0].a = 2; return o.a;"#, 0, r#"2"#, Refusal::Never);
  }

  #[test]
  fn alias_in_array_in_a_loader() {
    in_a_loader(r#"const o = { a: 1 }; const l = [o]; l[0].a = 2; return o.a;"#, 0, r#"2"#, Refusal::Never);
  }

  #[test]
  fn alias_in_object_in_a_helper() {
    in_a_helper(r#"const o = { a: 1 }; const h = { o }; h.o.a = 2; return o.a;"#, 0, r#"2"#, Refusal::Never);
  }

  #[test]
  fn alias_in_object_in_a_loader() {
    in_a_loader(r#"const o = { a: 1 }; const h = { o }; h.o.a = 2; return o.a;"#, 0, r#"2"#, Refusal::Never);
  }

  #[test]
  fn alias_loop_item_in_a_helper() {
    in_a_helper(r#"const l = [{ a: 1 }]; for (const i of l) { i.a = 2; } return l[0].a;"#, 0, r#"2"#, Refusal::Never);
  }

  #[test]
  fn alias_loop_item_in_a_loader() {
    in_a_loader(r#"const l = [{ a: 1 }]; for (const i of l) { i.a = 2; } return l[0].a;"#, 0, r#"2"#, Refusal::Never);
  }

  #[test]
  fn alias_map_item_in_a_helper() {
    in_a_helper(r#"const m = new Map(); const o = { a: 1 }; m.set("o", o); o.a = 2; return m.get("o").a;"#, 0, r#"2"#, Refusal::Never);
  }

  #[test]
  fn alias_map_item_in_a_loader() {
    in_a_loader(r#"const m = new Map(); const o = { a: 1 }; m.set("o", o); o.a = 2; return m.get("o").a;"#, 0, r#"2"#, Refusal::Never);
  }

  #[test]
  fn return_shared_object_in_a_helper() {
    in_a_helper(r#"const mk = () => { const o = { n: 0 }; return { o, inc: () => { o.n += 1; return o.n; } }; }; const r = mk(); r.inc(); return r.inc();"#, 0, r#"2"#, Refusal::Never);
  }

  #[test]
  fn return_shared_object_in_a_loader() {
    in_a_loader(r#"const mk = () => { const o = { n: 0 }; return { o, inc: () => { o.n += 1; return o.n; } }; }; const r = mk(); r.inc(); return r.inc();"#, 0, r#"2"#, Refusal::Never);
  }

  #[test]
  fn self_equal_in_a_helper() {
    in_a_helper(r#"const o = {}; return o === o;"#, 0, r#"true"#, Refusal::Never);
  }

  #[test]
  fn self_equal_in_a_loader() {
    in_a_loader(r#"const o = {}; return o === o;"#, 0, r#"true"#, Refusal::Never);
  }

  #[test]
  fn two_literals_differ_in_a_helper() {
    in_a_helper(r#"return ({}) === ({});"#, 0, r#"false"#, Refusal::Never);
  }

  #[test]
  fn two_literals_differ_in_a_loader() {
    in_a_loader(r#"return ({}) === ({});"#, 0, r#"false"#, Refusal::Never);
  }

  #[test]
  fn two_objects_same_shape_differ_in_a_helper() {
    in_a_helper(r#"const a = { x: 1 }; const b = { x: 1 }; return a === b;"#, 0, r#"false"#, Refusal::Never);
  }

  #[test]
  fn two_objects_same_shape_differ_in_a_loader() {
    in_a_loader(r#"const a = { x: 1 }; const b = { x: 1 }; return a === b;"#, 0, r#"false"#, Refusal::Never);
  }

  #[test]
  fn alias_equal_in_a_helper() {
    in_a_helper(r#"const a = { x: 1 }; const b = a; return a === b;"#, 0, r#"true"#, Refusal::Never);
  }

  #[test]
  fn alias_equal_in_a_loader() {
    in_a_loader(r#"const a = { x: 1 }; const b = a; return a === b;"#, 0, r#"true"#, Refusal::Never);
  }

  #[test]
  fn two_arrays_differ_in_a_helper() {
    in_a_helper(r#"const a = [1]; const b = [1]; return a === b;"#, 0, r#"false"#, Refusal::Never);
  }

  #[test]
  fn two_arrays_differ_in_a_loader() {
    in_a_loader(r#"const a = [1]; const b = [1]; return a === b;"#, 0, r#"false"#, Refusal::Never);
  }

  #[test]
  fn includes_object_in_a_helper() {
    in_a_helper(r#"const o = { x: 1 }; return [o].includes(o);"#, 0, r#"true"#, Refusal::Never);
  }

  #[test]
  fn includes_object_in_a_loader() {
    in_a_loader(r#"const o = { x: 1 }; return [o].includes(o);"#, 0, r#"true"#, Refusal::Never);
  }

  #[test]
  fn includes_same_shape_in_a_helper() {
    in_a_helper(r#"const l = [{ x: 1 }]; return l.includes({ x: 1 });"#, 0, r#"false"#, Refusal::Never);
  }

  #[test]
  fn includes_same_shape_in_a_loader() {
    in_a_loader(r#"const l = [{ x: 1 }]; return l.includes({ x: 1 });"#, 0, r#"false"#, Refusal::Never);
  }

  #[test]
  fn indexOf_same_shape_in_a_helper() {
    in_a_helper(r#"const l = [{ x: 1 }]; return l.indexOf({ x: 1 });"#, 0, r#"-1"#, Refusal::Never);
  }

  #[test]
  fn indexOf_same_shape_in_a_loader() {
    in_a_loader(r#"const l = [{ x: 1 }]; return l.indexOf({ x: 1 });"#, 0, r#"-1"#, Refusal::Never);
  }

  #[test]
  fn set_dedupes_by_identity_in_a_helper() {
    in_a_helper(r#"const s = new Set([{ x: 1 }, { x: 1 }]); return s.size;"#, 0, r#"2"#, Refusal::Never);
  }

  #[test]
  fn set_dedupes_by_identity_in_a_loader() {
    in_a_loader(r#"const s = new Set([{ x: 1 }, { x: 1 }]); return s.size;"#, 0, r#"2"#, Refusal::Never);
  }

  #[test]
  fn set_same_object_once_in_a_helper() {
    in_a_helper(r#"const o = { x: 1 }; const s = new Set([o, o]); return s.size;"#, 0, r#"1"#, Refusal::Never);
  }

  #[test]
  fn set_same_object_once_in_a_loader() {
    in_a_loader(r#"const o = { x: 1 }; const s = new Set([o, o]); return s.size;"#, 0, r#"1"#, Refusal::Never);
  }

  #[test]
  fn map_object_key_differs_in_a_helper() {
    in_a_helper(r#"const m = new Map(); m.set({ x: 1 }, 1); return m.get({ x: 1 }) === undefined;"#, 0, r#"true"#, Refusal::Never);
  }

  #[test]
  fn map_object_key_differs_in_a_loader() {
    in_a_loader(r#"const m = new Map(); m.set({ x: 1 }, 1); return m.get({ x: 1 }) === undefined;"#, 0, r#"true"#, Refusal::Never);
  }

  #[test]
  fn map_object_key_same_in_a_helper() {
    in_a_helper(r#"const o = { x: 1 }; const m = new Map(); m.set(o, 1); return m.get(o);"#, 0, r#"1"#, Refusal::Never);
  }

  #[test]
  fn map_object_key_same_in_a_loader() {
    in_a_loader(r#"const o = { x: 1 }; const m = new Map(); m.set(o, 1); return m.get(o);"#, 0, r#"1"#, Refusal::Never);
  }

  #[test]
  fn spread_is_a_copy_in_a_helper() {
    in_a_helper(r#"const o = { a: 1 }; const p = { ...o }; p.a = 2; return [o.a, p.a];"#, 0, r#"[1,2]"#, Refusal::Never);
  }

  #[test]
  fn spread_is_a_copy_in_a_loader() {
    in_a_loader(r#"const o = { a: 1 }; const p = { ...o }; p.a = 2; return [o.a, p.a];"#, 0, r#"[1,2]"#, Refusal::Never);
  }

  #[test]
  fn array_spread_is_a_copy_in_a_helper() {
    in_a_helper(r#"const a = [1]; const b = [...a]; b.push(2); return [a.length, b.length];"#, 0, r#"[1,2]"#, Refusal::Never);
  }

  #[test]
  fn array_spread_is_a_copy_in_a_loader() {
    in_a_loader(r#"const a = [1]; const b = [...a]; b.push(2); return [a.length, b.length];"#, 0, r#"[1,2]"#, Refusal::Never);
  }

  #[test]
  fn map_callback_mutates_item_in_a_helper() {
    in_a_helper(r#"const l = [{ a: 1 }]; l.map((i) => { i.a = 2; return i; }); return l[0].a;"#, 0, r#"2"#, Refusal::Never);
  }

  #[test]
  fn map_callback_mutates_item_in_a_loader() {
    in_a_loader(r#"const l = [{ a: 1 }]; l.map((i) => { i.a = 2; return i; }); return l[0].a;"#, 0, r#"2"#, Refusal::Never);
  }

  #[test]
  fn foreach_mutates_item_in_a_helper() {
    in_a_helper(r#"const l = [{ a: 1 }]; l.forEach((i) => { i.a = 2; }); return l[0].a;"#, 0, r#"2"#, Refusal::Never);
  }

  #[test]
  fn foreach_mutates_item_in_a_loader() {
    in_a_loader(r#"const l = [{ a: 1 }]; l.forEach((i) => { i.a = 2; }); return l[0].a;"#, 0, r#"2"#, Refusal::Never);
  }

  #[test]
  fn sort_returns_same_array_in_a_helper() {
    in_a_helper(r#"const a = [2, 1]; const b = a.sort(); return a === b;"#, 0, r#"true"#, Refusal::Never);
  }

  #[test]
  fn sort_returns_same_array_in_a_loader() {
    in_a_loader(r#"const a = [2, 1]; const b = a.sort(); return a === b;"#, 0, r#"true"#, Refusal::Never);
  }

  #[test]
  fn filter_shares_items_in_a_helper() {
    in_a_helper(r#"const l = [{ a: 1 }]; const f = l.filter(() => true); f[0].a = 2; return l[0].a;"#, 0, r#"2"#, Refusal::Never);
  }

  #[test]
  fn filter_shares_items_in_a_loader() {
    in_a_loader(r#"const l = [{ a: 1 }]; const f = l.filter(() => true); f[0].a = 2; return l[0].a;"#, 0, r#"2"#, Refusal::Never);
  }

  #[test]
  fn object_values_share_items_in_a_helper() {
    in_a_helper(r#"const o = { p: { a: 1 } }; const v = Object.values(o); v[0].a = 2; return o.p.a;"#, 0, r#"2"#, Refusal::Never);
  }

  #[test]
  fn object_values_share_items_in_a_loader() {
    in_a_loader(r#"const o = { p: { a: 1 } }; const v = Object.values(o); v[0].a = 2; return o.p.a;"#, 0, r#"2"#, Refusal::Never);
  }

  #[test]
  fn object_assign_mutates_target_in_a_helper() {
    in_a_helper(r#"const o = { a: 1 }; Object.assign(o, { b: 2 }); return o;"#, 0, r#"{"a":1,"b":2}"#, Refusal::Never);
  }

  #[test]
  fn object_assign_mutates_target_in_a_loader() {
    in_a_loader(r#"const o = { a: 1 }; Object.assign(o, { b: 2 }); return o;"#, 0, r#"{"a":1,"b":2}"#, Refusal::Never);
  }

  #[test]
  fn return_object_then_mutate_copy_in_a_helper() {
    in_a_helper(r#"const mk = () => ({ a: 1 }); const x = mk(); const y = mk(); x.a = 2; return y.a;"#, 0, r#"1"#, Refusal::Never);
  }

  #[test]
  fn return_object_then_mutate_copy_in_a_loader() {
    in_a_loader(r#"const mk = () => ({ a: 1 }); const x = mk(); const y = mk(); x.a = 2; return y.a;"#, 0, r#"1"#, Refusal::Never);
  }

  #[test]
  fn cycle_in_a_helper() {
    in_a_helper(r#"const o = {}; o.self = o; return o.self === o;"#, 0, r#"true"#, Refusal::Never);
  }

  #[test]
  fn cycle_in_a_loader() {
    in_a_loader(r#"const o = {}; o.self = o; return o.self === o;"#, 0, r#"true"#, Refusal::Never);
  }
}

mod loops {
  use super::*;

  const CONSTANT: &str = "const T = { inner: { n: 1 }, list: [{ k: 1 }, { k: 2 }] };\n";

  #[test]
  fn break_leaves_the_loop_in_a_helper() {
    in_a_helper(r#"let n = 0; for (const y of [1, 2, 3]) { if (y === 2) break; n += y; } return n;"#, 0, r#"1"#, Refusal::Never);
  }

  #[test]
  fn break_leaves_the_loop_in_a_loader() {
    in_a_loader(r#"let n = 0; for (const y of [1, 2, 3]) { if (y === 2) break; n += y; } return n;"#, 0, r#"1"#, Refusal::Never);
  }

  #[test]
  fn break_leaves_a_counted_loop_in_a_helper() {
    in_a_helper(r#"let n = 0; for (let i = 0; i < 5; i++) { if (i === 3) break; n += i; } return n;"#, 0, r#"3"#, Refusal::Never);
  }

  #[test]
  fn break_leaves_a_counted_loop_in_a_loader() {
    in_a_loader(r#"let n = 0; for (let i = 0; i < 5; i++) { if (i === 3) break; n += i; } return n;"#, 0, r#"3"#, Refusal::Never);
  }

  #[test]
  fn break_ends_a_switch_arm_in_a_loop_in_a_helper() {
    in_a_helper(r#"let n = 0; for (const y of [1, 2, 3]) { switch (y) { case 2: n += 10; break; default: n += y; } } return n;"#, 0, r#"14"#, Refusal::Never);
  }

  #[test]
  fn break_ends_a_switch_arm_in_a_loop_in_a_loader() {
    in_a_loader(r#"let n = 0; for (const y of [1, 2, 3]) { switch (y) { case 2: n += 10; break; default: n += y; } } return n;"#, 0, r#"14"#, Refusal::Never);
  }

  #[test]
  fn continue_inside_a_switch_goes_to_the_next_item_in_a_helper() {
    in_a_helper(r#"let n = 0; for (const y of [1, 2, 3]) { switch (y) { case 2: continue; default: n += y; } n += 100; } return n;"#, 0, r#"204"#, Refusal::Never);
  }

  #[test]
  fn continue_inside_a_switch_goes_to_the_next_item_in_a_loader() {
    in_a_loader(r#"let n = 0; for (const y of [1, 2, 3]) { switch (y) { case 2: continue; default: n += y; } n += 100; } return n;"#, 0, r#"204"#, Refusal::Never);
  }

  #[test]
  fn break_in_a_nested_loop_leaves_the_inner_one_in_a_helper() {
    in_a_helper(r#"let n = 0; for (const a of [1, 2]) { for (const b of [1, 2, 3]) { if (b === 2) break; n += a * b; } } return n;"#, 0, r#"3"#, Refusal::Never);
  }

  #[test]
  fn break_in_a_nested_loop_leaves_the_inner_one_in_a_loader() {
    in_a_loader(r#"let n = 0; for (const a of [1, 2]) { for (const b of [1, 2, 3]) { if (b === 2) break; n += a * b; } } return n;"#, 0, r#"3"#, Refusal::Never);
  }

  #[test]
  fn counted_start_runs_once_in_a_helper() {
    in_a_helper(r#"let k = []; let s = 0; for (let i = k.push(1); i < 3; i++) { s += i; } return [s, k.length];"#, 0, r#"[3,1]"#, Refusal::Never);
  }

  #[test]
  fn counted_start_runs_once_in_a_loader() {
    in_a_loader(r#"let k = []; let s = 0; for (let i = k.push(1); i < 3; i++) { s += i; } return [s, k.length];"#, 0, r#"[3,1]"#, Refusal::Never);
  }

  #[test]
  fn local_function_calls_itself_in_a_helper() {
    in_a_helper(r#"function fact(k) { return k <= 1 ? 1 : k * fact(k - 1); } return fact(5);"#, 0, r#"120"#, Refusal::Never);
  }

  #[test]
  fn local_function_calls_itself_in_a_loader() {
    in_a_loader(r#"function fact(k) { return k <= 1 ? 1 : k * fact(k - 1); } return fact(5);"#, 0, r#"120"#, Refusal::Never);
  }

  #[test]
  fn local_function_called_above_its_declaration_in_a_helper() {
    in_a_helper(r#"const r = g(2); function g(x) { return x * 3; } return r;"#, 0, r#"6"#, Refusal::Never);
  }

  #[test]
  fn local_function_called_above_its_declaration_in_a_loader() {
    in_a_loader(r#"const r = g(2); function g(x) { return x * 3; } return r;"#, 0, r#"6"#, Refusal::Never);
  }

  #[test]
  fn return_from_a_counted_loop_in_a_helper() {
    in_a_helper(r#"for (let i = 0; i < 9; i++) { if (i * i > 10) return i; } return -1;"#, 0, r#"4"#, Refusal::Never);
  }

  #[test]
  fn return_from_a_counted_loop_in_a_loader() {
    in_a_loader(r#"for (let i = 0; i < 9; i++) { if (i * i > 10) return i; } return -1;"#, 0, r#"4"#, Refusal::Never);
  }

  #[test]
  fn a_counted_bound_is_tested_every_pass_in_a_helper() {
    in_a_helper(r#"const s = { k: 3 }; const lim = () => { s.k -= 1; return s.k; }; let n = 0; for (let i = 0; i < lim(); i++) n += 1; return [n, s.k];"#, 0, r#"[1,1]"#, Refusal::Never);
  }

  #[test]
  fn a_counted_bound_is_tested_every_pass_in_a_loader() {
    in_a_loader(r#"const s = { k: 3 }; const lim = () => { s.k -= 1; return s.k; }; let n = 0; for (let i = 0; i < lim(); i++) n += 1; return [n, s.k];"#, 0, r#"[1,1]"#, Refusal::Never);
  }

  #[test]
  fn a_continue_still_steps_and_tests_the_bound_in_a_helper() {
    in_a_helper(r#"const s = { k: 0 }; const lim = () => { s.k += 1; return 3; }; let n = 0; for (let i = 0; i < lim(); i++) { if (i === 1) continue; n += i; } return [n, s.k];"#, 0, r#"[2,4]"#, Refusal::Never);
  }

  #[test]
  fn a_continue_still_steps_and_tests_the_bound_in_a_loader() {
    in_a_loader(r#"const s = { k: 0 }; const lim = () => { s.k += 1; return 3; }; let n = 0; for (let i = 0; i < lim(); i++) { if (i === 1) continue; n += i; } return [n, s.k];"#, 0, r#"[2,4]"#, Refusal::Never);
  }

  #[test]
  fn a_part_read_before_a_write_through_the_root_sees_it_in_a_loader() {
    agrees(format!("{PRELUDE}{CONSTANT}export async function load() {{\n  {}\n}}\n", r#"const a = T.inner; T.inner.n = 2; return a.n;"#), 0, r#"2"#, Refusal::Never);
  }

  #[test]
  fn a_write_through_a_part_reaches_the_root_in_a_loader() {
    agrees(format!("{PRELUDE}{CONSTANT}export async function load() {{\n  {}\n}}\n", r#"const a = T.inner; a.n = 3; return T.inner.n;"#), 0, r#"3"#, Refusal::Never);
  }

  #[test]
  fn an_item_found_before_a_write_is_the_one_written_in_a_loader() {
    agrees(format!("{PRELUDE}{CONSTANT}export async function load() {{\n  {}\n}}\n", r#"const first = T.list[0]; T.list[0].k = 9; return [first.k, T.list.includes(first)];"#), 0, r#"[9,true]"#, Refusal::Never);
  }

  #[test]
  fn two_reads_of_a_constant_are_one_object_after_a_write_in_a_loader() {
    agrees(format!("{PRELUDE}{CONSTANT}export async function load() {{\n  {}\n}}\n", r#"const a = T.inner; a.n = 4; const b = T.inner; return [a === b, b.n];"#), 0, r#"[true,4]"#, Refusal::Never);
  }
}
