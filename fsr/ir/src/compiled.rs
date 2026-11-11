//! A body prepared once to run many times: each local read at the position it is bound to in the scope, each node a variant to match rather than an expression to look through, and whatever has no form here run by the interpreter as written.
//!
//! The scope stays the interpreter's, a list of names and values, so a node handed back to the interpreter finds every local by name. A local is pushed where the interpreter pushes it and its position from the body's first is known before the body runs: a `let` adds one, a block and a loop drop what they added, a callback written in place adds its parameters and drops them.

use std::sync::Arc;

use snapfire_fsr_core::{Fields, Value, ValueMap};

use crate::ast::{ArithOp, Body, CompareOp, Entry, Expr, Lit, LogicOp, Stmt};
use crate::interp::{Env, Fail, Flow, Name, Write};

/// A body ready to run, built by [`Program::of`].
pub struct Program {
  steps: Vec<Step>,
}

/// A function's body ready to run in the scope a function value makes: what it captured, in the order its free names come, then its own name, then its parameters.
pub(crate) struct Function {
  pub(crate) params: Vec<String>,
  pub(crate) name: Option<String>,
  pub(crate) captures: Vec<String>,
  /// The body as written, for the interpreter when a capture is missing where the value is made.
  pub(crate) body: Arc<Expr>,
  code: Code,
}

impl std::fmt::Debug for Function {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("Function").field("params", &self.params).field("name", &self.name).finish_non_exhaustive()
  }
}

impl Function {
  /// The module's functions prepared, by the constant each is: a `Lambda`, or a `Fix` over one for a function that calls itself.
  pub(crate) fn of_consts(consts: &crate::ast::Consts) -> std::collections::HashMap<String, Arc<Function>> {
    consts
      .iter()
      .filter_map(|(key, expr)| {
        let f = match expr {
          Expr::Lambda { params, body } => Function::of(params, body, None)?,
          Expr::Fix { name, f } => match &**f {
            Expr::Lambda { params, body } => Function::of(params, body, Some(name.clone()))?,
            _ => return None,
          },
          _ => return None,
        };
        Some((key.clone(), Arc::new(f)))
      })
      .collect()
  }

  /// `body` under `params` prepared as a function value runs it, `name` the one it calls itself by. `None` for a body that waits on a call.
  pub(crate) fn of(params: &[String], body: &Expr, name: Option<String>) -> Option<Function> {
    if body.has_call() || matches!(body, Expr::Block(stmts) if !statements_through(stmts, false)) {
      return None;
    }
    let mut free = Vec::new();
    body.free_vars(&mut free);
    let captures: Vec<String> = free.into_iter().filter(|name| !params.contains(name)).collect();
    let mut bound = Bound(captures.clone());
    bound.0.extend(name.iter().cloned());
    bound.0.extend(params.iter().cloned());
    let code = code(body, &mut bound);
    Some(Function { params: params.to_vec(), name, captures, body: Arc::new(body.clone()), code })
  }

  /// Runs the body in `env`, whose scope holds the function's frame from its first entry.
  pub(crate) fn run(&self, env: &mut Env) -> Result<Value, Fail> {
    eval(&self.code, env, 0)
  }
}

enum Step {
  Let(Name, Code),
  Set(usize, Code),
  If(Code, Vec<Step>, Vec<Step>),
  ForOf { name: Name, over: Code, body: Vec<Step> },
  While { cond: Code, body: Vec<Step>, step: Vec<Step> },
  Return(Code),
  Break,
  Continue,
  Expr(Code),
  /// A guard at the body's own level: the failure when `cond` holds, its message read only then.
  Guard { cond: Code, kind: snapfire_fsr_runtime::FailureKind, message: Code },
  /// A statement the interpreter runs, as a body of one; it leaves the scope as long as it found it.
  Interpreted(Body),
}

