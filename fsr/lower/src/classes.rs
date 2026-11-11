//! Classes an application declares, lowered into the value model: an instance is an object of its fields carrying its class in `$c`, each member a constant holding a function whose first parameter is the instance, a call site of an instance member dispatching on the class the value was made by.

use snapfire_fsr_ir::ast::{Entry, Lit, Stmt};
use snapfire_fsr_ir::Expr;
use oxc_span::GetSpan;
use oxc_ast::ast as js;
use crate::syntax::WalkMut;

use crate::component::{FunctionBody, Member};
use crate::{Lowered, Lowerer};

pub(crate) use snapfire_fsr_ir::interp::{class_getter_key, class_method_key, class_parent_key, CLASS_FIELD};

/// The field a class named as a value holds its id in.
pub(crate) const CLASS_VALUE: &str = "$class";

/// The name `this` is written as inside a member, the member's first parameter.
pub(crate) const THIS: &str = "$this";

/// What one class declares, by name: its instance methods and getters, its static methods, getters and fields.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct ClassInfo {
  /// `file#Name`.
  pub id: String,
  /// The id of the class it extends, when the build follows it.
  pub parent: Option<String>,
  pub methods: Vec<String>,
  pub getters: Vec<String>,
  pub statics: Vec<String>,
  pub static_getters: Vec<String>,
  pub static_fields: Vec<String>,
}

/// Every class the build has read, by id.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct ClassTable {
  pub classes: Vec<ClassInfo>,
}

/// The constant a static member of `owner` is lowered into.
pub(crate) fn static_key(owner: &str, name: &str) -> String {
  format!("{owner}::{name}")
}

/// The constant the constructor of `id` is lowered into: a function of the constructor's arguments answering the instance.
pub(crate) fn constructor_key(id: &str) -> String {
  format!("{id}.constructor")
}

impl ClassTable {
  pub(crate) fn get(&self, id: &str) -> Option<&ClassInfo> {
    self.classes.iter().find(|c| c.id == id)
  }

  /// `id` and each class it extends, nearest first.
  pub(crate) fn chain(&self, id: &str) -> Vec<&ClassInfo> {
    let mut out = Vec::new();
    let mut at = self.get(id);
    while let Some(class) = at {
      if out.iter().any(|c: &&ClassInfo| c.id == class.id) {
        break;
      }
      out.push(class);
      at = class.parent.as_deref().and_then(|p| self.get(p));
    }
    out
  }

  /// The constant `id` runs for the instance member `name`, a method or with `getter` a getter, from the nearest class on its chain that declares it.
  pub(crate) fn instance_member(&self, id: &str, name: &str, getter: bool) -> Option<String> {
    self.chain(id).into_iter().find(|c| if getter { c.getters.iter().any(|g| g == name) } else { c.methods.iter().any(|m| m == name) }).map(|c| if getter { class_getter_key(&c.id, name) } else { class_method_key(&c.id, name) })
  }

  /// Whether any class declares the instance member `name`, a method or with `getter` a getter.
  pub(crate) fn declares(&self, name: &str, getter: bool) -> bool {
    self.classes.iter().any(|c| if getter { c.getters.iter().any(|g| g == name) } else { c.methods.iter().any(|m| m == name) })
  }

  /// Whether any class declares a static member `name`.
  pub(crate) fn declares_static(&self, name: &str) -> bool {
    self.classes.iter().any(|c| c.statics.iter().chain(&c.static_getters).chain(&c.static_fields).any(|s| s == name))
  }

  /// What `id.name` is as a static member: the constant and whether it is a method, a getter or a field.
  pub(crate) fn static_member(&self, id: &str, name: &str) -> Option<(String, Static)> {
    for class in self.chain(id) {
      let found = if class.statics.iter().any(|s| s == name) {
        Some(Static::Method)
      } else if class.static_getters.iter().any(|s| s == name) {
        Some(Static::Getter)
      } else if class.static_fields.iter().any(|s| s == name) {
        Some(Static::Field)
      } else {
        None
      };
      if let Some(found) = found {
        return Some((static_key(&class.id, name), found));
      }
    }
    None
  }

