//! Coverage-guided fuzzing of the reader. Under `cargo test` these run as
//! randomised checks against the saved corpus; under
//! `cargo bolero test --package snapfire_fsr_ir <name>` they run an engine
//! with coverage feedback.

use arbitrary::{Arbitrary, Unstructured};
use bolero::check;
use snapfire_fsr_ir::sexpr::{parse, print, Sx};

/// A tree built from the fuzzer's bytes. The depth is capped so the generator
/// itself cannot be what overflows.
fn tree(u: &mut Unstructured, depth: usize) -> arbitrary::Result<Sx> {
  if depth >= 6 || u.is_empty() {
    return Ok(match u8::arbitrary(u)? % 2 {
      0 => Sx::Sym(String::arbitrary(u)?),
      _ => Sx::Str(String::arbitrary(u)?),
    });
  }
  Ok(match u8::arbitrary(u)? % 4 {
    0 => Sx::Sym(String::arbitrary(u)?),
    1 => Sx::Str(String::arbitrary(u)?),
    2 => Sx::Interp(Box::new(tree(u, depth + 1)?)),
    _ => {
      let n = u8::arbitrary(u)? % 4;
      let mut items = Vec::new();
      for _ in 0..n {
        items.push(tree(u, depth + 1)?);
      }
      Sx::List(items)
    }
  })
}

/// Any text at all: the reader answers or errors, and never aborts.
#[test]
fn fuzz_text_is_answered_or_refused() {
  check!().with_type::<String>().for_each(|src| {
    let _ = parse(src);
  });
}

/// The same over raw bytes, so the fuzzer can steer through the UTF-8 boundary
/// cases a `String` generator would never offer.
#[test]
fn fuzz_bytes_are_answered_or_refused() {
  check!().with_type::<Vec<u8>>().for_each(|bytes| {
    if let Ok(src) = std::str::from_utf8(bytes) {
      let _ = parse(src);
    }
  });
}

/// Whatever the printer writes, the parser reads back, with the fuzzer picking
/// the tree.
#[test]
fn fuzz_a_printed_tree_reads_back() {
  check!().with_type::<Vec<u8>>().for_each(|bytes| {
    let mut u = Unstructured::new(bytes);
    if let Ok(form) = tree(&mut u, 0) {
      let text = print(std::slice::from_ref(&form));
      match parse(&text) {
        Ok(back) => assert_eq!(back, vec![form], "printed as: {text}"),
        Err(e) => panic!("{e}\nprinted as: {text}"),
      }
    }
  });
}
