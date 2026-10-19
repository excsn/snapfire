//! `atan` and `atan2`: libc/src/__support/math/atan.h, atan2.h and atan_utils.h. Neither has an accurate pass, so the last bit follows the arithmetic of the path taken; this is the fused multiply-add path.

use super::atan_tables::ATAN_I;
use super::c;
use super::dd::{add, div, exact_add, exact_mult, Dd};

const PI: Dd = Dd { lo: f64::from_bits(0x3ca1a62633145c07), hi: f64::from_bits(0x400921fb54442d18) };
const PI_OVER_2: Dd = Dd { lo: f64::from_bits(0x3c91a62633145c07), hi: f64::from_bits(0x3ff921fb54442d18) };
const PI_OVER_4: Dd = Dd { lo: f64::from_bits(0x3c81a62633145c07), hi: f64::from_bits(0x3fe921fb54442d18) };
const THREE_PI_OVER_4: Dd = Dd { lo: f64::from_bits(0x3c9a79394c9e8a0a), hi: f64::from_bits(0x4002d97c7f3321d2) };
const ZERO: Dd = Dd { lo: 0.0, hi: 0.0 };
const MZERO: Dd = Dd { lo: -0.0, hi: -0.0 };

fn negated(d: Dd) -> Dd {
  Dd { lo: -d.lo, hi: -d.hi }
}

/// `atan(x)` for a small reduced `x`, as a double-double.
fn atan_eval(x: Dd) -> Dd {
  let x_hi_sq = x.hi * x.hi;
  let c0 = x_hi_sq.mul_add(c(0x3fc999999999999a), c(0xbfd5555555555555));
  let c1 = x_hi_sq.mul_add(c(0x3fbc71c71c71c71c), c(0xbfc2492492492492));
  let x_hi_3 = x_hi_sq * x.hi;
  let x_hi_4 = x_hi_sq * x_hi_sq;
  let d0 = x_hi_4.mul_add(c1, c0);
  let d1 = (x_hi_4 - x_hi_sq).mul_add(x.lo, x.lo);
  Dd { lo: x_hi_3.mul_add(d0, d1), hi: x.hi }
}

fn sign_of(neg: bool) -> f64 {
  if neg {
    -1.0
  } else {
    1.0
  }
}

pub(super) fn atan(x: f64) -> f64 {
  let x_sign = x.is_sign_negative();
  let x_abs = x.to_bits() & !(1 << 63);
  let x_exp = (x_abs >> 52) as i32 - 1023;
  let x_d = f64::from_bits(x_abs);
  if x_exp < 0 {
    if x_exp < -26 {
      if x == 0.0 {
        return x;
      }
      return c(0xbc90000000000000).mul_add(x, x);
    }
    let k = (64.0 * x_d).round_ties_even();
    let idx = k as usize;
    let k = k * c(0x3f90000000000000);
    let num = Dd { lo: 0.0, hi: x_d - k };
    let den_hi = x_d.mul_add(k, 1.0);
    let prod = exact_mult(x_d, k);
    let den = Dd { lo: ((1.0 - den_hi) + prod.hi) + prod.lo, hi: den_hi };
    let p = atan_eval(div(num, den));
    let c0 = exact_add(ATAN_I[idx].hi, p.hi);
    let c1 = c0.lo + (ATAN_I[idx].lo + p.lo);
    return sign_of(x_sign) * (c0.hi + c1);
  }
  if x_exp >= 53 {
    if x.is_nan() {
      return f64::NAN;
    }
    return sign_of(x_sign).mul_add(PI_OVER_2.hi, sign_of(x_sign) * PI_OVER_2.lo);
  }
  let y = 1.0 / x_d;
  let k = (64.0 * y).round_ties_even();
  let idx = k as usize;
  let k = k * c(0x3f90000000000000);
  let den = exact_add(x_d, k);
  let num_hi = (-x_d).mul_add(k, 1.0);
  let prod = exact_mult(x_d, k);
  let num = Dd { lo: ((1.0 - num_hi) - prod.hi) - prod.lo, hi: num_hi };
  let p = atan_eval(div(num, den));
  let mpi_over_2 = negated(PI_OVER_2);
  let c0 = exact_add(mpi_over_2.hi, ATAN_I[idx].hi);
  let c1 = exact_add(c0.hi, p.hi);
  let c2 = c1.lo + (c0.lo + p.lo) + (ATAN_I[idx].lo + mpi_over_2.lo);
  sign_of(!x_sign) * (c1.hi + c2)
}

