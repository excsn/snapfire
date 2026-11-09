//! Reads a `*.test.ts` file into test cases the runner replays through the
//! interpreter. A file holds imports, `const` fixtures, mock functions, `let`
//! bindings a hook assigns, hooks, `describe` blocks and tests. A test or a
//! hook holds `ctx({ ... })` mocks, runs of a loader or an action, local
//! `const`s, what a mock function answers and expectations. Anything else
//! fails the file with its line, since a test that silently did less than it
//! says is worse than none.

use snapfire_fsr_ir::ast::{Entry, Expr, Lit};
use oxc_span::{GetSpan, Span};
use oxc_ast::ast as js;

use crate::{Lowered, LowerError, Lowerer, Parsed, Residue, SessionDefaults, parse, prop_name};

/// The field that marks an object inside an expected value as one of
/// `expect`'s helpers: `expect.any(String)` lowers to
/// `{ "$sf.expect": "any", "of": "String" }`.
pub const EXPECT_MARK: &str = "$sf.expect";

/// Where a run goes: the loader module beside the test or one export of the
/// actions module. Paths are relative to the app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
  Loader { file: String },
  /// The `meta` of a loader module, run over the data rather than a ctx.
  Meta { file: String },
  /// The `store` of a loader module, run over the data the same way.
  Store { file: String },
  /// The `paths` of a page loader, run over the ctx bound above the call.
  Paths { file: String },
  Action { file: String, export: String },
  Handler { file: String, export: String },
  Middleware { file: String },
}

/// The `ctx({ ... })` literal, each part an expression the runner evaluates.
#[derive(Debug, Clone, PartialEq, Default)]
#[non_exhaustive]
pub struct Mock {
  pub session: Vec<(String, Expr)>,
  /// `(service, method, lambda)`; a value that is not a function is a lambda of no parameters.
  pub services: Vec<(String, String, Expr)>,
  /// `(service, method, name)`: a method a mock function answers, by the name the test gave it.
  pub mock_fns: Vec<(String, String, String)>,
  pub input: Option<Expr>,
  pub params: Vec<(String, Expr)>,
  pub query: Vec<(String, Expr)>,
  pub identity: Option<Expr>,
  pub locale: Option<Expr>,
  pub locales: Option<Expr>,
  pub path: Option<Expr>,
  pub host: Option<Expr>,
  pub origin: Option<Expr>,
  pub config: Vec<(String, Expr)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Binding {
  Name(String),
  /// `const { a, b: local } = ...`: field, local name.
  Fields(Vec<(String, String)>),
}

/// Whether a test runs. `Skip` covers `.skip`, a skipped `describe` around it
/// and an `.only` elsewhere in the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
  Run,
  Skip,
  Todo,
}

/// An `each` table: its rows (evaluated when the file runs) and what a row
/// binds. An array row is spread over the parameters; any other row is the
/// one argument.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Each {
  pub table: Expr,
  pub params: Vec<Binding>,
}

/// What `toMatch`, `toThrow` or `expect.stringMatching` compares text with:
/// a string it must contain or a regular expression, kept as its source.
#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
  Text(Expr),
  Regex { source: String, flags: String },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Matcher {
  Be(Expr),
  Equal(Expr),
  StrictEqual(Expr),
  Truthy,
  Falsy,
  Null,
  Undefined,
  Defined,
  NaN,
  GreaterThan(Expr),
  GreaterThanOrEqual(Expr),
  LessThan(Expr),
  LessThanOrEqual(Expr),
  CloseTo(Expr, Option<Expr>),
  Contain(Expr),
  ContainEqual(Expr),
  Length(Expr),
  Property(Expr, Option<Expr>),
  Match(Pattern),
  MatchObject(Expr),
  TypeOf(Expr),
  OneOf(Expr),
  /// A failed run's kind or message, under `.rejects`.
  Throw(Option<Pattern>),
  Called,
  CalledOnce,
  CalledTimes(Expr),
  CalledWith(Vec<Expr>),
  CalledExactlyOnceWith(Vec<Expr>),
  LastCalledWith(Vec<Expr>),
  NthCalledWith(Expr, Vec<Expr>),
  Returned,
  ReturnedTimes(Expr),
  ReturnedWith(Expr),
  LastReturnedWith(Expr),
}

