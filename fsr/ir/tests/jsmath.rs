//! `Math.sin` and `Math.cos` against what V8 computes and at the values JavaScript defines exactly.

use snapfire_fsr_ir::jsmath::{cos, sin};

fn same(ours: f64, theirs: f64) -> bool {
  ours.to_bits() == theirs.to_bits() || (ours.is_nan() && theirs.is_nan())
}

#[test]
fn sine_and_cosine_print_the_digits_v8_prints() {
  let fixture = include_str!("fixtures/math_trig.txt");
  let mut checked = 0;
  for line in fixture.lines().filter(|line| !line.starts_with('#')) {
    let bits: Vec<f64> = line.split_whitespace().map(|h| f64::from_bits(u64::from_str_radix(h, 16).expect("hex bits"))).collect();
    let [x, s, c] = bits[..] else { panic!("{line}") };
    assert!(same(sin(x), s), "sin({x:e}) = {:e}, V8 {s:e}", sin(x));
    assert!(same(cos(x), c), "cos({x:e}) = {:e}, V8 {c:e}", cos(x));
    checked += 1;
  }
  assert_eq!(checked, 8000);
}

#[test]
fn the_values_javascript_defines_exactly_hold() {
  assert!(same(sin(0.0), 0.0) && same(sin(-0.0), -0.0));
  assert!(same(cos(0.0), 1.0) && same(cos(-0.0), 1.0));
  for x in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
    assert!(sin(x).is_nan() && cos(x).is_nan(), "{x}");
  }
  let tiny = f64::from_bits(1);
  assert!(same(sin(tiny), tiny) && same(sin(-tiny), -tiny));
  assert!(same(cos(tiny), 1.0));
  assert!(same(sin(f64::MAX), f64::from_bits(0x3f7452fc98b34e97)));
  assert!(same(cos(f64::MAX), f64::from_bits(0xbfefffe62ecfab75)));
  assert!(same(sin(std::f64::consts::PI), f64::from_bits(0x3ca1a62633145c07)));
  assert!(same(cos(std::f64::consts::PI), -1.0));
  assert!(same(cos(std::f64::consts::FRAC_PI_2), f64::from_bits(0x3c91a62633145c07)));
}
