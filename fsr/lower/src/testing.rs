//! Reads a `*.test.ts` file into test cases the runner replays through the
//! interpreter. The dialect is `_private_docs/TESTING.md` section 2: `test`
//! blocks whose statements are a `ctx({ ... })` mock, a `load` or action run
//! and assertions, plus module-level `const` fixtures the tests share.
//! Anything else fails the file with its line, since a test that silently did
//! less than it says is worse than none.

use snapfire_fsr_ir::ast::Expr;
use swc_core::common::Spanned;
use swc_core::ecma::ast as js;

use crate::{Lowered, LowerError, Lowerer, Parsed, SessionDefaults, parse, prop_name};

/// Where a run goes: the loader module beside the test or one export of the
/// actions module. Paths are relative to the app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
  Loader { file: String },
  Action { file: String, export: String },
  Handler { file: String, export: String },
}

/// The `ctx({ ... })` literal, each part an expression the runner evaluates.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Mock {
  pub session: Vec<(String, Expr)>,
  /// `(service, method, lambda)`; a value that is not a function is a lambda of no parameters.
  pub services: Vec<(String, String, Expr)>,
  pub input: Option<Expr>,
  pub params: Vec<(String, Expr)>,
  pub query: Vec<(String, Expr)>,
  pub identity: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Binding {
  Name(String),
  /// `const { a, b: local } = ...`: field, local name.
  Fields(Vec<(String, String)>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Assertion {
  Ok(Expr),
  Equal(Expr, Expr),
  /// The run must fail, with this kind when one is named.
  Rejects { target: Target, ctx: String, kind: Option<String> },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Step {
  Mock { name: String, mock: Mock },
  Run { binding: Option<Binding>, target: Target, ctx: String },
  Assert(Assertion),
}

#[derive(Debug, Clone, PartialEq)]
pub struct TestCase {
  pub name: String,
  pub line: usize,
  pub steps: Vec<(usize, Step)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TestFile {
  pub file: String,
  pub tests: Vec<TestCase>,
}

/// Lowers `source`, the test file at `file` relative to the app.
pub fn lower_tests(file: &str, source: &str) -> Result<TestFile, LowerError> {
  let parsed = parse(file, source)?;
  let defaults = SessionDefaults::new();
  let mut imports: Vec<(String, Target)> = Vec::new();
  let mut helpers: Vec<String> = Vec::new();
  for item in &parsed.module.body {
    let js::ModuleItem::ModuleDecl(js::ModuleDecl::Import(import)) = item else { continue };
    if import.type_only {
      continue;
    }
    let source = import.src.value.to_atom_lossy().to_string();
    for spec in &import.specifiers {
      let js::ImportSpecifier::Named(named) = spec else {
        return Err(parsed.residue(spec.span(), "a default or namespace import in a test").into());
      };
      if named.is_type_only {
        continue;
      }
      let local = named.local.sym.to_string();
      let imported = match &named.imported {
        Some(js::ModuleExportName::Ident(id)) => id.sym.to_string(),
        Some(js::ModuleExportName::Str(s)) => s.value.to_atom_lossy().to_string(),
        None => local.clone(),
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
        ("loader", "load") => Target::Loader { file: target_file },
        ("actions", export) => Target::Action { file: target_file, export: export.to_owned() },
        ("route", method) if crate::HANDLER_METHODS.contains(&method) => Target::Handler { file: target_file, export: method.to_owned() },
        _ => return Err(parsed.residue(named.span, format!("`{imported}` from `{source}`; a test imports `load` from a route's `loader`, an action from its `actions` or a method from its `route`")).into()),
      };
      imports.push((local, target));
    }
  }

  let mut tests = Vec::new();
  let mut fixtures: Vec<(String, Expr)> = Vec::new();
  for item in &parsed.module.body {
    if let js::ModuleItem::Stmt(js::Stmt::Decl(js::Decl::Var(var))) = item {
      let mut lowerer = Lowerer::new(&parsed, &defaults);
      lowerer.globals = fixtures.clone();
      for decl in &var.decls {
        let js::Pat::Ident(name) = &decl.name else {
          return Err(parsed.residue(decl.name.span(), "a fixture must be one name").into());
        };
        let init = decl.init.as_deref().ok_or_else(|| parsed.residue(decl.span, "a fixture without a value"))?;
        let expr = lowerer.expr(init)?;
        fixtures.push((name.id.sym.to_string(), expr.clone()));
        lowerer.globals.push((name.id.sym.to_string(), expr));
      }
      continue;
    }
    let js::ModuleItem::Stmt(js::Stmt::Expr(stmt)) = item else {
      if let js::ModuleItem::ModuleDecl(js::ModuleDecl::Import(_)) = item {
        continue;
      }
      return Err(parsed.residue(item.span(), "a statement outside `test(...)`; a file holds imports, `const` fixtures and tests").into());
    };
    let js::Expr::Call(call) = &*stmt.expr else {
      return Err(parsed.residue(stmt.span, "a statement outside `test(...)`").into());
    };
    if !is_ident_call(call, "test") {
      return Err(parsed.residue(call.span, "a call other than `test(name, async () => { ... })` at the top level").into());
    }
    let (Some(name), Some(body)) = (call.args.first(), call.args.get(1)) else {
      return Err(parsed.residue(call.span, "`test` takes a name and a function").into());
    };
    let js::Expr::Lit(js::Lit::Str(name)) = &*name.expr else {
      return Err(parsed.residue(name.expr.span(), "a test name must be a string").into());
    };
    let js::Expr::Arrow(arrow) = &*body.expr else {
      return Err(parsed.residue(body.expr.span(), "a test body must be an arrow function").into());
    };
    let js::ArrowFunctionBody::FunctionBody(block) = &*arrow.body else {
      return Err(parsed.residue(arrow.span, "a test body must be a block").into());
    };
    let line = parsed.cm.lookup_char_pos(call.span.lo).line;
    let mut lowerer = Lowerer::new(&parsed, &defaults);
    lowerer.globals = fixtures.clone();
    let mut tl = TestLowerer { lowerer, parsed: &parsed, imports: &imports, helpers: &helpers, mocks: Vec::new() };
    let mut steps = Vec::new();
    for stmt in &block.stmts {
      let line = parsed.cm.lookup_char_pos(stmt.span().lo).line;
      steps.push((line, tl.stmt(stmt)?));
    }
    tests.push(TestCase { name: name.value.to_atom_lossy().to_string(), line, steps });
  }
  Ok(TestFile { file: file.to_owned(), tests })
}

fn is_ident_call(call: &js::CallExpr, name: &str) -> bool {
  matches!(&call.callee, js::Callee::Expr(e) if matches!(&**e, js::Expr::Ident(id) if id.sym.as_ref() == name))
}

struct TestLowerer<'a> {
  lowerer: Lowerer<'a>,
  parsed: &'a Parsed,
  imports: &'a [(String, Target)],
  helpers: &'a [String],
  mocks: Vec<String>,
}

impl<'a> TestLowerer<'a> {
  fn stmt(&mut self, stmt: &js::Stmt) -> Lowered<Step> {
    match stmt {
      js::Stmt::Decl(js::Decl::Var(var)) => {
        if var.decls.len() != 1 {
          return Err(self.lowerer.residue(var.span, "one binding per declaration"));
        }
        let decl = &var.decls[0];
        let init = decl.init.as_deref().ok_or_else(|| self.lowerer.residue(decl.span, "a declaration without a value"))?;
        if let js::Expr::Call(call) = unwrap_await(init) {
          if is_ident_call(call, "ctx") && self.helpers.iter().any(|h| h == "ctx") {
            let js::Pat::Ident(name) = &decl.name else {
              return Err(self.lowerer.residue(decl.name.span(), "`ctx(...)` must be bound to a name"));
            };
            let mock = self.mock(call)?;
            let name = name.id.sym.to_string();
            self.lowerer.scope.push((name.clone(), Expr::Var(name.clone())));
            self.mocks.push(name.clone());
            return Ok(Step::Mock { name, mock });
          }
          if let Some((target, ctx)) = self.run_target(call)? {
            let binding = match &decl.name {
              js::Pat::Ident(name) => {
                let name = name.id.sym.to_string();
                self.lowerer.scope.push((name.clone(), Expr::Var(name.clone())));
                Binding::Name(name)
              }
              js::Pat::Object(obj) => {
                let mut fields = Vec::new();
                for prop in &obj.props {
                  let (field, local) = match prop {
                    js::ObjectPatProp::Assign(a) => (a.key.id.sym.to_string(), a.key.id.sym.to_string()),
                    js::ObjectPatProp::KeyValue(kv) => {
                      let key = prop_name(&kv.key).ok_or_else(|| self.lowerer.residue(kv.key.span(), "a computed field"))?;
                      let js::Pat::Ident(local) = &*kv.value else {
                        return Err(self.lowerer.residue(kv.value.span(), "a nested pattern"));
                      };
                      (key, local.id.sym.to_string())
                    }
                    js::ObjectPatProp::Rest(r) => return Err(self.lowerer.residue(r.span, "a rest pattern")),
                  };
                  self.lowerer.scope.push((local.clone(), Expr::Var(local.clone())));
                  fields.push((field, local));
                }
                Binding::Fields(fields)
              }
              other => return Err(self.lowerer.residue(other.span(), "a pattern the runner does not bind")),
            };
            return Ok(Step::Run { binding: Some(binding), target, ctx });
          }
        }
        Err(self.lowerer.residue(init.span(), "a `const` other than `ctx(...)` or a run of the loader or an action"))
      }
      js::Stmt::Expr(expr_stmt) => {
        let js::Expr::Call(call) = unwrap_await(&expr_stmt.expr) else {
          return Err(self.lowerer.residue(expr_stmt.span, "an expression statement other than a run or an assertion"));
        };
        if let Some((target, ctx)) = self.run_target(call)? {
          return Ok(Step::Run { binding: None, target, ctx });
        }
        self.assertion(call).map(Step::Assert)
      }
      other => Err(self.lowerer.residue(other.span(), "a statement a test cannot hold; a test is `ctx(...)`, runs and assertions")),
    }
  }

  /// `load(c)` or `addToCart(c)` for an imported name, else `None`.
  fn run_target(&mut self, call: &js::CallExpr) -> Lowered<Option<(Target, String)>> {
    let js::Callee::Expr(callee) = &call.callee else { return Ok(None) };
    let js::Expr::Ident(id) = &**callee else { return Ok(None) };
    let Some((_, target)) = self.imports.iter().find(|(local, _)| *local == id.sym.as_ref()) else { return Ok(None) };
    let ctx = self.ctx_arg(call)?;
    Ok(Some((target.clone(), ctx)))
  }

  fn ctx_arg(&mut self, call: &js::CallExpr) -> Lowered<String> {
    let Some(first) = call.args.first() else {
      return Err(self.lowerer.residue(call.span, "a run takes the `ctx(...)` it runs against"));
    };
    let js::Expr::Ident(id) = &*first.expr else {
      return Err(self.lowerer.residue(first.expr.span(), "a run takes the name a `ctx(...)` was bound to"));
    };
    let name = id.sym.to_string();
    if !self.mocks.contains(&name) {
      return Err(self.lowerer.residue(id.span, format!("`{name}` is not a `ctx(...)` bound above")));
    }
    Ok(name)
  }

  fn assertion(&mut self, call: &js::CallExpr) -> Lowered<Assertion> {
    let js::Callee::Expr(callee) = &call.callee else {
      return Err(self.lowerer.residue(call.span, "a call the runner does not know"));
    };
    let js::Expr::Member(member) = &**callee else {
      return Err(self.lowerer.residue(callee.span(), "a call other than `assert.<method>(...)`"));
    };
    let is_assert = matches!(&*member.obj, js::Expr::Ident(id) if id.sym.as_ref() == "assert") && self.helpers.iter().any(|h| h == "assert");
    if !is_assert {
      return Err(self.lowerer.residue(member.span, "a call other than `assert.<method>(...)`"));
    }
    let js::MemberProp::Ident(method) = &member.prop else {
      return Err(self.lowerer.residue(member.span, "a computed assertion"));
    };
    let arg = |this: &mut Self, i: usize| -> Lowered<Expr> {
      let a = call.args.get(i).ok_or_else(|| this.lowerer.residue(call.span, format!("`assert.{}` takes more arguments", method.sym)))?;
      this.lowerer.expr(&a.expr)
    };
    match method.sym.as_ref() {
      "ok" => Ok(Assertion::Ok(arg(self, 0)?)),
      "equal" => Ok(Assertion::Equal(arg(self, 0)?, arg(self, 1)?)),
      "rejects" | "throws" => {
        let Some(first) = call.args.first() else {
          return Err(self.lowerer.residue(call.span, "`assert.rejects` takes a run"));
        };
        let run = match &*first.expr {
          js::Expr::Arrow(arrow) => match &*arrow.body {
            js::ArrowFunctionBody::Expr(e) => unwrap_await(e),
            js::ArrowFunctionBody::FunctionBody(_) => return Err(self.lowerer.residue(arrow.span, "`assert.rejects` takes `load(c)` or `() => load(c)`")),
          },
          other => unwrap_await(other),
        };
        let js::Expr::Call(run) = run else {
          return Err(self.lowerer.residue(first.expr.span(), "`assert.rejects` takes `load(c)` or `() => load(c)`"));
        };
        let Some((target, ctx)) = self.run_target(run)? else {
          return Err(self.lowerer.residue(run.span, "`assert.rejects` takes a run of the loader or an action"));
        };
        let kind = match call.args.get(1) {
          Some(a) => match &*a.expr {
            js::Expr::Lit(js::Lit::Str(s)) => Some(s.value.to_atom_lossy().to_string()),
            other => return Err(self.lowerer.residue(other.span(), "the expected kind must be a string")),
          },
          None => None,
        };
        Ok(Assertion::Rejects { target, ctx, kind })
      }
      other => Err(self.lowerer.residue(member.span, format!("`assert.{other}`; the assertions are `ok`, `equal` and `rejects`"))),
    }
  }

  /// `ctx({ session, services, input, params, query, identity })`.
  fn mock(&mut self, call: &js::CallExpr) -> Lowered<Mock> {
    let mut mock = Mock::default();
    let Some(first) = call.args.first() else { return Ok(mock) };
    let js::Expr::Object(obj) = &*first.expr else {
      return Err(self.lowerer.residue(first.expr.span(), "`ctx` takes an object literal"));
    };
    for prop in &obj.props {
      let (key, value) = self.key_value(prop)?;
      match key.as_str() {
        "session" => mock.session = self.entries(value)?,
        "params" => mock.params = self.entries(value)?,
        "query" => mock.query = self.entries(value)?,
        "input" => mock.input = Some(self.lowerer.expr(value)?),
        "identity" => mock.identity = Some(self.lowerer.expr(value)?),
        "services" => {
          let js::Expr::Object(services) = value else {
            return Err(self.lowerer.residue(value.span(), "`services` must be an object of services"));
          };
          for service in &services.props {
            let (name, methods) = self.key_value(service)?;
            let js::Expr::Object(methods) = methods else {
              return Err(self.lowerer.residue(methods.span(), format!("`services.{name}` must be an object of methods")));
            };
            for method in &methods.props {
              let (method_name, body) = self.key_value(method)?;
              let lambda = match body {
                js::Expr::Arrow(arrow) => self.lowerer.lambda(arrow)?,
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

  fn entries(&mut self, value: &js::Expr) -> Lowered<Vec<(String, Expr)>> {
    let js::Expr::Object(obj) = value else {
      return Err(self.lowerer.residue(value.span(), "an object literal"));
    };
    let mut out = Vec::new();
    for prop in &obj.props {
      let (key, value) = self.key_value(prop)?;
      out.push((key, self.lowerer.expr(value)?));
    }
    Ok(out)
  }

  fn key_value<'p>(&self, prop: &'p js::PropOrSpread) -> Lowered<(String, &'p js::Expr)> {
    let js::PropOrSpread::Prop(p) = prop else {
      return Err(self.parsed.residue(prop.span(), "a spread in a mock"));
    };
    match &**p {
      js::Prop::KeyValue(kv) => {
        let key = prop_name(&kv.key).ok_or_else(|| self.parsed.residue(kv.key.span(), "a computed key in a mock"))?;
        Ok((key, &kv.value))
      }
      other => Err(self.parsed.residue(other.span(), "a mock entry must be `key: value`")),
    }
  }
}

fn unwrap_await(expr: &js::Expr) -> &js::Expr {
  match expr {
    js::Expr::Await(a) => unwrap_await(&a.arg),
    js::Expr::Paren(p) => unwrap_await(&p.expr),
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
import { load } from "./loader";
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
    let from_folder = lower_tests("tests/cart/loader.test.ts", &source.replace("\"./loader\"", "\"../../routes/cart/loader\"").replace("\"./actions\"", "\"@routes/cart/actions\"")).unwrap();
    assert_eq!(from_folder.tests[0].steps[1].1, file.tests[0].steps[1].1, "a test under tests/ names the same loader");
    assert_eq!(from_folder.tests[1].steps[1].1, file.tests[1].steps[1].1, "an alias names the same actions");
    let first = &file.tests[0];
    assert_eq!(first.name, "held lines carry the catalog's names");
    let Step::Mock { name, mock } = &first.steps[0].1 else { panic!("{:?}", first.steps[0]) };
    assert_eq!(name, "c");
    assert_eq!(mock.session.len(), 1);
    assert!(matches!(&mock.services[0], (s, m, Expr::Lambda { params, .. }) if s == "shopping" && m == "listProducts" && params.is_empty()));
    assert_eq!(first.steps[1].1, Step::Run { binding: Some(Binding::Fields(vec![("lines".to_owned(), "lines".to_owned())])), target: Target::Loader { file: "routes/cart/loader.ts".to_owned() }, ctx: "c".to_owned() });
    assert!(matches!(&first.steps[2].1, Step::Assert(Assertion::Equal(Expr::Var(v), Expr::Array(_))) if v == "lines"));
    assert!(matches!(&first.steps[3].1, Step::Assert(Assertion::Ok(_))));
    let second = &file.tests[1];
    let Step::Mock { mock, .. } = &second.steps[0].1 else { panic!() };
    assert!(matches!(&mock.input, Some(Expr::Object(_))));
    assert_eq!(second.steps[1].1, Step::Run { binding: None, target: Target::Action { file: "routes/cart/actions.ts".to_owned(), export: "addToCart".to_owned() }, ctx: "c".to_owned() });
    assert_eq!(second.steps[2].1, Step::Assert(Assertion::Rejects { target: Target::Action { file: "routes/cart/actions.ts".to_owned(), export: "addToCart".to_owned() }, ctx: "c".to_owned(), kind: Some("invalid".to_owned()) }));
    let _ = Lit::Null;
  }

  #[test]
  fn a_statement_outside_the_dialect_fails_with_its_line() {
    let source = "import { load } from \"./loader\";\nimport { assert, ctx, test } from \"@snapfire/fsr/testing\";\ntest(\"x\", async () => {\n  const c = ctx({});\n  const r = await load(c);\n  console.log(r);\n});\n";
    let err = lower_tests("routes/index/loader.test.ts", source).unwrap_err();
    assert_eq!(err.to_string(), "routes/index/loader.test.ts:6:3: a call other than `assert.<method>(...)`");
    let source = "import { assert, ctx, test } from \"@snapfire/fsr/testing\";\ntest(\"x\", async () => {\n  const r = await load(c);\n});\n";
    let err = lower_tests("routes/index/loader.test.ts", source).unwrap_err();
    assert!(err.to_string().contains("a `const` other than"), "{err}");
  }
}