  /// `id` and every class that extends it, however far down.
  pub(crate) fn descendants(&self, id: &str) -> Vec<String> {
    self.classes.iter().filter(|c| self.chain(&c.id).iter().any(|a| a.id == id)).map(|c| c.id.clone()).collect()
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Static {
  Method,
  Getter,
  Field,
}

/// The name a class member is declared under.
pub(crate) fn member_name(key: &js::PropertyKey, computed: bool) -> Option<String> {
  crate::prop_name(key, computed)
}

/// What `class` declares, its parent left for the caller to resolve.
pub(crate) fn describe(id: String, class: &js::Class) -> ClassInfo {
  let mut info = ClassInfo { id, ..ClassInfo::default() };
  for member in &class.body.body {
    match member {
      js::ClassElement::MethodDefinition(m) => {
        let Some(name) = member_name(&m.key, m.computed) else { continue };
        let list = match (m.r#static, m.kind) {
          (_, js::MethodDefinitionKind::Constructor) => continue,
          (false, js::MethodDefinitionKind::Method) => &mut info.methods,
          (false, js::MethodDefinitionKind::Get) => &mut info.getters,
          (true, js::MethodDefinitionKind::Method) => &mut info.statics,
          (true, js::MethodDefinitionKind::Get) => &mut info.static_getters,
          (_, js::MethodDefinitionKind::Set) => continue,
        };
        list.push(name);
      }
      js::ClassElement::PropertyDefinition(p) if p.r#static => {
        if let Some(name) = member_name(&p.key, p.computed) {
          info.static_fields.push(name);
        }
      }
      _ => {}
    }
  }
  info
}

/// Every module-level class `parsed` declares, by its local name, a default export's under `default` as well.
pub(crate) fn declared<'e>(module: &'e js::Program<'e>) -> Vec<(String, &'e js::Class<'e>)> {
  let mut out = Vec::new();
  for item in &module.body {
    let class = match item {
      js::Statement::ClassDeclaration(c) => c,
      js::Statement::ExportDeclaration(export) => match &export.declaration {
        js::Declaration::ClassDeclaration(c) => c,
        _ => continue,
      },
      js::Statement::ExportDefaultDeclaration(d) => {
        if let js::ExportDefaultDeclarationKind::ClassDeclaration(c) = &d.declaration {
          if let Some(id) = &c.id {
            out.push((id.name.to_string(), &**c));
          }
          out.push(("default".to_owned(), &**c));
        }
        continue;
      }
      _ => continue,
    };
    if let Some(id) = &class.id {
      out.push((id.name.to_string(), &**class));
    }
  }
  out
}

/// A class named as a value, `C` in `new C()` or `C.parse(s)`.
pub(crate) fn class_value(id: &str) -> Expr {
  Expr::Object(vec![Entry::Field(CLASS_VALUE.to_owned(), Expr::lit_str(id))])
}

/// The class `value` names, when it is one [`class_value`] made.
pub(crate) fn class_of_value(value: &Expr) -> Option<&str> {
  match value {
    Expr::Object(entries) => match entries.as_slice() {
      [Entry::Field(field, Expr::Lit(Lit::Str(id)))] if field == CLASS_VALUE => Some(id),
      _ => None,
    },
    _ => None,
  }
}

/// Whether `module` declares a module-level class `name`, the default export's under `default`.
pub(crate) fn declares_class(module: &js::Program, name: &str) -> bool {
  declared(module).iter().any(|(n, _)| n == name)
}

/// `this` as the module-level name `with` throughout a member's body, arrows included, a nested `function` or class left with its own.
struct ThisAs<'s, 'a> {
  with: &'s str,
  scope: &'s crate::scope::Scope,
  b: crate::syntax::Build<'a>,
}

impl<'a> oxc_ast_visit::VisitMut<'a> for ThisAs<'_, 'a> {
  fn visit_expression(&mut self, e: &mut js::Expression<'a>) {
    if let js::Expression::ThisExpression(t) = e {
      let id = self.b.ident_ref(self.with, t.span);
      self.scope.mark_top(&id);
      *e = js::Expression::Identifier(self.b.boxed(id));
      return;
    }
    oxc_ast_visit::walk_mut::walk_expression(self, e);
  }

