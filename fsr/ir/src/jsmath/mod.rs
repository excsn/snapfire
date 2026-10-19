//! `Math.sin` and `Math.cos` as V8 computes them: LLVM libc's correctly
//! rounded double-precision functions, libc/src/__support/math/sin.h, cos.h,
//! sincos_eval.h and range_reduction_double_{common,fma}.h in llvm-project at
//! 64f50ffd4f1c9e2d7cfde07251dadcb8573d4451, ported along the fused
//! multiply-add path with rounding to nearest. V8 calls these for both since
//! v8 commit 8787f0842a. A correctly rounded result is the one double nearest
//! the true value, so the path taken does not change it.
//!
//! Part of the LLVM Project, under the Apache License v2.0 with LLVM Exceptions.
//! See https://llvm.org/LICENSE.txt for license information.
//! SPDX-License-Identifier: Apache-2.0 WITH LLVM-exception

mod tables;

use tables::{ONE_TWENTY_EIGHT_OVER_PI, SIN_K_PI_OVER_128, SIN_K_PI_OVER_128_F128};

const EXP_BIAS: i32 = 1023;
const FAST_PASS_EXPONENT: i32 = 16;

fn c(bits: u64) -> f64 {
  f64::from_bits(bits)
}

/// `hi + lo`, LLVM's `DoubleDouble`.
#[derive(Clone, Copy, Default)]
pub(super) struct Dd {
  pub(super) lo: f64,
  pub(super) hi: f64,
}

/// `(-1)^neg * mant * 2^exp`, LLVM's `DyadicFloat<128>`; normal when `mant` has its top bit set or is zero.
#[derive(Clone, Copy)]
pub(super) struct Dyadic {
  neg: bool,
  exp: i32,
  mant: u128,
}

impl Dyadic {
  pub(super) const fn new(neg: bool, exp: i32, mant: u128) -> Self {
    let mut d = Dyadic { neg, exp, mant };
    if mant != 0 {
      let shift = mant.leading_zeros();
      d.exp -= shift as i32;
      d.mant <<= shift;
    }
    d
  }

  fn from_f64(x: f64) -> Self {
    let bits = x.to_bits();
    let neg = bits >> 63 == 1;
    let biased = ((bits >> 52) & 0x7ff) as i32;
    let fraction = bits & ((1 << 52) - 1);
    let (exp, mant) = match biased {
      0 => (1 - EXP_BIAS, fraction),
      _ => (biased - EXP_BIAS, fraction | (1 << 52)),
    };
    Dyadic::new(neg, exp - 52, mant as u128)
  }

  fn shift_right(&mut self, n: u32) {
    self.exp += n as i32;
    self.mant = if n < 128 { self.mant >> n } else { 0 };
  }

  /// The double nearest the value, ties to even, as `fast_as` rounds under rounding to nearest.
  fn to_f64(self) -> f64 {
    let sign = (self.neg as u64) << 63;
    if self.mant == 0 {
      return f64::from_bits(sign);
    }
    let biased = self.exp + 127 + EXP_BIAS;
    if biased >= 2047 {
      return f64::from_bits(sign | 0x7ff0_0000_0000_0000);
    }
    let (shift, field) = if biased > 0 { (75u32, biased as u64) } else { (75 + (1 - biased) as u32, 0) };
    let kept = if shift >= 128 { 0 } else { (self.mant >> shift) as u64 };
    let round = shift <= 128 && (self.mant >> (shift - 1)) & 1 == 1;
    let sticky = match shift - 1 {
      0 => false,
      below if below >= 128 => self.mant != 0,
      below => self.mant & ((1u128 << below) - 1) != 0,
    };
    let mut m = kept;
    if round && (sticky || m & 1 == 1) {
      m += 1;
    }
    let magnitude = if field > 0 { (field << 52) + m - (1 << 52) } else { m };
    f64::from_bits(sign | magnitude)
  }
}

