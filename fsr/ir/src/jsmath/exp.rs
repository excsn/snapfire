//! `exp` and `expm1`: libc/src/__support/math/exp.h, expm1.h and the denormal Ziv test of exp_utils.h.

use super::c;
use super::dd::{add, exact_add, polyeval, quick_mult, Dd};
use super::dyadic::{self, quick_add, quick_mul, Dyadic};
use super::exp_tables::{EXP2_MID1, EXP2_MID2};

const LOG2_E: u64 = 0x3ff71547652b82fe;
const MLOG_2_EXP2_M12_HI: u64 = 0xbf262e42ff000000;
const MLOG_2_EXP2_M12_MID: u64 = 0x3d0718432a1b0e26;
const MLOG_2_EXP2_M12_MID_30: u64 = 0x3d0718432a000000;
const MLOG_2_EXP2_M12_LO: u64 = 0x3b0b0e2633fe0685;

/// `x = (hi + idx1 / 64 + idx2 / 4096) * log(2) + dx`, from `k = round(x * 4096 / log(2))`.
struct Reduction {
  kd: f64,
  idx1: usize,
  idx2: usize,
  hi: i32,
  exp_mid: Dd,
}

fn reduce(x: f64) -> Reduction {
  let tmp = x.mul_add(c(LOG2_E), c(0x4148000000040000));
  let k = (tmp.to_bits() >> 19) as u32 as i32;
  let idx1 = ((k >> 6) & 0x3f) as usize;
  let idx2 = (k & 0x3f) as usize;
  let exp_mid1 = Dd { lo: EXP2_MID1[idx1].mid, hi: EXP2_MID1[idx1].hi };
  let exp_mid2 = Dd { lo: EXP2_MID2[idx2].mid, hi: EXP2_MID2[idx2].hi };
  Reduction { kd: k as f64, idx1, idx2, hi: k >> 12, exp_mid: quick_mult(exp_mid1, exp_mid2) }
}

/// `x - kd * log(2) / 4096` to double precision.
fn dx_d(x: f64, kd: f64) -> f64 {
  let lo_h = kd.mul_add(c(MLOG_2_EXP2_M12_HI), x);
  kd.mul_add(c(MLOG_2_EXP2_M12_MID), lo_h)
}

/// `x - kd * log(2) / 4096` as a double-double.
fn dx_dd(x: f64, kd: f64) -> Dd {
  let t1 = kd.mul_add(c(MLOG_2_EXP2_M12_HI), x);
  let t2 = kd * c(MLOG_2_EXP2_M12_MID_30);
  let t3 = kd * c(MLOG_2_EXP2_M12_LO);
  let mut dx = exact_add(t1, t2);
  dx.lo += t3;
  dx
}

fn dx_f128(x: f64, kd: f64) -> Dyadic {
  let t1 = kd.mul_add(c(MLOG_2_EXP2_M12_HI), x);
  let t2 = kd * c(MLOG_2_EXP2_M12_MID_30);
  let t3 = kd * c(MLOG_2_EXP2_M12_LO);
  quick_add(Dyadic::from_f64(t1), quick_add(Dyadic::from_f64(t2), Dyadic::from_f64(t3)))
}

/// `2^(idx1 / 64 + idx2 / 4096)` in 128 bits.
fn exp_mid_f128(idx1: usize, idx2: usize) -> Dyadic {
  let part = |t: super::dd::Td| quick_add(Dyadic::from_f64(t.hi), quick_add(Dyadic::from_f64(t.mid), Dyadic::from_f64(t.lo)));
  quick_mul(part(EXP2_MID1[idx1]), part(EXP2_MID2[idx2]))
}

/// `1 + dx/2 + dx^2/6 + dx^3/24`, the fast pass's `(exp(dx) - 1) / dx`.
fn poly_approx_d(dx: f64) -> f64 {
  let dx2 = dx * dx;
  let c0 = dx.mul_add(0.5, 1.0);
  let c1 = dx.mul_add(c(0x3fa5555555555555), c(0x3fc5555555555555));
  dx2.mul_add(c1, c0)
}

