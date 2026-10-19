//! `log`, `log2`, `log10` and `log1p`: libc/src/__support/math/log.h, log2.h, log10.h, log1p.h, log_range_reduction.h and common_constants.h.

use super::c;
use super::dd::{exact_add, exact_mult, quick_mult, Dd};
use super::dyadic::{quick_add, quick_mul, Dyadic};
use super::log_tables::{LOG_COEFFS, LOG_R_DD, RD, S2, S3, S4};
use super::{ln_tables, log10_tables, log1p_tables, log2_tables};

const LOG_2_HI: u64 = 0x3fe62e42fefa3800;
const LOG_2_LO: u64 = 0x3d2ef35793c76730;
const HI_ERR: u64 = 0x3aa0000000000000;

/// A logarithm's `-log(r)` for each of the four range reduction steps.
struct LogRr {
  step_1: &'static [Dyadic; 128],
  step_2: &'static [Dyadic; 193],
  step_3: &'static [Dyadic; 161],
  step_4: &'static [Dyadic; 130],
}

const LN_RR: LogRr = LogRr { step_1: &ln_tables::LN_TABLE_STEP_1, step_2: &ln_tables::LN_TABLE_STEP_2, step_3: &ln_tables::LN_TABLE_STEP_3, step_4: &ln_tables::LN_TABLE_STEP_4 };
const LOG2_RR: LogRr = LogRr { step_1: &log2_tables::LOG2_TABLE_STEP_1, step_2: &log2_tables::LOG2_TABLE_STEP_2, step_3: &log2_tables::LOG2_TABLE_STEP_3, step_4: &log2_tables::LOG2_TABLE_STEP_4 };
const LOG10_RR: LogRr = LogRr { step_1: &log10_tables::LOG10_TABLE_STEP_1, step_2: &log10_tables::LOG10_TABLE_STEP_2, step_3: &log10_tables::LOG10_TABLE_STEP_3, step_4: &log10_tables::LOG10_TABLE_STEP_4 };

/// Steps 2 to 4 of the range reduction in exact fixed point: adds their `-log(r)` to `sum` and answers the reduced argument, `|v| < 2^-29`.
fn log_range_reduction(m_x: f64, table: &LogRr, sum: &mut Dyadic) -> Dyadic {
  let v = (m_x * c(0x43b0000000000000)) as i64;
  let idx2 = ((v + 0x0010_2000_0000_0000) >> 46) as usize;
  *sum = quick_add(*sum, table.step_2[idx2]);
  let s2 = S2[idx2] as i64;
  let sv2 = s2 * v;
  let spv2 = (s2 << 44) + v;
  let vv2 = (spv2 << 16) + sv2;
  let idx3 = ((vv2 + 0x2840_0000_0000_0000) >> 55) as usize;
  *sum = quick_add(*sum, table.step_3[idx3]);
  let s3 = S3[idx3] as i64;
  let spv3 = (s3 << 55) + vv2;
  let sv3 = s3 as i128 * vv2 as i128;
  let vv3 = ((spv3 as i128) << 21) + sv3;
  let idx4 = (((vv3 >> 68) as i32 + 131) >> 1) as usize;
  *sum = quick_add(*sum, table.step_4[idx4]);
  let s4 = S4[idx4] as i128;
  let spv4 = (s4 << 69) + vv3;
  let sv4 = s4 * vv3;
  let vv4 = (spv4 << 28) + sv4;
  Dyadic::new(vv4 < 0, -125, vv4.unsigned_abs())
}

/// `x = 2^x_e * m` with `m` in `[1, 2)` split at the table index; `Err` holds the value a logarithm answers outright.
struct Split {
  x_e: i32,
  index: usize,
  u: f64,
}

fn split(x: f64) -> Result<Split, f64> {
  let mut x_u = x.to_bits();
  let mut x_e = -1023;
  if x_u == 1.0f64.to_bits() {
    return Err(0.0);
  }
  if x_u < 0x0010_0000_0000_0000 || x_u > 0x7fef_ffff_ffff_ffff {
    if x == 0.0 {
      return Err(f64::NEG_INFINITY);
    }
    if x.is_sign_negative() && !x.is_nan() {
      return Err(f64::NAN);
    }
    if x.is_infinite() || x.is_nan() {
      return Err(x);
    }
    x_u = (x * c(0x4330000000000000)).to_bits();
    x_e -= 52;
  }
  let index = ((x_u >> 45) as i32 & 0x7f) as usize;
  x_e += ((x_u + (1 << 45)) >> 52) as i32;
  let m = f64::from_bits((x_u & 0x000f_ffff_ffff_ffff) | 0x3ff0_0000_0000_0000);
  Ok(Split { x_e, index, u: RD[index].mul_add(m, -1.0) })
}

