//! Reads Tailwind's own `*.test.ts` files and writes the cases that compile CSS as JSON
//! fixtures for `tests/conformance.rs`.
//!
//! ```sh
//! cargo run -p snapfire_tailwind --example extract_fixtures -- <tailwindcss>/packages/tailwindcss/src tailwind/tests/fixtures
//! ```

use std::path::Path;

use oxc_allocator::Allocator;
use oxc_ast::ast::{CallExpression, Expression, ObjectPropertyKind, PropertyKey, TemplateLiteral};
use oxc_ast_visit::{Visit, walk};
use oxc_parser::Parser;
use oxc_span::{GetSpan, SourceType};
use serde::Serialize;

#[derive(Serialize)]
struct Fixture {
  test: String,
  /// `css` compares the optimized output, `error` the thrown message.
  kind: &'static str,
  candidates: Vec<String>,
  css: String,
  expected: String,
  /// `@import` ids load Tailwind's own stylesheets.
  loader: bool,
  polyfills: Option<String>,
  base: Option<String>,
  /// A loader that answers every `@import` with this stylesheet and base.
  stylesheet: Option<(String, String)>,
  /// The test shuffles the candidates, so the output must not depend on their order.
  shuffled: bool,
}

/// The base a loader answers with when it hands back the base it was given.
const PASSTHROUGH: &str = "<base>";

#[derive(Clone, Default)]
struct Options {
  loader: bool,
  polyfills: Option<String>,
  base: Option<String>,
  stylesheet: Option<(String, String)>,
}

struct Call {
  candidates: Vec<String>,
  css: String,
  loader: bool,
  polyfills: Option<String>,
  base: Option<String>,
  stylesheet: Option<(String, String)>,
  shuffled: bool,
}

/// A `css-parser.test.ts` case: a stylesheet and the tree or the error it parses to.
#[derive(Serialize)]
struct ParserFixture {
  test: String,
  css: String,
  windows: bool,
  /// Parsed with a file name, so errors carry a location.
  located: bool,
  ast: Option<serde_json::Value>,
  error: Option<String>,
}

/// A call to one of Tailwind's string helpers and what it returns.
#[derive(Serialize)]
struct UnitFixture {
  test: String,
  function: String,
  args: Vec<serde_json::Value>,
  /// `sort` when the test sorts the result, `sign` when it compares `Math.sign(…)`.
  post: Option<String>,
  /// `value` compares JSON, `snapshot` compares pretty-format text, `error` the message.
  kind: &'static str,
  expected: serde_json::Value,
}

const UNIT_FUNCTIONS: &[&str] = &[
  "segment",
  "decodeArbitraryValue",
  "expand",
  "escape",
  "unescape",
  "toKeyPath",
  "compare",
];

/// A `candidate.test.ts` case: the utilities and variants the test registers, then the parse.
#[derive(Serialize)]
struct ParseFixture {
  test: String,
  candidate: String,
  utilities: Vec<(String, String)>,
  variants: Vec<(String, String, String)>,
  prefix: Option<String>,
  /// The parsed candidates as pretty-format prints them.
  expected: String,
}

#[derive(Default, Serialize)]
struct Skipped {
  test: String,
  reason: String,
}

struct Extractor<'s> {
  source: &'s str,
  css_dir: &'s Path,
  names: Vec<String>,
  scopes: Vec<std::collections::HashMap<String, String>>,
  arrays: Vec<std::collections::HashMap<String, Vec<String>>>,
  values: Vec<std::collections::HashMap<String, serde_json::Value>>,
  bindings: std::cell::RefCell<Vec<(String, String)>>,
  option_vars: std::collections::HashMap<String, Options>,
  snapshots: std::collections::HashMap<String, String>,
  snapshot_counts: std::collections::HashMap<String, usize>,
  registered_utilities: Vec<(String, String)>,
  registered_variants: Vec<(String, String, String)>,
  parses: Vec<ParseFixture>,
  parser: Vec<ParserFixture>,
  units: Vec<UnitFixture>,
  fixtures: Vec<Fixture>,
  skipped: Vec<Skipped>,
}

fn cooked_template(t: &TemplateLiteral) -> Option<String> {
  if !t.expressions.is_empty() {
    return None;
  }
  t.quasis.first()?.value.cooked.as_ref().map(|c| c.to_string())
}

/// Jest's `stripAddedIndentation`.
fn strip_indentation(snapshot: &str) -> String {
  let indentation = snapshot
    .lines()
    .find(|l| !l.trim().is_empty())
    .map(|l| &l[..l.len() - l.trim_start().len()])
    .unwrap_or("");
  if indentation.is_empty() {
    return snapshot.to_owned();
  }
  let mut lines: Vec<String> = snapshot.split('\n').map(str::to_owned).collect();
  if lines.len() <= 2 {
    return snapshot.to_owned();
  }
  if !lines[0].trim().is_empty() || !lines[lines.len() - 1].trim().is_empty() {
    return snapshot.to_owned();
  }
  let last = lines.len() - 1;
  for line in lines.iter_mut().take(last).skip(1) {
    if !line.is_empty() {
      if !line.starts_with(indentation) {
        return snapshot.to_owned();
      }
      *line = line[indentation.len()..].to_owned();
    }
  }
  lines[last] = String::new();
  lines.join("\n")
}

/// The value a string snapshot holds: the stripped text without its added line breaks and quotes.
fn snapshot_string(snapshot: &str) -> Option<String> {
  let stripped = strip_indentation(snapshot);
  let inner = stripped.strip_prefix('\n').unwrap_or(&stripped);
  let inner = inner.strip_suffix('\n').unwrap_or(inner);
  let inner = inner.strip_prefix('"')?.strip_suffix('"')?;
  Some(inner.to_owned())
}