  fn visit_function(&mut self, _: &mut js::Function<'a>, _: oxc_syntax::scope::ScopeFlags) {}

  fn visit_class(&mut self, _: &mut js::Class<'a>) {}
}

/// A parameter binding the module-level name `name`, which the build writes.
fn written_param<'a>(lowerer: &Lowerer<'a>, name: &str, span: oxc_span::Span) -> crate::Param<'a> {
  let b = lowerer.build();
  let pattern = b.binding(name, span);
  if let js::BindingPattern::BindingIdentifier(id) = &pattern {
    lowerer.parsed.scope.mark_top_binding(id);
  }
  crate::Param { pattern: b.alloc(pattern), type_annotation: None, default: None, rest: false, span }
}

/// The name a parameter property binds: `x` of `private x` or of `private x = 1`.
fn property_name(param: &js::FormalParameter) -> Option<String> {
  match &param.pattern {
    js::BindingPattern::BindingIdentifier(b) => Some(b.name.to_string()),
    js::BindingPattern::AssignmentPattern(a) => match &a.left {
      js::BindingPattern::BindingIdentifier(b) => Some(b.name.to_string()),
      _ => None,
    },
    _ => None,
  }
}

/// Whether a constructor parameter declares a field: `private x`, `readonly x` and the rest.
pub(crate) fn is_property(param: &js::FormalParameter) -> bool {
  param.accessibility.is_some() || param.readonly || param.r#override
}

/// The constructor `class` declares.
pub(crate) fn constructor_of<'e>(class: &'e js::Class<'e>) -> Option<&'e js::Function<'e>> {
  class.body.body.iter().find_map(|m| match m {
    js::ClassElement::MethodDefinition(c) if c.kind == js::MethodDefinitionKind::Constructor => Some(&*c.value),
    _ => None,
  })
}

/// The kinds `class` declares its fields with, from a parameter property's or a field's type.
fn field_kinds(class: &js::Class, aliases: &[(String, crate::AliasKind)]) -> Vec<(String, crate::Kind)> {
  let mut out = Vec::new();
  if let Some(c) = constructor_of(class) {
    for param in c.params.items.iter().filter(|p| is_property(p)) {
      let Some(name) = property_name(param) else { continue };
      if let Some(kind) = param.type_annotation.as_deref().and_then(|t| crate::Kind::of_type_in(&t.type_annotation, aliases)) {
        out.push((name, kind));
      }
    }
  }
  for member in &class.body.body {
    if let js::ClassElement::PropertyDefinition(p) = member {
      if p.r#static {
        continue;
      }
      let (Some(name), Some(kind)) = (member_name(&p.key, p.computed), p.type_annotation.as_deref().and_then(|t| crate::Kind::of_type_in(&t.type_annotation, aliases)).or_else(|| p.value.as_ref().and_then(crate::made_kind))) else { continue };
      out.push((name, kind));
    }
  }
  out
}