/// The high 128 bits of a 256-bit product as `UInt<128>::quick_mul_hi` approximates them: the low words' product is never formed.
fn mul_hi(a: u128, b: u128) -> u128 {
  let (a0, a1) = (a as u64 as u128, a >> 64);
  let (b0, b1) = (b as u64 as u128, b >> 64);
  let (cross, carry) = (a0 * b1).overflowing_add(a1 * b0);
  let middle = (cross >> 64) | ((carry as u128) << 64);
  (a1 * b1).wrapping_add(middle)
}

fn quick_add(mut a: Dyadic, mut b: Dyadic) -> Dyadic {
  if a.mant == 0 {
    return b;
  }
  if b.mant == 0 {
    return a;
  }
  if a.exp > b.exp {
    b.shift_right((a.exp - b.exp) as u32);
  } else if b.exp > a.exp {
    a.shift_right((b.exp - a.exp) as u32);
  }
  if a.neg == b.neg {
    let (sum, overflow) = a.mant.overflowing_add(b.mant);
    let mut result = Dyadic { neg: a.neg, exp: a.exp, mant: sum };
    if overflow {
      result.shift_right(1);
      result.mant |= 1 << 127;
    }
    return result;
  }
  let result = if a.mant >= b.mant { Dyadic { neg: a.neg, exp: a.exp, mant: a.mant - b.mant } } else { Dyadic { neg: b.neg, exp: b.exp, mant: b.mant - a.mant } };
  Dyadic::new(result.neg, result.exp, result.mant)
}

fn quick_mul(a: Dyadic, b: Dyadic) -> Dyadic {
  let mut result = Dyadic { neg: a.neg != b.neg, exp: a.exp + b.exp + 128, mant: 0 };
  if a.mant != 0 && b.mant != 0 {
    result.mant = mul_hi(a.mant, b.mant);
    if result.mant >> 127 == 0 {
      result.exp -= 1;
      result.mant <<= 1;
    }
  }
  result
}

/// `coeffs[0] + x * (coeffs[1] + x * (...))`, each step `quick_add(c, quick_mul(x, rest))`.
fn polyeval(x: Dyadic, coeffs: &[Dyadic]) -> Dyadic {
  let (last, rest) = coeffs.split_last().expect("a coefficient");
  rest.iter().rev().fold(*last, |acc, coeff| quick_add(*coeff, quick_mul(x, acc)))
}

fn exact_add(a: f64, b: f64) -> Dd {
  let hi = a + b;
  let t = hi - a;
  Dd { lo: b - t, hi }
}

fn exact_mult(a: f64, b: f64) -> Dd {
  let hi = a * b;
  Dd { lo: a.mul_add(b, -hi), hi }
}

fn quick_mult(a: Dd, b: Dd) -> Dd {
  let r = exact_mult(a.hi, b.hi);
  let t1 = a.hi.mul_add(b.lo, r.lo);
  Dd { lo: a.lo.mul_add(b.hi, t1), hi: r.hi }
}

fn biased_exponent(x: f64) -> i32 {
  ((x.to_bits() >> 52) & 0x7ff) as i32
}

/// `k = round(x * 128/pi)` and `x - k * pi/128` for `2^-7 < |x| < 2^16`.
fn range_reduction_small(x: f64) -> (u32, Dd) {
  let mpi_over_128 = [c(0xbf9921fb54400000), c(0xbd70b4611a600000), c(0xbb43198a2e037073)];
  let kd = (x * c(0x40445f306dc9c883)).round_ties_even();
  let y_hi = kd.mul_add(mpi_over_128[0], x);
  let hi = kd.mul_add(mpi_over_128[1], y_hi);
  let u0 = y_hi - hi;
  let u1 = kd.mul_add(mpi_over_128[1], u0);
  let lo = kd.mul_add(mpi_over_128[2], u1);
  ((kd as i64) as u32, Dd { lo, hi })
}

const PI_OVER_128_F128: Dyadic = Dyadic::new(false, -133, 0xc90fdaa22168c234c4c6628b80dc1cd1);