fn snapshot_error(snapshot: &str) -> Option<String> {
  let stripped = strip_indentation(snapshot);
  let inner = stripped.trim_start_matches('\n');
  let inner = inner.strip_suffix('\n').unwrap_or(inner);
  let inner = inner.strip_prefix("[Error: ")?.strip_suffix(']')?;
  Some(inner.to_owned())
}

fn callee_name<'a>(call: &'a CallExpression) -> Option<&'a str> {
  match &call.callee {
    Expression::Identifier(id) => Some(id.name.as_str()),
    _ => None,
  }
}

impl<'s> Extractor<'s> {
  fn variable(&self, name: &str) -> Option<String> {
    if let Some((_, v)) = self.bindings.borrow().iter().rev().find(|(n, _)| n == name) {
      return Some(v.clone());
    }
    self.scopes.iter().rev().find_map(|scope| scope.get(name).cloned())
  }

  fn json_variable(&self, name: &str) -> Option<serde_json::Value> {
    self.values.iter().rev().find_map(|scope| scope.get(name).cloned())
  }

  fn array(&self, name: &str) -> Option<Vec<String>> {
    self.arrays.iter().rev().find_map(|scope| scope.get(name).cloned())
  }

  /// A list of strings: an array literal, an array variable or a sorted or mapped list. The flag
  /// says the list was shuffled.
  fn string_list(&self, expr: &Expression) -> Option<(Vec<String>, bool)> {
    match expr {
      Expression::ArrayExpression(array) => {
        let mut items = Vec::new();
        for element in &array.elements {
          items.push(self.string_of(element.as_expression()?)?);
        }
        Some((items, false))
      }
      Expression::Identifier(id) => self.array(&id.name).map(|items| (items, false)),
      Expression::CallExpression(call) => {
        let Expression::StaticMemberExpression(member) = &call.callee else {
          return None;
        };
        match member.property.name.as_str() {
          "sort" => Some((self.string_list(&member.object)?.0, true)),
          "map" => {
            let (items, shuffled) = self.string_list(&member.object)?;
            let Some(Expression::ArrowFunctionExpression(f)) = call.arguments.first().and_then(|a| a.as_expression())
            else {
              return None;
            };
            let param = match &f.params.items.first()?.pattern {
              oxc_ast::ast::BindingPattern::BindingIdentifier(id) => id.name.to_string(),
              _ => return None,
            };
            let body = f.body.as_expression()?;
            let mut out = Vec::new();
            for item in items {
              self.bindings.borrow_mut().push((param.clone(), item));
              let value = self.string_of(body);
              self.bindings.borrow_mut().pop();
              out.push(value?);
            }
            Some((out, shuffled))
          }
          _ => None,
        }
      }
      _ => None,
    }
  }

  fn push_scope(&mut self, scope: std::collections::HashMap<String, String>) {
    self.scopes.push(scope);
    self.arrays.push(Default::default());
    self.values.push(Default::default());
  }

  fn pop_scope(&mut self) {
    self.scopes.pop();
    self.arrays.pop();
    self.values.pop();
  }

  /// A string the expression evaluates to: a literal, a template whose parts are known, a
  /// variable holding one or a read of one of Tailwind's own stylesheets.
  fn string_of(&self, expr: &Expression) -> Option<String> {
    match expr {
      Expression::StringLiteral(s) => Some(s.value.to_string()),
      Expression::TemplateLiteral(t) => self.template(t, false),
      Expression::TaggedTemplateExpression(t)
        if t.quasi.expressions.is_empty()
          && &self.source[t.tag.span().start as usize..t.tag.span().end as usize] == "String.raw" =>
      {
        Some(t.quasi.quasis.iter().map(|q| q.value.raw.as_str()).collect())
      }
      Expression::TaggedTemplateExpression(t) => {
        let Expression::Identifier(tag) = &t.tag else {
          return None;
        };
        if tag.name != "css" && tag.name != "html" {
          return None;
        }
        self.template(&t.quasi, true)
      }
      Expression::Identifier(id) => self.variable(&id.name),
      Expression::CallExpression(call) => {
        let text = &self.source[call.span.start as usize..call.span.end as usize];
        if let Expression::StaticMemberExpression(member) = &call.callee
          && member.property.name == "replaceAll"
          && let Some(Expression::RegExpLiteral(pattern)) = call.arguments.first().and_then(|a| a.as_expression())
          && pattern.regex.pattern.text.as_str() == "\\r?\\n"
        {
          let value = self.string_of(&member.object)?.replace("\r\n", "\n");
          let windows = self.variable("lineEndings").as_deref() == Some("Windows");
          return Some(if windows { value.replace('\n', "\r\n") } else { value });
        }
        if let Expression::StaticMemberExpression(member) = &call.callee
          && member.property.name == "join"
          && let Some((items, _)) = self.string_list(&member.object)
          && let Some(separator) = call
            .arguments
            .first()
            .and_then(|a| a.as_expression())
            .and_then(|e| self.string_of(e))
        {
          return Some(items.join(&separator));
        }
        if !text.starts_with("fs.readFileSync(") {
          return None;
        }
        for file in ["index.css", "theme.css", "preflight.css", "utilities.css"] {
          if text.contains(&format!("'{file}'")) {
            return std::fs::read_to_string(self.css_dir.join(file)).ok();
          }
        }
        None
      }
      _ => None,
    }
  }

  fn template(&self, t: &TemplateLiteral, raw: bool) -> Option<String> {
    let mut out = String::new();
    for (i, quasi) in t.quasis.iter().enumerate() {
      if raw {
        out.push_str(&quasi.value.raw);
      } else {
        out.push_str(quasi.value.cooked.as_ref()?);
      }
      if let Some(expr) = t.expressions.get(i) {
        out.push_str(&self.string_of(expr)?);
      }
    }
    Some(out)
  }

