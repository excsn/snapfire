//! Extraction: the state, the handlers and the effects a page or a layout
//! holds, moved with the smallest part of its markup that uses them into an
//! island module beside it, so that the page itself is composition.

use std::collections::BTreeSet;
use std::ops::Range;

use oxc_span::{GetSpan, Span};
use oxc_ast::ast as js;
use crate::syntax::Walk;

use crate::component::{find_function, Found};
use crate::Parsed;

/// A page with its browser half moved out.
#[non_exhaustive]
pub struct Extraction {
  /// The page's source as composition, placing the island where the moved markup stood.
  pub page: String,
  /// The island module's file, beside the page's.
  pub island_file: String,
  pub island: String,
  /// The names the island took from the page's body, for the report.
  pub holds: Vec<String>,
}

/// What a page that cannot be split keeps: the reason, for the report.
pub struct Kept(pub String);

/// The name the page places the island under.
const PLACED: &str = "SfIsland";

/// Hooks whose bindings only a framework keeps. `useLocale` and `useMemo`
/// lower to values the server computes.
fn browser_hook(name: &str) -> bool {
  name.len() > 3 && name.starts_with("use") && name[3..].starts_with(|c: char| c.is_ascii_uppercase()) && !matches!(name, "useLocale" | "useMemo")
}

fn hook_name<'e>(expr: &'e js::Expression<'e>) -> Option<&'e str> {
  let js::Expression::CallExpression(call) = expr else { return None };
  let callee = &call.callee;
  match callee {
    js::Expression::Identifier(id) => Some(id.name.as_str()),
    member_expr if member_expr.is_member_expression() => { let member = member_expr.as_member_expression().expect("matched as a member"); match crate::syntax::prop(member) {
      crate::syntax::MemberProp::Ident(prop) => Some(prop.name.as_str()),
      _ => None,
    } },
    _ => None,
  }
}

/// The names a function's parameters bind, its rest included.
fn bound_params(params: &js::FormalParameters, out: &mut Vec<String>) {
  params.items.iter().for_each(|p| bound(&p.pattern, out));
  if let Some(rest) = &params.rest {
    bound(&rest.rest.argument, out);
  }
}

fn bound(pat: &js::BindingPattern, out: &mut Vec<String>) {
  match pat {
    js::BindingPattern::BindingIdentifier(id) => out.push(id.name.to_string()),
    js::BindingPattern::ArrayPattern(array) => {
      array.elements.iter().flatten().for_each(|p| bound(p, out));
      if let Some(rest) = &array.rest {
        bound(&rest.argument, out);
      }
    }
    js::BindingPattern::ObjectPattern(object) => {
      for prop in crate::syntax::pat_props(object) {
        match prop {
          crate::syntax::PatProp::KeyValue(kv) => bound(&kv.value, out),
          crate::syntax::PatProp::Assign(assign) => out.push(assign.key.name.to_string()),
          crate::syntax::PatProp::Rest(rest) => bound(&rest.argument, out),
        }
      }
    }
    js::BindingPattern::AssignmentPattern(assign) => bound(&assign.left, out),
  }
}

/// Every identifier a node reads or binds, which is a superset of what it reads.
#[derive(Default)]
struct Names(BTreeSet<String>);

impl<'a> oxc_ast_visit::Visit<'a> for Names {
  fn visit_identifier_reference(&mut self, ident: &js::IdentifierReference<'a>) {
    self.0.insert(ident.name.to_string());
  }
}

fn names_of<'a, N: crate::syntax::Walk<'a> + ?Sized>(node: &N) -> BTreeSet<String> {
  let mut names = Names::default();
  node.walk(&mut names);
  names.0
}

fn is_event(attr: &js::JSXAttribute) -> bool {
  let js::JSXAttributeName::Identifier(name) = &attr.name else { return false };
  let name = name.name.as_str();
  let event = name.len() > 2 && name.starts_with("on") && name[2..].starts_with(|c: char| c.is_ascii_uppercase());
  event && matches!(&attr.value, Some(js::JSXAttributeValue::ExpressionContainer(_)))
}