pub(super) fn atan2(y: f64, x: f64) -> f64 {
  let const_adj = |x_neg: bool, y_neg: bool, recip: bool| match (x_neg, y_neg, recip) {
    (false, false, false) => ZERO,
    (false, false, true) => negated(PI_OVER_2),
    (false, true, false) => MZERO,
    (false, true, true) => negated(PI_OVER_2),
    (true, _, false) => negated(PI),
    (true, _, true) => PI_OVER_2,
  };
  let x_sign = x.is_sign_negative();
  let y_sign = y.is_sign_negative();
  let x_abs = x.to_bits() & !(1 << 63);
  let y_abs = y.to_bits() & !(1 << 63);
  let recip = x_abs < y_abs;
  let mut min_abs = if recip { x_abs } else { y_abs };
  let mut max_abs = if recip { y_abs } else { x_abs };
  let mut min_exp = (min_abs >> 52) as u32;
  let mut max_exp = (max_abs >> 52) as u32;
  let mut num = f64::from_bits(min_abs);
  let mut den = f64::from_bits(max_abs);
  if max_exp > 0x7ff - 128 || min_exp < 128 {
    if x.is_nan() || y.is_nan() {
      return f64::NAN;
    }
    let kind = |v: f64, abs: u64| {
      if v == 0.0 {
        0
      } else if f64::from_bits(abs).is_infinite() {
        2
      } else {
        1
      }
    };
    let (x_except, y_except) = (kind(x, x_abs), kind(y, y_abs));
    if x_except != 1 || y_except != 1 {
      let excepts = [[[ZERO, PI], [ZERO, PI], [ZERO, PI]], [[PI_OVER_2, PI_OVER_2], [ZERO, ZERO], [ZERO, PI]], [[PI_OVER_2, PI_OVER_2], [PI_OVER_2, PI_OVER_2], [PI_OVER_4, THREE_PI_OVER_4]]];
      let r = excepts[y_except][x_except][x_sign as usize];
      return sign_of(y_sign).mul_add(r.hi, sign_of(y_sign) * r.lo);
    }
    let scale_up = min_exp < 128;
    let scale_down = max_exp > 0x7ff - 128;
    if scale_up {
      num *= c(0x43f0000000000000);
      if !scale_down {
        den *= c(0x43f0000000000000);
      }
    } else if scale_down {
      den *= c(0x3bf0000000000000);
      num *= c(0x3bf0000000000000);
    }
    min_abs = num.to_bits();
    max_abs = den.to_bits();
    min_exp = (min_abs >> 52) as u32;
    max_exp = (max_abs >> 52) as u32;
  }
  let final_sign = sign_of((x_sign != y_sign) != recip);
  let const_term = const_adj(x_sign, y_sign, recip);
  let exp_diff = max_exp.wrapping_sub(min_exp);
  if exp_diff > 54 {
    return final_sign.mul_add(const_term.hi, final_sign * (const_term.lo + num / den));
  }
  let k = (64.0 * num / den).round_ties_even();
  let idx = k as usize;
  let k = k * c(0x3f90000000000000);
  let num_k = exact_mult(num, k);
  let den_k = exact_mult(den, k);
  let num_dd = exact_add(num - den_k.hi, -den_k.lo);
  let mut den_dd = exact_add(den, num_k.hi);
  den_dd.lo += num_k.lo;
  let p = atan_eval(div(num_dd, den_dd));
  let r = add(const_term, add(ATAN_I[idx], p));
  final_sign * r.hi + final_sign * r.lo
}
