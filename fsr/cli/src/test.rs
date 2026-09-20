//! `fsr test`: every `*.test.ts` under the app, lowered and replayed through
//! the interpreter against the mocked context each test builds. The loader or
//! action under test is lowered the way the build lowers it. A mock's service
//! methods are lambdas or mock functions behind a transport under the app's
//! contract, which checks both what a mock is asked and what it answers, so a
//! mock that lies about the world fails with the method's name.

use std::collections::{HashMap, VecDeque};
use std::fmt::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures_util::future::BoxFuture;
use parking_lot::Mutex;
use snapfire_fsr_core::{Params, Value, ValueMap};
use snapfire_fsr_ir::ast::Consts;
use snapfire_fsr_ir::{Body, Expr, Fail, Interpreter};
use snapfire_fsr_lower::testing::{lower_tests, Answer, Assertion, Binding, Matcher, Mock, Mode, Pattern, Step, Subject, Target, TestCase, TestFile, EXPECT_MARK};
use snapfire_fsr_plan::Manifest;
use snapfire_fsr_runtime::{FailureKind, Identity, RequestCtx, ServiceError, SessionCell};
use snapfire_fsr_service::{Call, Contract, Services, Transport};

use crate::{BuildError, Options, build};

/// How one test ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
  Passed,
  Failed(String),
  /// `skip` or left out by an `only` elsewhere in its file.
  Skipped,
  Todo,
}

#[derive(Debug, Default)]
pub struct Summary {
  pub passed: usize,
  pub failed: usize,
  pub skipped: usize,
  pub todo: usize,
  pub lines: Vec<String>,
}

impl Summary {
  /// Counts `outcome` and writes its line.
  pub fn record(&mut self, file: &str, name: &str, outcome: Outcome) {
    match outcome {
      Outcome::Passed => {
        self.passed += 1;
        self.lines.push(format!("test {file}: {name} ... ok"));
      }
      Outcome::Failed(failure) => {
        self.failed += 1;
        self.lines.push(format!("test {file}: {name} ... FAILED\n{failure}"));
      }
      Outcome::Skipped => {
        self.skipped += 1;
        self.lines.push(format!("test {file}: {name} ... skipped"));
      }
      Outcome::Todo => {
        self.todo += 1;
        self.lines.push(format!("test {file}: {name} ... todo"));
      }
    }
  }
}