/// A node of the markup an island can be cut at: an element, a fragment or an
/// expression container in child position, where a placement can stand.
#[derive(Clone)]
struct Cut {
  span: Span,
  /// What the island returns: the node itself or a container's expression without its braces.
  body: Span,
  /// A function inside the markup encloses it, whose parameters the island would not have.
  inner: bool,
  /// The element's name when the node is a plain element.
  tag: Option<String>,
}

/// Elements whose content model names their children, per the HTML standard:
/// a list, a table and its sections, a row, a column group and a select. An
/// island's boundary element is not among the children any of them admits.
const NAMES_ITS_CHILDREN: &[&str] = &["ul", "ol", "menu", "dl", "table", "thead", "tbody", "tfoot", "tr", "colgroup", "select", "optgroup"];

fn admits_island(tag: &str) -> bool {
  !NAMES_ITS_CHILDREN.contains(&tag)
}

/// Where in the markup the browser half is used: for each use, the cuts
/// around it from the outermost in.
struct Uses<'a> {
  browser: &'a BTreeSet<String>,
  stack: Vec<Cut>,
  depth: usize,
  found: Vec<Vec<Cut>>,
  /// Where the lowerer stopped inside the markup, with the parsed file to read spans against.
  residue: Option<(usize, &'a Parsed<'a>)>,
  /// The cuts around the residue, the deepest holding it last.
  at: Option<Vec<Cut>>,
}

impl Uses<'_> {
  fn enter(&mut self, span: Span, body: Span, tag: Option<String>) {
    self.stack.push(Cut { span, body, inner: self.depth > 0, tag });
    if let Some((offset, parsed)) = self.residue {
      if parsed.range(span).contains(&offset) {
        self.at = Some(self.stack.clone());
      }
    }
  }
}

impl<'a> oxc_ast_visit::Visit<'a> for Uses<'_> {
  fn visit_jsx_element(&mut self, el: &js::JSXElement<'a>) {
    let tag = match &el.opening_element.name {
      js::JSXElementName::Identifier(ident) => Some(ident.name.to_string()),
      js::JSXElementName::IdentifierReference(ident) => Some(ident.name.to_string()),
      _ => None,
    };
    self.enter(el.span, el.span, tag);
    oxc_ast_visit::walk::walk_jsx_element(self, el);
    self.stack.pop();
  }

  fn visit_jsx_child(&mut self, child: &js::JSXChild<'a>) {
    let cut = match child {
      js::JSXChild::ExpressionContainer(container) => container.expression.as_expression().map(|expr| (container.span, expr.span())),
      js::JSXChild::Fragment(fragment) => Some((fragment.span, fragment.span)),
      _ => None,
    };
    let Some((span, body)) = cut else {
      oxc_ast_visit::walk::walk_jsx_child(self, child);
      return;
    };
    self.enter(span, body, None);
    oxc_ast_visit::walk::walk_jsx_child(self, child);
    self.stack.pop();
  }

  fn visit_jsx_attribute(&mut self, attr: &js::JSXAttribute<'a>) {
    if is_event(attr) {
      self.found.push(self.stack.clone());
    }
    oxc_ast_visit::walk::walk_jsx_attribute(self, attr);
  }

  fn visit_identifier_reference(&mut self, ident: &js::IdentifierReference<'a>) {
    if self.browser.contains(ident.name.as_str()) {
      self.found.push(self.stack.clone());
    }
  }

  fn visit_arrow_function_expression(&mut self, arrow: &js::ArrowFunctionExpression<'a>) {
    self.depth += 1;
    oxc_ast_visit::walk::walk_arrow_function_expression(self, arrow);
    self.depth -= 1;
  }

  fn visit_function(&mut self, function: &js::Function<'a>, flags: oxc_syntax::scope::ScopeFlags) {
    self.depth += 1;
    oxc_ast_visit::walk::walk_function(self, function, flags);
    self.depth -= 1;
  }
}

