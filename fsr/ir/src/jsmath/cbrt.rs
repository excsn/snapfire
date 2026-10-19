//! `cbrt`: libc/src/__support/math/cbrt.h, after Sibidanov and Zimmermann's correctly rounded cube root.

use super::c;
use super::dd::{exact_add, exact_mult, quick_mult_by, Dd};
use super::dyadic::{quick_add, quick_mul, Dyadic};

/// A degree-7 approximation of `x^(-1/3)` on `[1, 2)`.
fn initial_approximation(x: f64) -> f64 {
  let coeffs = [c(0x400bc52aedead5c6), c(0xc01b52bfebf110b3), c(0x4021d8d71d53d126), c(0xc01de2db9e81cf87), c(0x4010154ca06153bd), c(0xbff5973c66ee6da7), c(0x3fd07bf6ac832552), c(0xbf95e53d9ce41cb8)];
  let x_sq = x * x;
  let c0 = x.mul_add(coeffs[1], coeffs[0]);
  let c1 = x.mul_add(coeffs[3], coeffs[2]);
  let c2 = x.mul_add(coeffs[5], coeffs[4]);
  let c3 = x.mul_add(coeffs[7], coeffs[6]);
  let x_4 = x_sq * x_sq;
  let d0 = x_sq.mul_add(c1, c0);
  let d1 = x_sq.mul_add(c3, c2);
  x_4.mul_add(d1, d0)
}

/// `x^3 * a^2 - 1`.
fn get_error(x_3: Dd, a_sq: Dd) -> f64 {
  x_3.hi.mul_add(a_sq.hi, -1.0) + x_3.lo.mul_add(a_sq.hi, x_3.hi * a_sq.lo)
}

pub(super) fn cbrt(x: f64) -> f64 {
  let mut x = x;
  let x_abs = x.to_bits() & !(1 << 63);
  let mut exp_bias_correction: u32 = 682;
  if x_abs < 0x0010_0000_0000_0000 || x_abs >= 0x7ff0_0000_0000_0000 {
    if x == 0.0 || x_abs >= 0x7ff0_0000_0000_0000 {
      return x + x;
    }
    x *= c(0x43b0000000000000);
    exp_bias_correction -= 20;
  }
  let bits = x.to_bits();
  let x_e = ((bits >> 52) & 0x7ff) as u32;
  let out_e = x_e / 3 + exp_bias_correction;
  let shift_e = x_e % 3;
  let x_r = f64::from_bits((bits & ((1 << 52) - 1)) | (1023 << 52));
  let a = f64::from_bits((bits & 0x800f_ffff_ffff_ffff) | (((shift_e + 1023) as u64) << 52));
  let p = initial_approximation(x_r);
  let exp2_m2_over_3 = [1.0, c(0x3fe428a2f98d728b), c(0x3fd965fea53d6e3d)];
  let x0 = exp2_m2_over_3[shift_e as usize] * p;
  let a_sq = exact_mult(a, a);
  let x0_sq = exact_mult(x0, x0);
  let x0_3 = quick_mult_by(x0, x0_sq);
  let h0 = get_error(x0_3, a_sq);
  let rel_error = c(0x3cc0000000000000);
  let err_coeffs = [c(0xbfd5555555555555) - rel_error, c(0x3fcc71c71c71c71c), c(0xbfc61f9add3c0ca4)];
  let e0 = h0.mul_add(h0.mul_add(err_coeffs[2], err_coeffs[1]), err_coeffs[0]);
  let x0_h0 = x0 * h0;
  let x1_dd = Dd { lo: x0_h0 * e0, hi: x0 };
  let r1 = quick_mult_by(a, x1_dd);
  let update_exponent = |r: f64| f64::from_bits((r.to_bits().wrapping_sub(0x3ff0_0000_0000_0000)).wrapping_add((out_e as u64) << 52));
  let r1_lower = r1.hi + r1.lo;
  let r1_upper = r1.hi + x0_h0.mul_add(2.0 * rel_error * a, r1.lo);
  if r1_upper == r1_lower {
    return update_exponent(r1_lower);
  }
  let x1 = x1_dd.hi + (e0 + rel_error) * x0_h0;
  let x1_sq = exact_mult(x1, x1);
  let x1_3 = quick_mult_by(x1, x1_sq);
  let h1 = get_error(x1_3, a_sq);
  let e1 = h1 * (x1 * c(0xbfd5555555555555));
  let x2 = exact_add(x1, e1);
  let r2 = quick_mult_by(a, x2);
  let r2_upper = r2.hi + a.mul_add(c(0x3990000000000000), r2.lo);
  let r2_lower = r2.hi + a.mul_add(-c(0x3990000000000000), r2.lo);
  if r2_upper == r2_lower {
    return update_exponent(r2_upper);
  }
  let x2_f128 = quick_add(Dyadic::from_f64(x2.hi), Dyadic::from_f64(x2.lo));
  let x2_3 = quick_mul(quick_mul(x2_f128, x2_f128), x2_f128);
  let a_sq_f128 = quick_mul(Dyadic::from_f64(a), Dyadic::from_f64(a));
  let x2_3_a_sq = quick_mul(x2_3, a_sq_f128);
  let h2 = quick_add(x2_3_a_sq, Dyadic::from_f64(-1.0)).to_f64();
  let t2 = quick_add(Dyadic::from_f64(1.0), Dyadic::from_f64(h2 * c(0xbfd5555555555555)));
  let x3 = quick_mul(x2_f128, t2);
  let mut r3 = quick_mul(Dyadic::from_f64(a), x3);
  let low_mask: u128 = 0x0000_0000_0000_03ff_ffff_ffff_ffff_ffff;
  let rounding_bits = r3.mant & low_mask;
  let mut result = r3.to_f64();
  if rounding_bits < 0xf || rounding_bits >= 0x0000_0000_0000_03ff_ffff_ffff_ffff_fff0 {
    r3.mant &= !0xf;
    result = if rounding_bits >= 0x0000_0000_0000_03ff_ffff_ffff_ffff_fff0 {
      let tmp = Dyadic::new(r3.neg, r3.exp - 123, 1 << 127);
      quick_add(r3, tmp).to_f64()
    } else {
      r3.to_f64()
    };
  }
  update_exponent(result)
}