  fn test_name(&self) -> String {
    self.names.join(" > ")
  }

  fn skip(&mut self, reason: impl Into<String>) {
    self.skipped.push(Skipped {
      test: self.test_name(),
      reason: reason.into(),
    });
  }

  /// The stylesheet and base a loader with no parameters always answers with.
  fn constant_loader(&self, value: &Expression) -> Option<(String, String)> {
    let params = match value {
      Expression::FunctionExpression(f) => &f.params,
      Expression::ArrowFunctionExpression(f) => &f.params,
      _ => return None,
    };
    let base_param = params.items.get(1).and_then(|p| match &p.pattern {
      oxc_ast::ast::BindingPattern::BindingIdentifier(id) => Some(id.name.to_string()),
      oxc_ast::ast::BindingPattern::AssignmentPattern(a) => match &a.left {
        oxc_ast::ast::BindingPattern::BindingIdentifier(id) => Some(id.name.to_string()),
        _ => None,
      },
      _ => None,
    });
    let body = match value {
      Expression::FunctionExpression(f) => {
        let statements = &f.body.as_ref()?.statements;
        let [oxc_ast::ast::Statement::ReturnStatement(ret)] = statements.as_slice() else {
          return None;
        };
        ret.argument.as_ref()?
      }
      Expression::ArrowFunctionExpression(f) => match &f.body {
        oxc_ast::ast::ArrowFunctionBody::FunctionBody(body) => {
          let [oxc_ast::ast::Statement::ReturnStatement(ret)] = body.statements.as_slice() else {
            return None;
          };
          ret.argument.as_ref()?
        }
        other => other.as_expression()?,
      },
      _ => return None,
    };
    let body = match body {
      Expression::ParenthesizedExpression(p) => &p.expression,
      Expression::CallExpression(c) => {
        let text = &self.source[c.callee.span().start as usize..c.callee.span().end as usize];
        if text != "Promise.resolve" {
          return None;
        }
        c.arguments.first()?.as_expression()?
      }
      other => other,
    };
    let Expression::ObjectExpression(object) = body else {
      return None;
    };
    let mut content = None;
    let mut base = None;
    for property in &object.properties {
      let ObjectPropertyKind::ObjectProperty(p) = property else {
        return None;
      };
      let PropertyKey::StaticIdentifier(id) = &p.key else {
        return None;
      };
      match id.name.as_str() {
        "content" => content = Some(self.string_of(&p.value)?),
        "base" => {
          base = match &p.value {
            Expression::Identifier(id) if Some(id.name.to_string()) == base_param => Some(PASSTHROUGH.to_owned()),
            other => Some(self.string_of(other)?),
          }
        }
        "path" => {}
        _ => return None,
      }
    }
    Some((content?, base?))
  }

  fn options(&self, object: &oxc_ast::ast::ObjectExpression) -> Result<Options, String> {
    let mut options = Options::default();
    for property in &object.properties {
      let ObjectPropertyKind::ObjectProperty(p) = property else {
        return Err("options use a spread".to_owned());
      };
      let key = match &p.key {
        PropertyKey::StaticIdentifier(id) => id.name.to_string(),
        _ => String::new(),
      };
      match key.as_str() {
        "loadStylesheet" => {
          let text = &self.source[p.span.start as usize..p.span.end as usize];
          if text.contains("readFileSync") && text.contains("index.css") {
            options.loader = true;
          } else if let Some(constant) = self.constant_loader(&p.value) {
            options.stylesheet = Some(constant);
          } else {
            return Err("a custom loadStylesheet".to_owned());
          }
        }
        "base" => match self.string_of(&p.value) {
          Some(b) => options.base = Some(b),
          None => return Err("a computed base".to_owned()),
        },
        "polyfills" => {
          let text = &self.source[p.value.span().start as usize..p.value.span().end as usize];
          options.polyfills = Some(text.to_owned());
        }
        other => return Err(format!("option `{other}`")),
      }
    }
    Ok(options)
  }

  /// `run([…], css, options)`, `compileCss(css, options)` or `compile(css, options)`.
  fn compile_call(&mut self, expr: &Expression) -> Option<Call> {
    let expr = match expr {
      Expression::AwaitExpression(a) => &a.argument,
      Expression::ArrowFunctionExpression(f) => f.body.as_expression()?,
      other => other,
    };
    let expr = match expr {
      Expression::AwaitExpression(a) => &a.argument,
      other => other,
    };
    let Expression::CallExpression(call) = expr else {
      return None;
    };
    let name = callee_name(call)?;
    let mut shuffled = false;
    let (candidates, css_arg, options_arg) = match name {
      "run" => {
        let Some((candidates, was_shuffled)) = call
          .arguments
          .first()
          .and_then(|a| a.as_expression())
          .and_then(|e| self.string_list(e))
        else {
          self.skip("candidates are not a list this extractor can read");
          return None;
        };
        shuffled = was_shuffled;
        (candidates, call.arguments.get(1), call.arguments.get(2))
      }
      "compileCss" | "compile" => (Vec::new(), call.arguments.first(), call.arguments.get(1)),
      _ => return None,
    };

    let css = match css_arg {
      None => "@tailwind utilities;".to_owned(),
      Some(arg) => match arg.as_expression().and_then(|e| self.string_of(e)) {
        Some(css) => css,
        None => {
          self.skip("the stylesheet is not a literal");
          return None;
        }
      },
    };

    let options = match options_arg.and_then(|a| a.as_expression()) {
      None => Options::default(),
      Some(Expression::ObjectExpression(object)) => match self.options(object) {
        Ok(o) => o,
        Err(reason) => {
          self.skip(reason);
          return None;
        }
      },
      Some(Expression::Identifier(id)) => match self.option_vars.get(id.name.as_str()) {
        Some(o) => o.clone(),
        None => {
          self.skip("options are a variable this extractor could not read");
          return None;
        }
      },
      Some(_) => {
        self.skip("options are not an object literal");
        return None;
      }
    };
    let Options {
      loader,
      polyfills,
      base,
      stylesheet,
    } = options;
    Some(Call {
      candidates,
      css,
      loader,
      polyfills,
      base,
      stylesheet,
      shuffled,
    })
  }

