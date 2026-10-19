//! LLVM libc's `DyadicFloat<128>`, `FPUtil/dyadic_float.h`: the accurate passes compute in it, so its rounding is reproduced step for step.

pub(super) const EXP_BIAS: i32 = 1023;

/// `(-1)^neg * mant * 2^exp`; normal when `mant` has its top bit set or is zero.
#[derive(Clone, Copy, Debug)]
pub(super) struct Dyadic {
  pub(super) neg: bool,
  pub(super) exp: i32,
  pub(super) mant: u128,
}

impl Dyadic {
  pub(super) const ZERO: Dyadic = Dyadic { neg: false, exp: 0, mant: 0 };

  pub(super) const fn new(neg: bool, exp: i32, mant: u128) -> Self {
    let mut d = Dyadic { neg, exp, mant };
    if mant != 0 {
      let shift = mant.leading_zeros();
      d.exp -= shift as i32;
      d.mant <<= shift;
    }
    d
  }

  pub(super) fn from_f64(x: f64) -> Self {
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

  pub(super) fn shift_right(&mut self, n: u32) {
    self.exp += n as i32;
    self.mant = if n < 128 { self.mant >> n } else { 0 };
  }

  pub(super) fn negated(self) -> Self {
    Dyadic { neg: !self.neg, ..self }
  }

  /// The double nearest the value, ties to even, as `fast_as` rounds under rounding to nearest.
  pub(super) fn to_f64(self) -> f64 {
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

pub(super) fn quick_add(mut a: Dyadic, mut b: Dyadic) -> Dyadic {
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

pub(super) fn quick_sub(a: Dyadic, b: Dyadic) -> Dyadic {
  quick_add(a, b.negated())
}

pub(super) fn quick_mul(a: Dyadic, b: Dyadic) -> Dyadic {
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
pub(super) fn polyeval(x: Dyadic, coeffs: &[Dyadic]) -> Dyadic {
  let (last, rest) = coeffs.split_last().expect("a coefficient");
  rest.iter().rev().fold(*last, |acc, coeff| quick_add(*coeff, quick_mul(x, acc)))
}