const DD_COEFFS: [Dd; 8] = [
  Dd { lo: 0.0, hi: 1.0 },
  Dd { lo: 0.0, hi: 1.0 },
  Dd { lo: 0.0, hi: 0.5 },
  Dd { lo: f64::from_bits(0x3c65555555555555), hi: f64::from_bits(0x3fc5555555555555) },
  Dd { lo: f64::from_bits(0x3c45555555555555), hi: f64::from_bits(0x3fa5555555555555) },
  Dd { lo: f64::from_bits(0x3c01111111111111), hi: f64::from_bits(0x3f81111111111111) },
  Dd { lo: f64::from_bits(0xbbef49f49f49f49f), hi: f64::from_bits(0x3f56c16c16c16c17) },
  Dd { lo: f64::from_bits(0x3b6a01a01a01a01a), hi: f64::from_bits(0x3f2a01a01a01a01a) },
];

const F128_COEFFS: [Dyadic; 8] = [
  Dyadic::new(false, -127, 0x80000000000000000000000000000000),
  Dyadic::new(false, -127, 0x80000000000000000000000000000000),
  Dyadic::new(false, -128, 0x80000000000000000000000000000000),
  Dyadic::new(false, -130, 0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaab),
  Dyadic::new(false, -132, 0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaab),
  Dyadic::new(false, -134, 0x88888888888888888888888888888889),
  Dyadic::new(false, -137, 0xb60b60b60b60b60b60b60b60b60b60b6),
  Dyadic::new(false, -140, 0xd00d00d00d00d00d00d00d00d00d00d0),
];

/// `bits(r) + hi * 2^52`, the exponent of `r` raised by `hi`.
fn scale(r: f64, hi: i32) -> f64 {
  f64::from_bits(((hi as i64) << 52).wrapping_add(r.to_bits() as i64) as u64)
}

/// The rounding of `2^hi * (mid + lo)` for a result below the normal range, when `err` leaves it certain.
fn ziv_test_denorm(hi: i32, mid: f64, lo: f64, mut err: f64) -> Option<f64> {
  let exp_hi = ((hi + 1022) as i64) << 52;
  let shifted = |v: f64| f64::from_bits(exp_hi.wrapping_add(v.to_bits() as i64) as u64);
  let mid_hi = shifted(mid);
  let lo_scaled = if lo != 0.0 { shifted(lo) } else { 0.0 };
  let mut extra_factor = 0.0;
  let mut scale_down: u64 = 0x3fe0_0000_0000_0000;
  if (1.0 - mid_hi) > lo_scaled {
    err += c(0x3cb0000000000000);
    extra_factor = 1.0;
    scale_down = 0x3ff0_0000_0000_0000;
  }
  let err_scaled = shifted(err);
  let upper = extra_factor + (mid_hi + (lo_scaled + err_scaled));
  let lower = extra_factor + (mid_hi + (lo_scaled - err_scaled));
  (upper == lower).then(|| f64::from_bits(upper.to_bits().wrapping_sub(scale_down)))
}