/// A statement of the component's body and what it binds.
struct Line<'a> {
  stmt: &'a js::Statement<'a>,
  names: Vec<String>,
  browser: bool,
  function: bool,
}

/// Where the default export reaches `offset`: `offset` itself when it lies in
/// the export's body, else the first reference in the body to the
/// module-level function holding it, followed through the module-level
/// functions between. `None` when the body never reaches it.
pub(crate) fn page_offset(parsed: &Parsed, offset: usize) -> Option<usize> {
  let body = match find_function(parsed, "default")? {
    Found::Declared(_, _, span) => parsed.range(span),
    Found::Arrow(arrow) => parsed.range(arrow.span),
  };
  if body.contains(&offset) {
    return Some(offset);
  }
  let mut functions: Vec<(String, Range<usize>)> = Vec::new();
  for item in &parsed.module().body {
    let decl = match item {
      js::Statement::ExportDeclaration(export) => &export.declaration,
      other => match other.as_declaration() {
        Some(decl) => decl,
        None => continue,
      },
    };
    match decl {
      js::Declaration::FunctionDeclaration(f) => functions.extend(f.id.as_ref().map(|id| (id.name.to_string(), parsed.range(f.span)))),
      js::Declaration::VariableDeclaration(var) => {
        for d in &var.declarations {
          if let (js::BindingPattern::BindingIdentifier(id), Some(init @ (js::Expression::ArrowFunctionExpression(_) | js::Expression::FunctionExpression(_)))) = (&d.id, d.init.as_ref()) {
            functions.push((id.name.to_string(), parsed.range(init.span())));
          }
        }
      }
      _ => {}
    }
  }
  struct Refs<'a>(&'a Parsed<'a>, Vec<(String, usize)>);
  impl<'a> oxc_ast_visit::Visit<'a> for Refs<'_> {
    fn visit_identifier_reference(&mut self, ident: &js::IdentifierReference<'a>) {
      self.1.push((ident.name.to_string(), self.0.range(ident.span).start));
    }
  }
  let mut refs = Refs(parsed, Vec::new());
  parsed.module().walk(&mut refs);
  let mut at = offset;
  let mut seen: Vec<String> = Vec::new();
  loop {
    let (name, range) = functions.iter().find(|(name, range)| range.contains(&at) && !seen.contains(name))?.clone();
    let uses = |within: &Range<usize>| refs.1.iter().filter(|(n, o)| *n == name && within.contains(o) && !range.contains(o)).map(|(_, o)| *o).min();
    if let Some(found) = uses(&body) {
      return Some(found);
    }
    seen.push(name.clone());
    at = functions.iter().filter(|(other, _)| !seen.contains(other)).find_map(|(_, within)| uses(within))?;
  }
}