fn range_reduction_small_f128(x: f64) -> Dyadic {
  let kd = (x * c(0x40445f306dc9c883)).round_ties_even();
  let mk = Dyadic::from_f64(-kd);
  let xf = Dyadic::from_f64(x);
  let p_hi = quick_mul(xf, Dyadic::from_f64(ONE_TWENTY_EIGHT_OVER_PI[3][0]));
  let p_mid = quick_mul(xf, Dyadic::from_f64(ONE_TWENTY_EIGHT_OVER_PI[3][1]));
  let p_lo = quick_mul(xf, Dyadic::from_f64(ONE_TWENTY_EIGHT_OVER_PI[3][2]));
  let s_hi = quick_add(p_hi, mk);
  let s_lo = quick_add(p_mid, p_lo);
  quick_mul(quick_add(s_hi, s_lo), PI_OVER_128_F128)
}

/// The range reduction for `|x| >= 2^16`, holding what the accurate pass reads.
#[derive(Default)]
struct LargeRangeReduction {
  idx: usize,
  x_reduced: f64,
  y_hi: f64,
  y_lo: f64,
  y_mid: Dd,
}

impl LargeRangeReduction {
  fn fast(&mut self, x: f64) -> (u32, Dd) {
    let bits = x.to_bits();
    let x_e_m62 = biased_exponent(x) - (EXP_BIAS + 62);
    self.idx = ((x_e_m62 >> 4) + 3) as usize;
    let exponent = ((x_e_m62 & 15) + EXP_BIAS + 62) as u64;
    self.x_reduced = f64::from_bits((bits & !(0x7ff << 52)) | (exponent << 52));
    let chunks = ONE_TWENTY_EIGHT_OVER_PI[self.idx];
    let ph = exact_mult(self.x_reduced, chunks[0]);
    let pm = exact_mult(self.x_reduced, chunks[1]);
    let pl = exact_mult(self.x_reduced, chunks[2]);
    let kd = (ph.lo + pm.hi).round_ties_even();
    self.y_hi = (ph.lo - kd) + pm.hi;
    self.y_mid = exact_add(pm.lo, pl.hi);
    self.y_lo = pl.lo;
    let y_l = self.x_reduced.mul_add(chunks[3], self.y_lo);
    let mut y = exact_add(self.y_hi, self.y_mid.hi);
    y.lo += self.y_mid.lo + y_l;
    let pi_over_128 = Dd { lo: c(0x3c31a62633145c07), hi: c(0x3f9921fb54442d18) };
    ((kd as i64) as u32, quick_mult(y, pi_over_128))
  }

  fn accurate(&self) -> Dyadic {
    let y_lo_0 = Dyadic::from_f64(self.x_reduced * ONE_TWENTY_EIGHT_OVER_PI[self.idx][3]);
    let y_lo_1 = quick_add(Dyadic::from_f64(self.y_lo), y_lo_0);
    let y_mid = quick_add(Dyadic::from_f64(self.y_mid.lo), y_lo_1);
    let y_hi = quick_add(Dyadic::from_f64(self.y_hi), Dyadic::from_f64(self.y_mid.hi));
    quick_mul(quick_add(y_hi, y_mid), PI_OVER_128_F128)
  }
}

/// `sin(u)` and `cos(u)` for a reduced `u`, with the error bound of the pair.
fn sincos_eval(u: Dd) -> (Dd, Dd, f64) {
  let u_hi_sq = u.hi * u.hi;
  let p1 = u_hi_sq.mul_add(c(0xbf2a01a01a01a01a), c(0x3f81111111111111));
  let q1 = u_hi_sq.mul_add(c(0x3fa5555555555555), -0.5);
  let u_hi_3 = u_hi_sq * u.hi;
  let p2 = u_hi_sq.mul_add(p1, c(0xbfc5555555555555));
  let q2 = u_hi_sq.mul_add(q1, 1.0);
  let sin_lo = u_hi_3.mul_add(p2, u.lo * q2);
  let u_hi_neg_half = -0.5 * u.hi;
  let v_hi = u.hi.mul_add(u_hi_neg_half, 1.0);
  let v_lo = u.hi.mul_add(u_hi_neg_half, 1.0 - v_hi);
  let r1 = u_hi_sq.mul_add(c(0x3efa01a01a01a01a), c(0xbf56c16c16c16c17));
  let s1 = u_hi_sq.mul_add(c(0x3fc5555555555555), -1.0);
  let u_hi_4 = u_hi_sq * u_hi_sq;
  let u_hi_u_lo = u.hi * u.lo;
  let r2 = u_hi_sq.mul_add(r1, c(0x3fa5555555555555));
  let s2 = u_hi_u_lo.mul_add(s1, v_lo);
  let cos_lo = u_hi_4.mul_add(r2, s2);
  let err = u_hi_3.abs().mul_add(c(0x3cc0000000000000), c(0x3960000000000000));
  (exact_add(u.hi, sin_lo), exact_add(v_hi, cos_lo), err)
}