/// One member of the class `id` as the constant it is lowered into: an instance member a function of the instance and its arguments, a static one a function of its arguments or a value, the constructor a function of its arguments answering the instance. `name` is the module-level name the class is declared under.
pub(crate) fn lower_member(lowerer: &mut Lowerer<'_>, id: &str, parent: Option<&str>, class: &js::Class, name: &str, member: &Member<'_>) -> Lowered<Expr> {
  lowerer.this_kinds = field_kinds(class, &lowerer.type_kinds);
  let b = lowerer.build();
  match member {
    Member::Instance(f) | Member::Static(f) => {
      let Some(declared) = &f.body else { return Err(lowerer.residue(f.span, "a method without a body")) };
      let instance = matches!(member, Member::Instance(_));
      let with = if instance { THIS } else { name };
      let mut body = b.copy(&**declared);
      body.walk_mut(&mut ThisAs { with, scope: &lowerer.parsed.scope, b });
      let body = b.alloc(body);
      let mut params = Vec::new();
      if instance {
        params.push(written_param(lowerer, THIS, f.span));
      }
      params.extend(crate::params_of(&f.params));
      crate::component::function_to_lambda(lowerer, &params, FunctionBody::Block(&body.statements), f.r#async)
    }
    Member::StaticField(value) => {
      let mut value = b.copy(*value);
      value.walk_mut(&mut ThisAs { with: name, scope: &lowerer.parsed.scope, b });
      let value = b.alloc(value);
      lowerer.expr(value)
    }
    Member::Constructor => constructor(lowerer, id, parent, class),
  }
}

/// The constructor of `id`: the parent's instance when it extends one, else a fresh object, then its class, its parameter properties and its fields, then the constructor's own statements writing `this`, answering the instance.
fn constructor(lowerer: &mut Lowerer<'_>, id: &str, parent: Option<&str>, class: &js::Class) -> Lowered<Expr> {
  let b = lowerer.build();
  let declared = constructor_of(class);
  let mut pats = Vec::new();
  let mut properties = Vec::new();
  match declared {
    Some(c) => {
      for (param, view) in c.params.items.iter().zip(crate::params_of(&c.params)) {
        if is_property(param) {
          properties.extend(property_name(param));
        }
        pats.push(view);
      }
      if let Some(rest) = c.params.rest.as_ref() {
        pats.push(crate::Param { pattern: &rest.rest.argument, type_annotation: rest.type_annotation.as_deref(), default: None, rest: true, span: rest.span });
      }
    }
    None if parent.is_some() => pats.extend((0..8).map(|i| written_param(lowerer, &format!("$a{i}"), class.span))),
    None => {}
  }
  let depth = lowerer.scope.len();
  let names = crate::component::bind_params(lowerer, &pats)?;
  lowerer.scope.push((THIS.to_owned(), Expr::Var(THIS.to_owned())));
  let mut stmts: Vec<js::Statement> = declared.and_then(|c| c.body.as_ref()).map(|body| body.statements.iter().map(|s| b.copy(s)).collect()).unwrap_or_default();
  for stmt in &mut stmts {
    stmt.walk_mut(&mut ThisAs { with: THIS, scope: &lowerer.parsed.scope, b });
  }
  let stmts: &[js::Statement] = b.alloc(b.vec(stmts));
  let super_args = match stmts.first() {
    Some(js::Statement::ExpressionStatement(stmt)) => match &stmt.expression {
      js::Expression::CallExpression(call) if matches!(call.callee, js::Expression::Super(_)) => Some(&call.arguments),
      _ => None,
    },
    _ => None,
  };
  let stmts = if super_args.is_some() { &stmts[1..] } else { stmts };
  let result = (|| -> Lowered<Expr> {
    let base = match (parent, super_args) {
      (Some(parent), Some(args)) => {
        let mut lowered = Vec::with_capacity(args.len());
        for arg in args {
          if crate::syntax::is_spread(arg) {
            return Err(lowerer.residue(crate::syntax::arg_expr(arg).span(), "a spread argument to `super`"));
          }
          lowered.push(lowerer.expr(crate::syntax::arg_expr(arg))?);
        }
        Expr::Apply { f: Box::new(Expr::Const(constructor_key(parent))), args: lowered }
      }
      (Some(parent), None) if declared.is_none() => Expr::Apply { f: Box::new(Expr::Const(constructor_key(parent))), args: names.iter().map(|n| Expr::Var(n.clone())).collect() },
      (Some(_), None) => return Err(lowerer.residue(class.span, "a constructor that does not begin by calling `super`")),
      (None, Some(args)) if extends_error(class) => {
        let message = match args.first() {
          Some(arg) if !crate::syntax::is_spread(arg) => lowerer.expr(crate::syntax::arg_expr(arg))?,
          _ => Expr::lit_str(""),
        };
        let name = id.rsplit_once('#').map_or(id, |(_, name)| name);
        crate::error_object(Expr::Object(vec![Entry::Field("name".to_owned(), Expr::lit_str(name)), Entry::Field("message".to_owned(), message)]))
      }
      (None, _) => Expr::Object(Vec::new()),
    };
    let mut entries = vec![Entry::Spread(base), Entry::Field(CLASS_FIELD.to_owned(), Expr::lit_str(id))];
    entries.extend(properties.iter().map(|p| Entry::Field(p.clone(), Expr::Var(p.clone()))));
    for member in &class.body.body {
      let js::ClassElement::PropertyDefinition(prop) = member else { continue };
      if prop.r#static {
        continue;
      }
      let Some(key) = member_name(&prop.key, prop.computed) else { return Err(lowerer.residue(prop.span, "a computed class field")) };
      let value = match &prop.value {
        Some(value) => {
          let mut value = b.copy(value);
          value.walk_mut(&mut ThisAs { with: THIS, scope: &lowerer.parsed.scope, b });
          let value = b.alloc(value);
          lowerer.expr(value)?
        }
        None => Expr::Lit(Lit::Undefined),
      };
      entries.push(Entry::Field(key, value));
    }
    let mut body = vec![Stmt::Let { name: THIS.to_owned(), expr: Expr::Object(entries) }];
    body.extend(lowerer.block(stmts)?);
    body.push(Stmt::Return(Expr::Var(THIS.to_owned())));
    Ok(Expr::Lambda { params: names.clone(), body: Box::new(Expr::Block(body)) })
  })();
  lowerer.scope.truncate(depth);
  result
}

