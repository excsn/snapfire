//! LLVM libc's double-double arithmetic, `FPUtil/double_double.h`, on the fused multiply-add path.

/// `hi + lo`, LLVM's `DoubleDouble`; the low part is written first, as in the tables.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Dd {
  pub(super) lo: f64,
  pub(super) hi: f64,
}

/// `a + b` exactly when `|a| >= |b|` (Fast2Sum).
pub(super) fn exact_add(a: f64, b: f64) -> Dd {
  let hi = a + b;
  let t = hi - a;
  Dd { lo: b - t, hi }
}

/// `a + b` exactly whatever their magnitudes (2Sum), `exact_add<false>`.
pub(super) fn two_sum(a: f64, b: f64) -> Dd {
  let hi = a + b;
  let t1 = hi - a;
  let t2 = hi - t1;
  let t3 = b - t1;
  let t4 = a - t2;
  Dd { lo: t3 + t4, hi }
}

/// `a * b` exactly.
pub(super) fn exact_mult(a: f64, b: f64) -> Dd {
  let hi = a * b;
  Dd { lo: a.mul_add(b, -hi), hi }
}

/// `a * b` dropping `a.lo * b.lo`.
pub(super) fn quick_mult(a: Dd, b: Dd) -> Dd {
  let r = exact_mult(a.hi, b.hi);
  let t1 = a.hi.mul_add(b.lo, r.lo);
  Dd { lo: a.lo.mul_add(b.hi, t1), hi: r.hi }
}

/// `a * b` for a double `a`.
pub(super) fn quick_mult_by(a: f64, b: Dd) -> Dd {
  let r = exact_mult(a, b.hi);
  Dd { lo: a.mul_add(b.lo, r.lo), hi: r.hi }
}

/// `a + b` renormalised.
pub(super) fn add(a: Dd, b: Dd) -> Dd {
  let r = exact_add(a.hi, b.hi);
  let lo = a.lo + b.lo;
  exact_add(r.hi, r.lo + lo)
}

/// `a / b` to about 104 bits.
pub(super) fn div(a: Dd, b: Dd) -> Dd {
  let q = 1.0 / b.hi;
  let hi = a.hi * q;
  let e_hi = b.hi.mul_add(-hi, a.hi);
  let e_lo = b.lo.mul_add(-hi, a.lo);
  Dd { lo: q * (e_hi + e_lo), hi }
}

/// `c + a * b` as `add(c, quick_mult(a, b))`, the double-double `multiply_add`.
pub(super) fn multiply_add(a: Dd, b: Dd, c: Dd) -> Dd {
  add(c, quick_mult(a, b))
}

/// `hi + mid + lo`, LLVM's `TripleDouble`; the low part is written first, as in the tables.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Td {
  pub(super) lo: f64,
  pub(super) mid: f64,
  pub(super) hi: f64,
}

/// `coeffs[0] + x * (coeffs[1] + x * (...))` with each step the double-double `multiply_add`.
pub(super) fn polyeval(x: Dd, coeffs: &[Dd]) -> Dd {
  let (last, rest) = coeffs.split_last().expect("a coefficient");
  rest.iter().rev().fold(*last, |acc, coeff| multiply_add(x, acc, *coeff))
}