impl fmt::Display for Summary {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    for line in &self.lines {
      writeln!(f, "{line}")?;
    }
    write!(f, "\ntest result: {}. {} passed; {} failed", if self.failed == 0 { "ok" } else { "FAILED" }, self.passed, self.failed)?;
    if self.skipped > 0 {
      write!(f, "; {} skipped", self.skipped)?;
    }
    if self.todo > 0 {
      write!(f, "; {} todo", self.todo)?;
    }
    writeln!(f)
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
    let mut targets = Targets {
      manifest: Arc::new(built.manifest.clone()),
      prefix: options.site.as_ref().map(|site| site.prefix()).unwrap_or_default(),
      consts: Some(Arc::new(built.manifest.consts.clone())),
      loaders: HashMap::new(),
      actions: HashMap::new(),
    };
    let mut entered: Vec<Entered<'_>> = Vec::new();
    let chosen = |name: &str| !filter.is_some_and(|f| !name.contains(f) && !rel.contains(f));
    for case in &file.tests {
      let written = file.full_name(case);
      match case.mode {
        Mode::Skip | Mode::Todo => {
          if chosen(&written) {
            summary.record(&rel, &written, if case.mode == Mode::Skip { Outcome::Skipped } else { Outcome::Todo });
          }
          continue;
        }
        Mode::Run => {}
      }
      let expansions = match runtime.block_on(expansions(&file, case, &contract, &targets.consts)) {
        Ok(expansions) => expansions,
        Err(failure) => {
          if chosen(&written) {
            summary.record(&rel, &written, Outcome::Failed(failure));
          }
          continue;
        }
      };
      for expansion in expansions {
        if !chosen(&expansion.name) {
          continue;
        }
        let outcome = runtime.block_on(run_case(&file, case, &expansion, &mut targets, &contract, &mut entered));
        summary.record(&rel, &expansion.name, outcome);
      }
    }
    for (name, failure) in runtime.block_on(finish(&file, &mut targets, &entered)) {
      summary.record(&rel, &name, Outcome::Failed(failure));
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

/// The bodies a file's tests run: the ones the build lowered, so an import the
/// build follows is followed here too. A site's manifest carries its prefix on
/// every module, which the lookup strips.
struct Targets {
  manifest: Arc<Manifest>,
  prefix: String,
  consts: Option<Arc<Consts>>,
  loaders: HashMap<String, Arc<Body>>,
  actions: HashMap<String, Arc<Body>>,
}

impl Targets {
  fn module_is(&self, module: Option<&str>, file: &str) -> bool {
    module.map(|m| m.strip_prefix(&self.prefix).unwrap_or(m)) == Some(file)
  }

  fn source(&self, file: &str) -> Result<&snapfire_fsr_plan::SourceEntry, String> {
    self.manifest.sources.iter().find(|s| self.module_is(s.module.as_deref(), file)).ok_or_else(|| format!("{file}: not a loader the build lowered"))
  }

  fn body(&mut self, target: &Target) -> Result<Arc<Body>, String> {
    match target {
      Target::Loader { file } => {
        if let Some(body) = self.loaders.get(file) {
          return Ok(body.clone());
        }
        let body = Arc::new(self.source(file)?.body.clone().ok_or_else(|| format!("{file}: the build did not lower `load`"))?);
        self.loaders.insert(file.clone(), body.clone());
        Ok(body)
      }
      Target::Meta { file } | Target::Store { file } | Target::Paths { file } => {
        let export = match target {
          Target::Store { .. } => "store",
          Target::Paths { .. } => "paths",
          _ => "meta",
        };
        let key = format!("{file}#{export}");
        if let Some(body) = self.loaders.get(&key) {
          return Ok(body.clone());
        }
        let source = self.source(file)?;
        let lowered = match export {
          "store" => source.store.clone(),
          "paths" => source.paths.clone(),
          _ => source.meta.clone(),
        };
        let body = Arc::new(lowered.ok_or_else(|| format!("{file} exports no `{export}`"))?);
        self.loaders.insert(key, body.clone());
        Ok(body)
      }
      Target::Middleware { file } => {
        if let Some(body) = self.loaders.get(file) {
          return Ok(body.clone());
        }
        let body = Arc::new(self.manifest.middleware.clone().ok_or_else(|| format!("{file}: the build did not lower `middleware`"))?);
        self.loaders.insert(file.clone(), body.clone());
        Ok(body)
      }
      Target::Handler { file, export } => {
        let key = format!("{file}#{export}");
        if let Some(body) = self.actions.get(&key) {
          return Ok(body.clone());
        }
        let found = self.manifest.handlers.iter().find(|h| self.module_is(h.module.as_deref(), file) && h.method.eq_ignore_ascii_case(export));
        let body = Arc::new(found.and_then(|h| h.body.clone()).ok_or_else(|| format!("{file} exports no `{export}` the build lowered"))?);
        self.actions.insert(key, body.clone());
        Ok(body)
      }
      Target::Action { file, export } => {
        let key = format!("{file}#{export}");
        if let Some(body) = self.actions.get(&key) {
          return Ok(body.clone());
        }
        let found = self.manifest.actions.iter().find(|a| self.module_is(a.module.as_deref(), file) && a.export.as_deref() == Some(export));
        let body = Arc::new(found.and_then(|a| a.body.clone()).ok_or_else(|| format!("{file} exports no `{export}` the build lowered"))?);
        self.actions.insert(key, body.clone());
        Ok(body)
      }
    }
  }
}

/// What a mock function answers, resolved when the answer was given.
#[derive(Debug, Clone)]
enum Resolved {
  Returns(Value),
  Calls(Expr),
  Fails(Value),
}

/// A mock function: what it answers and what it has been called with.
#[derive(Debug, Default)]
struct MockFn {
  answer: Option<Resolved>,
  once: VecDeque<Resolved>,
  calls: Vec<Value>,
  results: Vec<Result<Value, String>>,
}

/// Every mock function a run knows, by name, shared with the transports that answer through them.
type Fns = Arc<Mutex<HashMap<String, MockFn>>>;

enum Method {
  Lambda(Expr),
  Fn(String),
}

/// A mocked service layer: each method a lambda or a mock function, every call recorded.
struct LambdaTransport {
  methods: HashMap<String, Method>,
  calls: Mutex<Vec<Value>>,
  interpreter: Interpreter,
  fns: Fns,
}

impl Transport for LambdaTransport {
  fn call(&self, call: Call) -> BoxFuture<'static, Result<Value, ServiceError>> {
    let mut call = call;
    call.service = crate::unprefixed(&call.service).to_owned();
    let path = format!("{}.{}", call.service, call.method);
    let mut record = ValueMap::default();
    record.insert("service".to_owned(), Value::str(call.service.clone()));
    record.insert("method".to_owned(), Value::str(call.method.clone()));
    record.insert("args".to_owned(), Value::Map(call.args.clone()));
    self.calls.lock().push(Value::Map(record));
    let interpreter = self.interpreter.clone();
    let args = Value::Map(call.args);
    let (service, method) = (call.service, call.method);
    match self.methods.get(&path) {
      None => {
        let error = ServiceError::new(FailureKind::Unavailable, &service, &method, format!("the test mocks no `{path}`"));
        Box::pin(async move { Err(error) })
      }
      Some(Method::Lambda(lambda)) => {
        let lambda = lambda.clone();
        Box::pin(async move { interpreter.apply(&lambda, vec![args]).await.map_err(|fail| ServiceError::new(fail.kind, &service, &method, format!("the mock failed: {}", fail.message))) })
      }
      Some(Method::Fn(name)) => {
        let fns = self.fns.clone();
        let name = name.clone();
        let answer = {
          let mut table = fns.lock();
          table.get_mut(&name).map(|mock| {
            mock.calls.push(args.clone());
            mock.once.pop_front().or_else(|| mock.answer.clone())
          })
        };
        Box::pin(async move {
          let Some(answer) = answer else {
            return Err(ServiceError::new(FailureKind::Unavailable, &service, &method, format!("`{name}` is not a mock function the test declared")));
          };
          let result = match answer {
            None => Ok(Value::Null),
            Some(Resolved::Returns(value)) => Ok(value),
            Some(Resolved::Calls(lambda)) => interpreter.apply(&lambda, vec![args]).await.map_err(|fail| ServiceError::new(fail.kind, &service, &method, format!("the mock failed: {}", fail.message))),
            Some(Resolved::Fails(value)) => Err(failure_of(&value, &service, &method)),
          };
          if let Some(mock) = fns.lock().get_mut(&name) {
            mock.results.push(result.as_ref().map(Clone::clone).map_err(|e| e.to_string()));
          }
          result
        })
      }
    }
  }
}

/// The service failure `mockRejectedValue(value)` stands for: `{ kind, message }` names its kind; anything else is an internal failure saying what it was.
fn failure_of(value: &Value, service: &str, method: &str) -> ServiceError {
  let (kind, message) = match value {
    Value::Map(map) => (map.get("kind").and_then(text).and_then(|k| kind_of(&k)), map.get("message").and_then(text).unwrap_or_else(|| show(value))),
    Value::Str(message) => (None, message.to_string()),
    other => (None, show(other)),
  };
  ServiceError::new(kind.unwrap_or(FailureKind::Internal), service, method, message)
}

fn kind_of(kind: &str) -> Option<FailureKind> {
  Some(match kind {
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

/// One `ctx(...)`: the request it stands for and what the test reads back.
#[derive(Clone)]
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
    let mut map = ValueMap::default();
    map.insert("session".to_owned(), Value::Map(session));
    map.insert("params".to_owned(), Value::Map(self.ctx.params.iter().map(|(k, v)| (k.clone(), Value::str(v.clone()))).collect()));
    map.insert("query".to_owned(), Value::Map(self.ctx.query.iter().map(|(k, v)| (k.clone(), Value::str(v.clone()))).collect()));
    map.insert("address".to_owned(), self.ctx.address.as_ref().map(snapfire_fsr_runtime::Address::value).unwrap_or(Value::Null));
    map.insert("address".to_owned(), self.ctx.address.as_ref().map(snapfire_fsr_runtime::Address::value).unwrap_or(Value::Null));
    map.insert("input".to_owned(), self.input.clone().unwrap_or(Value::Null));
    let mut trace = ValueMap::default();
    trace.insert("calls".to_owned(), Value::seq(self.transport.calls.lock().clone()));
    let mut session_trace = ValueMap::default();
    session_trace.insert("written".to_owned(), Value::Seq(self.written.iter().map(Value::str).collect()));
    trace.insert("session".to_owned(), Value::Map(session_trace));
    map.insert("trace".to_owned(), Value::Map(trace));
    Value::Map(map)
  }
}

/// What a test has bound so far: its values, its contexts and its mock functions.
#[derive(Clone)]
struct Run<'a> {
  interpreter: Interpreter,
  contract: &'a Arc<Contract>,
  scope: Vec<(String, Value)>,
  mocks: HashMap<String, MockCtx>,
  fns: Fns,
}

impl<'a> Run<'a> {
  fn new(contract: &'a Arc<Contract>, consts: &Option<Arc<Consts>>) -> Self {
    Self { interpreter: Interpreter::default().with_consts(consts.clone()), contract, scope: Vec::new(), mocks: HashMap::new(), fns: Arc::new(Mutex::new(HashMap::new())) }
  }