const SIN_COEFFS: [Dyadic; 7] = [
  Dyadic::new(false, -127, 0x80000000000000000000000000000000),
  Dyadic::new(true, -130, 0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaab),
  Dyadic::new(false, -134, 0x88888888888888888888888888888889),
  Dyadic::new(true, -140, 0xd00d00d00d00d00d00d00d00d00d00d0),
  Dyadic::new(false, -146, 0xb8ef1d2ab6399c7d560e4472800b8ef2),
  Dyadic::new(true, -153, 0xd7322b3faa271c7f3a3f25c1bee38f10),
  Dyadic::new(false, -160, 0xb092309d43684be51c198e91d7b4269e),
];

const COS_COEFFS: [Dyadic; 7] = [
  Dyadic::new(false, -127, 0x80000000000000000000000000000000),
  Dyadic::new(true, -128, 0x80000000000000000000000000000000),
  Dyadic::new(false, -132, 0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaab),
  Dyadic::new(true, -137, 0xb60b60b60b60b60b60b60b60b60b60b6),
  Dyadic::new(false, -143, 0xd00d00d00d00d00d00d00d00d00d00d0),
  Dyadic::new(true, -149, 0x93f27dbbc4fae397780b69f5333c725b),
  Dyadic::new(false, -156, 0x8f76c77fc6c4bdaa26d4c3d67f425f60),
];

fn sincos_eval_f128(u: Dyadic) -> (Dyadic, Dyadic) {
  let u_sq = quick_mul(u, u);
  (quick_mul(u, polyeval(u_sq, &SIN_COEFFS)), polyeval(u_sq, &COS_COEFFS))
}

fn sin_k_f128(k: u32) -> Dyadic {
  let idx = if k & 64 != 0 { 64 - (k & 63) } else { k & 63 };
  let ans = SIN_K_PI_OVER_128_F128[idx as usize];
  if k & 128 != 0 {
    Dyadic { neg: true, ..ans }
  } else {
    ans
  }
}

fn reduced_f128(x: f64, large: &LargeRangeReduction) -> Dyadic {
  match biased_exponent(x) < EXP_BIAS + FAST_PASS_EXPONENT {
    true => range_reduction_small_f128(x),
    false => large.accurate(),
  }
}

fn sin_accurate(x: f64, k: u32, large: &LargeRangeReduction) -> f64 {
  let (sin_u, cos_u) = sincos_eval_f128(reduced_f128(x, large));
  let sin_k = sin_k_f128(k);
  let cos_k = sin_k_f128(k.wrapping_add(64));
  quick_add(quick_mul(sin_k, cos_u), quick_mul(cos_k, sin_u)).to_f64()
}

fn cos_accurate(x: f64, k: u32, large: &LargeRangeReduction) -> f64 {
  let (sin_u, cos_u) = sincos_eval_f128(reduced_f128(x, large));
  let msin_k = sin_k_f128(k.wrapping_add(128));
  let cos_k = sin_k_f128(k.wrapping_add(64));
  quick_add(quick_mul(cos_k, cos_u), quick_mul(msin_k, sin_u)).to_f64()
}

