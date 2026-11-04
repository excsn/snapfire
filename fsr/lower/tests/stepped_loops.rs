//! A counted `for` that steps by more than one, in a helper a loader calls.

use snapfire_fsr_core::Value;
use snapfire_fsr_ir::Interpreter;

fn loaded(helper: &str) -> Result<Value, String> {
  let loader = format!("{helper}\nexport async function load() {{\n  return f();\n}}\n");
  let body = snapfire_fsr_lower::lower_loader("page.loader.ts", &loader).map_err(|e| e.to_string())?;
  let ctx = snapfire_fsr_runtime::RequestCtx::anonymous(Default::default());
  futures::executor::block_on(Interpreter::default().run(&body, &ctx, None)).map(|outcome| outcome.value).map_err(|e| e.message)
}

fn numbers(items: &[f64]) -> Value {
  Value::Seq(items.iter().map(|n| Value::F64(*n)).collect())
}

#[test]
fn a_counted_loop_steps_by_any_amount_its_body_does_not_write() {
  let cases = [
    ("for (let i = 0; i < 10; i += 3) out.push(i);", vec![0.0, 3.0, 6.0, 9.0]),
    ("for (let i = 0; i <= 9; i += 3) out.push(i);", vec![0.0, 3.0, 6.0, 9.0]),
    ("for (let i = 1; i < 2; i += 5) out.push(i);", vec![1.0]),
    ("for (let i = 5; i < 2; i += 2) out.push(i);", vec![]),
    ("const s = 4; for (let i = 2; i <= 10; i += s) out.push(i);", vec![2.0, 6.0, 10.0]),
  ];
  for (code, expected) in cases {
    let helper = format!("function f() {{\n  const out = [];\n  {code}\n  return out;\n}}");
    assert_eq!(loaded(&helper), Ok(numbers(&expected)), "{code}");
  }
}

#[test]
fn a_sieve_whose_inner_loop_steps_by_the_outer_counter_finds_the_primes() {
  let helper = "function f() {\n  const n = 30;\n  const composite = [];\n  for (let i = 0; i <= n; i++) composite.push(false);\n  const found = [];\n  for (let i = 2; i <= n; i++) {\n    if (composite[i]) continue;\n    found.push(i);\n    for (let j = i * i; j <= n; j += i) composite[j] = true;\n  }\n  return found;\n}";
  assert_eq!(loaded(helper), Ok(numbers(&[2.0, 3.0, 5.0, 7.0, 11.0, 13.0, 17.0, 19.0, 23.0, 29.0])));
}

#[test]
fn a_step_that_is_not_positive_fails_the_request_rather_than_running_for_ever() {
  let helper = "function f() {\n  const s = 0;\n  const out = [];\n  for (let i = 0; i < 3; i += s) out.push(i);\n  return out;\n}";
  let failed = loaded(helper).unwrap_err();
  assert!(failed.contains("would build"), "{failed}");
}
