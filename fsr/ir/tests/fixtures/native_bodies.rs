// Written by the native generator from `written_bodies` in tests/prepared.rs. `FSR_WRITE_FIXTURES=1 cargo nextest run -p snapfire_fsr_ir --test prepared` rewrites it.

#[allow(unused_mut, unused_variables, unused_labels, unused_imports, unreachable_code, clippy::all)]
pub fn sorted_by_n(f: &mut snapfire_fsr_ir::native::Frame<'_>) -> Result<snapfire_fsr_ir::native::Value, snapfire_fsr_ir::native::Fail> {
use snapfire_fsr_ir::native::{Value as V_, Frame as F_, Arith as A_, Compare as C_, Builtin as B_, MAX_PASSES as P_, Read as R_};
let mut l1: V_ = { let mut a: Vec<V_> = Vec::with_capacity(4); { let v = { let mut m = F_::fields(2); { let v = V_::F64(3.0f64); m.insert("n", v); } { let v = V_::str("c"); m.insert("name", v); } V_::Map(m.into()) }; a.push(v); } { let v = { let mut m = F_::fields(2); { let v = V_::F64(1.0f64); m.insert("n", v); } { let v = V_::str("a"); m.insert("name", v); } V_::Map(m.into()) }; a.push(v); } { let v = { let mut m = F_::fields(2); { let v = V_::F64(3.0f64); m.insert("n", v); } { let v = V_::str("d"); m.insert("name", v); } V_::Map(m.into()) }; a.push(v); } { let v = { let mut m = F_::fields(2); { let v = V_::F64(2.0f64); m.insert("n", v); } { let v = V_::str("b"); m.insert("name", v); } V_::Map(m.into()) }; a.push(v); } V_::seq(a) };
return Ok({
let mut s2: Vec<V_> = F_::seq(f.read(&l1), "toSorted")?.into_items().into();
let mut w3: usize = 1;
while w3 < s2.len() {
let mut m4: Vec<V_> = Vec::with_capacity(s2.len());
let mut p5: usize = 0;
while p5 < s2.len() {
let h6 = (p5 + w3).min(s2.len());
let e7 = (p5 + 2 * w3).min(s2.len());
let (mut x8, mut y9) = (p5, h6);
while x8 < h6 && y9 < e7 {
if {
let mut l10: V_ = s2[x8].clone();
let mut l11: V_ = s2[y9].clone();
F_::order(&{ let a = f.field(&l10, "n"); let b = f.field(&l11, "n"); f.arith(A_::Sub, a, b)? }) } > 0.0 { m4.push(s2[y9].clone()); y9 += 1; } else { m4.push(s2[x8].clone()); x8 += 1; }
}
m4.extend_from_slice(&s2[x8..h6]);
m4.extend_from_slice(&s2[y9..e7]);
p5 = e7;
}
s2 = m4;
w3 *= 2;
}
V_::seq(s2) });
  Ok(V_::Null)
}

#[allow(unused_mut, unused_variables, unused_labels, unused_imports, unreachable_code, clippy::all)]
pub fn sorted_plainly(f: &mut snapfire_fsr_ir::native::Frame<'_>) -> Result<snapfire_fsr_ir::native::Value, snapfire_fsr_ir::native::Fail> {
use snapfire_fsr_ir::native::{Value as V_, Frame as F_, Arith as A_, Compare as C_, Builtin as B_, MAX_PASSES as P_, Read as R_};
let mut l1: V_ = { let mut a: Vec<V_> = Vec::with_capacity(5); { let v = V_::Int(10i128); a.push(v); } { let v = V_::Int(9i128); a.push(v); } { let v = V_::Int(1i128); a.push(v); } { let v = V_::str("b"); a.push(v); } { let v = V_::str("a"); a.push(v); } V_::seq(a) };
return Ok({
let mut s2: Vec<V_> = F_::seq(f.read(&l1), "toSorted")?.into_items().into();
let mut w3: usize = 1;
while w3 < s2.len() {
let mut m4: Vec<V_> = Vec::with_capacity(s2.len());
let mut p5: usize = 0;
while p5 < s2.len() {
let h6 = (p5 + w3).min(s2.len());
let e7 = (p5 + 2 * w3).min(s2.len());
let (mut x8, mut y9) = (p5, h6);
while x8 < h6 && y9 < e7 {
if F_::default_order(&s2[x8], &s2[y9])? > 0.0 { m4.push(s2[y9].clone()); y9 += 1; } else { m4.push(s2[x8].clone()); x8 += 1; }
}
m4.extend_from_slice(&s2[x8..h6]);
m4.extend_from_slice(&s2[y9..e7]);
p5 = e7;
}
s2 = m4;
w3 *= 2;
}
V_::seq(s2) });
  Ok(V_::Null)
}

