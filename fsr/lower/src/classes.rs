//! Classes an application declares, lowered into the value model: an instance is an object of its fields carrying its class in `$c`, each member a constant holding a function whose first parameter is the instance, a call site of an instance member dispatching on the class the value was made by.

use snapfire_fsr_ir::ast::{Entry, Lit, Stmt};
use snapfire_fsr_ir::Expr;
use swc_core::common::Spanned;
use swc_core::ecma::ast as js;
use swc_core::ecma::visit::{VisitMut, VisitMutWith};

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
pub(crate) fn member_name(key: &js::PropName) -> Option<String> {
  crate::prop_name(key)
}

/// What `class` declares, its parent left for the caller to resolve.
pub(crate) fn describe(id: String, class: &js::Class) -> ClassInfo {
  let mut info = ClassInfo { id, ..ClassInfo::default() };
  for member in &class.body {
    match member {
      js::ClassMember::Method(m) => {
        let Some(name) = member_name(&m.key) else { continue };
        let list = match (m.is_static, m.kind) {
          (false, js::MethodKind::Method) => &mut info.methods,
          (false, js::MethodKind::Getter) => &mut info.getters,
          (true, js::MethodKind::Method) => &mut info.statics,
          (true, js::MethodKind::Getter) => &mut info.static_getters,
          (_, js::MethodKind::Setter) => continue,
        };
        list.push(name);
      }
      js::ClassMember::ClassProp(p) if p.is_static => {
        if let Some(name) = member_name(&p.key) {
          info.static_fields.push(name);
        }
      }
      _ => {}
    }
  }
  info
}

/// Every module-level class `parsed` declares, by its local name, a default export's under `default` as well.
pub(crate) fn declared(module: &js::Module) -> Vec<(String, &js::Class)> {
  let mut out = Vec::new();
  for item in &module.body {
    match item {
      js::ModuleItem::Stmt(js::Stmt::Decl(js::Decl::Class(c))) => out.push((c.ident.sym.to_string(), &*c.class)),
      js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDecl(js::ExportDecl { decl: js::Decl::Class(c), .. })) => out.push((c.ident.sym.to_string(), &*c.class)),
      js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDefaultDecl(d)) => {
        if let js::DefaultDecl::Class(c) = &d.decl {
          if let Some(id) = &c.ident {
            out.push((id.sym.to_string(), &*c.class));
          }
          out.push(("default".to_owned(), &*c.class));
        }
      }
      _ => {}
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

/// The identifier a module-level class `name` is declared under.
pub(crate) fn declared_ident(module: &js::Module, name: &str) -> Option<js::Ident> {
  module.body.iter().find_map(|item| match item {
    js::ModuleItem::Stmt(js::Stmt::Decl(js::Decl::Class(c))) | js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDecl(js::ExportDecl { decl: js::Decl::Class(c), .. })) if c.ident.sym.as_ref() == name => Some(c.ident.clone()),
    js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDefaultDecl(d)) => match &d.decl {
      js::DefaultDecl::Class(c) => c.ident.clone().filter(|id| id.sym.as_ref() == name || name == "default"),
      _ => None,
    },
    _ => None,
  })
}

/// `this` as `with` throughout a member's body, arrows included, a nested `function` or class left with its own.
struct ThisAs {
  with: js::Ident,
}

impl VisitMut for ThisAs {
  fn visit_mut_expr(&mut self, e: &mut js::Expr) {
    if let js::Expr::This(t) = e {
      *e = js::Expr::Ident(js::Ident { span: t.span, ..self.with.clone() });
      return;
    }
    e.visit_mut_children_with(self);
  }

  fn visit_mut_function(&mut self, _: &mut js::Function) {}

  fn visit_mut_class(&mut self, _: &mut js::Class) {}
}

fn this_param(lowerer: &Lowerer<'_>, span: swc_core::common::Span) -> js::Ident {
  js::Ident::new(THIS.into(), span, lowerer.parsed.top)
}

/// The kinds `class` declares its fields with, from a parameter property's or a field's type.
fn field_kinds(class: &js::Class, aliases: &[(String, crate::AliasKind)]) -> Vec<(String, crate::Kind)> {
  let mut out = Vec::new();
  for member in &class.body {
    match member {
      js::ClassMember::Constructor(c) => {
        for param in &c.params {
          let js::ParamOrTsParamProp::TsParamProp(p) = param else { continue };
          let binding = match &p.param {
            js::TsParamPropParam::Ident(b) => b,
            js::TsParamPropParam::Assign(a) => match &*a.left {
              js::Pat::Ident(b) => b,
              _ => continue,
            },
          };
          if let Some(kind) = binding.type_ann.as_deref().and_then(|t| crate::Kind::of_type_in(&t.type_ann, aliases)) {
            out.push((binding.id.sym.to_string(), kind));
          }
        }
      }
      js::ClassMember::ClassProp(p) if !p.is_static => {
        let (Some(name), Some(kind)) = (member_name(&p.key), p.type_ann.as_deref().and_then(|t| crate::Kind::of_type_in(&t.type_ann, aliases)).or_else(|| p.value.as_deref().and_then(crate::made_kind))) else { continue };
        out.push((name, kind));
      }
      _ => {}
    }
  }
  out
}

