use crate::*;

impl<'a> Lowerer<'a> {
  /// The one object literal a service or native method takes, as named
  /// arguments. `kind` names the caller in a diagnostic.
  pub(crate) fn object_args(&mut self, call: &js::CallExpression, kind: &str) -> Lowered<Vec<(String, Expr)>> {
    let mut args = Vec::new();
    if let Some(a) = call.arguments.first() {
      let js::Expression::ObjectExpression(obj) = crate::syntax::arg_expr(a) else {
        return Err(self.residue(crate::syntax::arg_expr(a).span(), format!("{kind} arguments must be an object literal")));
      };
      for prop in &obj.properties {
        match prop {
          js::ObjectPropertyKind::ObjectProperty(p) => match crate::syntax::prop_of(p) {
            crate::syntax::Prop::Shorthand(id) => args.push((id.name.to_string(), self.ident(id)?)),
            crate::syntax::Prop::KeyValue(kv) => {
              let key = prop_name(&kv.key, kv.computed).ok_or_else(|| self.residue(kv.key.span(), "a computed argument name"))?;
              args.push((key, self.expr(&kv.value)?));
            }
            other => return Err(self.residue(other.span(), "a method in the arguments")),
          },
          js::ObjectPropertyKind::SpreadProperty(s) => return Err(self.residue(s.argument.span(), format!("a spread into {kind} arguments"))),
        }
      }
    }
    if call.arguments.len() > 1 {
      return Err(self.residue(call.span, format!("a {kind} method takes one object")));
    }
    Ok(args)
  }

  /// `native.<name>` or `ctx.native.<name>`.
  pub(crate) fn native_of(&self, obj: &js::Expression) -> Option<(String, bool)> {
    self.rooted_at(obj, Root::Native, "native")
  }

  /// `services.<name>` or `ctx.services.<name>`.
  pub(crate) fn service_of(&self, obj: &js::Expression) -> Option<(String, bool)> {
    self.rooted_at(obj, Root::Services, "services")
  }

  /// `<root>.<name>` bare or `ctx.<field>.<name>`; the bool says which.
  pub(crate) fn rooted_at(&self, obj: &js::Expression, root: Root, field: &str) -> Option<(String, bool)> {
    let Some(m) = (obj).as_member_expression() else { return None };
    let name = self.member_name(m)?;
    match m.object() {
      js::Expression::Identifier(id) if !self.scope.iter().any(|(n, _)| n == id.name.as_str()) => match self.root_of(id) {
        Some(held) if held == root => Some((name, false)),
        _ => None,
      },
      inner_expr if inner_expr.is_member_expression() => {
let inner = inner_expr.as_member_expression().expect("matched as a member");
        let js::Expression::Identifier(id) = inner.object() else { return None };
        if self.scope.iter().any(|(n, _)| n == id.name.as_str()) {
          return None;
        }
        match (self.root_of(id), self.member_name(inner).as_deref()) {
          (Some(Root::Ctx), Some(via)) if via == field => Some((name, true)),
          _ => None,
        }
      }
      _ => None,
    }
  }

  /// An arrow function applied by a builtin. Its body is one expression or a
  /// block that only returns one. Destructured parameters read as fields and
  /// indexes of the positional parameter.
  /// `new Date(x)`, `new Map(pairs)`, `new Set(items)` and `new URLSearchParams(x)`; anything else made with `new` is residue.
  /// `Some(true)` for the global `Promise.resolve(…)` and `Some(false)` for `Promise.reject(…)` in a page whose host resumes a handler after its step.
  pub(crate) fn settled_promise(&self, call: &js::CallExpression) -> Option<bool> {
    let callee = &call.callee;
    let Some(m) = (crate::meaning::bare(callee)).as_member_expression() else { return None };
    let (js::Expression::Identifier(promise), crate::syntax::MemberProp::Ident(method)) = (m.object(), crate::syntax::prop(m)) else { return None };
    if !self.reaches(Reach::Later) || self.parsed.ctxt(promise) != crate::Ctxt::Unresolved || promise.name.as_str() != "Promise" {
      return None;
    }
    match method.name.as_str() {
      "resolve" => Some(true),
      "reject" => Some(false),
      _ => None,
    }
  }

