use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use futures_util::future::{join_all, BoxFuture};
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_runtime::{FailureKind, RequestCtx, SessionCell};

use crate::catalog::Catalogs;
use crate::ext::{Ambient, Extensions};
use crate::ast::{ArithOp, Body, Builtin, CompareOp, Entry, Expr, Lit, LogicOp, Stmt};
use crate::bind::kind_name;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("{}: {message}", kind.as_str())]
pub struct Fail {
  pub kind: FailureKind,
  pub message: String,
}

impl Fail {
  pub fn new(kind: FailureKind, message: impl Into<String>) -> Self {
    Self { kind, message: message.into() }
  }

  pub(crate) fn internal(message: impl Into<String>) -> Self {
    Self::new(FailureKind::Internal, message)
  }
}

pub struct Outcome {
  pub value: Value,
  /// Session keys the body wrote or deleted, in order, committed before this
  /// was returned.
  pub written: Vec<String>,
}

/// What `ctx.now` reads. Milliseconds since the Unix epoch, as `Value::Int`.
pub trait Clock: Send + Sync {
  fn now(&self) -> i128;
}

struct SystemClock;

impl Clock for SystemClock {
  fn now(&self) -> i128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i128).unwrap_or(0)
  }
}

#[derive(Clone)]
pub struct Interpreter {
  clock: Arc<dyn Clock>,
  extensions: Arc<Extensions>,
  catalogs: Option<Arc<Catalogs>>,
}

impl Default for Interpreter {
  fn default() -> Self {
    Self { clock: Arc::new(SystemClock), extensions: Arc::new(Extensions::standard()), catalogs: None }
  }
}

impl Interpreter {
  pub fn with_clock(clock: Arc<dyn Clock>) -> Self {
    Self { clock, extensions: Arc::new(Extensions::standard()), catalogs: None }
  }

  /// The message catalogs `i18n.t` reads; none by default, where every key answers as itself.
  pub fn with_catalogs(mut self, catalogs: Option<Arc<Catalogs>>) -> Self {
    self.catalogs = catalogs;
    self
  }

  pub fn catalogs(&self) -> Option<&Arc<Catalogs>> {
    self.catalogs.as_ref()
  }

  /// The same interpreter answering `extensions` in place of the standard library alone.
  pub fn with_extensions(mut self, extensions: Arc<Extensions>) -> Self {
    self.extensions = extensions;
    self
  }

  pub fn extensions(&self) -> &Arc<Extensions> {
    &self.extensions
  }

  /// Evaluates one expression over `scope` with no request: what a test's
  /// assertion or a rendered component reads.
  pub async fn evaluate(&self, expr: &Expr, scope: Vec<(String, Value)>) -> Result<Value, Fail> {
    let mut env = Env::detached(self, scope);
    env.eval(expr).await
  }

  /// Applies a lambda to `args` with no request: what a mocked service method is.
  pub async fn apply(&self, lambda: &Expr, args: Vec<Value>) -> Result<Value, Fail> {
    let mut env = Env::detached(self, Vec::new());
    env.apply(lambda, args).await
  }

  /// Runs a body. Session writes land in a draft committed only on success.
  /// A guard that reads no bound variable and sits before the first statement
  /// that could write the session runs before anything else, so a call is
  /// never made for a request a guard would refuse.
  pub async fn run(&self, body: &Body, ctx: &RequestCtx, input: Option<Value>) -> Result<Outcome, Fail> {
    let (data, identity) = ctx.session.snapshot();
    let mut env = Env {
      ctx: ctx.clone(),
      store: ValueMap::new(),
      hoists: None,
      state: None,
      server_mode: false,
      input: input.unwrap_or(Value::Null),
      identity: identity.map(|id| {
        let mut map = ValueMap::new();
        map.insert("subject".to_owned(), Value::Str(id.subject));
        map.insert("claims".to_owned(), Value::Map(id.claims));
        Value::Map(map)
      }),
      session: data,
      written: Vec::new(),
      scope: Vec::new(),
      clock: self.clock.clone(),
      extensions: self.extensions.clone(),
      catalogs: self.catalogs.clone(),
    };

    for stmt in body {
      match stmt {
        Stmt::Guard { cond, kind, message } => {
          let mut free = Vec::new();
          cond.free_vars(&mut free);
          if free.is_empty() && !cond.has_call() {
            env.guard(cond, kind, message).await?;
          }
        }
        Stmt::Let { .. } | Stmt::Return(_) | Stmt::Expr(_) => {}
        Stmt::If { .. } | Stmt::ForOf { .. } | Stmt::SessionSet { .. } | Stmt::SessionDelete { .. } => break,
      }
    }

    let value = match env.block(body).await? {
      Flow::Return(value) => value,
      Flow::Next => Value::Null,
    };
    commit(&ctx.session, &env.session, &env.written);
    Ok(Outcome { value, written: env.written })
  }
}

fn commit(cell: &SessionCell, draft: &ValueMap, written: &[String]) {
  for key in written {
    match draft.get(key) {
      Some(value) => cell.insert(key.clone(), value.clone()),
      None => {
        cell.remove(key);
      }
    }
  }
}

pub(crate) enum Flow {
  Next,
  Return(Value),
}

pub(crate) struct Env {
  pub(crate) ctx: RequestCtx,
  input: Value,
  identity: Option<Value>,
  session: ValueMap,
  written: Vec<String>,
  pub(crate) scope: Vec<(String, Value)>,
  pub(crate) store: ValueMap,
  clock: Arc<dyn Clock>,
  extensions: Arc<Extensions>,
  catalogs: Option<Arc<Catalogs>>,
  /// Where a render records hoisted values; `None` in a body, which hoists nothing.
  pub(crate) hoists: Option<Hoists>,
  /// Values that stand in for a component's state `let`s, when an island in
  /// server mode renders after a handler ran.
  pub(crate) state: Option<ValueMap>,
  /// Rendering an island in server mode: handler markers print as attributes
  /// the browser binds, which a browser-mode render must never show React.
  pub(crate) server_mode: bool,
}

