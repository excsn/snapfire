//! Rust source for a body, for a production build to compile into the application: each local a Rust variable, each loop a Rust loop, over the same values and the same runtime the interpreter uses. A body with anything this does not write gets no source and runs prepared instead, [`crate::compiled`].

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

use snapfire_fsr_core::{Fields, ValueSeq};

use crate::ast::{ArithOp, Body, CompareOp, Consts, Entry, Expr, Lit, LogicOp, Stmt};
use crate::interp::{Env, Name, Write};

pub use crate::ast::{ArithOp as Arith, Builtin, CompareOp as Compare};
pub use crate::interp::{Fail, Read, MAX_PASSES};
pub use snapfire_fsr_core::Value;

/// A body compiled into the application: the row it was written for, the plan it was written from as [`plan_hash`] and the function. The host attaches it to every body with that plan.
pub struct Native {
  pub source: &'static str,
  pub plan: u64,
  pub body: NativeBody,
}

/// What identifies a plan's body and the module functions it can call, so code written from one runs only for that one. It hashes the body and each constant the body reaches, directly or through another constant, as the plan file spells them on one line.
pub fn plan_hash(body: &Body, consts: Option<&Consts>) -> u64 {
  use crate::sexpr::{expr_to_sx, inline, stmt_to_sx};
  let mut hasher = xxhash_rust::xxh3::Xxh3::new();
  for stmt in body {
    hasher.update(inline(&stmt_to_sx(stmt)).as_bytes());
    hasher.update(b"\n");
  }
  if let Some(consts) = consts {
    let mut pending: Vec<String> = Vec::new();
    crate::ast::body_visit(body, &mut |expr| {
      if let Expr::Const(name) = expr {
        pending.push(name.clone());
      }
    });
    let mut reached = std::collections::BTreeSet::new();
    while let Some(name) = pending.pop() {
      if let Some(value) = consts.get(&name).filter(|_| !reached.contains(&name)) {
        value.visit(&mut |expr| {
          if let Expr::Const(name) = expr {
            pending.push(name.clone());
          }
        });
        reached.insert(name);
      }
    }
    hasher.update(b"\0consts");
    for name in reached {
      hasher.update(name.as_bytes());
      hasher.update(b"\0");
      hasher.update(inline(&expr_to_sx(&consts[&name])).as_bytes());
    }
  }
  hasher.digest()
}

/// What generated code runs against: the interpreter's environment for one run of a body.
pub struct Frame<'a> {
  env: &'a mut Env,
}