  fn json_of(&self, expr: &Expression) -> Option<serde_json::Value> {
    use serde_json::Value;
    Some(match expr {
      Expression::ObjectExpression(object) => {
        let mut map = serde_json::Map::new();
        for property in &object.properties {
          let ObjectPropertyKind::ObjectProperty(p) = property else {
            return None;
          };
          let key = match &p.key {
            PropertyKey::StaticIdentifier(id) => id.name.to_string(),
            PropertyKey::StringLiteral(s) => s.value.to_string(),
            _ => return None,
          };
          map.insert(key, self.json_of(&p.value)?);
        }
        Value::Object(map)
      }
      Expression::ArrayExpression(array) => Value::Array(
        array
          .elements
          .iter()
          .map(|e| e.as_expression().and_then(|e| self.json_of(e)))
          .collect::<Option<Vec<_>>>()?,
      ),
      Expression::BooleanLiteral(b) => Value::Bool(b.value),
      Expression::NullLiteral(_) => Value::Null,
      Expression::NumericLiteral(n) => serde_json::json!(n.value),
      Expression::UnaryExpression(u) if u.operator == oxc_ast::ast::UnaryOperator::UnaryNegation => {
        match self.json_of(&u.argument)? {
          Value::Number(n) => serde_json::json!(-n.as_f64()?),
          _ => return None,
        }
      }
      Expression::Identifier(id) if self.json_variable(&id.name).is_some() => self.json_variable(&id.name)?,
      Expression::CallExpression(call)
        if call.arguments.is_empty()
          && let Expression::StaticMemberExpression(member) = &call.callee
          && member.property.name == "sort" =>
      {
        let Value::Array(items) = self.json_of(&member.object)? else {
          return None;
        };
        let mut strings: Vec<String> = items
          .into_iter()
          .map(|v| v.as_str().map(str::to_owned))
          .collect::<Option<_>>()?;
        strings.sort();
        serde_json::json!(strings)
      }
      other => Value::String(self.string_of(other)?),
    })
  }

  /// The rows of a `test.each` table, each cell as JSON or `None` when it cannot be read.
  fn each_rows(&self, table: &Expression) -> Option<Vec<Vec<Option<serde_json::Value>>>> {
    if let Expression::Identifier(id) = table {
      let serde_json::Value::Array(rows) = self.json_variable(&id.name)? else {
        return None;
      };
      return Some(
        rows
          .into_iter()
          .map(|row| match row {
            serde_json::Value::Array(cells) => cells.into_iter().map(Some).collect(),
            other => vec![Some(other)],
          })
          .collect(),
      );
    }
    let Expression::ArrayExpression(table) = table else {
      return None;
    };
    Some(
      table
        .elements
        .iter()
        .map(|row| match row.as_expression() {
          Some(Expression::ArrayExpression(cells)) => cells
            .elements
            .iter()
            .map(|c| c.as_expression().and_then(|e| self.json_of(e)))
            .collect(),
          Some(other) => vec![self.json_of(other)],
          None => Vec::new(),
        })
        .collect(),
    )
  }

  /// `expect(segment(…)).toEqual([…])` and the like, for the string helpers.
  fn unit_expectation(&mut self, call: &CallExpression) -> bool {
    let Expression::StaticMemberExpression(member) = &call.callee else {
      return false;
    };
    let Expression::CallExpression(expect) = &member.object else {
      return false;
    };
    if callee_name(expect) != Some("expect") {
      return false;
    }
    let mut subject = match expect.arguments.first().and_then(|a| a.as_expression()) {
      Some(Expression::ArrowFunctionExpression(f)) => match f.body.as_expression() {
        Some(e) => e,
        None => return false,
      },
      Some(e) => e,
      None => return false,
    };
    let mut post = None;
    if let Expression::CallExpression(c) = subject {
      let text = &self.source[c.callee.span().start as usize..c.callee.span().end as usize];
      if text == "Math.sign"
        && let Some(inner) = c.arguments.first().and_then(|a| a.as_expression())
      {
        post = Some("sign".to_owned());
        subject = inner;
      }
    }
    let Expression::CallExpression(target) = subject else {
      return false;
    };
    let (function, args): (String, Vec<serde_json::Value>) = match &target.callee {
      Expression::Identifier(id) if UNIT_FUNCTIONS.contains(&id.name.as_str()) => {
        let args: Option<Vec<_>> = target
          .arguments
          .iter()
          .map(|a| a.as_expression().and_then(|e| self.json_of(e)))
          .collect();
        let Some(args) = args else {
          self.skip(format!("arguments to `{}` this extractor cannot read", id.name));
          return true;
        };
        (id.name.to_string(), args)
      }
      Expression::StaticMemberExpression(m) if m.property.name == "sort" => {
        let inner = &m.object;
        if let Expression::CallExpression(c) = inner
          && let Expression::Identifier(id) = &c.callee
          && UNIT_FUNCTIONS.contains(&id.name.as_str())
        {
          let args: Option<Vec<_>> = c
            .arguments
            .iter()
            .map(|a| a.as_expression().and_then(|e| self.json_of(e)))
            .collect();
          let Some(args) = args else { return false };
          post = Some("sort".to_owned());
          (id.name.to_string(), args)
        } else if matches!(target.arguments.first().and_then(|a| a.as_expression()), Some(Expression::Identifier(id)) if id.name == "compare")
        {
          let Some((items, _)) = self.string_list(inner) else {
            return false;
          };
          ("sortCompare".to_owned(), vec![serde_json::json!(items)])
        } else {
          return false;
        }
      }
      _ => return false,
    };
    let argument = call.arguments.first().and_then(|a| a.as_expression());
    let (kind, expected) = match (member.property.name.as_str(), argument) {
      ("toEqual" | "toBe", Some(e)) => match self.json_of(e) {
        Some(v) => ("value", v),
        None => {
          self.skip("an expected value this extractor cannot read");
          return true;
        }
      },
      ("toMatchInlineSnapshot", Some(Expression::TemplateLiteral(t))) => match cooked_template(t) {
        Some(snapshot) => (
          "snapshot",
          serde_json::Value::String(strip_indentation(&snapshot).trim().to_owned()),
        ),
        None => return true,
      },
      ("toThrowErrorMatchingInlineSnapshot", Some(Expression::TemplateLiteral(t))) => {
        match cooked_template(t).and_then(|s| snapshot_error(&s)) {
          Some(message) => ("error", serde_json::Value::String(message)),
          None => return true,
        }
      }
      _ => {
        self.skip(format!("helper matcher `{}`", member.property.name));
        return true;
      }
    };
    self.units.push(UnitFixture {
      test: self.test_name(),
      function,
      args,
      post,
      kind,
      expected,
    });
    true
  }