/// The hoisted values of one island: keyed by the module, the hoist id and
/// the indices of the loops enclosing it, `module|id@i.j`. A key recorded
/// twice with different values is dead: the browser computes it instead.
#[derive(Debug, Default, Clone)]
pub struct Hoists {
  pub module: String,
  pub path: Vec<usize>,
  pub table: ValueMap,
  dead: Vec<String>,
}

impl Hoists {
  pub fn new(module: impl Into<String>) -> Self {
    Self { module: module.into(), path: Vec::new(), table: ValueMap::new(), dead: Vec::new() }
  }

  pub fn key(&self, id: u32) -> String {
    let mut key = format!("{}|{id}", self.module);
    if !self.path.is_empty() {
      key.push('@');
      for (i, index) in self.path.iter().enumerate() {
        if i > 0 {
          key.push('.');
        }
        key.push_str(&index.to_string());
      }
    }
    key
  }

  pub fn record(&mut self, id: u32, value: &Value) {
    let key = self.key(id);
    if self.dead.contains(&key) {
      return;
    }
    match self.table.get(&key) {
      Some(existing) if existing == value => {}
      Some(_) => {
        self.table.shift_remove(&key);
        self.dead.push(key);
      }
      None => {
        self.table.insert(key, value.clone());
      }
    }
  }
}

impl Env {
  /// An environment for a render: no request, no session, no input; only
  /// the scope a component's props make.
  pub(crate) fn detached(interpreter: &Interpreter, scope: Vec<(String, Value)>) -> Self {
    Self {
      ctx: RequestCtx::anonymous(Default::default()),
      input: Value::Null,
      identity: None,
      session: ValueMap::new(),
      written: Vec::new(),
      scope,
      store: ValueMap::new(),
      clock: interpreter.clock.clone(),
      extensions: interpreter.extensions.clone(),
      catalogs: interpreter.catalogs.clone(),
      hoists: None,
      state: None,
      server_mode: false,
    }
  }

  fn lookup(&self, name: &str) -> Result<Value, Fail> {
    self
      .scope
      .iter()
      .rev()
      .find(|(n, _)| n == name)
      .map(|(_, v)| v.clone())
      .ok_or_else(|| Fail::internal(format!("`{name}` is not bound")))
  }

  /// The request's locale, or null under a context that has none.
  fn locale(&self) -> Value {
    if self.ctx.locale.tag.is_empty() {
      Value::Null
    } else {
      Value::Str(self.ctx.locale.tag.clone())
    }
  }

  fn touch(&mut self, key: &str) {
    if !self.written.iter().any(|k| k == key) {
      self.written.push(key.to_owned());
    }
  }

  async fn guard(&mut self, cond: &Expr, kind: &str, message: &str) -> Result<(), Fail> {
    if truthy(&self.eval(cond).await?) {
      let kind = parse_kind(kind).ok_or_else(|| Fail::internal(format!("`{kind}` is not a failure kind")))?;
      return Err(Fail::new(kind, message));
    }
    Ok(())
  }

  pub(crate) fn block<'a>(&'a mut self, body: &'a Body) -> BoxFuture<'a, Result<Flow, Fail>> {
    Box::pin(async move {
      let depth = self.scope.len();
      let mut i = 0;
      while i < body.len() {
        let run = independent_lets(&body[i..]);
        if run > 1 {
          let names_and_values = self.parallel_lets(&body[i..i + run]).await?;
          self.scope.extend(names_and_values);
          i += run;
          continue;
        }
        match self.stmt(&body[i]).await? {
          Flow::Next => {}
          Flow::Return(v) => {
            self.scope.truncate(depth);
            return Ok(Flow::Return(v));
          }
        }
        i += 1;
      }
      self.scope.truncate(depth);
      Ok(Flow::Next)
    })
  }

  async fn parallel_lets(&mut self, stmts: &[Stmt]) -> Result<Vec<(String, Value)>, Fail> {
    let futures: Vec<_> = stmts
      .iter()
      .map(|stmt| match stmt {
        Stmt::Let { name, expr } => {
          let name = name.clone();
          let mut snapshot = self.snapshot();
          async move { snapshot.eval(expr).await.map(|v| (name, v)) }
        }
        _ => unreachable!("independent_lets only groups lets"),
      })
      .collect();
    join_all(futures).await.into_iter().collect()
  }

  /// A read-only copy for evaluating in parallel. Session writes are
  /// statements, never expressions, so a copy cannot diverge from the draft.
  fn snapshot(&self) -> Env {
    Env {
      ctx: self.ctx.clone(),
      input: self.input.clone(),
      identity: self.identity.clone(),
      session: self.session.clone(),
      written: Vec::new(),
      scope: self.scope.clone(),
      store: self.store.clone(),
      clock: self.clock.clone(),
      extensions: self.extensions.clone(),
      catalogs: self.catalogs.clone(),
      hoists: None,
      state: None,
      server_mode: false,
    }
  }

  fn ambient(&self) -> Ambient {
    Ambient { locale: self.ctx.locale.tag.clone(), now: self.clock.now(), catalogs: self.catalogs.clone() }
  }