/// A body compiled into the application, called in place of the interpreter.
pub type NativeBody = fn(&mut Frame<'_>) -> Result<Value, Fail>;

impl<'a> Frame<'a> {
  pub(crate) fn new(env: &'a mut Env) -> Self {
    Frame { env }
  }

  /// A local's value as this body reads it: its own copy of a frozen object it wrote.
  #[inline]
  pub fn read(&self, value: &Value) -> Value {
    if self.env.thawed_is_empty() || !crate::interp::frozen(value) {
      return value.clone();
    }
    self.env.resolved_value(value.clone())
  }

  /// A native method the build read as `fn`, called as the interpreter calls one: an argument that is `null` or `undefined` is left out.
  pub fn native(&self, module: &str, method: &str, args: Vec<(&str, Value)>) -> Result<Value, Fail> {
    let mut map = snapfire_fsr_core::ValueMap::default();
    for (name, value) in args {
      if !matches!(value, Value::Null | Value::Undefined) {
        map.insert(name, value);
      }
    }
    match self.env.ctx.natives.call_sync(module, method, map) {
      Some(answer) => answer.map_err(|e| Fail::new(e.kind, e.message)),
      None => Err(Fail::internal(format!("`{module}.{method}` is not a synchronous native"))),
    }
  }

  /// `Number(value)`.
  pub fn number(value: Value) -> Result<Value, Fail> {
    crate::interp::number_of(value)
  }

  /// `BigInt(value)`.
  pub fn bigint(value: Value) -> Result<Value, Fail> {
    crate::interp::bigint_of(value)
  }

  /// `String(value)`.
  pub fn string(&mut self, value: &Value) -> Result<Value, Fail> {
    crate::interp::spelled(|out| self.env.text_into(value, out))
  }

  #[inline]
  pub fn request(&self, read: Read<'_>) -> Value {
    self.env.read_request(read)
  }

  /// The failure a guard of `kind` raises with `message`.
  pub fn guard(&self, kind: &str, message: &Value) -> Fail {
    let Some(kind) = crate::interp::parse_kind(kind) else { return Fail::internal(format!("`{kind}` is not a failure kind")) };
    match crate::interp::stringify(message) {
      Ok(message) => Fail::new(kind, message),
      Err(fail) => fail,
    }
  }

  pub fn constant(&mut self, key: &str) -> Result<Value, Fail> {
    let value = self.env.const_value(key)?;
    Ok(self.env.resolved_value(value))
  }

  #[inline]
  pub fn field(&self, target: &Value, name: &str) -> Value {
    match target {
      Value::Map(map) => map.get(name).map(|value| self.read(value)).unwrap_or(Value::Undefined),
      other => self.read(&crate::interp::field_of(other, name)),
    }
  }

  pub fn index(&self, target: &Value, key: &Value) -> Result<Value, Fail> {
    Ok(self.read(&crate::interp::index_of(target, key)?))
  }

  #[inline]
  pub fn arith(&mut self, op: ArithOp, l: Value, r: Value) -> Result<Value, Fail> {
    self.env.arith(op, l, r)
  }

  /// `a op b` on two numbers, as [`Self::arith`] answers it for two `F64`s.
  #[inline]
  pub fn float(op: ArithOp, a: f64, b: f64) -> f64 {
    crate::interp::float_inlined(op, a, b)
  }

  /// The number a value known to be an `F64` holds: what `length` and the rounding, `min` and `max` builtins answer.
  #[inline]
  pub fn as_number(value: Value) -> Result<f64, Fail> {
    match value {
      Value::F64(n) => Ok(n),
      other => Err(crate::interp::type_error("a number local", "a number", &other)),
    }
  }

  /// How many passes a `for...of` over `Range(n)` makes, refused as `Range` refuses it.
  #[inline]
  pub fn range_len(n: f64) -> Result<usize, Fail> {
    crate::interp::range_len(n)
  }

  /// [`Self::range_len`] of a count that is not known to be a number.
  pub fn range_len_of(value: Value) -> Result<usize, Fail> {
    crate::interp::range_len_of(&value)
  }

  #[inline]
  pub fn compare(&mut self, op: CompareOp, l: Value, r: Value) -> Result<Value, Fail> {
    self.env.compare(op, l, r).map(Value::Bool)
  }

  #[inline]
  pub fn truthy(value: &Value) -> bool {
    crate::interp::truthy(value)
  }

  /// One part of a template written onto `out`, converted as the template reaches it.
  pub fn text_part(&mut self, out: &mut String, value: &Value) -> Result<(), Fail> {
    self.env.text_into(value, out)
  }

  pub fn text(out: String) -> Value {
    Value::str(out.as_str())
  }

  #[inline]
  pub fn fields(capacity: usize) -> Fields {
    Fields::with_capacity(capacity)
  }

  /// `...value` into an object being built, the first spread taken whole with room for the `rest` entries after it.
  pub fn spread(fields: &mut Fields, value: Value, rest: usize) -> Result<(), Fail> {
    match value {
      Value::Map(inner) if fields.is_empty() => *fields = inner.into_fields_with_room(rest),
      Value::Map(inner) => {
        fields.reserve(inner.len());
        fields.extend(inner);
      }
      Value::Null | Value::Undefined => {}
      other => return Err(crate::interp::type_error("spread into an object", "an object", &other)),
    }
    Ok(())
  }

  pub fn computed(fields: &mut Fields, key: Value, value: Value) -> Result<(), Fail> {
    let key = match key {
      Value::Str(s) => s.to_string(),
      other => crate::interp::stringify(&other)?,
    };
    fields.insert(key, value);
    Ok(())
  }

  pub fn spread_items(items: &mut Vec<Value>, value: Value) -> Result<(), Fail> {
    match value {
      Value::Seq(inner) => items.extend(inner),
      Value::Null | Value::Undefined => {}
      Value::Str(text) => items.extend(text.chars().map(|c| Value::str(c.to_string()))),
      other => return Err(crate::interp::type_error("spread into an array", "an array", &other)),
    }
    Ok(())
  }

  pub fn seq(value: Value, what: &str) -> Result<ValueSeq, Fail> {
    match value {
      Value::Seq(items) => Ok(items),
      other => Err(crate::interp::type_error(what, "an array", &other)),
    }
  }

  /// Fields set in place on the object `target` holds, as `Overwrite` sets them.
  pub fn write_fields(&mut self, target: &Value, fields: Vec<(&str, Value)>) -> Result<Value, Fail> {
    let held = self.env.thawed_for_write(target.clone());
    let write = match <[_; 1]>::try_from(fields) {
      Ok([(key, value)]) => Write::Field(Name::new(key), value),
      Err(fields) => Write::Fields(fields.into_iter().map(|(k, v)| (Name::new(k), v)).collect()),
    };
    self.env.write_into(held, write)
  }

  /// One field set in place, as [`Self::write_fields`] sets several.
  pub fn write_field(&mut self, target: &Value, key: &str, value: Value) -> Result<Value, Fail> {
    let held = self.env.thawed_for_write(target.clone());
    self.env.write_into(held, Write::Field(Name::new(key), value))
  }

  /// One computed field set in place.
  pub fn write_key(&mut self, target: &Value, key: Value, value: Value) -> Result<Value, Fail> {
    let held = self.env.thawed_for_write(target.clone());
    let key = crate::interp::key_name(key)?;
    self.env.write_into(held, Write::Field(key, value))
  }

  /// One item pushed in place.
  pub fn push_one(&mut self, target: &Value, item: Value) -> Result<Value, Fail> {
    let held = self.env.thawed_for_write(target.clone());
    self.env.write_into(held, Write::Item(item))
  }

  pub fn write_computed(&mut self, target: &Value, fields: Vec<(Value, Value)>) -> Result<Value, Fail> {
    let held = self.env.thawed_for_write(target.clone());
    let mut out = Vec::with_capacity(fields.len());
    for (key, value) in fields {
      out.push((crate::interp::key_name(key)?, value));
    }
    let write = match <[_; 1]>::try_from(out) {
      Ok([(key, value)]) => Write::Field(key, value),
      Err(out) => Write::Fields(out),
    };
    self.env.write_into(held, write)
  }

  /// `target[key] = value` in place, as `Overwrite` with `SetAt` makes it.
  pub fn write_at(&mut self, target: &Value, key: Value, value: Value) -> Result<Value, Fail> {
    let held = self.env.thawed_for_write(target.clone());
    self.env.write_into(held, Write::At(key, value))
  }

  /// The whole of an object or an array replaced in place, as `Overwrite` makes it when the next value is no part it can set.
  pub fn write_whole(&mut self, target: &Value, next: Value) -> Result<Value, Fail> {
    let held = self.env.thawed_for_write(target.clone());
    self.env.write_into(held, Write::Whole(next))
  }

  /// What a comparator's answer says of the order, as `sort` reads it.
  pub fn order(answer: &Value) -> f64 {
    crate::interp::comparator_order(answer)
  }

  /// `sort`'s order without a comparator.
  pub fn default_order(a: &Value, b: &Value) -> Result<f64, Fail> {
    crate::interp::default_order(a, b)
  }

  pub fn push(&mut self, target: &Value, items: Vec<Value>) -> Result<Value, Fail> {
    let held = self.env.thawed_for_write(target.clone());
    let write = match <[_; 1]>::try_from(items) {
      Ok([item]) => Write::Item(item),
      Err(items) => Write::Items(items),
    };
    self.env.write_into(held, write)
  }

  pub fn heap_on(&mut self) {
    self.env.heap_on();
  }

  pub fn builtin(&mut self, name: Builtin, values: Vec<Value>) -> Result<Value, Fail> {
    self.env.builtin_values(&name, values)
  }

  /// A module function, by its constant, applied to `args` as the interpreter applies one.
  pub fn call(&mut self, key: &str, args: Vec<Value>) -> Result<Value, Fail> {
    self.env.set_backing();
    self.env.apply_sync(&Expr::Const(key.to_owned()), args)
  }

  pub fn length(value: Value) -> Result<Value, Fail> {
    crate::interp::length_of(value)
  }

  /// `Object.keys(value).length`.
  pub fn key_count(&self, value: Value) -> Result<Value, Fail> {
    match value {
      Value::Map(map) => Ok(Value::F64(map.len() as f64)),
      other => Err(crate::interp::type_error("Object.keys", "an object", &other)),
    }
  }

  pub fn keys(&self, value: Value) -> Result<Value, Fail> {
    self.env.keys_of(value)
  }

  pub fn values(value: Value) -> Result<Value, Fail> {
    crate::interp::values_of(value)
  }

  pub fn past_passes() -> Fail {
    Fail::internal(format!("a loop ran past {} passes", crate::interp::MAX_PASSES))
  }

  pub fn not_an_array(what: &str, value: &Value) -> Fail {
    crate::interp::type_error(what, "an array", value)
  }
}

/// Rust source for `body` as a function named `name` taking a [`Frame`], or `None` when it holds something this does not write. `consts` are the module's, to read the functions it calls.
pub fn emit(name: &str, body: &Body, consts: Option<&Consts>) -> Option<String> {
  emit_or_why(name, body, consts).ok()
}

/// [`emit`], or the first statement or expression it does not write.
pub fn emit_or_why(name: &str, body: &Body, consts: Option<&Consts>) -> Result<String, String> {
  emit_in("snapfire_fsr_ir::native", name, body, consts)
}

/// [`emit_or_why`] for code that reaches this module as `root`, a crate's re-export of it.
pub fn emit_in(root: &str, name: &str, body: &Body, consts: Option<&Consts>) -> Result<String, String> {
  if !crate::compiled::runs_through_body(body) {
    return Err("a call or a statement only a request's body holds".to_owned());
  }
  let mut writer = Gen { vars: Vec::new(), numbers: numbers(body), next: 0, consts, loops: Vec::new(), returns: Vec::new(), why: None };
  match writer.block(body) {
    Some(source) => {
      let mut out = String::new();
      let _ = writeln!(out, "#[allow(unused_mut, unused_variables, unused_labels, unused_imports, unreachable_code, clippy::all)]");
      let _ = writeln!(out, "pub fn {name}(f: &mut {root}::Frame<'_>) -> Result<{root}::Value, {root}::Fail> {{");
      let _ = writeln!(out, "use {root}::{{Value as V_, Frame as F_, Arith as A_, Compare as C_, Builtin as B_, MAX_PASSES as P_, Read as R_}};");
      out.push_str(&source);
      out.push_str("  Ok(V_::Null)\n}\n");
      Ok(out)
    }
    None => Err(writer.why.unwrap_or_default()),
  }
}

const V: &str = "V_";
const F: &str = "F_";

enum Loop {
  /// A `for...of`: `continue` goes on with the next item.
  For(String),
  /// A `while`: `continue` leaves the pass, `body`, and runs the step.
  While { outer: String, body: String },
}

struct Gen<'c> {
  /// Each local in scope: its name, its Rust identifier and whether it is an `f64`.
  vars: Vec<(String, String, bool)>,
  /// [`numbers`] of the body.
  numbers: HashSet<String>,
  next: usize,
  consts: Option<&'c Consts>,
  loops: Vec<Loop>,
  /// The labelled blocks a `return` inside a block expression leaves with its value; empty at the body's own level, whose `return` returns.
  returns: Vec<String>,
  /// The first statement or expression not written, innermost first.
  why: Option<String>,
}

