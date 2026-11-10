//! A promise chain in an async function, `await p.then(f).catch(g)`, written with `await`: `then(f)` calls `f` with the awaited value, `catch(g)` is a `try` whose `catch` calls `g` with what was thrown and `finally(h)` is a `try` whose `finally` awaits `h()`. Each callback is bound to a name first, so the build calls it as a named function. A chain this pass cannot write, or one outside an async function, is left as written with a note.

use oxc_allocator::Allocator;
use oxc_ast::ast::{self as js, Argument, Expression, Statement};
use oxc_ast_visit::{walk, Visit};
use oxc_span::{GetSpan, Span};

use crate::edit::Edits;
use crate::syntax::{is_script, parse, Parsed};
use crate::{Pass, Report, Tree};

pub(crate) fn passes() -> Vec<Box<dyn Pass>> {
  vec![Box::new(Promises)]
}

struct Promises;

impl Pass for Promises {
  fn name(&self) -> &'static str {
    "promises"
  }

  fn run(&self, tree: &mut Tree, report: &mut Report) {
    let paths: Vec<String> = tree.paths().filter(|p| p.starts_with("app/") && is_script(p)).map(str::to_owned).collect();
    for path in paths {
      let Some(text) = tree.text(&path).map(str::to_owned) else { continue };
      if !text.contains(".then(") && !text.contains(".catch(") && !text.contains(".finally(") {
        continue;
      }
      let allocator = Allocator::default();
      let Ok(parsed) = parse(&allocator, &path, &text) else { continue };
      let mut walker = Walker { parsed: &parsed, text: &text, asyncs: Vec::new(), edits: Edits::default(), notes: Vec::new(), names: 0, written: Vec::new() };
      walker.visit_program(&parsed.program);
      for (line, message) in walker.notes {
        report.note(&path, line, message);
      }
      if !walker.edits.is_empty() {
        tree.insert(path.clone(), walker.edits.apply(&text));
      }
    }
  }
}

/// Where a chain's value goes.
enum Sink {
  /// `const x = ...` or `let x = ...`: the keyword and the binding as written.
  Declare(String, String),
  /// `x = ...`.
  Assign(String),
  Return,
  /// `await ...;`, the value dropped.
  Drop,
}

/// One link of a chain: `then`, `catch` or `finally` and its callback as written.
struct Link {
  method: &'static str,
  callback: Option<String>,
}

struct Walker<'p, 'a> {
  parsed: &'p Parsed<'a>,
  text: &'p str,
  /// Whether each function the walk is inside is async, innermost last.
  asyncs: Vec<bool>,
  edits: Edits,
  notes: Vec<(usize, String)>,
  names: usize,
  /// The spans of the chains this pass wrote, so the walk notes only the ones it left.
  written: Vec<Span>,
}