enum Code {
  Value(Value),
  /// The local at this position from the body's first.
  Local(usize),
  /// A module constant, as the interpreter reads one.
  Const(String),
  Field(Box<Code>, Name),
  Index(Box<Code>, Box<Code>),
  Arith(ArithOp, Box<Code>, Box<Code>),
  Compare(CompareOp, Box<Code>, Box<Code>),
  Logic(LogicOp, Box<Code>, Box<Code>),
  Not(Box<Code>),
  Coalesce(Box<Code>, Box<Code>),
  Ternary(Box<Code>, Box<Code>, Box<Code>),
  Template(Vec<Code>),
  Object(Vec<Part>),
  Array(Vec<Part>),
  Block(Vec<Step>),
  /// A function value made here.
  Lambda(Arc<Function>),
  /// A lambda written in place and applied where it stands, its parameters bound above the caller's locals.
  CallInPlace { params: Vec<String>, body: Box<Code>, args: Vec<Code>, written: Vec<Expr> },
  /// A function value applied: the function as written and its arguments prepared.
  Call { f: Expr, args: Vec<Code>, written: Vec<Expr> },
  Map(Box<Code>, Callback),
  Filter(Box<Code>, Callback),
  Reduce(Box<Code>, Box<Code>, Callback),
  /// `Overwrite` of a local: the write's parts and the local they go to.
  Write { target: Box<Code>, write: WriteCode },
  /// An expression the interpreter evaluates.
  Interpreted(Expr),
}

enum Part {
  Field(Name, Code),
  Computed(Code, Code),
  Spread(Code),
  Item(Code),
}

enum WriteCode {
  At(Box<Code>, Box<Code>),
  Fields(Vec<(Name, Code)>),
  ComputedFields(Vec<(Code, Code)>),
  Items(Vec<Code>),
}

/// A callback written in place, its parameters bound where the call stands.
struct Callback {
  params: Vec<Name>,
  body: Box<Code>,
}

/// The names bound at each point while a body is prepared, in the order the interpreter binds them.
struct Bound(Vec<String>);

impl Bound {
  fn at(&self, name: &str) -> Option<usize> {
    self.0.iter().rposition(|bound| bound == name)
  }
}

impl Program {
  /// `body` prepared to run where the scope holds nothing the body binds, as a request's body starts. `None` for a body that waits on a call or holds a statement only a request's own body may, which the interpreter runs.
  pub fn of(body: &Body) -> Option<Program> {
    if !runs_through(body) {
      return None;
    }
    let mut bound = Bound(Vec::new());
    Some(Program { steps: steps(body, &mut bound, true) })
  }

  /// Runs the program in `env` as the interpreter runs the body.
  pub(crate) fn run(&self, env: &mut Env) -> Result<Flow, Fail> {
    let base = env.scope.len();
    let flow = run_steps(&self.steps, env, base);
    env.scope.truncate(base);
    flow
  }
}

pub(crate) fn runs_through_body(body: &Body) -> bool {
  runs_through(body)
}

/// Whether `body` runs to its end without waiting: no service call, `next` or async native call anywhere in it and no session write, extension or dispatch. A guard is one only at the body's own level and in its `If`, `ForOf` and `While`, since a `Try` and a block expression run interpreted where no guard may.
fn runs_through(body: &Body) -> bool {
  let mut waits = false;
  crate::ast::body_visit(body, &mut |e| waits |= matches!(e, Expr::Call { .. } | Expr::Next(_) | Expr::ApplyOf(..) | Expr::NativeCall { sync: false, .. }));
  !waits && statements_through(body, true)
}