impl Matcher {
  pub fn name(&self) -> &'static str {
    match self {
      Self::Be(_) => "toBe",
      Self::Equal(_) => "toEqual",
      Self::StrictEqual(_) => "toStrictEqual",
      Self::Truthy => "toBeTruthy",
      Self::Falsy => "toBeFalsy",
      Self::Null => "toBeNull",
      Self::Undefined => "toBeUndefined",
      Self::Defined => "toBeDefined",
      Self::NaN => "toBeNaN",
      Self::GreaterThan(_) => "toBeGreaterThan",
      Self::GreaterThanOrEqual(_) => "toBeGreaterThanOrEqual",
      Self::LessThan(_) => "toBeLessThan",
      Self::LessThanOrEqual(_) => "toBeLessThanOrEqual",
      Self::CloseTo(..) => "toBeCloseTo",
      Self::Contain(_) => "toContain",
      Self::ContainEqual(_) => "toContainEqual",
      Self::Length(_) => "toHaveLength",
      Self::Property(..) => "toHaveProperty",
      Self::Match(_) => "toMatch",
      Self::MatchObject(_) => "toMatchObject",
      Self::TypeOf(_) => "toBeTypeOf",
      Self::OneOf(_) => "toBeOneOf",
      Self::Throw(_) => "toThrow",
      Self::Called => "toHaveBeenCalled",
      Self::CalledOnce => "toHaveBeenCalledOnce",
      Self::CalledTimes(_) => "toHaveBeenCalledTimes",
      Self::CalledWith(_) => "toHaveBeenCalledWith",
      Self::CalledExactlyOnceWith(_) => "toHaveBeenCalledExactlyOnceWith",
      Self::LastCalledWith(_) => "toHaveBeenLastCalledWith",
      Self::NthCalledWith(..) => "toHaveBeenNthCalledWith",
      Self::Returned => "toHaveReturned",
      Self::ReturnedTimes(_) => "toHaveReturnedTimes",
      Self::ReturnedWith(_) => "toHaveReturnedWith",
      Self::LastReturnedWith(_) => "toHaveLastReturnedWith",
    }
  }

  /// Whether this compares against something the test wrote, which a failure's header names as `expected`.
  pub fn takes_expected(&self) -> bool {
    !matches!(self, Self::Truthy | Self::Falsy | Self::Null | Self::Undefined | Self::Defined | Self::NaN | Self::Called | Self::CalledOnce | Self::Returned | Self::Throw(None))
  }

  /// Whether this reads a mock function's calls rather than a value.
  pub fn reads_calls(&self) -> bool {
    matches!(
      self,
      Self::Called | Self::CalledOnce | Self::CalledTimes(_) | Self::CalledWith(_) | Self::CalledExactlyOnceWith(_) | Self::LastCalledWith(_) | Self::NthCalledWith(..) | Self::Returned | Self::ReturnedTimes(_) | Self::ReturnedWith(_) | Self::LastReturnedWith(_)
    )
  }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Subject {
  Value(Expr),
  /// `expect(run).resolves` or `.rejects`: what the run returned or how it
  /// failed as `{ kind, message }`.
  Settled { target: Target, ctx: String, input: Option<Expr>, rejects: bool },
  /// A mock function, by name, for the matchers that read its calls.
  Mock(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Assertion {
  Ok(Expr),
  Equal(Expr, Expr),
  /// The run must fail, with this kind when one is named.
  Rejects { target: Target, ctx: String, input: Option<Expr>, kind: Option<String> },
  /// `message` is `expect`'s second argument, which leads the report when the expectation fails.
  Expect { subject: Subject, not: bool, matcher: Matcher, message: Option<String> },
}

/// What a mock function answers a call with.
#[derive(Debug, Clone, PartialEq)]
pub enum Answer {
  /// `mockReturnValue(v)` or `mockResolvedValue(v)`: this value, whatever the arguments.
  Returns(Expr),
  /// `fn(impl)` or `mockImplementation(impl)`: a lambda over the call's arguments.
  Calls(Expr),
  /// `mockRejectedValue(v)`: a failure, of the kind `v.kind` names when it is an object that names one.
  Fails(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Step {
  Mock { name: String, mock: Mock },
  /// `input` is what a `meta({ data })` names, run in place of the ctx's own.
  Run { binding: Option<Binding>, target: Target, ctx: String, input: Option<Expr> },
  /// `const name = value` inside a test or a hook.
  Let { name: String, value: Expr },
  Assert(Assertion),
  /// `const name = fn(impl)`: a mock function a ctx's services may name.
  /// Declared at the top of a file or a `describe`, it starts every test
  /// with no calls.
  Fn { name: String, answer: Option<Answer> },
  /// `name.mockReturnValue(v)` and the rest: what the mock function answers
  /// from here on or next time only.
  Answer { name: String, answer: Answer, once: bool },
  /// `name.mockClear()` or `mockReset()`; an empty name is every mock
  /// function, as `vi.clearAllMocks()` asks.
  Clear { name: String, reset: bool },
}

/// A `describe` (or the file itself as block 0): its hooks and the table that
/// repeats it.
#[derive(Debug, Clone, PartialEq, Default)]
#[non_exhaustive]
pub struct Block {
  pub name: String,
  pub parent: Option<usize>,
  pub each: Option<Each>,
  pub before_all: Vec<(usize, Step)>,
  pub after_all: Vec<(usize, Step)>,
  pub before_each: Vec<(usize, Step)>,
  pub after_each: Vec<(usize, Step)>,
}

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct TestCase {
  /// The test's own name; under an `each`, the pattern each row fills.
  pub name: String,
  pub line: usize,
  pub block: usize,
  pub mode: Mode,
  pub each: Option<Each>,
  pub steps: Vec<(usize, Step)>,
}

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct TestFile {
  pub file: String,
  pub blocks: Vec<Block>,
  pub tests: Vec<TestCase>,
}

impl TestFile {
  /// The blocks around `block`, outermost first: the file itself, then each `describe`.
  pub fn chain(&self, block: usize) -> Vec<usize> {
    let mut out = vec![block];
    let mut at = block;
    while let Some(parent) = self.blocks[at].parent {
      out.push(parent);
      at = parent;
    }
    out.reverse();
    out
  }

  /// `case`'s name under the `describe` blocks around it, each pattern as written.
  pub fn full_name(&self, case: &TestCase) -> String {
    let mut names: Vec<&str> = self.chain(case.block).into_iter().map(|b| self.blocks[b].name.as_str()).filter(|n| !n.is_empty()).collect();
    names.push(&case.name);
    names.join(" > ")
  }
}

/// Lowers `source`, the test file at `file` relative to the app.
pub fn lower_tests(file: &str, source: &str) -> Result<TestFile, LowerError> {
  let arena = oxc_allocator::Allocator::default();
  let parsed = parse(&arena, file, source)?;
  let defaults = SessionDefaults::new();
  let (imports, helpers) = imports_of(file, &parsed)?;
  let mut reader = Reader { parsed: &parsed, defaults: &defaults, imports: &imports, helpers: &helpers, blocks: vec![Block::default()], asked: vec![Asked::Run], cases: Vec::new(), frames: vec![Frame::default()] };
  for item in &parsed.module().body {
    match item {
      js::Statement::ImportDeclaration(_) => {}
      other if other.is_module_declaration() => return Err(parsed.residue(other.span(), "an export in a test; a file holds imports, fixtures, hooks, `describe` blocks and tests").into()),
      stmt => reader.statement(stmt, 0)?,
    }
  }
  Ok(reader.finish(file))
}

fn imports_of(file: &str, parsed: &Parsed) -> Result<(Vec<(String, Target)>, Vec<String>), LowerError> {
  let mut imports: Vec<(String, Target)> = Vec::new();
  let mut helpers: Vec<String> = Vec::new();
  for item in &parsed.module().body {
    let js::Statement::ImportDeclaration(import) = item else { continue };
    if import.import_kind.is_type() {
      continue;
    }
    let source = crate::lossy(import.source.value);
    for spec in import.specifiers.iter().flatten() {
      let js::ImportDeclarationSpecifier::ImportSpecifier(named) = spec else {
        return Err(parsed.residue(spec.span(), "a default or namespace import in a test").into());
      };
      if named.import_kind.is_type() {
        continue;
      }
      let local = named.local.name.to_string();
      let imported = match &named.imported {
        js::ModuleExportName::IdentifierName(id) => id.name.to_string(),
        js::ModuleExportName::IdentifierReference(id) => id.name.to_string(),
        js::ModuleExportName::StringLiteral(s) => crate::lossy(s.value),
      };
      if source == "@snapfire/fsr/testing" {
        helpers.push(local);
        continue;
      }
      let Some(resolved) = crate::resolve_specifier(file, &source) else {
        return Err(parsed.residue(import.span, format!("`{source}`; a test imports a route's loader or actions by path or alias and `@snapfire/fsr/testing`")).into());
      };
      let target_file = format!("{resolved}.ts");
      let stem = source.rsplit('/').next().unwrap_or(&source);
      let target = match (stem, imported.as_str()) {
        ("page.loader" | "layout.loader", "load") => Target::Loader { file: target_file },
        ("page.loader" | "layout.loader", "meta") => Target::Meta { file: target_file },
        ("page.loader" | "layout.loader", "store") => Target::Store { file: target_file },
        ("page.loader", "paths") => Target::Paths { file: target_file },
        ("actions", export) => Target::Action { file: target_file, export: export.to_owned() },
        ("route", method) if crate::HANDLER_METHODS.contains(&method) => Target::Handler { file: target_file, export: method.to_owned() },
        ("middleware", "middleware") => Target::Middleware { file: target_file },
        _ => return Err(parsed.residue(named.span, format!("`{imported}` from `{source}`; a test imports `load`, `meta` or `store` from a `page.loader` or a `layout.loader`, `paths` from a `page.loader`, an action from its `actions` or a method from its `route`")).into()),
      };
      imports.push((local, target));
    }
  }
  Ok((imports, helpers))
}

fn is_ident_call(call: &js::CallExpression, name: &str) -> bool {
  matches!(&call.callee, js::Expression::Identifier(id) if id.name.as_str() == name)
}

/// `a.b.c` as `["a", "b", "c"]`, for an expression of identifiers and names.
fn path_of(expr: &js::Expression) -> Option<Vec<String>> {
  match expr {
    js::Expression::Identifier(id) => Some(vec![id.name.to_string()]),
    member_expr if member_expr.is_member_expression() => {
let member = member_expr.as_member_expression().expect("matched as a member");
      let crate::syntax::MemberProp::Ident(prop) = crate::syntax::prop(member) else { return None };
      let mut out = path_of(member.object())?;
      out.push(prop.name.to_string());
      Some(out)
    }
    _ => None,
  }
}

fn callee_path(call: &js::CallExpression) -> Option<Vec<String>> {
  path_of(&call.callee)
}

/// How a `test` or a `describe` asked to run, before an `.only` elsewhere in
/// the file is known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Asked {
  Run,
  Skip,
  Only,
  Todo,
}

/// The names a `describe` or the file makes visible to what it holds.
#[derive(Debug, Default)]
struct Frame {
  fixtures: Vec<(String, Expr)>,
  lets: Vec<String>,
  fns: Vec<String>,
  /// What an `each` row binds.
  params: Vec<String>,
}

/// Where an `each` takes its rows from.
enum Rows<'p> {
  Array(&'p js::CallExpression<'p>),
  Template(&'p js::TaggedTemplateExpression<'p>),
}

struct Reader<'a> {
  parsed: &'a Parsed<'a>,
  defaults: &'a SessionDefaults,
  imports: &'a [(String, Target)],
  helpers: &'a [String],
  blocks: Vec<Block>,
  /// How each block asked to run, by index.
  asked: Vec<Asked>,
  cases: Vec<(TestCase, Asked)>,
  frames: Vec<Frame>,
}

impl<'a> Reader<'a> {
  fn line(&self, span: Span) -> usize {
    self.parsed.loc(span.start).0
  }

  fn lowerer(&self) -> Lowerer<'a> {
    let mut lowerer = Lowerer::new(self.parsed, self.defaults);
    lowerer.globals = self.frames.iter().flat_map(|f| f.fixtures.iter().cloned()).collect();
    for name in self.frames.iter().flat_map(|f| f.lets.iter().chain(f.params.iter())) {
      lowerer.scope.push((name.clone(), Expr::Var(name.clone())));
    }
    lowerer
  }

  fn statement(&mut self, stmt: &js::Statement, block: usize) -> Lowered<()> {
    match stmt {
      js::Statement::VariableDeclaration(var) => self.declaration(var, block),
      js::Statement::ExpressionStatement(e) => {
        let js::Expression::CallExpression(call) = unwrap_await(&e.expression) else {
          return Err(self.parsed.residue(e.span, "a statement outside a test; a file or a `describe` holds fixtures, hooks, `describe` blocks and tests"));
        };
        self.call(call, block)
      }
      other => Err(self.parsed.residue(other.span(), "a statement outside a test; a file or a `describe` holds fixtures, hooks, `describe` blocks and tests")),
    }
  }

  fn declaration(&mut self, var: &js::VariableDeclaration, block: usize) -> Lowered<()> {
    for decl in &var.declarations {
      let js::BindingPattern::BindingIdentifier(name) = &decl.id else {
        return Err(self.parsed.residue(decl.id.span(), "a fixture must be one name"));
      };
      let name = name.name.to_string();
      let Some(init) = decl.init.as_ref() else {
        if var.kind == js::VariableDeclarationKind::Const {
          return Err(self.parsed.residue(decl.span, "a fixture without a value"));
        }
        self.frames.last_mut().expect("a frame is open").lets.push(name);
        continue;
      };
      let mut lowerer = self.lowerer();
      if let js::Expression::CallExpression(call) = unwrap_await(init) {
        if let Some(answer) = mock_fn(&mut lowerer, call)? {
          let line = self.line(decl.span);
          self.frames.last_mut().expect("a frame is open").fns.push(name.clone());
          self.blocks[block].before_each.push((line, Step::Fn { name, answer }));
          continue;
        }
      }
      let expr = lowerer.expr(init)?;
      self.frames.last_mut().expect("a frame is open").fixtures.push((name, expr));
    }
    Ok(())
  }

  fn call(&mut self, call: &js::CallExpression, block: usize) -> Lowered<()> {
    let refused = || self.parsed.residue(call.span, "a call other than `describe`, `test`, `it` or a hook at the top of a file or a `describe`");
    let callee = &call.callee;
    let (path, rows) = match callee {
      js::Expression::CallExpression(inner) => match callee_path(inner) {
        Some(path) if path.last().map(String::as_str) == Some("each") => (path, Some(Rows::Array(inner))),
        _ => return Err(refused()),
      },
      js::Expression::TaggedTemplateExpression(tagged) => match path_of(&tagged.tag) {
        Some(path) if path.last().map(String::as_str) == Some("each") => (path, Some(Rows::Template(tagged))),
        _ => return Err(refused()),
      },
      other => match path_of(other) {
        Some(path) => (path, None),
        None => return Err(refused()),
      },
    };
    let base = path[0].as_str();
    let modifiers: Vec<&str> = path[1..].iter().map(String::as_str).filter(|m| *m != "each").collect();
    match base {
      "beforeAll" | "afterAll" | "beforeEach" | "afterEach" if path.len() == 1 => self.hook(base, call, block),
      "test" | "it" | "xit" | "xtest" | "fit" => self.test(base, &modifiers, rows, call, block),
      "describe" | "xdescribe" | "fdescribe" => self.describe(base, &modifiers, rows, call, block),
      _ => Err(refused()),
    }
  }

  fn asked(&self, base: &str, modifiers: &[&str], todo: bool, span: Span) -> Lowered<Asked> {
    let mut asked = match base {
      "xit" | "xtest" | "xdescribe" => Asked::Skip,
      "fit" | "fdescribe" => Asked::Only,
      _ => Asked::Run,
    };
    for modifier in modifiers {
      asked = match *modifier {
        "skip" => Asked::Skip,
        "only" => Asked::Only,
        "todo" if todo => Asked::Todo,
        "concurrent" | "sequential" => asked,
        other => return Err(self.parsed.residue(span, format!("`.{other}`, which a body test does not take"))),
      };
    }
    Ok(asked)
  }

  fn name(&self, arg: Option<&js::Argument>, span: Span) -> Lowered<String> {
    match arg.map(|a| crate::syntax::arg_expr(a)) {
      Some(js::Expression::StringLiteral(name)) => Ok(crate::lossy(name.value)),
      Some(js::Expression::TemplateLiteral(tpl)) if tpl.expressions.is_empty() => Ok(tpl.quasis.iter().map(|q| q.value.raw.to_string()).collect()),
      Some(other) => Err(self.parsed.residue(other.span(), "a test name must be a string")),
      None => Err(self.parsed.residue(span, "a test or a `describe` takes a name first")),
    }
  }

  fn arrow<'p>(&self, arg: Option<&'p js::Argument<'p>>, span: Span) -> Lowered<&'p js::ArrowFunctionExpression<'p>> {
    match arg.map(|a| crate::syntax::arg_expr(a)) {
      Some(js::Expression::ArrowFunctionExpression(arrow)) => Ok(arrow),
      Some(other) => Err(self.parsed.residue(other.span(), "a body must be an arrow function")),
      None => Err(self.parsed.residue(span, "a body is missing")),
    }
  }

  fn each(&self, rows: Rows<'_>, arrow: &js::ArrowFunctionExpression) -> Lowered<Each> {
    let mut params = Vec::new();
    for param in crate::params_of(&arrow.params) {
      if param.default.is_some() || param.rest {
        return Err(self.parsed.residue(param.span, "a pattern the runner does not bind"));
      }
      params.push(binding_of(self.parsed, param.pattern)?);
    }
    let table = match rows {
      Rows::Array(call) => {
        let Some(first) = call.arguments.first() else { return Err(self.parsed.residue(call.span, "`each` takes its table")) };
        self.lowerer().expr(crate::syntax::arg_expr(first))?
      }
      Rows::Template(tagged) => {
        let headings: Vec<String> = tagged.quasi.quasis.first().map(|q| q.value.raw.to_string()).unwrap_or_default().split('|').map(|h| h.trim().to_owned()).filter(|h| !h.is_empty()).collect();
        if headings.is_empty() {
          return Err(self.parsed.residue(tagged.span, "a template table starts with a heading row, `a | b`"));
        }
        if tagged.quasi.expressions.len() % headings.len() != 0 {
          return Err(self.parsed.residue(tagged.span, format!("a template table of {} columns holds {} values", headings.len(), tagged.quasi.expressions.len())));
        }
        let mut lowerer = self.lowerer();
        let mut rows = Vec::new();
        for chunk in tagged.quasi.expressions.chunks(headings.len()) {
          let mut fields = Vec::new();
          for (heading, value) in headings.iter().zip(chunk) {
            fields.push(Entry::Field(heading.clone(), lowerer.expr(value)?));
          }
          rows.push(Entry::Item(Expr::Object(fields)));
        }
        Expr::Array(rows)
      }
    };
    Ok(Each { table, params })
  }

  fn test(&mut self, base: &str, modifiers: &[&str], rows: Option<Rows<'_>>, call: &js::CallExpression, block: usize) -> Lowered<()> {
    let asked = self.asked(base, modifiers, true, call.span)?;
    let name = self.name(call.arguments.first(), call.span)?;
    let line = self.line(call.span);
    if asked == Asked::Todo || call.arguments.get(1).is_none() {
      if rows.is_some() {
        return Err(self.parsed.residue(call.span, "a todo has no rows"));
      }
      self.cases.push((TestCase { name, line, block, mode: Mode::Todo, each: None, steps: Vec::new() }, Asked::Todo));
      return Ok(());
    }
    let arrow = self.arrow(call.arguments.get(1), call.span)?;
    let each = match rows {
      Some(rows) => Some(self.each(rows, arrow)?),
      None if !crate::params_of(&arrow.params).is_empty() => return Err(self.parsed.residue(arrow.span, "a body test's body takes no arguments: it is async and awaits what it runs")),
      None => None,
    };
    let params = each.as_ref().map(|e| bound_names(&e.params)).unwrap_or_default();
    self.frames.push(Frame { params, ..Frame::default() });
    let steps = self.body(arrow);
    self.frames.pop();
    self.cases.push((TestCase { name, line, block, mode: Mode::Run, each, steps: steps? }, asked));
    Ok(())
  }

  fn describe(&mut self, base: &str, modifiers: &[&str], rows: Option<Rows<'_>>, call: &js::CallExpression, block: usize) -> Lowered<()> {
    let asked = self.asked(base, modifiers, false, call.span)?;
    let name = self.name(call.arguments.first(), call.span)?;
    let arrow = self.arrow(call.arguments.get(1), call.span)?;
    if arrow.r#async {
      return Err(self.parsed.residue(arrow.span, "a `describe` body runs at once; put what it awaits in a test or a hook"));
    }
    let each = match rows {
      Some(rows) => Some(self.each(rows, arrow)?),
      None => None,
    };
    let js::ArrowFunctionBody::FunctionBody(body) = &arrow.body else {
      return Err(self.parsed.residue(arrow.span, "a `describe` body must be a block"));
    };
    let id = self.blocks.len();
    let params = each.as_ref().map(|e| bound_names(&e.params)).unwrap_or_default();
    self.blocks.push(Block { name, parent: Some(block), each, ..Block::default() });
    self.asked.push(asked);
    self.frames.push(Frame { params, ..Frame::default() });
    let mut result = Ok(());
    for stmt in &body.statements {
      result = self.statement(stmt, id);
      if result.is_err() {
        break;
      }
    }
    self.frames.pop();
    result
  }

  fn hook(&mut self, kind: &str, call: &js::CallExpression, block: usize) -> Lowered<()> {
    let arrow = self.arrow(call.arguments.first(), call.span)?;
    if !crate::params_of(&arrow.params).is_empty() {
      return Err(self.parsed.residue(arrow.span, "a hook takes no arguments: it is async and awaits what it runs"));
    }
    let steps = self.body(arrow)?;
    let hooks = match kind {
      "beforeAll" => &mut self.blocks[block].before_all,
      "afterAll" => &mut self.blocks[block].after_all,
      "beforeEach" => &mut self.blocks[block].before_each,
      _ => &mut self.blocks[block].after_each,
    };
    hooks.extend(steps);
    Ok(())
  }

  fn body(&self, arrow: &js::ArrowFunctionExpression) -> Lowered<Vec<(usize, Step)>> {
    let js::ArrowFunctionBody::FunctionBody(block) = &arrow.body else {
      return Err(self.parsed.residue(arrow.span, "a body must be a block"));
    };
    let lets: Vec<String> = self.frames.iter().flat_map(|f| f.lets.iter().cloned()).collect();
    let fns: Vec<String> = self.frames.iter().flat_map(|f| f.fns.iter().cloned()).collect();
    let mut tl = TestLowerer { lowerer: self.lowerer(), parsed: self.parsed, imports: self.imports, helpers: self.helpers, mocks: lets.clone(), lets, fns };
    let mut steps = Vec::new();
    for stmt in &block.statements {
      let line = self.line(stmt.span());
      let step = tl.stmt(stmt).map_err(|residue| match tl.lowerer.unbound.take() {
        Some(name) if self.imports.iter().any(|(local, _)| *local == name) => Residue {
          message: format!("a run of `{name}` inside a larger expression or a function the test declares"),
          hint: Some(format!("a body test is lowered, so a run stands alone: a statement, `const data = await {name}(c)`, or the whole value `expect` takes, `expect(await {name}(c))`")),
          ..residue
        },
        _ => residue,
      })?;
      steps.push((line, step));
    }
    Ok(steps)
  }

  fn finish(self, file: &str) -> TestFile {
    let only = self.asked.contains(&Asked::Only) || self.cases.iter().any(|(_, asked)| *asked == Asked::Only);
    let blocks = self.blocks;
    let asked_blocks = self.asked;
    let ancestors = |mut at: usize| {
      let mut out = vec![at];
      while let Some(parent) = blocks[at].parent {
        out.push(parent);
        at = parent;
      }
      out
    };
    let tests = self
      .cases
      .into_iter()
      .map(|(mut case, asked)| {
        let chain = ancestors(case.block);
        let skipped = asked == Asked::Skip || chain.iter().any(|b| asked_blocks[*b] == Asked::Skip);
        let chosen = asked == Asked::Only || chain.iter().any(|b| asked_blocks[*b] == Asked::Only);
        case.mode = match asked {
          Asked::Todo => Mode::Todo,
          _ if skipped || (only && !chosen) => Mode::Skip,
          _ => Mode::Run,
        };
        case
      })
      .collect();
    TestFile { file: file.to_owned(), blocks, tests }
  }
}