#[allow(unused_mut, unused_variables, unused_labels, unused_imports, unreachable_code, clippy::all)]
pub fn sorted_in_place(f: &mut snapfire_fsr_ir::native::Frame<'_>) -> Result<snapfire_fsr_ir::native::Value, snapfire_fsr_ir::native::Fail> {
use snapfire_fsr_ir::native::{Value as V_, Frame as F_, Arith as A_, Compare as C_, Builtin as B_, MAX_PASSES as P_, Read as R_};
let mut l1: V_ = { let mut a: Vec<V_> = Vec::with_capacity(4); { let v = { let mut m = F_::fields(2); { let v = V_::F64(3.0f64); m.insert("n", v); } { let v = V_::str("c"); m.insert("name", v); } V_::Map(m.into()) }; a.push(v); } { let v = { let mut m = F_::fields(2); { let v = V_::F64(1.0f64); m.insert("n", v); } { let v = V_::str("a"); m.insert("name", v); } V_::Map(m.into()) }; a.push(v); } { let v = { let mut m = F_::fields(2); { let v = V_::F64(3.0f64); m.insert("n", v); } { let v = V_::str("d"); m.insert("name", v); } V_::Map(m.into()) }; a.push(v); } { let v = { let mut m = F_::fields(2); { let v = V_::F64(2.0f64); m.insert("n", v); } { let v = V_::str("b"); m.insert("name", v); } V_::Map(m.into()) }; a.push(v); } V_::seq(a) };
let _ = { let v = {
let mut s2: Vec<V_> = F_::seq(f.read(&l1), "toSorted")?.into_items().into();
let mut w3: usize = 1;
while w3 < s2.len() {
let mut m4: Vec<V_> = Vec::with_capacity(s2.len());
let mut p5: usize = 0;
while p5 < s2.len() {
let h6 = (p5 + w3).min(s2.len());
let e7 = (p5 + 2 * w3).min(s2.len());
let (mut x8, mut y9) = (p5, h6);
while x8 < h6 && y9 < e7 {
if {
let mut l10: V_ = s2[x8].clone();
let mut l11: V_ = s2[y9].clone();
F_::order(&{ let a = f.field(&l10, "n"); let b = f.field(&l11, "n"); f.arith(A_::Sub, a, b)? }) } > 0.0 { m4.push(s2[y9].clone()); y9 += 1; } else { m4.push(s2[x8].clone()); x8 += 1; }
}
m4.extend_from_slice(&s2[x8..h6]);
m4.extend_from_slice(&s2[y9..e7]);
p5 = e7;
}
s2 = m4;
w3 *= 2;
}
V_::seq(s2) }; f.write_whole(&l1, v)? };
return Ok(f.read(&l1));
  Ok(V_::Null)
}

#[allow(unused_mut, unused_variables, unused_labels, unused_imports, unreachable_code, clippy::all)]
pub fn object_from_a_block(f: &mut snapfire_fsr_ir::native::Frame<'_>) -> Result<snapfire_fsr_ir::native::Value, snapfire_fsr_ir::native::Fail> {
use snapfire_fsr_ir::native::{Value as V_, Frame as F_, Arith as A_, Compare as C_, Builtin as B_, MAX_PASSES as P_, Read as R_};
let mut l2: V_ = 'b1: {
break 'b1 ({ let mut m = F_::fields(2); { let v = V_::F64(1.0f64); m.insert("n", v); } { let v = V_::str("a"); m.insert("name", v); } V_::Map(m.into()) });
V_::Null };
return Ok(f.read(&l2));
  Ok(V_::Null)
}

#[allow(unused_mut, unused_variables, unused_labels, unused_imports, unreachable_code, clippy::all)]
pub fn local_from_a_block(f: &mut snapfire_fsr_ir::native::Frame<'_>) -> Result<snapfire_fsr_ir::native::Value, snapfire_fsr_ir::native::Fail> {
use snapfire_fsr_ir::native::{Value as V_, Frame as F_, Arith as A_, Compare as C_, Builtin as B_, MAX_PASSES as P_, Read as R_};
let mut l1: V_ = V_::Int(2i128);
let mut l3: V_ = 'b2: {
break 'b2 f.read(&l1);
V_::Null };
return Ok(f.read(&l3));
  Ok(V_::Null)
}