fn statements_through(body: &Body, guards: bool) -> bool {
  body.iter().all(|stmt| match stmt {
    Stmt::Guard { kind, .. } => guards && crate::interp::parse_kind(kind).is_some(),
    Stmt::SessionSet { .. } | Stmt::SessionDelete { .. } | Stmt::SessionExtend { .. } | Stmt::Act { .. } => false,
    Stmt::If { then, r#else, .. } => statements_through(then, guards) && statements_through(r#else, guards),
    Stmt::ForOf { body, .. } => statements_through(body, guards),
    Stmt::While { body, step, .. } => statements_through(body, guards) && statements_through(step, guards),
    Stmt::Try { body, handler, finally, .. } => statements_through(body, false) && handler.as_ref().is_none_or(|h| statements_through(h, false)) && statements_through(finally, false),
    _ => true,
  })
}

/// `guards` when `body` is the body's own level or a branch or loop of it, where a guard is a step; inside a block expression it is the interpreter's.
fn steps(body: &Body, bound: &mut Bound, guards: bool) -> Vec<Step> {
  let depth = bound.0.len();
  let out = body.iter().map(|stmt| step(stmt, bound, guards)).collect();
  bound.0.truncate(depth);
  out
}

fn step(stmt: &Stmt, bound: &mut Bound, guards: bool) -> Step {
  match stmt {
    Stmt::Let { name, expr } => {
      let code = code(expr, bound);
      bound.0.push(name.clone());
      Step::Let(Name::new(name), code)
    }
    Stmt::Set { name, expr } if !in_place(name, expr) => match bound.at(name) {
      Some(at) => Step::Set(at, code(expr, bound)),
      None => Step::Interpreted(vec![stmt.clone()]),
    },
    Stmt::If { cond, then, r#else } => Step::If(code(cond, bound), steps(then, bound, guards), steps(r#else, bound, guards)),
    Stmt::ForOf { name, over, body } => {
      let over = code(over, bound);
      bound.0.push(name.clone());
      let body = steps(body, bound, guards);
      bound.0.pop();
      Step::ForOf { name: Name::new(name), over, body }
    }
    Stmt::While { cond, body, step } => Step::While { cond: code(cond, bound), body: steps(body, bound, guards), step: steps(step, bound, guards) },
    Stmt::Return(expr) => Step::Return(code(expr, bound)),
    Stmt::Break => Step::Break,
    Stmt::Continue => Step::Continue,
    Stmt::Expr(expr) => Step::Expr(code(expr, bound)),
    Stmt::Guard { cond, kind, message } if guards => match crate::interp::parse_kind(kind) {
      Some(kind) => Step::Guard { cond: code(cond, bound), kind, message: code(message, bound) },
      None => Step::Interpreted(vec![stmt.clone()]),
    },
    other => Step::Interpreted(vec![other.clone()]),
  }
}

/// Whether the interpreter writes `name = expr` in place, `x = { ...x, k: v }` and its kin, which it keeps for itself.
fn in_place(name: &str, expr: &Expr) -> bool {
  let own = |e: &Expr| matches!(e, Expr::Var(n) if n == name);
  match expr {
    Expr::Object(entries) | Expr::Array(entries) => matches!(entries.first(), Some(Entry::Spread(first)) if own(first)),
    Expr::Builtin { name: crate::ast::Builtin::SetAt, args } => args.first().is_some_and(own),
    _ => false,
  }
}

fn code(expr: &Expr, bound: &mut Bound) -> Code {
  match expr {
    Expr::Var(name) => match bound.at(name) {
      Some(at) => Code::Local(at),
      None => Code::Interpreted(expr.clone()),
    },
    Expr::Lit(lit) => Code::Value(literal(lit)),
    Expr::Const(key) => Code::Const(key.clone()),
    Expr::Folded(value) => Code::Value(value.clone()),
    Expr::Field(target, name) => Code::Field(Box::new(code(target, bound)), Name::new(name)),
    Expr::Index(target, key) => Code::Index(Box::new(code(target, bound)), Box::new(code(key, bound))),
    Expr::Arith(op, l, r) => Code::Arith(*op, Box::new(code(l, bound)), Box::new(code(r, bound))),
    Expr::Compare(op, l, r) => Code::Compare(*op, Box::new(code(l, bound)), Box::new(code(r, bound))),
    Expr::Logic(op, l, r) => Code::Logic(*op, Box::new(code(l, bound)), Box::new(code(r, bound))),
    Expr::Not(e) => Code::Not(Box::new(code(e, bound))),
    Expr::Coalesce(l, r) => Code::Coalesce(Box::new(code(l, bound)), Box::new(code(r, bound))),
    Expr::Ternary(c, t, e) => Code::Ternary(Box::new(code(c, bound)), Box::new(code(t, bound)), Box::new(code(e, bound))),
    Expr::Template(parts) => Code::Template(parts.iter().map(|p| code(p, bound)).collect()),
    Expr::Object(entries) if entries.iter().all(|e| !matches!(e, Entry::Baked(_) | Entry::Item(_))) => Code::Object(entries.iter().map(|e| part(e, bound)).collect()),
    Expr::Array(entries) if entries.iter().all(|e| matches!(e, Entry::Item(_) | Entry::Spread(_))) => Code::Array(entries.iter().map(|e| part(e, bound)).collect()),
    Expr::Block(body) => Code::Block(steps(body, bound, false)),
    Expr::Apply { f, args } => {
      let compiled: Vec<Code> = args.iter().map(|a| code(a, bound)).collect();
      match &**f {
        Expr::Lambda { params, body } => {
          let depth = bound.0.len();
          bound.0.extend(params.iter().cloned());
          let body = code(body, bound);
          bound.0.truncate(depth);
          Code::CallInPlace { params: params.clone(), body: Box::new(body), args: compiled, written: args.clone() }
        }
        f => Code::Call { f: f.clone(), args: compiled, written: args.clone() },
      }
    }
    Expr::Lambda { params, body } => match Function::of(params, body, None) {
      Some(f) => Code::Lambda(Arc::new(f)),
      None => Code::Interpreted(expr.clone()),
    },
    Expr::Fix { name, f } => match &**f {
      Expr::Lambda { params, body } => match Function::of(params, body, Some(name.clone())) {
        Some(f) => Code::Lambda(Arc::new(f)),
        None => Code::Interpreted(expr.clone()),
      },
      _ => Code::Interpreted(expr.clone()),
    },
    Expr::Map(over, f) => match callback(f, 2, bound) {
      Some(f) => Code::Map(Box::new(code(over, bound)), f),
      None => Code::Interpreted(expr.clone()),
    },
    Expr::Filter(over, f) => match callback(f, 2, bound) {
      Some(f) => Code::Filter(Box::new(code(over, bound)), f),
      None => Code::Interpreted(expr.clone()),
    },
    Expr::Reduce(over, init, f) => match callback(f, 3, bound) {
      Some(f) => Code::Reduce(Box::new(code(over, bound)), Box::new(code(init, bound)), f),
      None => Code::Interpreted(expr.clone()),
    },
    Expr::Builtin { name: crate::ast::Builtin::Overwrite, args } if args.len() == 2 => match (&args[0], write(&args[0], &args[1], bound)) {
      (target @ Expr::Var(name), Some(write)) if bound.at(name).is_some() => Code::Write { target: Box::new(code(target, bound)), write },
      _ => Code::Interpreted(expr.clone()),
    },
    other => Code::Interpreted(other.clone()),
  }
}

fn part(entry: &Entry, bound: &mut Bound) -> Part {
  match entry {
    Entry::Field(name, e) => Part::Field(Name::new(name), code(e, bound)),
    Entry::Computed(k, e) => Part::Computed(code(k, bound), code(e, bound)),
    Entry::Spread(e) => Part::Spread(code(e, bound)),
    Entry::Item(e) => Part::Item(code(e, bound)),
    Entry::Baked(_) => unreachable!("an entry the caller refused"),
  }
}

/// The parts of an `Overwrite` the interpreter makes in place: fields set on an object spread first, items pushed on an array spread first.
fn write(target: &Expr, next: &Expr, bound: &mut Bound) -> Option<WriteCode> {
  match next {
    Expr::Object(entries) if matches!(entries.first(), Some(Entry::Spread(first)) if first == target) => {
      let rest = &entries[1..];
      if rest.iter().all(|e| matches!(e, Entry::Field(..))) {
        return Some(WriteCode::Fields(rest.iter().map(|e| match e {
          Entry::Field(name, value) => (Name::new(name), code(value, bound)),
          _ => unreachable!("checked"),
        }).collect()));
      }
      if rest.iter().all(|e| matches!(e, Entry::Computed(..))) {
        return Some(WriteCode::ComputedFields(rest.iter().map(|e| match e {
          Entry::Computed(key, value) => (code(key, bound), code(value, bound)),
          _ => unreachable!("checked"),
        }).collect()));
      }
      None
    }
    Expr::Builtin { name: crate::ast::Builtin::SetAt, args } if args.len() == 3 && args[0] == *target => Some(WriteCode::At(Box::new(code(&args[1], bound)), Box::new(code(&args[2], bound)))),
    Expr::Array(entries) if matches!(entries.first(), Some(Entry::Spread(first)) if first == target) && entries[1..].iter().all(|e| matches!(e, Entry::Item(_))) => Some(WriteCode::Items(entries[1..].iter().map(|e| match e {
      Entry::Item(value) => code(value, bound),
      _ => unreachable!("checked"),
    }).collect())),
    _ => None,
  }
}

/// A lambda written in place with no more than `most` plain parameters, prepared with them bound where the call stands.
fn callback(f: &Expr, most: usize, bound: &mut Bound) -> Option<Callback> {
  let Expr::Lambda { params, body } = f else { return None };
  if params.len() > most || params.iter().any(|p| p.starts_with("...")) {
    return None;
  }
  let depth = bound.0.len();
  bound.0.extend(params.iter().cloned());
  let body = code(body, bound);
  bound.0.truncate(depth);
  Some(Callback { params: params.iter().map(Name::new).collect(), body: Box::new(body) })
}

fn literal(lit: &Lit) -> Value {
  match lit {
    Lit::Null => Value::Null,
    Lit::Undefined => Value::Undefined,
    Lit::Bool(b) => Value::Bool(*b),
    Lit::Int(n) => Value::Int(*n),
    Lit::Float(f) => Value::F64(*f),
    Lit::Str(s) => Value::str(s.as_str()),
  }
}

fn run_steps(steps: &[Step], env: &mut Env, base: usize) -> Result<Flow, Fail> {
  let depth = env.scope.len();
  for step in steps {
    match step {
      Step::Let(name, code) => {
        let value = eval(code, env, base)?;
        env.scope.push((name.clone(), value));
      }
      Step::Guard { cond, kind, message } => {
        if crate::interp::truthy(&eval(cond, env, base)?) {
          let message = crate::interp::stringify(&eval(message, env, base)?)?;
          env.scope.truncate(depth);
          return Err(Fail::new(*kind, message));
        }
      }
      Step::Set(at, code) => {
        let value = eval(code, env, base)?;
        env.scope[base + at].1 = value;
      }
      Step::If(cond, then, otherwise) => {
        let branch = if crate::interp::truthy(&eval(cond, env, base)?) { then } else { otherwise };
        match run_steps(branch, env, base)? {
          Flow::Next => {}
          flow => {
            env.scope.truncate(depth);
            return Ok(flow);
          }
        }
      }
      Step::ForOf { name, over, body } => {
        let items = match eval(over, env, base)? {
          Value::Seq(items) => items,
          other => return Err(crate::interp::type_error("for...of", "an array", &other)),
        };
        for at in 0..items.len() {
          let item = items[at].clone();
          env.scope.push((name.clone(), item));
          let flow = run_steps(body, env, base);
          env.scope.pop();
          match flow? {
            Flow::Next | Flow::Continue => {}
            Flow::Break => break,
            Flow::Return(v) => {
              env.scope.truncate(depth);
              return Ok(Flow::Return(v));
            }
          }
        }
      }
      Step::While { cond, body, step } => {
        let mut passes = 0;
        while crate::interp::truthy(&eval(cond, env, base)?) {
          passes += 1;
          if passes > crate::interp::MAX_PASSES {
            return Err(Fail::internal(format!("a loop ran past {} passes", crate::interp::MAX_PASSES)));
          }
          match run_steps(body, env, base)? {
            Flow::Next | Flow::Continue => {}
            Flow::Break => break,
            Flow::Return(v) => {
              env.scope.truncate(depth);
              return Ok(Flow::Return(v));
            }
          }
          run_steps(step, env, base)?;
        }
      }
      Step::Return(code) => {
        let value = eval(code, env, base)?;
        env.scope.truncate(depth);
        return Ok(Flow::Return(value));
      }
      Step::Break => {
        env.scope.truncate(depth);
        return Ok(Flow::Break);
      }
      Step::Continue => {
        env.scope.truncate(depth);
        return Ok(Flow::Continue);
      }
      Step::Expr(code) => {
        eval(code, env, base)?;
      }
      Step::Interpreted(stmt) => match env.block_sync(stmt)? {
        Flow::Next => {}
        flow => {
          env.scope.truncate(depth);
          return Ok(flow);
        }
      },
    }
  }
  env.scope.truncate(depth);
  Ok(Flow::Next)
}

/// The local at `at`, as a body that wrote frozen objects sees it.
#[inline]
fn local<'a>(env: &'a Env, base: usize, at: usize) -> &'a Value {
  &env.scope[base + at].1
}