/// `Math.sin(x)`.
pub fn sin(x: f64) -> f64 {
  let x_e = biased_exponent(x);
  let mut large = LargeRangeReduction::default();
  let (k, y) = if x_e < EXP_BIAS + FAST_PASS_EXPONENT {
    if x_e < EXP_BIAS - 4 {
      if x_e < EXP_BIAS - 26 {
        if x == 0.0 {
          return x + x;
        }
        return x.mul_add(c(0xbc90000000000000), x);
      }
      let x_sq = x * x;
      let c0 = x_sq.mul_add(c(0x3f8111111110f491), c(0xbfc5555555555555));
      let c1 = x_sq.mul_add(c(0x3ec71c24233f1baf), c(0xbf2a01a00e16af3e));
      let x4 = x_sq * x_sq;
      let x3 = x * x_sq;
      let r_lo = x4.mul_add(c1, c0) * x3;
      let err = x_sq.mul_add(c(0x3cb0000000000000), c(0x3bb0000000000000));
      let r_upper = x + x.mul_add(err, r_lo);
      let r_lower = x + (-x).mul_add(err, r_lo);
      if r_upper == r_lower {
        return r_upper;
      }
      let (k, _) = range_reduction_small(x);
      return sin_accurate(x, k, &large);
    }
    range_reduction_small(x)
  } else {
    if x_e > 2 * EXP_BIAS {
      return f64::NAN;
    }
    large.fast(x)
  };
  let (sin_y, cos_y, err) = sincos_eval(y);
  let sin_k = SIN_K_PI_OVER_128[(k & 255) as usize];
  let cos_k = SIN_K_PI_OVER_128[(k.wrapping_add(64) & 255) as usize];
  let sin_k_cos_y = quick_mult(cos_y, sin_k);
  let cos_k_sin_y = quick_mult(sin_y, cos_k);
  let mut rr = exact_add(sin_k_cos_y.hi, cos_k_sin_y.hi);
  rr.lo += sin_k_cos_y.lo + cos_k_sin_y.lo;
  let r_upper = rr.hi + (rr.lo + err);
  let r_lower = rr.hi + (rr.lo - err);
  if r_upper == r_lower {
    return r_upper;
  }
  sin_accurate(x, k, &large)
}

/// `Math.cos(x)`.
pub fn cos(x: f64) -> f64 {
  let x_e = biased_exponent(x);
  let mut large = LargeRangeReduction::default();
  let (k, y) = if x_e < EXP_BIAS + FAST_PASS_EXPONENT {
    if x_e < EXP_BIAS - 4 {
      if x_e < EXP_BIAS - 27 {
        return 1.0;
      }
      let x_sq = x * x;
      let c0 = x_sq.mul_add(c(0x3fa5555555555262), -0.5);
      let c1 = x_sq.mul_add(c(0x3efa00ffd769159a), c(0xbf56c16c1508bff1));
      let x4 = x_sq * x_sq;
      let r_lo = x4.mul_add(c1, c0) * x_sq;
      let err = x_sq.mul_add(c(0x3cc0000000000000), c(0x3ba0000000000000));
      let r_upper = 1.0 + (r_lo + err);
      let r_lower = 1.0 + (r_lo - err);
      if r_upper == r_lower {
        return r_upper;
      }
      let (k, _) = range_reduction_small(x);
      return cos_accurate(x, k, &large);
    }
    range_reduction_small(x)
  } else {
    if x_e > 2 * EXP_BIAS {
      return f64::NAN;
    }
    large.fast(x)
  };
  let (sin_y, cos_y, err) = sincos_eval(y);
  let msin_k = SIN_K_PI_OVER_128[(k.wrapping_add(128) & 255) as usize];
  let cos_k = SIN_K_PI_OVER_128[(k.wrapping_add(64) & 255) as usize];
  let cos_k_cos_y = quick_mult(cos_y, cos_k);
  let msin_k_sin_y = quick_mult(sin_y, msin_k);
  let mut rr = exact_add(cos_k_cos_y.hi, msin_k_sin_y.hi);
  rr.lo += msin_k_sin_y.lo + cos_k_cos_y.lo;
  let r_upper = rr.hi + (rr.lo + err);
  let r_lower = rr.hi + (rr.lo - err);
  if r_upper == r_lower {
    return r_upper;
  }
  cos_accurate(x, k, &large)
}
