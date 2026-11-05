use snapfire_fsr_core::{Params, Value};
use snapfire_fsr_ir::{Body, Interpreter, Prepared};
use snapfire_fsr_runtime::{FailureKind, RequestCtx};

#[path = "written/bodies.rs"]
mod bodies;
use bodies::written_bodies;

fn outcome(body: &Body, prepared: Prepared<'_>) -> Result<Value, (FailureKind, String)> {
  let ctx = RequestCtx::anonymous(Params::new());
  tokio::runtime::Builder::new_current_thread().build().unwrap().block_on(Interpreter::default().run_with(body, prepared, &ctx, None)).map(|o| o.value).map_err(|f| (f.kind, f.message))
}

mod written {
  #![deny(warnings)]
  include!("fixtures/native_bodies.rs");
}

#[test]
fn a_natively_written_body_answers_as_the_interpreter_does() {
  let native: [(&str, snapfire_fsr_ir::native::NativeBody); 9] = [
    ("sorted_by_n", written::sorted_by_n),
    ("sorted_plainly", written::sorted_plainly),
    ("sorted_in_place", written::sorted_in_place),
    ("object_from_a_block", written::object_from_a_block),
    ("local_from_a_block", written::local_from_a_block),
    ("counted_numbers", written::counted_numbers),
    ("counted_fraction", written::counted_fraction),
    ("numbers_beside_an_integer", written::numbers_beside_an_integer),
    ("counted_while", written::counted_while),
  ];
  for ((name, body), (written_name, f)) in written_bodies().into_iter().zip(native) {
    assert_eq!(name, written_name);
    let interpreted = outcome(&body, Prepared::No);
    assert_eq!(outcome(&body, Prepared::Native(f)), interpreted, "{name}");
    assert!(interpreted.is_ok(), "{name}: {interpreted:?}");
  }
  let order = |v: Value| match v {
    Value::Seq(items) => items.iter().map(|i| match i { Value::Map(m) => m.get("name").map(|n| format!("{n:?}")).unwrap_or_default(), other => format!("{other:?}") }).collect::<Vec<_>>().join(" "),
    other => format!("{other:?}"),
  };
  assert_eq!(order(outcome(&written_bodies()[0].1, Prepared::No).unwrap()), r#"Str("a") Str("b") Str("c") Str("d")"#, "stable: c stays before d");
  assert_eq!(order(outcome(&written_bodies()[1].1, Prepared::No).unwrap()), r#"Int(1) Int(10) Int(9) Str("a") Str("b")"#, "without a comparator, as strings");
}

#[test]
fn a_local_that_only_holds_numbers_is_written_as_an_f64_and_a_range_is_counted_without_an_array() {
  let source = snapfire_fsr_ir::native::emit_or_why("b", &bodies::counted_numbers(), None).unwrap();
  assert!(source.contains(": f64 = "), "{source}");
  assert!(!source.contains("B_::Range"), "{source}");
}