  /// `expect(parse(…)).toEqual([…])` and `expect(() => parse(…)).toThrow…` in `css-parser.test.ts`.
  fn parser_expectation(&mut self, call: &CallExpression) -> bool {
    let Expression::StaticMemberExpression(member) = &call.callee else {
      return false;
    };
    let Expression::CallExpression(expect) = &member.object else {
      return false;
    };
    if callee_name(expect) != Some("expect") {
      return false;
    }
    let subject = match expect.arguments.first().and_then(|a| a.as_expression()) {
      Some(Expression::ArrowFunctionExpression(f)) => match f.body.as_expression() {
        Some(e) => e,
        None => return false,
      },
      Some(e) => e,
      None => return false,
    };
    let Expression::CallExpression(parse) = subject else {
      return false;
    };
    let located = match callee_name(parse) {
      Some("parse") => false,
      Some("parseWithLoc") => true,
      _ => return false,
    };
    let Some(css) = parse
      .arguments
      .first()
      .and_then(|a| a.as_expression())
      .and_then(|e| self.string_of(e))
    else {
      self.skip("a parsed stylesheet this extractor cannot read");
      return true;
    };
    let windows = self.variable("lineEndings").as_deref() == Some("Windows");
    let argument = call.arguments.first().and_then(|a| a.as_expression());
    let (ast, error) = match (member.property.name.as_str(), argument) {
      ("toEqual", Some(e)) => match self.json_of(e) {
        Some(v) => (Some(v), None),
        None => {
          self.skip("an expected tree this extractor cannot read");
          return true;
        }
      },
      ("toThrowErrorMatchingInlineSnapshot", Some(Expression::TemplateLiteral(t))) => {
        let Some(snapshot) = cooked_template(t) else {
          return true;
        };
        let message = snapshot
          .trim()
          .strip_prefix("[CssSyntaxError: ")
          .and_then(|m| m.strip_suffix(']'))
          .map(str::to_owned);
        match message {
          Some(m) => (None, Some(m)),
          None => {
            self.skip("an error snapshot of another shape");
            return true;
          }
        }
      }
      _ => {
        self.skip(format!("parser matcher `{}`", member.property.name));
        return true;
      }
    };
    self.parser.push(ParserFixture {
      test: self.test_name(),
      css,
      windows,
      located,
      ast,
      error,
    });
    true
  }

  /// `expect(run('…', { utilities, variants, prefix }))` in `candidate.test.ts`.
  fn parse_expectation(&mut self, call: &CallExpression) -> bool {
    let Expression::StaticMemberExpression(member) = &call.callee else {
      return false;
    };
    let Expression::CallExpression(expect) = &member.object else {
      return false;
    };
    if callee_name(expect) != Some("expect") {
      return false;
    }
    let Some(Expression::CallExpression(run)) = expect.arguments.first().and_then(|a| a.as_expression()) else {
      return false;
    };
    if callee_name(run) != Some("run") {
      return false;
    }
    let Some(candidate) = run
      .arguments
      .first()
      .and_then(|a| a.as_expression())
      .and_then(|e| self.string_of(e))
    else {
      return false;
    };
    let mut prefix = None;
    let mut utilities = false;
    let mut variants = false;
    if let Some(Expression::ObjectExpression(object)) = run.arguments.get(1).and_then(|a| a.as_expression()) {
      for property in &object.properties {
        let ObjectPropertyKind::ObjectProperty(p) = property else {
          continue;
        };
        let PropertyKey::StaticIdentifier(id) = &p.key else {
          continue;
        };
        match id.name.as_str() {
          "prefix" => prefix = self.string_of(&p.value),
          "utilities" => utilities = true,
          "variants" => variants = true,
          _ => {}
        }
      }
    }
    let expected = match (
      member.property.name.as_str(),
      call.arguments.first().and_then(|a| a.as_expression()),
    ) {
      ("toEqual", Some(Expression::ArrayExpression(a))) if a.elements.is_empty() => "[]".to_owned(),
      ("toMatchInlineSnapshot", Some(Expression::TemplateLiteral(t))) => match cooked_template(t) {
        Some(snapshot) => strip_indentation(&snapshot).trim().to_owned(),
        None => return false,
      },
      _ => return false,
    };
    self.parses.push(ParseFixture {
      test: self.test_name(),
      candidate,
      utilities: if utilities {
        self.registered_utilities.clone()
      } else {
        Vec::new()
      },
      variants: if variants {
        self.registered_variants.clone()
      } else {
        Vec::new()
      },
      prefix,
      expected,
    });
    true
  }

