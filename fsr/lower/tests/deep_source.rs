//! Lowering source nested deeper than a thread's stack holds frames for: the stack grows to fit, as the interpreter's does.

use snapfire_fsr_lower::lower_loader;

const DEPTH: usize = 200;

#[test]
fn a_loader_nested_deeper_than_a_small_stack_lowers() {
  let nested = (0..DEPTH).fold("1".to_owned(), |inner, _| format!("[1].map(() => {inner})[0]"));
  let source = format!("export async function load() {{\n  return {{ v: {nested} }};\n}}\n");
  let lowered = std::thread::Builder::new().stack_size(2 << 20).spawn(move || lower_loader("l.ts", &source).map(|_| ()).map_err(|e| e.to_string())).unwrap().join();
  assert!(matches!(lowered, Ok(Ok(()))), "{lowered:?}");
}
