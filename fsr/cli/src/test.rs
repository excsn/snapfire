//! `fsr test`: every `*.test.ts` under the app, lowered and replayed through
//! the interpreter against the mocked context each test builds. The loader or
//! action under test is lowered the way the build lowers it. The mock's
//! service methods are lambdas behind a transport under the app's contract,
//! which checks both what a mock is asked and what it answers, so a mock that
//! lies about the world fails with the method's name.

use std::collections::HashMap;
use std::fmt::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures_util::future::BoxFuture;
use parking_lot::Mutex;
use snapfire_fsr_core::{Params, Value, ValueMap};
use snapfire_fsr_ir::{Body, Expr, Fail, Interpreter};
use snapfire_fsr_lower::testing::{Assertion, Binding, Mock, Step, Target, TestCase, lower_tests};
use snapfire_fsr_lower::{LowerError, SessionDefaults, lower_actions_with, lower_handlers_with, lower_loader_with, lower_middleware_with};
use snapfire_fsr_runtime::{Identity, RequestCtx, ServiceError, SessionCell};
use snapfire_fsr_service::{Call, Contract, Services, Transport};

use crate::{BuildError, Options, build};

#[derive(Debug, Default)]
pub struct Summary {
  pub passed: usize,
  pub failed: usize,
  pub lines: Vec<String>,
}

impl fmt::Display for Summary {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    for line in &self.lines {
      writeln!(f, "{line}")?;
    }
    writeln!(f, "\ntest result: {}. {} passed; {} failed", if self.failed == 0 { "ok" } else { "FAILED" }, self.passed, self.failed)
  }
}

/// Runs every test file under `app` whose name matches `filter`, when given.
pub fn run(app: &Path, options: &Options, filter: Option<&str>) -> Result<Summary, BuildError> {
  let built = build(app, options)?;
  let contract = Arc::new(built.contract.clone());
  let mut files = Vec::new();
  discover(app, app, &mut files)?;
  let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| BuildError::Dev(format!("runtime: {e}")))?;
  let mut summary = Summary::default();
  for path in files {
    let rel = path.strip_prefix(app).unwrap_or(&path).to_string_lossy().replace('\\', "/");
    let source = std::fs::read_to_string(&path).map_err(|e| BuildError::Io(path.clone(), e))?;
    let file = lower_tests(&rel, &source)?;
    let mut targets = Targets { app: app.to_path_buf(), defaults: built.defaults.clone(), loaders: HashMap::new(), actions: HashMap::new() };
    for case in &file.tests {
      if filter.is_some_and(|f| !case.name.contains(f) && !rel.contains(f)) {
        continue;
      }
      let outcome = runtime.block_on(run_case(case, &mut targets, &contract));
      match outcome {
        Ok(()) => {
          summary.passed += 1;
          summary.lines.push(format!("test {rel}: {} ... ok", case.name));
        }
        Err(failure) => {
          summary.failed += 1;
          summary.lines.push(format!("test {rel}: {} ... FAILED\n{failure}", case.name));
        }
      }
    }
  }
  crate::spec::run(app, &built, &contract, filter, &runtime, &mut summary)?;
  Ok(summary)
}

fn discover(app: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), BuildError> {
  let mut entries: Vec<PathBuf> = std::fs::read_dir(dir).map_err(|e| BuildError::Io(dir.to_path_buf(), e))?.flatten().map(|e| e.path()).collect();
  entries.sort();
  for path in entries {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if path.is_dir() {
      if path.parent() == Some(app) && ["generated", "dist", "types", "vendor", "node_modules"].contains(&name.as_str()) {
        continue;
      }
      discover(app, &path, out)?;
    } else if name.ends_with(".test.ts") {
      out.push(path);
    }
  }
  Ok(())
}

/// The bodies a file's tests run, lowered once each.
struct Targets {
  app: PathBuf,
  defaults: SessionDefaults,
  loaders: HashMap<String, Arc<Body>>,
  actions: HashMap<String, Arc<Body>>,
}