  fn expectation(&mut self, call: &CallExpression) {
    let Expression::StaticMemberExpression(member) = &call.callee else {
      return;
    };
    let matcher = member.property.name.as_str();
    if !matches!(
      matcher,
      "toMatchInlineSnapshot"
        | "toMatchSnapshot"
        | "toEqual"
        | "toBe"
        | "toThrowErrorMatchingInlineSnapshot"
        | "toThrow"
    ) {
      return;
    }

    let (target, rejects) = match &member.object {
      Expression::StaticMemberExpression(inner) if inner.property.name == "rejects" => (&inner.object, true),
      other => (other, false),
    };
    let Expression::CallExpression(expect) = target else {
      return;
    };
    if callee_name(expect) != Some("expect") {
      return;
    }
    let Some(subject) = expect.arguments.first().and_then(|a| a.as_expression()) else {
      return;
    };

    let is_compile = |e: &Expression| {
      let e = match e {
        Expression::AwaitExpression(a) => &a.argument,
        Expression::ArrowFunctionExpression(f) => match f.body.as_expression() {
          Some(Expression::AwaitExpression(a)) => &a.argument,
          Some(other) => other,
          None => return false,
        },
        other => other,
      };
      let names: &[&str] = if rejects {
        &["run", "compileCss", "compile"]
      } else {
        &["run", "compileCss"]
      };
      matches!(e, Expression::CallExpression(c) if callee_name(c).is_some_and(|n| names.contains(&n)))
    };
    if !is_compile(subject) {
      let text = &self.source[subject.span().start as usize..subject.span().end as usize];
      let head: String = text.chars().take(40).collect();
      self.skip(format!(
        "asserts on `{}`",
        head.split('(').next().unwrap_or(&head).trim()
      ));
      return;
    }

    let Some(Call {
      candidates,
      css,
      loader,
      polyfills,
      base,
      stylesheet,
      shuffled,
    }) = self.compile_call(subject)
    else {
      return;
    };
    let argument = call.arguments.first().and_then(|a| a.as_expression());

    let (kind, expected) = if rejects {
      if matcher == "toThrow" && argument.is_none() {
        self.fixtures.push(Fixture {
          test: self.test_name(),
          kind: "throws",
          candidates,
          css,
          expected: String::new(),
          loader,
          polyfills,
          base,
          stylesheet,
          shuffled,
        });
        return;
      }
      let snapshot = match argument {
        Some(Expression::TemplateLiteral(t)) => cooked_template(t),
        Some(Expression::StringLiteral(s)) => Some(s.value.to_string()),
        _ => {
          self.skip("a rejection without an inline snapshot");
          return;
        }
      };
      let Some(snapshot) = snapshot else {
        self.skip("an interpolated error snapshot");
        return;
      };
      let Some(message) = snapshot_error(&snapshot) else {
        self.skip("an error snapshot of another shape");
        return;
      };
      ("error", message)
    } else {
      match (matcher, argument) {
        ("toMatchInlineSnapshot", Some(Expression::TemplateLiteral(t))) => {
          let Some(snapshot) = cooked_template(t) else {
            self.skip("an interpolated snapshot");
            return;
          };
          match snapshot_string(&snapshot) {
            Some(value) => ("css", value),
            None if snapshot.trim().is_empty() || snapshot.trim() == "\"\"" => ("css", String::new()),
            None => {
              self.skip("a snapshot of another shape");
              return;
            }
          }
        }
        ("toMatchSnapshot", None) => {
          let name = self.test_name();
          let count = self.snapshot_counts.entry(name.clone()).or_insert(0);
          *count += 1;
          let key = format!("{name} {count}");
          match self.snapshots.get(&key).and_then(|v| file_snapshot(v)) {
            Some(value) => ("css", value),
            None => {
              self.skip(format!("no file snapshot `{key}`"));
              return;
            }
          }
        }
        ("toEqual" | "toBe", Some(e)) if matches!(e, Expression::AwaitExpression(_)) => {
          let Some(other) = self.compile_call(e) else { return };
          ("equal", other.css)
        }
        ("toEqual" | "toBe", Some(e)) => match self.string_of(e) {
          Some(s) => ("css", s),
          None => {
            self.skip("compared against a non-literal");
            return;
          }
        },
        _ => {
          self.skip(format!("matcher `{matcher}`"));
          return;
        }
      }
    };

    self.fixtures.push(Fixture {
      test: self.test_name(),
      kind,
      candidates,
      css,
      expected,
      loader,
      polyfills,
      base,
      stylesheet,
      shuffled,
    });
  }
}