  /// Whether `call` is the global `Promise.reject(…)` where no handler resumes later: a body holds no promise values, so a rejection is a throw where it is made, as a call that fails throws where it is made.
  pub(crate) fn rejects_here(&self, call: &js::CallExpression) -> bool {
    let Some(m) = (crate::meaning::bare(&call.callee)).as_member_expression() else { return false };
    let (js::Expression::Identifier(promise), crate::syntax::MemberProp::Ident(method)) = (m.object(), crate::syntax::prop(m)) else { return false };
    !self.reaches(Reach::Later) && self.parsed.ctxt(promise) == crate::Ctxt::Unresolved && promise.name.as_str() == "Promise" && method.name.as_str() == "reject"
  }

  /// A fresh deferred settled at once with `value`, fulfilled when `ok` and rejected otherwise, as `new Promise` settles one its executor settles.
  pub(crate) fn settled(&mut self, ok: bool, value: Expr) -> Expr {
    let deferred = self.temp();
    let id = Expr::Ext { module: "crypto".to_owned(), name: "random".to_owned(), args: vec![Expr::Lit(Lit::Int(8))] };
    let settle = Expr::Object(vec![Entry::Field(snapfire_fsr_ir::host::SETTLES.to_owned(), Expr::Var(deferred.clone())), Entry::Field("ok".to_owned(), Expr::Lit(Lit::Bool(ok)))]);
    Expr::Block(vec![
      Stmt::Let { name: deferred.clone(), expr: Expr::Object(vec![Entry::Field(snapfire_fsr_ir::render::DEFERRED.to_owned(), id)]) },
      Stmt::Act { action: snapfire_fsr_ir::host::Act::Settle.name().to_owned(), input: Expr::Object(vec![Entry::Field("settle".to_owned(), settle), Entry::Field("value".to_owned(), value)]) },
      Stmt::Return(Expr::Var(deferred)),
    ])
  }

  /// `new Promise((resolve, reject) => …)` in a terminal page: a fresh deferred, the executor run at once with `resolve` and `reject` held as [`snapfire_fsr_ir::host::SETTLES`] values on it, then the deferred.
  pub(crate) fn terminal_promise(&mut self, n: &js::NewExpression, args: &[js::Argument]) -> Lowered<Expr> {
    let Some(js::Expression::ArrowFunctionExpression(executor)) = args.first().map(|a| crate::meaning::bare(crate::syntax::arg_expr(a))) else { return Err(self.residue(n.span, "a `Promise` whose executor is not an arrow written in place")) };
    let deferred = self.temp();
    let id = Expr::Ext { module: "crypto".to_owned(), name: "random".to_owned(), args: vec![Expr::Lit(Lit::Int(8))] };
    let mut body = vec![Stmt::Let { name: deferred.clone(), expr: Expr::Object(vec![Entry::Field(snapfire_fsr_ir::render::DEFERRED.to_owned(), id)]) }];
    let depth = self.scope.len();
    let settlers = self.settlers.len();
    for (param, ok) in crate::params_of(&executor.params).into_iter().zip([true, false]) {
      let (js::BindingPattern::BindingIdentifier(name), None, false) = (param.pattern, param.default, param.rest) else { return Err(self.residue(param.span, "a `Promise` executor's parameter that is not a name")) };
      let settle = Expr::Object(vec![Entry::Field(snapfire_fsr_ir::host::SETTLES.to_owned(), Expr::Var(deferred.clone())), Entry::Field("ok".to_owned(), Expr::Lit(Lit::Bool(ok)))]);
      self.scope.push((name.name.to_string(), settle));
      self.settlers.push(name.name.to_string());
    }
    let ran = match crate::syntax::arrow_body_of(executor) {
      crate::syntax::ArrowBody::Block(b) => crate::component::block_to_expr(self, b.body),
      crate::syntax::ArrowBody::Expr(e) => self.effect(e).map(|stmt| Expr::Block(vec![stmt])),
    };
    self.scope.truncate(depth);
    self.settlers.truncate(settlers);
    body.push(Stmt::Expr(ran?));
    body.push(Stmt::Return(Expr::Var(deferred)));
    Ok(Expr::Block(body))
  }