fn bound_names(params: &[Binding]) -> Vec<String> {
  params
    .iter()
    .flat_map(|p| match p {
      Binding::Name(name) => vec![name.clone()],
      Binding::Fields(fields) => fields.iter().map(|(_, local)| local.clone()).collect(),
    })
    .collect()
}

fn binding_of(parsed: &Parsed, pat: &js::BindingPattern) -> Lowered<Binding> {
  match pat {
    js::BindingPattern::BindingIdentifier(name) => Ok(Binding::Name(name.name.to_string())),
    js::BindingPattern::ObjectPattern(obj) => {
      let mut fields = Vec::new();
      for prop in crate::syntax::pat_props(obj) {
        let (field, local) = match prop {
          crate::syntax::PatProp::Assign(a) => (a.key.name.to_string(), a.key.name.to_string()),
          crate::syntax::PatProp::KeyValue(kv) => {
            let key = prop_name(&kv.key, kv.computed).ok_or_else(|| parsed.residue(kv.key.span(), "a computed field"))?;
            let js::BindingPattern::BindingIdentifier(local) = &kv.value else {
              return Err(parsed.residue(kv.value.span(), "a nested pattern"));
            };
            (key, local.name.to_string())
          }
          crate::syntax::PatProp::Rest(r) => return Err(parsed.residue(r.span, "a rest pattern")),
        };
        fields.push((field, local));
      }
      Ok(Binding::Fields(fields))
    }
    other => Err(parsed.residue(other.span(), "a pattern the runner does not bind")),
  }
}

