use super::*;

pub(super) fn find_value<'a>(parsed: &'a Parsed<'a>, name: &str) -> Option<Global<'a>> {
  for item in &parsed.module().body {
    let decl = match crate::syntax::top(item) {
      crate::syntax::Top::Decl(decl) => decl,
      crate::syntax::Top::DefaultFunction(f) => {
        if name == "default" || f.id.as_ref().is_some_and(|id| id.name.as_str() == name) {
          let body = f.body.as_ref()?;
          return Some(Global::Function(crate::params_of(&f.params), FunctionBody::Block(&body.statements), f.r#async));
        }
        continue;
      }
      crate::syntax::Top::DefaultExpr(e) if name == "default" => {
        return match crate::meaning::bare(e) {
          js::Expression::Identifier(id) => find_value(parsed, id.name.as_str()),
          js::Expression::ArrowFunctionExpression(arrow) => Some(Global::Function(crate::params_of(&arrow.params), arrow_body(arrow), arrow.r#async)),
          value => Some(Global::Const(value)),
        };
      }
      _ => continue,
    };
    match decl {
      js::Declaration::FunctionDeclaration(f) if f.id.as_ref().is_some_and(|id| id.name.as_str() == name) => {
        let body = f.body.as_ref()?;
        return Some(Global::Function(crate::params_of(&f.params), FunctionBody::Block(&body.statements), f.r#async));
      }
      js::Declaration::VariableDeclaration(var) => {
        for d in &var.declarations {
          if matches!(&d.id, js::BindingPattern::BindingIdentifier(id) if id.name.as_str() == name) {
            return match d.init.as_ref()? {
              js::Expression::ArrowFunctionExpression(arrow) => Some(Global::Function(crate::params_of(&arrow.params), arrow_body(arrow), arrow.r#async)),
              init => Some(Global::Const(init)),
            };
          }
        }
      }
      _ => {}
    }
  }
  None
}

/// The module's top-level `let` and `const` declarations with a value that is not a function, in order: each name, its initialiser and whether a function in the module writes it.
pub(super) fn module_lets_of<'e>(parsed: &'e Parsed<'e>) -> Vec<(String, Option<&'e js::Expression<'e>>, bool)> {
  let writes = module_writes(&parsed.module());
  let mutations = module_mutations(&parsed.module());
  let mut out = Vec::new();
  for item in &parsed.module().body {
    let var = match item {
      js::Statement::VariableDeclaration(var) => var,
      js::Statement::ExportDeclaration(export) => match &export.declaration {
        js::Declaration::VariableDeclaration(var) => var,
        _ => continue,
      },
      _ => continue,
    };
    for decl in &var.declarations {
      let js::BindingPattern::BindingIdentifier(id) = &decl.id else { continue };
      let init = decl.init.as_ref();
      if matches!(init, Some(js::Expression::ArrowFunctionExpression(_) | js::Expression::FunctionExpression(_))) {
        continue;
      }
      let name = id.name.to_string();
      // A module-level collection a function changes in place, `cache.set(k, v)` on a `new Map()`, is state too.
      let mutated = init.is_some_and(|init| crate::made_kind(init).is_some()) && mutations.contains(&name);
      let written = (var.kind != js::VariableDeclarationKind::Const && writes.contains(&name)) || mutated;
      out.push((name, init, written));
    }
  }
  out
}

/// Whether `expr` reads the machine, a file or the clock: an extension member past `Render` reach, or `now`.
pub(super) fn reads_host(expr: &Expr, natives: &[(String, Reach)]) -> bool {
  let mut found = false;
  expr.visit(&mut |e| match e {
    Expr::Ext { module, name, .. } => {
      let key = format!("{module}.{name}");
      let reach = snapfire_fsr_ir::standard_reach(module, name).or_else(|| natives.iter().find(|(n, _)| *n == key).map(|(_, r)| *r));
      found |= reach.is_some_and(|r| r != Reach::Render);
    }
    Expr::Now => found = true,
    _ => {}
  });
  found
}

/// Whether `expr` reads any cell.
pub(super) fn reads_a_cell(expr: &Expr) -> bool {
  let mut found = false;
  expr.visit(&mut |e| found |= matches!(e, Expr::ReadCell(_)));
  found
}

/// Whether `expr` reads or writes the cell bound to `name`.
pub(super) fn held_in_a_cell(expr: &Expr, name: &str) -> bool {
  let mut found = false;
  expr.visit(&mut |e| {
    if let Expr::ReadCell(handle) | Expr::WriteCell(handle, _) = e {
      found |= matches!(&**handle, Expr::Var(n) if n == name);
    }
  });
  found
}

/// The declaration span of `name` when it is a module-level `let` or `var` that a function in the module writes.
pub(super) fn written_module_let(parsed: &Parsed, name: &str) -> Option<Span> {
  if !module_lets_of(parsed).iter().any(|(n, _, written)| n == name && *written) {
    return None;
  }
  for item in &parsed.module().body {
    let var = match item {
      js::Statement::VariableDeclaration(var) => var,
      js::Statement::ExportDeclaration(export) => match &export.declaration {
        js::Declaration::VariableDeclaration(var) => var,
        _ => continue,
      },
      _ => continue,
    };
    if let Some(decl) = var.declarations.iter().find(|d| matches!(&d.id, js::BindingPattern::BindingIdentifier(id) if id.name.as_str() == name)) {
      return Some(decl.span);
    }
  }
  None
}

/// The specifier string of every value import, `export * from` and
/// `export { .. } from` in `module`.
pub(super) fn value_sources<'e>(module: &'e js::Program<'e>) -> impl Iterator<Item = &'e js::StringLiteral<'e>> {
  module.body.iter().filter_map(|item| {
    let decl = item;
    match decl {
      js::Statement::ImportDeclaration(import) if !import.import_kind.is_type() && (import.specifiers.as_ref().is_none_or(|s| s.is_empty()) || import.specifiers.iter().flatten().any(|spec| !matches!(spec, js::ImportDeclarationSpecifier::ImportSpecifier(named) if named.import_kind.is_type()))) => Some(&import.source),
      js::Statement::ExportAllDeclaration(export) if !export.export_kind.is_type() => Some(&export.source),
      js::Statement::ExportFromDeclaration(export) if !export.export_kind.is_type() => Some(&export.source),
      _ => None,
    }
  })
}

/// Every `import(`prefix${x}suffix`)` in `module`: where the call starts and
/// the literal text on either side of its one substitution.
pub(super) fn template_import_calls(module: &js::Program) -> Vec<(u32, String, String)> {
  struct Found(Vec<(u32, String, String)>);
  impl<'a> oxc_ast_visit::Visit<'a> for Found {
    fn visit_import_expression(&mut self, call: &js::ImportExpression<'a>) {
      if let js::Expression::TemplateLiteral(tpl) = &call.source {
        if let ([before, after], [_]) = (tpl.quasis.as_slice(), tpl.expressions.as_slice()) {
          let text = |q: &js::TemplateElement| q.value.cooked.map(crate::lossy).unwrap_or_else(|| q.value.raw.to_string());
          self.0.push((crate::lo(call.span), text(before), text(after)));
        }
      }
      oxc_ast_visit::walk::walk_import_expression(self, call);
    }
  }
  let mut found = Found(Vec::new());
  module.walk(&mut found);
  found.0
}

/// Whether every `await` in a component's body awaits an `import(...)`, which
/// the build resolves itself, so the component may be `async`.
pub(super) fn awaits_only_imports(found: &Found<'_>) -> bool {
  struct Awaits(bool);
  impl<'a> oxc_ast_visit::Visit<'a> for Awaits {
    fn visit_await_expression(&mut self, awaited: &js::AwaitExpression<'a>) {
      let mut arg = &awaited.argument;
      while let js::Expression::ParenthesizedExpression(inner) = arg {
        arg = &inner.expression;
      }
      if !matches!(arg, js::Expression::ImportExpression(_)) {
        self.0 = false;
      }
      oxc_ast_visit::walk::walk_await_expression(self, awaited);
    }
    fn visit_for_of_statement(&mut self, stmt: &js::ForOfStatement<'a>) {
      if stmt.r#await {
        self.0 = false;
      }
      oxc_ast_visit::walk::walk_for_of_statement(self, stmt);
    }
  }
  let mut awaits = Awaits(true);
  match found {
    Found::Declared(_, stmts, _) => stmts.iter().for_each(|s| s.walk(&mut awaits)),
    Found::Arrow(arrow) => match crate::syntax::arrow_body_of(arrow) {
      crate::syntax::ArrowBody::Block(block) => block.body.iter().for_each(|s| s.walk(&mut awaits)),
      crate::syntax::ArrowBody::Expr(e) => e.walk(&mut awaits),
    },
  }
  awaits.0
}

/// The call an `await import(...)` awaits, through parentheses.
pub(super) fn awaited_import<'e>(init: &'e js::Expression<'e>) -> Option<&'e js::ImportExpression<'e>> {
  let mut expr = init;
  while let js::Expression::ParenthesizedExpression(inner) = expr {
    expr = &inner.expression;
  }
  let js::Expression::AwaitExpression(awaited) = expr else { return None };
  let mut arg = &awaited.argument;
  while let js::Expression::ParenthesizedExpression(inner) = arg {
    arg = &inner.expression;
  }
  match arg {
    js::Expression::ImportExpression(call) => Some(call),
    _ => None,
  }
}

/// Every `import("literal")` in `module`, wherever it sits: a module the browser loads on demand, which the build compiles like one it imports statically and never preloads.
pub(super) fn dynamic_sources(module: &js::Program) -> Vec<String> {
  struct Dynamic(Vec<String>);
  impl<'a> oxc_ast_visit::Visit<'a> for Dynamic {
    fn visit_import_expression(&mut self, call: &js::ImportExpression<'a>) {
      if let js::Expression::StringLiteral(s) = &call.source {
        self.0.push(crate::lossy(s.value));
      }
      oxc_ast_visit::walk::walk_import_expression(self, call);
    }
  }
  let mut found = Dynamic(Vec::new());
  module.walk(&mut found);
  found.0
}

/// The app-relative path an asset specifier names from `file`, or `None`
/// for one that is not relative.
pub(crate) fn asset_path(file: &str, specifier: &str) -> Option<String> {
  let clean = specifier.split(['?', '#']).next().unwrap_or(specifier);
  crate::resolve_specifier(file, clean)
}

/// `{ src, width, height, sources }`, what an imported image is bound to on
/// the server, so a component may read any field of it and a `meta` may
/// preload it. The browser's binding has the first three.
pub(super) fn asset_object(facts: &ImageFacts) -> Expr {
  Expr::Object(vec![
    Entry::Field("src".to_owned(), Expr::lit_str(facts.src.clone())),
    Entry::Field("width".to_owned(), Expr::Lit(Lit::Int(i128::from(facts.width)))),
    Entry::Field("height".to_owned(), Expr::Lit(Lit::Int(i128::from(facts.height)))),
    Entry::Field(
      "sources".to_owned(),
      Expr::Array(
        facts
          .sources
          .iter()
          .map(|(mime, srcset)| Entry::Item(Expr::Object(vec![Entry::Field("type".to_owned(), Expr::lit_str(mime.clone())), Entry::Field("srcset".to_owned(), Expr::lit_str(srcset.clone()))])))
          .collect(),
      ),
    ),
  ])
}

/// The facts back out of what `asset_object` wrote, so a record of imports
/// bound as a global reads as the images it holds.
pub(crate) fn asset_facts(expr: &Expr) -> Option<ImageFacts> {
  let Expr::Object(entries) = expr else { return None };
  let field = |name: &str| entries.iter().find_map(|e| match e {
    Entry::Field(n, v) if n == name => Some(v),
    _ => None,
  });
  let Expr::Lit(Lit::Str(src)) = field("src")? else { return None };
  let Expr::Lit(Lit::Int(width)) = field("width")? else { return None };
  let Expr::Lit(Lit::Int(height)) = field("height")? else { return None };
  let Expr::Array(items) = field("sources")? else { return None };
  let mut sources = Vec::new();
  for item in items {
    let Entry::Item(Expr::Object(fields)) = item else { return None };
    let get = |name: &str| fields.iter().find_map(|e| match e {
      Entry::Field(n, Expr::Lit(Lit::Str(v))) if n == name => Some(v.clone()),
      _ => None,
    });
    sources.push((get("type")?, get("srcset")?));
  }
  Some(ImageFacts { src: src.clone(), width: *width as u32, height: *height as u32, sources })
}

/// Whether a lowered value is a record of imported images, which stays
/// inline rather than named as a constant, so a `Picture` indexing it can
/// read the entries.
pub(crate) fn is_asset_record(expr: &Expr) -> bool {
  let Expr::Object(entries) = expr else { return false };
  !entries.is_empty() && entries.iter().all(|e| matches!(e, Entry::Field(_, value) if asset_facts(value).is_some()))
}

/// `native("module.member", f?)` from the standard library: the pair's
/// name and its reach, `render` with a browser half and `body` without.
/// `None` for any other initialiser.
pub(super) fn native_declaration(parsed: &Parsed, init: &js::Expression) -> Result<Option<(String, String, Reach)>, LowerError> {
  let js::Expression::CallExpression(call) = init else { return Ok(None) };
  let callee = &call.callee;
  if parsed.meanings.get().and_then(|meanings| meanings.of_expr(callee)).and_then(|meant| meant.std()) != Some("native") {
    return Ok(None);
  }
  let name = match call.arguments.first().map(|a| crate::syntax::arg_expr(a)) {
    Some(js::Expression::StringLiteral(s)) => crate::lossy(s.value),
    Some(other) => return Err(LowerError::Extension(parsed.residue(other.span(), "the name of a native pair must be a string literal"))),
    None => return Err(LowerError::Extension(parsed.residue(call.span, "`native` takes the pair's name, `module.member`"))),
  };
  let Some((module, member)) = name.split_once('.') else {
    return Err(LowerError::Extension(parsed.residue(call.span, format!("`{name}` is not a pair name; write `module.member`"))));
  };
  if module.is_empty() || member.is_empty() || module.contains('.') || member.contains('.') {
    return Err(LowerError::Extension(parsed.residue(call.span, format!("`{name}` is not a pair name; write `module.member`"))));
  }
  let reach = if call.arguments.len() >= 2 { Reach::Render } else { Reach::Body };
  Ok(Some((module.to_owned(), member.to_owned(), reach)))
}

/// `(source, imported name)` for a value import binding `local`.
/// A JSX tag's name as written, `Ns.Theme.Provider` for a member chain.
pub(super) fn jsx_tag_name(name: &js::JSXElementName) -> String {
  match name {
    js::JSXElementName::Identifier(id) => id.name.to_string(),
    js::JSXElementName::IdentifierReference(id) => id.name.to_string(),
    js::JSXElementName::MemberExpression(m) => format!("{}.{}", jsx_object_name(&m.object), m.property.name),
    js::JSXElementName::NamespacedName(n) => format!("{}:{}", n.namespace.name, n.name.name),
    js::JSXElementName::ThisExpression(_) => "this".to_owned(),
  }
}

pub(super) fn jsx_object_name(obj: &js::JSXMemberExpressionObject) -> String {
  match obj {
    js::JSXMemberExpressionObject::IdentifierReference(id) => id.name.to_string(),
    js::JSXMemberExpressionObject::MemberExpression(m) => format!("{}.{}", jsx_object_name(&m.object), m.property.name),
    js::JSXMemberExpressionObject::ThisExpression(_) => "this".to_owned(),
  }
}

pub(crate) fn find_import(parsed: &Parsed, local: &str) -> Option<(String, String)> {
  for item in &parsed.module().body {
    let js::Statement::ImportDeclaration(import) = item else { continue };
    if import.import_kind.is_type() {
      continue;
    }
    for spec in import.specifiers.iter().flatten() {
      match spec {
        js::ImportDeclarationSpecifier::ImportSpecifier(named) if !named.import_kind.is_type() && named.local.name.as_str() == local => {
          let imported = crate::schema::export_name(&named.imported);
          return Some((crate::lossy(import.source.value), imported));
        }
        js::ImportDeclarationSpecifier::ImportDefaultSpecifier(d) if d.local.name.as_str() == local => {
          return Some((crate::lossy(import.source.value), "default".to_owned()));
        }
        _ => {}
      }
    }
  }
  None
}