  async fn stmt(&mut self, stmt: &Stmt) -> Result<Flow, Fail> {
    match stmt {
      Stmt::Let { name, expr } => {
        let value = self.eval(expr).await?;
        self.scope.push((name.clone(), value));
      }
      Stmt::If { cond, then, r#else } => {
        let branch = if truthy(&self.eval(cond).await?) { then } else { r#else };
        if let Flow::Return(v) = self.block(branch).await? {
          return Ok(Flow::Return(v));
        }
      }
      Stmt::ForOf { name, over, body } => {
        let items = match self.eval(over).await? {
          Value::Seq(items) => items,
          other => return Err(type_error("for...of", "an array", &other)),
        };
        for item in items {
          self.scope.push((name.clone(), item));
          let flow = self.block(body).await;
          self.scope.pop();
          if let Flow::Return(v) = flow? {
            return Ok(Flow::Return(v));
          }
        }
      }
      Stmt::Return(expr) => return Ok(Flow::Return(self.eval(expr).await?)),
      Stmt::Guard { cond, kind, message } => self.guard(cond, kind, message).await?,
      Stmt::SessionSet { key, path, value } => {
        let value = self.eval(value).await?;
        let mut steps = Vec::with_capacity(path.len());
        for step in path {
          steps.push(self.eval(step).await?);
        }
        let root = self.session.entry(key.clone()).or_insert(Value::Null);
        set_path(root, &steps, value)?;
        self.touch(key);
      }
      Stmt::SessionDelete { key, path } => {
        let mut steps = Vec::with_capacity(path.len());
        for step in path {
          steps.push(self.eval(step).await?);
        }
        if steps.is_empty() {
          self.session.shift_remove(key);
        } else if let Some(root) = self.session.get_mut(key) {
          delete_path(root, &steps)?;
        }
        self.touch(key);
      }
      Stmt::Expr(expr) => {
        self.eval(expr).await?;
      }
    }
    Ok(Flow::Next)
  }

  /// `eval` for an expression with no service call in it: a component body,
  /// a render, a test's assertion. One plain recursion and no future per node.
  pub(crate) fn eval_sync(&mut self, expr: &Expr) -> Result<Value, Fail> {
    match expr {
      Expr::Param(name) => Ok(self.ctx.params.get(name).map(|s| Value::Str(s.clone())).unwrap_or(Value::Null)),
      Expr::Query(name) => Ok(self.ctx.query.get(name).map(|s| Value::Str(s.clone())).unwrap_or(Value::Null)),
      Expr::Session(key) => Ok(self.session.get(key).cloned().unwrap_or(Value::Null)),
      Expr::Store(key) => Ok(self.store.get(key).cloned().unwrap_or(Value::Null)),
      Expr::Locale => Ok(self.locale()),
      Expr::Identity(path) => {
        let mut current = self.identity.clone().unwrap_or(Value::Null);
        for step in path {
          current = get_field(&current, step);
        }
        Ok(current)
      }
      Expr::Input => Ok(self.input.clone()),
      Expr::Now => Ok(Value::Int(self.clock.now())),
      Expr::Var(name) => self.lookup(name),
      Expr::Lit(lit) => Ok(match lit {
        Lit::Null => Value::Null,
        Lit::Bool(b) => Value::Bool(*b),
        Lit::Int(n) => Value::Int(*n),
        Lit::Float(f) => Value::F64(*f),
        Lit::Str(s) => Value::Str(s.clone()),
      }),
      Expr::Object(entries) => {
        let mut map = ValueMap::new();
        for entry in entries {
          match entry {
            Entry::Field(name, e) => {
              let v = self.eval_sync(e)?;
              map.insert(name.clone(), v);
            }
            Entry::Computed(k, e) => {
              let key = match self.eval_sync(k)? {
                Value::Str(s) => s,
                other => return Err(type_error("a computed key", "a string", &other)),
              };
              let v = self.eval_sync(e)?;
              map.insert(key, v);
            }
            Entry::Spread(e) => match self.eval_sync(e)? {
              Value::Map(inner) => map.extend(inner),
              Value::Null => {}
              other => return Err(type_error("spread into an object", "an object", &other)),
            },
            Entry::Item(_) => return Err(Fail::internal("an object literal has no positional entries")),
          }
        }
        Ok(Value::Map(map))
      }
      Expr::Array(entries) => {
        let mut items = Vec::new();
        for entry in entries {
          match entry {
            Entry::Item(e) => items.push(self.eval_sync(e)?),
            Entry::Spread(e) => match self.eval_sync(e)? {
              Value::Seq(inner) => items.extend(inner),
              Value::Null => {}
              other => return Err(type_error("spread into an array", "an array", &other)),
            },
            Entry::Field(..) | Entry::Computed(..) => return Err(Fail::internal("an array literal has no named entries")),
          }
        }
        Ok(Value::Seq(items))
      }
      Expr::Field(target, name) => Ok(get_field(&self.eval_sync(target)?, name)),
      Expr::Index(target, key) => {
        let target = self.eval_sync(target)?;
        let key = self.eval_sync(key)?;
        index(&target, &key)
      }
      Expr::Arith(op, l, r) => {
        let l = self.eval_sync(l)?;
        let r = self.eval_sync(r)?;
        arith(*op, l, r)
      }
      Expr::Compare(op, l, r) => {
        let l = self.eval_sync(l)?;
        let r = self.eval_sync(r)?;
        compare(*op, &l, &r).map(Value::Bool)
      }
      Expr::Logic(op, l, r) => {
        let l = self.eval_sync(l)?;
        match (op, truthy(&l)) {
          (LogicOp::And, false) | (LogicOp::Or, true) => Ok(l),
          _ => self.eval_sync(r),
        }
      }
      Expr::Not(e) => Ok(Value::Bool(!truthy(&self.eval_sync(e)?))),
      Expr::Coalesce(l, r) => match self.eval_sync(l)? {
        Value::Null => self.eval_sync(r),
        v => Ok(v),
      },
      Expr::Ternary(c, t, e) => {
        if truthy(&self.eval_sync(c)?) {
          self.eval_sync(t)
        } else {
          self.eval_sync(e)
        }
      }
      Expr::Template(parts) => {
        let mut out = String::new();
        for part in parts {
          out.push_str(&stringify(&self.eval_sync(part)?)?);
        }
        Ok(Value::Str(out))
      }
      Expr::Call { .. } => Err(Fail::internal("a service call in an expression that cannot suspend")),
      Expr::Lambda { .. } => Err(Fail::internal("a lambda is applied, never a value")),
      Expr::Hoist { id, expr } => {
        let value = self.eval_sync(expr)?;
        if let Some(hoists) = &mut self.hoists {
          hoists.record(*id, &value);
        }
        Ok(value)
      }
      Expr::Apply { f, args } => {
        let mut values = Vec::with_capacity(args.len());
        for arg in args {
          values.push(self.eval_sync(arg)?);
        }
        self.apply_sync(f, values)
      }
      Expr::Builtin { name, args } => {
        let mut values = Vec::with_capacity(args.len());
        for arg in args {
          values.push(self.eval_sync(arg)?);
        }
        builtin(*name, values)
      }
      Expr::Ext { module, name, args } => {
        let mut values = Vec::with_capacity(args.len());
        for arg in args {
          values.push(self.eval_sync(arg)?);
        }
        self.extensions.call(&format!("{module}.{name}"), &self.ambient(), &values)
      }
      Expr::Map(over, f) => {
        let items = self.seq_sync(over, "map")?;
        let mut out = Vec::with_capacity(items.len());
        for (i, item) in items.into_iter().enumerate() {
          out.push(self.apply_sync(f, vec![item, Value::F64(i as f64)])?);
        }
        Ok(Value::Seq(out))
      }
      Expr::Filter(over, f) => {
        let items = self.seq_sync(over, "filter")?;
        let mut out = Vec::new();
        for (i, item) in items.into_iter().enumerate() {
          if truthy(&self.apply_sync(f, vec![item.clone(), Value::F64(i as f64)])?) {
            out.push(item);
          }
        }
        Ok(Value::Seq(out))
      }
      Expr::Reduce(over, init, f) => {
        let items = self.seq_sync(over, "reduce")?;
        let mut acc = self.eval_sync(init)?;
        for item in items {
          acc = self.apply_sync(f, vec![acc, item])?;
        }
        Ok(acc)
      }
      Expr::Find(over, f) => {
        let items = self.seq_sync(over, "find")?;
        for (i, item) in items.into_iter().enumerate() {
          if truthy(&self.apply_sync(f, vec![item.clone(), Value::F64(i as f64)])?) {
            return Ok(item);
          }
        }
        Ok(Value::Null)
      }
      Expr::FindIndex(over, f) => {
        let items = self.seq_sync(over, "findIndex")?;
        for (i, item) in items.into_iter().enumerate() {
          if truthy(&self.apply_sync(f, vec![item, Value::F64(i as f64)])?) {
            return Ok(Value::F64(i as f64));
          }
        }
        Ok(Value::F64(-1.0))
      }
      Expr::Some(over, f) => {
        let items = self.seq_sync(over, "some")?;
        for item in items {
          if truthy(&self.apply_sync(f, vec![item])?) {
            return Ok(Value::Bool(true));
          }
        }
        Ok(Value::Bool(false))
      }
      Expr::Every(over, f) => {
        let items = self.seq_sync(over, "every")?;
        for item in items {
          if !truthy(&self.apply_sync(f, vec![item])?) {
            return Ok(Value::Bool(false));
          }
        }
        Ok(Value::Bool(true))
      }
      Expr::Entries(e) => match self.eval_sync(e)? {
        Value::Map(map) => Ok(Value::Seq(
          map.into_iter().map(|(k, v)| Value::Seq(vec![Value::Str(k), v])).collect(),
        )),
        other => Err(type_error("Object.entries", "an object", &other)),
      },
      Expr::Keys(e) => match self.eval_sync(e)? {
        Value::Map(map) => Ok(Value::Seq(map.into_keys().map(Value::Str).collect())),
        other => Err(type_error("Object.keys", "an object", &other)),
      },
      Expr::Values(e) => match self.eval_sync(e)? {
        Value::Map(map) => Ok(Value::Seq(map.into_values().collect())),
        other => Err(type_error("Object.values", "an object", &other)),
      },
      Expr::Length(e) => match self.eval_sync(e)? {
        Value::Seq(items) => Ok(Value::F64(items.len() as f64)),
        Value::Str(s) => Ok(Value::F64(s.chars().count() as f64)),
        Value::Map(map) => Ok(Value::F64(map.len() as f64)),
        other => Err(type_error("length", "an array, a string or an object", &other)),
      },
      Expr::Str(e) => stringify(&self.eval_sync(e)?).map(Value::Str),
      Expr::Num(e) => match self.eval_sync(e)? {
        Value::Int(n) => Ok(Value::F64(n as f64)),
        Value::UInt(n) => Ok(Value::F64(n as f64)),
        v @ (Value::F32(_) | Value::F64(_)) => Ok(v),
        Value::Bool(b) => Ok(Value::F64(if b { 1.0 } else { 0.0 })),
        Value::Str(s) => s
          .trim()
          .parse::<f64>()
          .map(Value::F64)
          .map_err(|_| Fail::new(FailureKind::Invalid, format!("`{s}` is not a number"))),
        other => Err(type_error("Number", "a number, a string or a boolean", &other)),
      },
      Expr::BigInt(e) => match self.eval_sync(e)? {
        v @ Value::Int(_) => Ok(v),
        Value::UInt(n) => Ok(Value::Int(n as i128)),
        Value::F64(f) if f.fract() == 0.0 && f.abs() <= SAFE_INTEGER => Ok(Value::Int(f as i128)),
        Value::F32(f) if f.fract() == 0.0 && (f as f64).abs() <= SAFE_INTEGER => Ok(Value::Int(f as i128)),
        Value::Bool(b) => Ok(Value::Int(i128::from(b))),
        Value::Str(s) => s
          .trim()
          .parse::<i128>()
          .map(Value::Int)
          .map_err(|_| Fail::new(FailureKind::Invalid, format!("`{s}` is not an integer"))),
        other => Err(type_error("BigInt", "an integer, an integral number or a string", &other)),
      },
    }
  }

  fn seq_sync(&mut self, over: &Expr, what: &str) -> Result<Vec<Value>, Fail> {
    match self.eval_sync(over)? {
      Value::Seq(items) => Ok(items),
      other => Err(type_error(what, "an array", &other)),
    }
  }

  pub(crate) fn apply_sync(&mut self, f: &Expr, args: Vec<Value>) -> Result<Value, Fail> {
    let Expr::Lambda { params, body } = f else {
      return Err(Fail::internal("a builtin takes a lambda"));
    };
    let depth = self.scope.len();
    for (param, arg) in params.iter().zip(args) {
      self.scope.push((param.clone(), arg));
    }
    let result = self.eval_sync(body);
    self.scope.truncate(depth);
    result
  }

  /// An expression with a service call somewhere in it suspends on the call;
  /// one without goes through `eval_sync` and never allocates a future.
  pub(crate) fn eval<'a>(&'a mut self, expr: &'a Expr) -> BoxFuture<'a, Result<Value, Fail>> {
    if !expr.has_call() {
      let result = self.eval_sync(expr);
      return Box::pin(async move { result });
    }
    Box::pin(async move {
      match expr {
        Expr::Param(name) => Ok(self.ctx.params.get(name).map(|s| Value::Str(s.clone())).unwrap_or(Value::Null)),
        Expr::Query(name) => Ok(self.ctx.query.get(name).map(|s| Value::Str(s.clone())).unwrap_or(Value::Null)),
        Expr::Session(key) => Ok(self.session.get(key).cloned().unwrap_or(Value::Null)),
        Expr::Store(key) => Ok(self.store.get(key).cloned().unwrap_or(Value::Null)),
        Expr::Locale => Ok(self.locale()),
        Expr::Identity(path) => {
          let mut current = self.identity.clone().unwrap_or(Value::Null);
          for step in path {
            current = get_field(&current, step);
          }
          Ok(current)
        }
        Expr::Input => Ok(self.input.clone()),
        Expr::Now => Ok(Value::Int(self.clock.now())),
        Expr::Var(name) => self.lookup(name),
        Expr::Lit(lit) => Ok(match lit {
          Lit::Null => Value::Null,
          Lit::Bool(b) => Value::Bool(*b),
          Lit::Int(n) => Value::Int(*n),
          Lit::Float(f) => Value::F64(*f),
          Lit::Str(s) => Value::Str(s.clone()),
        }),
        Expr::Object(entries) => {
          let mut map = ValueMap::new();
          for entry in entries {
            match entry {
              Entry::Field(name, e) => {
                let v = self.eval(e).await?;
                map.insert(name.clone(), v);
              }
              Entry::Computed(k, e) => {
                let key = match self.eval(k).await? {
                  Value::Str(s) => s,
                  other => return Err(type_error("a computed key", "a string", &other)),
                };
                let v = self.eval(e).await?;
                map.insert(key, v);
              }
              Entry::Spread(e) => match self.eval(e).await? {
                Value::Map(inner) => map.extend(inner),
                Value::Null => {}
                other => return Err(type_error("spread into an object", "an object", &other)),
              },
              Entry::Item(_) => return Err(Fail::internal("an object literal has no positional entries")),
            }
          }
          Ok(Value::Map(map))
        }
        Expr::Array(entries) => {
          let mut items = Vec::new();
          for entry in entries {
            match entry {
              Entry::Item(e) => items.push(self.eval(e).await?),
              Entry::Spread(e) => match self.eval(e).await? {
                Value::Seq(inner) => items.extend(inner),
                Value::Null => {}
                other => return Err(type_error("spread into an array", "an array", &other)),
              },
              Entry::Field(..) | Entry::Computed(..) => return Err(Fail::internal("an array literal has no named entries")),
            }
          }
          Ok(Value::Seq(items))
        }
        Expr::Field(target, name) => Ok(get_field(&self.eval(target).await?, name)),
        Expr::Index(target, key) => {
          let target = self.eval(target).await?;
          let key = self.eval(key).await?;
          index(&target, &key)
        }
        Expr::Arith(op, l, r) => {
          let l = self.eval(l).await?;
          let r = self.eval(r).await?;
          arith(*op, l, r)
        }
        Expr::Compare(op, l, r) => {
          let l = self.eval(l).await?;
          let r = self.eval(r).await?;
          compare(*op, &l, &r).map(Value::Bool)
        }
        Expr::Logic(op, l, r) => {
          let l = self.eval(l).await?;
          match (op, truthy(&l)) {
            (LogicOp::And, false) | (LogicOp::Or, true) => Ok(l),
            _ => self.eval(r).await,
          }
        }
        Expr::Not(e) => Ok(Value::Bool(!truthy(&self.eval(e).await?))),
        Expr::Coalesce(l, r) => match self.eval(l).await? {
          Value::Null => self.eval(r).await,
          v => Ok(v),
        },
        Expr::Ternary(c, t, e) => {
          if truthy(&self.eval(c).await?) {
            self.eval(t).await
          } else {
            self.eval(e).await
          }
        }
        Expr::Template(parts) => {
          let mut out = String::new();
          for part in parts {
            out.push_str(&stringify(&self.eval(part).await?)?);
          }
          Ok(Value::Str(out))
        }
        Expr::Call { service, method, args } => {
          let mut map = ValueMap::new();
          for (name, e) in args {
            match self.eval(e).await? {
              Value::Null => {}
              v => {
                map.insert(name.clone(), v);
              }
            }
          }
          self.ctx.services.call(service, method, map).await.map_err(|e| Fail::new(e.kind, e.message))
        }
        Expr::Lambda { .. } => Err(Fail::internal("a lambda is applied, never a value")),
        Expr::Hoist { expr, .. } => self.eval(expr).await,
        Expr::Apply { f, args } => {
          let mut values = Vec::with_capacity(args.len());
          for arg in args {
            values.push(self.eval(arg).await?);
          }
          self.apply(f, values).await
        }
        Expr::Builtin { name, args } => {
          let mut values = Vec::with_capacity(args.len());
          for arg in args {
            values.push(self.eval(arg).await?);
          }
          builtin(*name, values)
        }
        Expr::Ext { module, name, args } => {
          let mut values = Vec::with_capacity(args.len());
          for arg in args {
            values.push(self.eval(arg).await?);
          }
          self.extensions.call(&format!("{module}.{name}"), &self.ambient(), &values)
        }
        Expr::Map(over, f) => {
          let items = self.seq(over, "map").await?;
          let mut out = Vec::with_capacity(items.len());
          for (i, item) in items.into_iter().enumerate() {
            out.push(self.apply(f, vec![item, Value::F64(i as f64)]).await?);
          }
          Ok(Value::Seq(out))
        }
        Expr::Filter(over, f) => {
          let items = self.seq(over, "filter").await?;
          let mut out = Vec::new();
          for (i, item) in items.into_iter().enumerate() {
            if truthy(&self.apply(f, vec![item.clone(), Value::F64(i as f64)]).await?) {
              out.push(item);
            }
          }
          Ok(Value::Seq(out))
        }
        Expr::Reduce(over, init, f) => {
          let items = self.seq(over, "reduce").await?;
          let mut acc = self.eval(init).await?;
          for item in items {
            acc = self.apply(f, vec![acc, item]).await?;
          }
          Ok(acc)
        }
        Expr::Find(over, f) => {
          let items = self.seq(over, "find").await?;
          for (i, item) in items.into_iter().enumerate() {
            if truthy(&self.apply(f, vec![item.clone(), Value::F64(i as f64)]).await?) {
              return Ok(item);
            }
          }
          Ok(Value::Null)
        }
        Expr::FindIndex(over, f) => {
          let items = self.seq(over, "findIndex").await?;
          for (i, item) in items.into_iter().enumerate() {
            if truthy(&self.apply(f, vec![item, Value::F64(i as f64)]).await?) {
              return Ok(Value::F64(i as f64));
            }
          }
          Ok(Value::F64(-1.0))
        }
        Expr::Some(over, f) => {
          let items = self.seq(over, "some").await?;
          for item in items {
            if truthy(&self.apply(f, vec![item]).await?) {
              return Ok(Value::Bool(true));
            }
          }
          Ok(Value::Bool(false))
        }
        Expr::Every(over, f) => {
          let items = self.seq(over, "every").await?;
          for item in items {
            if !truthy(&self.apply(f, vec![item]).await?) {
              return Ok(Value::Bool(false));
            }
          }
          Ok(Value::Bool(true))
        }
        Expr::Entries(e) => match self.eval(e).await? {
          Value::Map(map) => Ok(Value::Seq(
            map.into_iter().map(|(k, v)| Value::Seq(vec![Value::Str(k), v])).collect(),
          )),
          other => Err(type_error("Object.entries", "an object", &other)),
        },
        Expr::Keys(e) => match self.eval(e).await? {
          Value::Map(map) => Ok(Value::Seq(map.into_keys().map(Value::Str).collect())),
          other => Err(type_error("Object.keys", "an object", &other)),
        },
        Expr::Values(e) => match self.eval(e).await? {
          Value::Map(map) => Ok(Value::Seq(map.into_values().collect())),
          other => Err(type_error("Object.values", "an object", &other)),
        },
        Expr::Length(e) => match self.eval(e).await? {
          Value::Seq(items) => Ok(Value::F64(items.len() as f64)),
          Value::Str(s) => Ok(Value::F64(s.chars().count() as f64)),
          Value::Map(map) => Ok(Value::F64(map.len() as f64)),
          other => Err(type_error("length", "an array, a string or an object", &other)),
        },
        Expr::Str(e) => stringify(&self.eval(e).await?).map(Value::Str),
        Expr::Num(e) => match self.eval(e).await? {
          Value::Int(n) => Ok(Value::F64(n as f64)),
          Value::UInt(n) => Ok(Value::F64(n as f64)),
          v @ (Value::F32(_) | Value::F64(_)) => Ok(v),
          Value::Bool(b) => Ok(Value::F64(if b { 1.0 } else { 0.0 })),
          Value::Str(s) => s
            .trim()
            .parse::<f64>()
            .map(Value::F64)
            .map_err(|_| Fail::new(FailureKind::Invalid, format!("`{s}` is not a number"))),
          other => Err(type_error("Number", "a number, a string or a boolean", &other)),
        },
        Expr::BigInt(e) => match self.eval(e).await? {
          v @ Value::Int(_) => Ok(v),
          Value::UInt(n) => Ok(Value::Int(n as i128)),
          Value::F64(f) if f.fract() == 0.0 && f.abs() <= SAFE_INTEGER => Ok(Value::Int(f as i128)),
          Value::F32(f) if f.fract() == 0.0 && (f as f64).abs() <= SAFE_INTEGER => Ok(Value::Int(f as i128)),
          Value::Bool(b) => Ok(Value::Int(i128::from(b))),
          Value::Str(s) => s
            .trim()
            .parse::<i128>()
            .map(Value::Int)
            .map_err(|_| Fail::new(FailureKind::Invalid, format!("`{s}` is not an integer"))),
          other => Err(type_error("BigInt", "an integer, an integral number or a string", &other)),
        },
      }
    })
  }

  async fn seq(&mut self, over: &Expr, what: &str) -> Result<Vec<Value>, Fail> {
    match self.eval(over).await? {
      Value::Seq(items) => Ok(items),
      other => Err(type_error(what, "an array", &other)),
    }
  }

  pub(crate) async fn apply(&mut self, f: &Expr, args: Vec<Value>) -> Result<Value, Fail> {
    let Expr::Lambda { params, body } = f else {
      return Err(Fail::internal("a builtin takes a lambda"));
    };
    let depth = self.scope.len();
    for (param, arg) in params.iter().zip(args) {
      self.scope.push((param.clone(), arg));
    }
    let result = self.eval(body).await;
    self.scope.truncate(depth);
    result
  }
}

/// How many statements from the front are `let`s that read none of each
/// other's names. Two or more are evaluated together.
fn independent_lets(stmts: &[Stmt]) -> usize {
  let mut bound: Vec<&str> = Vec::new();
  let mut count = 0;
  for stmt in stmts {
    let Stmt::Let { name, expr } = stmt else { break };
    let mut free = Vec::new();
    expr.free_vars(&mut free);
    if free.iter().any(|f| bound.iter().any(|b| b == f)) {
      break;
    }
    bound.push(name);
    count += 1;
  }
  let calls = stmts[..count].iter().filter(|s| matches!(s, Stmt::Let { expr, .. } if expr.has_call())).count();
  if calls < 2 { 1 } else { count }
}

fn parse_kind(name: &str) -> Option<FailureKind> {
  Some(match name {
    "unauthorized" => FailureKind::Unauthorized,
    "not_found" => FailureKind::NotFound,
    "invalid" => FailureKind::Invalid,
    "conflict" => FailureKind::Conflict,
    "timeout" => FailureKind::Timeout,
    "unavailable" => FailureKind::Unavailable,
    "internal" => FailureKind::Internal,
    _ => return None,
  })
}

pub(crate) fn type_error(what: &str, wanted: &str, got: &Value) -> Fail {
  Fail::internal(format!("{what} wants {wanted}, got {}", kind_name(got)))
}

pub(crate) fn truthy(value: &Value) -> bool {
  match value {
    Value::Null => false,
    Value::Bool(b) => *b,
    Value::Int(n) => *n != 0,
    Value::UInt(n) => *n != 0,
    Value::F32(f) => *f != 0.0,
    Value::F64(f) => *f != 0.0,
    Value::Str(s) => !s.is_empty(),
    _ => true,
  }
}

fn get_field(target: &Value, name: &str) -> Value {
  match target {
    Value::Map(map) => map.get(name).cloned().unwrap_or(Value::Null),
    _ => Value::Null,
  }
}

fn index(target: &Value, key: &Value) -> Result<Value, Fail> {
  match (target, key) {
    (Value::Map(map), Value::Str(k)) => Ok(map.get(k).cloned().unwrap_or(Value::Null)),
    (Value::Seq(items), Value::Int(i)) => Ok(usize::try_from(*i).ok().and_then(|i| items.get(i)).cloned().unwrap_or(Value::Null)),
    (Value::Seq(items), Value::UInt(i)) => Ok(usize::try_from(*i).ok().and_then(|i| items.get(i)).cloned().unwrap_or(Value::Null)),
    (Value::Seq(items), Value::F64(f)) if f.fract() == 0.0 && *f >= 0.0 => Ok(items.get(*f as usize).cloned().unwrap_or(Value::Null)),
    (Value::Null, _) => Ok(Value::Null),
    (Value::Map(_), other) => Err(type_error("indexing an object", "a string key", other)),
    (Value::Seq(_), other) => Err(type_error("indexing an array", "an integer", other)),
    (other, _) => Err(type_error("indexing", "an object or an array", other)),
  }
}

fn arith(op: ArithOp, l: Value, r: Value) -> Result<Value, Fail> {
  match (l, r) {
    (Value::Int(a), Value::Int(b)) => Ok(Value::Int(match op {
      ArithOp::Add => a.checked_add(b).ok_or_else(|| Fail::internal("integer overflow"))?,
      ArithOp::Sub => a.checked_sub(b).ok_or_else(|| Fail::internal("integer overflow"))?,
      ArithOp::Mul => a.checked_mul(b).ok_or_else(|| Fail::internal("integer overflow"))?,
      ArithOp::Div => a.checked_div(b).ok_or_else(|| Fail::internal("division by zero"))?,
      ArithOp::Rem => a.checked_rem(b).ok_or_else(|| Fail::internal("division by zero"))?,
    })),
    (Value::F64(a), Value::F64(b)) => Ok(Value::F64(match op {
      ArithOp::Add => a + b,
      ArithOp::Sub => a - b,
      ArithOp::Mul => a * b,
      ArithOp::Div => a / b,
      ArithOp::Rem => a % b,
    })),
    (Value::Str(a), Value::Str(b)) if op == ArithOp::Add => Ok(Value::Str(a + &b)),
    (l, r) => Err(Fail::internal(format!(
      "{:?} wants two integers, two numbers or two strings, got {} and {}",
      op,
      kind_name(&l),
      kind_name(&r)
    ))),
  }
}

fn compare(op: CompareOp, l: &Value, r: &Value) -> Result<bool, Fail> {
  use std::cmp::Ordering;
  let ordering = match (l, r) {
    (Value::Int(a), Value::Int(b)) => Some(a.cmp(b)),
    (Value::F64(a), Value::F64(b)) => a.partial_cmp(b),
    (Value::Str(a), Value::Str(b)) => Some(a.cmp(b)),
    (Value::Bool(a), Value::Bool(b)) => Some(a.cmp(b)),
    (Value::Null, Value::Null) => Some(Ordering::Equal),
    (Value::Null, _) | (_, Value::Null) => {
      return match op {
        CompareOp::Eq => Ok(false),
        CompareOp::Ne => Ok(true),
        _ => Err(Fail::internal("ordering against null")),
      };
    }
    (l, r) => {
      return Err(Fail::internal(format!(
        "comparing {} with {} is not defined",
        kind_name(l),
        kind_name(r)
      )));
    }
  };
  let Some(ordering) = ordering else {
    return Ok(matches!(op, CompareOp::Ne));
  };
  Ok(match op {
    CompareOp::Eq => ordering == Ordering::Equal,
    CompareOp::Ne => ordering != Ordering::Equal,
    CompareOp::Lt => ordering == Ordering::Less,
    CompareOp::Le => ordering != Ordering::Greater,
    CompareOp::Gt => ordering == Ordering::Greater,
    CompareOp::Ge => ordering != Ordering::Less,
  })
}

/// JavaScript's `Number.MAX_SAFE_INTEGER`; a double past it has no exact
/// integer to promote to.
const SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

fn number(what: &str, value: &Value) -> Result<f64, Fail> {
  match value {
    Value::Int(n) => Ok(*n as f64),
    Value::UInt(n) => Ok(*n as f64),
    Value::F32(f) => Ok(*f as f64),
    Value::F64(f) => Ok(*f),
    other => Err(type_error(what, "a number", other)),
  }
}

fn text<'a>(what: &str, value: &'a Value) -> Result<&'a str, Fail> {
  match value {
    Value::Str(s) => Ok(s),
    other => Err(type_error(what, "a string", other)),
  }
}