#[allow(unused_mut, unused_variables, unused_labels, unused_imports, unreachable_code, clippy::all)]
pub fn counted_numbers(f: &mut snapfire_fsr_ir::native::Frame<'_>) -> Result<snapfire_fsr_ir::native::Value, snapfire_fsr_ir::native::Fail> {
use snapfire_fsr_ir::native::{Value as V_, Frame as F_, Arith as A_, Compare as C_, Builtin as B_, MAX_PASSES as P_, Read as R_};
let mut l1: f64 = 10.0f64;
let mut l2: f64 = 0.0f64;
let mut l3: f64 = 0.0f64;
{
let s5: usize = F_::range_len(l1)?;
'l4: for i6 in 0..s5 {
let mut l7: f64 = i6 as f64;
let mut l8: f64 = F_::float(A_::Add, 2.0f64, F_::float(A_::Mul, l7, 3.0f64));
if F_::float(A_::Rem, l8, 4.0f64) == 2.0f64 {
continue 'l4;
} else {
}
l2 = F_::float(A_::Rem, F_::float(A_::Add, l2, l8), 7.0f64);
l3 = F_::float(A_::Add, l3, 1.0f64);
}
}
return Ok({ let mut a: Vec<V_> = Vec::with_capacity(9); { let v = V_::F64(l2); a.push(v); } { let v = V_::F64(l3); a.push(v); } { let v = V_::F64(F_::float(A_::Div, l2, 0.0f64)); a.push(v); } { let v = V_::F64(F_::float(A_::Rem, -7.0f64, 3.0f64)); a.push(v); } { let v = V_::Bool(f64::from_bits(0x7ff8000000000000) < 1.0f64); a.push(v); } { let v = V_::Bool(f64::from_bits(0x7ff8000000000000) != f64::from_bits(0x7ff8000000000000)); a.push(v); } { let v = V_::F64(F_::float(A_::Pow, 2.0f64, 10.0f64)); a.push(v); } { let v = { if l3 >= 3.0f64 { V_::F64(l3) } else { V_::F64(-1.0f64) } }; a.push(v); } { let v = V_::F64(F_::float(A_::Add, F_::as_number({ let args = vec![V_::F64(F_::float(A_::Div, l2, 2.0f64))]; f.builtin(B_::Floor, args)? })?, 1.0f64)); a.push(v); } V_::seq(a) });
  Ok(V_::Null)
}

#[allow(unused_mut, unused_variables, unused_labels, unused_imports, unreachable_code, clippy::all)]
pub fn counted_fraction(f: &mut snapfire_fsr_ir::native::Frame<'_>) -> Result<snapfire_fsr_ir::native::Value, snapfire_fsr_ir::native::Fail> {
use snapfire_fsr_ir::native::{Value as V_, Frame as F_, Arith as A_, Compare as C_, Builtin as B_, MAX_PASSES as P_, Read as R_};
let mut l1: V_ = { let mut a: Vec<V_> = Vec::with_capacity(0); V_::seq(a) };
{
let s3: usize = F_::range_len(2.5f64)?;
'l2: for i4 in 0..s3 {
let mut l5: f64 = i4 as f64;
let _ = { let v = V_::F64(F_::float(A_::Mul, l5, 2.0f64)); f.push_one(&l1, v)? };
}
}
return Ok(f.read(&l1));
  Ok(V_::Null)
}

#[allow(unused_mut, unused_variables, unused_labels, unused_imports, unreachable_code, clippy::all)]
pub fn numbers_beside_an_integer(f: &mut snapfire_fsr_ir::native::Frame<'_>) -> Result<snapfire_fsr_ir::native::Value, snapfire_fsr_ir::native::Fail> {
use snapfire_fsr_ir::native::{Value as V_, Frame as F_, Arith as A_, Compare as C_, Builtin as B_, MAX_PASSES as P_, Read as R_};
let mut l1: V_ = V_::Int(2i128);
let mut l2: f64 = 0.5f64;
let mut l3: V_ = { let a = f.read(&l1); let b = V_::F64(l2); f.arith(A_::Add, a, b)? };
let mut l4: f64 = F_::float(A_::Mul, l2, 4.0f64);
return Ok({ let mut a: Vec<V_> = Vec::with_capacity(2); { let v = f.read(&l3); a.push(v); } { let v = V_::F64(l4); a.push(v); } V_::seq(a) });
  Ok(V_::Null)
}

#[allow(unused_mut, unused_variables, unused_labels, unused_imports, unreachable_code, clippy::all)]
pub fn counted_while(f: &mut snapfire_fsr_ir::native::Frame<'_>) -> Result<snapfire_fsr_ir::native::Value, snapfire_fsr_ir::native::Fail> {
use snapfire_fsr_ir::native::{Value as V_, Frame as F_, Arith as A_, Compare as C_, Builtin as B_, MAX_PASSES as P_, Read as R_};
let mut l1: f64 = 0.0f64;
let mut l2: f64 = 0.0f64;
{
let mut n5: usize = 0;
'w3: loop {
if !(l1 < 5.0f64) { break; }
n5 += 1;
if n5 > P_ { return Err(F_::past_passes()); }
'p4: {
l2 = F_::float(A_::Add, l2, l1);
}
l1 = F_::float(A_::Add, l1, 1.0f64);
}
}
return Ok({ let mut a: Vec<V_> = Vec::with_capacity(2); { let v = V_::F64(l1); a.push(v); } { let v = V_::F64(l2); a.push(v); } V_::seq(a) });
  Ok(V_::Null)
}

