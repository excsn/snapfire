use crate::*;

impl<'a> Lowerer<'a> {
  /// `x = e`, `x += e` and their kin on a local; `x.field = e` rebinds `x` to a copy with that field.
  /// The handle of the cell an assignment writes, when it writes a name held in one or a path into one.
  pub(crate) fn cell_of_target(&self, target: &js::AssignmentTarget) -> Option<Expr> {
    let id = match target {
      js::AssignmentTarget::AssignmentTargetIdentifier(id) => &**id,
      other => write_root(other.as_member_expression()?)?.0,
    };
    self.cells.iter().rev().find(|(name, _)| *name == id.name.as_str()).map(|(_, handle)| handle.clone())
  }

  pub(crate) fn local_assign(&mut self, assign: &js::AssignmentExpression) -> Lowered<Stmt> {
    let name = self.local_target(&assign.left).expect("checked");
    let b = syntax::Build::new(self.parsed.arena());
    let read = syntax::target_expr(b, &assign.left).expect("local_target answers for a name or a path");
    let value = match syntax::compound(b, assign.operator, b.copy(&read), b.copy(&assign.right), assign.span) {
      None => self.expr(&assign.right)?,
      Some(computed) => self.expr(&computed)?,
    };
    let Some(member) = (&read).as_member_expression() else {
      return Ok(Stmt::Set { name, expr: value });
    };
    if let (Some(Kind::Url), js::Expression::Identifier(_), crate::syntax::MemberProp::Ident(part)) = (self.kind_of(member.object()), member.object(), crate::syntax::prop(member)) {
      let part = part.name.to_string();
      if part == "origin" || !URL_PARTS.contains(&part.as_str()) && part != "href" {
        return Err(self.residue(member.span(), format!("a write to `.{part}` of a `URL`")));
      }
      return Ok(Stmt::Set { expr: Expr::Builtin { name: Builtin::UrlSet, args: vec![Expr::Var(name.clone()), Expr::lit_str(&part), value] }, name });
    }
    let (_, path) = write_root(member).expect("local_target found the root");
    let expr = self.written_at(Expr::Var(name.clone()), &path, value)?;
    Ok(Stmt::Set { name, expr })
  }

  /// `m.set(k, v)`, `m.delete(k)` or `s.add(x)` with `m` or `s` a local collection: the local, which kind it is and the method.
  /// `held` after `call` changes it, a collection of `kind`.
  pub(crate) fn collection_written(&mut self, call: &js::CallExpression, kind: Kind, method: &str, held: Expr) -> Lowered<Expr> {
    let arg = |this: &mut Self, i: usize| -> Lowered<Expr> {
      let a = call.arguments.get(i).ok_or_else(|| this.residue(call.span, format!("`{method}` takes {} argument{}", i + 1, if i == 0 { "" } else { "s" })))?;
      this.expr(crate::syntax::arg_expr(a))
    };
    let weak_key = |this: &mut Self| -> Lowered<Expr> {
      let key = arg(this, 0)?;
      Ok(if kind.weak() { this.weak_checked(kind, key) } else { key })
    };
    let written = match (kind.collection(), method) {
      (Kind::Map, "set") => Expr::Object(vec![Entry::Spread(held), Entry::Computed(Expr::Builtin { name: Builtin::MapKey, args: vec![weak_key(self)?] }, arg(self, 1)?)]),
      (Kind::Map, "delete") => Expr::Builtin { name: Builtin::Omit, args: vec![held, Expr::Builtin { name: Builtin::MapKey, args: vec![arg(self, 0)?] }] },
      (Kind::Set, "add") => Expr::Builtin { name: Builtin::Unique, args: vec![Expr::Array(vec![Entry::Spread(held), Entry::Item(weak_key(self)?)])] },
      (Kind::Set, "delete") => {
        let item = self.temp();
        let kept = Expr::Compare(CompareOp::Ne, Box::new(Expr::Var(item.clone())), Box::new(arg(self, 0)?));
        Expr::Filter(Box::new(held), Box::new(Expr::Lambda { params: vec![item], body: Box::new(kept) }))
      }
      (Kind::Map, "clear") => Expr::Object(Vec::new()),
      (Kind::Set, "clear") => Expr::Array(Vec::new()),
      (Kind::Query, "set" | "append") => Expr::Builtin { name: if method == "set" { Builtin::FormSet } else { Builtin::FormAppend }, args: vec![held, Expr::Str(Box::new(arg(self, 0)?)), arg(self, 1)?] },
      (Kind::Query, "delete") => Expr::Builtin { name: Builtin::FormDelete, args: vec![held, Expr::Str(Box::new(arg(self, 0)?))] },
      (_, other) => return Err(self.residue(call.span, format!("`.{other}()` of a local `{}` as a statement", kind.name()))),
    };
    Ok(kind.made(written))
  }