impl<'a> Walker<'_, 'a> {
  fn source(&self, span: Span) -> &str {
    &self.text[self.parsed.range(span)]
  }

  fn fresh(&mut self, stem: &str) -> String {
    self.names += 1;
    format!("{stem}{}", self.names)
  }

  /// The links of `e` from the innermost out and the expression they hang from, when `e` is a call of `then`, `catch` or `finally`. `None` for a call of another shape, a `then` with a rejection handler among them.
  fn chain<'e>(&self, mut e: &'e Expression<'a>) -> Option<Result<(&'e Expression<'a>, Vec<Link>), String>> {
    let mut links = Vec::new();
    loop {
      let Expression::CallExpression(call) = e.without_parentheses() else { break };
      let Expression::StaticMemberExpression(member) = call.callee.without_parentheses() else { break };
      let method = match member.property.name.as_str() {
        "then" => "then",
        "catch" => "catch",
        "finally" => "finally",
        _ => break,
      };
      if call.arguments.iter().any(|a| matches!(a, Argument::SpreadElement(_))) {
        return Some(Err(format!("`.{method}()` with a spread argument")));
      }
      if method == "then" && call.arguments.len() > 1 {
        return Some(Err("`.then()` with a rejection handler; write it as `.then(f).catch(g)` if that is what it means".to_owned()));
      }
      let callback = call.arguments.first().map(|a| self.source(a.span()).to_owned());
      links.push(Link { method, callback });
      e = &member.object;
    }
    if links.is_empty() {
      return None;
    }
    links.reverse();
    Some(Ok((e, links)))
  }

  /// `callback` bound to a fresh name in `lines`, the name returned. A name as written is used as it is.
  fn named(&mut self, callback: &str, lines: &mut Vec<String>) -> String {
    let plain = callback.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$' || c == '.');
    if plain {
      return callback.to_owned();
    }
    let name = self.fresh("$then");
    lines.push(format!("const {name} = {callback};"));
    name
  }

  /// The statements that put the chain's value in `sink`: `value` is the expression the links hang from, already awaited.
  fn written(&mut self, value: String, links: &[Link], sink: &Sink, lines: &mut Vec<String>, indent: &str) {
    let last = links.iter().rposition(|l| l.method != "then");
    let (inner, outer) = match last {
      Some(at) => links.split_at(at + 1),
      None => (&links[..0], links),
    };
    let mut value = value;
    if let Some((settle, before)) = inner.split_last() {
      let held = self.fresh("$settled");
      lines.push(format!("let {held};"));
      let mut tried = Vec::new();
      self.written(value, before, &Sink::Assign(held.clone()), &mut tried, &format!("{indent}  "));
      let mut handled = Vec::new();
      let clause = match (settle.method, &settle.callback) {
        ("catch", Some(callback)) => {
          let f = self.named(callback, lines);
          let caught = self.fresh("$caught");
          handled.push(format!("{held} = await {f}({caught});"));
          format!("catch ({caught})")
        }
        ("catch", None) => {
          handled.push(format!("{held} = undefined;"));
          "catch".to_owned()
        }
        (_, Some(callback)) => {
          let f = self.named(callback, lines);
          handled.push(format!("await {f}();"));
          "finally".to_owned()
        }
        (_, None) => "finally".to_owned(),
      };
      let inner_indent = format!("{indent}  ");
      lines.push(format!("try {{\n{}\n{indent}}} {clause} {{\n{}\n{indent}}}", tried.iter().map(|l| format!("{inner_indent}{l}")).collect::<Vec<_>>().join("\n"), handled.iter().map(|l| format!("{inner_indent}{l}")).collect::<Vec<_>>().join("\n")));
      value = held;
    }
    for link in outer {
      match &link.callback {
        Some(callback) => {
          let f = self.named(callback, lines);
          value = format!("await {f}({value})");
        }
        None => {}
      }
    }
    lines.push(match sink {
      Sink::Declare(keyword, binding) => format!("{keyword} {binding} = {value};"),
      Sink::Assign(name) => format!("{name} = {value};"),
      Sink::Return => format!("return {value};"),
      Sink::Drop => format!("{value};"),
    });
  }

  /// The statement `stmt` rewritten when it awaits or returns a chain the pass can write.
  fn statement(&mut self, stmt: &Statement<'a>) -> Option<()> {
    let (expr, sink) = match stmt {
      Statement::VariableDeclaration(decl) if decl.declarations.len() == 1 => {
        let d = &decl.declarations[0];
        let Some(Expression::AwaitExpression(a)) = d.init.as_ref().map(|e| e.without_parentheses()) else { return None };
        let keyword = if decl.kind == js::VariableDeclarationKind::Const { "const" } else { "let" };
        (&a.argument, Sink::Declare(keyword.to_owned(), self.source(d.id.span()).to_owned()))
      }
      Statement::ExpressionStatement(s) => match s.expression.without_parentheses() {
        Expression::AwaitExpression(a) => (&a.argument, Sink::Drop),
        Expression::AssignmentExpression(assign) if assign.operator == js::AssignmentOperator::Assign => {
          let Expression::AwaitExpression(a) = assign.right.without_parentheses() else { return None };
          (&a.argument, Sink::Assign(self.source(assign.left.span()).to_owned()))
        }
        _ => return None,
      },
      Statement::ReturnStatement(r) => match r.argument.as_ref()?.without_parentheses() {
        Expression::AwaitExpression(a) => (&a.argument, Sink::Return),
        other => (other, Sink::Return),
      },
      _ => return None,
    };
    let (base, links) = match self.chain(expr)? {
      Ok(found) => found,
      Err(why) => {
        self.notes.push((self.parsed.line(expr.span()), format!("{why}, so `fsr migrate` leaves this promise chain as written")));
        self.written.push(expr.span());
        return None;
      }
    };
    let indent = {
      let start = self.parsed.range(stmt.span()).start;
      let line_start = self.text[..start].rfind('\n').map_or(0, |i| i + 1);
      self.text[line_start..start].chars().take_while(|c| c.is_whitespace()).collect::<String>()
    };
    let mut lines = Vec::new();
    let value = format!("await {}", self.source(base.span()));
    self.written(value, &links, &sink, &mut lines, &indent);
    let joined = lines.join(&format!("\n{indent}"));
    self.edits.replace(self.parsed.range(stmt.span()), joined);
    self.written.push(expr.span());
    Some(())
  }
}

impl<'a> Visit<'a> for Walker<'_, 'a> {
  fn visit_function(&mut self, f: &js::Function<'a>, flags: oxc_syntax::scope::ScopeFlags) {
    self.asyncs.push(f.r#async);
    walk::walk_function(self, f, flags);
    self.asyncs.pop();
  }

  fn visit_arrow_function_expression(&mut self, f: &js::ArrowFunctionExpression<'a>) {
    self.asyncs.push(f.r#async);
    walk::walk_arrow_function_expression(self, f);
    self.asyncs.pop();
  }

  fn visit_statement(&mut self, stmt: &Statement<'a>) {
    if self.asyncs.last() == Some(&true) && self.statement(stmt).is_some() {
      return;
    }
    walk::walk_statement(self, stmt);
  }

  fn visit_call_expression(&mut self, call: &js::CallExpression<'a>) {
    if let Expression::StaticMemberExpression(member) = call.callee.without_parentheses() {
      let method = member.property.name.as_str();
      let promise = matches!(method, "then" | "catch" | "finally") && call.arguments.len() <= 2;
      if promise && !self.written.iter().any(|s| s.start <= call.span.start && call.span.end <= s.end) {
        let why = if self.asyncs.last() == Some(&true) { "a promise chain that is not awaited or returned whole" } else { "a promise chain outside an async function" };
        self.notes.push((self.parsed.line(call.span), format!("{why}: `fsr migrate` leaves `.{method}()` as written, and the build refuses it; write it with `await`")));
        self.written.push(call.span);
      }
    }
    walk::walk_call_expression(self, call);
  }
}