fn is_path(code: &Code) -> bool {
  match code {
    Code::Local(_) => true,
    Code::Field(target, _) => is_path(target),
    _ => false,
  }
}

/// What `code` reads in place, a local or a field of one, when reading it takes no copy of anything frozen this body wrote.
fn place<'a>(code: &Code, env: &'a Env, base: usize) -> Option<&'a Value> {
  let found = match code {
    Code::Local(at) => local(env, base, *at),
    Code::Field(target, name) => match place(target, env, base)? {
      Value::Map(map) => map.get(name.as_str())?,
      _ => return None,
    },
    _ => return None,
  };
  (env.thawed_is_empty() || !crate::interp::frozen(found)).then_some(found)
}

fn eval(code: &Code, env: &mut Env, base: usize) -> Result<Value, Fail> {
  let value = match code {
    Code::Value(value) => return Ok(value.clone()),
    Code::Const(key) => env.const_value(key)?,
    Code::Local(at) => {
      let value = local(env, base, *at);
      if env.thawed_is_empty() || !crate::interp::frozen(value) {
        return Ok(value.clone());
      }
      value.clone()
    }
    Code::Field(target, name) => {
      if let Some(value) = place(code, env, base) {
        return Ok(value.clone());
      }
      let target = eval(target, env, base)?;
      crate::interp::field_of(&target, name)
    }
    // A path of locals and fields reads nothing that changes, so it is read after the key as JavaScript reads it before.
    Code::Index(target, key) if is_path(target) => {
      let key = eval(key, env, base)?;
      let read = match place(target, env, base) {
        Some(target) => crate::interp::index_of(target, &key),
        None => {
          let target = eval(target, env, base)?;
          crate::interp::index_of(&target, &key)
        }
      };
      read?
    }
    Code::Index(target, key) => {
      let target = eval(target, env, base)?;
      let key = eval(key, env, base)?;
      crate::interp::index_of(&target, &key)?
    }
    Code::Arith(op, l, r) => {
      let l = eval(l, env, base)?;
      let r = eval(r, env, base)?;
      return env.arith(*op, l, r);
    }
    Code::Compare(op, l, r) => {
      let l = eval(l, env, base)?;
      let r = eval(r, env, base)?;
      return env.compare(*op, l, r).map(Value::Bool);
    }
    Code::Logic(op, l, r) => {
      let l = eval(l, env, base)?;
      return match (op, crate::interp::truthy(&l)) {
        (LogicOp::And, false) | (LogicOp::Or, true) => Ok(l),
        _ => eval(r, env, base),
      };
    }
    Code::Not(e) => return Ok(Value::Bool(!crate::interp::truthy(&eval(e, env, base)?))),
    Code::Coalesce(l, r) => {
      return match eval(l, env, base)? {
        Value::Null | Value::Undefined => eval(r, env, base),
        other => Ok(other),
      };
    }
    Code::Ternary(c, t, e) => {
      return if crate::interp::truthy(&eval(c, env, base)?) { eval(t, env, base) } else { eval(e, env, base) };
    }
    Code::Template(parts) => {
      return crate::interp::spelled(|out| {
        for part in parts {
          let value = eval(part, env, base)?;
          env.text_into(&value, out)?;
        }
        Ok(())
      });
    }
    Code::Object(parts) => return object(parts, env, base),
    Code::Array(parts) => {
      let mut items = Vec::with_capacity(parts.len());
      for part in parts {
        match part {
          Part::Item(e) => items.push(eval(e, env, base)?),
          Part::Spread(e) => match eval(e, env, base)? {
            Value::Seq(inner) => items.extend(inner),
            Value::Null | Value::Undefined => {}
            Value::Str(text) => items.extend(text.chars().map(|c| Value::str(c.to_string()))),
            other => return Err(crate::interp::type_error("spread into an array", "an array", &other)),
          },
          _ => unreachable!("an array holds items and spreads"),
        }
      }
      return Ok(Value::seq(items));
    }
    Code::Block(steps) => {
      let depth = env.scope.len();
      let flow = run_steps(steps, env, base);
      env.scope.truncate(depth);
      return flow?.value();
    }
    Code::Lambda(f) => return env.close_compiled(f),
    Code::CallInPlace { params, body, args, written } => {
      let mut values = Vec::with_capacity(args.len());
      for arg in args {
        values.push(eval(arg, env, base)?);
      }
      env.set_backing();
      let backing = env.take_backing();
      let depth = env.scope.len();
      crate::interp::bind(&mut env.scope, params, values);
      let result = eval(body, env, base);
      env.scope.truncate(depth);
      let value = env.unwrap_written_value(result, backing)?;
      env.write_back_args(written)?;
      return Ok(value);
    }
    Code::Call { f, args, written } => {
      let mut values = Vec::with_capacity(args.len());
      for arg in args {
        values.push(eval(arg, env, base)?);
      }
      env.set_backing();
      let value = env.apply_sync(f, values)?;
      env.write_back_args(written)?;
      return Ok(value);
    }
    Code::Map(over, f) => {
      let items = sequence(over, env, base, "map")?;
      let mut out = Vec::with_capacity(items.len());
      for (i, item) in items.into_iter().enumerate() {
        out.push(call(f, env, base, [item, Value::F64(i as f64), Value::Undefined])?);
      }
      return Ok(Value::seq(out));
    }
    Code::Filter(over, f) => {
      let items = sequence(over, env, base, "filter")?;
      let mut out = Vec::new();
      for (i, item) in items.into_iter().enumerate() {
        if crate::interp::truthy(&call(f, env, base, [item.clone(), Value::F64(i as f64), Value::Undefined])?) {
          out.push(item);
        }
      }
      return Ok(Value::seq(out));
    }
    Code::Reduce(over, init, f) => {
      let items = sequence(over, env, base, "reduce")?;
      let mut acc = eval(init, env, base)?;
      for (i, item) in items.into_iter().enumerate() {
        acc = call(f, env, base, [acc, item, Value::F64(i as f64)])?;
      }
      return Ok(acc);
    }
    Code::Write { target, write } => {
      let held = eval(target, env, base)?;
      let held = env.thawed_for_write(held);
      let write = match write {
        WriteCode::At(key, value) => {
          let key = eval(key, env, base)?;
          Write::At(key, eval(value, env, base)?)
        }
        WriteCode::Fields(fields) if fields.len() == 1 => Write::Field(fields[0].0.clone(), eval(&fields[0].1, env, base)?),
        WriteCode::ComputedFields(fields) if fields.len() == 1 => {
          let key = eval(&fields[0].0, env, base)?;
          Write::Field(crate::interp::key_name(key)?, eval(&fields[0].1, env, base)?)
        }
        WriteCode::Items(items) if items.len() == 1 => Write::Item(eval(&items[0], env, base)?),
        WriteCode::Fields(fields) => {
          let mut out = Vec::with_capacity(fields.len());
          for (name, value) in fields {
            out.push((name.clone(), eval(value, env, base)?));
          }
          Write::Fields(out)
        }
        WriteCode::ComputedFields(fields) => {
          let mut out = Vec::with_capacity(fields.len());
          for (key, value) in fields {
            let key = eval(key, env, base)?;
            out.push((crate::interp::key_name(key)?, eval(value, env, base)?));
          }
          Write::Fields(out)
        }
        WriteCode::Items(items) => {
          let mut out = Vec::with_capacity(items.len());
          for item in items {
            out.push(eval(item, env, base)?);
          }
          Write::Items(out)
        }
      };
      return env.write_into(held, write);
    }
    Code::Interpreted(expr) => return env.eval_sync(expr),
  };
  Ok(env.resolved_value(value))
}