impl<'a> Visit<'a> for Extractor<'_> {
  fn visit_for_of_statement(&mut self, it: &oxc_ast::ast::ForOfStatement<'a>) {
    let oxc_ast::ast::ForStatementLeft::VariableDeclaration(declaration) = &it.left else {
      return walk::walk_for_of_statement(self, it);
    };
    let Some(oxc_ast::ast::BindingPattern::BindingIdentifier(id)) = declaration.declarations.first().map(|d| &d.id)
    else {
      return walk::walk_for_of_statement(self, it);
    };
    let Some((items, _)) = self.string_list(&it.right) else {
      return walk::walk_for_of_statement(self, it);
    };
    let name = id.name.to_string();
    for item in items {
      let mut scope = std::collections::HashMap::new();
      scope.insert(name.clone(), item);
      self.push_scope(scope);
      self.visit_statement(&it.body);
      self.pop_scope();
    }
  }

  fn visit_variable_declarator(&mut self, it: &oxc_ast::ast::VariableDeclarator<'a>) {
    if let oxc_ast::ast::BindingPattern::BindingIdentifier(id) = &it.id
      && let Some(init) = &it.init
      && let Some((items, _)) = self.string_list(init)
      && let Some(scope) = self.arrays.last_mut()
    {
      scope.insert(id.name.to_string(), items);
    }
    if let oxc_ast::ast::BindingPattern::BindingIdentifier(id) = &it.id
      && let Some(init) = &it.init
      && matches!(
        init,
        Expression::ArrayExpression(_) | Expression::NumericLiteral(_) | Expression::UnaryExpression(_)
      )
      && let Some(value) = self.json_of(init)
      && let Some(scope) = self.values.last_mut()
    {
      scope.insert(id.name.to_string(), value);
    }
    if let oxc_ast::ast::BindingPattern::BindingIdentifier(id) = &it.id
      && let Some(Expression::ObjectExpression(object)) = &it.init
      && let Ok(options) = self.options(object)
    {
      self.option_vars.insert(id.name.to_string(), options);
    }
    if let oxc_ast::ast::BindingPattern::BindingIdentifier(id) = &it.id
      && let Some(init) = &it.init
      && let Some(value) = self.string_of(init)
    {
      let name = id.name.to_string();
      if let Some(scope) = self.scopes.last_mut() {
        scope.insert(name, value);
      }
    }
    walk::walk_variable_declarator(self, it);
  }

  fn visit_call_expression(&mut self, call: &CallExpression<'a>) {
    if let Some(name) = callee_name(call)
      && matches!(name, "test" | "it" | "describe")
    {
      let title = call
        .arguments
        .first()
        .and_then(|a| a.as_expression())
        .and_then(|e| self.string_of(e))
        .unwrap_or_else(|| "<dynamic>".to_owned());
      self.names.push(title);
      self.push_scope(Default::default());
      let saved = (
        std::mem::take(&mut self.registered_utilities),
        std::mem::take(&mut self.registered_variants),
      );
      self.registered_utilities = saved.0.clone();
      self.registered_variants = saved.1.clone();
      walk::walk_call_expression(self, call);
      self.registered_utilities = saved.0;
      self.registered_variants = saved.1;
      self.pop_scope();
      self.names.pop();
      return;
    }
    if let Expression::CallExpression(each) = &call.callee
      && let Expression::StaticMemberExpression(member) = &each.callee
      && member.property.name == "each"
      && matches!(&member.object, Expression::Identifier(id) if id.name == "test" || id.name == "it" || id.name == "describe")
      && let Some(rows) = each
        .arguments
        .first()
        .and_then(|a| a.as_expression())
        .and_then(|e| self.each_rows(e))
    {
      let title = call
        .arguments
        .first()
        .and_then(|a| a.as_expression())
        .and_then(|e| self.string_of(e))
        .unwrap_or_else(|| "<dynamic>".to_owned());
      let Some(body) = call.arguments.get(1).and_then(|a| a.as_expression()) else {
        return;
      };
      let (params, function_body) = match body {
        Expression::ArrowFunctionExpression(f) => (&f.params, &f.body),
        _ => {
          self.names.push(title);
          self.skip("a test.each body that is not an arrow function");
          self.names.pop();
          return;
        }
      };
      let names: Vec<String> = params
        .items
        .iter()
        .map(|p| match &p.pattern {
          oxc_ast::ast::BindingPattern::BindingIdentifier(id) => id.name.to_string(),
          _ => String::new(),
        })
        .collect();
      for (row_index, values) in rows.into_iter().enumerate() {
        let mut scope = std::collections::HashMap::new();
        let mut json = std::collections::HashMap::new();
        for (name, value) in names.iter().zip(values) {
          match value {
            Some(serde_json::Value::String(text)) => {
              scope.insert(name.clone(), text);
            }
            Some(other) => {
              json.insert(name.clone(), other);
            }
            None => {}
          }
        }
        self.names.push(format!("{title} [{row_index}]"));
        self.push_scope(scope);
        *self.values.last_mut().unwrap() = json;
        self.visit_arrow_body(function_body);
        self.pop_scope();
        self.names.pop();
      }
      return;
    }
    if let Expression::StaticMemberExpression(member) = &call.callee
      && let Expression::Identifier(object) = &member.object
      && (object.name == "utilities" || object.name == "variants")
      && let Some(name) = call
        .arguments
        .first()
        .and_then(|a| a.as_expression())
        .and_then(|e| self.string_of(e))
    {
      let kind = member.property.name.to_string();
      if object.name == "utilities" {
        self.registered_utilities.push((kind, name));
      } else {
        let compounds = match (kind.as_str(), call.arguments.get(1).and_then(|a| a.as_expression())) {
          ("compound", Some(e)) => self.source[e.span().start as usize..e.span().end as usize].to_owned(),
          _ => String::new(),
        };
        self.registered_variants.push((kind, name, compounds));
      }
    }
    if self.unit_expectation(call) {
      walk::walk_call_expression(self, call);
      return;
    }
    if self.parser_expectation(call) {
      walk::walk_call_expression(self, call);
      return;
    }
    if self.parse_expectation(call) {
      walk::walk_call_expression(self, call);
      return;
    }
    if let Expression::StaticMemberExpression(member) = &call.callee
      && member.property.name == "push"
      && let Expression::Identifier(id) = &member.object
    {
      let values: Option<Vec<String>> = call
        .arguments
        .iter()
        .map(|a| a.as_expression().and_then(|e| self.string_of(e)))
        .collect();
      let name = id.name.to_string();
      if let Some(values) = values
        && let Some(scope) = self.arrays.iter_mut().rev().find(|scope| scope.contains_key(&name))
      {
        scope.get_mut(&name).expect("the array").extend(values);
      }
    }
    self.expectation(call);
    walk::walk_call_expression(self, call);
  }
}