/// `fn(impl)`, `vi.fn(impl)` or `jest.fn(impl)` as the mock function's first
/// answer; `None` for any other call.
fn mock_fn(lowerer: &mut Lowerer<'_>, call: &js::CallExpression) -> Lowered<Option<Option<Answer>>> {
  let Some(path) = callee_path(call) else { return Ok(None) };
  if !matches!(path.iter().map(String::as_str).collect::<Vec<_>>().as_slice(), ["fn"] | ["vi", "fn"] | ["jest", "fn"]) {
    return Ok(None);
  }
  match call.arguments.first().map(|a| crate::syntax::arg_expr(a)) {
    None => Ok(Some(None)),
    Some(js::Expression::ArrowFunctionExpression(arrow)) => Ok(Some(Some(Answer::Calls(lowerer.lambda(arrow)?)))),
    Some(other) => Err(lowerer.residue(other.span(), "a mock function's implementation is an arrow function")),
  }
}

struct TestLowerer<'a> {
  lowerer: Lowerer<'a>,
  parsed: &'a Parsed<'a>,
  imports: &'a [(String, Target)],
  helpers: &'a [String],
  /// Names a run may take as its ctx: the ones `ctx(...)` was bound to, plus every `let`, which a hook may have assigned one.
  mocks: Vec<String>,
  lets: Vec<String>,
  fns: Vec<String>,
}

impl<'a> TestLowerer<'a> {
  fn stmt(&mut self, stmt: &js::Statement) -> Lowered<Step> {
    match stmt {
      js::Statement::VariableDeclaration(var) => {
        if var.declarations.len() != 1 {
          return Err(self.lowerer.residue(var.span, "one binding per declaration"));
        }
        let decl = &var.declarations[0];
        let init = decl.init.as_ref().ok_or_else(|| self.lowerer.residue(decl.span, "a declaration without a value"))?;
        if let js::Expression::CallExpression(call) = unwrap_await(init) {
          if is_ident_call(call, "ctx") && self.helpers.iter().any(|h| h == "ctx") {
            let js::BindingPattern::BindingIdentifier(name) = &decl.id else {
              return Err(self.lowerer.residue(decl.id.span(), "`ctx(...)` must be bound to a name"));
            };
            let mock = self.mock(call)?;
            let name = name.name.to_string();
            self.bind(&name);
            self.mocks.push(name.clone());
            return Ok(Step::Mock { name, mock });
          }
          if let Some(answer) = mock_fn(&mut self.lowerer, call)? {
            let js::BindingPattern::BindingIdentifier(name) = &decl.id else {
              return Err(self.lowerer.residue(decl.id.span(), "a mock function must be bound to a name"));
            };
            let name = name.name.to_string();
            self.bind(&name);
            self.fns.push(name.clone());
            return Ok(Step::Fn { name, answer });
          }
          if let Some((target, ctx, input)) = self.run_target(call)? {
            let binding = binding_of(self.parsed, &decl.id)?;
            for name in bound_names(std::slice::from_ref(&binding)) {
              self.bind(&name);
            }
            return Ok(Step::Run { binding: Some(binding), target, ctx, input });
          }
          if matches!(&call.callee, js::Expression::Identifier(_)) {
            return Err(self.lowerer.residue(init.span(), "a `const` other than a value, `ctx(...)`, a mock function or a run of the loader or an action"));
          }
        }
        let js::BindingPattern::BindingIdentifier(name) = &decl.id else {
          return Err(self.lowerer.residue(decl.id.span(), "a pattern the runner does not bind"));
        };
        let name = name.name.to_string();
        let value = self.expected(init)?;
        self.bind(&name);
        Ok(Step::Let { name, value })
      }
      js::Statement::ExpressionStatement(expr_stmt) => {
        let expr = unwrap_await(&expr_stmt.expression);
        if let js::Expression::AssignmentExpression(assign) = expr {
          return self.assign(assign);
        }
        let js::Expression::CallExpression(call) = expr else {
          return Err(self.lowerer.residue(expr_stmt.span, "an expression statement other than a run, a mock function's answer or an assertion"));
        };
        if let Some((target, ctx, input)) = self.run_target(call)? {
          return Ok(Step::Run { binding: None, target, ctx, input });
        }
        if let Some(step) = self.mock_setter(call)? {
          return Ok(step);
        }
        if let Some(assertion) = self.expect(call)? {
          return Ok(Step::Assert(assertion));
        }
        self.assertion(call).map(Step::Assert)
      }
      other => Err(self.lowerer.residue(other.span(), "a statement a test cannot hold; a test is `ctx(...)`, runs, values, mock functions and assertions")),
    }
  }

  fn bind(&mut self, name: &str) {
    self.lowerer.scope.push((name.to_owned(), Expr::Var(name.to_owned())));
  }

  /// `c = ctx(...)`, `data = await load(c)` or `fetch = fn(...)`: a `let` the file or a `describe` declared, assigned in a hook or a test.
  fn assign(&mut self, assign: &js::AssignmentExpression) -> Lowered<Step> {
    let js::AssignmentTarget::AssignmentTargetIdentifier(target) = &assign.left else {
      return Err(self.lowerer.residue(assign.span, "an assignment other than to a `let` the file or a `describe` declared"));
    };
    let name = target.name.to_string();
    if !self.lets.contains(&name) {
      return Err(self.lowerer.residue(assign.span, format!("`{name}` is not a `let` the file or a `describe` declared")));
    }
    if assign.operator != js::AssignmentOperator::Assign {
      return Err(self.lowerer.residue(assign.span, "an assignment other than `=`"));
    }
    let right = unwrap_await(&assign.right);
    if let js::Expression::CallExpression(call) = right {
      if is_ident_call(call, "ctx") && self.helpers.iter().any(|h| h == "ctx") {
        let mock = self.mock(call)?;
        return Ok(Step::Mock { name, mock });
      }
      if let Some(answer) = mock_fn(&mut self.lowerer, call)? {
        self.fns.push(name.clone());
        return Ok(Step::Fn { name, answer });
      }
      if let Some((target, ctx, input)) = self.run_target(call)? {
        return Ok(Step::Run { binding: Some(Binding::Name(name)), target, ctx, input });
      }
    }
    Ok(Step::Let { name, value: self.expected(right)? })
  }

  /// `load(c)` or `addToCart(c)` for an imported name, else `None`. A
  /// `meta({ data })` takes its data where the others take their ctx, so it
  /// carries the ctx bound above it instead: a meta body may read the locale
  /// or the identity the way a loader does.
  fn run_target(&mut self, call: &js::CallExpression) -> Lowered<Option<(Target, String, Option<Expr>)>> {
    let callee = &call.callee;
    let js::Expression::Identifier(id) = callee else { return Ok(None) };
    let Some((_, target)) = self.imports.iter().find(|(local, _)| *local == id.name.as_str()) else { return Ok(None) };
    if let Target::Meta { .. } | Target::Store { .. } = target {
      let export = match target {
        Target::Store { .. } => "store",
        _ => "meta",
      };
      let data = self.data_arg(call, export)?;
      let ctx = self.mocks.last().cloned().ok_or_else(|| self.lowerer.residue(call.span, format!("a `{export}(...)` runs against the `ctx(...)` bound above it; this test binds none")))?;
      return Ok(Some((target.clone(), ctx, Some(data))));
    }
    if let Target::Paths { .. } = target {
      if call.arguments.is_empty() {
        let ctx = self.mocks.last().cloned().ok_or_else(|| self.lowerer.residue(call.span, "a `paths()` runs against the `ctx(...)` bound above it; this test binds none"))?;
        return Ok(Some((target.clone(), ctx, None)));
      }
    }
    let ctx = self.ctx_arg(call)?;
    Ok(Some((target.clone(), ctx, None)))
  }

