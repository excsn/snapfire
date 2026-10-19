//! `asin` and `acos`: libc/src/__support/math/asin.h, acos.h and asin_utils.h.

use super::asin_tables::{ASIN_COEFFS, ASIN_COEFFS_F128};
use super::c;
use super::dd::{exact_add, exact_mult, quick_mult, Dd};
use super::dyadic::{polyeval, quick_add, quick_mul, Dyadic, EXP_BIAS};

const PI: Dd = Dd { lo: f64::from_bits(0x3ca1a62633145c07), hi: f64::from_bits(0x400921fb54442d18) };
const PI_OVER_TWO: Dd = Dd { lo: f64::from_bits(0x3c91a62633145c07), hi: f64::from_bits(0x3ff921fb54442d18) };
const PI_OVER_TWO_F128: Dyadic = Dyadic::new(false, -127, 0xc90fdaa22168c234c4c6628b80dc1cd1);
const PI_F128: Dyadic = Dyadic::new(false, -126, 0xc90fdaa22168c234c4c6628b80dc1cd1);

/// `asin(sqrt(u)) / sqrt(u)` around `idx / 32`, with `err` scaled to the bound of the result.
fn asin_eval(u: Dd, err: &mut f64) -> Dd {
  let k = (u.hi * c(0x4040000000000000)).round_ties_even();
  let idx = k as usize;
  let y_hi = k.mul_add(-c(0x3fa0000000000000), u.hi);
  let y = exact_add(y_hi, u.lo);
  let y2 = y.hi * y.hi;
  *err = err.mul_add(y2, c(0x3990000000000000));
  let coeffs = &ASIN_COEFFS[idx];
  let c0 = quick_mult(y, Dd { lo: coeffs[3], hi: coeffs[2] });
  let c1 = y.hi.mul_add(coeffs[5], coeffs[4]);
  let c2 = y.hi.mul_add(coeffs[7], coeffs[6]);
  let c3 = y.hi.mul_add(coeffs[9], coeffs[8]);
  let c4 = y.hi.mul_add(coeffs[11], coeffs[10]);
  let y4 = y2 * y2;
  let d0 = y2.mul_add(c2, c1);
  let d1 = y2.mul_add(c4, c3);
  let mut r = exact_add(coeffs[0], c0.hi);
  let e1 = y4.mul_add(d1, d0);
  r.lo = y2.mul_add(e1, coeffs[1] + c0.lo + r.lo);
  r
}

fn asin_eval_f128(u: Dyadic, idx: usize) -> Dyadic {
  polyeval(u, &ASIN_COEFFS_F128[idx])
}

/// `-(idx / 64) + hi` and `lo` gathered in 128 bits, for the accurate pass near zero.
fn reduced_square(x_sq: Dd, idx: usize) -> Dyadic {
  let u_hi = Dyadic::from_f64((idx as f64).mul_add(-c(0x3f90000000000000), x_sq.hi));
  quick_add(u_hi, Dyadic::from_f64(x_sq.lo))
}

/// `2 * sqrt(u)` as `vh + vl` and the low correction the accurate pass adds, for `u = (1 - |x|) / 2`.
fn half_angle(u: f64) -> (f64, f64, f64, f64) {
  let v_hi = u.sqrt();
  let h = v_hi.mul_add(-v_hi, u);
  (v_hi, h, v_hi * 2.0, h / v_hi)
}

fn m_v(v_hi: f64, h: f64, vh: f64, vl: f64, u: f64) -> Dyadic {
  let vl_lo = (-v_hi).mul_add(vl, h) / v_hi;
  let t = h * -0.25 / u;
  let vll = vl.mul_add(t, vl_lo);
  quick_add(Dyadic::from_f64(vh), quick_add(Dyadic::from_f64(vl), Dyadic::from_f64(vll)))
}