impl Extractor<'_> {
  fn visit_arrow_body<'a>(&mut self, body: &oxc_ast::ast::ArrowFunctionBody<'a>) {
    match body {
      oxc_ast::ast::ArrowFunctionBody::FunctionBody(b) => self.visit_function_body(b),
      other => {
        if let Some(e) = other.as_expression() {
          self.visit_expression(e);
        }
      }
    }
  }
}

/// The entries of a vitest `.snap` file, by name.
fn read_snapshots(path: &Path) -> std::collections::HashMap<String, String> {
  let mut out = std::collections::HashMap::new();
  let Ok(source) = std::fs::read_to_string(path) else {
    return out;
  };
  let allocator = Allocator::default();
  let parsed = Parser::new(&allocator, &source, SourceType::mjs()).parse();
  for statement in &parsed.program.body {
    let oxc_ast::ast::Statement::ExpressionStatement(e) = statement else {
      continue;
    };
    let Expression::AssignmentExpression(assign) = &e.expression else {
      continue;
    };
    let oxc_ast::ast::AssignmentTarget::ComputedMemberExpression(target) = &assign.left else {
      continue;
    };
    let Expression::TemplateLiteral(key) = &target.expression else {
      continue;
    };
    let Expression::TemplateLiteral(value) = &assign.right else {
      continue;
    };
    if let (Some(k), Some(v)) = (cooked_template(key), cooked_template(value)) {
      out.insert(k, v);
    }
  }
  out
}

/// A file snapshot's string: the value without its surrounding line breaks and quotes.
fn file_snapshot(value: &str) -> Option<String> {
  let inner = value.strip_prefix('\n')?.strip_suffix('\n')?;
  Some(inner.strip_prefix('"')?.strip_suffix('"')?.to_owned())
}

fn main() {
  let args: Vec<String> = std::env::args().collect();
  let (src, out) = (Path::new(&args[1]), Path::new(&args[2]));
  std::fs::create_dir_all(out).expect("the output directory");

  let files = [
    "utils/segment.test.ts",
    "utils/decode-arbitrary-value.test.ts",
    "utils/brace-expansion.test.ts",
    "utils/escape.test.ts",
    "utils/to-key-path.test.ts",
    "utils/compare.test.ts",
    "css-parser.test.ts",
    "candidate.test.ts",
    "utilities.test.ts",
    "variants.test.ts",
    "index.test.ts",
    "css-functions.test.ts",
    "at-import.test.ts",
    "prefix.test.ts",
    "important.test.ts",
    "compat/legacy-utilities.test.ts",
  ];
  let mut total = 0;
  let mut skipped_total = 0;
  for file in files {
    let path = src.join(file);
    let source = std::fs::read_to_string(&path).expect("a test file");
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, &source, SourceType::ts()).parse();
    assert!(parsed.diagnostics.is_empty(), "{file} parses");

    let css_dir = src.parent().expect("the package directory");
    let mut extractor = Extractor {
      source: &source,
      css_dir,
      names: Vec::new(),
      scopes: vec![Default::default()],
      arrays: vec![Default::default()],
      values: vec![Default::default()],
      bindings: Default::default(),
      option_vars: Default::default(),
      snapshots: read_snapshots(
        &src
          .join("__snapshots__")
          .join(format!("{}.snap", file.rsplit('/').next().unwrap())),
      ),
      snapshot_counts: Default::default(),
      registered_utilities: Vec::new(),
      registered_variants: Vec::new(),
      parses: Vec::new(),
      parser: Vec::new(),
      units: Vec::new(),
      fixtures: Vec::new(),
      skipped: Vec::new(),
    };
    extractor.visit_program(&parsed.program);

    let stem = file.replace('/', "-").replace(".test.ts", "");
    if !extractor.fixtures.is_empty() {
      let fixtures = serde_json::to_string_pretty(&extractor.fixtures).unwrap();
      std::fs::write(out.join(format!("{stem}.json")), fixtures + "\n").unwrap();
    }
    if !extractor.units.is_empty() {
      let units = serde_json::to_string_pretty(&extractor.units).unwrap();
      std::fs::write(out.join(format!("{stem}-units.json")), units + "\n").unwrap();
      println!("{file}: {} helper fixtures", extractor.units.len());
    }
    if !extractor.parser.is_empty() {
      let parser = serde_json::to_string_pretty(&extractor.parser).unwrap();
      std::fs::write(out.join(format!("{stem}-cases.json")), parser + "\n").unwrap();
      println!("{file}: {} parser fixtures", extractor.parser.len());
    }
    if !extractor.parses.is_empty() {
      let parses = serde_json::to_string_pretty(&extractor.parses).unwrap();
      std::fs::write(out.join(format!("{stem}-parse.json")), parses + "\n").unwrap();
      println!("{file}: {} parse fixtures", extractor.parses.len());
    }
    let skipped = serde_json::to_string_pretty(&extractor.skipped).unwrap();
    std::fs::write(out.join(format!("{stem}.skipped.json")), skipped + "\n").unwrap();
    println!(
      "{file}: {} fixtures, {} skipped",
      extractor.fixtures.len(),
      extractor.skipped.len()
    );
    total += extractor.fixtures.len();
    skipped_total += extractor.skipped.len();
  }
  println!("total: {total} fixtures, {skipped_total} skipped");
}