impl Gen<'_> {
  fn fresh(&mut self, prefix: &str) -> String {
    self.next += 1;
    format!("{prefix}{}", self.next)
  }

  fn bind(&mut self, name: &str) -> String {
    let ident = self.fresh("l");
    self.vars.push((name.to_owned(), ident.clone(), self.numbers.contains(name)));
    ident
  }

  fn local(&self, name: &str) -> Option<&str> {
    self.vars.iter().rev().find(|(n, ..)| n == name).map(|(_, ident, _)| ident.as_str())
  }

  /// The identifier of a local that is an `f64`.
  fn number_local(&self, name: &str) -> Option<&str> {
    self.vars.iter().rev().find(|(n, ..)| n == name).filter(|(.., number)| *number).map(|(_, ident, _)| ident.as_str())
  }

  /// Whether `expr` is a number, written as an `f64` by [`Self::num`].
  fn is_num(&self, expr: &Expr) -> bool {
    match expr {
      Expr::Var(name) => self.number_local(name).is_some(),
      other => numeric(other, &self.numbers),
    }
  }

  /// `expr` as an `f64`, for an expression [`Self::is_num`] holds is a number.
  fn num(&mut self, expr: &Expr) -> Option<String> {
    Some(match expr {
      Expr::Var(name) => self.number_local(name)?.to_owned(),
      Expr::Lit(Lit::Float(n)) | Expr::Folded(Value::F64(n)) => float(*n),
      Expr::Arith(op, l, r) if float_op(*op) => {
        let (l, r) = (self.num(l)?, self.num(r)?);
        format!("{F}::float(A_::{op:?}, {l}, {r})")
      }
      Expr::Ternary(c, t, e) => {
        let (c, t, e) = (self.test(c)?, self.num(t)?, self.num(e)?);
        format!("if {c} {{ {t} }} else {{ {e} }}")
      }
      Expr::Length(_) | Expr::Builtin { .. } if numeric(expr, &self.numbers) => format!("{F}::as_number({})?", self.expr(expr)?),
      _ => return None,
    })
  }

  /// `expr` as a Rust `bool` read as JavaScript reads a condition: a comparison of two numbers compared in place.
  fn test(&mut self, expr: &Expr) -> Option<String> {
    match expr {
      Expr::Compare(op, l, r) if self.is_num(l) && self.is_num(r) => self.compared(*op, l, r),
      Expr::Not(e) => self.fails(e),
      other => Some(format!("{F}::truthy(&{})", self.expr(other)?)),
    }
  }

  /// [`Self::test`] negated, for a loop that stops when its condition does not hold.
  fn fails(&mut self, expr: &Expr) -> Option<String> {
    match expr {
      Expr::Compare(op, l, r) if self.is_num(l) && self.is_num(r) => Some(format!("!({})", self.compared(*op, l, r)?)),
      Expr::Not(e) => self.test(e),
      other => Some(format!("!{F}::truthy(&{})", self.expr(other)?)),
    }
  }

  fn compared(&mut self, op: CompareOp, l: &Expr, r: &Expr) -> Option<String> {
    let operand = |code: String| if code.starts_with("if ") { format!("({code})") } else { code };
    let (l, r) = (operand(self.num(l)?), operand(self.num(r)?));
    let op = match op {
      CompareOp::Eq => "==",
      CompareOp::Ne => "!=",
      CompareOp::Lt => "<",
      CompareOp::Le => "<=",
      CompareOp::Gt => ">",
      CompareOp::Ge => ">=",
    };
    Some(format!("{l} {op} {r}"))
  }

  fn block(&mut self, body: &Body) -> Option<String> {
    let depth = self.vars.len();
    let mut out = String::new();
    for stmt in body {
      out.push_str(&self.stmt(stmt)?);
    }
    self.vars.truncate(depth);
    Some(out)
  }

  fn stmt(&mut self, stmt: &Stmt) -> Option<String> {
    let written = self.stmt_inner(stmt);
    if written.is_none() && self.why.is_none() {
      self.why = Some(format!("{stmt:?}").chars().take(300).collect());
    }
    written
  }

  fn stmt_inner(&mut self, stmt: &Stmt) -> Option<String> {
    Some(match stmt {
      Stmt::Let { name, expr } if self.numbers.contains(name) => {
        let value = self.num(expr)?;
        let ident = self.bind(name);
        format!("let mut {ident}: f64 = {value};\n")
      }
      Stmt::Let { name, expr } => {
        let value = self.expr(expr)?;
        let ident = self.bind(name);
        format!("let mut {ident}: {V} = {value};\n")
      }
      Stmt::Set { name, expr } if self.number_local(name).is_some() => {
        let value = self.num(expr)?;
        let ident = self.local(name)?.to_owned();
        format!("{ident} = {value};\n")
      }
      Stmt::Set { name, expr } => {
        let value = self.expr(expr)?;
        let ident = self.local(name)?.to_owned();
        format!("{ident} = {value};\n")
      }
      Stmt::If { cond, then, r#else } => {
        let cond = self.test(cond)?;
        let then = self.block(then)?;
        let otherwise = self.block(r#else)?;
        format!("if {cond} {{\n{then}}} else {{\n{otherwise}}}\n")
      }
      Stmt::ForOf { name, over: Expr::Builtin { name: Builtin::Range, args }, body } if self.numbers.contains(name) && args.len() == 1 => {
        let count = match self.is_num(&args[0]) {
          true => format!("{F}::range_len({})?", self.num(&args[0])?),
          false => format!("{F}::range_len_of({})?", self.expr(&args[0])?),
        };
        let label = self.fresh("'l");
        let passes = self.fresh("s");
        let at = self.fresh("i");
        let depth = self.vars.len();
        let ident = self.bind(name);
        self.loops.push(Loop::For(label.clone()));
        let body = self.block(body);
        self.loops.pop();
        self.vars.truncate(depth);
        let body = body?;
        format!("{{\nlet {passes}: usize = {count};\n{label}: for {at} in 0..{passes} {{\nlet mut {ident}: f64 = {at} as f64;\n{body}}}\n}}\n")
      }
      Stmt::ForOf { name, over, body } => {
        let over = self.expr(over)?;
        let label = self.fresh("'l");
        let items = self.fresh("s");
        let at = self.fresh("i");
        let depth = self.vars.len();
        let ident = self.bind(name);
        self.loops.push(Loop::For(label.clone()));
        let body = self.block(body);
        self.loops.pop();
        self.vars.truncate(depth);
        let body = body?;
        format!("{{\nlet {items} = {F}::seq({over}, \"for...of\")?;\n{label}: for {at} in 0..{items}.len() {{\nlet mut {ident}: {V} = {items}[{at}].clone();\n{body}}}\n}}\n")
      }
      Stmt::While { cond, body, step } => {
        let outer = self.fresh("'w");
        let inner = self.fresh("'p");
        let passes = self.fresh("n");
        let stop = self.fails(cond)?;
        self.loops.push(Loop::While { outer: outer.clone(), body: inner.clone() });
        let body = self.block(body);
        self.loops.pop();
        let body = body?;
        let step = self.block(step)?;
        format!(
          "{{\nlet mut {passes}: usize = 0;\n{outer}: loop {{\nif {stop} {{ break; }}\n{passes} += 1;\nif {passes} > P_ {{ return Err({F}::past_passes()); }}\n{inner}: {{\n{body}}}\n{step}}}\n}}\n"
        )
      }
      Stmt::Return(expr) => {
        let value = self.expr(expr)?;
        match self.returns.last() {
          // A block value after a label reads as a labelled loop to rustc, which warns unless it is parenthesised.
          Some(label) if value.starts_with(['{', '\'']) => format!("break {label} ({value});\n"),
          Some(label) => format!("break {label} {value};\n"),
          None => format!("return Ok({value});\n"),
        }
      }
      Stmt::Break => match self.loops.last()? {
        Loop::For(label) | Loop::While { outer: label, .. } => format!("break {label};\n"),
      },
      Stmt::Continue => match self.loops.last()? {
        Loop::For(label) => format!("continue {label};\n"),
        Loop::While { body, .. } => format!("break {body};\n"),
      },
      Stmt::Expr(expr) => {
        let value = self.expr(expr)?;
        format!("let _ = {value};\n")
      }
      Stmt::Guard { cond, kind, message } if self.returns.is_empty() => {
        crate::interp::parse_kind(kind)?;
        let (cond, message) = (self.test(cond)?, self.expr(message)?);
        format!("if {cond} {{ let m = {message}; return Err(f.guard({kind:?}, &m)); }}\n")
      }
      _ => return None,
    })
  }

  fn expr(&mut self, expr: &Expr) -> Option<String> {
    let written = self.expr_inner(expr);
    if written.is_none() && self.why.is_none() {
      self.why = Some(format!("{expr:?}").chars().take(300).collect());
    }
    written
  }

  fn expr_inner(&mut self, expr: &Expr) -> Option<String> {
    Some(match expr {
      Expr::Var(name) if self.number_local(name).is_some() => format!("{V}::F64({})", self.number_local(name)?),
      Expr::Var(name) => format!("f.read(&{})", self.local(name)?),
      Expr::Arith(op, l, r) if float_op(*op) && self.is_num(l) && self.is_num(r) => format!("{V}::F64({})", self.num(expr)?),
      Expr::Compare(op, l, r) if self.is_num(l) && self.is_num(r) => format!("{V}::Bool({})", self.compared(*op, l, r)?),
      Expr::Lit(lit) => literal(lit)?,
      Expr::Folded(value) => folded(value)?,
      Expr::Const(key) => format!("f.constant({key:?})?"),
      Expr::NativeCall { module, method, args, sync: true } => {
        let mut out = String::from("{ let mut args = Vec::new(); ");
        for (name, e) in args {
          let value = self.expr(e)?;
          let _ = write!(out, "{{ let v = {value}; args.push(({name:?}, v)); }} ");
        }
        let _ = write!(out, "f.native({module:?}, {method:?}, args)? }}");
        out
      }
      Expr::Num(e) => format!("{F}::number({})?", self.expr(e)?),
      Expr::BigInt(e) => format!("{F}::bigint({})?", self.expr(e)?),
      Expr::Str(e) => format!("{{ let v = {}; f.string(&v)? }}", self.expr(e)?),
      Expr::Param(name) => format!("f.request(R_::Param({name:?}))"),
      Expr::Query(name) => format!("f.request(R_::Query({name:?}))"),
      Expr::Session(key) => format!("f.request(R_::Session({key:?}))"),
      Expr::Config(key) => format!("f.request(R_::Config({key:?}))"),
      Expr::ParamsAll => "f.request(R_::ParamsAll)".to_owned(),
      Expr::QueryAll => "f.request(R_::QueryAll)".to_owned(),
      Expr::Locale => "f.request(R_::Locale)".to_owned(),
      Expr::Locales => "f.request(R_::Locales)".to_owned(),
      Expr::Path => "f.request(R_::Path)".to_owned(),
      Expr::Document => "f.request(R_::Document)".to_owned(),
      Expr::Host => "f.request(R_::Host)".to_owned(),
      Expr::Origin => "f.request(R_::Origin)".to_owned(),
      Expr::Address => "f.request(R_::Address)".to_owned(),
      Expr::Input => "f.request(R_::Input)".to_owned(),
      Expr::Now => "f.request(R_::Now)".to_owned(),
      Expr::Field(target, name) => match &**target {
        Expr::Var(var) if self.local(var).is_some() && self.number_local(var).is_none() => format!("f.field(&{}, {name:?})", self.local(var)?),
        target => {
          let target = self.expr(target)?;
          format!("{{ let t = {target}; f.field(&t, {name:?}) }}")
        }
      },
      Expr::Index(target, key) => {
        let target = self.expr(target)?;
        let key = self.expr(key)?;
        format!("{{ let t = {target}; let k = {key}; f.index(&t, &k)? }}")
      }
      Expr::Arith(op, l, r) => {
        let (l, r) = (self.expr(l)?, self.expr(r)?);
        format!("{{ let a = {l}; let b = {r}; f.arith(A_::{op:?}, a, b)? }}")
      }
      Expr::Compare(op, l, r) => {
        let (l, r) = (self.expr(l)?, self.expr(r)?);
        format!("{{ let a = {l}; let b = {r}; f.compare(C_::{op:?}, a, b)? }}")
      }
      Expr::Logic(op, l, r) => {
        let (l, r) = (self.expr(l)?, self.expr(r)?);
        match op {
          LogicOp::And => format!("{{ let a = {l}; if !{F}::truthy(&a) {{ a }} else {{ {r} }} }}"),
          LogicOp::Or => format!("{{ let a = {l}; if {F}::truthy(&a) {{ a }} else {{ {r} }} }}"),
        }
      }
      Expr::Not(e) => format!("{V}::Bool(!{F}::truthy(&{}))", self.expr(e)?),
      Expr::Coalesce(l, r) => {
        let (l, r) = (self.expr(l)?, self.expr(r)?);
        format!("{{ match {l} {{ {V}::Null | {V}::Undefined => {r}, other => other }} }}")
      }
      Expr::Ternary(c, t, e) => {
        let (c, t, e) = (self.test(c)?, self.expr(t)?, self.expr(e)?);
        format!("{{ if {c} {{ {t} }} else {{ {e} }} }}")
      }
      Expr::Template(parts) => {
        let parts = parts.iter().map(|p| self.expr(p)).collect::<Option<Vec<_>>>()?;
        let mut out = String::from("{ let mut s = String::new(); ");
        for part in parts {
          let _ = write!(out, "{{ let v = {part}; f.text_part(&mut s, &v)?; }} ");
        }
        out.push_str(&format!("{F}::text(s) }}"));
        out
      }
      Expr::Object(entries) => {
        let mut out = format!("{{ let mut m = {F}::fields({}); ", if matches!(entries.first(), Some(Entry::Spread(_))) { 0 } else { entries.len() });
        for (at, entry) in entries.iter().enumerate() {
          match entry {
            Entry::Field(name, e) => {
              let value = self.expr(e)?;
              let _ = write!(out, "{{ let v = {value}; m.insert({name:?}, v); }} ");
            }
            Entry::Computed(k, e) => {
              let (k, value) = (self.expr(k)?, self.expr(e)?);
              let _ = write!(out, "{{ let k = {k}; let v = {value}; {F}::computed(&mut m, k, v)?; }} ");
            }
            Entry::Spread(e) => {
              let value = self.expr(e)?;
              let _ = write!(out, "{{ let v = {value}; {F}::spread(&mut m, v, {})?; }} ", entries.len() - at - 1);
            }
            Entry::Item(_) | Entry::Baked(_) => return None,
          }
        }
        out.push_str(&format!("{V}::Map(m.into()) }}"));
        out
      }
      Expr::Array(entries) => {
        let mut out = format!("{{ let mut a: Vec<{V}> = Vec::with_capacity({}); ", entries.len());
        for entry in entries {
          match entry {
            Entry::Item(e) => {
              let value = self.expr(e)?;
              let _ = write!(out, "{{ let v = {value}; a.push(v); }} ");
            }
            Entry::Spread(e) => {
              let value = self.expr(e)?;
              let _ = write!(out, "{{ let v = {value}; {F}::spread_items(&mut a, v)?; }} ");
            }
            _ => return None,
          }
        }
        out.push_str(&format!("{V}::seq(a) }}"));
        out
      }
      Expr::Block(body) => {
        let label = self.fresh("'b");
        self.returns.push(label.clone());
        let body = self.block(body);
        self.returns.pop();
        format!("{label}: {{\n{}{V}::Null }}", body?)
      }
      // The count of an object's keys, without the array of them `Object.keys` makes.
      Expr::Length(e) if matches!(&**e, Expr::Keys(_)) => {
        let Expr::Keys(inner) = &**e else { unreachable!("matched") };
        format!("{{ let v = {}; f.key_count(v)? }}", self.expr(inner)?)
      }
      Expr::Length(e) => format!("{F}::length({})?", self.expr(e)?),
      Expr::Keys(e) => format!("{{ let v = {}; f.keys(v)? }}", self.expr(e)?),
      Expr::Values(e) => format!("{F}::values({})?", self.expr(e)?),
      Expr::Map(over, lambda) => self.each("map", over, lambda, Each::Map)?,
      Expr::Sort(over, comparator) => self.sort(over, comparator)?,
      Expr::Filter(over, lambda) => self.each("filter", over, lambda, Each::Filter)?,
      Expr::Find(over, lambda) => self.each("find", over, lambda, Each::Find)?,
      Expr::FindIndex(over, lambda) => self.each("findIndex", over, lambda, Each::FindIndex)?,
      Expr::Some(over, lambda) => self.each("some", over, lambda, Each::Some)?,
      Expr::Every(over, lambda) => self.each("every", over, lambda, Each::Every)?,
      Expr::Reduce(over, init, lambda) => {
        let Expr::Lambda { params, body } = &**lambda else { return None };
        if params.len() > 3 || params.iter().any(|p| p.starts_with("...")) || writes_back(body) {
          return None;
        }
        let over = self.expr(over)?;
        let init = self.expr(init)?;
        let (items, at, acc) = (self.fresh("s"), self.fresh("i"), self.fresh("acc"));
        let depth = self.vars.len();
        let mut binds = String::new();
        for (n, param) in params.iter().enumerate() {
          let ident = self.bind(param);
          let value = match n {
            0 => format!("std::mem::replace(&mut {acc}, {V}::Undefined)"),
            1 => format!("{items}[{at}].clone()"),
            _ => format!("{V}::F64({at} as f64)"),
          };
          let _ = writeln!(binds, "let mut {ident}: {V} = {value};");
        }
        let body = self.expr(body);
        self.vars.truncate(depth);
        format!("{{\nlet {items} = {F}::seq({over}, \"reduce\")?;\nlet mut {acc}: {V} = {init};\nfor {at} in 0..{items}.len() {{\n{binds}{acc} = {};\n}}\n{acc} }}", body?)
      }
      Expr::Apply { f, args } => {
        let values = args.iter().map(|a| self.expr(a)).collect::<Option<Vec<_>>>()?;
        match &**f {
          Expr::Lambda { params, body } if !params.iter().any(|p| p.starts_with("...")) && !writes_back(body) => {
            let depth = self.vars.len();
            let mut binds = String::new();
            let mut given = values.into_iter();
            for param in params {
              let value = given.next().unwrap_or_else(|| format!("{V}::Undefined"));
              let ident = self.bind(param);
              let _ = writeln!(binds, "let mut {ident}: {V} = {value};");
            }
            let rest: Vec<String> = given.collect();
            let body = self.expr(body);
            self.vars.truncate(depth);
            let evaluated: String = rest.iter().map(|v| format!("let _ = {v};\n")).collect();
            format!("{{\n{binds}{evaluated}{} }}", body?)
          }
          Expr::Const(key) if self.calls_plainly(key) => format!("{{ let args = vec![{}]; f.call({key:?}, args)? }}", values.join(", ")),
          _ => return None,
        }
      }
      Expr::Builtin { name: Builtin::HeapOn, .. } => format!("{{ f.heap_on(); {V}::Null }}"),
      Expr::Builtin { name: Builtin::Overwrite, args } if args.len() == 2 => self.overwrite(&args[0], &args[1])?,
      Expr::Builtin { name, args } if builtin_on_values(*name) => {
        let values = args.iter().map(|a| self.expr(a)).collect::<Option<Vec<_>>>()?;
        format!("{{ let args = vec![{}]; f.builtin(B_::{name:?}, args)? }}", values.join(", "))
      }
      _ => return None,
    })
  }

  /// Whether the module function `key` is one a call reaches by value alone, writing back nothing of what it is passed.
  fn calls_plainly(&self, key: &str) -> bool {
    match self.consts.and_then(|consts| consts.get(key)) {
      Some(Expr::Lambda { body, .. }) => !writes_back(body),
      Some(Expr::Fix { f, .. }) => matches!(&**f, Expr::Lambda { body, .. } if !writes_back(body)),
      _ => false,
    }
  }

  fn each(&mut self, what: &str, over: &Expr, lambda: &Expr, each: Each) -> Option<String> {
    let Expr::Lambda { params, body } = lambda else { return None };
    if params.len() > 2 || params.iter().any(|p| p.starts_with("...")) || writes_back(body) {
      return None;
    }
    let over = self.expr(over)?;
    let (items, at, out) = (self.fresh("s"), self.fresh("i"), self.fresh("o"));
    let depth = self.vars.len();
    let mut binds = String::new();
    for (n, param) in params.iter().enumerate() {
      let ident = self.bind(param);
      let value = if n == 0 { format!("{items}[{at}].clone()") } else { format!("{V}::F64({at} as f64)") };
      let _ = writeln!(binds, "let mut {ident}: {V} = {value};");
    }
    let body = self.expr(body);
    self.vars.truncate(depth);
    let body = body?;
    Some(match each {
      Each::Map => format!("{{\nlet {items} = {F}::seq({over}, {what:?})?;\nlet mut {out}: Vec<{V}> = Vec::with_capacity({items}.len());\nfor {at} in 0..{items}.len() {{\n{binds}let v = {body};\n{out}.push(v);\n}}\n{V}::seq({out}) }}"),
      Each::Find => format!("{{\nlet {items} = {F}::seq({over}, {what:?})?;\nlet mut {out}: {V} = {V}::Undefined;\nfor {at} in 0..{items}.len() {{\n{binds}if {F}::truthy(&{body}) {{ {out} = {items}[{at}].clone(); break; }}\n}}\n{out} }}"),
      Each::FindIndex => format!("{{\nlet {items} = {F}::seq({over}, {what:?})?;\nlet mut {out}: {V} = {V}::F64(-1.0);\nfor {at} in 0..{items}.len() {{\n{binds}if {F}::truthy(&{body}) {{ {out} = {V}::F64({at} as f64); break; }}\n}}\n{out} }}"),
      Each::Some => format!("{{\nlet {items} = {F}::seq({over}, {what:?})?;\nlet mut {out} = false;\nfor {at} in 0..{items}.len() {{\n{binds}if {F}::truthy(&{body}) {{ {out} = true; break; }}\n}}\n{V}::Bool({out}) }}"),
      Each::Every => format!("{{\nlet {items} = {F}::seq({over}, {what:?})?;\nlet mut {out} = true;\nfor {at} in 0..{items}.len() {{\n{binds}if !{F}::truthy(&{body}) {{ {out} = false; break; }}\n}}\n{V}::Bool({out}) }}"),
      Each::Filter => format!("{{\nlet {items} = {F}::seq({over}, {what:?})?;\nlet mut {out}: Vec<{V}> = Vec::new();\nfor {at} in 0..{items}.len() {{\n{binds}if {F}::truthy(&{body}) {{ {out}.push({items}[{at}].clone()); }}\n}}\n{V}::seq({out}) }}"),
    })
  }

  /// `toSorted`: the interpreter's stable bottom-up merge, the comparator written where two items are compared and its answer read by the interpreter's rule.
  fn sort(&mut self, over: &Expr, comparator: &Expr) -> Option<String> {
    let over = self.expr(over)?;
    let (items, width, merged, start, mid, end, x, y) = (self.fresh("s"), self.fresh("w"), self.fresh("m"), self.fresh("p"), self.fresh("h"), self.fresh("e"), self.fresh("x"), self.fresh("y"));
    let order = match comparator {
      Expr::Lit(Lit::Null) => format!("{F}::default_order(&{items}[{x}], &{items}[{y}])?"),
      Expr::Lambda { params, body } => {
        if params.len() > 2 || params.iter().any(|p| p.starts_with("...")) || writes_back(body) {
          return None;
        }
        let depth = self.vars.len();
        let mut binds = String::new();
        for (n, param) in params.iter().enumerate() {
          let ident = self.bind(param);
          let at = if n == 0 { &x } else { &y };
          let _ = writeln!(binds, "let mut {ident}: {V} = {items}[{at}].clone();");
        }
        let body = self.expr(body);
        self.vars.truncate(depth);
        format!("{{\n{binds}{F}::order(&{}) }}", body?)
      }
      _ => return None,
    };
    Some(format!(
      "{{\nlet mut {items}: Vec<{V}> = {F}::seq({over}, \"toSorted\")?.into_items().into();\nlet mut {width}: usize = 1;\nwhile {width} < {items}.len() {{\nlet mut {merged}: Vec<{V}> = Vec::with_capacity({items}.len());\nlet mut {start}: usize = 0;\nwhile {start} < {items}.len() {{\nlet {mid} = ({start} + {width}).min({items}.len());\nlet {end} = ({start} + 2 * {width}).min({items}.len());\nlet (mut {x}, mut {y}) = ({start}, {mid});\nwhile {x} < {mid} && {y} < {end} {{\nif {order} > 0.0 {{ {merged}.push({items}[{y}].clone()); {y} += 1; }} else {{ {merged}.push({items}[{x}].clone()); {x} += 1; }}\n}}\n{merged}.extend_from_slice(&{items}[{x}..{mid}]);\n{merged}.extend_from_slice(&{items}[{y}..{end}]);\n{start} = {end};\n}}\n{items} = {merged};\n{width} *= 2;\n}}\n{V}::seq({items}) }}"
    ))
  }

  /// `Overwrite` of a local: fields set on an object spread first, items pushed on an array spread first or an item set, each in place as the interpreter makes it, and otherwise the whole of the object or array replaced in place.
  fn overwrite(&mut self, target: &Expr, next: &Expr) -> Option<String> {
    let Expr::Var(name) = target else { return None };
    if self.number_local(name).is_some() {
      return None;
    }
    let ident = self.local(name)?.to_owned();
    if let Some(part) = self.overwrite_part(&ident, target, next) {
      return Some(part);
    }
    let next = self.expr(next)?;
    Some(format!("{{ let v = {next}; f.write_whole(&{ident}, v)? }}"))
  }

  fn overwrite_part(&mut self, ident: &str, target: &Expr, next: &Expr) -> Option<String> {
    match next {
      Expr::Object(entries) if matches!(entries.first(), Some(Entry::Spread(first)) if first == target) => {
        let rest = &entries[1..];
        if rest.iter().all(|e| matches!(e, Entry::Field(..))) {
          let fields = rest
            .iter()
            .map(|e| match e {
              Entry::Field(key, value) => self.expr(value).map(|v| format!("({key:?}, {v})")),
              _ => None,
            })
            .collect::<Option<Vec<_>>>()?;
          if let [only] = &rest[..] {
            let Entry::Field(key, value) = only else { unreachable!("checked") };
            let value = self.expr(value)?;
            return Some(format!("{{ let v = {value}; f.write_field(&{ident}, {key:?}, v)? }}"));
          }
          return Some(format!("{{ let fs = vec![{}]; f.write_fields(&{ident}, fs)? }}", fields.join(", ")));
        }
        if rest.iter().all(|e| matches!(e, Entry::Computed(..))) {
          let fields = rest
            .iter()
            .map(|e| match e {
              Entry::Computed(key, value) => Some(format!("({}, {})", self.expr(key)?, self.expr(value)?)),
              _ => None,
            })
            .collect::<Option<Vec<_>>>()?;
          if let [only] = &rest[..] {
            let Entry::Computed(key, value) = only else { unreachable!("checked") };
            let (key, value) = (self.expr(key)?, self.expr(value)?);
            return Some(format!("{{ let k = {key}; let v = {value}; f.write_key(&{ident}, k, v)? }}"));
          }
          return Some(format!("{{ let fs = vec![{}]; f.write_computed(&{ident}, fs)? }}", fields.join(", ")));
        }
        None
      }
      Expr::Builtin { name: Builtin::SetAt, args } if args.len() == 3 && args[0] == *target => {
        let (key, value) = (self.expr(&args[1])?, self.expr(&args[2])?);
        Some(format!("{{ let k = {key}; let v = {value}; f.write_at(&{ident}, k, v)? }}"))
      }
      Expr::Array(entries) if matches!(entries.first(), Some(Entry::Spread(first)) if first == target) && entries[1..].iter().all(|e| matches!(e, Entry::Item(_))) => {
        let items = entries[1..]
          .iter()
          .map(|e| match e {
            Entry::Item(value) => self.expr(value),
            _ => None,
          })
          .collect::<Option<Vec<_>>>()?;
        if let [only] = &items[..] {
          return Some(format!("{{ let v = {only}; f.push_one(&{ident}, v)? }}"));
        }
        Some(format!("{{ let items = vec![{}]; f.push(&{ident}, items)? }}", items.join(", ")))
      }
      _ => None,
    }
  }
}