impl Targets {
  fn body(&mut self, target: &Target) -> Result<Arc<Body>, String> {
    match target {
      Target::Loader { file } => {
        if let Some(body) = self.loaders.get(file) {
          return Ok(body.clone());
        }
        let source = std::fs::read_to_string(self.app.join(file)).map_err(|e| format!("{file}: {e}"))?;
        let body = Arc::new(lower_loader_with(file, &source, &self.defaults).map_err(|e| e.to_string())?);
        self.loaders.insert(file.clone(), body.clone());
        Ok(body)
      }
      Target::Middleware { file } => {
        if let Some(body) = self.loaders.get(file) {
          return Ok(body.clone());
        }
        let source = std::fs::read_to_string(self.app.join(file)).map_err(|e| format!("{file}: {e}"))?;
        let body = Arc::new(lower_middleware_with(file, &source, &self.defaults).map_err(|e| e.to_string())?);
        self.loaders.insert(file.clone(), body.clone());
        Ok(body)
      }
      Target::Handler { file, export } => {
        let key = format!("{file}#{export}");
        if let Some(body) = self.actions.get(&key) {
          return Ok(body.clone());
        }
        let source = std::fs::read_to_string(self.app.join(file)).map_err(|e| format!("{file}: {e}"))?;
        let lowered = lower_handlers_with(file, &source, &self.defaults).map_err(|e| e.to_string())?;
        for handler in lowered {
          self.actions.insert(format!("{file}#{}", handler.method), Arc::new(handler.body));
        }
        self.actions.get(&key).cloned().ok_or_else(|| LowerError::MissingExport { file: file.clone(), export: export.clone() }.to_string())
      }
      Target::Action { file, export } => {
        let key = format!("{file}#{export}");
        if let Some(body) = self.actions.get(&key) {
          return Ok(body.clone());
        }
        let source = std::fs::read_to_string(self.app.join(file)).map_err(|e| format!("{file}: {e}"))?;
        let lowered = lower_actions_with(file, &source, &self.defaults).map_err(|e| e.to_string())?;
        for action in lowered {
          self.actions.insert(format!("{file}#{}", action.export), Arc::new(action.body));
        }
        self.actions.get(&key).cloned().ok_or_else(|| LowerError::MissingExport { file: file.clone(), export: export.clone() }.to_string())
      }
    }
  }
}

/// A mocked service layer: each method a lambda, every call recorded.
struct LambdaTransport {
  methods: HashMap<String, Expr>,
  calls: Mutex<Vec<Value>>,
  interpreter: Interpreter,
}

impl Transport for LambdaTransport {
  fn call(&self, call: Call) -> BoxFuture<'static, Result<Value, ServiceError>> {
    let path = format!("{}.{}", call.service, call.method);
    let mut record = ValueMap::new();
    record.insert("service".to_owned(), Value::Str(call.service.clone()));
    record.insert("method".to_owned(), Value::Str(call.method.clone()));
    record.insert("args".to_owned(), Value::Map(call.args.clone()));
    self.calls.lock().push(Value::Map(record));
    let Some(lambda) = self.methods.get(&path).cloned() else {
      let error = ServiceError::new(snapfire_fsr_runtime::FailureKind::Unavailable, &call.service, &call.method, format!("the test mocks no `{path}`"));
      return Box::pin(async move { Err(error) });
    };
    let interpreter = self.interpreter.clone();
    let args = Value::Map(call.args);
    let (service, method) = (call.service, call.method);
    Box::pin(async move {
      interpreter.apply(&lambda, vec![args]).await.map_err(|fail| ServiceError::new(fail.kind, &service, &method, format!("the mock failed: {}", fail.message)))
    })
  }
}

/// One `ctx(...)`: the request it stands for and what the test reads back.
struct MockCtx {
  ctx: RequestCtx,
  input: Option<Value>,
  transport: Arc<LambdaTransport>,
  written: Vec<String>,
}

impl MockCtx {
  /// `c` as the test's expressions see it, refreshed after every run.
  fn value(&self) -> Value {
    let (session, _) = self.ctx.session.snapshot();
    let mut map = ValueMap::new();
    map.insert("session".to_owned(), Value::Map(session));
    map.insert("params".to_owned(), Value::Map(self.ctx.params.iter().map(|(k, v)| (k.clone(), Value::Str(v.clone()))).collect()));
    map.insert("query".to_owned(), Value::Map(self.ctx.query.iter().map(|(k, v)| (k.clone(), Value::Str(v.clone()))).collect()));
    map.insert("input".to_owned(), self.input.clone().unwrap_or(Value::Null));
    let mut trace = ValueMap::new();
    trace.insert("calls".to_owned(), Value::Seq(self.transport.calls.lock().clone()));
    let mut session_trace = ValueMap::new();
    session_trace.insert("written".to_owned(), Value::Seq(self.written.iter().cloned().map(Value::Str).collect()));
    trace.insert("session".to_owned(), Value::Map(session_trace));
    map.insert("trace".to_owned(), Value::Map(trace));
    Value::Map(map)
  }
}