fn coeffs_poly(u: f64, u_sq: f64, a0: f64) -> f64 {
  let p0 = u.mul_add(LOG_COEFFS[1], LOG_COEFFS[0]);
  let p1 = u.mul_add(LOG_COEFFS[3], LOG_COEFFS[2]);
  let p2 = u.mul_add(LOG_COEFFS[5], LOG_COEFFS[4]);
  u_sq.mul_add(u_sq.mul_add(u_sq.mul_add(p2, p1), p0), a0)
}

fn big_poly(v: Dyadic, coeffs: &[Dyadic]) -> Dyadic {
  let mut p = quick_mul(v, coeffs[0]);
  for coeff in &coeffs[1..] {
    p = quick_mul(v, quick_add(p, *coeff));
  }
  p
}

pub(super) fn ln(x: f64) -> f64 {
  let Split { x_e, index, u } = match split(x) {
    Ok(s) => s,
    Err(v) => return v,
  };
  let e_x = x_e as f64;
  let hi = e_x.mul_add(c(LOG_2_HI), LOG_R_DD[index].hi);
  let lo = e_x.mul_add(c(LOG_2_LO), LOG_R_DD[index].lo);
  let r1 = exact_add(hi, u);
  let u_sq = u * u;
  let p = coeffs_poly(u, u_sq, lo + r1.lo);
  let err = u_sq.mul_add(c(0x3cd0000000000000), c(HI_ERR));
  let left = r1.hi + (p - err);
  let right = r1.hi + (p + err);
  if left == right {
    return left;
  }
  let mut sum = quick_mul(ln_tables::LN_2, Dyadic::from_f64(x_e as f64));
  sum = quick_add(sum, LN_RR.step_1[index]);
  let v = log_range_reduction(u, &LN_RR, &mut sum);
  sum = quick_add(sum, v);
  let p = quick_mul(v, big_poly(v, &ln_tables::BIG_COEFFS));
  quick_add(sum, p).to_f64()
}

pub(super) fn log2(x: f64) -> f64 {
  let Split { x_e, index, u } = match split(x) {
    Ok(s) => s,
    Err(v) => return v,
  };
  let e_x = x_e as f64;
  let log_r1 = log2_tables::LOG_R1[index];
  let mut r1 = exact_add(log_r1.hi, u);
  let u_sq = u * u;
  r1.lo += coeffs_poly(u, u_sq, log_r1.lo);
  let log2_e = Dd { lo: c(0x3c7777d0ffda0d24), hi: c(0x3ff71547652b82fe) };
  let r2 = quick_mult(r1, log2_e);
  let mut r3 = exact_add(e_x, r2.hi);
  r3.lo += r2.lo;
  let err = u_sq * c(0x3ce0000000000000);
  let left = r3.hi + (r3.lo - err);
  let right = r3.hi + (r3.lo + err);
  if left == right {
    return left;
  }
  let mut sum = Dyadic::from_f64(x_e as f64);
  sum = quick_add(sum, LOG2_RR.step_1[index]);
  let v = log_range_reduction(u, &LOG2_RR, &mut sum);
  quick_add(sum, big_poly(v, &log2_tables::BIG_COEFFS)).to_f64()
}

/// `log10(10^n)` for the powers of ten a double holds exactly.
const POWERS_OF_TEN: [u64; 22] = [
  0x4024000000000000, 0x4059000000000000, 0x408f400000000000, 0x40c3880000000000, 0x40f86a0000000000, 0x412e848000000000, 0x416312d000000000, 0x4197d78400000000, 0x41cdcd6500000000, 0x4202a05f20000000, 0x42374876e8000000,
  0x426d1a94a2000000, 0x42a2309ce5400000, 0x42d6bcc41e900000, 0x430c6bf526340000, 0x4341c37937e08000, 0x4376345785d8a000, 0x43abc16d674ec800, 0x43e158e460913d00, 0x4415af1d78b58c40, 0x444b1ae4d6e2ef50, 0x4480f0cf064dd592,
];

pub(super) fn log10(x: f64) -> f64 {
  let Split { x_e, index, u } = match split(x) {
    Ok(s) => s,
    Err(v) => return v,
  };
  let e_x = x_e as f64;
  let hi = e_x.mul_add(c(LOG_2_HI), LOG_R_DD[index].hi);
  let lo = e_x.mul_add(c(LOG_2_LO), LOG_R_DD[index].lo);
  let u_sq = u * u;
  let p = coeffs_poly(u, u_sq, lo);
  let mut r1 = exact_add(hi, u);
  r1.lo += p;
  let log10_e = Dd { lo: c(0x3c695355baaafad3), hi: c(0x3fdbcb7b1526e50e) };
  let r2 = quick_mult(r1, log10_e);
  let err = u_sq.mul_add(c(0x3cc0000000000000), c(HI_ERR));
  let left = r2.hi + (r2.lo - err);
  let right = r2.hi + (r2.lo + err);
  if left == right {
    return left;
  }
  if let Some(n) = POWERS_OF_TEN.iter().position(|bits| *bits == x.to_bits()) {
    return (n + 1) as f64;
  }
  let mut sum = quick_mul(log10_tables::LOG10_2, Dyadic::from_f64(x_e as f64));
  sum = quick_add(sum, LOG10_RR.step_1[index]);
  let v = log_range_reduction(u, &LOG10_RR, &mut sum);
  quick_add(sum, big_poly(v, &log10_tables::BIG_COEFFS)).to_f64()
}

