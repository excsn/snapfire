//! `Math.sin` and `Math.cos` against what V8 computes and at the values JavaScript defines exactly.

use snapfire_fsr_ir::jsmath::{acos, asin, atan, atan2, cbrt, cos, exp, expm1, log, log10, log1p, log2, sin, tan};

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

/// The inputs `tests/v8_math.js` generates for `name` from `seed`, in the same order.
struct Inputs {
  state: u32,
  bands: &'static [(u32, i32, i32)],
}

impl Inputs {
  fn new(name: &str, seed: u32) -> Self {
    let bands: &'static [(u32, i32, i32)] = match name {
      "sin" | "cos" | "tan" => &[(0, -30, 30), (0, -1022, 1023)],
      "exp" | "sinh" | "cosh" => &[(0, -30, 9), (0, -60, 10)],
      "expm1" => &[(0, -60, 9), (0, -30, 10)],
      "log" | "log2" | "log10" => &[(1, -1022, 1023), (1, -3, 3)],
      "log1p" => &[(1, -1022, 1023), (2, -60, -1)],
      "cbrt" => &[(0, -1022, 1023), (0, -10, 10)],
      "asin" | "acos" | "atanh" => &[(0, -60, -1), (0, -4, -1)],
      "atan" | "atan2" | "asinh" => &[(0, -1022, 1023), (0, -30, 30)],
      "acosh" => &[(1, 0, 1023), (1, 0, 3)],
      other => panic!("no bands for {other}"),
    };
    Inputs { state: if seed == 0 { 1 } else { seed }, bands }
  }

  fn next(&mut self) -> u32 {
    self.state ^= self.state << 13;
    self.state ^= self.state >> 17;
    self.state ^= self.state << 5;
    self.state
  }

  fn input(&mut self) -> f64 {
    let (sign, emin, emax) = self.bands[(self.next() % self.bands.len() as u32) as usize];
    let e = (1023 + emin) as u32 + self.next() % (emax - emin + 1) as u32;
    let neg = sign == 2 || (sign == 0 && self.next() & 1 == 1);
    let lo = self.next();
    let hi = (if neg { 0x8000_0000 } else { 0 }) | (e << 20) | (self.next() & 0xfffff);
    f64::from_bits(((hi as u64) << 32) | lo as u64)
  }
}

/// FNV-1a over the low and the high words of each result, as `tests/v8_math.js` folds them; every NaN is the one V8 writes.
fn digest(name: &str, seed: u32, count: usize, f: &dyn Fn(&mut Inputs) -> f64) -> String {
  let mut inputs = Inputs::new(name, seed);
  let (mut h0, mut h1) = (0x811c9dc5u32, 0x811c9dc5u32);
  let mix = |mut h: u32, w: u32| {
    for i in 0..4 {
      h ^= (w >> (8 * i)) & 0xff;
      h = h.wrapping_mul(0x01000193);
    }
    h
  };
  for _ in 0..count {
    let r = f(&mut inputs);
    let bits = if r.is_nan() { 0x7ff8_0000_0000_0000 } else { r.to_bits() };
    h0 = mix(h0, bits as u32);
    h1 = mix(h1, (bits >> 32) as u32);
  }
  format!("{h1:08x}{h0:08x}")
}

/// What HeadlessChrome 153 answered for `tests/v8_math.js` with 100000 inputs from seeds 1 to 5, a function's digests in seed order.
const V8_DIGESTS: &[(&str, [&str; 5])] = &[
  ("sin", ["6afa0fb8cd7820b7", "c4dbb71e33d91e09", "4483fb8ae6cd8cdd", "1cce69a903d6d103", "c580d0c04c257303"]),
  ("cos", ["ae80b43f7b14e441", "dcc9f7c19ef4c9b2", "c24a9b2e20da980f", "ac2adf7e006053b0", "ee30404f40d91e4a"]),
  ("tan", ["0209389d22ed049d", "ed0908ec82f85a70", "8c1f5d7c4c1aee10", "7f5bca933945a681", "0f3a7eaff8a704cf"]),
  ("exp", ["a96f40a3ea726a42", "bb2e272287794b58", "5eb07e98cdcebe89", "8dc772f29afcc010", "fbdd50416f9dc10c"]),
  ("expm1", ["78333706477d8fd5", "c2610bb8ba5c1e23", "b7208ac78b038e8a", "fc5a7be17169f2af", "cb06526ccbd00adc"]),
  ("log", ["508abdacacb3c3a2", "1c352eb643607aa6", "02d0a7835960dc25", "c4f5fdf163d503d5", "9261f48f6f01d390"]),
  ("log2", ["4ec0ffe09fe42099", "f05bbe1ea23fdf78", "fc143f73a3de6345", "33ced0cf17552d0d", "d587cb521cdbb91a"]),
  ("log10", ["58cdc82bdba3027d", "b7ab98dffadea09e", "6cfca582aa4bb449", "64564f86678435b6", "d8636eefad8bf29f"]),
  ("log1p", ["046d67b2e89fffe9", "e8c853725da6cbca", "3455bd7efea65e2e", "9312411eed3b9236", "2b2e5379f03ab3ab"]),
  ("cbrt", ["c10f3070024fb0ee", "351cda7ec6fa370a", "bc4c4630678bd4ef", "c03a1b04ff298eee", "75a2ec558157a79d"]),
  ("asin", ["2f576aa6ab901f38", "d3c124e1b66ec3b0", "31608ee98ba1f594", "8452424cce6999e1", "eea7a189428fc5a1"]),
  ("acos", ["eac148242226ccb6", "e6d36f20bcf77ff6", "63f237259b8fc6c5", "624eb7762b0205ef", "cb21746ff3620734"]),
  ("atan", ["c239f00f9e9e5571", "edb7a7feb2fa35f1", "7bd510a8c01d366d", "38988199c9664db0", "c2f60319d0012004"]),
  ("atan2", ["d963bef8a0d7c976", "41b927f4267afd03", "0138a9964d1bbd12", "4526077f262e378f", "ba79d64c991a01ea"]),
];

fn unary(f: fn(f64) -> f64) -> impl Fn(&mut Inputs) -> f64 {
  move |inputs: &mut Inputs| f(inputs.input())
}

#[test]
fn every_function_answers_what_v8_answers_over_half_a_million_inputs() {
  for (name, digests) in V8_DIGESTS {
    let f: Box<dyn Fn(&mut Inputs) -> f64> = match *name {
      "sin" => Box::new(unary(sin)),
      "cos" => Box::new(unary(cos)),
      "tan" => Box::new(unary(tan)),
      "exp" => Box::new(unary(exp)),
      "expm1" => Box::new(unary(expm1)),
      "log" => Box::new(unary(log)),
      "log2" => Box::new(unary(log2)),
      "log10" => Box::new(unary(log10)),
      "log1p" => Box::new(unary(log1p)),
      "cbrt" => Box::new(unary(cbrt)),
      "asin" => Box::new(unary(asin)),
      "acos" => Box::new(unary(acos)),
      "atan" => Box::new(unary(atan)),
      "atan2" => Box::new(|inputs: &mut Inputs| {
        let y = inputs.input();
        atan2(y, inputs.input())
      }),
      other => panic!("{other}"),
    };
    for (seed, expected) in digests.iter().enumerate() {
      assert_eq!(digest(name, seed as u32 + 1, 100_000, &*f), *expected, "{name} seed {}", seed + 1);
    }
  }
}
