//! `sinh`, `cosh`, `asinh`, `acosh` and `atanh` as V8 computes them in src/base/ieee754.cc: fdlibm's algorithms over the LLVM libc `log`, `log1p`, `exp` and `expm1` ported beside them.
//!
//! Copyright (C) 1993 by Sun Microsystems, Inc. All rights reserved.
//! Developed at SunSoft, a Sun Microsystems, Inc. business.
//! Permission to use, copy, modify, and distribute this
//! software is freely granted, provided that this notice
//! is preserved.

use super::exp::{exp, expm1};
use super::log::{ln, log1p};

const LN2: f64 = 6.93147180559945286227e-01;

fn high_word(x: f64) -> i32 {
  (x.to_bits() >> 32) as u32 as i32
}

fn low_word(x: f64) -> u32 {
  x.to_bits() as u32
}

pub(super) fn sinh(x: f64) -> f64 {
  let h = if x < 0.0 { -0.5 } else { 0.5 };
  let ax = x.abs();
  if ax < 22.0 {
    if ax < 3.725290298461914e-9 {
      return x;
    }
    let t = expm1(ax);
    if ax < 1.0 {
      return h * (2.0 * t - t * t / (t + 1.0));
    }
    return h * (t + t / (t + 1.0));
  }
  if ax < 709.7822265625 {
    return h * exp(ax);
  }
  if ax <= 710.4758600739439 {
    let w = exp(0.5 * ax);
    let t = h * w;
    return t * w;
  }
  x * 1.0e307
}

pub(super) fn cosh(x: f64) -> f64 {
  let ix = high_word(x) & 0x7fff_ffff;
  if ix < 0x3fd6_2e43 {
    let t = expm1(x.abs());
    let w = 1.0 + t;
    if ix < 0x3c80_0000 {
      return w;
    }
    return 1.0 + (t * t) / (w + w);
  }
  if ix < 0x4036_0000 {
    let t = exp(x.abs());
    return 0.5 * t + 0.5 / t;
  }
  if ix < 0x4086_2e42 {
    return 0.5 * exp(x.abs());
  }
  if x.abs() <= 710.4758600739439 {
    let w = exp(0.5 * x.abs());
    let t = 0.5 * w;
    return t * w;
  }
  if ix >= 0x7ff0_0000 {
    return x * x;
  }
  f64::INFINITY
}

pub(super) fn asinh(x: f64) -> f64 {
  let hx = high_word(x);
  let ix = hx & 0x7fff_ffff;
  if ix >= 0x7ff0_0000 {
    return x + x;
  }
  if ix < 0x3e30_0000 {
    return x;
  }
  let w = if ix > 0x41b0_0000 {
    ln(x.abs()) + LN2
  } else if ix > 0x4000_0000 {
    let t = x.abs();
    ln(2.0 * t + 1.0 / ((x * x + 1.0).sqrt() + t))
  } else {
    let t = x * x;
    log1p(x.abs() + t / (1.0 + (1.0 + t).sqrt()))
  };
  if hx > 0 {
    w
  } else {
    -w
  }
}

pub(super) fn acosh(x: f64) -> f64 {
  let hx = high_word(x);
  let lx = low_word(x);
  if hx < 0x3ff0_0000 {
    return f64::NAN;
  }
  if hx >= 0x41b0_0000 {
    if hx >= 0x7ff0_0000 {
      return x + x;
    }
    return ln(x) + LN2;
  }
  if (hx - 0x3ff0_0000) as u32 | lx == 0 {
    return 0.0;
  }
  if hx > 0x4000_0000 {
    let t = x * x;
    return ln(2.0 * x - 1.0 / (x + (t - 1.0).sqrt()));
  }
  let t = x - 1.0;
  log1p(t + (2.0 * t + t * t).sqrt())
}

pub(super) fn atanh(x: f64) -> f64 {
  let hx = high_word(x);
  let lx = low_word(x);
  let ix = hx & 0x7fff_ffff;
  if (ix | ((lx | lx.wrapping_neg()) >> 31) as i32) > 0x3ff0_0000 {
    return f64::NAN;
  }
  if ix == 0x3ff0_0000 {
    return if x > 0.0 { f64::INFINITY } else { f64::NEG_INFINITY };
  }
  if ix < 0x3e30_0000 {
    return x;
  }
  let x = x.abs();
  let t = if ix < 0x3fe0_0000 {
    let t = x + x;
    0.5 * log1p(t + t * x / (1.0 - x))
  } else {
    0.5 * log1p((x + x) / (1.0 - x))
  };
  if hx >= 0 {
    t
  } else {
    -t
  }
}