/// A JavaScript number, which every builtin produces; integers stay `Int`
/// only when they were `BigInt`s.
fn whole(f: f64) -> Value {
  Value::F64(f)
}

/// One builtin, with JavaScript's semantics for the value model: `round` is
/// half up, `toFixed` rounds half away from zero, `localeNumber` groups by
/// thousands with a comma.
fn builtin(name: Builtin, args: Vec<Value>) -> Result<Value, Fail> {
  let what = format!("{name:?}");
  let arg = |i: usize| args.get(i).ok_or_else(|| Fail::internal(format!("{what} takes more arguments")));
  Ok(match name {
    Builtin::Round => whole((number(&what, arg(0)?)? + 0.5).floor()),
    Builtin::Floor => whole(number(&what, arg(0)?)?.floor()),
    Builtin::Ceil => whole(number(&what, arg(0)?)?.ceil()),
    Builtin::Abs => whole(number(&what, arg(0)?)?.abs()),
    Builtin::Min | Builtin::Max => {
      let mut best: Option<f64> = None;
      for value in &args {
        let n = number(&what, value)?;
        best = Some(match best {
          None => n,
          Some(b) if name == Builtin::Min => b.min(n),
          Some(b) => b.max(n),
        });
      }
      whole(best.ok_or_else(|| Fail::internal(format!("{what} takes a number")))?)
    }
    Builtin::ToFixed => {
      let n = number(&what, arg(0)?)?;
      let digits = args.get(1).map(|d| number(&what, d)).transpose()?.unwrap_or(0.0).max(0.0) as usize;
      let scale = 10f64.powi(digits as i32);
      let rounded = (n.abs() * scale + 0.5).floor() / scale;
      let rounded = if n < 0.0 { -rounded } else { rounded };
      Value::Str(format!("{rounded:.digits$}"))
    }
    Builtin::Repeat => {
      let s = text(&what, arg(0)?)?;
      let n = number(&what, arg(1)?)?.max(0.0) as usize;
      Value::Str(s.repeat(n))
    }
    Builtin::Join => {
      let Value::Seq(items) = arg(0)? else { return Err(type_error(&what, "an array", arg(0)?)) };
      let sep = match args.get(1) {
        Some(v) => text(&what, v)?.to_owned(),
        None => ",".to_owned(),
      };
      let parts: Result<Vec<String>, Fail> = items.iter().map(|v| if matches!(v, Value::Null) { Ok(String::new()) } else { stringify(v) }).collect();
      Value::Str(parts?.join(&sep))
    }
    Builtin::Trim => Value::Str(text(&what, arg(0)?)?.trim().to_owned()),
    Builtin::Upper => Value::Str(text(&what, arg(0)?)?.to_uppercase()),
    Builtin::Lower => Value::Str(text(&what, arg(0)?)?.to_lowercase()),
    Builtin::Includes => match arg(0)? {
      Value::Str(s) => Value::Bool(s.contains(text(&what, arg(1)?)?)),
      Value::Seq(items) => Value::Bool(items.contains(arg(1)?)),
      other => return Err(type_error(&what, "a string or an array", other)),
    },
    Builtin::EncodeUriComponent => {
      let s = stringify(arg(0)?)?;
      let mut out = String::with_capacity(s.len());
      for byte in s.bytes() {
        match byte {
          b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')' => out.push(byte as char),
          _ => out.push_str(&format!("%{byte:02X}")),
        }
      }
      Value::Str(out)
    }
    Builtin::LocaleNumber => {
      let n = number(&what, arg(0)?)?;
      let text = stringify(&whole(n))?;
      let (sign, digits) = text.strip_prefix('-').map(|d| ("-", d)).unwrap_or(("", &text));
      let (int, frac) = digits.split_once('.').map(|(i, f)| (i, Some(f))).unwrap_or((digits, None));
      let mut grouped = String::new();
      for (i, c) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
          grouped.push(',');
        }
        grouped.push(c);
      }
      Value::Str(match frac {
        Some(f) => format!("{sign}{grouped}.{f}"),
        None => format!("{sign}{grouped}"),
      })
    }
    Builtin::Range => {
      let n = number(&what, arg(0)?)?.max(0.0) as i64;
      Value::Seq((0..n).map(|i| Value::F64(i as f64)).collect())
    }
    Builtin::Omit => {
      let mut map = match arg(0)? {
        Value::Map(map) => map.clone(),
        Value::Null => ValueMap::new(),
        other => return Err(type_error(&what, "an object", other)),
      };
      for key in args.iter().skip(1) {
        map.shift_remove(text(&what, key)?);
      }
      Value::Map(map)
    }
  })
}

