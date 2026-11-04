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