  async fn eval(&self, expr: &Expr) -> Result<Value, Fail> {
    self.interpreter.evaluate(expr, self.scope.clone()).await
  }

  async fn value(&self, expr: &Expr) -> Result<Value, String> {
    self.eval(expr).await.map_err(|f| f.message)
  }

  fn bind(&mut self, name: &str, value: Value) {
    if let Some(slot) = self.scope.iter_mut().find(|(n, _)| n == name) {
      slot.1 = value;
    } else {
      self.scope.push((name.to_owned(), value));
    }
  }

  async fn resolve(&self, answer: &Answer) -> Result<Resolved, String> {
    Ok(match answer {
      Answer::Returns(expr) => Resolved::Returns(self.value(expr).await?),
      Answer::Calls(lambda) => Resolved::Calls(lambda.clone()),
      Answer::Fails(expr) => Resolved::Fails(self.value(expr).await?),
    })
  }

  async fn mock(&mut self, name: &str, mock: &Mock) -> Result<(), String> {
    let mut session = ValueMap::default();
    for (key, expr) in &mock.session {
      session.insert(key.clone(), self.eval(expr).await.map_err(|f| format!("session.{key}: {}", f.message))?);
    }
    let identity = match &mock.identity {
      Some(expr) => match self.eval(expr).await.map_err(|f| format!("identity: {}", f.message))? {
        Value::Map(map) => {
          let subject = match map.get("subject") {
            Some(Value::Str(s)) => s.to_string(),
            _ => return Err("identity.subject must be a string".to_owned()),
          };
          let claims = match map.get("claims") {
            Some(Value::Map(claims)) => claims.clone(),
            None => ValueMap::default(),
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
      methods.insert(format!("{service}.{method}"), Method::Lambda(lambda.clone()));
    }
    for (service, method, fn_name) in &mock.mock_fns {
      methods.insert(format!("{service}.{method}"), Method::Fn(fn_name.clone()));
    }
    let transport = Arc::new(LambdaTransport { methods, calls: Mutex::new(Vec::new()), interpreter: self.interpreter.clone(), fns: self.fns.clone() });
    let services = Services::builder().contract((**self.contract).clone()).default_transport(transport.clone()).build();
    let handle = services.bind(identity.clone(), Arc::new(snapfire_fsr_service::NoCredentials));
    let params = self.params(&mock.params, "params").await?;
    let query = self.params(&mock.query, "query").await?;
    let input = match &mock.input {
      Some(expr) => Some(self.eval(expr).await.map_err(|f| format!("input: {}", f.message))?),
      None => None,
    };
    let locale = match &mock.locale {
      Some(expr) => match self.eval(expr).await.map_err(|f| format!("locale: {}", f.message))? {
        Value::Str(tag) => snapfire_fsr_runtime::Locale::new(tag, false),
        other => return Err(format!("locale must be a string, got {}", show(&other))),
      },
      None => snapfire_fsr_runtime::Locale::new("en", true),
    };
    let path = match &mock.path {
      Some(expr) => match self.eval(expr).await.map_err(|f| format!("path: {}", f.message))? {
        Value::Str(path) => path.to_string(),
        other => return Err(format!("path must be a string, got {}", show(&other))),
      },
      None => String::new(),
    };
    let host = match &mock.host {
      Some(expr) => match self.eval(expr).await.map_err(|f| format!("host: {}", f.message))? {
        Value::Str(host) => Some(host.to_string()),
        Value::Null => None,
        other => return Err(format!("host must be a string, got {}", show(&other))),
      },
      None => None,
    };
    let mut config = ValueMap::default();
    for (key, expr) in &mock.config {
      config.insert(key.clone(), self.eval(expr).await.map_err(|f| format!("config.{key}: {}", f.message))?);
    }
    let ctx = RequestCtx { params, query, path, document: None, address: None, session: SessionCell::new(session, identity), locale, host, config, csrf: snapfire_fsr_runtime::CsrfHandle::default(), services: handle, natives: Default::default() };
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
          out.insert(key.to_string(), s.to_string());
        }
        other => return Err(format!("{what}.{key} must be a string, got {}", show(&other))),
      }
    }
    Ok(out)
  }