fn sequence(over: &Code, env: &mut Env, base: usize, what: &str) -> Result<Vec<Value>, Fail> {
  match eval(over, env, base)? {
    Value::Seq(items) => Ok(items.into_items()),
    other => Err(crate::interp::type_error(what, "an array", &other)),
  }
}

/// A callback called with `args`, as many of them as it takes bound above the caller's locals.
fn call<const N: usize>(f: &Callback, env: &mut Env, base: usize, args: [Value; N]) -> Result<Value, Fail> {
  let backing = env.take_backing();
  let depth = env.scope.len();
  for (name, arg) in f.params.iter().zip(args) {
    env.scope.push((name.clone(), arg));
  }
  let result = eval(&f.body, env, base);
  env.scope.truncate(depth);
  env.unwrap_written_value(result, backing)
}

fn object(parts: &[Part], env: &mut Env, base: usize) -> Result<Value, Fail> {
  let mut map = if matches!(parts.first(), Some(Part::Spread(_))) { Fields::new() } else { Fields::with_capacity(parts.len()) };
  for (at, part) in parts.iter().enumerate() {
    match part {
      Part::Field(name, e) => {
        let value = eval(e, env, base)?;
        map.insert(name.clone(), value);
      }
      Part::Computed(k, e) => {
        let key = crate::interp::key_text(&eval(k, env, base)?)?;
        let value = eval(e, env, base)?;
        map.insert(key, value);
      }
      Part::Spread(e) => match eval(e, env, base)? {
        Value::Map(inner) if map.is_empty() => map = inner.into_fields_with_room(parts.len() - at - 1),
        Value::Map(inner) => {
          map.reserve(inner.len());
          map.extend(inner);
        }
        Value::Null | Value::Undefined => {}
        other => return Err(crate::interp::type_error("spread into an object", "an object", &other)),
      },
      Part::Item(_) => return Err(Fail::internal("an object literal has no positional entries")),
    }
  }
  Ok(Value::Map(ValueMap::from(map)))
}

/// A program shared by every request that runs one body.
pub type Shared = Arc<Program>;