struct Run<'a> {
  interpreter: Interpreter,
  contract: &'a Arc<Contract>,
  scope: Vec<(String, Value)>,
  mocks: HashMap<String, MockCtx>,
}

impl Run<'_> {
  async fn eval(&self, expr: &Expr) -> Result<Value, Fail> {
    self.interpreter.evaluate(expr, self.scope.clone()).await
  }

  fn bind(&mut self, name: &str, value: Value) {
    if let Some(slot) = self.scope.iter_mut().find(|(n, _)| n == name) {
      slot.1 = value;
    } else {
      self.scope.push((name.to_owned(), value));
    }
  }

  async fn mock(&mut self, name: &str, mock: &Mock) -> Result<(), String> {
    let mut session = ValueMap::new();
    for (key, expr) in &mock.session {
      session.insert(key.clone(), self.eval(expr).await.map_err(|f| format!("session.{key}: {}", f.message))?);
    }
    let identity = match &mock.identity {
      Some(expr) => match self.eval(expr).await.map_err(|f| format!("identity: {}", f.message))? {
        Value::Map(map) => {
          let subject = match map.get("subject") {
            Some(Value::Str(s)) => s.clone(),
            _ => return Err("identity.subject must be a string".to_owned()),
          };
          let claims = match map.get("claims") {
            Some(Value::Map(claims)) => claims.clone(),
            None => ValueMap::new(),
            _ => return Err("identity.claims must be an object".to_owned()),
          };
          Some(Identity { subject, claims })
        }
        _ => return Err("identity must be an object".to_owned()),
      },
      None => None,
    };
    let mut methods = HashMap::new();
    for (service, method, lambda) in &mock.services {
      methods.insert(format!("{service}.{method}"), lambda.clone());
    }
    let transport = Arc::new(LambdaTransport { methods, calls: Mutex::new(Vec::new()), interpreter: self.interpreter.clone() });
    let services = Services::builder().contract((**self.contract).clone()).default_transport(transport.clone()).build();
    let handle = services.bind(identity.clone(), Arc::new(snapfire_fsr_service::NoCredentials));
    let params = self.params(&mock.params, "params").await?;
    let query = self.params(&mock.query, "query").await?;
    let input = match &mock.input {
      Some(expr) => Some(self.eval(expr).await.map_err(|f| format!("input: {}", f.message))?),
      None => None,
    };
    let ctx = RequestCtx { params, query, session: SessionCell::new(session, identity), csrf: None, services: handle };
    let mock = MockCtx { ctx, input, transport, written: Vec::new() };
    self.bind(name, mock.value());
    self.mocks.insert(name.to_owned(), mock);
    Ok(())
  }

  async fn params(&self, entries: &[(String, Expr)], what: &str) -> Result<Params, String> {
    let mut out = Params::new();
    for (key, expr) in entries {
      match self.eval(expr).await.map_err(|f| format!("{what}.{key}: {}", f.message))? {
        Value::Str(s) => {
          out.insert(key.clone(), s);
        }
        other => return Err(format!("{what}.{key} must be a string, got {}", show(&other))),
      }
    }
    Ok(out)
  }

  async fn run(&mut self, body: &Body, ctx_name: &str) -> Result<Result<Value, Fail>, String> {
    let mock = self.mocks.get_mut(ctx_name).ok_or_else(|| format!("`{ctx_name}` is not a ctx"))?;
    let outcome = self.interpreter.run(body, &mock.ctx, mock.input.clone()).await;
    let result = match outcome {
      Ok(outcome) => {
        mock.written = outcome.written;
        Ok(outcome.value)
      }
      Err(fail) => Err(fail),
    };
    let value = mock.value();
    self.bind(ctx_name, value);
    Ok(result)
  }
}