/// One member of the class `id` as the constant it is lowered into: an instance member a function of the instance and its arguments, a static one a function of its arguments or a value, the constructor a function of its arguments answering the instance.
pub(crate) fn lower_member(lowerer: &mut Lowerer<'_>, id: &str, parent: Option<&str>, class: &js::Class, name: &js::Ident, member: &Member<'_>) -> Lowered<Expr> {
  lowerer.this_kinds = field_kinds(class, &lowerer.type_kinds);
  match member {
    Member::Instance(f) | Member::Static(f) => {
      let mut body = f.body.clone().ok_or_else(|| lowerer.residue(f.span, "a method without a body"))?;
      let instance = matches!(member, Member::Instance(_));
      let with = if instance { this_param(lowerer, f.span) } else { name.clone() };
      body.visit_mut_with(&mut ThisAs { with: with.clone() });
      let mut params = Vec::new();
      if instance {
        params.push(js::Pat::Ident(js::BindingIdent { id: with, type_ann: None }));
      }
      params.extend(crate::component::patterns(f));
      crate::component::function_to_lambda(lowerer, &params, FunctionBody::Block(&body.stmts), f.is_async)
    }
    Member::StaticField(value) => {
      let mut value = (*value).clone();
      value.visit_mut_with(&mut ThisAs { with: name.clone() });
      lowerer.expr(&value)
    }
    Member::Constructor => constructor(lowerer, id, parent, class),
  }
}

/// The constructor of `id`: the parent's instance when it extends one, else a fresh object, then its class, its parameter properties and its fields, then the constructor's own statements writing `this`, answering the instance.
fn constructor(lowerer: &mut Lowerer<'_>, id: &str, parent: Option<&str>, class: &js::Class) -> Lowered<Expr> {
  let declared = class.body.iter().find_map(|m| match m {
    js::ClassMember::Constructor(c) => Some(c),
    _ => None,
  });
  let mut pats = Vec::new();
  let mut properties = Vec::new();
  match declared {
    Some(c) => {
      for param in &c.params {
        match param {
          js::ParamOrTsParamProp::Param(p) => pats.push(p.pat.clone()),
          js::ParamOrTsParamProp::TsParamProp(p) => match &p.param {
            js::TsParamPropParam::Ident(b) => {
              properties.push(b.id.sym.to_string());
              pats.push(js::Pat::Ident(b.clone()));
            }
            js::TsParamPropParam::Assign(a) => {
              if let js::Pat::Ident(b) = &*a.left {
                properties.push(b.id.sym.to_string());
              }
              pats.push(js::Pat::Assign(a.clone()));
            }
          },
        }
      }
    }
    None if parent.is_some() => pats.extend((0..8).map(|i| js::Pat::Ident(js::BindingIdent { id: js::Ident::new(format!("$a{i}").into(), class.span, lowerer.parsed.top), type_ann: None }))),
    None => {}
  }
  let depth = lowerer.scope.len();
  let names = crate::component::bind_params(lowerer, &pats)?;
  let this = this_param(lowerer, class.span);
  lowerer.scope.push((THIS.to_owned(), Expr::Var(THIS.to_owned())));
  let mut stmts: Vec<js::Stmt> = declared.and_then(|c| c.body.clone()).map(|b| b.stmts).unwrap_or_default();
  for stmt in &mut stmts {
    stmt.visit_mut_with(&mut ThisAs { with: this.clone() });
  }
  let super_args = match stmts.first() {
    Some(js::Stmt::Expr(js::ExprStmt { expr, .. })) => match &**expr {
      js::Expr::Call(js::CallExpr { callee: js::Callee::Super(_), args, .. }) => Some(args.clone()),
      _ => None,
    },
    _ => None,
  };
  if super_args.is_some() {
    stmts.remove(0);
  }
  let result = (|| -> Lowered<Expr> {
    let base = match (parent, super_args) {
      (Some(parent), Some(args)) => {
        let mut lowered = Vec::with_capacity(args.len());
        for arg in &args {
          if arg.spread.is_some() {
            return Err(lowerer.residue(arg.expr.span(), "a spread argument to `super`"));
          }
          lowered.push(lowerer.expr(&arg.expr)?);
        }
        Expr::Apply { f: Box::new(Expr::Const(constructor_key(parent))), args: lowered }
      }
      (Some(parent), None) if declared.is_none() => Expr::Apply { f: Box::new(Expr::Const(constructor_key(parent))), args: names.iter().map(|n| Expr::Var(n.clone())).collect() },
      (Some(_), None) => return Err(lowerer.residue(class.span, "a constructor that does not begin by calling `super`")),
      (None, Some(args)) if extends_error(class) => {
        let message = match args.first() {
          Some(arg) if arg.spread.is_none() => lowerer.expr(&arg.expr)?,
          _ => Expr::lit_str(""),
        };
        let name = id.rsplit_once('#').map_or(id, |(_, name)| name);
        Expr::Object(vec![Entry::Field("name".to_owned(), Expr::lit_str(name)), Entry::Field("message".to_owned(), message)])
      }
      (None, _) => Expr::Object(Vec::new()),
    };
    let mut entries = vec![Entry::Spread(base), Entry::Field(CLASS_FIELD.to_owned(), Expr::lit_str(id))];
    entries.extend(properties.iter().map(|p| Entry::Field(p.clone(), Expr::Var(p.clone()))));
    for member in &class.body {
      let js::ClassMember::ClassProp(prop) = member else { continue };
      if prop.is_static {
        continue;
      }
      let Some(key) = member_name(&prop.key) else { return Err(lowerer.residue(prop.span, "a computed class field")) };
      let value = match prop.value.as_deref() {
        Some(value) => {
          let mut value = value.clone();
          value.visit_mut_with(&mut ThisAs { with: this.clone() });
          lowerer.expr(&value)?
        }
        None => Expr::Lit(Lit::Undefined),
      };
      entries.push(Entry::Field(key, value));
    }
    let mut body = vec![Stmt::Let { name: THIS.to_owned(), expr: Expr::Object(entries) }];
    body.extend(lowerer.block(&stmts)?);
    body.push(Stmt::Return(Expr::Var(THIS.to_owned())));
    Ok(Expr::Lambda { params: names.clone(), body: Box::new(Expr::Block(body)) })
  })();
  lowerer.scope.truncate(depth);
  result
}