/// Whether `class` extends the global `Error`, whose instance holds the class's name and the message `super` was given.
fn extends_error(class: &js::Class) -> bool {
  matches!(class.heritage.as_ref().map(|h| crate::meaning::bare(&h.expression)), Some(js::Expression::Identifier(id)) if id.name.as_str() == "Error")
}

impl Lowerer<'_> {
  /// The class `e` names when it is one the set read: a binding already resolved to one, or a module binding lowered to find out, which resolves it.
  pub(crate) fn named_class(&mut self, e: &js::Expression) -> Lowered<Option<String>> {
    let js::Expression::Identifier(id) = crate::meaning::bare(e) else { return Ok(None) };
    if self.classes.is_none() || self.parsed.ctxt(id) == crate::Ctxt::Unresolved {
      return Ok(None);
    }
    let name = id.name.as_str();
    if let Some((_, bound)) = self.scope.iter().rev().find(|(n, _)| n == name) {
      return Ok(class_of_value(bound).map(str::to_owned));
    }
    if let Some((_, bound)) = self.globals.iter().rev().find(|(n, _)| n == name) {
      return Ok(class_of_value(bound).map(str::to_owned));
    }
    let declared_here = declares_class(self.parsed.module(), name);
    let imported = matches!(self.meanings.as_ref().and_then(|m| m.of_ident(id)), Some(crate::meaning::Meaning::Own { .. }));
    if !declared_here && !imported {
      return Ok(None);
    }
    self.expr(e).map(|value| class_of_value(&value).map(str::to_owned))
  }

  fn table(&self) -> Option<std::cell::Ref<'_, ClassTable>> {
    self.classes.as_ref().map(|c| c.borrow())
  }

  /// `super.m(args)`, `C.m(args)` for a static method of a class the set read: the member's constant called. `None` for any other call.
  pub(crate) fn class_call(&mut self, call: &js::CallExpression, callee: &js::Expression) -> Lowered<Option<Expr>> {
    let args = |this: &mut Self| -> Lowered<Vec<Expr>> {
      let mut out = Vec::with_capacity(call.arguments.len());
      for a in &call.arguments {
        if crate::syntax::is_spread(a) {
          return Err(this.residue(crate::syntax::arg_expr(a).span(), "a spread argument"));
        }
        out.push(this.expr(crate::syntax::arg_expr(a))?);
      }
      Ok(out)
    };
    if let Some(sp) = callee.as_member_expression().filter(|m| matches!(m.object(), js::Expression::Super(_))) {
      let crate::syntax::MemberProp::Ident(name) = crate::syntax::prop(sp) else { return Err(self.residue(sp.span(), "a computed `super` member")) };
      let Some(parent) = self.class_parent.clone() else { return Err(self.residue(sp.span(), "`super` outside a class the build reads")) };
      let Some(key) = self.table().and_then(|t| t.instance_member(&parent, name.name.as_str(), false)) else { return Err(self.residue(sp.span(), format!("`super.{}`, which no class it extends declares", name.name))) };
      let mut called = vec![Expr::Var(THIS.to_owned())];
      called.extend(args(self)?);
      return Ok(Some(Expr::Apply { f: Box::new(Expr::Const(key)), args: called }));
    }
    let Some(m) = (crate::meaning::bare(callee)).as_member_expression() else { return Ok(None) };
    let crate::syntax::MemberProp::Ident(name) = crate::syntax::prop(m) else { return Ok(None) };
    let name = name.name.to_string();
    if !self.table().is_some_and(|t| t.declares_static(&name)) {
      return Ok(None);
    }
    let Some(id) = self.named_class(m.object())? else { return Ok(None) };
    let found = self.table().and_then(|t| t.static_member(&id, &name));
    let Some((key, kind)) = found else { return Err(self.residue(m.span(), format!("`{name}`, which the class does not declare as static"))) };
    let f = match kind {
      Static::Getter => Expr::Apply { f: Box::new(Expr::Const(key)), args: Vec::new() },
      Static::Method | Static::Field => Expr::Const(key),
    };
    Ok(Some(Expr::Apply { f: Box::new(f), args: args(self)? }))
  }

  /// `new C(args)` with `C` a class the set read: its constructor called. `None` for any other `new`.
  pub(crate) fn class_new(&mut self, made: &js::NewExpression) -> Lowered<Option<Expr>> {
    let Some(id) = self.named_class(&made.callee)? else { return Ok(None) };
    let mut args = Vec::new();
    for a in &made.arguments {
      if crate::syntax::is_spread(a) {
        return Err(self.residue(crate::syntax::arg_expr(a).span(), "a spread argument"));
      }
      args.push(self.expr(crate::syntax::arg_expr(a))?);
    }
    Ok(Some(Expr::Apply { f: Box::new(Expr::Const(constructor_key(&id))), args }))
  }

  /// `C.name` with `C` a class the set read: a static field's value, a static getter called or a static method as a value. `None` for any other read.
  pub(crate) fn class_static_read(&mut self, member: &js::MemberExpression) -> Lowered<Option<Expr>> {
    let crate::syntax::MemberProp::Ident(name) = crate::syntax::prop(member) else { return Ok(None) };
    let name = name.name.to_string();
    if !self.table().is_some_and(|t| t.declares_static(&name)) {
      return Ok(None);
    }
    let Some(id) = self.named_class(member.object())? else { return Ok(None) };
    let found = self.table().and_then(|t| t.static_member(&id, &name));
    Ok(found.map(|(key, kind)| match kind {
      Static::Getter => Expr::Apply { f: Box::new(Expr::Const(key)), args: Vec::new() },
      Static::Method | Static::Field => Expr::Const(key),
    }))
  }

  /// Whether a read of `name` off a value may be an instance's getter: some class the set read declares one.
  pub(crate) fn may_be_getter(&self, name: &str) -> bool {
    self.table().is_some_and(|t| t.declares(name, true))
  }

  /// Whether a call of `name` on a value may be an instance's method: some class the set read declares one.
  pub(crate) fn may_be_method(&self, name: &str) -> bool {
    self.table().is_some_and(|t| t.declares(name, false))
  }

  /// `x.name(args)` with `name` a method some class the set read declares and nothing known of what `x` holds: the method of the class `x` was made by when it is an instance, else the call as it lowers without classes.
  pub(crate) fn instance_call(&mut self, call: &js::CallExpression, callee: &js::Expression) -> Lowered<Option<Expr>> {
    let Some(m) = (crate::meaning::bare(callee)).as_member_expression() else { return Ok(None) };
    let crate::syntax::MemberProp::Ident(name) = crate::syntax::prop(m) else { return Ok(None) };
    let name = name.name.to_string();
    if !self.may_be_method(&name) || self.kind_of(m.object()).is_some() || !self.may_hold_instance(m.object()) || call.arguments.iter().any(|a| crate::syntax::is_spread(a)) {
      return Ok(None);
    }
    let target = self.expr(m.object())?;
    if plain_root(&target) || matches!(target, Expr::Lit(_) | Expr::Array(_)) {
      return Ok(None);
    }
    let mut args = Vec::with_capacity(call.arguments.len());
    for a in &call.arguments {
      args.push(self.expr(crate::syntax::arg_expr(a))?);
    }
    let t = self.temp();
    let b = self.build();
    let receiver = b.ident(&t, m.object().span());
    if let js::Expression::Identifier(id) = &receiver {
      self.parsed.scope.mark_top(id);
    }
    let ast = b.ast();
    let callee = match m {
      js::MemberExpression::StaticMemberExpression(s) => js::Expression::new_static_member_expression(s.span, receiver, b.copy(&s.property), false, &ast),
      js::MemberExpression::ComputedMemberExpression(c) => js::Expression::new_computed_member_expression(c.span, receiver, b.copy(&c.expression), false, &ast),
      js::MemberExpression::PrivateFieldExpression(p) => js::Expression::new_private_field_expression(p.span, receiver, b.copy(&p.field), false, &ast),
    };
    let rewritten = b.alloc(js::CallExpression::new(call.span, callee, None, b.copy(&call.arguments), false, &ast));
    self.scope.push((t.clone(), Expr::Var(t.clone())));
    self.dispatching.push(t.clone());
    let plain = self.call(rewritten);
    self.dispatching.pop();
    self.scope.pop();
    let read = Expr::Var(t.clone());
    let otherwise = match plain {
      Ok(plain) => plain,
      Err(residue) if self.unbound.is_some() => return Err(residue),
      Err(_) => Expr::Apply { f: Box::new(Expr::Field(Box::new(read.clone()), name.clone())), args: args.clone() },
    };
    let mut called = vec![read.clone(), Expr::lit_str(&name)];
    called.extend(args);
    let instance = Expr::Builtin { name: snapfire_fsr_ir::ast::Builtin::CallMethod, args: called };
    Ok(Some(dispatched(t, target, read, instance, otherwise)))
  }

  /// Whether `e` may be an instance: not a receiver already dispatched on, nor rooted in a global or an import from a package.
  fn may_hold_instance(&self, e: &js::Expression) -> bool {
    let mut root = crate::meaning::bare(e);
    while let Some(m) = (root).as_member_expression() {
      root = crate::meaning::bare(m.object());
    }
    match root {
      js::Expression::Identifier(id) => {
        self.parsed.ctxt(id) != crate::Ctxt::Unresolved && !self.dispatching.iter().any(|d| d == id.name.as_str()) && matches!(self.meanings.as_ref().and_then(|m| m.of_ident(id)), None | Some(crate::meaning::Meaning::Own { .. }))
      }
      _ => true,
    }
  }

  /// `x instanceof C` with `C` a class the set read: whether `x` was made by `C` or a class extending it.
  pub(crate) fn class_instance_of(&mut self, value: &js::Expression, class: &js::Expression) -> Lowered<Option<Expr>> {
    let Some(id) = self.named_class(class)? else { return Ok(None) };
    let made = Expr::Builtin { name: snapfire_fsr_ir::ast::Builtin::ClassOf, args: vec![self.expr(value)?] };
    let ids = self.table().map(|t| t.descendants(&id)).unwrap_or_default();
    let test = ids.into_iter().map(|id| Expr::Compare(snapfire_fsr_ir::ast::CompareOp::Eq, Box::new(made.clone()), Box::new(Expr::lit_str(id)))).reduce(|a, b| Expr::Logic(snapfire_fsr_ir::ast::LogicOp::Or, Box::new(a), Box::new(b)));
    Ok(Some(test.unwrap_or(Expr::Lit(Lit::Bool(false)))))
  }
}