  /// The `data` of a `meta({ data })` or a `store({ data })`, as the expression the runner evaluates.
  fn data_arg(&mut self, call: &js::CallExpression, export: &str) -> Lowered<Expr> {
    let Some(first) = call.arguments.first() else {
      return Err(self.lowerer.residue(call.span, format!("a `{export}(...)` takes `{{ data }}`")));
    };
    let js::Expression::ObjectExpression(obj) = crate::syntax::arg_expr(first) else {
      return Err(self.lowerer.residue(crate::syntax::arg_expr(first).span(), format!("a `{export}(...)` takes an object literal `{{ data }}`")));
    };
    for prop in &obj.properties {
      let js::ObjectPropertyKind::ObjectProperty(prop) = prop else { continue };
      match crate::syntax::prop_of(prop) {
        crate::syntax::Prop::Shorthand(id) if id.name.as_str() == "data" => return self.lowerer.expr(&prop.value),
        crate::syntax::Prop::KeyValue(kv) if prop_name(&kv.key, kv.computed).as_deref() == Some("data") => return self.lowerer.expr(&kv.value),
        _ => {}
      }
    }
    Err(self.lowerer.residue(obj.span, format!("a `{export}(...)` takes `{{ data }}`; this literal has no `data`")))
  }

  fn ctx_arg(&mut self, call: &js::CallExpression) -> Lowered<String> {
    let Some(first) = call.arguments.first() else {
      return Err(self.lowerer.residue(call.span, "a run takes the `ctx(...)` it runs against"));
    };
    let js::Expression::Identifier(id) = crate::syntax::arg_expr(first) else {
      return Err(self.lowerer.residue(crate::syntax::arg_expr(first).span(), "a run takes the name a `ctx(...)` was bound to"));
    };
    let name = id.name.to_string();
    if !self.mocks.contains(&name) {
      return Err(self.lowerer.residue(id.span, format!("`{name}` is not a `ctx(...)` bound above")));
    }
    Ok(name)
  }

  /// A run inside `expect(...)` for `.resolves` or `.rejects`: `load(c)` or `() => load(c)`.
  fn run_of(&mut self, expr: &js::Expression) -> Lowered<(Target, String, Option<Expr>)> {
    let run = match expr {
      js::Expression::ArrowFunctionExpression(arrow) => match &arrow.body {
        js::ArrowFunctionBody::FunctionBody(_) => return Err(self.lowerer.residue(arrow.span, "`.resolves` and `.rejects` take `load(c)` or `() => load(c)`")),
        e => unwrap_await(e.as_expression().expect("an arrow body is a block or an expression")),
      },
      other => unwrap_await(other),
    };
    let js::Expression::CallExpression(run) = run else {
      return Err(self.lowerer.residue(expr.span(), "`.resolves` and `.rejects` take `load(c)` or `() => load(c)`"));
    };
    self.run_target(run)?.ok_or_else(|| self.lowerer.residue(run.span, "`.resolves` and `.rejects` take a run of the loader or an action"))
  }

  /// `fetch.mockReturnValue(rows)` and the rest, `vi.clearAllMocks()` among them.
  fn mock_setter(&mut self, call: &js::CallExpression) -> Lowered<Option<Step>> {
    let Some(path) = callee_path(call) else { return Ok(None) };
    let [owner, method] = path.as_slice() else { return Ok(None) };
    if owner == "vi" || owner == "jest" {
      return Ok(match method.as_str() {
        "clearAllMocks" => Some(Step::Clear { name: String::new(), reset: false }),
        "resetAllMocks" | "restoreAllMocks" => Some(Step::Clear { name: String::new(), reset: true }),
        _ => None,
      });
    }
    if !self.fns.contains(owner) && !self.lets.contains(owner) {
      return Ok(None);
    }
    let arg = |this: &mut Self| -> Lowered<Expr> {
      let a = call.arguments.first().ok_or_else(|| this.lowerer.residue(call.span, format!("`{method}` takes a value")))?;
      this.expected(crate::syntax::arg_expr(a))
    };
    let (answer, once) = match method.as_str() {
      "mockReturnValue" | "mockResolvedValue" => (Answer::Returns(arg(self)?), false),
      "mockReturnValueOnce" | "mockResolvedValueOnce" => (Answer::Returns(arg(self)?), true),
      "mockRejectedValue" => (Answer::Fails(arg(self)?), false),
      "mockRejectedValueOnce" => (Answer::Fails(arg(self)?), true),
      "mockImplementation" | "mockImplementationOnce" => {
        let Some(js::Expression::ArrowFunctionExpression(arrow)) = call.arguments.first().map(|a| crate::syntax::arg_expr(a)) else {
          return Err(self.lowerer.residue(call.span, "a mock function's implementation is an arrow function"));
        };
        (Answer::Calls(self.lowerer.lambda(arrow)?), method == "mockImplementationOnce")
      }
      "mockClear" => return Ok(Some(Step::Clear { name: owner.clone(), reset: false })),
      "mockReset" | "mockRestore" => return Ok(Some(Step::Clear { name: owner.clone(), reset: true })),
      other => return Err(self.lowerer.residue(call.span, format!("`{other}`, which a body test's mock function does not take"))),
    };
    Ok(Some(Step::Answer { name: owner.clone(), answer, once }))
  }

  /// `expect(subject)` with `.not`, `.resolves` or `.rejects` and a matcher; `None` for any other call.
  fn expect(&mut self, call: &js::CallExpression) -> Lowered<Option<Assertion>> {
    let callee = &call.callee;
    let Some(member) = (callee).as_member_expression() else { return Ok(None) };
    let crate::syntax::MemberProp::Ident(matcher_name) = crate::syntax::prop(member) else { return Ok(None) };
    let mut not = false;
    let mut settles: Option<bool> = None;
    let mut at: &js::Expression = member.object();
    let expect_call = loop {
      match at {
        m_expr if m_expr.is_member_expression() => {
let m = m_expr.as_member_expression().expect("matched as a member");
          let crate::syntax::MemberProp::Ident(prop) = crate::syntax::prop(m) else { return Ok(None) };
          match prop.name.as_str() {
            "not" => not = !not,
            "resolves" => settles = Some(false),
            "rejects" => settles = Some(true),
            _ => return Ok(None),
          }
          at = m.object();
        }
        js::Expression::CallExpression(inner) if is_ident_call(inner, "expect") => break inner,
        _ => return Ok(None),
      }
    };
    let Some(arg) = expect_call.arguments.first() else {
      return Err(self.lowerer.residue(expect_call.span, "`expect` takes the value to check"));
    };
    let message = match expect_call.arguments.get(1).map(|a| crate::syntax::arg_expr(a)) {
      None => None,
      Some(js::Expression::StringLiteral(s)) => Some(crate::lossy(s.value)),
      Some(js::Expression::TemplateLiteral(tpl)) if tpl.expressions.is_empty() => Some(tpl.quasis.iter().map(|q| q.value.raw.to_string()).collect()),
      Some(other) => return Err(self.lowerer.residue(other.span(), "an expectation's message must be a string")),
    };
    let matcher = self.matcher(matcher_name.name.as_str(), call)?;
    if matches!(matcher, Matcher::Throw(_)) && settles != Some(true) {
      return Err(self.lowerer.residue(call.span, "`toThrow` reads a failed run: `await expect(load(c)).rejects.toThrow(\"invalid\")`"));
    }
    let subject = match settles {
      Some(rejects) => {
        let (target, ctx, input) = self.run_of(crate::syntax::arg_expr(arg))?;
        Subject::Settled { target, ctx, input, rejects }
      }
      None if matcher.reads_calls() => {
        let js::Expression::Identifier(id) = crate::syntax::arg_expr(arg) else {
          return Err(self.lowerer.residue(crate::syntax::arg_expr(arg).span(), format!("`{}` reads a mock function, named where `expect` takes it", matcher.name())));
        };
        Subject::Mock(id.name.to_string())
      }
      None => match crate::syntax::arg_expr(arg) {
        js::Expression::AwaitExpression(awaited) => match unwrap_await(&awaited.argument) {
          js::Expression::CallExpression(run) => match self.run_target(run)? {
            Some((target, ctx, input)) => Subject::Settled { target, ctx, input, rejects: false },
            None => Subject::Value(self.lowerer.expr(crate::syntax::arg_expr(arg))?),
          },
          _ => Subject::Value(self.lowerer.expr(crate::syntax::arg_expr(arg))?),
        },
        _ => Subject::Value(self.lowerer.expr(crate::syntax::arg_expr(arg))?),
      },
    };
    Ok(Some(Assertion::Expect { subject, not, matcher, message }))
  }