enum Each {
  Map,
  Filter,
  Find,
  FindIndex,
  Some,
  Every,
}

/// The locals the body keeps as `f64`: the item of a `for...of` over `Range` and each name every binding and write of which is a number by [`numeric`]. A name also bound as a parameter, by another `for...of` or as the target of an in-place write is not one.
fn numbers(body: &Body) -> HashSet<String> {
  #[derive(Default)]
  struct Found {
    written: HashMap<String, Vec<Expr>>,
    counted: HashSet<String>,
    other: HashSet<String>,
  }
  impl Found {
    fn stmts(&mut self, body: &Body) {
      for stmt in body {
        match stmt {
          Stmt::Let { name, expr } | Stmt::Set { name, expr } => self.written.entry(name.clone()).or_default().push(expr.clone()),
          Stmt::ForOf { name, over: Expr::Builtin { name: Builtin::Range, args }, body } if args.len() == 1 => {
            self.counted.insert(name.clone());
            self.stmts(body);
          }
          Stmt::ForOf { name, body, .. } => {
            self.other.insert(name.clone());
            self.stmts(body);
          }
          Stmt::If { then, r#else, .. } => {
            self.stmts(then);
            self.stmts(r#else);
          }
          Stmt::While { body, step, .. } => {
            self.stmts(body);
            self.stmts(step);
          }
          _ => {}
        }
      }
    }

    fn exprs(&mut self, body: &Body) {
      crate::ast::body_visit(body, &mut |e| match e {
        Expr::Lambda { params, .. } => self.other.extend(params.iter().map(|p| p.trim_start_matches("...").to_owned())),
        Expr::Fix { name, .. } => {
          self.other.insert(name.clone());
        }
        Expr::Builtin { name: Builtin::Overwrite | Builtin::WriteBack, args } => {
          if let Some(Expr::Var(target)) = args.first() {
            self.other.insert(target.clone());
          }
        }
        Expr::Block(inner) => self.stmts(inner),
        _ => {}
      });
    }
  }
  let mut found = Found::default();
  found.stmts(body);
  found.exprs(body);
  let mut numbers: HashSet<String> = found.written.keys().chain(&found.counted).filter(|name| !found.other.contains(*name)).cloned().collect();
  loop {
    let now = numbers.clone();
    numbers.retain(|name| found.written.get(name).is_none_or(|writes| writes.iter().all(|e| numeric(e, &now))));
    if numbers.len() == now.len() {
      return numbers;
    }
  }
}

/// Whether `expr` is a number when every name in `numbers` is: what the interpreter answers as an `F64` whatever it is given.
fn numeric(expr: &Expr, numbers: &HashSet<String>) -> bool {
  match expr {
    Expr::Var(name) => numbers.contains(name),
    Expr::Lit(Lit::Float(_)) | Expr::Folded(Value::F64(_)) => true,
    Expr::Arith(op, l, r) => float_op(*op) && numeric(l, numbers) && numeric(r, numbers),
    Expr::Ternary(_, t, e) => numeric(t, numbers) && numeric(e, numbers),
    Expr::Length(e) => !matches!(&**e, Expr::Keys(_)),
    Expr::Builtin { name, .. } => matches!(name, Builtin::Round | Builtin::Floor | Builtin::Ceil | Builtin::Abs | Builtin::Min | Builtin::Max),
    _ => false,
  }
}

/// Whether `op` on two `F64`s answers an `F64`: every operator but the bitwise ones, which answer an integer.
fn float_op(op: ArithOp) -> bool {
  !matches!(op, ArithOp::BitAnd | ArithOp::BitOr | ArithOp::BitXor | ArithOp::Shl | ArithOp::Shr | ArithOp::UShr)
}

/// Whether `body` hands back what it wrote of its parameters, which a call made from native code has no place to write back to.
fn writes_back(body: &Expr) -> bool {
  let mut found = false;
  body.visit(&mut |e| found |= matches!(e, Expr::Builtin { name: Builtin::WriteBack, .. }));
  found
}

/// Whether a builtin runs on its arguments' values alone, as [`Env::builtin_values`] applies it.
fn builtin_on_values(name: Builtin) -> bool {
  !matches!(name, Builtin::HeapOn | Builtin::Overwrite | Builtin::WriteBack | Builtin::Json | Builtin::CloneElement | Builtin::CallMethod | Builtin::GetMember | Builtin::Emit)
}

fn literal(lit: &Lit) -> Option<String> {
  Some(match lit {
    Lit::Null => format!("{V}::Null"),
    Lit::Undefined => format!("{V}::Undefined"),
    Lit::Bool(b) => format!("{V}::Bool({b})"),
    Lit::Int(n) => format!("{V}::Int({n}i128)"),
    Lit::Float(f) => format!("{V}::F64({})", float(*f)),
    Lit::Str(s) => format!("{V}::str({s:?})"),
  })
}

fn float(f: f64) -> String {
  // `f64::NAN` compared in place trips rustc's `invalid_nan_comparisons` lint.
  if f.is_nan() {
    format!("f64::from_bits({:#x})", f64::NAN.to_bits())
  } else if f == f64::INFINITY {
    "f64::INFINITY".to_owned()
  } else if f == f64::NEG_INFINITY {
    "f64::NEG_INFINITY".to_owned()
  } else {
    format!("{f:?}f64")
  }
}

fn folded(value: &Value) -> Option<String> {
  Some(match value {
    Value::Null => format!("{V}::Null"),
    Value::Undefined => format!("{V}::Undefined"),
    Value::Bool(b) => format!("{V}::Bool({b})"),
    Value::Int(n) => format!("{V}::Int({n}i128)"),
    Value::F64(f) => format!("{V}::F64({})", float(*f)),
    Value::Str(s) => format!("{V}::str({:?})", s.as_str()),
    _ => return None,
  })
}