pub(super) fn exp(x: f64) -> f64 {
  let x_u = x.to_bits();
  if x_u >= 0xc0874910d52d3052 || (x_u < 0xbca0000000000000 && x_u >= 0x40862e42fefa39f0) || x_u < 0x3ca0000000000000 {
    let x_abs = x_u & !(1 << 63);
    if x_abs <= 0x3ca0_0000_0000_0000 {
      return 1.0 + x;
    }
    if x_u >= 0xc087_4910_d52d_3052 {
      if x.is_nan() {
        return x;
      }
      return 0.0;
    }
    return x + f64::INFINITY;
  }
  let r = reduce(x);
  let denorm = r.hi <= -1022;
  let dx = dx_d(x, r.kd);
  let mid_lo = dx * r.exp_mid.hi;
  let p = poly_approx_d(dx);
  let lo = p.mul_add(mid_lo, r.exp_mid.lo);
  let err_d = c(0x3c08000000000000);
  if denorm {
    if let Some(v) = ziv_test_denorm(r.hi, r.exp_mid.hi, lo, err_d) {
      return v;
    }
  } else {
    let upper = r.exp_mid.hi + (lo + err_d);
    let lower = r.exp_mid.hi + (lo - err_d);
    if upper == lower {
      return scale(upper, r.hi);
    }
  }
  let r_dd = quick_mult(r.exp_mid, polyeval(dx_dd(x, r.kd), &DD_COEFFS[..7]));
  let err_dd = c(0x39c0000000000000);
  if denorm {
    if let Some(v) = ziv_test_denorm(r.hi, r_dd.hi, r_dd.lo, err_dd) {
      return v;
    }
  } else {
    let upper = r_dd.hi + (r_dd.lo + err_dd);
    let lower = r_dd.hi + (r_dd.lo - err_dd);
    if upper == lower {
      return scale(upper, r.hi);
    }
  }
  let p = dyadic::polyeval(dx_f128(x, r.kd), &F128_COEFFS);
  let mut result = quick_mul(exp_mid_f128(r.idx1, r.idx2), p);
  result.exp += r.kd as i32 >> 12;
  result.to_f64()
}

pub(super) fn expm1(x: f64) -> f64 {
  let x_u = x.to_bits();
  let x_is_neg = x.is_sign_negative();
  if x_u >= 0xc042b708872320e2 || (x_u <= 0xbca0000000000000 && x_u >= 0x40862e42fefa39f0) || x_u <= 0x3ca0000000000000 {
    let x_abs = x_u & !(1 << 63);
    if x_abs <= 0x3ca0_0000_0000_0000 {
      if x_abs <= 0x0370_0000_0000_0000 {
        if x_abs == 0 {
          return x;
        }
        return c(0x3370000000000000) * x.mul_add(c(0x4c70000000000000), c(0x0010000000000000));
      }
      return x;
    }
    if x_u >= 0xc042_b708_8723_20e2 {
      if x.is_infinite() {
        return -1.0;
      }
      if x.is_nan() {
        return x;
      }
      return -1.0;
    }
    return x + f64::INFINITY;
  }
  let r = reduce(x);
  let one_scaled = f64::from_bits((1 << 63) | (((1023 - r.hi) as u64) << 52));
  let mut hi_part = if x_is_neg { exact_add(one_scaled, r.exp_mid.hi) } else { exact_add(r.exp_mid.hi, one_scaled) };
  hi_part.lo += r.exp_mid.lo;
  let dx = dx_d(x, r.kd);
  let mid_lo = dx * r.exp_mid.hi;
  let p = poly_approx_d(dx);
  let lo = p.mul_add(mid_lo, hi_part.lo);
  let err: u64 = if x_is_neg { ((-r.hi) as u64) << 52 } else { 0 };
  let err_d = f64::from_bits(0x3c08000000000000u64.wrapping_add(err));
  let upper = hi_part.hi + (lo + err_d);
  let lower = hi_part.hi + (lo - err_d);
  if upper == lower {
    return scale(upper, r.hi);
  }
  let dx = dx_dd(x, r.kd);
  let p = polyeval(dx, &DD_COEFFS[1..]);
  let r_dd = add(hi_part, quick_mult(quick_mult(r.exp_mid, dx), p));
  let err_dd = f64::from_bits(0x39c0000000000000u64.wrapping_add(err));
  let upper_dd = r_dd.hi + (r_dd.lo + err_dd);
  let lower_dd = r_dd.hi + (r_dd.lo - err_dd);
  if upper_dd == lower_dd {
    return scale(upper_dd, r.hi);
  }
  let dx = dx_f128(x, r.kd);
  let exp_mid = exp_mid_f128(r.idx1, r.idx2);
  let hi = r.kd as i32 >> 12;
  let minus_one = Dyadic::new(true, -127 - hi, 1 << 127);
  let exp_mid_m1 = quick_add(exp_mid, minus_one);
  let p = dyadic::polyeval(dx, &F128_COEFFS[1..]);
  let mut result = quick_add(exp_mid_m1, quick_mul(quick_mul(exp_mid, dx), p));
  result.exp += hi;
  result.to_f64()
}