  pub(crate) fn new_expr(&mut self, n: &js::NewExpression) -> Lowered<Expr> {
    // A `Scope` starts with no value; the cell made for it holds what `run` and `set` set.
    if matches!(&n.callee, js::Expression::Identifier(id) if id.name.as_str() == "Scope") {
      return Ok(Expr::Lit(Lit::Undefined));
    }
    let args = &n.arguments[..];
    if let Some(spread) = args.iter().find(|a| crate::syntax::is_spread(a)) {
      return Err(self.residue(crate::syntax::arg_expr(spread).span(), "a spread argument"));
    }
    let svelte = match self.meanings.as_ref().and_then(|meanings| meanings.of_expr(&n.callee)) {
      Some(meaning::Meaning::Library(meaning::Construct::SvelteClass(class))) => Some(class),
      _ => None,
    };
    if let Some("Spring" | "Tween") = svelte {
      let value = match args.first() {
        Some(a) => self.expr(crate::syntax::arg_expr(a))?,
        None => Expr::Lit(Lit::Null),
      };
      return Ok(Expr::Object(vec![Entry::Field("current".to_owned(), value.clone()), Entry::Field("target".to_owned(), value)]));
    }
    if self.reaches(Reach::Later) && matches!(&n.callee, js::Expression::Identifier(id) if self.parsed.ctxt(id) == crate::Ctxt::Unresolved && id.name.as_str() == "Promise") {
      return self.terminal_promise(n, &args);
    }
    if matches!(&n.callee, js::Expression::Identifier(id) if self.parsed.ctxt(id) == crate::Ctxt::Unresolved && id.name.as_str() == "Function") {
      return self.function_from_text(args, n.span);
    }
    if matches!(&n.callee, js::Expression::Identifier(id) if self.parsed.ctxt(id) == crate::Ctxt::Unresolved && id.name.as_str() == "Promise") {
      let Some(executor) = args.first() else { return Err(self.residue(n.span, "`new Promise` without an executor")) };
      let executor = self.expr(crate::syntax::arg_expr(executor))?;
      let (made, reason) = (self.temp(), self.temp());
      let field = |name: &str| Expr::Field(Box::new(Expr::Var(made.clone())), name.to_owned());
      let run = Expr::Apply { f: Box::new(executor), args: vec![field("resolve"), field("reject")] };
      let rejected = Expr::Apply { f: Box::new(field("reject")), args: vec![Expr::Var(reason.clone())] };
      return Ok(Expr::Block(vec![
        Stmt::Let { name: made.clone(), expr: self.with_resolvers() },
        Stmt::Try { body: vec![Stmt::Expr(run)], name: Some(reason), handler: Some(vec![Stmt::Expr(rejected)]), finally: Vec::new() },
        Stmt::Return(field("promise")),
      ]));
    }
    if let js::Expression::Identifier(id) = &n.callee {
      let class = id.name.as_str();
      if self.parsed.ctxt(id) == crate::Ctxt::Unresolved && ERROR_CONSTRUCTORS.contains(&class) {
        // `AggregateError` takes its errors first and `SuppressedError` the error and what it suppressed, both before the message.
        let leading = match class {
          "AggregateError" => 1,
          "SuppressedError" => 2,
          _ => 0,
        };
        let mut fields = vec![Entry::Field("name".to_owned(), Expr::lit_str(class))];
        match class {
          "AggregateError" => {
            let errors = match args.first() {
              Some(a) => self.expr(crate::syntax::arg_expr(a))?,
              None => Expr::Array(Vec::new()),
            };
            fields.push(Entry::Field("errors".to_owned(), Expr::Array(vec![Entry::Spread(errors)])));
          }
          "SuppressedError" => {
            for (at, key) in ["error", "suppressed"].into_iter().enumerate() {
              let value = match args.get(at) {
                Some(a) => self.expr(crate::syntax::arg_expr(a))?,
                None => Expr::Lit(Lit::Undefined),
              };
              fields.push(Entry::Field(key.to_owned(), value));
            }
          }
          _ => {}
        }
        let message = match args.get(leading) {
          Some(a) if !matches!(strip_parens(crate::syntax::arg_expr(a)), js::Expression::Identifier(u) if u.name.as_str() == "undefined") => Expr::Str(Box::new(self.expr(crate::syntax::arg_expr(a))?)),
          _ => Expr::lit_str(""),
        };
        fields.insert(1, Entry::Field("message".to_owned(), message));
        if class != "SuppressedError" {
          if let Some(options) = args.get(leading + 1) {
            let options = self.expr(crate::syntax::arg_expr(options))?;
            let held = self.temp();
            let has_cause = Expr::Builtin { name: Builtin::HasKey, args: vec![Expr::Var(held.clone()), Expr::lit_str("cause")] };
            let with_cause = Expr::Object(fields.iter().cloned().chain([Entry::Field("cause".to_owned(), Expr::Field(Box::new(Expr::Var(held.clone())), "cause".to_owned()))]).collect());
            let without = Expr::Object(fields.clone());
            let object = Expr::Logic(LogicOp::And, Box::new(Expr::Compare(CompareOp::Eq, Box::new(Expr::Builtin { name: Builtin::TypeOf, args: vec![Expr::Var(held.clone())] }), Box::new(Expr::lit_str("object")))), Box::new(Expr::Not(Box::new(Expr::Builtin { name: Builtin::IsNullish, args: vec![Expr::Var(held.clone())] }))));
            let pick = Expr::Ternary(Box::new(Expr::Logic(LogicOp::And, Box::new(object), Box::new(has_cause))), Box::new(with_cause), Box::new(without));
            return Ok(crate::error_object(Expr::Block(vec![Stmt::Let { name: held, expr: options }, Stmt::Return(pick)])));
          }
        }
        return Ok(crate::error_object(Expr::Object(fields)));
      }
    }
    if crate::intl_member(&n.callee) == Some("Locale") {
      let tag = match args.first() {
        Some(a) => Expr::Str(Box::new(self.expr(crate::syntax::arg_expr(a))?)),
        None => return Err(self.residue(n.span, "`new Intl.Locale` without a tag")),
      };
      return Ok(Expr::Ext { module: "intl".to_owned(), name: "locale".to_owned(), args: vec![tag, Expr::lit_str("parse")] });
    }
    if let Some(name) = crate::temporal_type(&n.callee) {
      let mut lowered = vec![Expr::lit_str(name)];
      for a in args {
        lowered.push(self.expr(crate::syntax::arg_expr(a))?);
      }
      return Ok(Expr::Ext { module: "temporal".to_owned(), name: "new".to_owned(), args: lowered });
    }
    let global = match &n.callee {
      _ if svelte.is_some() => svelte.expect("checked").to_owned(),
      _ if matches!(platform_object(n), Some("TextEncoder" | "TextDecoder")) => return Err(self.residue_with(n.span, "a `TextEncoder` or `TextDecoder` kept as a value", "call its method where it is made or on a name bound to it: `new TextEncoder().encode(s)`")),
      _ if platform_object(n).is_some() => return Err(self.residue_with(n.span, "an `Intl` formatter kept as a value", "call `.format(x)` on it where it is made, `new Intl.NumberFormat(undefined, options).format(n)`")),
      js::Expression::Identifier(id) if self.parsed.ctxt(id) == crate::Ctxt::Unresolved => id.name.to_string(),
      other => match self.class_new(n)? {
        Some(made) => return Ok(made),
        None => return Err(self.residue(other.span(), "`new` of something other than a platform global or a class the build reads")),
      },
    };
    let first = match args.first() {
      Some(a) => Some(self.expr(crate::syntax::arg_expr(a))?),
      None => None,
    };
    match (global.as_str(), first) {
      ("Date", None) => Ok(Kind::Date.made(self.date_now(n.span)?)),
      ("Date", Some(x)) if args.len() == 1 => Ok(Kind::Date.made(Expr::Builtin { name: Builtin::DateMs, args: vec![x] })),
      ("Date", Some(year)) if self.reaches(Reach::Terminal) || !self.on_render_path() => {
        let mut parts = vec![year];
        for a in args.iter().skip(1).take(6) {
          parts.push(self.expr(crate::syntax::arg_expr(a))?);
        }
        Ok(Kind::Date.made(self.local_to_utc(Expr::Builtin { name: Builtin::DateUtc, args: parts })))
      }
      ("Date", Some(_)) => Err(self.browser_only(n.span, "`new Date(year, month, ...)`, which reads the viewer's time zone")),
      ("Map", None) => Ok(Kind::Map.made(Expr::Object(Vec::new()))),
      ("Map", Some(pairs)) => Ok(Kind::Map.made(Expr::Builtin { name: Builtin::MapOf, args: vec![pairs] })),
      ("Set", None) => Ok(Kind::Set.made(Expr::Array(Vec::new()))),
      ("Set", Some(items)) => Ok(Kind::Set.made(Expr::Builtin { name: Builtin::Unique, args: vec![items] })),
      ("WeakMap", None) => Ok(Kind::WeakMap.made(Expr::Object(Vec::new()))),
      ("WeakMap", Some(pairs)) => {
        let pair = self.temp();
        let key = Expr::Index(Box::new(Expr::Var(pair.clone())), Box::new(Expr::Lit(Lit::Int(0))));
        let checked = Expr::Array(vec![Entry::Item(self.weak_checked(Kind::WeakMap, key)), Entry::Item(Expr::Index(Box::new(Expr::Var(pair.clone())), Box::new(Expr::Lit(Lit::Int(1)))))]);
        let pairs = Expr::Map(Box::new(Expr::Array(vec![Entry::Spread(pairs)])), Box::new(Expr::Lambda { params: vec![pair], body: Box::new(checked) }));
        Ok(Kind::WeakMap.made(Expr::Builtin { name: Builtin::MapOf, args: vec![pairs] }))
      }
      ("WeakSet", None) => Ok(Kind::WeakSet.made(Expr::Array(Vec::new()))),
      ("WeakSet", Some(items)) => {
        let item = self.temp();
        let checked = self.weak_checked(Kind::WeakSet, Expr::Var(item.clone()));
        let items = Expr::Map(Box::new(Expr::Array(vec![Entry::Spread(items)])), Box::new(Expr::Lambda { params: vec![item], body: Box::new(checked) }));
        Ok(Kind::WeakSet.made(Expr::Builtin { name: Builtin::Unique, args: vec![items] }))
      }
      ("WeakRef", Some(target)) => Ok(Expr::Object(vec![Entry::Field("$weakRef".to_owned(), target)])),
      ("FinalizationRegistry", _) => Ok(Expr::Object(Vec::new())),
      (name @ ("Number" | "Boolean" | "String"), given) => {
        let primitive = match (name, given) {
          ("Number", Some(x)) => Expr::Num(Box::new(x)),
          ("Number", None) => Expr::Lit(Lit::Float(0.0)),
          ("Boolean", Some(x)) => Expr::Not(Box::new(Expr::Not(Box::new(x)))),
          ("Boolean", None) => Expr::Lit(Lit::Bool(false)),
          (_, Some(x)) => Expr::Str(Box::new(x)),
          (_, None) => Expr::lit_str(""),
        };
        Ok(Expr::Builtin { name: Builtin::Tagged, args: vec![Expr::lit_str(snapfire_fsr_ir::render::typed::WRAPPER), primitive] })
      }
      ("ArrayBuffer" | "SharedArrayBuffer", length) => {
        let slot = self.temp();
        let length = Expr::Num(Box::new(length.unwrap_or(Expr::Lit(Lit::Float(0.0)))));
        Ok(crate::opaque_object(Expr::Map(Box::new(Expr::Builtin { name: Builtin::Range, args: vec![length] }), Box::new(Expr::Lambda { params: vec![slot], body: Box::new(Expr::Lit(Lit::Float(0.0))) }))))
      }
      ("DataView", Some(buffer)) => Ok(buffer),
      ("Proxy", Some(target)) => {
        let handler = match args.get(1) {
          Some(a) => self.expr(crate::syntax::arg_expr(a))?,
          None => return Err(self.residue(n.span, "`new Proxy` without a handler")),
        };
        Ok(Expr::Object(vec![Entry::Field(PROXY_TARGET.to_owned(), target), Entry::Field(PROXY_HANDLER.to_owned(), handler)]))
      }
      ("DisposableStack" | "AsyncDisposableStack", _) => Ok(crate::opaque_object(Expr::Object(vec![Entry::Field(super::writes::STACK.to_owned(), Expr::Array(Vec::new())), Entry::Field("disposed".to_owned(), Expr::Lit(Lit::Bool(false)))]))),
      (name, source) if crate::TYPED_ELEMENTS.iter().any(|(n, _)| *n == name) => {
        let element = crate::TYPED_ELEMENTS.iter().find(|(n, _)| *n == name).map(|(_, e)| *e).expect("matched");
        Ok(Expr::Builtin { name: Builtin::TypedNew, args: vec![Expr::lit_str(element), source.unwrap_or(Expr::Lit(Lit::Float(0.0)))] })
      }
      ("Array", None) => Ok(Expr::Array(Vec::new())),
      ("Array", Some(first)) if args.len() == 1 => {
        let held = self.temp();
        let number = Expr::Compare(CompareOp::Eq, Box::new(Expr::Builtin { name: Builtin::TypeOf, args: vec![Expr::Var(held.clone())] }), Box::new(Expr::lit_str("number")));
        let valid = Expr::Logic(LogicOp::And, Box::new(Expr::Compare(CompareOp::Eq, Box::new(Expr::Builtin { name: Builtin::Trunc, args: vec![Expr::Var(held.clone())] }), Box::new(Expr::Var(held.clone())))), Box::new(Expr::Logic(LogicOp::And, Box::new(Expr::Compare(CompareOp::Ge, Box::new(Expr::Var(held.clone())), Box::new(Expr::Lit(Lit::Float(0.0))))), Box::new(Expr::Compare(CompareOp::Le, Box::new(Expr::Var(held.clone())), Box::new(Expr::Lit(Lit::Float(4_294_967_295.0))))))));
        let error = crate::error_object(Expr::Object(vec![Entry::Field("name".to_owned(), Expr::lit_str("RangeError")), Entry::Field("message".to_owned(), Expr::lit_str("Invalid array length"))]));
        let thrown = Expr::Builtin { name: Builtin::Throw, args: vec![Expr::lit_str("Invalid array length"), error] };
        let slot = self.temp();
        let holes = Expr::Map(Box::new(Expr::Builtin { name: Builtin::Range, args: vec![Expr::Var(held.clone())] }), Box::new(Expr::Lambda { params: vec![slot], body: Box::new(Expr::Lit(Lit::Undefined)) }));
        let sized = Expr::Block(vec![Stmt::If { cond: Expr::Not(Box::new(valid)), then: vec![Stmt::Expr(thrown)], r#else: Vec::new() }, Stmt::Return(holes)]);
        Ok(Expr::Block(vec![Stmt::Let { name: held.clone(), expr: first }, Stmt::Return(Expr::Ternary(Box::new(number), Box::new(sized), Box::new(Expr::Array(vec![Entry::Item(Expr::Var(held))]))))]))
      }
      ("Array", Some(first)) => {
        let mut items = vec![Entry::Item(first)];
        for a in &args[1..] {
          items.push(Entry::Item(self.expr(crate::syntax::arg_expr(a))?));
        }
        Ok(Expr::Array(items))
      }
      ("Object", None) => Ok(Expr::Object(Vec::new())),
      ("Object", Some(x)) => {
        let held = self.temp();
        let kind = Expr::Builtin { name: Builtin::TypeOf, args: vec![Expr::Var(held.clone())] };
        let is = |of: &str| Expr::Compare(CompareOp::Eq, Box::new(kind.clone()), Box::new(Expr::lit_str(of)));
        let object = Expr::Logic(LogicOp::Or, Box::new(is("object")), Box::new(is("function")));
        let wrapped = Expr::Builtin { name: Builtin::Tagged, args: vec![Expr::lit_str(snapfire_fsr_ir::render::typed::WRAPPER), Expr::Var(held.clone())] };
        let answer = Expr::Ternary(Box::new(Expr::Builtin { name: Builtin::IsNullish, args: vec![Expr::Var(held.clone())] }), Box::new(Expr::Object(Vec::new())), Box::new(Expr::Ternary(Box::new(object), Box::new(Expr::Var(held.clone())), Box::new(wrapped))));
        Ok(Expr::Block(vec![Stmt::Let { name: held, expr: x }, Stmt::Return(answer)]))
      }
      ("Response", body) => {
        let mut args = vec![body.unwrap_or(Expr::Lit(Lit::Undefined))];
        if let Some(init) = n.arguments.get(1) {
          args.push(self.expr(crate::syntax::arg_expr(init))?);
        }
        Ok(Expr::Builtin { name: Builtin::Response, args })
      }
      ("Request", Some(url)) => {
        let mut args = vec![url];
        if let Some(init) = n.arguments.get(1) {
          args.push(self.expr(crate::syntax::arg_expr(init))?);
        }
        Ok(Expr::Builtin { name: Builtin::RequestNew, args })
      }
      ("URLSearchParams", None) => Ok(Expr::lit_str("")),
      ("URLSearchParams", Some(x)) => Ok(Expr::Builtin { name: Builtin::FormEncode, args: vec![x] }),
      ("URL", Some(x)) => {
        let mut args = vec![x];
        if let Some(base) = n.arguments.get(1) {
          args.push(self.expr(crate::syntax::arg_expr(base))?);
        }
        Ok(Expr::Builtin { name: Builtin::UrlParse, args })
      }
      ("RegExp", Some(_)) if matches!(crate::syntax::arg_expr(&args[0]), js::Expression::RegExpLiteral(_)) => Err(self.residue(n.span, "`new RegExp` of a regular expression")),
      ("RegExp", Some(pattern)) => {
        let flags = match args.get(1) {
          Some(a) => Some(crate::syntax::arg_expr(a)),
          None => None,
        };
        if let (js::Expression::StringLiteral(p), None | Some(js::Expression::StringLiteral(_))) = (crate::syntax::arg_expr(&args[0]), flags) {
          let flags = match flags {
            Some(js::Expression::StringLiteral(f)) => crate::lossy(f.value),
            _ => String::new(),
          };
          return regex_value(self, crate::lossy(p.value).as_ref(), &flags, n.span);
        }
        let flags = match flags {
          Some(f) => Expr::Str(Box::new(self.expr(f)?)),
          None => Expr::lit_str(""),
        };
        Ok(Expr::Object(vec![
          Entry::Field(snapfire_fsr_ir::jsregex::PATTERN.to_owned(), Expr::Str(Box::new(pattern))),
          Entry::Field(snapfire_fsr_ir::jsregex::FLAGS.to_owned(), flags),
          Entry::Field(snapfire_fsr_ir::jsregex::LAST_INDEX.to_owned(), Expr::Lit(Lit::Float(0.0))),
        ]))
      }
      (other, _) => Err(self.residue(n.span, format!("`new {other}`"))),
    }
  }

  /// `Date.now()` or `new Date()`: the request's clock in a body; on a render path the server and the browser would each read their own moment.
  pub(crate) fn date_now(&mut self, span: Span) -> Lowered<Expr> {
    if self.on_render_path() && !self.reaches(Reach::Terminal) {
      return Err(self.browser_only(span, "the current time on a render path, which the server and the browser read at different moments"));
    }
    Ok(Expr::Num(Box::new(Expr::Ext { module: "time".to_owned(), name: "now".to_owned(), args: Vec::new() })))
  }
}

impl Lowerer<'_> {
  /// The instant a wall-clock time in this machine's time zone names, `wall` written as milliseconds as though it were UTC. It tries the offsets in force a day before and a day after and keeps the one that names `wall`. A time a change to daylight saving skips or repeats takes the offset from before the change, as JavaScript's `UTC(t)` does.
  pub(crate) fn local_to_utc(&mut self, wall: Expr) -> Expr {
    let (at, before, after, early, late) = (self.temp(), self.temp(), self.temp(), self.temp(), self.temp());
    let var = |n: &String| Expr::Var(n.clone());
    let offset = |of: Expr| Expr::Ext { module: "time".to_owned(), name: "localOffset".to_owned(), args: vec![of] };
    let day = |sign: ArithOp| Expr::Arith(sign, Box::new(var(&at)), Box::new(Expr::Lit(Lit::Float(86_400_000.0))));
    let moved = |by: &String| Expr::Arith(ArithOp::Add, Box::new(var(&at)), Box::new(Expr::Arith(ArithOp::Mul, Box::new(var(by)), Box::new(Expr::Lit(Lit::Float(60_000.0))))));
    let holds = |t: &String, o: &String, op: CompareOp| Expr::Compare(op, Box::new(offset(var(t))), Box::new(var(o)));
    let only_late = Expr::Logic(LogicOp::And, Box::new(holds(&early, &before, CompareOp::Ne)), Box::new(holds(&late, &after, CompareOp::Eq)));
    Expr::Block(vec![
      Stmt::Let { name: at.clone(), expr: wall },
      Stmt::Let { name: before.clone(), expr: offset(day(ArithOp::Sub)) },
      Stmt::Let { name: after.clone(), expr: offset(day(ArithOp::Add)) },
      Stmt::Let { name: early.clone(), expr: moved(&before) },
      Stmt::Let { name: late.clone(), expr: moved(&after) },
      Stmt::Return(Expr::Ternary(Box::new(only_late), Box::new(var(&late)), Box::new(var(&early)))),
    ])
  }
}

impl Lowerer<'_> {
  /// `new Function(...params, body)` and `Function(...)` with every argument a string written in place: the function that text declares, read at build. It sees the module's globals and none of the locals around it, as JavaScript's does.
  pub(crate) fn function_from_text(&mut self, args: &[js::Argument], span: Span) -> Lowered<Expr> {
    let mut texts = Vec::with_capacity(args.len());
    for a in args {
      match strip_parens(crate::syntax::arg_expr(a)) {
        js::Expression::StringLiteral(s) => texts.push(crate::lossy(s.value).to_string()),
        js::Expression::TemplateLiteral(t) if t.expressions.is_empty() => texts.push(t.quasis.iter().filter_map(|q| q.value.cooked.map(crate::lossy)).collect::<Vec<_>>().concat()),
        other => return Err(self.residue_with(other.span(), "`Function` of text made at run time", "the build reads a function only from text written in place; write the function itself")),
      }
    }
    let body = texts.pop().unwrap_or_default();
    let source = format!("(function anonymous({}\n) {{\n{body}\n}})", texts.join(","));
    let (line, column) = self.parsed.loc(span.start);
    let parsed = self.parsed.parse_expr_at(&source, line, column).map_err(|e| self.residue(span, format!("`Function` of text that does not parse: {e}")))?;
    let parsed = self.parsed.arena().alloc(parsed);
    let outer = std::mem::take(&mut self.scope);
    let lowered = self.expr(parsed);
    self.scope = outer;
    lowered
  }
}

/// The field a `Proxy` keeps its target under.
pub(crate) const PROXY_TARGET: &str = "$target";
/// The field a `Proxy` keeps its handler under.
pub(crate) const PROXY_HANDLER: &str = "$handler";

impl Lowerer<'_> {
  /// The `trap` of the proxy `proxy` called with its target, `args` and the proxy, when its handler has one; else `plain` over the target and `args`.
  pub(crate) fn proxy_trap(&mut self, trap: &str, proxy: Expr, args: Vec<Expr>, plain: impl FnOnce(Expr, &[Expr]) -> Expr) -> Expr {
    let (held, values) = (self.temp(), self.temp());
    let target = Expr::Field(Box::new(Expr::Var(held.clone())), PROXY_TARGET.to_owned());
    let handler = Expr::Field(Box::new(Expr::Var(held.clone())), PROXY_HANDLER.to_owned());
    let read: Vec<Expr> = (0..args.len()).map(|i| Expr::Index(Box::new(Expr::Var(values.clone())), Box::new(Expr::Lit(Lit::Int(i as i128))))).collect();
    let mut called = vec![target.clone()];
    called.extend(read.iter().cloned());
    if trap != "ownKeys" {
      called.push(Expr::Var(held.clone()));
    }
    let trapped = Expr::Apply { f: Box::new(Expr::Field(Box::new(handler.clone()), trap.to_owned())), args: called };
    let has = Expr::Not(Box::new(Expr::Builtin { name: Builtin::IsNullish, args: vec![Expr::Field(Box::new(handler), trap.to_owned())] }));
    Expr::Block(vec![
      Stmt::Let { name: held, expr: proxy },
      Stmt::Let { name: values, expr: Expr::Array(args.into_iter().map(Entry::Item).collect()) },
      Stmt::Return(Expr::Ternary(Box::new(has), Box::new(trapped), Box::new(plain(target, &read)))),
    ])
  }
}