/// Splits `file`'s default export, a page or a layout, into composition and
/// an island. `None` when it holds nothing a framework has to run. `slots` are
/// the layout's slot props, which composition fills and an island cannot take.
pub(crate) fn extract(parsed: &Parsed, file: &str, slots: &[String], residue: Option<usize>, whole: bool) -> Result<Option<Extraction>, Kept> {
  let source = parsed.text().to_owned();
  let Some(found) = find_function(parsed, "default") else { return Ok(None) };
  let (params, stmts, ret): (&js::FormalParameters, &[js::Statement], &js::Expression) = match found {
    Found::Declared(params, stmts, _) => {
      let Some(ret) = stmts.iter().rev().find_map(|s| match s {
        js::Statement::ReturnStatement(ret) => ret.argument.as_ref(),
        _ => None,
      }) else {
        return Ok(None);
      };
      (params, stmts, ret)
    }
    Found::Arrow(arrow) => match &arrow.body {
      js::ArrowFunctionBody::FunctionBody(body) => {
        let Some(ret) = body.statements.iter().rev().find_map(|s| match s {
          js::Statement::ReturnStatement(ret) => ret.argument.as_ref(),
          _ => None,
        }) else {
          return Ok(None);
        };
        (&arrow.params, &body.statements[..], ret)
      }
      body => (&arrow.params, &[][..], body.as_expression().expect("an arrow body is a block or an expression")),
    },
  };

  let mut lines: Vec<Line> = Vec::new();
  for stmt in stmts {
    match stmt {
      js::Statement::ReturnStatement(_) => break,
      js::Statement::VariableDeclaration(var) => {
        let mut names = Vec::new();
        let mut browser = false;
        let mut function = false;
        for decl in &var.declarations {
          bound(&decl.id, &mut names);
          match decl.init.as_ref() {
            Some(init) if hook_name(init).is_some_and(browser_hook) => browser = true,
            Some(js::Expression::ArrowFunctionExpression(_) | js::Expression::FunctionExpression(_)) => function = true,
            _ => {}
          }
        }
        lines.push(Line { stmt, names, browser, function });
      }
      js::Statement::FunctionDeclaration(f) => lines.push(Line { stmt, names: f.id.iter().map(|id| id.name.to_string()).collect(), browser: false, function: true }),
      js::Statement::ExpressionStatement(e) if hook_name(&e.expression).is_some_and(browser_hook) => lines.push(Line { stmt, names: Vec::new(), browser: true, function: false }),
      other => lines.push(Line { stmt: other, names: Vec::new(), browser: false, function: false }),
    }
  }
  // A statement the lowerer stopped in only the browser can run.
  if let Some(offset) = residue {
    for line in &mut lines {
      if parsed.range(line.stmt.span()).contains(&offset) {
        line.browser = true;
      }
    }
  }
  let early = stmts.iter().any(|s| matches!(s, js::Statement::IfStatement(_)));

  // A function an event attribute names runs in the browser, and so does
  // anything that reads what the browser holds.
  let mut handlers = Names::default();
  {
    struct Events<'a>(&'a mut Names);
    impl<'a> oxc_ast_visit::Visit<'a> for Events<'_> {
      fn visit_jsx_attribute(&mut self, attr: &js::JSXAttribute<'a>) {
        if is_event(attr) {
          if let Some(value) = &attr.value {
            oxc_ast_visit::Visit::visit_jsx_attribute_value(&mut *self.0, value);
          }
        }
      }
    }
    ret.walk(&mut Events(&mut handlers));
  }
  let mut browser: BTreeSet<String> = BTreeSet::new();
  loop {
    let mut grew = false;
    for line in &mut lines {
      if line.browser {
        continue;
      }
      let reads = names_of(line.stmt);
      let named = line.function && line.names.iter().any(|n| handlers.0.contains(n));
      if named || reads.iter().any(|n| browser.contains(n) && !line.names.contains(n)) {
        line.browser = true;
      }
    }
    for line in lines.iter().filter(|l| l.browser) {
      for name in &line.names {
        grew |= browser.insert(name.clone());
      }
    }
    if !grew {
      break;
    }
  }
  let effects = lines.iter().any(|l| l.browser && l.names.is_empty());
  let in_markup = residue.filter(|offset| parsed.range(ret.span()).contains(offset));
  let mut uses = Uses { browser: &browser, stack: Vec::new(), depth: 0, found: Vec::new(), residue: in_markup.map(|offset| (offset, parsed)), at: None };
  ret.walk(&mut uses);
  if let Some(at) = uses.at.take() {
    uses.found.push(at);
  } else if in_markup.is_some() {
    uses.found.push(Vec::new());
  }
  if uses.found.is_empty() && !effects {
    return Ok(None);
  }

  // The unit: the deepest cut every use sits in, that no function inside the
  // markup encloses and whose nearest element above admits an island; the
  // whole tree when there is none, when an early return chooses between
  // trees or when `whole` asks for it.
  let mut unit: Option<Cut> = None;
  if !early && !whole && !uses.found.is_empty() {
    let first = &uses.found[0];
    let common = (0..first.len()).take_while(|&i| uses.found.iter().all(|stack| stack.get(i).map(|cut| cut.span) == Some(first[i].span))).count();
    let shared = &first[..common];
    unit = (0..common)
      .rev()
      .filter(|&i| !shared[i].inner)
      .find(|&i| shared[..i].iter().rev().find_map(|cut| cut.tag.as_deref()).is_none_or(admits_island))
      .map(|i| shared[i].clone());
  }
  let range_of = |span: Span| parsed.range(span);
  let unit_range: Range<usize> = match (&unit, &found) {
    (Some(cut), _) => range_of(cut.span),
    (None, _) if early => range_of(Span::new(stmts.first().map(|s| s.span().start).unwrap_or_default(), stmts.last().map(|s| s.span().end).unwrap_or_default())),
    (None, _) => range_of(ret.span()),
  };
  let whole_body = unit.is_none() && early;

  let moved: Vec<&Line> = lines.iter().filter(|l| l.browser || whole_body).collect();
  let unit_text = &source[unit.as_ref().map(|cut| range_of(cut.body)).unwrap_or(unit_range.clone())];
  let moved_text: Vec<&str> = moved.iter().map(|l| &source[range_of(l.stmt.span())]).collect();

  let mut scope: Vec<String> = Vec::new();
  bound_params(params, &mut scope);
  for line in lines.iter().filter(|l| !l.browser && !whole_body) {
    scope.extend(line.names.iter().cloned());
  }
  let mut reads = BTreeSet::new();
  {
    let mut names = Names::default();
    for line in &moved {
      line.stmt.walk(&mut names);
    }
    if whole_body {
      for stmt in stmts {
        stmt.walk(&mut names);
      }
    } else {
      struct Within<'a> {
        range: Range<usize>,
        parsed: &'a Parsed<'a>,
        names: &'a mut Names,
      }
      impl<'a> oxc_ast_visit::Visit<'a> for Within<'_> {
        fn visit_identifier_reference(&mut self, ident: &js::IdentifierReference<'a>) {
          let at = self.parsed.range(ident.span);
          if at.start >= self.range.start && at.end <= self.range.end {
            self.names.0.insert(ident.name.to_string());
          }
        }
      }
      ret.walk(&mut Within { range: unit_range.clone(), parsed, names: &mut names });
    }
    reads.extend(names.0);
  }
  let props: Vec<String> = scope.iter().filter(|n| reads.contains(*n)).cloned().collect::<BTreeSet<_>>().into_iter().collect();
  if let Some(slot) = props.iter().find(|p| slots.contains(p)) {
    return Err(Kept(format!("its state reaches the slot `{slot}`, which composition fills and an island cannot take")));
  }

  // The module's own imports and every module-level item the island reads,
  // the component itself aside.
  let own = default_function_name(parsed);
  let mut items: Vec<(Range<usize>, Vec<String>)> = Vec::new();
  let mut imports: Vec<Range<usize>> = Vec::new();
  for item in &parsed.module().body {
    match item {
      js::Statement::ImportDeclaration(import) => imports.push(range_of(import.span)),
      js::Statement::ExportDefaultDeclaration(_) => {}
      js::Statement::ExportDeclaration(export) => items.push((range_of(export.span), decl_names(&export.declaration))),
      other => {
        if let Some(decl) = other.as_declaration() {
          items.push((range_of(decl.span()), decl_names(decl)));
        }
      }
    }
  }
  items.retain(|(_, names)| own.as_ref().is_none_or(|own| !names.contains(own)));
  let mut wanted: BTreeSet<String> = reads.clone();
  let mut taken = vec![false; items.len()];
  loop {
    let mut grew = false;
    for (i, (range, names)) in items.iter().enumerate() {
      if !taken[i] && names.iter().any(|n| wanted.contains(n)) {
        taken[i] = true;
        grew = true;
        let text = &source[range.clone()];
        let arena = oxc_allocator::Allocator::default();
        if let Ok(parsed) = crate::parse_with(&arena, file, text, true) {
          wanted.extend(names_of(parsed.module()));
        }
      }
    }
    if !grew {
      break;
    }
  }

  let stem = file.rsplit_once('/').map(|(_, name)| name).unwrap_or(file);
  let stem = stem.rsplit_once('.').map(|(stem, _)| stem).unwrap_or(stem);
  let dir = file.rsplit_once('/').map(|(dir, _)| format!("{dir}/")).unwrap_or_default();
  let island_file = format!("{dir}{stem}.island0.tsx");
  let name = format!("{}Island", own.as_deref().unwrap_or("Page"));
  let takes_children = props.iter().any(|p| p == "children");
  let passed: Vec<&String> = props.iter().filter(|p| *p != "children").collect();

  let mut island = String::new();
  for range in &imports {
    island.push_str(&source[range.clone()]);
    island.push('\n');
  }
  for (i, (range, _)) in items.iter().enumerate() {
    if taken[i] {
      island.push_str(&source[range.clone()]);
      island.push('\n');
    }
  }
  let destructured = props.join(", ");
  island.push_str(&format!("\nexport default function {name}({{ {destructured} }}: any) {{\n"));
  if whole_body {
    island.push_str("  ");
    island.push_str(unit_text);
    island.push('\n');
  } else {
    for text in &moved_text {
      island.push_str("  ");
      island.push_str(text);
      island.push('\n');
    }
    island.push_str("  return (\n    ");
    island.push_str(unit_text);
    island.push_str("\n  );\n");
  }
  island.push_str("}\n");

  let attrs: String = passed.iter().map(|p| format!(" {p}={{{p}}}")).collect();
  let placement = match takes_children {
    true => format!("<{PLACED}{attrs}>{{children}}</{PLACED}>"),
    false => format!("<{PLACED}{attrs} />"),
  };
  let mut edits: Vec<(Range<usize>, String)> = Vec::new();
  if whole_body {
    edits.push((unit_range, format!("return {placement};")));
  } else {
    edits.push((unit_range, placement));
    for line in &moved {
      edits.push((range_of(line.stmt.span()), String::new()));
    }
  }
  edits.sort_by(|a, b| b.0.start.cmp(&a.0.start));
  let mut page = source.clone();
  for (range, text) in edits {
    page.replace_range(range, &text);
  }
  page.insert_str(0, &format!("import {PLACED} from \"./{stem}.island0.tsx\";\n"));

  let holds: Vec<String> = moved.iter().flat_map(|l| l.names.iter().cloned()).collect();
  Ok(Some(Extraction { page, island_file, island, holds }))
}