pub(super) fn asin(x: f64) -> f64 {
  let x_exp = super::biased_exponent(x);
  if x_exp < EXP_BIAS - 1 {
    if x_exp < EXP_BIAS - 26 {
      return x.mul_add(c(0x3c90000000000000), x);
    }
    let x_sq = exact_mult(x, x);
    let mut err = x.abs() * c(0x3cc0000000000000);
    let p = asin_eval(x_sq, &mut err);
    let r0 = exact_mult(x, p.hi);
    let r_lo = x.mul_add(p.lo, r0.lo);
    let r_upper = r0.hi + (r_lo + err);
    let r_lower = r0.hi + (r_lo - err);
    if r_upper == r_lower {
      return r_upper;
    }
    let idx = (x_sq.hi * c(0x4050000000000000)).round_ties_even() as usize;
    let p_f128 = asin_eval_f128(reduced_square(x_sq, idx), idx);
    return quick_mul(Dyadic::from_f64(x), p_f128).to_f64();
  }
  let x_abs = x.abs();
  let x_sign: f64 = if x.is_sign_negative() { -1.0 } else { 1.0 };
  if x_exp >= EXP_BIAS {
    if x_abs == 1.0 {
      return x_sign.mul_add(PI_OVER_TWO.hi, x_sign * PI_OVER_TWO.lo);
    }
    return f64::NAN;
  }
  let u = x_abs.mul_add(-0.5, 0.5);
  let (v_hi, h, vh, vl) = half_angle(u);
  let mut err = vh * c(0x3cc0000000000000);
  let p = asin_eval(Dd { lo: 0.0, hi: u }, &mut err);
  let r0 = quick_mult(Dd { lo: vl, hi: vh }, p);
  let r = exact_add(PI_OVER_TWO.hi, -r0.hi);
  let r_lo = PI_OVER_TWO.lo - r0.lo + r.lo;
  let r_upper = r.hi.mul_add(x_sign, r_lo.mul_add(x_sign, err));
  let r_lower = r.hi.mul_add(x_sign, r_lo.mul_add(x_sign, -err));
  if r_upper == r_lower {
    return r_upper;
  }
  let idx = (u * c(0x4050000000000000)).round_ties_even() as usize;
  let mut m_v = m_v(v_hi, h, vh, vl, u);
  m_v.neg = true;
  let y_f128 = Dyadic::from_f64((idx as f64).mul_add(-c(0x3f90000000000000), u));
  let p_f128 = asin_eval_f128(y_f128, idx);
  let mut r_f128 = quick_add(PI_OVER_TWO_F128, quick_mul(m_v, p_f128));
  if x.is_sign_negative() {
    r_f128.neg = true;
  }
  r_f128.to_f64()
}

pub(super) fn acos(x: f64) -> f64 {
  let x_exp = super::biased_exponent(x);
  if x_exp < EXP_BIAS - 1 {
    if x_exp < EXP_BIAS - 55 {
      return (x.abs() + c(0x35f0000000000000)) + PI_OVER_TWO.hi;
    }
    let x_sq = exact_mult(x, x);
    let mut err = x.abs() * c(0x3cc0000000000000);
    let p = asin_eval(x_sq, &mut err);
    let r0 = exact_mult(x, p.hi);
    let r_hi = (-x).mul_add(p.hi, PI_OVER_TWO.hi);
    let r_lo = ((PI_OVER_TWO.hi - r_hi) - r0.hi) - r0.lo;
    let r_lo = (-x).mul_add(p.lo, r_lo + PI_OVER_TWO.lo);
    let r_upper = r_hi + (r_lo + err);
    let r_lower = r_hi + (r_lo - err);
    if r_upper == r_lower {
      return r_upper;
    }
    let idx = (x_sq.hi * c(0x4050000000000000)).round_ties_even() as usize;
    let p_f128 = asin_eval_f128(reduced_square(x_sq, idx), idx);
    let x_f128 = Dyadic::from_f64(x).negated();
    return quick_add(PI_OVER_TWO_F128, quick_mul(x_f128, p_f128)).to_f64();
  }
  let x_abs = x.abs();
  let x_sign: f64 = if x.is_sign_negative() { -1.0 } else { 1.0 };
  if x_exp >= EXP_BIAS {
    if x_abs == 1.0 {
      return if x == 1.0 { 0.0 } else { (-x_sign).mul_add(PI.hi, PI.lo) };
    }
    return f64::NAN;
  }
  let u = x_abs.mul_add(-0.5, 0.5);
  let (v_hi, h, vh, vl) = half_angle(u);
  let mut err = vh * c(0x3cc0000000000000);
  let p = asin_eval(Dd { lo: 0.0, hi: u }, &mut err);
  let r0 = quick_mult(Dd { lo: vl, hi: vh }, p);
  let (r_hi, r_lo) = if x.is_sign_negative() {
    let r = exact_add(PI.hi, -r0.hi);
    (r.hi, (PI.lo - r0.lo) + r.lo)
  } else {
    (r0.hi, r0.lo)
  };
  let r_upper = r_hi + (r_lo + err);
  let r_lower = r_hi + (r_lo - err);
  if r_upper == r_lower {
    return r_upper;
  }
  let idx = (u * c(0x4050000000000000)).round_ties_even() as usize;
  let mut m_v = m_v(v_hi, h, vh, vl, u);
  m_v.neg = x.is_sign_negative();
  let y_f128 = Dyadic::from_f64((idx as f64).mul_add(-c(0x3f90000000000000), u));
  let p_f128 = asin_eval_f128(y_f128, idx);
  let mut r_f128 = quick_mul(m_v, p_f128);
  if x.is_sign_negative() {
    r_f128 = quick_add(PI_F128, r_f128);
  }
  r_f128.to_f64()
}