  async fn run(&mut self, body: &Body, ctx_name: &str, input: Option<Value>) -> Result<Result<Value, Fail>, String> {
    let mock = self.mocks.get_mut(ctx_name).ok_or_else(|| format!("`{ctx_name}` is not a ctx"))?;
    let input = input.or_else(|| mock.input.clone());
    let outcome = self.interpreter.run(body, &mock.ctx, input).await;
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

/// A block's state after its `beforeAll` hooks, for one row of each `each`
/// around it: what every test under it starts from.
struct Entered<'c> {
  block: usize,
  rows: Vec<Option<usize>>,
  run: Run<'c>,
  failure: Option<String>,
}

/// One run of a test: a row of every `each` table around and on it, what
/// those rows bind at each level (the file's first) and the name it reads as.
struct Expansion {
  name: String,
  rows: Vec<Option<usize>>,
  bindings: Vec<Vec<(String, Value)>>,
}

async fn expansions(file: &TestFile, case: &TestCase, contract: &Arc<Contract>, consts: &Option<Arc<Consts>>) -> Result<Vec<Expansion>, String> {
  let chain = file.chain(case.block);
  let run = Run::new(contract, consts);
  let mut combos = vec![Expansion { name: String::new(), rows: Vec::new(), bindings: Vec::new() }];
  for level in 0..=chain.len() {
    let (label, each) = match chain.get(level) {
      Some(&block) => (file.blocks[block].name.as_str(), file.blocks[block].each.as_ref()),
      None => (case.name.as_str(), case.each.as_ref()),
    };
    let options: Vec<(Option<usize>, Vec<Value>, Vec<(String, Value)>)> = match each {
      None => vec![(None, Vec::new(), Vec::new())],
      Some(each) => {
        let table = run.value(&each.table).await.map_err(|m| format!("  the table of `{label}`: {m}"))?;
        let Value::Seq(rows) = &table else {
          return Err(format!("  the table of `{label}` is {}, not an array of rows", show(&table)));
        };
        if rows.is_empty() {
          return Err(format!("  the table of `{label}` has no rows"));
        }
        let mut out = Vec::new();
        for (i, row) in rows.iter().enumerate() {
          let args = args_of(row);
          let bindings = bind_row(&each.params, &args).map_err(|m| format!("  row {i} of `{label}`: {m}"))?;
          out.push((Some(i), args, bindings));
        }
        out
      }
    };
    let mut next = Vec::new();
    for combo in &combos {
      for (row, args, bindings) in &options {
        let mut name = combo.name.clone();
        if !label.is_empty() {
          if !name.is_empty() {
            name.push_str(" > ");
          }
          name.push_str(&match row {
            Some(i) => titled(label, args, *i),
            None => label.to_owned(),
          });
        }
        let mut rows = combo.rows.clone();
        rows.push(*row);
        let mut all = combo.bindings.clone();
        all.push(bindings.clone());
        next.push(Expansion { name, rows, bindings: all });
      }
    }
    combos = next;
  }
  Ok(combos)
}

fn args_of(row: &Value) -> Vec<Value> {
  match row {
    Value::Seq(items) => items.iter().cloned().collect(),
    other => vec![other.clone()],
  }
}

fn bind_row(params: &[Binding], args: &[Value]) -> Result<Vec<(String, Value)>, String> {
  let mut out = Vec::new();
  for (i, param) in params.iter().enumerate() {
    let value = args.get(i).cloned().unwrap_or(Value::Null);
    match param {
      Binding::Name(name) => out.push((name.clone(), value)),
      Binding::Fields(fields) => {
        let Value::Map(map) = &value else {
          return Err(format!("{} is not an object to destructure", show(&value)));
        };
        for (field, local) in fields {
          out.push((local.clone(), map.get(field).cloned().unwrap_or(Value::Null)));
        }
      }
    }
  }
  Ok(out)
}

/// An `each` case's name: printf placeholders take the arguments in order,
/// `%#` is the row's index, `%$` its number and `$name` a field of a row
/// that is one object.
fn titled(name: &str, args: &[Value], index: usize) -> String {
  let mut out = String::new();
  let mut next = 0;
  let mut chars = name.chars().peekable();
  while let Some(c) = chars.next() {
    if c != '%' {
      out.push(c);
      continue;
    }
    match chars.peek().copied() {
      Some('%') => {
        chars.next();
        out.push('%');
      }
      Some('#') => {
        chars.next();
        out.push_str(&index.to_string());
      }
      Some('$') => {
        chars.next();
        out.push_str(&(index + 1).to_string());
      }
      Some(flag @ ('s' | 'd' | 'i' | 'f' | 'j' | 'o' | 'O' | 'p')) if next < args.len() => {
        chars.next();
        let value = &args[next];
        next += 1;
        out.push_str(&match (flag, value) {
          ('s', Value::Str(s)) => s.to_string(),
          ('d' | 'i', v) => number(v).map(|n| format!("{}", n.trunc() as i128)).unwrap_or_else(|| "NaN".to_owned()),
          ('f', v) => number(v).map(|n| n.to_string()).unwrap_or_else(|| "NaN".to_owned()),
          (_, v) => show(v),
        });
      }
      _ => out.push('%'),
    }
  }
  if let [Value::Map(row)] = args {
    let mut filled = String::new();
    let mut rest = out.as_str();
    while let Some(at) = rest.find('$') {
      filled.push_str(&rest[..at]);
      let tail = &rest[at + 1..];
      let len = tail.find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.')).unwrap_or(tail.len());
      let path = tail[..len].trim_end_matches('.');
      let mut found: Option<&Value> = None;
      for (i, key) in path.split('.').enumerate() {
        found = match (i, found) {
          (0, _) => row.get(key),
          (_, Some(Value::Map(map))) => map.get(key),
          _ => None,
        };
        if found.is_none() {
          break;
        }
      }
      match found {
        Some(Value::Str(s)) if !path.is_empty() => filled.push_str(s),
        Some(value) if !path.is_empty() => filled.push_str(&show(value)),
        _ => {
          filled.push('$');
          filled.push_str(path);
        }
      }
      rest = &tail[path.len()..];
    }
    filled.push_str(rest);
    out = filled;
  }
  out
}

async fn run_case<'c>(file: &TestFile, case: &TestCase, expansion: &Expansion, targets: &mut Targets, contract: &'c Arc<Contract>, entered: &mut Vec<Entered<'c>>) -> Outcome {
  let chain = file.chain(case.block);
  let mut base: Option<Run<'c>> = None;
  let mut failure: Option<String> = None;
  for (depth, &block) in chain.iter().enumerate() {
    let rows = expansion.rows[..=depth].to_vec();
    if let Some(held) = entered.iter().find(|e| e.block == block && e.rows == rows) {
      if failure.is_none() {
        failure = held.failure.clone();
      }
      base = Some(held.run.clone());
      continue;
    }
    let mut run = base.clone().unwrap_or_else(|| Run::new(contract, &targets.consts));
    for (name, value) in &expansion.bindings[depth] {
      run.bind(name, value.clone());
    }
    let mut failed = None;
    for (line, step) in &file.blocks[block].before_all {
      if let Err(message) = perform_at(&mut run, targets, *line, step).await {
        failed = Some(format!("  beforeAll failed\n{message}"));
        break;
      }
    }
    entered.push(Entered { block, rows, run: run.clone(), failure: failed.clone() });
    if failure.is_none() {
      failure = failed;
    }
    base = Some(run);
  }
  if let Some(failure) = failure {
    return Outcome::Failed(failure);
  }
  let mut run = base.unwrap_or_else(|| Run::new(contract, &targets.consts));
  for (name, value) in expansion.bindings.last().into_iter().flatten() {
    run.bind(name, value.clone());
  }
  let mut failed: Option<String> = None;
  'body: {
    for &block in &chain {
      for (line, step) in &file.blocks[block].before_each {
        if let Err(message) = perform_at(&mut run, targets, *line, step).await {
          failed = Some(message);
          break 'body;
        }
      }
    }
    for (line, step) in &case.steps {
      if let Err(message) = perform_at(&mut run, targets, *line, step).await {
        failed = Some(message);
        break 'body;
      }
    }
  }
  for &block in chain.iter().rev() {
    for (line, step) in &file.blocks[block].after_each {
      if let Err(message) = perform_at(&mut run, targets, *line, step).await {
        failed.get_or_insert(message);
        break;
      }
    }
  }
  match failed {
    None => Outcome::Passed,
    Some(message) => Outcome::Failed(message),
  }
}

