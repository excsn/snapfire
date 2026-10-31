//! `Promise.all` over any list, as a body writes it. A body has no promise values, so it reads as its argument.

use snapfire_fsr_core::Value;
use snapfire_fsr_ir::Interpreter;

fn loaded(loader: &str) -> Value {
  let body = snapfire_fsr_lower::lower_loader("page.loader.ts", loader).unwrap_or_else(|e| panic!("{e}"));
  let ctx = snapfire_fsr_runtime::RequestCtx::anonymous(Default::default());
  futures::executor::block_on(Interpreter::default().run(&body, &ctx, None)).map(|outcome| outcome.value).unwrap_or_else(|e| panic!("{}", e.message))
}

fn numbers(items: &[f64]) -> Value {
  Value::Seq(items.iter().map(|n| Value::F64(*n)).collect())
}

fn load_of(body: &str) -> String {
  format!("export async function load() {{\n  const items = [1, 2];\n{body}\n}}\n")
}

#[test]
fn it_takes_a_mapped_list() {
  assert_eq!(loaded(&load_of("  return await Promise.all(items.map((n) => n * 2));")), numbers(&[2.0, 4.0]));
}

#[test]
fn it_takes_an_async_mapper() {
  assert_eq!(loaded(&load_of("  return await Promise.all(items.map(async (n) => n * 2));")), numbers(&[2.0, 4.0]));
  assert_eq!(loaded(&load_of("  return await Promise.all(items.map(async (n) => { const twice = await Promise.resolve(n * 2); return twice; }));")), numbers(&[2.0, 4.0]));
}

#[test]
fn it_takes_a_list_held_in_a_name() {
  assert_eq!(loaded(&load_of("  const calls = items.map((n) => n + 1);\n  const done = await Promise.all(calls);\n  return done;")), numbers(&[2.0, 3.0]));
}

#[test]
fn it_takes_spreads_and_works_without_an_await_where_it_is_returned() {
  assert_eq!(loaded(&load_of("  return await Promise.all([...items.map((n) => n * 2), 9]);")), numbers(&[2.0, 4.0, 9.0]));
  assert_eq!(loaded(&load_of("  return Promise.all(items.map((n) => n * 2));")), numbers(&[2.0, 4.0]));
}

#[test]
fn an_empty_list_is_an_empty_answer() {
  assert_eq!(loaded("export async function load() {\n  const none: number[] = [];\n  return await Promise.all(none.map((n) => n));\n}\n"), numbers(&[]));
}

#[test]
fn a_service_call_in_the_mapper_lowers_to_that_call() {
  let src = "export async function load({ services }) {\n  const ids = [1, 2];\n  return await Promise.all(ids.map((id) => services.shop.get({ id })));\n}\n";
  let text = format!("{:?}", snapfire_fsr_lower::lower_loader("page.loader.ts", src).unwrap_or_else(|e| panic!("{e}")));
  assert!(text.contains("get") && !text.contains("Promise"), "{text}");
}

#[test]
fn the_literal_form_still_runs_each_value() {
  assert_eq!(loaded(&load_of("  const [a, b] = await Promise.all([1, 2]);\n  return [b, a];")), numbers(&[2.0, 1.0]));
}