fn log1p_accurate(e_x: i32, index: usize, m_x: Dd) -> f64 {
  let mut sum = quick_mul(log1p_tables::LN_2, Dyadic::from_f64(e_x as f64));
  sum = quick_add(sum, log1p_tables::LOG_R1[index]);
  let mut v = quick_add(Dyadic::from_f64(m_x.hi), Dyadic::from_f64(m_x.lo));
  if m_x.hi > c(0x3f00000000000000) || m_x.hi < -c(0x3f00000000000000) {
    let idx2 = (c(0x40d0000000000000) * (m_x.hi + (91.0 * c(0x3f10000000000000) + c(0x3f00000000000000)))) as i32 as usize;
    sum = quick_add(sum, log1p_tables::LOG_R2[idx2]);
    let s2 = Dyadic::from_f64(log1p_tables::S2[idx2]);
    v = quick_add(quick_add(v, s2), quick_mul(v, s2));
  }
  if v.exp > -150 {
    let idx3 = (c(0x4140000000000000) * (v.to_f64() + (69.0 * c(0x3ea0000000000000) + c(0x3e90000000000000)))) as i32 as usize;
    sum = quick_add(sum, log1p_tables::LOG_R3[idx3]);
    let s3 = Dyadic::from_f64(log1p_tables::S3[idx3]);
    v = quick_add(quick_add(v, s3), quick_mul(v, s3));
  }
  let p = quick_add(v, quick_mul(v, big_poly(v, &log1p_tables::BIG_COEFFS)));
  quick_add(sum, p).to_f64()
}

pub(super) fn log1p(x: f64) -> f64 {
  let mut x_u = x.to_bits();
  let x_exp = ((x_u >> 52) & 0x7ff) as i32;
  let x_dd = if x_exp >= 1023 {
    if x_u >= 0x4650_0000_0000_0000 {
      if x_u > 0x7fef_ffff_ffff_ffff {
        if x_u == 0xbff0_0000_0000_0000 {
          return f64::NEG_INFINITY;
        }
        if x.is_sign_negative() && !x.is_nan() {
          return f64::NAN;
        }
        return x;
      }
      Dd { lo: 0.0, hi: x }
    } else {
      exact_add(x, 1.0)
    }
  } else {
    if x_exp < 1023 - 52 - 1 {
      return x;
    }
    exact_add(1.0, x)
  };
  let xhi_frac = x_dd.hi.to_bits() & ((1 << 52) - 1);
  x_u = x_dd.hi.to_bits();
  let idx = ((xhi_frac + (1 << 44)) >> 45) as usize;
  let x_e = ((x_u >> 52) & 0x7ff) as i32 - 1023 + (idx as i32 >> 7);
  let e_x = x_e as f64;
  let hi = e_x.mul_add(c(LOG_2_HI), log1p_tables::LOG_R1_DD[idx].hi);
  let lo = e_x.mul_add(c(LOG_2_LO), log1p_tables::LOG_R1_DD[idx].lo);
  let err_hi = if hi == 0.0 { 0.0 } else { c(HI_ERR) };
  let s_u = (x_u & 0x7ff0_0000_0000_0000) as i64 - (1023i64 << 52);
  let m_hi = 0x3ff0_0000_0000_0000 | xhi_frac;
  let m_lo = if x_dd.lo.abs() > x_dd.hi * c(0x3800000000000000) { (x_dd.lo.to_bits() as i64).wrapping_sub(s_u) as u64 } else { 0 };
  let m_dd = Dd { lo: f64::from_bits(m_lo), hi: f64::from_bits(m_hi) };
  let r = log1p_tables::R1[idx];
  let v_lo = exact_mult(m_dd.lo, r);
  let v_hi = r.mul_add(m_dd.hi, -1.0);
  let mut v_dd = exact_add(v_hi, v_lo.hi);
  v_dd.lo += v_lo.lo;
  let r1 = exact_add(hi, v_dd.hi);
  let v_sq = v_dd.hi * v_dd.hi;
  let coeffs = &log1p_tables::P_COEFFS;
  let p0 = v_dd.hi.mul_add(coeffs[1], coeffs[0]);
  let p1 = v_dd.hi.mul_add(coeffs[3], coeffs[2]);
  let p2 = v_dd.hi.mul_add(coeffs[5], coeffs[4]);
  let p = v_sq.mul_add(v_sq.mul_add(v_sq.mul_add(p2, p1), p0), (v_dd.lo + r1.lo) + lo);
  let err = v_sq.mul_add(c(0x3ce0000000000000), err_hi);
  let left = r1.hi + (p - err);
  let right = r1.hi + (p + err);
  if left == right {
    return left;
  }
  log1p_accurate(x_e, idx, v_dd)
}