/// Runs the `afterAll` hooks of every block a test entered, innermost first,
/// after the file's last test; each failure is named after its block.
async fn finish(file: &TestFile, targets: &mut Targets, entered: &[Entered<'_>]) -> Vec<(String, String)> {
  let mut out = Vec::new();
  for held in entered.iter().rev() {
    let block = &file.blocks[held.block];
    let mut run = held.run.clone();
    for (line, step) in &block.after_all {
      if let Err(message) = perform_at(&mut run, targets, *line, step).await {
        let place: Vec<&str> = file.chain(held.block).into_iter().map(|b| file.blocks[b].name.as_str()).filter(|n| !n.is_empty()).collect();
        out.push((if place.is_empty() { "afterAll".to_owned() } else { format!("{} > afterAll", place.join(" > ")) }, message));
        break;
      }
    }
  }
  out
}

async fn perform_at(run: &mut Run<'_>, targets: &mut Targets, line: usize, step: &Step) -> Result<(), String> {
  perform(run, targets, step).await.map_err(|message| format!("  line {line}: {message}"))
}

async fn perform(run: &mut Run<'_>, targets: &mut Targets, step: &Step) -> Result<(), String> {
  match step {
    Step::Mock { name, mock } => run.mock(name, mock).await,
    Step::Run { binding, target, ctx, input } => {
      let body = targets.body(target)?;
      let input = match input {
        Some(expr) => Some(run.value(expr).await?),
        None => None,
      };
      let value = match run.run(&body, ctx, input).await? {
        Ok(value) => value,
        Err(fail) => return Err(format!("{} failed: {}: {}", describe(target), fail.kind.as_str(), fail.message)),
      };
      match binding {
        Some(Binding::Name(name)) => run.bind(name, value),
        Some(Binding::Fields(fields)) => {
          let Value::Map(map) = value else { return Err(format!("{} returned {}, not an object to destructure", describe(target), show(&value))) };
          for (field, local) in fields {
            run.bind(local, map.get(field).cloned().unwrap_or(Value::Null));
          }
        }
        None => {}
      }
      Ok(())
    }
    Step::Let { name, value } => {
      let value = run.value(value).await?;
      run.bind(name, value);
      Ok(())
    }
    Step::Fn { name, answer } => {
      let answer = match answer {
        Some(answer) => Some(run.resolve(answer).await?),
        None => None,
      };
      run.fns.lock().insert(name.clone(), MockFn { answer, ..MockFn::default() });
      run.bind(name, Value::str(format!("[mock function {name}]")));
      Ok(())
    }
    Step::Answer { name, answer, once } => {
      let resolved = run.resolve(answer).await?;
      let mut fns = run.fns.lock();
      let mock = fns.get_mut(name).ok_or_else(|| format!("`{name}` is not a mock function"))?;
      match once {
        true => mock.once.push_back(resolved),
        false => mock.answer = Some(resolved),
      }
      Ok(())
    }
    Step::Clear { name, reset } => {
      let mut fns = run.fns.lock();
      let names: Vec<String> = if name.is_empty() { fns.keys().cloned().collect() } else { vec![name.clone()] };
      for name in names {
        let mock = fns.get_mut(&name).ok_or_else(|| format!("`{name}` is not a mock function"))?;
        mock.calls.clear();
        mock.results.clear();
        if *reset {
          mock.answer = None;
          mock.once.clear();
        }
      }
      Ok(())
    }
    Step::Assert(assertion) => check(run, targets, assertion).await,
  }
}

async fn check(run: &mut Run<'_>, targets: &mut Targets, assertion: &Assertion) -> Result<(), String> {
  match assertion {
    Assertion::Ok(expr) => {
      let value = run.value(expr).await?;
      if !truthy(&value) {
        return Err(format!("assert.ok: {}", show(&value)));
      }
      Ok(())
    }
    Assertion::Equal(left, right) => {
      let actual = run.value(left).await?;
      let expected = run.value(right).await?;
      if !same(&actual, &expected) {
        return Err(format!("assert.equal\n    actual:   {}\n    expected: {}", show(&actual), show(&expected)));
      }
      Ok(())
    }
    Assertion::Rejects { target, ctx, input, kind } => {
      let body = targets.body(target)?;
      let input = match input {
        Some(expr) => Some(run.value(expr).await?),
        None => None,
      };
      match run.run(&body, ctx, input).await? {
        Ok(value) => Err(format!("assert.rejects: {} returned {}", describe(target), show(&value))),
        Err(fail) => match kind {
          Some(kind) if fail.kind.as_str() != kind => Err(format!("assert.rejects: {} failed with `{}`, not `{kind}`: {}", describe(target), fail.kind.as_str(), fail.message)),
          _ => Ok(()),
        },
      }
    }
    Assertion::Expect { subject, not, matcher, message } => expect(run, targets, subject, *not, matcher, message.as_deref()).await,
  }
}

async fn expect(run: &mut Run<'_>, targets: &mut Targets, subject: &Subject, not: bool, matcher: &Matcher, message: Option<&str>) -> Result<(), String> {
  let how = match subject {
    Subject::Settled { rejects: true, .. } => ".rejects",
    Subject::Settled { .. } => ".resolves",
    _ => "",
  };
  let lead = message.map(|m| format!("{m}\n    ")).unwrap_or_default();
  let header = format!("{lead}expect(received){how}{}.{}({})", if not { ".not" } else { "" }, matcher.name(), if matcher.takes_expected() { "expected" } else { "" });
  let (pass, detail) = match subject {
    Subject::Mock(name) => {
      let (calls, results) = {
        let fns = run.fns.lock();
        let mock = fns.get(name).ok_or_else(|| format!("{header}: `{name}` is not a mock function"))?;
        (mock.calls.clone(), mock.results.clone())
      };
      calls_match(run, matcher, &calls, &results, not).await?
    }
    Subject::Settled { target, ctx, input, rejects } => {
      let body = targets.body(target)?;
      let input = match input {
        Some(expr) => Some(run.value(expr).await?),
        None => None,
      };
      let value = match (run.run(&body, ctx, input).await?, *rejects) {
        (Ok(value), false) => value,
        (Err(fail), false) => return Err(format!("{header}\n    {} failed instead of returning: {}: {}", describe(target), fail.kind.as_str(), fail.message)),
        (Ok(value), true) => return Err(format!("{header}\n    {} returned instead of failing: {}", describe(target), show(&value))),
        (Err(fail), true) => {
          let mut failure = ValueMap::default();
          failure.insert("kind".to_owned(), Value::str(fail.kind.as_str()));
          failure.insert("message".to_owned(), Value::str(fail.message.clone()));
          Value::Map(failure)
        }
      };
      value_match(run, matcher, &value, not).await?
    }
    Subject::Value(expr) => {
      let value = run.value(expr).await?;
      value_match(run, matcher, &value, not).await?
    }
  };
  if pass == not {
    return Err(format!("{header}\n{detail}"));
  }
  Ok(())
}

/// Whether `actual` meets `matcher` and what to say when that is not what was wanted.
async fn value_match(run: &Run<'_>, matcher: &Matcher, actual: &Value, not: bool) -> Result<(bool, String), String> {
  let report = |expected: String| format!("    Expected: {}{expected}\n    Received: {}", if not { "not " } else { "" }, show(actual));
  let compare = |expected: &Value, op: fn(f64, f64) -> bool| matches!((number(actual), number(expected)), (Some(a), Some(b)) if op(a, b));
  Ok(match matcher {
    Matcher::Be(e) | Matcher::Equal(e) => {
      let expected = run.value(e).await?;
      (agree(actual, &expected, false), report(show(&expected)))
    }
    Matcher::StrictEqual(e) => {
      let expected = run.value(e).await?;
      (agree(actual, &expected, true), report(show(&expected)))
    }
    Matcher::Truthy => (truthy(actual), report("a truthy value".to_owned())),
    Matcher::Falsy => (!truthy(actual), report("a falsy value".to_owned())),
    Matcher::Null | Matcher::Undefined => (matches!(actual, Value::Null), report("null".to_owned())),
    Matcher::Defined => (!matches!(actual, Value::Null), report("a defined value".to_owned())),
    Matcher::NaN => (number(actual).is_some_and(f64::is_nan), report("NaN".to_owned())),
    Matcher::GreaterThan(e) => {
      let expected = run.value(e).await?;
      (compare(&expected, |a, b| a > b), report(format!("> {}", show(&expected))))
    }
    Matcher::GreaterThanOrEqual(e) => {
      let expected = run.value(e).await?;
      (compare(&expected, |a, b| a >= b), report(format!(">= {}", show(&expected))))
    }
    Matcher::LessThan(e) => {
      let expected = run.value(e).await?;
      (compare(&expected, |a, b| a < b), report(format!("< {}", show(&expected))))
    }
    Matcher::LessThanOrEqual(e) => {
      let expected = run.value(e).await?;
      (compare(&expected, |a, b| a <= b), report(format!("<= {}", show(&expected))))
    }
    Matcher::CloseTo(e, digits) => {
      let expected = run.value(e).await?;
      let digits = match digits {
        Some(d) => number(&run.value(d).await?).unwrap_or(2.0),
        None => 2.0,
      };
      let pass = matches!((number(actual), number(&expected)), (Some(a), Some(b)) if a == b || (a - b).abs() < 10f64.powf(-digits) / 2.0);
      (pass, report(format!("{} to {digits} digits", show(&expected))))
    }
    Matcher::Contain(e) => {
      let expected = run.value(e).await?;
      let pass = match (actual, text(actual), text(&expected)) {
        (Value::Seq(items), _, _) => items.iter().any(|item| agree(item, &expected, false)),
        (_, Some(a), Some(b)) => a.contains(&b),
        _ => false,
      };
      (pass, report(format!("a value containing {}", show(&expected))))
    }
    Matcher::ContainEqual(e) => {
      let expected = run.value(e).await?;
      let pass = matches!(actual, Value::Seq(items) if items.iter().any(|item| agree(item, &expected, false)));
      (pass, report(format!("a list holding an item equal to {}", show(&expected))))
    }
    Matcher::Length(e) => {
      let expected = run.value(e).await?;
      let has = length(actual);
      let pass = matches!((has, number(&expected)), (Some(h), Some(n)) if h as f64 == n);
      (pass, format!("{}\n    Received length: {}", report(format!("length {}", show(&expected))), has.map(|h| h.to_string()).unwrap_or_else(|| "none".to_owned())))
    }
    Matcher::Property(path, value) => {
      let path = run.value(path).await?;
      let keys = keys_of(&path)?;
      let found = walk(actual, &keys);
      let pass = match (found, value) {
        (None, _) => false,
        (Some(_), None) => true,
        (Some(at), Some(e)) => agree(at, &run.value(e).await?, false),
      };
      (pass, report(format!("a property at {}", show(&path))))
    }
    Matcher::Match(pattern) => {
      let pass = match text(actual) {
        Some(t) => pattern_matches(run, pattern, &t).await?,
        None => false,
      };
      (pass, report(pattern_label(run, pattern).await?))
    }
    Matcher::MatchObject(e) => {
      let expected = run.value(e).await?;
      (subset(actual, &expected), report(format!("an object matching {}", show(&expected))))
    }
    Matcher::TypeOf(e) => {
      let expected = run.value(e).await?;
      (text(&expected).as_deref() == Some(type_of(actual)), report(format!("a value of type {}", show(&expected))))
    }
    Matcher::OneOf(e) => {
      let expected = run.value(e).await?;
      (matches!(&expected, Value::Seq(items) if items.iter().any(|item| agree(actual, item, false))), report(format!("one of {}", show(&expected))))
    }
    Matcher::Throw(pattern) => {
      let (kind, message) = match actual {
        Value::Map(map) => (map.get("kind").and_then(text).unwrap_or_default(), map.get("message").and_then(text).unwrap_or_default()),
        other => (String::new(), show(other)),
      };
      let pass = match pattern {
        None => true,
        Some(Pattern::Text(e)) => {
          let wanted = text(&run.value(e).await?).unwrap_or_default();
          kind == wanted || message.contains(&wanted)
        }
        Some(Pattern::Regex { source, flags }) => regex_of(source, flags)?.is_match(&format!("{kind} {message}")),
      };
      let wanted = match pattern {
        None => "a failure".to_owned(),
        Some(pattern) => format!("a failure matching {}", pattern_label(run, pattern).await?),
      };
      (pass, format!("    Expected: {}{wanted}\n    Received: {kind}: {message}", if not { "not " } else { "" }))
    }
    other => return Err(format!("`{}` reads a mock function, not a value", other.name())),
  })
}

async fn calls_match(run: &Run<'_>, matcher: &Matcher, calls: &[Value], results: &[Result<Value, String>], not: bool) -> Result<(bool, String), String> {
  let neg = if not { "not " } else { "" };
  let shown = format!("    Calls: {}", show(&Value::seq(calls.to_vec())));
  let returned: Vec<&Value> = results.iter().filter_map(|r| r.as_ref().ok()).collect();
  let with = |call: &Value, args: &Value| agree(&Value::seq(vec![call.clone()]), args, false);
  let values = |exprs: &[Expr]| {
    let exprs = exprs.to_vec();
    async move {
      let mut out = Vec::new();
      for expr in &exprs {
        out.push(run.value(expr).await?);
      }
      Ok::<_, String>(Value::seq(out))
    }
  };
  Ok(match matcher {
    Matcher::Called => (!calls.is_empty(), format!("    Expected: {neg}a call\n{shown}")),
    Matcher::CalledOnce => (calls.len() == 1, format!("    Expected: {neg}one call\n{shown}")),
    Matcher::CalledTimes(e) => {
      let n = run.value(e).await?;
      (number(&n) == Some(calls.len() as f64), format!("    Expected: {neg}{} calls\n{shown}", show(&n)))
    }
    Matcher::CalledWith(args) => {
      let args = values(args).await?;
      (calls.iter().any(|call| with(call, &args)), format!("    Expected: {neg}a call with {}\n{shown}", show(&args)))
    }
    Matcher::CalledExactlyOnceWith(args) => {
      let args = values(args).await?;
      (calls.len() == 1 && with(&calls[0], &args), format!("    Expected: {neg}one call, with {}\n{shown}", show(&args)))
    }
    Matcher::LastCalledWith(args) => {
      let args = values(args).await?;
      (calls.last().is_some_and(|call| with(call, &args)), format!("    Expected: {neg}a last call with {}\n{shown}", show(&args)))
    }
    Matcher::NthCalledWith(n, args) => {
      let n = number(&run.value(n).await?).unwrap_or(0.0) as usize;
      let args = values(args).await?;
      (n >= 1 && calls.get(n - 1).is_some_and(|call| with(call, &args)), format!("    Expected: {neg}call {n} with {}\n{shown}", show(&args)))
    }
    Matcher::Returned => (!returned.is_empty(), format!("    Expected: {neg}a return\n    Returns: {}", returned.len())),
    Matcher::ReturnedTimes(e) => {
      let n = run.value(e).await?;
      (number(&n) == Some(returned.len() as f64), format!("    Expected: {neg}{} returns\n    Returns: {}", show(&n), returned.len()))
    }
    Matcher::ReturnedWith(e) => {
      let expected = run.value(e).await?;
      (returned.iter().any(|value| agree(value, &expected, false)), format!("    Expected: {neg}a return of {}", show(&expected)))
    }
    Matcher::LastReturnedWith(e) => {
      let expected = run.value(e).await?;
      (matches!(results.last(), Some(Ok(value)) if agree(value, &expected, false)), format!("    Expected: {neg}a last return of {}", show(&expected)))
    }
    other => return Err(format!("`{}` reads a value, not a mock function", other.name())),
  })
}

async fn pattern_matches(run: &Run<'_>, pattern: &Pattern, text_value: &str) -> Result<bool, String> {
  match pattern {
    Pattern::Text(e) => Ok(text(&run.value(e).await?).is_some_and(|wanted| text_value.contains(&wanted))),
    Pattern::Regex { source, flags } => Ok(regex_of(source, flags)?.is_match(text_value)),
  }
}

async fn pattern_label(run: &Run<'_>, pattern: &Pattern) -> Result<String, String> {
  match pattern {
    Pattern::Text(e) => Ok(format!("a string containing {}", show(&run.value(e).await?))),
    Pattern::Regex { source, flags } => Ok(format!("a string matching /{source}/{flags}")),
  }
}

fn regex_of(source: &str, flags: &str) -> Result<regex::Regex, String> {
  regex::RegexBuilder::new(source)
    .case_insensitive(flags.contains('i'))
    .multi_line(flags.contains('m'))
    .dot_matches_new_line(flags.contains('s'))
    .build()
    .map_err(|e| format!("the pattern /{source}/{flags}: {e}"))
}

fn helper_of(value: &Value) -> Option<(String, &ValueMap)> {
  let Value::Map(map) = value else { return None };
  match map.get(EXPECT_MARK) {
    Some(Value::Str(helper)) => Some((helper.to_string(), map)),
    _ => None,
  }
}

/// Whether `actual` meets one of `expect`'s helpers: `any`, `anything`,
/// `objectContaining` and the rest.
fn helper_matches(helper: &str, fields: &ValueMap, actual: &Value) -> bool {
  match helper {
    "anything" => !matches!(actual, Value::Null),
    "any" => match fields.get("of").and_then(text).as_deref() {
      Some("String") => matches!(actual, Value::Str(_)),
      Some("Number") => matches!(actual, Value::Int(_) | Value::UInt(_) | Value::F32(_) | Value::F64(_)),
      Some("BigInt") => matches!(actual, Value::Int(_) | Value::UInt(_)),
      Some("Boolean") => matches!(actual, Value::Bool(_)),
      Some("Array") => matches!(actual, Value::Seq(_)),
      Some("Object") => matches!(actual, Value::Map(_)),
      _ => false,
    },
    "objectContaining" => match (actual, fields.get("value")) {
      (Value::Map(a), Some(Value::Map(e))) => e.iter().filter(|(k, _)| k.as_str() != EXPECT_MARK).all(|(k, v)| a.get(k).is_some_and(|w| agree(w, v, false))),
      _ => false,
    },
    "arrayContaining" => match (actual, fields.get("value")) {
      (Value::Seq(a), Some(Value::Seq(e))) => e.iter().all(|v| a.iter().any(|w| agree(w, v, false))),
      _ => false,
    },
    "stringContaining" => matches!((text(actual), fields.get("value").and_then(text)), (Some(a), Some(e)) if a.contains(&e)),
    "stringMatching" => {
      let Some(a) = text(actual) else { return false };
      let (source, flags) = match fields.get("source").and_then(text) {
        Some(source) => (source, fields.get("flags").and_then(text).unwrap_or_default()),
        None => (fields.get("value").and_then(text).unwrap_or_default(), String::new()),
      };
      regex_of(&source, &flags).is_ok_and(|re| re.is_match(&a))
    }
    "closeTo" => match (number(actual), fields.get("value").and_then(number)) {
      (Some(a), Some(b)) => (a - b).abs() < 10f64.powf(-fields.get("digits").and_then(number).unwrap_or(2.0)) / 2.0,
      _ => false,
    },
    _ => false,
  }
}

/// `toEqual`'s equality, reading `expect`'s helpers wherever they sit in
/// `expected`. Loose equality lets a bigint equal the whole number it stands
/// for; `strict` does not.
fn agree(actual: &Value, expected: &Value, strict: bool) -> bool {
  if let Some((helper, fields)) = helper_of(expected) {
    let hit = helper_matches(&helper, fields, actual);
    return hit != matches!(fields.get("not"), Some(Value::Bool(true)));
  }
  match (actual, expected) {
    (Value::Seq(x), Value::Seq(y)) => x.len() == y.len() && x.iter().zip(y.iter()).all(|(p, q)| agree(p, q, strict)),
    (Value::Map(x), Value::Map(y)) => x.len() == y.len() && y.iter().all(|(k, v)| x.get(k).is_some_and(|w| agree(w, v, strict))),
    (Value::Variant { tag: t, payload: p }, Value::Variant { tag: u, payload: q }) => {
      t == u
        && match (p, q) {
          (Some(p), Some(q)) => agree(p, q, strict),
          (None, None) => true,
          _ => false,
        }
    }
    _ if strict => actual == expected,
    _ => same(actual, expected),
  }
}

/// `toMatchObject`'s subset: every field `expected` names is in `actual` and
/// matches, recursively; a list matches only a list of the same length.
fn subset(actual: &Value, expected: &Value) -> bool {
  if helper_of(expected).is_some() {
    return agree(actual, expected, false);
  }
  match (actual, expected) {
    (Value::Map(a), Value::Map(e)) => e.iter().all(|(k, v)| a.get(k).is_some_and(|w| subset(w, v))),
    (Value::Seq(a), Value::Seq(e)) => a.len() == e.len() && a.iter().zip(e.iter()).all(|(w, v)| subset(w, v)),
    _ => agree(actual, expected, false),
  }
}

fn number(value: &Value) -> Option<f64> {
  match value {
    Value::Int(n) => Some(*n as f64),
    Value::UInt(n) => Some(*n as f64),
    Value::F64(f) => Some(*f),
    Value::F32(f) => Some(*f as f64),
    _ => None,
  }
}

fn text(value: &Value) -> Option<String> {
  match value {
    Value::Str(s) => Some(s.to_string()),
    _ => None,
  }
}

fn length(value: &Value) -> Option<usize> {
  match value {
    Value::Seq(items) => Some(items.len()),
    Value::Str(s) => Some(s.chars().count()),
    Value::Bytes(b) => Some(b.len()),
    _ => None,
  }
}

/// A value's `typeof` as the TypeScript that reads it would say it: an integer is a bigint.
fn type_of(value: &Value) -> &'static str {
  match value {
    Value::Bool(_) => "boolean",
    Value::Int(_) | Value::UInt(_) => "bigint",
    Value::F32(_) | Value::F64(_) => "number",
    Value::Str(_) => "string",
    _ => "object",
  }
}