  fn matcher(&mut self, name: &str, call: &js::CallExpression) -> Lowered<Matcher> {
    let arg = |this: &mut Self, i: usize| -> Lowered<Expr> {
      let a = call.arguments.get(i).ok_or_else(|| this.lowerer.residue(call.span, format!("`{name}` takes more arguments")))?;
      this.expected(crate::syntax::arg_expr(a))
    };
    let optional = |this: &mut Self, i: usize| -> Lowered<Option<Expr>> { call.arguments.get(i).map(|a| this.expected(crate::syntax::arg_expr(a))).transpose() };
    let all = |this: &mut Self, from: usize| -> Lowered<Vec<Expr>> { call.arguments.iter().skip(from).map(|a| this.expected(crate::syntax::arg_expr(a))).collect() };
    Ok(match name {
      "toBe" => Matcher::Be(arg(self, 0)?),
      "toEqual" => Matcher::Equal(arg(self, 0)?),
      "toStrictEqual" => Matcher::StrictEqual(arg(self, 0)?),
      "toBeTruthy" => Matcher::Truthy,
      "toBeFalsy" => Matcher::Falsy,
      "toBeNull" => Matcher::Null,
      "toBeUndefined" => Matcher::Undefined,
      "toBeDefined" => Matcher::Defined,
      "toBeNaN" => Matcher::NaN,
      "toBeGreaterThan" => Matcher::GreaterThan(arg(self, 0)?),
      "toBeGreaterThanOrEqual" => Matcher::GreaterThanOrEqual(arg(self, 0)?),
      "toBeLessThan" => Matcher::LessThan(arg(self, 0)?),
      "toBeLessThanOrEqual" => Matcher::LessThanOrEqual(arg(self, 0)?),
      "toBeCloseTo" => Matcher::CloseTo(arg(self, 0)?, optional(self, 1)?),
      "toContain" => Matcher::Contain(arg(self, 0)?),
      "toContainEqual" => Matcher::ContainEqual(arg(self, 0)?),
      "toHaveLength" => Matcher::Length(arg(self, 0)?),
      "toHaveProperty" => Matcher::Property(arg(self, 0)?, optional(self, 1)?),
      "toMatch" => {
        let a = call.arguments.first().ok_or_else(|| self.lowerer.residue(call.span, "`toMatch` takes a string or a pattern"))?;
        Matcher::Match(self.pattern(crate::syntax::arg_expr(a))?)
      }
      "toMatchObject" => Matcher::MatchObject(arg(self, 0)?),
      "toBeTypeOf" => Matcher::TypeOf(arg(self, 0)?),
      "toBeOneOf" => Matcher::OneOf(arg(self, 0)?),
      "toThrow" | "toThrowError" => Matcher::Throw(match call.arguments.first() {
        Some(a) => Some(self.pattern(crate::syntax::arg_expr(a))?),
        None => None,
      }),
      "toHaveBeenCalled" | "toBeCalled" => Matcher::Called,
      "toHaveBeenCalledOnce" => Matcher::CalledOnce,
      "toHaveBeenCalledTimes" | "toBeCalledTimes" => Matcher::CalledTimes(arg(self, 0)?),
      "toHaveBeenCalledWith" | "toBeCalledWith" => Matcher::CalledWith(all(self, 0)?),
      "toHaveBeenCalledExactlyOnceWith" => Matcher::CalledExactlyOnceWith(all(self, 0)?),
      "toHaveBeenLastCalledWith" | "lastCalledWith" => Matcher::LastCalledWith(all(self, 0)?),
      "toHaveBeenNthCalledWith" | "nthCalledWith" => Matcher::NthCalledWith(arg(self, 0)?, all(self, 1)?),
      "toHaveReturned" | "toReturn" => Matcher::Returned,
      "toHaveReturnedTimes" | "toReturnTimes" => Matcher::ReturnedTimes(arg(self, 0)?),
      "toHaveReturnedWith" | "toReturnWith" => Matcher::ReturnedWith(arg(self, 0)?),
      "toHaveLastReturnedWith" | "lastReturnedWith" => Matcher::LastReturnedWith(arg(self, 0)?),
      other => return Err(self.lowerer.residue(call.span, format!("`{other}`, which a body test does not read; the matchers for markup are a page spec's"))),
    })
  }

  fn pattern(&mut self, expr: &js::Expression) -> Lowered<Pattern> {
    match expr {
      js::Expression::RegExpLiteral(re) => Ok(Pattern::Regex { source: re.regex.pattern.text.to_string(), flags: re.regex.flags.to_string() }),
      other => Ok(Pattern::Text(self.expected(other)?)),
    }
  }

  /// An expected value, where `expect.any(String)` and the other helpers may
  /// sit at any depth. A value holding none lowers the way any value does.
  fn expected(&mut self, expr: &js::Expression) -> Lowered<Expr> {
    if !holds_helper(expr) {
      return self.lowerer.expr(expr);
    }
    match expr {
      js::Expression::ParenthesizedExpression(p) => self.expected(&p.expression),
      js::Expression::CallExpression(call) => self.helper(call),
      js::Expression::ObjectExpression(obj) => {
        let mut entries = Vec::new();
        for prop in &obj.properties {
          match prop {
            js::ObjectPropertyKind::SpreadProperty(spread) => entries.push(Entry::Spread(self.lowerer.expr(&spread.argument)?)),
            js::ObjectPropertyKind::ObjectProperty(p) => match crate::syntax::prop_of(p) {
              crate::syntax::Prop::KeyValue(kv) => {
                let key = prop_name(&kv.key, kv.computed).ok_or_else(|| self.lowerer.residue(kv.key.span(), "a computed key beside a matcher"))?;
                entries.push(Entry::Field(key, self.expected(&kv.value)?));
              }
              crate::syntax::Prop::Shorthand(id) => {
                let read = self.lowerer.build().read(id);
                entries.push(Entry::Field(id.name.to_string(), self.lowerer.expr(&read)?));
              }
              other => return Err(self.lowerer.residue(other.span(), "an object entry other than `key: value` beside a matcher")),
            },
          }
        }
        Ok(Expr::Object(entries))
      }
      js::Expression::ArrayExpression(arr) => {
        let mut items = Vec::new();
        for elem in &arr.elements {
          match elem {
            js::ArrayExpressionElement::SpreadElement(spread) => items.push(Entry::Spread(self.lowerer.expr(&spread.argument)?)),
            js::ArrayExpressionElement::Elision(_) => items.push(Entry::Item(Expr::Lit(Lit::Null))),
            other => items.push(Entry::Item(self.expected(other.to_expression())?)),
          }
        }
        Ok(Expr::Array(items))
      }
      other => self.lowerer.expr(other),
    }
  }