async fn run_case(case: &TestCase, targets: &mut Targets, contract: &Arc<Contract>) -> Result<(), String> {
  let mut run = Run { interpreter: Interpreter::default(), contract, scope: Vec::new(), mocks: HashMap::new() };
  for (line, step) in &case.steps {
    let at = |message: String| format!("  line {line}: {message}");
    match step {
      Step::Mock { name, mock } => run.mock(name, mock).await.map_err(at)?,
      Step::Run { binding, target, ctx } => {
        let body = targets.body(target).map_err(at)?;
        let value = match run.run(&body, ctx).await.map_err(at)? {
          Ok(value) => value,
          Err(fail) => return Err(at(format!("{} failed: {}: {}", describe(target), fail.kind.as_str(), fail.message))),
        };
        match binding {
          Some(Binding::Name(name)) => run.bind(name, value),
          Some(Binding::Fields(fields)) => {
            let Value::Map(map) = value else { return Err(at(format!("{} returned {}, not an object to destructure", describe(target), show(&value)))) };
            for (field, local) in fields {
              run.bind(local, map.get(field).cloned().unwrap_or(Value::Null));
            }
          }
          None => {}
        }
      }
      Step::Assert(Assertion::Ok(expr)) => {
        let value = run.eval(expr).await.map_err(|f| at(f.message))?;
        if !truthy(&value) {
          return Err(at(format!("assert.ok: {}", show(&value))));
        }
      }
      Step::Assert(Assertion::Equal(left, right)) => {
        let actual = run.eval(left).await.map_err(|f| at(f.message))?;
        let expected = run.eval(right).await.map_err(|f| at(f.message))?;
        if actual != expected {
          return Err(at(format!("assert.equal\n    actual:   {}\n    expected: {}", show(&actual), show(&expected))));
        }
      }
      Step::Assert(Assertion::Rejects { target, ctx, kind }) => {
        let body = targets.body(target).map_err(at)?;
        match run.run(&body, ctx).await.map_err(at)? {
          Ok(value) => return Err(at(format!("assert.rejects: {} returned {}", describe(target), show(&value)))),
          Err(fail) => {
            if let Some(kind) = kind {
              if fail.kind.as_str() != kind {
                return Err(at(format!("assert.rejects: {} failed with `{}`, not `{kind}`: {}", describe(target), fail.kind.as_str(), fail.message)));
              }
            }
          }
        }
      }
    }
  }
  Ok(())
}

fn describe(target: &Target) -> String {
  match target {
    Target::Loader { .. } => "`load`".to_owned(),
    Target::Middleware { .. } => "`middleware`".to_owned(),
    Target::Action { export, .. } | Target::Handler { export, .. } => format!("`{export}`"),
  }
}

fn truthy(value: &Value) -> bool {
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

/// A value as TypeScript would write it, so `1n` and `1` read as different.
pub fn show(value: &Value) -> String {
  let mut out = String::new();
  write_value(value, &mut out);
  out
}

fn write_value(value: &Value, out: &mut String) {
  match value {
    Value::Null => out.push_str("null"),
    Value::Bool(b) => {
      let _ = write!(out, "{b}");
    }
    Value::Int(n) => {
      let _ = write!(out, "{n}n");
    }
    Value::UInt(n) => {
      let _ = write!(out, "{n}n");
    }
    Value::F32(f) => {
      let _ = write!(out, "{f}");
    }
    Value::F64(f) => {
      let _ = write!(out, "{f}");
    }
    Value::Str(s) => {
      let _ = write!(out, "{s:?}");
    }
    Value::Bytes(b) => {
      let _ = write!(out, "<{} bytes>", b.len());
    }
    Value::TypedArray(_) => out.push_str("<typed array>"),
    Value::Seq(items) => {
      out.push('[');
      for (i, item) in items.iter().enumerate() {
        if i > 0 {
          out.push_str(", ");
        }
        write_value(item, out);
      }
      out.push(']');
    }
    Value::Map(map) => {
      if map.is_empty() {
        out.push_str("{}");
        return;
      }
      out.push_str("{ ");
      for (i, (k, v)) in map.iter().enumerate() {
        if i > 0 {
          out.push_str(", ");
        }
        let _ = write!(out, "{k:?}: ");
        write_value(v, out);
      }
      out.push_str(" }");
    }
    Value::Variant { tag, payload } => {
      let _ = write!(out, "{tag}");
      if let Some(payload) = payload {
        out.push('(');
        write_value(payload, out);
        out.push(')');
      }
    }
    Value::Ref { kind, id } => {
      let _ = write!(out, "<{kind:?} {id}>");
    }
  }
}