/// Whether `class` extends the global `Error`, whose instance holds the class's name and the message `super` was given.
fn extends_error(class: &js::Class) -> bool {
  matches!(class.super_class.as_deref().map(crate::meaning::bare), Some(js::Expr::Ident(id)) if id.sym == "Error")
}

impl Lowerer<'_> {
  /// The class `e` names when it is one the set read: a binding already resolved to one, or a module binding lowered to find out, which resolves it.
  pub(crate) fn named_class(&mut self, e: &js::Expr) -> Lowered<Option<String>> {
    let js::Expr::Ident(id) = crate::meaning::bare(e) else { return Ok(None) };
    if self.classes.is_none() || id.ctxt == self.parsed.unresolved {
      return Ok(None);
    }
    let name = id.sym.as_ref();
    if let Some((_, bound)) = self.scope.iter().rev().find(|(n, _)| n == name) {
      return Ok(class_of_value(bound).map(str::to_owned));
    }
    if let Some((_, bound)) = self.globals.iter().rev().find(|(n, _)| n == name) {
      return Ok(class_of_value(bound).map(str::to_owned));
    }
    let declared_here = declared(&self.parsed.module).iter().any(|(n, _)| n == name);
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
  pub(crate) fn class_call(&mut self, call: &js::CallExpr, callee: &js::Expr) -> Lowered<Option<Expr>> {
    let args = |this: &mut Self| -> Lowered<Vec<Expr>> {
      let mut out = Vec::with_capacity(call.args.len());
      for a in &call.args {
        if a.spread.is_some() {
          return Err(this.residue(a.expr.span(), "a spread argument"));
        }
        out.push(this.expr(&a.expr)?);
      }
      Ok(out)
    };
    if let js::Expr::SuperProp(sp) = callee {
      let js::SuperProp::Ident(name) = &sp.prop else { return Err(self.residue(sp.span, "a computed `super` member")) };
      let Some(parent) = self.class_parent.clone() else { return Err(self.residue(sp.span, "`super` outside a class the build reads")) };
      let Some(key) = self.table().and_then(|t| t.instance_member(&parent, name.sym.as_ref(), false)) else { return Err(self.residue(sp.span, format!("`super.{}`, which no class it extends declares", name.sym))) };
      let mut called = vec![Expr::Var(THIS.to_owned())];
      called.extend(args(self)?);
      return Ok(Some(Expr::Apply { f: Box::new(Expr::Const(key)), args: called }));
    }
    let js::Expr::Member(m) = crate::meaning::bare(callee) else { return Ok(None) };
    let js::MemberProp::Ident(name) = &m.prop else { return Ok(None) };
    let name = name.sym.to_string();
    if !self.table().is_some_and(|t| t.declares_static(&name)) {
      return Ok(None);
    }
    let Some(id) = self.named_class(&m.obj)? else { return Ok(None) };
    let found = self.table().and_then(|t| t.static_member(&id, &name));
    let Some((key, kind)) = found else { return Err(self.residue(m.span, format!("`{name}`, which the class does not declare as static"))) };
    let f = match kind {
      Static::Getter => Expr::Apply { f: Box::new(Expr::Const(key)), args: Vec::new() },
      Static::Method | Static::Field => Expr::Const(key),
    };
    Ok(Some(Expr::Apply { f: Box::new(f), args: args(self)? }))
  }

  /// `new C(args)` with `C` a class the set read: its constructor called. `None` for any other `new`.
  pub(crate) fn class_new(&mut self, made: &js::NewExpr) -> Lowered<Option<Expr>> {
    let Some(id) = self.named_class(&made.callee)? else { return Ok(None) };
    let mut args = Vec::new();
    for a in made.args.as_deref().unwrap_or_default() {
      if a.spread.is_some() {
        return Err(self.residue(a.expr.span(), "a spread argument"));
      }
      args.push(self.expr(&a.expr)?);
    }
    Ok(Some(Expr::Apply { f: Box::new(Expr::Const(constructor_key(&id))), args }))
  }

  /// `C.name` with `C` a class the set read: a static field's value, a static getter called or a static method as a value. `None` for any other read.
  pub(crate) fn class_static_read(&mut self, member: &js::MemberExpr) -> Lowered<Option<Expr>> {
    let js::MemberProp::Ident(name) = &member.prop else { return Ok(None) };
    let name = name.sym.to_string();
    if !self.table().is_some_and(|t| t.declares_static(&name)) {
      return Ok(None);
    }
    let Some(id) = self.named_class(&member.obj)? else { return Ok(None) };
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
  pub(crate) fn instance_call(&mut self, call: &js::CallExpr, callee: &js::Expr) -> Lowered<Option<Expr>> {
    let js::Expr::Member(m) = crate::meaning::bare(callee) else { return Ok(None) };
    let js::MemberProp::Ident(name) = &m.prop else { return Ok(None) };
    let name = name.sym.to_string();
    if !self.may_be_method(&name) || self.kind_of(&m.obj).is_some() || !self.may_hold_instance(&m.obj) || call.args.iter().any(|a| a.spread.is_some()) {
      return Ok(None);
    }
    let target = self.expr(&m.obj)?;
    if plain_root(&target) || matches!(target, Expr::Lit(_) | Expr::Array(_)) {
      return Ok(None);
    }
    let mut args = Vec::with_capacity(call.args.len());
    for a in &call.args {
      args.push(self.expr(&a.expr)?);
    }
    let t = self.temp();
    let mut rewritten = call.clone();
    rewritten.callee = js::Callee::Expr(Box::new(js::Expr::Member(js::MemberExpr { span: m.span, obj: Box::new(js::Expr::Ident(js::Ident::new(t.clone().into(), m.obj.span(), self.parsed.top))), prop: m.prop.clone() })));
    self.scope.push((t.clone(), Expr::Var(t.clone())));
    self.dispatching.push(t.clone());
    let plain = self.call(&rewritten);
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
  fn may_hold_instance(&self, e: &js::Expr) -> bool {
    let mut root = crate::meaning::bare(e);
    while let js::Expr::Member(m) = root {
      root = crate::meaning::bare(&m.obj);
    }
    match root {
      js::Expr::Ident(id) => {
        id.ctxt != self.parsed.unresolved && !self.dispatching.iter().any(|d| d == id.sym.as_ref()) && matches!(self.meanings.as_ref().and_then(|m| m.of_ident(id)), None | Some(crate::meaning::Meaning::Own { .. }))
      }
      _ => true,
    }
  }

  /// `x instanceof C` with `C` a class the set read: whether `x` was made by `C` or a class extending it.
  pub(crate) fn class_instance_of(&mut self, value: &js::Expr, class: &js::Expr) -> Lowered<Option<Expr>> {
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
    Expr::Param(_) | Expr::Query(_) | Expr::ParamsAll | Expr::QueryAll | Expr::Session(_) | Expr::Store(_) | Expr::Identity(_) | Expr::Config(_) | Expr::Locale | Expr::Input | Expr::Lit(_) => true,
    _ => false,
  }
}