  /// `expect.any(String)`, `expect.not.stringContaining("x")` and the rest, as a marked object the runner reads.
  fn helper(&mut self, call: &js::CallExpression) -> Lowered<Expr> {
    let path = callee_path(call).unwrap_or_default();
    let (not, name) = match path.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
      ["expect", "not", name] => (true, (*name).to_owned()),
      ["expect", name] => (false, (*name).to_owned()),
      _ => {
        let build = self.lowerer.build();
        return self.lowerer.expr(&js::Expression::CallExpression(build.boxed(build.copy(call))));
      }
    };
    let mut fields = vec![Entry::Field(EXPECT_MARK.to_owned(), Expr::lit_str(name.clone()))];
    if not {
      fields.push(Entry::Field("not".to_owned(), Expr::Lit(Lit::Bool(true))));
    }
    let first = call.arguments.first().map(|a| crate::syntax::arg_expr(a));
    match (name.as_str(), first) {
      ("anything", _) => {}
      ("any", Some(js::Expression::Identifier(id))) => fields.push(Entry::Field("of".to_owned(), Expr::lit_str(id.name.to_string()))),
      ("objectContaining" | "arrayContaining", Some(value)) => fields.push(Entry::Field("value".to_owned(), self.expected(value)?)),
      ("stringContaining", Some(value)) => fields.push(Entry::Field("value".to_owned(), self.lowerer.expr(value)?)),
      ("stringMatching", Some(js::Expression::RegExpLiteral(re))) => {
        fields.push(Entry::Field("source".to_owned(), Expr::lit_str(re.regex.pattern.text.to_string())));
        fields.push(Entry::Field("flags".to_owned(), Expr::lit_str(re.regex.flags.to_string())));
      }
      ("stringMatching", Some(value)) => fields.push(Entry::Field("value".to_owned(), self.lowerer.expr(value)?)),
      ("closeTo", Some(value)) => {
        fields.push(Entry::Field("value".to_owned(), self.lowerer.expr(value)?));
        if let Some(digits) = call.arguments.get(1) {
          fields.push(Entry::Field("digits".to_owned(), self.lowerer.expr(crate::syntax::arg_expr(digits))?));
        }
      }
      (other, _) => return Err(self.lowerer.residue(call.span, format!("`expect.{other}` with these arguments, which a body test does not read"))),
    }
    Ok(Expr::Object(fields))
  }

  fn assertion(&mut self, call: &js::CallExpression) -> Lowered<Assertion> {
    let callee = &call.callee;
    let Some(member) = (callee).as_member_expression() else {
      return Err(self.lowerer.residue(callee.span(), "a call other than a run, `expect(...)` or `assert.<method>(...)`"));
    };
    let is_assert = matches!(member.object(), js::Expression::Identifier(id) if id.name.as_str() == "assert") && self.helpers.iter().any(|h| h == "assert");
    if !is_assert {
      return Err(self.lowerer.residue(member.span(), "a call other than a run, `expect(...)` or `assert.<method>(...)`"));
    }
    let crate::syntax::MemberProp::Ident(method) = crate::syntax::prop(member) else {
      return Err(self.lowerer.residue(member.span(), "a computed assertion"));
    };
    let arg = |this: &mut Self, i: usize| -> Lowered<Expr> {
      let a = call.arguments.get(i).ok_or_else(|| this.lowerer.residue(call.span, format!("`assert.{}` takes more arguments", method.name)))?;
      this.lowerer.expr(crate::syntax::arg_expr(a))
    };
    match method.name.as_str() {
      "ok" => Ok(Assertion::Ok(arg(self, 0)?)),
      "equal" => Ok(Assertion::Equal(arg(self, 0)?, arg(self, 1)?)),
      "rejects" | "throws" => {
        let Some(first) = call.arguments.first() else {
          return Err(self.lowerer.residue(call.span, "`assert.rejects` takes a run"));
        };
        let (target, ctx, input) = self.run_of(crate::syntax::arg_expr(first))?;
        let kind = match call.arguments.get(1) {
          Some(a) => match crate::syntax::arg_expr(a) {
            js::Expression::StringLiteral(s) => Some(crate::lossy(s.value)),
            other => return Err(self.lowerer.residue(other.span(), "the expected kind must be a string")),
          },
          None => None,
        };
        Ok(Assertion::Rejects { target, ctx, input, kind })
      }
      "match" => {
        let subject = Subject::Value(arg(self, 0)?);
        let pattern = call.arguments.get(1).ok_or_else(|| self.lowerer.residue(call.span, "`assert.match` takes a string or a pattern"))?;
        let matcher = Matcher::Match(self.pattern(crate::syntax::arg_expr(pattern))?);
        let message = match call.arguments.get(2).map(|a| crate::syntax::arg_expr(a)) {
          None => None,
          Some(js::Expression::StringLiteral(s)) => Some(crate::lossy(s.value)),
          Some(other) => return Err(self.lowerer.residue(other.span(), "an assertion's message must be a string")),
        };
        Ok(Assertion::Expect { subject, not: false, matcher, message })
      }
      other => Err(self.lowerer.residue(member.span(), format!("`assert.{other}`; the assertions are `ok`, `equal`, `match`, `rejects` and `throws`"))),
    }
  }

  /// `ctx({ session, services, input, params, query, identity, locale, path })`.
  fn mock(&mut self, call: &js::CallExpression) -> Lowered<Mock> {
    let mut mock = Mock::default();
    let Some(first) = call.arguments.first() else { return Ok(mock) };
    let js::Expression::ObjectExpression(obj) = crate::syntax::arg_expr(first) else {
      return Err(self.lowerer.residue(crate::syntax::arg_expr(first).span(), "`ctx` takes an object literal"));
    };
    for prop in &obj.properties {
      let (key, value) = self.key_value(prop)?;
      match key.as_str() {
        "session" => mock.session = self.entries("session", value)?,
        "params" => mock.params = self.entries("params", value)?,
        "query" => mock.query = self.entries("query", value)?,
        "input" | "request" => mock.input = Some(self.lowerer.expr(value)?),
        "identity" => mock.identity = Some(self.lowerer.expr(value)?),
        "locale" => mock.locale = Some(self.lowerer.expr(value)?),
        "locales" => mock.locales = Some(self.lowerer.expr(value)?),
        "path" => mock.path = Some(self.lowerer.expr(value)?),
        "host" => mock.host = Some(self.lowerer.expr(value)?),
        "origin" => mock.origin = Some(self.lowerer.expr(value)?),
        "config" => mock.config = self.entries("config", value)?,
        "services" => {
          let js::Expression::ObjectExpression(services) = value else {
            return Err(self.lowerer.residue(value.span(), "`services` must be an object of services"));
          };
          for service in &services.properties {
            let (name, methods) = self.key_value(service)?;
            let js::Expression::ObjectExpression(methods) = methods else {
              return Err(self.lowerer.residue(methods.span(), format!("`services.{name}` must be an object of methods")));
            };
            for method in &methods.properties {
              let (method_name, body) = match method {
                js::ObjectPropertyKind::ObjectProperty(p) => match crate::syntax::prop_of(p) {
                  crate::syntax::Prop::Shorthand(id) => (id.name.to_string(), None),
                  _ => {
                    let (k, v) = self.key_value(method)?;
                    (k, Some(v))
                  }
                },
                js::ObjectPropertyKind::SpreadProperty(s) => return Err(self.lowerer.residue(s.span, "a spread in a mock")),
              };
              let named = match body {
                None => Some(method_name.clone()),
                Some(js::Expression::Identifier(id)) if self.fns.contains(&id.name.to_string()) || self.lets.contains(&id.name.to_string()) => Some(id.name.to_string()),
                _ => None,
              };
              if let Some(fn_name) = named {
                mock.mock_fns.push((name.clone(), method_name, fn_name));
                continue;
              }
              let lambda = match body.expect("a body when no mock function is named") {
                js::Expression::ArrowFunctionExpression(arrow) => self.lowerer.lambda(arrow)?,
                other => Expr::Lambda { params: Vec::new(), body: Box::new(self.lowerer.expr(other)?) },
              };
              mock.services.push((name.clone(), method_name, lambda));
            }
          }
        }
        other => return Err(self.lowerer.residue(value.span(), format!("`{other}` is not a part of a mocked context"))),
      }
    }
    Ok(mock)
  }

  fn entries(&mut self, part: &str, value: &js::Expression) -> Lowered<Vec<(String, Expr)>> {
    let js::Expression::ObjectExpression(obj) = value else {
      return Err(self.lowerer.residue(value.span(), format!("`{part}` in a mocked context is an object literal written in the test; {} is not read", found(value))));
    };
    let mut out = Vec::new();
    for prop in &obj.properties {
      let (key, value) = self.key_value(prop)?;
      out.push((key, self.lowerer.expr(value)?));
    }
    Ok(out)
  }

  fn key_value<'p>(&self, prop: &'p js::ObjectPropertyKind<'p>) -> Lowered<(String, &'p js::Expression<'p>)> {
    let js::ObjectPropertyKind::ObjectProperty(p) = prop else {
      return Err(self.parsed.residue(prop.span(), "a spread in a mock"));
    };
    match crate::syntax::prop_of(p) {
      crate::syntax::Prop::KeyValue(kv) => {
        let key = prop_name(&kv.key, kv.computed).ok_or_else(|| self.parsed.residue(kv.key.span(), "a computed key in a mock"))?;
        Ok((key, &kv.value))
      }
      other => Err(self.parsed.residue(other.span(), "a mock entry must be `key: value`")),
    }
  }
}

/// What a mocked context's part was given in place of an object literal.
fn found(expr: &js::Expression) -> &'static str {
  match expr {
    js::Expression::ParenthesizedExpression(p) => found(&p.expression),
    js::Expression::CallExpression(_) => "a call",
    js::Expression::Identifier(_) => "a name bound elsewhere",
    __expr if __expr.is_member_expression() => { let _ = __expr.as_member_expression().expect("matched as a member"); "a property read" },
    js::Expression::ConditionalExpression(_) => "a conditional",
    js::Expression::AwaitExpression(_) => "an awaited value",
    js::Expression::ArrayExpression(_) => "a list",
    e if e.is_literal() => "a single value",
    js::Expression::TemplateLiteral(_) => "a single value",
    _ => "this expression",
  }
}

/// Whether an expected value holds a call to one of `expect`'s helpers, at any depth.
fn holds_helper(expr: &js::Expression) -> bool {
  match expr {
    js::Expression::ParenthesizedExpression(p) => holds_helper(&p.expression),
    js::Expression::CallExpression(call) => callee_path(call).is_some_and(|p| p.first().map(String::as_str) == Some("expect") && p.len() >= 2),
    js::Expression::ObjectExpression(obj) => obj.properties.iter().any(|p| match p {
      js::ObjectPropertyKind::ObjectProperty(p) => matches!(crate::syntax::prop_of(p), crate::syntax::Prop::KeyValue(kv) if holds_helper(&kv.value)),
      js::ObjectPropertyKind::SpreadProperty(_) => false,
    }),
    js::Expression::ArrayExpression(arr) => arr.elements.iter().any(|e| match e {
      js::ArrayExpressionElement::Elision(_) => false,
      js::ArrayExpressionElement::SpreadElement(s) => holds_helper(&s.argument),
      other => holds_helper(other.to_expression()),
    }),
    _ => false,
  }
}