fn decl_names(decl: &js::Declaration) -> Vec<String> {
  let mut out = Vec::new();
  match decl {
    js::Declaration::FunctionDeclaration(f) => out.extend(f.id.iter().map(|id| id.name.to_string())),
    js::Declaration::ClassDeclaration(c) => out.extend(c.id.iter().map(|id| id.name.to_string())),
    js::Declaration::VariableDeclaration(var) => var.declarations.iter().for_each(|d| bound(&d.id, &mut out)),
    js::Declaration::TSInterfaceDeclaration(i) => out.push(i.id.name.to_string()),
    js::Declaration::TSTypeAliasDeclaration(t) => out.push(t.id.name.to_string()),
    js::Declaration::TSEnumDeclaration(e) => out.push(e.id.name.to_string()),
    _ => {}
  }
  out
}

/// The name `export default function Name` gives the component, which the island is named after.
fn default_function_name(parsed: &Parsed) -> Option<String> {
  parsed.module().body.iter().find_map(|item| match item {
    js::Statement::ExportDefaultDeclaration(d) => match &d.declaration {
      js::ExportDefaultDeclarationKind::FunctionDeclaration(f) => f.id.as_ref().map(|id| id.name.to_string()),
      js::ExportDefaultDeclarationKind::Identifier(id) => Some(id.name.to_string()),
      _ => None,
    },
    _ => None,
  })
}
