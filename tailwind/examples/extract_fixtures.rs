//! Reads Tailwind's own `*.test.ts` files and writes the cases that compile CSS as JSON
//! fixtures for `tests/conformance.rs`.
//!
//! ```sh
//! cargo run -p snapfire_tailwind --example extract_fixtures -- <tailwindcss>/packages/tailwindcss/src tailwind/tests/fixtures
//! ```

use std::path::Path;

use oxc_allocator::Allocator;
use oxc_ast::ast::{Argument, CallExpression, Expression, ObjectPropertyKind, PropertyKey, TemplateLiteral};
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
    self.scopes.iter().rev().find_map(|scope| scope.get(name).cloned())
  }

  /// A string the expression evaluates to: a literal, a template whose parts are known, a
  /// variable holding one or a read of one of Tailwind's own stylesheets.
  fn string_of(&self, expr: &Expression) -> Option<String> {
    match expr {
      Expression::StringLiteral(s) => Some(s.value.to_string()),
      Expression::TemplateLiteral(t) => self.template(t, false),
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

  /// `run([…], css, options)` or `compileCss(css, options)`.
  fn compile_call(&mut self, expr: &Expression) -> Option<(Vec<String>, String, bool, Option<String>)> {
    let expr = match expr {
      Expression::AwaitExpression(a) => &a.argument,
      other => other,
    };
    let Expression::CallExpression(call) = expr else {
      return None;
    };
    let name = callee_name(call)?;
    let (candidates, css_arg, options_arg) = match name {
      "run" => {
        let Some(Argument::ArrayExpression(array)) = call.arguments.first() else {
          self.skip("candidates are not an array literal");
          return None;
        };
        let mut candidates = Vec::new();
        for element in &array.elements {
          let Some(e) = element.as_expression() else {
            self.skip("a candidate is a spread or a hole");
            return None;
          };
          let Some(s) = self.string_of(e) else {
            self.skip("a candidate is not a string literal");
            return None;
          };
          candidates.push(s);
        }
        (candidates, call.arguments.get(1), call.arguments.get(2))
      }
      "compileCss" => (Vec::new(), call.arguments.first(), call.arguments.get(1)),
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

    let mut loader = false;
    let mut polyfills = None;
    if let Some(arg) = options_arg {
      let Some(Expression::ObjectExpression(object)) = arg.as_expression() else {
        self.skip("options are not an object literal");
        return None;
      };
      for property in &object.properties {
        let ObjectPropertyKind::ObjectProperty(p) = property else {
          self.skip("options use a spread");
          return None;
        };
        let key = match &p.key {
          PropertyKey::StaticIdentifier(id) => id.name.to_string(),
          _ => String::new(),
        };
        match key.as_str() {
          "loadStylesheet" => {
            let text = &self.source[p.span.start as usize..p.span.end as usize];
            if text.contains("readFileSync") && text.contains("index.css") {
              loader = true;
            } else {
              self.skip("a custom loadStylesheet");
              return None;
            }
          }
          "polyfills" => {
            let text = &self.source[p.value.span().start as usize..p.value.span().end as usize];
            polyfills = Some(text.to_owned());
          }
          other => {
            self.skip(format!("option `{other}`"));
            return None;
          }
        }
      }
    }
    Some((candidates, css, loader, polyfills))
  }

  fn expectation(&mut self, call: &CallExpression) {
    let Expression::StaticMemberExpression(member) = &call.callee else {
      return;
    };
    let matcher = member.property.name.as_str();
    if !matches!(
      matcher,
      "toMatchInlineSnapshot" | "toEqual" | "toBe" | "toThrowErrorMatchingInlineSnapshot" | "toThrow"
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
        other => other,
      };
      matches!(e, Expression::CallExpression(c) if matches!(callee_name(c), Some("run") | Some("compileCss")))
    };
    if !is_compile(subject) {
      return;
    }

    let Some((candidates, css, loader, polyfills)) = self.compile_call(subject) else {
      return;
    };
    let argument = call.arguments.first().and_then(|a| a.as_expression());

    let (kind, expected) = if rejects {
      let Some(Expression::TemplateLiteral(t)) = argument else {
        self.skip("a rejection without an inline snapshot");
        return;
      };
      let Some(snapshot) = cooked_template(t) else {
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
    });
  }
}

impl<'a> Visit<'a> for Extractor<'_> {
  fn visit_variable_declarator(&mut self, it: &oxc_ast::ast::VariableDeclarator<'a>) {
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
      self.scopes.push(Default::default());
      walk::walk_call_expression(self, call);
      self.scopes.pop();
      self.names.pop();
      return;
    }
    self.expectation(call);
    walk::walk_call_expression(self, call);
  }
}

fn main() {
  let args: Vec<String> = std::env::args().collect();
  let (src, out) = (Path::new(&args[1]), Path::new(&args[2]));
  std::fs::create_dir_all(out).expect("the output directory");

  let files = [
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
      fixtures: Vec::new(),
      skipped: Vec::new(),
    };
    extractor.visit_program(&parsed.program);

    let stem = file.replace('/', "-").replace(".test.ts", "");
    let fixtures = serde_json::to_string_pretty(&extractor.fixtures).unwrap();
    std::fs::write(out.join(format!("{stem}.json")), fixtures + "\n").unwrap();
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