fn unwrap_await<'e>(expr: &'e js::Expression<'e>) -> &'e js::Expression<'e> {
  match expr {
    js::Expression::AwaitExpression(a) => unwrap_await(&a.argument),
    js::Expression::ParenthesizedExpression(p) => unwrap_await(&p.expression),
    other => other,
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use snapfire_fsr_ir::ast::Lit;

  #[test]
  fn a_test_file_lowers_to_mock_run_and_assert_steps() {
    let source = r#"
import { load } from "./page.loader";
import { addToCart } from "./actions";
import { assert, ctx, test } from "@snapfire/fsr/testing";

test("held lines carry the catalog's names", async () => {
  const c = ctx({
    session: { cart: { "1": 2n } },
    services: { shopping: { listProducts: () => [{ id: 1n, name: "Filament" }] } },
  });
  const { lines } = await load(c);
  assert.equal(lines, [{ id: 1n, name: "Filament", quantity: 2n }]);
  assert.ok(c.trace.calls.length === 1);
});

test("an empty cart cannot check out", async () => {
  const c = ctx({ input: { product_id: 1n, quantity: 0n } });
  await addToCart(c);
  await assert.rejects(addToCart(c), "invalid");
});
"#;
    let file = lower_tests("routes/cart/loader.test.ts", source).unwrap();
    assert_eq!(file.tests.len(), 2);
    let from_folder = lower_tests("tests/cart/loader.test.ts", &source.replace("\"./page.loader\"", "\"../../routes/cart/page.loader\"").replace("\"./actions\"", "\"@routes/cart/actions\"")).unwrap();
    assert_eq!(from_folder.tests[0].steps[1].1, file.tests[0].steps[1].1, "a test under tests/ names the same loader");
    assert_eq!(from_folder.tests[1].steps[1].1, file.tests[1].steps[1].1, "an alias names the same actions");
    let first = &file.tests[0];
    assert_eq!(first.name, "held lines carry the catalog's names");
    let Step::Mock { name, mock } = &first.steps[0].1 else { panic!("{:?}", first.steps[0]) };
    assert_eq!(name, "c");
    assert_eq!(mock.session.len(), 1);
    assert!(matches!(&mock.services[0], (s, m, Expr::Lambda { params, .. }) if s == "shopping" && m == "listProducts" && params.is_empty()));
    assert_eq!(first.steps[1].1, Step::Run { binding: Some(Binding::Fields(vec![("lines".to_owned(), "lines".to_owned())])), target: Target::Loader { file: "routes/cart/page.loader.ts".to_owned() }, ctx: "c".to_owned(), input: None });
    assert!(matches!(&first.steps[2].1, Step::Assert(Assertion::Equal(Expr::Var(v), Expr::Array(_))) if v == "lines"));
    assert!(matches!(&first.steps[3].1, Step::Assert(Assertion::Ok(_))));
    let second = &file.tests[1];
    let Step::Mock { mock, .. } = &second.steps[0].1 else { panic!() };
    assert!(matches!(&mock.input, Some(Expr::Object(_))));
    assert_eq!(second.steps[1].1, Step::Run { binding: None, target: Target::Action { file: "routes/cart/actions.ts".to_owned(), export: "addToCart".to_owned() }, ctx: "c".to_owned(), input: None });
    assert_eq!(second.steps[2].1, Step::Assert(Assertion::Rejects { target: Target::Action { file: "routes/cart/actions.ts".to_owned(), export: "addToCart".to_owned() }, ctx: "c".to_owned(), input: None, kind: Some("invalid".to_owned()) }));
    let _ = Lit::Null;
  }

  #[test]
  fn a_statement_outside_the_dialect_fails_with_its_line() {
    let source = "import { load } from \"./page.loader\";\nimport { assert, ctx, test } from \"@snapfire/fsr/testing\";\ntest(\"x\", async () => {\n  const c = ctx({});\n  const r = await load(c);\n  console.log(r);\n});\n";
    let err = lower_tests("routes/index/loader.test.ts", source).unwrap_err();
    assert_eq!(err.to_string(), "routes/index/loader.test.ts:6:3: a call other than a run, `expect(...)` or `assert.<method>(...)`");
    let source = "import { assert, ctx, test } from \"@snapfire/fsr/testing\";\ntest(\"x\", async () => {\n  const r = await load(c);\n});\n";
    let err = lower_tests("routes/index/loader.test.ts", source).unwrap_err();
    assert!(err.to_string().contains("a `const` other than"), "{err}");
  }

  #[test]
  fn blocks_hooks_tables_expectations_and_mock_functions_lower_with_their_places() {
    let source = r#"
import { load } from "./page.loader";
import { addToCart } from "./actions";
import { ctx, describe, expect, fn, it, test, beforeEach } from "@snapfire/fsr/testing";

const listProducts = fn(() => [{ id: 1n, name: "Filament" }]);
let c;

beforeEach(() => {
  c = ctx({ services: { shopping: { listProducts } } });
});

describe("the cart", () => {
  it("reads the catalog once", async () => {
    const { lines } = await load(c);
    expect(lines).toEqual([expect.objectContaining({ name: expect.any(String) })]);
    expect(listProducts).toHaveBeenCalledTimes(1);
    listProducts.mockReturnValueOnce([]);
  });

  test.each([[1n, "one"], [2n, "two"]])("%s is %s", async (n, word) => {
    expect(word).toMatch(/o/);
    expect(n).not.toBe(3);
  });

  it.skip("is skipped", async () => {});
  test.todo("comes later");
});

test("a failed add names its kind", async () => {
  await expect(addToCart(c)).rejects.toThrow("invalid");
});
"#;
    let file = lower_tests("routes/cart/loader.test.ts", source).unwrap();
    assert_eq!(file.blocks.len(), 2);
    assert!(matches!(&file.blocks[0].before_each[0].1, Step::Fn { name, answer: Some(Answer::Calls(_)) } if name == "listProducts"), "a module-level mock function starts every test fresh");
    let Step::Mock { name, mock } = &file.blocks[0].before_each[1].1 else { panic!("{:?}", file.blocks[0].before_each) };
    assert_eq!(name, "c");
    assert_eq!(mock.mock_fns, vec![("shopping".to_owned(), "listProducts".to_owned(), "listProducts".to_owned())]);
    let names: Vec<(&str, usize, Mode)> = file.tests.iter().map(|t| (t.name.as_str(), t.block, t.mode)).collect();
    assert_eq!(names, vec![("reads the catalog once", 1, Mode::Run), ("%s is %s", 1, Mode::Run), ("is skipped", 1, Mode::Skip), ("comes later", 1, Mode::Todo), ("a failed add names its kind", 0, Mode::Run)]);
    assert_eq!(file.full_name(&file.tests[0]), "the cart > reads the catalog once");
    let steps = &file.tests[0].steps;
    let Step::Assert(Assertion::Expect { subject: Subject::Value(Expr::Var(v)), not: false, matcher: Matcher::Equal(Expr::Array(items)), message: None }) = &steps[1].1 else { panic!("{:?}", steps[1]) };
    assert_eq!(v, "lines");
    assert!(matches!(&items[0], Entry::Item(Expr::Object(fields)) if matches!(&fields[0], Entry::Field(k, _) if k == EXPECT_MARK)));
    assert!(matches!(&steps[2].1, Step::Assert(Assertion::Expect { subject: Subject::Mock(n), matcher: Matcher::CalledTimes(_), .. }) if n == "listProducts"));
    assert!(matches!(&steps[3].1, Step::Answer { name, once: true, answer: Answer::Returns(_) } if name == "listProducts"));
    let each = file.tests[1].each.as_ref().unwrap();
    assert_eq!(each.params, vec![Binding::Name("n".to_owned()), Binding::Name("word".to_owned())]);
    assert!(matches!(&file.tests[1].steps[0].1, Step::Assert(Assertion::Expect { matcher: Matcher::Match(Pattern::Regex { source, .. }), .. }) if source == "o"));
    assert!(matches!(&file.tests[1].steps[1].1, Step::Assert(Assertion::Expect { not: true, matcher: Matcher::Be(_), .. })));
    assert!(matches!(&file.tests[4].steps[0].1, Step::Assert(Assertion::Expect { subject: Subject::Settled { rejects: true, .. }, matcher: Matcher::Throw(Some(Pattern::Text(_))), .. })));
  }

  #[test]
  fn assert_match_lowers_to_an_expectation_to_match() {
    let source = r#"
import { load } from "./page.loader";
import { assert, ctx, test } from "@snapfire/fsr/testing";

test("names the page", async () => {
  const c = ctx({});
  const { title } = await load(c);
  assert.match(title, /cart/i, "the title names the cart");
  assert.match(title, "Cart");
});
"#;
    let file = lower_tests("routes/cart/loader.test.ts", source).unwrap();
    let steps = &file.tests[0].steps;
    assert!(
      matches!(&steps[2].1, Step::Assert(Assertion::Expect { subject: Subject::Value(Expr::Var(v)), not: false, matcher: Matcher::Match(Pattern::Regex { source, flags }), message: Some(m) }) if v == "title" && source == "cart" && flags == "i" && m == "the title names the cart"),
      "{:?}",
      steps[2]
    );
    assert!(matches!(&steps[3].1, Step::Assert(Assertion::Expect { matcher: Matcher::Match(Pattern::Text(_)), message: None, .. })), "{:?}", steps[3]);
  }

  #[test]
  fn an_only_skips_every_test_it_does_not_cover_and_a_template_table_rows_objects() {
    let source = r#"
import { describe, expect, test } from "@snapfire/fsr/testing";

test("left out", async () => {
  expect(1).toBe(1);
});

describe.only("chosen", () => {
  test.each`
    word      | length
    ${"leek"} | ${4}
  `("$word has $length letters", async ({ word, length }) => {
    expect(word).toHaveLength(length);
  });
});
"#;
    let file = lower_tests("tests/words.test.ts", source).unwrap();
    assert_eq!(file.tests[0].mode, Mode::Skip);
    assert_eq!(file.tests[1].mode, Mode::Run);
    let each = file.tests[1].each.as_ref().unwrap();
    assert_eq!(each.params, vec![Binding::Fields(vec![("word".to_owned(), "word".to_owned()), ("length".to_owned(), "length".to_owned())])]);
    let Expr::Array(rows) = &each.table else { panic!("{:?}", each.table) };
    assert!(matches!(&rows[0], Entry::Item(Expr::Object(fields)) if fields.len() == 2));
  }
}