pub(crate) fn stringify(value: &Value) -> Result<String, Fail> {
  Ok(match value {
    Value::Null => "null".to_owned(),
    Value::Bool(b) => b.to_string(),
    Value::Int(n) => n.to_string(),
    Value::UInt(n) => n.to_string(),
    Value::F64(f) if *f == 0.0 => "0".to_owned(),
    Value::F64(f) => f.to_string(),
    Value::F32(f) if *f == 0.0 => "0".to_owned(),
    Value::F32(f) => (*f as f64).to_string(),
    Value::Str(s) => s.clone(),
    other => return Err(type_error("String", "a scalar", other)),
  })
}

fn set_path(root: &mut Value, steps: &[Value], value: Value) -> Result<(), Fail> {
  let Some((first, rest)) = steps.split_first() else {
    *root = value;
    return Ok(());
  };
  if matches!(root, Value::Null) {
    *root = Value::Map(ValueMap::new());
  }
  match (root, first) {
    (Value::Map(map), Value::Str(key)) => {
      let child = map.entry(key.clone()).or_insert(Value::Null);
      set_path(child, rest, value)
    }
    (Value::Seq(items), Value::Int(i)) => {
      let i = usize::try_from(*i).map_err(|_| Fail::internal("negative index"))?;
      let child = items.get_mut(i).ok_or_else(|| Fail::internal("index past the end"))?;
      set_path(child, rest, value)
    }
    (Value::Seq(items), Value::F64(f)) if f.fract() == 0.0 && *f >= 0.0 => {
      let i = *f as usize;
      let child = items.get_mut(i).ok_or_else(|| Fail::internal("index past the end"))?;
      set_path(child, rest, value)
    }
    (root, key) => Err(type_error("session write", "an object with a string key or an array with an index", if matches!(root, Value::Map(_) | Value::Seq(_)) { key } else { root })),
  }
}

fn delete_path(root: &mut Value, steps: &[Value]) -> Result<(), Fail> {
  let Some((first, rest)) = steps.split_first() else {
    return Ok(());
  };
  match (root, first) {
    (Value::Map(map), Value::Str(key)) => {
      if rest.is_empty() {
        map.shift_remove(key);
        Ok(())
      } else if let Some(child) = map.get_mut(key) {
        delete_path(child, rest)
      } else {
        Ok(())
      }
    }
    (Value::Null, _) => Ok(()),
    (root, key) => Err(type_error("session delete", "an object with a string key", if matches!(root, Value::Map(_)) { key } else { root })),
  }
}