/// `target.name` read through an instance's getter when it is one, else as its field.
pub(crate) fn getter_dispatch(t: String, target: Expr, name: &str) -> Expr {
  let read = Expr::Var(t.clone());
  let instance = Expr::Builtin { name: snapfire_fsr_ir::ast::Builtin::GetMember, args: vec![read.clone(), Expr::lit_str(name)] };
  let field = Expr::Field(Box::new(read.clone()), name.to_owned());
  dispatched(t, target, read, instance, field)
}

fn dispatched(t: String, target: Expr, read: Expr, instance: Expr, otherwise: Expr) -> Expr {
  let class = Expr::Builtin { name: snapfire_fsr_ir::ast::Builtin::ClassOf, args: vec![read] };
  Expr::Block(vec![Stmt::Let { name: t, expr: target }, Stmt::Return(Expr::Ternary(Box::new(class), Box::new(instance), Box::new(otherwise)))])
}

/// Whether `e` reads only what a request or a component's props hold, which is never an instance.
pub(crate) fn plain_root(e: &Expr) -> bool {
  match e {
    Expr::Field(inner, _) | Expr::Index(inner, _) => plain_root(inner),
    Expr::Var(name) => name == "$props",
    Expr::Param(_) | Expr::Query(_) | Expr::ParamsAll | Expr::QueryAll | Expr::Session(_) | Expr::Store(_) | Expr::Identity(_) | Expr::Config(_) | Expr::Locale | Expr::Locales | Expr::Input | Expr::Lit(_) => true,
    _ => false,
  }
}
