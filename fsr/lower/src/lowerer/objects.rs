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
    if let js::Expression::Identifier(id) = &n.callee {
      if self.parsed.ctxt(id) == crate::Ctxt::Unresolved && ERROR_CONSTRUCTORS.contains(&id.name.as_str()) {
        let message = match args.first() {
          Some(a) => Expr::Str(Box::new(self.expr(crate::syntax::arg_expr(a))?)),
          None => Expr::lit_str(""),
        };
        return Ok(Expr::Object(vec![Entry::Field("name".to_owned(), Expr::lit_str(id.name.as_str())), Entry::Field("message".to_owned(), message)]));
      }
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
      ("Date", None) => self.date_now(n.span),
      ("Date", Some(x)) if args.len() == 1 => Ok(Expr::Builtin { name: Builtin::DateMs, args: vec![x] }),
      ("Date", Some(_)) => Err(self.residue_with(n.span, "`new Date(year, month, ...)`, which reads the viewer's time zone", "an ISO 8601 string ending in `Z` names the same instant everywhere, `new Date(\"2026-10-05T00:00:00Z\")`")),
      ("Map", None) => Ok(Kind::Map.made(Expr::Object(Vec::new()))),
      ("Map", Some(pairs)) => Ok(Kind::Map.made(Expr::Builtin { name: Builtin::MapOf, args: vec![pairs] })),
      ("Set", None) => Ok(Kind::Set.made(Expr::Array(Vec::new()))),
      ("Set", Some(items)) => Ok(Kind::Set.made(Expr::Builtin { name: Builtin::Unique, args: vec![items] })),
      ("Response", body) => {
        let mut args = vec![body.unwrap_or(Expr::Lit(Lit::Undefined))];
        if let Some(init) = n.arguments.get(1) {
          args.push(self.expr(crate::syntax::arg_expr(init))?);
        }
        Ok(Expr::Builtin { name: Builtin::Response, args })
      }
      ("Request", Some(url)) => Ok(Expr::Builtin { name: Builtin::RequestNew, args: vec![url] }),
      ("Uint8Array", source) => Ok(Expr::Builtin { name: Builtin::Uint8Array, args: source.into_iter().collect() }),
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
