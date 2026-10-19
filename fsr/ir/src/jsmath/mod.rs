//! JavaScript's `Math` functions as V8 computes them. V8 calls LLVM libc's
//! correctly rounded double-precision functions for its transcendentals since
//! v8 commit 8787f0842a; this ports them from libc/src/__support/math in
//! llvm-project at 64f50ffd4f1c9e2d7cfde07251dadcb8573d4451, along the fused
//! multiply-add path with rounding to nearest. A correctly rounded result is
//! the one double nearest the true value, so the path taken does not change it.
//!
//! Part of the LLVM Project, under the Apache License v2.0 with LLVM Exceptions.
//! See https://llvm.org/LICENSE.txt for license information.
//! SPDX-License-Identifier: Apache-2.0 WITH LLVM-exception

mod asin;
mod asin_tables;
mod atan;
mod atan_tables;
mod dd;
mod dyadic;
mod trig;
mod trig_tables;

fn c(bits: u64) -> f64 {
  f64::from_bits(bits)
}

fn biased_exponent(x: f64) -> i32 {
  ((x.to_bits() >> 52) & 0x7ff) as i32
}

/// `Math.sin(x)`.
pub fn sin(x: f64) -> f64 {
  trig::sin(x)
}

/// `Math.cos(x)`.
pub fn cos(x: f64) -> f64 {
  trig::cos(x)
}

/// `Math.tan(x)`.
pub fn tan(x: f64) -> f64 {
  trig::tan(x)
}

/// `Math.atan(x)`.
pub fn atan(x: f64) -> f64 {
  atan::atan(x)
}

/// `Math.atan2(y, x)`.
pub fn atan2(y: f64, x: f64) -> f64 {
  atan::atan2(y, x)
}

/// `Math.asin(x)`.
pub fn asin(x: f64) -> f64 {
  asin::asin(x)
}

/// `Math.acos(x)`.
pub fn acos(x: f64) -> f64 {
  asin::acos(x)
}