fn keys_of(path: &Value) -> Result<Vec<String>, String> {
  match path {
    Value::Str(s) => Ok(s.replace('[', ".").replace(']', "").split('.').filter(|k| !k.is_empty()).map(str::to_owned).collect()),
    Value::Seq(items) => items.iter().map(|k| text(k).or_else(|| number(k).map(|n| (n as i64).to_string())).ok_or_else(|| format!("a property path of {}", show(k)))).collect(),
    other => Err(format!("a property path must be a string or a list, got {}", show(other))),
  }
}

fn walk<'v>(value: &'v Value, keys: &[String]) -> Option<&'v Value> {
  let mut at = value;
  for key in keys {
    at = match at {
      Value::Map(map) => map.get(key)?,
      Value::Seq(items) => items.iter().nth(key.parse::<usize>().ok()?)?,
      _ => return None,
    };
  }
  Some(at)
}

fn describe(target: &Target) -> String {
  match target {
    Target::Loader { .. } => "`load`".to_owned(),
    Target::Meta { .. } => "`meta`".to_owned(),
    Target::Store { .. } => "`store`".to_owned(),
    Target::Paths { .. } => "`paths`".to_owned(),
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
/// Equality as a test means it: an integer and a float holding the same whole
/// number are the same value, since a test writes `35` for the `35n` an
/// integer field reads back as and the rest is structural.
fn same(a: &Value, b: &Value) -> bool {
  fn whole(v: &Value) -> Option<f64> {
    match v {
      Value::Int(n) => Some(*n as f64),
      Value::UInt(n) => Some(*n as f64),
      Value::F64(f) if f.fract() == 0.0 => Some(*f),
      Value::F32(f) if f.fract() == 0.0 => Some(*f as f64),
      _ => None,
    }
  }
  match (a, b) {
    (Value::Seq(x), Value::Seq(y)) => x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same(p, q)),
    (Value::Map(x), Value::Map(y)) => x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| same(v, w))),
    (Value::Variant { tag: t, payload: p }, Value::Variant { tag: u, payload: q }) => {
      t == u
        && match (p, q) {
          (Some(p), Some(q)) => same(p, q),
          (None, None) => true,
          _ => false,
        }
    }
    _ if a == b => true,
    _ => matches!((whole(a), whole(b)), (Some(x), Some(y)) if x == y && (matches!(a, Value::Int(_) | Value::UInt(_)) != matches!(b, Value::Int(_) | Value::UInt(_)))),
  }
}

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
      if let Some((helper, fields)) = helper_of(value) {
        let _ = write!(out, "expect.{}{helper}(", if matches!(fields.get("not"), Some(Value::Bool(true))) { "not." } else { "" });
        if let Some(inner) = fields.get("value").or_else(|| fields.get("of")).or_else(|| fields.get("source")) {
          write_value(inner, out);
        }
        out.push(')');
        return;
      }
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