  /// `key` as a key of the weak collection `kind`, a `TypeError` thrown when it is not an object, as a `WeakMap` and a `WeakSet` throw.
  pub(crate) fn weak_checked(&mut self, kind: Kind, key: Expr) -> Expr {
    let held = self.temp();
    let kind_of = Expr::Builtin { name: Builtin::TypeOf, args: vec![Expr::Var(held.clone())] };
    let object = Expr::Logic(LogicOp::And, Box::new(Expr::Compare(CompareOp::Eq, Box::new(kind_of.clone()), Box::new(Expr::lit_str("object")))), Box::new(Expr::Not(Box::new(Expr::Builtin { name: Builtin::IsNullish, args: vec![Expr::Var(held.clone())] }))));
    let object = Expr::Logic(LogicOp::Or, Box::new(object), Box::new(Expr::Compare(CompareOp::Eq, Box::new(kind_of), Box::new(Expr::lit_str("function")))));
    let message = format!("Invalid value used {}", if kind == Kind::WeakMap { "as weak map key" } else { "in weak set" });
    let error = Expr::Object(vec![Entry::Field("name".to_owned(), Expr::lit_str("TypeError")), Entry::Field("message".to_owned(), Expr::lit_str(&message))]);
    let thrown = Expr::Builtin { name: Builtin::Throw, args: vec![Expr::lit_str(&message), error] };
    Expr::Block(vec![Stmt::Let { name: held.clone(), expr: key }, Stmt::If { cond: Expr::Not(Box::new(object)), then: vec![Stmt::Expr(thrown)], r#else: Vec::new() }, Stmt::Return(Expr::Var(held))])
  }

  /// `m.set(k, v)`, `m.delete(k)` or `s.add(x)` with `m` or `s` held in a cell: the cell's handle, the collection's kind and the method.
  pub(crate) fn cell_collection_write(&self, call: &js::CallExpression) -> Option<(Expr, Kind, String)> {
    let callee = &call.callee;
    let Some(member) = (callee).as_member_expression() else { return None };
    let js::Expression::Identifier(obj) = member.object() else { return None };
    let handle = self.cells.iter().rev().find(|(name, _)| *name == obj.name.as_str()).map(|(_, handle)| handle.clone())?;
    let kind = self.kind_of(member.object())?;
    let method = self.member_name(member)?;
    matches!(method.as_str(), "set" | "delete" | "add" | "append" | "clear").then_some((handle, kind, method))
  }

  /// `url.searchParams.set(k, v)`, `.append(k, v)` or `.delete(k)` with `url` a local `URL`: the local and the method.
  pub(crate) fn local_url_search_write(&self, call: &js::CallExpression) -> Option<(String, String)> {
    let callee = &call.callee;
    let Some(member) = (callee).as_member_expression() else { return None };
    let Some(params) = (member.object()).as_member_expression() else { return None };
    let (js::Expression::Identifier(url), crate::syntax::MemberProp::Ident(prop)) = (params.object(), crate::syntax::prop(params)) else { return None };
    let method = self.member_name(member)?;
    (prop.name.as_str() == "searchParams" && self.kind_of(params.object()) == Some(Kind::Url) && self.is_local(url.name.as_str()) && matches!(method.as_str(), "set" | "append" | "delete")).then(|| (url.name.to_string(), method))
  }

  /// `m.set(k, v)` and its kin on a name bound nowhere here, which may be a module-level collection the set binds and retries with: the name and its span.
  pub(crate) fn unbound_collection_write(&self, call: &js::CallExpression) -> Option<(String, Span)> {
    let callee = &call.callee;
    let Some(member) = (callee).as_member_expression() else { return None };
    let js::Expression::Identifier(obj) = member.object() else { return None };
    let method = self.member_name(member)?;
    if !matches!(method.as_str(), "set" | "delete" | "add" | "append" | "clear") || self.parsed.ctxt(obj) == crate::Ctxt::Unresolved {
      return None;
    }
    let name = obj.name.as_str();
    let bound = self.scope.iter().any(|(n, _)| n == name) || self.globals.iter().any(|(n, _)| n == name) || self.cells.iter().any(|(n, _)| n == name);
    (!bound).then(|| (name.to_owned(), obj.span))
  }

  pub(crate) fn local_collection_write(&self, call: &js::CallExpression) -> Option<(String, Kind, String)> {
    let callee = &call.callee;
    let Some(member) = (callee).as_member_expression() else { return None };
    let js::Expression::Identifier(obj) = member.object() else { return None };
    let kind = self.kind_of(member.object())?;
    let method = self.member_name(member)?;
    (matches!(method.as_str(), "set" | "delete" | "add" | "append" | "clear") && self.is_local(obj.name.as_str())).then(|| (obj.name.to_string(), kind, method))
  }

  /// `x.push(...)` or `x.unshift(...)` with `x` a local: the local and the method.
  pub(crate) fn local_push(&self, call: &js::CallExpression) -> Option<(String, String)> {
    let callee = &call.callee;
    let Some(member) = (callee).as_member_expression() else { return None };
    let js::Expression::Identifier(obj) = member.object() else { return None };
    let method = self.member_name(member)?;
    (matches!(method.as_str(), "push" | "unshift") && self.is_local(obj.name.as_str())).then(|| (obj.name.to_string(), method))
  }

  /// `session.extend(...)` or `ctx.session.extend(...)`.
  pub(crate) fn is_session_extend(&self, call: &js::CallExpression) -> bool {
    let callee = &call.callee;
    let Some(member) = (callee).as_member_expression() else { return false };
    if self.member_name(member).as_deref() != Some("extend") {
      return false;
    }
    match member.object() {
      js::Expression::Identifier(id) => self.root_of(id) == Some(Root::Session),
      via_expr if via_expr.is_member_expression() => {
let via = via_expr.as_member_expression().expect("matched as a member");
        let js::Expression::Identifier(id) = via.object() else { return false };
        self.root_of(id) == Some(Root::Ctx) && self.member_name(via).as_deref() == Some("session")
      }
      _ => false,
    }
  }

  pub(crate) fn is_ident_call(&self, call: &js::CallExpression, name: &str) -> bool {
    matches!(&call.callee, js::Expression::Identifier(id) if id.name.as_str() == name)
  }

  /// `session.key`, `session.key.sub`, `session.key[expr]` or the same
  /// through `ctx.session`. Returns the key and the path beneath it.
  pub(crate) fn session_target(&mut self, member: &js::MemberExpression) -> Lowered<(String, Vec<Expr>)> {
    let mut chain = vec![member];
    let mut current = member.object();
    let root_ident = loop {
      match current {
        js::Expression::ChainExpression(o) => match o.expression.as_member_expression() {
          Some(m) => {
            chain.push(m);
            current = m.object();
          }
          None => return Err(self.residue(o.expression.span(), "a call inside a session path")),
        },
        js::Expression::Identifier(id) => break id,
        m_expr if m_expr.is_member_expression() => {
          let m = m_expr.as_member_expression().expect("matched as a member");
          chain.push(m);
          current = m.object();
        }
        other => return Err(self.residue(other.span(), "a session write must start at `session`")),
      }
    };
    chain.reverse();
    let mut steps = chain.into_iter();
    let root = self.root_of(root_ident);
    let first = match root {
      Some(Root::Session) => steps.next(),
      Some(Root::Ctx) => {
        let via = steps.next().ok_or_else(|| self.residue(member.span(), "a write to `ctx` itself"))?;
        if self.member_name(via).as_deref() != Some("session") {
          return Err(self.residue(via.span(), "a write to something other than the session"));
        }
        steps.next()
      }
      _ => return Err(self.residue(root_ident.span, "a session write must start at `session`")),
    };
    let Some(first) = first else {
      return Err(self.residue(member.span(), "a write to the whole session; write one key"));
    };
    let key = self.member_name(first).ok_or_else(|| self.residue(first.span(), "the session key must be a name"))?;
    let mut path = Vec::new();
    for step in steps {
      path.push(match crate::syntax::prop(step) {
        crate::syntax::MemberProp::Ident(id) => Expr::Lit(Lit::Str(id.name.to_string())),
        crate::syntax::MemberProp::Computed(c) => self.expr(c)?,
        crate::syntax::MemberProp::Private(p) => return Err(self.residue(p.span, "a private name")),
      });
    }
    Ok((key, path))
  }

  pub(crate) fn member_name(&self, member: &js::MemberExpression) -> Option<String> {
    match crate::syntax::prop(member) {
      crate::syntax::MemberProp::Ident(id) => Some(id.name.to_string()),
      _ => None,
    }
  }

  /// Whether `call` is middleware's `next(...)` or `ctx.next(...)`.
  pub(crate) fn calls_next(&self, call: &js::CallExpression) -> bool {
    let callee = &call.callee;
    match crate::meaning::bare(callee) {
      js::Expression::Identifier(id) => self.root_of(id) == Some(Root::Next),
      m_expr if m_expr.is_member_expression() => { let m = m_expr.as_member_expression().expect("matched as a member"); self.middleware && matches!((m.object(), crate::syntax::prop(m)), (js::Expression::Identifier(obj), crate::syntax::MemberProp::Ident(prop)) if self.root_of(obj) == Some(Root::Ctx) && prop.name.as_str() == "next") },
      _ => false,
    }
  }

  /// `response.headers.get(name)` or `.has(name)` on a `Response`; `None` for any other call.
  pub(crate) fn response_header_read(&mut self, call: &js::CallExpression) -> Lowered<Option<Expr>> {
    let Some((response, op, args)) = self.response_headers_call(call) else { return Ok(None) };
    if !matches!(op.as_str(), "get" | "has") {
      return Err(self.residue(call.span, format!("`headers.{op}` inside an expression; it is a statement")));
    }
    let [name] = args else { return Err(self.residue(call.span, format!("`headers.{op}` takes a header name"))) };
    let mut lowered = vec![self.expr(response)?, self.expr(crate::syntax::arg_expr(name))?];
    if op == "has" {
      lowered.push(Expr::lit_str("has"));
    }
    Ok(Some(Expr::Builtin { name: Builtin::ResponseHeaderGet, args: lowered }))
  }

  /// A call `response.headers.<op>(...)` with `op` one of a `Headers`' `get`, `has`, `set`, `append` and `delete`, on anything not known to be another kind: the response, the operation and the arguments. A value that is not a `Response` fails where it runs.
  pub(crate) fn response_headers_call<'c, 'e>(&self, call: &'c js::CallExpression<'e>) -> Option<(&'c js::Expression<'e>, String, &'c [js::Argument<'e>])> {
    let callee = &call.callee;
    let Some(method) = (crate::meaning::bare(callee)).as_member_expression() else { return None };
    let crate::syntax::MemberProp::Ident(op) = crate::syntax::prop(method) else { return None };
    let Some(headers) = (crate::meaning::bare(method.object())).as_member_expression() else { return None };
    let crate::syntax::MemberProp::Ident(field) = crate::syntax::prop(headers) else { return None };
    let known = matches!(op.name.as_str(), "get" | "has" | "set" | "append" | "delete");
    (field.name.as_str() == "headers" && known && !matches!(self.kind_of(headers.object()), Some(kind) if kind != Kind::Response)).then(|| (headers.object(), op.name.to_string(), call.arguments.as_slice()))
  }

  pub(crate) fn root_of(&self, id: &js::IdentifierReference) -> Option<Root> {
    let name = id.name.as_str();
    self.roots.iter().rev().find(|(n, _)| n == name).map(|(_, r)| r.clone())
  }

  /// `expr` as a value JavaScript's conversion to a primitive reads the same. A `Map` or a `Set` holds an object or an array at run time, which would convert as one, so its text is decided here where its kind is known.
  /// `expr` as JavaScript converts it with the string hint, in `String(x)`, a template or a computed key, and with the default hint `+` and `==` use. A `Date` is its `toString()`: this machine's local time in a body, the viewer's in a component, which only the browser knows.
  pub(crate) fn as_text(&mut self, expr: &js::Expression) -> Lowered<Expr> {
    if self.kind_of(expr) != Some(Kind::Date) {
      return self.as_primitive(expr);
    }
    if self.on_render_path() && !self.reaches(Reach::Terminal) {
      return Err(self.browser_only(expr.span(), "a `Date` written as text, which reads the viewer's time zone"));
    }
    let when = self.expr(expr)?;
    Ok(Expr::Ext { module: "intl".to_owned(), name: "dateTimeString".to_owned(), args: vec![when] })
  }

  /// `expr`, or the primitive it wraps when it is a `new Number`, `new Boolean` or `new String`, which every read but `typeof` and truthiness converts it to.
  pub(crate) fn unwrapped(&mut self, expr: &js::Expression) -> Lowered<Expr> {
    let value = self.expr(expr)?;
    Ok(if self.kind_of(expr) == Some(Kind::Wrapper) { Expr::Builtin { name: Builtin::TaggedPayload, args: vec![value, Expr::lit_str(snapfire_fsr_ir::render::typed::WRAPPER)] } } else { value })
  }

  pub(crate) fn as_primitive(&mut self, expr: &js::Expression) -> Lowered<Expr> {
    if self.kind_of(expr) == Some(Kind::Wrapper) {
      return self.unwrapped(expr);
    }
    let text = match self.kind_of(expr) {
      Some(Kind::Map) => "[object Map]",
      Some(Kind::Set) => "[object Set]",
      Some(Kind::WeakMap) => "[object WeakMap]",
      Some(Kind::WeakSet) => "[object WeakSet]",
      Some(Kind::WeakRef) => "[object WeakRef]",
      Some(Kind::Registry) => "[object FinalizationRegistry]",
      _ => return self.expr(expr),
    };
    if matches!(crate::meaning::bare(expr), js::Expression::Identifier(_)) {
      return Ok(Expr::lit_str(text));
    }
    let held = self.expr(expr)?;
    let name = self.temp();
    Ok(Expr::Block(vec![Stmt::Let { name, expr: held }, Stmt::Return(Expr::lit_str(text))]))
  }

  /// Whether `expr` changes an object or an array a name reaches: a field written, a mutating method or `Object.assign`.
  pub(crate) fn object_write(&self, expr: &js::Expression) -> bool {
    let reached = |target: &js::Expression| -> bool {
      let root = match crate::meaning::bare(target) {
        js::Expression::Identifier(id) => id,
        member_expr if member_expr.is_member_expression() => { let member = member_expr.as_member_expression().expect("matched as a member"); match write_root(member) {
          Some((id, _)) => id,
          None => return false,
        } },
        _ => return false,
      };
      // A module constant, not module state: that one lowers to a cell or a store key.
      let global = || self.globals.iter().any(|(name, value)| *name == root.name.as_str() && matches!(value, Expr::Const(_) | Expr::Object(_) | Expr::Array(_)));
      let bound = self.scope.iter().any(|(name, _)| *name == root.name.as_str()) || global();
      bound && self.root_of(root).is_none() && !matches!(self.kind_of(target), Some(Kind::Url | Kind::Query | Kind::Response | Kind::Date | Kind::Scope))
    };
    // A write into what an expression answers, `f()[0].n = 2`, has no name to check.
    let written = |target: &js::Expression| -> bool {
      let mut at = crate::meaning::bare(target);
      while let Some(member) = (at).as_member_expression() {
        at = crate::meaning::bare(member.object());
      }
      match at {
        js::Expression::ThisExpression(_) => false,
        js::Expression::Identifier(id) if id.name.as_str() == crate::classes::THIS => false,
        js::Expression::Identifier(_) => reached(target),
        _ => true,
      }
    };
    match crate::meaning::bare(expr) {
      js::Expression::AssignmentExpression(assign) => assign.left.as_member_expression().is_some_and(|m| written(m.object())),
      js::Expression::UpdateExpression(update) => update.argument.as_member_expression().is_some_and(|m| written(m.object())),
      js::Expression::UnaryExpression(unary) if unary.operator == js::UnaryOperator::Delete => (crate::meaning::bare(&unary.argument)).as_member_expression().is_some_and(|m| written(m.object())),
      js::Expression::CallExpression(call) => {
        let callee = &call.callee;
        let Some(m) = (crate::meaning::bare(callee)).as_member_expression() else { return false };
        let held = |target: &js::Expression| reached(target);
        let reached = |target: &js::Expression| reached(target) || made_here(target);
        let reflect = matches!(crate::meaning::bare(m.object()), js::Expression::Identifier(id) if id.name.as_str() == "Reflect" && !self.scope.iter().any(|(n, _)| n == "Reflect"));
        if reflect {
          return matches!(self.member_name(m).as_deref(), Some("set" | "deleteProperty")) && call.arguments.first().is_some_and(|first| !crate::syntax::is_spread(first) && reached(crate::syntax::arg_expr(first)));
        }
        match self.member_name(m).as_deref() {
          Some("push" | "unshift" | "pop" | "shift" | "splice") => reached(m.object()) && !matches!(self.kind_of(m.object()), Some(Kind::Map | Kind::Set)),
          Some("reverse" | "sort" | "copyWithin" | "fill") => held(m.object()) && !matches!(self.kind_of(m.object()), Some(Kind::Map | Kind::Set)),
          Some("set" | "add" | "delete" | "clear") => reached(m.object()) && matches!(self.kind_of(m.object()).map(Kind::collection), Some(Kind::Map | Kind::Set)),
          Some("getOrInsert" | "getOrInsertComputed") => reached(m.object()) && self.kind_of(m.object()) == Some(Kind::Map),
          Some("compile") => (matches!(crate::meaning::bare(m.object()), js::Expression::RegExpLiteral(_)) || matches!(crate::meaning::bare(m.object()), js::Expression::Identifier(id) if self.is_local(id.name.as_str()))),
          Some("exec" | "test") => self.kind_of(m.object()).is_none() && matches!(crate::meaning::bare(m.object()), js::Expression::Identifier(id) if self.is_local(id.name.as_str())),
          Some(setter) if DATE_SETTERS.contains(&setter) || (LOCAL_DATE_SETTERS.contains(&setter) && (self.reaches(Reach::Terminal) || !self.on_render_path())) => self.kind_of(m.object()) == Some(Kind::Date) && (made_here(m.object()) || matches!(crate::meaning::bare(m.object()), js::Expression::Identifier(id) if self.is_local(id.name.as_str()))),
          Some("assign") => matches!(crate::meaning::bare(m.object()), js::Expression::Identifier(id) if id.name.as_str() == "Object") && call.arguments.first().is_some_and(|first| !crate::syntax::is_spread(first) && reached(crate::syntax::arg_expr(first))),
          _ => false,
        }
      }
      _ => false,
    }
  }

  /// [`Lowerer::object_write`]'s `expr` as what it answers, its object held once and overwritten through the handle.
  pub(crate) fn heap_write(&mut self, expr: &js::Expression) -> Lowered<Expr> {
    let overwrite = |target: &str, next: Expr| Stmt::Expr(Expr::Builtin { name: Builtin::Overwrite, args: vec![Expr::Var(target.to_owned()), next] });
    let held = |this: &mut Self, target: &js::Expression, lets: &mut Body| -> Lowered<String> {
      let value = this.expr(target)?;
      let name = this.temp();
      lets.push(Stmt::Let { name: name.clone(), expr: value });
      Ok(name)
    };
    let mut lets = Vec::new();
    let answer = match crate::meaning::bare(expr) {
      js::Expression::AssignmentExpression(assign) => {
        let Some(m) = assign.left.as_member_expression() else { unreachable!() };
        let target = held(self, m.object(), &mut lets)?;
        let b = syntax::Build::new(self.parsed.arena());
        let value = match syntax::compound(b, assign.operator, js::Expression::from(b.copy(m)), b.copy(&assign.right), assign.span) {
          None => self.expr(&assign.right)?,
          Some(computed) => self.expr(&computed)?,
        };
        let name = self.temp();
        lets.push(Stmt::Let { name: name.clone(), expr: value });
        let next = self.heap_set(&target, m, Expr::Var(name.clone()), &mut lets)?;
        lets.push(overwrite(&target, next));
        Expr::Var(name)
      }
      js::Expression::UpdateExpression(update) => {
        let Some(m) = update.argument.as_member_expression() else { unreachable!() };
        let target = held(self, m.object(), &mut lets)?;
        let b = self.build();
        let before = self.expr(&js::Expression::new_unary_expression(update.span, js::UnaryOperator::UnaryPlus, b.updated(&update.argument), &b.ast()))?;
        let old = self.temp();
        lets.push(Stmt::Let { name: old.clone(), expr: before });
        let op = if update.operator == js::UpdateOperator::Increment { ArithOp::Add } else { ArithOp::Sub };
        let after = Expr::Arith(op, Box::new(Expr::Var(old.clone())), Box::new(Expr::Lit(Lit::Float(1.0))));
        let name = self.temp();
        lets.push(Stmt::Let { name: name.clone(), expr: after });
        let next = self.heap_set(&target, m, Expr::Var(name.clone()), &mut lets)?;
        lets.push(overwrite(&target, next));
        Expr::Var(if update.prefix { name } else { old })
      }
      js::Expression::UnaryExpression(unary) => {
        let Some(m) = (crate::meaning::bare(&unary.argument)).as_member_expression() else { unreachable!() };
        let target = held(self, m.object(), &mut lets)?;
        let key = match self.member_name(m) {
          Some(field) => Expr::lit_str(&field),
          None => {
            let crate::syntax::MemberProp::Computed(key) = crate::syntax::prop(m) else { return Err(self.residue(m.span(), "`delete` of a private field")) };
            self.expr(key)?
          }
        };
        lets.push(overwrite(&target, Expr::Builtin { name: Builtin::Omit, args: vec![Expr::Var(target.clone()), key] }));
        Expr::Lit(Lit::Bool(true))
      }
      js::Expression::CallExpression(call) => {
        let callee = &call.callee;
        let Some(m) = (crate::meaning::bare(callee)).as_member_expression() else { unreachable!() };
        let method = self.member_name(m).expect("object_write read the name");
        if matches!(crate::meaning::bare(m.object()), js::Expression::Identifier(id) if id.name.as_str() == "Reflect") {
          let target = held(self, &crate::syntax::arg_expr(&call.arguments[0]), &mut lets)?;
          let key = match call.arguments.get(1) {
            Some(a) => self.expr(crate::syntax::arg_expr(a))?,
            None => Expr::Lit(Lit::Undefined),
          };
          let key = Expr::Str(Box::new(key));
          let next = if method == "set" {
            let value = match call.arguments.get(2) {
              Some(a) => self.expr(crate::syntax::arg_expr(a))?,
              None => Expr::Lit(Lit::Undefined),
            };
            Expr::Builtin { name: Builtin::SetAt, args: vec![Expr::Var(target.clone()), key, value] }
          } else {
            Expr::Builtin { name: Builtin::Omit, args: vec![Expr::Var(target.clone()), key] }
          };
          lets.push(overwrite(&target, next));
          Expr::Lit(Lit::Bool(true))
        } else if method == "assign" {
          let target = held(self, &crate::syntax::arg_expr(&call.arguments[0]), &mut lets)?;
          let mut entries = vec![Entry::Spread(Expr::Var(target.clone()))];
          for source in &call.arguments[1..] {
            entries.push(Entry::Spread(self.expr(crate::syntax::arg_expr(source))?));
          }
          lets.push(overwrite(&target, Expr::Object(entries)));
          Expr::Var(target)
        } else if self.kind_of(m.object()) == Some(Kind::Date) && made_here(m.object()) {
          let held = self.expr(m.object())?;
          self.date_set(&method, held, call)?
        } else if self.kind_of(m.object()) == Some(Kind::Date) {
          let js::Expression::Identifier(id) = crate::meaning::bare(m.object()) else { unreachable!("object_write took a local Date") };
          let name = id.name.to_string();
          let next = self.date_set(&method, Expr::Var(name.clone()), call)?;
          lets.push(Stmt::Set { name: name.clone(), expr: next });
          Expr::Var(name)
        } else {
          let kind = self.kind_of(m.object());
          let target = held(self, m.object(), &mut lets)?;
          let read = Expr::Var(target.clone());
          match (kind, method.as_str()) {
            (Some(Kind::Map), "getOrInsert" | "getOrInsertComputed") => {
              let (given, key) = (self.temp(), self.temp());
              let first = call.arguments.first().ok_or_else(|| self.residue(call.span, format!("`{method}` without a key")))?;
              let first = self.expr(crate::syntax::arg_expr(first))?;
              lets.push(Stmt::Let { name: given.clone(), expr: first });
              lets.push(Stmt::Let { name: key.clone(), expr: Expr::Builtin { name: Builtin::MapKey, args: vec![Expr::Var(given.clone())] } });
              let second = call.arguments.get(1).ok_or_else(|| self.residue(call.span, format!("`{method}` without a value")))?;
              let second = self.expr(crate::syntax::arg_expr(second))?;
              let value = if method == "getOrInsert" { second } else { Expr::Apply { f: Box::new(second), args: vec![Expr::Var(given)] } };
              let added = Kind::Map.made(Expr::Object(vec![Entry::Spread(read.clone()), Entry::Computed(Expr::Var(key.clone()), value)]));
              let missing = Expr::Not(Box::new(Expr::Builtin { name: Builtin::HasKey, args: vec![read.clone(), Expr::Var(key.clone())] }));
              lets.push(Stmt::If { cond: missing, then: vec![overwrite(&target, added)], r#else: Vec::new() });
              Expr::Index(Box::new(read), Box::new(Expr::Var(key)))
            }
            (_, "compile") => {
              let mut args = self.plain_args(call)?.into_iter();
              let pattern = args.next().map_or(Expr::lit_str("(?:)"), |p| Expr::Str(Box::new(p)));
              let flags = args.next().map_or(Expr::lit_str(""), |f| Expr::Str(Box::new(f)));
              let next = Expr::Object(vec![
                Entry::Field(snapfire_fsr_ir::jsregex::PATTERN.to_owned(), pattern),
                Entry::Field(snapfire_fsr_ir::jsregex::FLAGS.to_owned(), flags),
                Entry::Field(snapfire_fsr_ir::jsregex::LAST_INDEX.to_owned(), Expr::Lit(Lit::Float(0.0))),
              ]);
              lets.push(overwrite(&target, next));
              read
            }
            (_, "exec" | "test") => {
              let [subject] = call.arguments.as_slice() else { return Err(self.residue(call.span, format!("`{method}` takes the string to search"))) };
              let subject = self.expr(crate::syntax::arg_expr(subject))?;
              let ran = self.temp();
              let name = if method == "exec" { Builtin::ExecFrom } else { Builtin::TestFrom };
              lets.push(Stmt::Let { name: ran.clone(), expr: Expr::Builtin { name, args: vec![read.clone(), subject] } });
              lets.push(overwrite(&target, Expr::Index(Box::new(Expr::Var(ran.clone())), Box::new(Expr::Lit(Lit::Float(1.0))))));
              Expr::Index(Box::new(Expr::Var(ran)), Box::new(Expr::Lit(Lit::Float(0.0))))
            }
            (_, "splice") => {
              let after = self.temp();
              let spliced = Expr::Builtin { name: Builtin::Splice, args: [vec![read.clone()], self.spliced_args(call)?].concat() };
              lets.push(Stmt::Let { name: after.clone(), expr: spliced });
              lets.push(overwrite(&target, Expr::Index(Box::new(Expr::Var(after.clone())), Box::new(Expr::Lit(Lit::Float(0.0))))));
              Expr::Index(Box::new(Expr::Var(after)), Box::new(Expr::Lit(Lit::Float(1.0))))
            }
            (_, "copyWithin" | "fill") => {
              let name = if method == "fill" { Builtin::Fill } else { Builtin::CopyWithin };
              let changed = Expr::Builtin { name, args: [vec![read.clone()], self.plain_args(call)?].concat() };
              lets.push(overwrite(&target, changed));
              read
            }
            (Some(kind @ (Kind::Map | Kind::Set | Kind::WeakMap | Kind::WeakSet)), _) => {
              let had = self.temp();
              if method == "delete" {
                let given = call.arguments.first().ok_or_else(|| self.residue(call.span, "`delete` takes 1 argument"))?;
                let given = self.expr(crate::syntax::arg_expr(given))?;
                let held = if kind.collection() == Kind::Map { Expr::Builtin { name: Builtin::HasKey, args: vec![read.clone(), Expr::Builtin { name: Builtin::MapKey, args: vec![given] }] } } else { Expr::Builtin { name: Builtin::Includes, args: vec![read.clone(), given] } };
                lets.push(Stmt::Let { name: had.clone(), expr: held });
              }
              let next = self.collection_written(call, kind, &method, read.clone())?;
              lets.push(overwrite(&target, next));
              match method.as_str() {
                "delete" => Expr::Var(had),
                "clear" => Expr::Lit(Lit::Undefined),
                _ => read,
              }
            }
            (_, "push" | "unshift") => {
              let mut items = Vec::with_capacity(call.arguments.len() + 1);
              for arg in &call.arguments {
                let value = self.expr(crate::syntax::arg_expr(arg))?;
                items.push(if crate::syntax::is_spread(arg) { Entry::Spread(value) } else { Entry::Item(value) });
              }
              if method == "push" { items.insert(0, Entry::Spread(read.clone())) } else { items.push(Entry::Spread(read.clone())) }
              lets.push(overwrite(&target, Expr::Array(items)));
              Expr::Length(Box::new(read))
            }
            (_, "pop" | "shift") => {
              let (at, kept) = if method == "pop" { (-1, vec![read.clone(), Expr::Lit(Lit::Int(0)), Expr::Lit(Lit::Int(-1))]) } else { (0, vec![read.clone(), Expr::Lit(Lit::Int(1))]) };
              let name = self.temp();
              lets.push(Stmt::Let { name: name.clone(), expr: Expr::Builtin { name: Builtin::At, args: vec![read.clone(), Expr::Lit(Lit::Int(at))] } });
              lets.push(overwrite(&target, Expr::Builtin { name: Builtin::Slice, args: kept }));
              Expr::Var(name)
            }
            (_, "reverse") => {
              lets.push(overwrite(&target, Expr::Builtin { name: Builtin::Reverse, args: vec![read.clone()] }));
              read
            }
            _ => {
              let by = match call.arguments.first().map(|a| crate::syntax::arg_expr(a)) {
                None => Expr::Lit(Lit::Null),
                Some(js::Expression::ArrowFunctionExpression(arrow)) => self.lambda(arrow)?,
                Some(other) => self.expr(other)?,
              };
              lets.push(overwrite(&target, Expr::Sort(Box::new(read.clone()), Box::new(by))));
              read
            }
          }
        }
      }
      _ => unreachable!("object_write answers for these"),
    };
    lets.push(Stmt::Return(answer));
    Ok(Expr::Block(lets))
  }

  pub(crate) fn heap_set(&mut self, target: &str, member: &js::MemberExpression, value: Expr, lets: &mut Body) -> Lowered<Expr> {
    if let Some(field) = self.member_name(member) {
      return Ok(Expr::Object(vec![Entry::Spread(Expr::Var(target.to_owned())), Entry::Field(field, value)]));
    }
    let crate::syntax::MemberProp::Computed(key) = crate::syntax::prop(member) else { return Err(self.residue(member.span(), "a write through a private field")) };
    let key = self.expr(key)?;
    let name = self.temp();
    lets.push(Stmt::Let { name: name.clone(), expr: key });
    Ok(Expr::Builtin { name: Builtin::SetAt, args: vec![Expr::Var(target.to_owned()), Expr::Var(name), value] })
  }
}

/// Whether `target` is a value the expression makes in place, `[1, 2]`, `{ a: 1 }`, `/a/g`, `new Map()`, `new Set()`, `new Date()` or `new RegExp()`, which a write may change since nothing else holds it.
fn made_here(target: &js::Expression) -> bool {
  match crate::meaning::bare(target) {
    js::Expression::ArrayExpression(_) | js::Expression::ObjectExpression(_) | js::Expression::RegExpLiteral(_) => true,
    js::Expression::NewExpression(n) => matches!(&n.callee, js::Expression::Identifier(id) if matches!(id.name.as_str(), "Map" | "Set" | "Date" | "RegExp")),
    _ => false,
  }
}


impl Lowerer<'_> {
  /// The instant `held.<method>(...)` sets, one of [`DATE_SETTERS`] or [`LOCAL_DATE_SETTERS`]: `Date.UTC` of its parts with the ones the call names replaced, so a part past its range carries into the next as JavaScript's does. A local setter reads and writes the parts in this machine's time zone. `setTime` takes the instant whole.
  pub(crate) fn date_set(&mut self, method: &str, held: Expr, call: &js::CallExpression) -> Lowered<Expr> {
    let mut args = self.plain_args(call)?;
    if method == "setTime" {
      let given = args.into_iter().next().unwrap_or(Expr::Lit(Lit::Undefined));
      return Ok(Expr::Builtin { name: Builtin::DateMs, args: vec![Expr::Num(Box::new(given))] });
    }
    let local = LOCAL_DATE_SETTERS.contains(&method);
    if method == "setYear" {
      let year = self.temp();
      let given = args.drain(..).next().unwrap_or(Expr::Lit(Lit::Undefined));
      let short = Expr::Logic(LogicOp::And, Box::new(Expr::Compare(CompareOp::Ge, Box::new(Expr::Var(year.clone())), Box::new(Expr::Lit(Lit::Float(0.0))))), Box::new(Expr::Compare(CompareOp::Le, Box::new(Expr::Var(year.clone())), Box::new(Expr::Lit(Lit::Float(99.0))))));
      let full = Expr::Ternary(Box::new(short), Box::new(Expr::Arith(ArithOp::Add, Box::new(Expr::Var(year.clone())), Box::new(Expr::Lit(Lit::Float(1900.0))))), Box::new(Expr::Var(year.clone())));
      args = vec![Expr::Block(vec![Stmt::Let { name: year, expr: Expr::Num(Box::new(given)) }, Stmt::Return(full)])];
    }
    let (from, most) = match method.trim_start_matches("setUTC").trim_start_matches("set") {
      "FullYear" | "Year" => (0, 3),
      "Month" => (1, 2),
      "Date" => (2, 1),
      "Hours" => (3, 4),
      "Minutes" => (4, 3),
      "Seconds" => (5, 2),
      _ => (6, 1),
    };
    let offset = |at: &String| Expr::Arith(ArithOp::Mul, Box::new(Expr::Ext { module: "time".to_owned(), name: "localOffset".to_owned(), args: vec![Expr::Var(at.clone())] }), Box::new(Expr::Lit(Lit::Float(60_000.0))));
    let at = self.temp();
    let held = if local {
      let instant = self.temp();
      Expr::Block(vec![Stmt::Let { name: instant.clone(), expr: held }, Stmt::Return(Expr::Arith(ArithOp::Sub, Box::new(Expr::Var(instant.clone())), Box::new(offset(&instant))))])
    } else {
      held
    };
    let base = if matches!(method, "setUTCFullYear" | "setFullYear" | "setYear") {
      Expr::Ternary(Box::new(Expr::Builtin { name: Builtin::IsNaN, args: vec![held.clone()] }), Box::new(Expr::Lit(Lit::Float(0.0))), Box::new(held))
    } else {
      held
    };
    let mut parts: Vec<Expr> = ["year", "month", "date", "hours", "minutes", "seconds", "milliseconds"]
      .iter()
      .map(|part| Expr::Builtin { name: Builtin::DatePart, args: vec![Expr::Var(at.clone()), Expr::lit_str(*part)] })
      .collect();
    let given = args.len().clamp(1, most);
    let mut args = args.into_iter();
    for slot in parts.iter_mut().skip(from).take(given) {
      *slot = Expr::Num(Box::new(args.next().unwrap_or(Expr::Lit(Lit::Undefined))));
    }
    let made = Expr::Builtin { name: Builtin::DateUtc, args: parts };
    if !local {
      return Ok(Expr::Block(vec![Stmt::Let { name: at, expr: base }, Stmt::Return(made)]));
    }
    let instant = self.local_to_utc(made);
    Ok(Expr::Block(vec![Stmt::Let { name: at, expr: base }, Stmt::Return(instant)]))
  }
}
