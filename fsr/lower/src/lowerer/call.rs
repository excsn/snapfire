use crate::*;

/// Methods whose lowering takes a spread argument itself, through [`Builtin::Spread`] or the function it calls.
const SPREADS: &[&str] = &["fromCharCode", "fromCodePoint", "slice", "at", "indexOf", "lastIndexOf", "charAt", "charCodeAt", "codePointAt", "concat", "padStart", "padEnd", "substring", "call", "bind"];

impl<'a> Lowerer<'a> {
  pub(crate) fn call(&mut self, call: &js::CallExpression) -> Lowered<Expr> {
    if let Some(ok) = self.settled_promise(call) {
      let value = match call.arguments.as_slice() {
        [] => Expr::Lit(Lit::Undefined),
        [value] if !crate::syntax::is_spread(value) => self.expr(crate::syntax::arg_expr(value))?,
        _ => return Err(self.residue(call.span, "`Promise.resolve` or `Promise.reject` with more than one value")),
      };
      return Ok(self.settled(ok, value));
    }
    if self.rejects_here(call) {
      let [reason] = call.arguments.as_slice() else { return Err(self.residue(call.span, "`Promise.reject` takes one reason")) };
      let held = self.temp();
      let reason = self.expr(crate::syntax::arg_expr(reason))?;
      let message = Expr::Coalesce(Box::new(Expr::Var(held.clone()).field("message".to_owned())), Box::new(Expr::Var(held.clone())));
      return Ok(Expr::Block(vec![Stmt::Let { name: held.clone(), expr: reason }, Stmt::Return(Expr::Builtin { name: Builtin::Throw, args: vec![message, Expr::Var(held)] })]));
    }
    if self.calls_next(call) {
      let request = match call.arguments.as_slice() {
        [] => Expr::Lit(Lit::Undefined),
        [request] => self.expr(crate::syntax::arg_expr(request))?,
        _ => return Err(self.residue(call.span, "`next` takes one request")),
      };
      return Ok(Expr::Next(Box::new(request)));
    }
    if let Some(read) = self.response_header_read(call)? {
      return Ok(read);
    }
    if let (Some(reader), callee) = (&self.reader, &call.callee) {
      if matches!(callee, js::Expression::Identifier(id) if self.parsed.ctxt(id) == Ctxt::Local(*reader)) {
        let named = call.arguments.first().ok_or_else(|| self.residue(call.span, "a compute's `read` takes the source key"))?;
        return match self.expr(&crate::syntax::arg_expr(&named))? {
          Expr::Lit(Lit::Str(key)) => Ok(Expr::Store(key)),
          _ => Err(self.residue(crate::syntax::arg_expr(&named).span(), "a source key the build cannot read")),
        };
      }
    }
    if let Some(suspended) = self.suspend_call(call)? {
      return Ok(suspended);
    }
    if let (true, callee, Some(js::Expression::StringLiteral(id))) = (self.reaches(Reach::Process), &call.callee, call.arguments.first().map(|a| crate::meaning::bare(crate::syntax::arg_expr(a)))) {
      if self.meanings.as_ref().and_then(|meanings| meanings.of_expr(callee)) == Some(meaning::Meaning::Library(meaning::Construct::ClientAction)) {
        return Ok(Expr::Object(vec![Entry::Field(snapfire_fsr_ir::host::ACT_REF.to_owned(), Expr::lit_str(crate::lossy(id.value)))]));
      }
    }
    if self.reaches(Reach::Process) && self.spawns(call) {
      return self.spawn_value(call);
    }
    if let Some(chained) = self.then_call(call)? {
      return Ok(chained);
    }
    {
      let callee = &call.callee;
      if self.meanings.as_ref().and_then(|meanings| meanings.of_expr(callee)) == Some(meaning::Meaning::Library(meaning::Construct::Emit)) {
        let mut args = Vec::with_capacity(2);
        for a in call.arguments.iter().take(2) {
          args.push(self.expr(crate::syntax::arg_expr(a))?);
        }
        return Ok(Expr::Builtin { name: Builtin::Emit, args });
      }
      if self.meanings.as_ref().and_then(|meanings| meanings.of_expr(callee)).and_then(|meant| meant.store_call()) == Some("set") {
        let [key, value] = call.arguments.as_slice() else { return Err(self.residue(call.span, "`set` takes a key and a value")) };
        let Expr::Lit(Lit::Str(key)) = self.expr(crate::syntax::arg_expr(key))? else { return Err(self.residue(crate::syntax::arg_expr(key).span(), "a store key that is not a string the build can read")) };
        return Ok(Expr::WriteStore(key, Box::new(self.expr(crate::syntax::arg_expr(value))?)));
      }
    }
    let callee = &call.callee;
    if let Some(called) = self.class_call(call, callee)? {
      return Ok(called);
    }
    if let Some(called) = self.instance_call(call, callee)? {
      return Ok(called);
    }
    if let Some(read) = self.context_read(call, callee)? {
      return Ok(read);
    }
    if let Some(hook @ ("useDeferredValue" | "useFormStatus" | "useSyncExternalStore")) = self.meanings.as_ref().and_then(|meanings| meanings.of_expr(callee)).and_then(|meant| meant.hook()) {
      return self.server_hook_value(hook, call);
    }
    if let Some(module) = self.meanings.as_ref().and_then(|meanings| meanings.of_expr(callee)).and_then(|meant| meant.std()).filter(|module| *module != "native") {
      if module != "t" {
        return Err(self.residue(callee.span(), format!("`{module}` from the standard library is a module; call one of its members")));
      }
      let mut args = Vec::with_capacity(call.arguments.len());
      for a in &call.arguments {
        args.push(self.expr(crate::syntax::arg_expr(a))?);
      }
      return Ok(self.candidate(call.span, Expr::Ext { module: "i18n".to_owned(), name: "t".to_owned(), args }));
    }
    let element_builtin = match self.meanings.as_ref().and_then(|meanings| meanings.of_expr(callee)) {
      Some(meaning::Meaning::Library(meaning::Construct::CloneElement)) => Some(Builtin::CloneElement),
      Some(meaning::Meaning::Library(meaning::Construct::IsValidElement)) => Some(Builtin::IsElement),
      _ => None,
    };
    if let Some(name) = element_builtin {
      let mut args = Vec::with_capacity(call.arguments.len());
      for a in &call.arguments {
        args.push(self.expr(crate::syntax::arg_expr(a))?);
      }
      return Ok(Expr::Builtin { name, args });
    }
    if let Some(meaning::Meaning::Library(meaning::Construct::SvelteStore(store))) = self.meanings.as_ref().and_then(|meanings| meanings.of_expr(callee)) {
      return self.svelte_store(store, call);
    }
    if let Some(called) = self.std_call(call, callee)? {
      return Ok(called);
    }
    if let Some(meaning::Meaning::Library(meaning::Construct::Extension(key))) = self.meanings.as_ref().and_then(|meanings| meanings.of_expr(callee)) {
      let (module, member) = key.split_once('.').expect("an extension is `module.member`");
      let reach = standard_reach(module, member).expect("a package maps to a standard member");
      if let (false, Some(needs)) = (self.reaches(reach), reach.needs()) {
        return Err(self.residue(call.span, format!("a call to `{key}`, which {needs}")));
      }
      self.check_reach(call.span, key, reach)?;
      let mut args = Vec::with_capacity(call.arguments.len());
      for a in &call.arguments {
        if crate::syntax::is_spread(a) {
          return Err(self.residue(crate::syntax::arg_expr(a).span(), format!("a spread into `{key}`")));
        }
        args.push(self.expr(crate::syntax::arg_expr(a))?);
      }
      return Ok(Expr::Ext { module: module.to_owned(), name: member.to_owned(), args });
    }
    if self.meanings.as_ref().and_then(|meanings| meanings.of_expr(callee)) == Some(meaning::Meaning::Library(meaning::Construct::ClassNames)) {
      let mut items = Vec::with_capacity(call.arguments.len());
      for a in &call.arguments {
        items.push(Entry::Item(self.expr(crate::syntax::arg_expr(a))?));
      }
      return Ok(Expr::Builtin { name: Builtin::ClassNames, args: vec![Expr::Array(items)] });
    }
    if let Some(member) = (callee).as_member_expression() {
      if let Some(module) = self.meanings.as_ref().and_then(|meanings| meanings.of_expr(member.object())).and_then(|meant| meant.std()) {
        let method = self.member_name(member).ok_or_else(|| self.residue(member.span(), "a computed method name"))?;
        let key = format!("{module}.{method}");
        let reach = standard_reach(module, &method).ok_or_else(|| self.residue(member.span(), format!("`{key}` is not a member of the standard library")))?;
        self.check_reach(call.span, &key, reach)?;
        let mut args = Vec::with_capacity(call.arguments.len());
        for a in &call.arguments {
          args.push(self.expr(crate::syntax::arg_expr(a))?);
        }
        let expr = Expr::Ext { module: module.to_owned(), name: method, args };
        return Ok(if reach == Reach::Render { self.candidate(call.span, expr) } else { expr });
      }
    }
    if self.meanings.as_ref().and_then(|meanings| meanings.of_expr(callee)) == Some(meaning::Meaning::Library(meaning::Construct::StoreKey)) {
      let named = call.arguments.first().ok_or_else(|| self.residue(call.span, "`key` takes the key's name"))?;
      return self.expr(&crate::syntax::arg_expr(&named));
    }
    let math = (callee).as_member_expression().is_some_and(|m| matches!(m.object(), js::Expression::Identifier(id) if id.name.as_str() == "Math"));
    let applied = matches!(callee, js::Expression::Identifier(id) if self.scope.iter().any(|(n, _)| *n == id.name.as_str()) || self.local_fns.iter().any(|(n, _)| *n == id.name.as_str()) || self.globals.iter().any(|(n, _)| *n == id.name.as_str()))
      || matches!(crate::meaning::bare(callee), js::Expression::ArrowFunctionExpression(_) | js::Expression::FunctionExpression(_));
    let spreads_itself = callee.as_member_expression().is_some_and(|m| matches!(crate::syntax::prop(m), crate::syntax::MemberProp::Ident(p) if SPREADS.contains(&p.name.as_str())));
    for arg in &call.arguments {
      if crate::syntax::is_spread(arg) && !math && !applied && !spreads_itself {
        if let js::Expression::Identifier(id) = callee {
          self.unbound = Some(id.name.to_string());
        }
        return Err(self.residue(crate::syntax::arg_expr(arg).span(), "a spread argument"));
      }
    }

    if let js::Expression::Identifier(id) = callee {
      let name = id.name.as_str();
      if !self.scope.iter().any(|(n, _)| n == name) && self.root_of(id).is_none() {
        if let Some(function) = self.local_fns.iter().rev().find(|(n, _)| n == name).map(|(_, f)| *f) {
          let f = Box::new(self.function_expr(&function)?);
          return self.applied(f, call);
        }
        if let Some((_, f)) = self.globals.iter().rev().find(|(n, _)| n == name) {
          let f = Box::new(f.clone());
          if let Expr::Ext { module, name: member, args: declared } = &*f {
            if declared.is_empty() {
              let key = format!("{module}.{member}");
              let reach = self.natives.iter().find(|(n, _)| *n == key).map(|(_, r)| *r).unwrap_or(Reach::Render);
              self.check_reach(call.span, &key, reach)?;
              let mut args = Vec::with_capacity(call.arguments.len());
              for a in &call.arguments {
                args.push(self.expr(crate::syntax::arg_expr(a))?);
              }
              let expr = Expr::Ext { module: module.clone(), name: member.clone(), args };
              return Ok(if reach == Reach::Render { self.candidate(call.span, expr) } else { expr });
            }
          }
          if !matches!(&*f, Expr::Lambda { .. } | Expr::Fix { .. }) && !matches!(&*f, Expr::Var(v) if v == name) {
            return Err(self.residue(id.span, format!("`{name}` is not a function")));
          }
          if self.on_render_path() && !self.reaches(Reach::Terminal) {
            if let Some(key) = self.body_extension_in(&f) {
              if self.reads_only_the_date_clock(&f) {
                return Err(self.browser_only(call.span, format!("`{name}` reads the current time or the viewer's time zone on a render path, which only the browser knows")));
              }
              self.reach_violation = true;
              return Err(self.residue(call.span, format!("`{name}` calls `{key}` on a render path")));
            }
          }
          let applied = self.applied(f, call)?;
          return Ok(self.candidate(call.span, applied));
        }
        let one = |this: &mut Self| -> Lowered<Box<Expr>> {
          let a = call.arguments.first().ok_or_else(|| this.residue(call.span, format!("`{name}` takes one argument")))?;
          Ok(Box::new(this.expr(crate::syntax::arg_expr(a))?))
        };
        return match name {
          "String" if matches!(call.arguments.as_slice(), [only] if !crate::syntax::is_spread(only)) => Ok(Expr::Str(Box::new(self.as_text(&crate::syntax::arg_expr(&call.arguments[0]))?))),
          "String" => Ok(Expr::Str(one(self)?)),
          "Number" if matches!(call.arguments.as_slice(), [only] if !crate::syntax::is_spread(only)) => Ok(Expr::Num(Box::new(self.as_primitive(&crate::syntax::arg_expr(&call.arguments[0]))?))),
          "Number" => Ok(Expr::Num(one(self)?)),
          "BigInt" => Ok(Expr::BigInt(one(self)?)),
          "Function" => self.function_from_text(&call.arguments, call.span),
          "Boolean" => Ok(Expr::Not(Box::new(Expr::Not(one(self)?)))),
          "encodeURIComponent" => Ok(Expr::Builtin { name: Builtin::EncodeUriComponent, args: vec![*one(self)?] }),
          "decodeURIComponent" => Ok(Expr::Builtin { name: Builtin::DecodeUriComponent, args: vec![*one(self)?] }),
          "isNaN" => Ok(Expr::Builtin { name: Builtin::IsNaN, args: vec![*one(self)?] }),
          "btoa" | "atob" => Ok(Expr::Ext { module: "encoding".to_owned(), name: name.to_owned(), args: vec![*one(self)?] }),
          "structuredClone" if call.arguments.len() <= 1 => { let value = *one(self)?; Ok(if heap_pass() { Expr::Builtin { name: Builtin::CopyDeep, args: vec![value] } } else { value }) }
          "parseInt" => {
            let mut args = vec![*one(self)?];
            if let Some(radix) = call.arguments.get(1) {
              args.push(self.expr(crate::syntax::arg_expr(radix))?);
            }
            Ok(Expr::Builtin { name: Builtin::ParseInt, args })
          }
          "parseFloat" => Ok(Expr::Builtin { name: Builtin::ParseFloat, args: vec![*one(self)?] }),
          "isFinite" => {
            let n = self.temp();
            let value = Expr::Num(one(self)?);
            let difference = Expr::Arith(ArithOp::Sub, Box::new(Expr::Var(n.clone())), Box::new(Expr::Var(n.clone())));
            Ok(Expr::Block(vec![Stmt::Let { name: n, expr: value }, Stmt::Return(Expr::Not(Box::new(Expr::Builtin { name: Builtin::IsNaN, args: vec![difference] })))]))
          }
          "encodeURI" => Ok(Expr::Builtin { name: Builtin::EncodeUri, args: vec![*one(self)?] }),
          "decodeURI" => Ok(Expr::Builtin { name: Builtin::DecodeUri, args: vec![*one(self)?] }),
          name if ERROR_CONSTRUCTORS.contains(&name) && !matches!(name, "AggregateError" | "SuppressedError") => {
            let message = match call.arguments.first() {
              Some(a) => Expr::Str(Box::new(self.expr(crate::syntax::arg_expr(a))?)),
              None => Expr::lit_str(""),
            };
            Ok(crate::error_object(Expr::Object(vec![Entry::Field("name".to_owned(), Expr::lit_str(name)), Entry::Field("message".to_owned(), message)])))
          }
          "Symbol" => Ok(Expr::Builtin { name: Builtin::SymbolNew, args: self.plain_args(call)? }),
          "fail" => Err(self.residue(call.span, "`fail` inside an expression; it is a statement")),
          "redirect" if self.parsed.names_server(&call.callee, "redirect") => self.redirect_to(call),
          "after" if self.parsed.names_server(&call.callee, "after") => self.after_of(call),
          "$state" | "$derived" if self.parsed.file.ends_with(".svelte.ts") || self.parsed.file.ends_with(".svelte.js") => match call.arguments.first() {
            Some(_) => Ok(*one(self)?),
            None => Ok(Expr::Lit(Lit::Null)),
          },
          _ => {
            self.unbound = Some(name.to_owned());
            if let Some((source, _)) = crate::component::find_import(self.parsed, name).filter(|(source, _)| node_module(source)) {
              return Err(self.residue_with(id.span, format!("`{name}` from `{source}`, which is not FSR's"), NOT_FSR));
            }
            Err(self.residue_with(
              id.span,
              format!("a call to `{name}`, which the build cannot follow"),
              "the build reads a helper declared at module level, or imported from a file under the app; a bare package import is not followed, and neither is a name nothing declares",
            ))
          }
        };
      }
    }

    let Some(member) = (callee).as_member_expression() else {
      let f = Box::new(self.expr(callee)?);
      return self.applied(f, call);
    };
    if let Some(applied) = self.function_member(member, call)? {
      return Ok(applied);
    }
    let method = self.member_name(member).ok_or_else(|| self.residue(member.span(), "a computed method name"))?;
    if method == "toString" && call.arguments.is_empty() {
      if let Some(text) = self.function_property(member.object(), "toString") {
        return Ok(text);
      }
    }
    if self.kind_of(member.object()) == Some(Kind::Typed) && !matches!(method.as_str(), "set" | "sort" | "reverse" | "fill" | "copyWithin") {
      return self.typed_call(&method, member, call);
    }
    if self.kind_of(member.object()) == Some(Kind::Scope) {
      return self.scope_call(&method, member, call);
    }
    if method == "supportedLocalesOf" && crate::intl_member(member.object()).is_some() {
      return Ok(Expr::Ext { module: "intl".to_owned(), name: "supportedLocales".to_owned(), args: self.plain_args(call)? });
    }

    if let js::Expression::Identifier(obj) = member.object() {
      let global = obj.name.as_str();
      if !self.scope.iter().any(|(n, _)| n == global) && !self.globals.iter().any(|(n, _)| n == global) {
        match global {
          "Object" if matches!(method.as_str(), "assign" | "freeze" | "hasOwn" | "is" | "groupBy") => return self.object_static(&method, call),
          "Uint8Array" if matches!(method.as_str(), "fromBase64" | "fromHex") => return Ok(Expr::Ext { module: "encoding".to_owned(), name: method.clone(), args: self.plain_args(call)? }),
          typed if crate::TYPED_ELEMENTS.iter().any(|(name, _)| *name == typed) && matches!(method.as_str(), "from" | "of") => {
            let element = crate::TYPED_ELEMENTS.iter().find(|(name, _)| *name == typed).map(|(_, e)| *e).expect("matched");
            let mut items = Vec::with_capacity(call.arguments.len());
            for (at, a) in call.arguments.iter().enumerate() {
              let value = self.expr(crate::syntax::arg_expr(a))?;
              match method.as_str() {
                "from" if at == 0 => items.push(Entry::Spread(value)),
                "from" => return Err(self.residue(call.span, format!("`{typed}.from` with a mapping function"))),
                _ => items.push(if crate::syntax::is_spread(a) { Entry::Spread(value) } else { Entry::Item(value) }),
              }
            }
            return Ok(Expr::Builtin { name: Builtin::TypedNew, args: vec![Expr::lit_str(element), Expr::Array(items)] });
          }
          "Array" if method == "of" => {
            let mut items = Vec::with_capacity(call.arguments.len());
            for a in &call.arguments {
              let value = self.expr(crate::syntax::arg_expr(a))?;
              items.push(if crate::syntax::is_spread(a) { Entry::Spread(value) } else { Entry::Item(value) });
            }
            return Ok(Expr::Array(items));
          }
          "String" if matches!(method.as_str(), "fromCharCode" | "fromCodePoint") => {
            let name = if method == "fromCharCode" { Builtin::FromCharCode } else { Builtin::FromCodePoint };
            return self.builtin_call(name, Vec::new(), call);
          }
          "Promise" if method == "allSettled" => return self.all_settled(call),
          "Promise" if matches!(method.as_str(), "try" | "any" | "race") => return self.promise_static(&method, call),
          "Promise" if method == "withResolvers" => return Ok(self.with_resolvers()),
          "String" if method == "raw" => return self.string_raw(call),
          "Map" if method == "groupBy" => return self.map_group_by(call),
          "BigInt" if matches!(method.as_str(), "asIntN" | "asUintN") => return self.big_int_wrap(&method, call),
          "Array" if method == "fromAsync" => return self.from_async(call),
          "Reflect" => return self.reflect(&method, call),
          "Intl" if method == "supportedValuesOf" => {
            let [key] = call.arguments.as_slice() else { return Err(self.residue(call.span, "`Intl.supportedValuesOf` takes one key")) };
            let key = Expr::Str(Box::new(self.expr(crate::syntax::arg_expr(key))?));
            return Ok(Expr::Ext { module: "intl".to_owned(), name: "supported".to_owned(), args: vec![key] });
          }
          "Intl" if method == "getCanonicalLocales" => {
            let given = match call.arguments.first() {
              Some(a) => self.expr(crate::syntax::arg_expr(a))?,
              None => Expr::Array(Vec::new()),
            };
            let (held, tag) = (self.temp(), self.temp());
            let string = Expr::Compare(CompareOp::Eq, Box::new(Expr::Builtin { name: Builtin::TypeOf, args: vec![Expr::Var(held.clone())] }), Box::new(Expr::lit_str("string")));
            let tags = Expr::Ternary(Box::new(string), Box::new(Expr::Array(vec![Entry::Item(Expr::Var(held.clone()))])), Box::new(Expr::Array(vec![Entry::Spread(Expr::Var(held.clone()))])));
            let canonical = Expr::Lambda { params: vec![tag.clone()], body: Box::new(Expr::Ext { module: "intl".to_owned(), name: "locale".to_owned(), args: vec![Expr::Str(Box::new(Expr::Var(tag))), Expr::lit_str("canonical")] }) };
            let unique = Expr::Builtin { name: Builtin::Unique, args: vec![Expr::Map(Box::new(tags), Box::new(canonical))] };
            return Ok(Expr::Block(vec![Stmt::Let { name: held, expr: given }, Stmt::Return(Expr::Array(vec![Entry::Spread(unique)]))]));
          }
          "Iterator" if method == "concat" => {
            let mut parts = Vec::with_capacity(call.arguments.len());
            for a in &call.arguments {
              parts.push(Entry::Spread(self.expr(crate::syntax::arg_expr(a))?));
            }
            return Ok(Expr::Array(parts));
          }
          "RegExp" if method == "escape" => {
            let given = match call.arguments.first() {
              Some(a) => self.expr(crate::syntax::arg_expr(a))?,
              None => Expr::Lit(Lit::Undefined),
            };
            return Ok(Expr::Builtin { name: Builtin::RegExpEscape, args: vec![given] });
          }
          "Error" if method == "isError" => {
            let given = match call.arguments.first() {
              Some(a) => self.expr(crate::syntax::arg_expr(a))?,
              None => Expr::Lit(Lit::Undefined),
            };
            return Ok(Expr::Builtin { name: Builtin::Is, args: vec![given, Expr::lit_str("error")] });
          }
          "ArrayBuffer" if method == "isView" => {
            let Some(a) = call.arguments.first() else { return Ok(Expr::Lit(Lit::Bool(false))) };
            if matches!(self.kind_of(crate::syntax::arg_expr(a)), Some(Kind::View | Kind::Typed)) {
              return Ok(Expr::Lit(Lit::Bool(true)));
            }
            let given = self.expr(crate::syntax::arg_expr(a))?;
            return Ok(Expr::Builtin { name: Builtin::Is, args: vec![given, Expr::lit_str("view")] });
          }
          "URL" if matches!(method.as_str(), "parse" | "canParse") => {
            let tried = Expr::Builtin { name: Builtin::UrlTry, args: self.plain_args(call)? };
            return Ok(if method == "parse" { tried } else { Expr::Not(Box::new(Expr::Builtin { name: Builtin::IsNullish, args: vec![tried] })) });
          }
          "Object" if method == "getOwnPropertySymbols" => {
            let a = call.arguments.first().ok_or_else(|| self.residue(call.span, "`Object.getOwnPropertySymbols` takes an object"))?;
            let held = self.expr(crate::syntax::arg_expr(a))?;
            return Ok(Expr::Builtin { name: Builtin::SymbolKeys, args: vec![held] });
          }
          "Object" if method == "getOwnPropertyDescriptors" => {
            let a = call.arguments.first().ok_or_else(|| self.residue(call.span, "`Object.getOwnPropertyDescriptors` takes an object"))?;
            let held = self.expr(crate::syntax::arg_expr(a))?;
            let pair = self.temp();
            let at = |i: f64| Expr::Index(Box::new(Expr::Var(pair.clone())), Box::new(Expr::Lit(Lit::Float(i))));
            let field = |name: &str, value: Expr| Entry::Field(name.to_owned(), value);
            let described = Expr::Array(vec![Entry::Item(at(0.0)), Entry::Item(Expr::Object(vec![field("value", at(1.0)), field("writable", Expr::Lit(Lit::Bool(true))), field("enumerable", Expr::Lit(Lit::Bool(true))), field("configurable", Expr::Lit(Lit::Bool(true)))]))]);
            let pairs = Expr::Map(Box::new(Expr::Entries(Box::new(held))), Box::new(Expr::Lambda { params: vec![pair.clone()], body: Box::new(described) }));
            return Ok(Expr::Builtin { name: Builtin::FromEntries, args: vec![pairs] });
          }
          "Iterator" if method == "from" => {
            let [source] = call.arguments.as_slice() else { return Err(self.residue(call.span, "`Iterator.from` takes one iterable")) };
            let items = self.expr(crate::syntax::arg_expr(source))?;
            return Ok(Expr::Array(vec![Entry::Spread(items)]));
          }
          "Symbol" if matches!(method.as_str(), "for" | "keyFor") => return self.symbol_static(&method, call),
          "JSON" if matches!(method.as_str(), "rawJSON" | "isRawJSON") => return self.raw_json(&method, call),
          "Object" if matches!(method.as_str(), "getOwnPropertyNames" | "create" | "getOwnPropertyDescriptor" | "getPrototypeOf" | "isFrozen" | "isSealed" | "isExtensible" | "seal" | "preventExtensions") => return self.object_reflect(&method, call),
          "Object" => {
            let a = call.arguments.first().ok_or_else(|| self.residue(call.span, format!("`Object.{method}` takes one argument")))?;
            let target = Box::new(self.expr(crate::syntax::arg_expr(a))?);
            return match method.as_str() {
              "entries" => Ok(Expr::Entries(target)),
              "fromEntries" if self.kind_of(crate::syntax::arg_expr(a)) == Some(Kind::Map) => Ok(Expr::Builtin { name: Builtin::FromEntries, args: vec![Expr::Entries(target)] }),
              "fromEntries" => Ok(Expr::Builtin { name: Builtin::FromEntries, args: vec![*target] }),
              "keys" if self.kind_of(crate::syntax::arg_expr(a)) == Some(Kind::Proxy) => Ok(self.proxy_trap("ownKeys", *target, Vec::new(), |target, _| Expr::Keys(Box::new(target)))),
              "keys" => Ok(Expr::Keys(target)),
              "values" => Ok(Expr::Values(target)),
              _ => Err(self.residue(member.span(), format!("`Object.{method}`"))),
            };
          }
          "Math" if crate::JS_MATH.contains(&method.as_str()) => {
            return self.builtin_call(Builtin::Math, vec![Expr::lit_str(method.as_str())], call);
          }
          "Number" if method == "parseInt" => {
            let Some(first) = call.arguments.first() else { return Err(self.residue(call.span, "`Number.parseInt` takes a string and an optional radix")) };
            let mut args = vec![self.expr(crate::syntax::arg_expr(first))?];
            if let Some(radix) = call.arguments.get(1) {
              args.push(self.expr(crate::syntax::arg_expr(radix))?);
            }
            return Ok(Expr::Builtin { name: Builtin::ParseInt, args });
          }
          "Number" if method == "parseFloat" => {
            let Some(first) = call.arguments.first() else { return Err(self.residue(call.span, "`Number.parseFloat` takes a string")) };
            return Ok(Expr::Builtin { name: Builtin::ParseFloat, args: vec![self.expr(crate::syntax::arg_expr(first))?] });
          }
          "Number" if matches!(method.as_str(), "isInteger" | "isSafeInteger" | "isFinite" | "isNaN") => {
            let Some(arg) = call.arguments.first() else { return Err(self.residue(call.span, format!("`Number.{method}` takes one argument"))) };
            let x = self.expr(crate::syntax::arg_expr(arg))?;
            let both = |a: Expr, b: Expr| Expr::Logic(LogicOp::And, Box::new(a), Box::new(b));
            let compare = |op: CompareOp, a: Expr, b: Expr| Expr::Compare(op, Box::new(a), Box::new(b));
            let number = compare(CompareOp::Eq, Expr::Builtin { name: Builtin::TypeOf, args: vec![x.clone()] }, Expr::lit_str("number"));
            let finite = compare(CompareOp::Eq, Expr::Arith(ArithOp::Sub, Box::new(x.clone()), Box::new(x.clone())), Expr::Lit(Lit::Int(0)));
            let whole = compare(CompareOp::Eq, Expr::Builtin { name: Builtin::Trunc, args: vec![x.clone()] }, x.clone());
            return Ok(match method.as_str() {
              "isNaN" => both(number, Expr::Builtin { name: Builtin::IsNaN, args: vec![x] }),
              "isFinite" => both(number, finite),
              "isInteger" => both(number, both(finite, whole)),
              _ => both(number, both(finite, both(whole, compare(CompareOp::Le, Expr::Builtin { name: Builtin::Abs, args: vec![x] }, Expr::Lit(Lit::Int(9_007_199_254_740_991)))))),
            });
          }
          "Math" if method == "random" && (self.on_render_path() || self.module_level) && !self.reaches(Reach::Terminal) => {
            return Err(self.browser_only(call.span, "`Math.random()` on a render path or at module level, which the server and the browser answer differently"));
          }
          "Math" if method == "random" => {
            let bytes = Expr::Ext { module: "crypto".to_owned(), name: "random".to_owned(), args: vec![Expr::Lit(Lit::Float(6.0))] };
            let whole = Expr::Builtin { name: Builtin::ParseInt, args: vec![bytes, Expr::Lit(Lit::Float(16.0))] };
            return Ok(Expr::Arith(ArithOp::Div, Box::new(whole), Box::new(Expr::Lit(Lit::Float(281_474_976_710_656.0)))));
          }
          "Math" => {
            let name = match method.as_str() {
              "round" => Builtin::Round,
              "floor" => Builtin::Floor,
              "ceil" => Builtin::Ceil,
              "abs" => Builtin::Abs,
              "min" => Builtin::Min,
              "max" => Builtin::Max,
              "pow" => Builtin::Pow,
              "sqrt" => Builtin::Sqrt,
              "trunc" => Builtin::Trunc,
              "sign" => Builtin::Sign,
              _ => return Err(self.residue(member.span(), format!("`Math.{method}`"))),
            };
            if matches!(name, Builtin::Min | Builtin::Max) && call.arguments.iter().any(|a| crate::syntax::is_spread(a)) {
              let mut entries = Vec::with_capacity(call.arguments.len());
              for a in &call.arguments {
                let value = self.expr(crate::syntax::arg_expr(a))?;
                entries.push(if crate::syntax::is_spread(a) { Entry::Spread(value) } else { Entry::Item(value) });
              }
              let name = if name == Builtin::Min { Builtin::MinOf } else { Builtin::MaxOf };
              return Ok(Expr::Builtin { name, args: vec![Expr::Array(entries)] });
            }
            return self.builtin_call(name, Vec::new(), call);
          }
          "Response" if matches!(method.as_str(), "json" | "redirect") => {
            let mut args = Vec::with_capacity(call.arguments.len());
            for a in &call.arguments {
              args.push(self.expr(crate::syntax::arg_expr(a))?);
            }
            let name = if method == "json" { Builtin::ResponseJson } else { Builtin::ResponseRedirect };
            return Ok(Expr::Builtin { name, args });
          }
          "JSON" if method == "parse" => {
            let [arg] = call.arguments.as_slice() else { return Err(self.residue(call.span, "`JSON.parse` takes the text alone; a reviver is not lowered")) };
            return Ok(Expr::Builtin { name: Builtin::JsonParse, args: vec![self.expr(crate::syntax::arg_expr(arg))?] });
          }
          "JSON" if method == "stringify" => {
            let mut args = self.plain_args(call)?;
            if args.len() > 3 {
              return Err(self.residue(call.span, "`JSON.stringify` takes a value, a replacer and an indent"));
            }
            let plain = call.arguments.get(1).is_none_or(|a| matches!(strip_parens(crate::syntax::arg_expr(a)), js::Expression::NullLiteral(_)) || matches!(strip_parens(crate::syntax::arg_expr(a)), js::Expression::Identifier(id) if id.name.as_str() == "undefined"));
            if !plain {
              let replacer = args.remove(1);
              let value = args.remove(0);
              let replaced = self.replaced(value, replacer);
              args.insert(0, replaced);
              args.insert(1, Expr::Lit(Lit::Null));
            }
            return Ok(Expr::Builtin { name: Builtin::Json, args });
          }
          "Array" if method == "isArray" => {
            let [arg] = call.arguments.as_slice() else { return Err(self.residue(call.span, "`Array.isArray` takes one argument")) };
            return Ok(Expr::Builtin { name: Builtin::IsArray, args: vec![self.expr(crate::syntax::arg_expr(arg))?] });
          }
          "Array" if method == "from" && call.arguments.first().is_some_and(|a| !matches!(crate::syntax::arg_expr(a), js::Expression::ObjectExpression(_))) => {
            let source = &crate::syntax::arg_expr(&call.arguments[0]);
            let mut items = self.expr(source)?;
            items = match self.kind_of(source) {
              Some(Kind::Map) => Expr::Entries(Box::new(items)),
              _ => Expr::Array(vec![Entry::Spread(items)]),
            };
            return match call.arguments.get(1) {
              None => Ok(items),
              Some(f) => {
                let js::Expression::ArrowFunctionExpression(arrow) = &*crate::syntax::arg_expr(&f) else {
                  return Err(self.residue(crate::syntax::arg_expr(&f).span(), "`Array.from` takes an arrow function written in place"));
                };
                Ok(Expr::Map(Box::new(items), Box::new(self.lambda(arrow)?)))
              }
            };
          }
          "crypto" if method == "randomUUID" => {
            if self.on_render_path() && !self.reaches(Reach::Terminal) {
              return Err(self.browser_only(call.span, "`crypto.randomUUID()` on a render path, which gives the server and the browser different ids"));
            }
            return Ok(Expr::Ext { module: "crypto".to_owned(), name: "uuid".to_owned(), args: Vec::new() });
          }
          "performance" if method == "now" => {
            if self.on_render_path() && !self.reaches(Reach::Terminal) {
              return Err(self.browser_only(call.span, "the current time on a render path, which the server and the browser read at different moments"));
            }
            return Ok(Expr::Num(Box::new(Expr::Ext { module: "time".to_owned(), name: "monotonic".to_owned(), args: Vec::new() })));
          }
          "Date" => {
            return match method.as_str() {
              "now" => self.date_now(call.span),
              "UTC" => Ok(Expr::Builtin { name: Builtin::DateUtc, args: self.plain_args(call)? }),
              "parse" => {
                let arg = call.arguments.first().ok_or_else(|| self.residue(call.span, "`Date.parse` without a string"))?;
                Ok(Expr::Builtin { name: Builtin::DateMs, args: vec![self.expr(crate::syntax::arg_expr(arg))?] })
              }
              other => Err(self.residue(member.span(), format!("`Date.{other}`"))),
            };
          }
          "Array" if method == "from" => {
            let (Some(shape), Some(f)) = (call.arguments.first(), call.arguments.get(1)) else {
              return Err(self.residue(call.span, "`Array.from` takes `{ length }` and a function"));
            };
            let js::Expression::ObjectExpression(obj) = crate::syntax::arg_expr(shape) else {
              return Err(self.residue(crate::syntax::arg_expr(shape).span(), "`Array.from` over something other than `{ length: n }`"));
            };
            let length = obj.properties.iter().find_map(|p| match p {
              js::ObjectPropertyKind::ObjectProperty(p) => match crate::syntax::prop_of(p) {
                crate::syntax::Prop::KeyValue(kv) if prop_name(&kv.key, kv.computed).as_deref() == Some("length") => Some(&kv.value),
                _ => None,
              },
              _ => None,
            });
            let Some(length) = length else { return Err(self.residue(crate::syntax::arg_expr(shape).span(), "`Array.from` over something other than `{ length: n }`")) };
            let range = Box::new(Expr::Builtin { name: Builtin::Range, args: vec![self.expr(length)?] });
            let js::Expression::ArrowFunctionExpression(arrow) = &*crate::syntax::arg_expr(&f) else {
              return Err(self.residue(crate::syntax::arg_expr(&f).span(), "`Array.from` takes an arrow function written in place"));
            };
            return Ok(Expr::Map(range, Box::new(self.lambda(arrow)?)));
          }
          _ => {}
        }
      }
    }

    if method == "isPrototypeOf" {
      let [value] = call.arguments.as_slice() else { return Err(self.residue(call.span, "`isPrototypeOf` takes one value")) };
      let value = crate::syntax::arg_expr(value);
      if let Some(prototype) = member.object().as_member_expression().filter(|p| self.member_name(p).as_deref() == Some("prototype")) {
        return self.prototype_of(prototype.object(), value, call.span);
      }
      let held = self.expr(value)?;
      let name = self.temp();
      return Ok(Expr::Block(vec![Stmt::Let { name, expr: held }, Stmt::Return(Expr::Lit(Lit::Bool(false)))]));
    }

    if let Some((service, _via_ctx)) = self.service_of(member.object()) {
      let args = self.object_args(call, "service")?;
      return Ok(Expr::Call { service, method, args });
    }

    if let Some((module, _via_ctx)) = self.native_of(member.object()) {
      let args = self.object_args(call, "native")?;
      // `sync` is set by the build from the Rust signature, which this reader cannot see.
      return Ok(Expr::NativeCall { module, method, args, sync: false });
    }

    if let Some(type_name) = crate::temporal_type(member.object()) {
      let args = [vec![Expr::lit_str(type_name), Expr::lit_str(&method)], self.plain_args(call)?].concat();
      return Ok(Expr::Ext { module: "temporal".to_owned(), name: "static".to_owned(), args });
    }
    if crate::is_temporal_now(member.object()) {
      if self.on_render_path() && !self.reaches(Reach::Terminal) {
        return Err(self.browser_only(call.span, format!("`Temporal.Now.{method}()`, the current time on a render path, which the server and the browser read at different moments")));
      }
      let args = [vec![Expr::lit_str(&method)], self.plain_args(call)?].concat();
      return Ok(Expr::Ext { module: "temporal".to_owned(), name: "now".to_owned(), args });
    }
    if self.kind_of(member.object()) == Some(Kind::Temporal) {
      let target = self.expr(member.object())?;
      let args = [vec![target, Expr::lit_str(&method)], self.plain_args(call)?].concat();
      return Ok(Expr::Ext { module: "temporal".to_owned(), name: "call".to_owned(), args });
    }
    if method == "fill" {
      if let Some(length) = self.array_of_length(member.object()) {
        let length = self.expr(length)?;
        let range = Box::new(Expr::Builtin { name: Builtin::Range, args: vec![length] });
        if call.arguments.len() > 1 {
          let holes = Expr::Map(range, Box::new(Expr::Lambda { params: vec!["$filled".to_owned()], body: Box::new(Expr::Lit(Lit::Null)) }));
          return Ok(Expr::Builtin { name: Builtin::Fill, args: [vec![holes], self.plain_args(call)?].concat() });
        }
        let value = match call.arguments.first() {
          Some(arg) => self.expr(crate::syntax::arg_expr(arg))?,
          None => Expr::Lit(Lit::Null),
        };
        return Ok(Expr::Map(range, Box::new(Expr::Lambda { params: vec!["$filled".to_owned()], body: Box::new(value) })));
      }
    }
    if method == "segment" || method == "containing" {
      let (segmenter, text) = match (method.as_str(), strip_parens(member.object())) {
        ("segment", obj) => (self.formatter_of(obj), call.arguments.first().map(crate::syntax::arg_expr)),
        ("containing", js::Expression::CallExpression(inner)) => match strip_parens(&inner.callee).as_member_expression() {
          Some(m) if matches!(crate::syntax::prop(m), crate::syntax::MemberProp::Ident(p) if p.name.as_str() == "segment") => (self.formatter_of(m.object()), inner.arguments.first().map(crate::syntax::arg_expr)),
          _ => (None, None),
        },
        _ => (None, None),
      };
      if let Some(made) = segmenter.filter(|made| platform_object(made) == Some("Segmenter")) {
        self.grapheme_segmenter(&made)?;
        if !self.reaches(Reach::Terminal) || !self.splits_graphemes(&made) {
          let text = text.ok_or_else(|| self.residue(call.span, "`segment` without a string"))?;
          let granularity = made.arguments.get(1).and_then(|options| match crate::syntax::arg_expr(options) {
            js::Expression::ObjectExpression(obj) => obj.properties.iter().find_map(|prop| match prop {
              js::ObjectPropertyKind::ObjectProperty(prop) => match crate::syntax::prop_of(prop) {
                crate::syntax::Prop::KeyValue(kv) if prop_name(&kv.key, kv.computed).as_deref() == Some("granularity") => Some(&kv.value),
                _ => None,
              },
              _ => None,
            }),
            _ => None,
          });
          let granularity = match granularity {
            Some(g) => Expr::Str(Box::new(self.expr(g)?)),
            None => Expr::lit_str("grapheme"),
          };
          let mut args = vec![Expr::Str(Box::new(self.expr(text)?)), granularity];
          if method == "containing" {
            let at = call.arguments.first().ok_or_else(|| self.residue(call.span, "`containing` without an index"))?;
            args.push(self.expr(crate::syntax::arg_expr(at))?);
          }
          return Ok(Expr::Builtin { name: Builtin::Segments, args });
        }
        let text = text.ok_or_else(|| self.residue(call.span, "`segment` without a string"))?;
        let mut args = vec![self.expr(text)?];
        if method == "containing" {
          let at = call.arguments.first().ok_or_else(|| self.residue(call.span, "`containing` without an index"))?;
          args.push(self.expr(crate::syntax::arg_expr(at))?);
        }
        return Ok(Expr::Builtin { name: Builtin::Graphemes, args });
      }
    }
    if matches!(method.as_str(), "encode" | "decode") {
      if let Some(made) = self.formatter_of(member.object()) {
        match (platform_object(&made), method.as_str()) {
          (Some("TextEncoder"), "encode") | (Some("TextDecoder"), "decode") => {
            if method == "decode" {
              let args = &made.arguments[..];
              let utf8 = match args.first().map(|a| strip_parens(crate::syntax::arg_expr(a))) {
                None => true,
                Some(js::Expression::StringLiteral(s)) => matches!(crate::lossy(s.value).to_ascii_lowercase().as_str(), "utf-8" | "utf8" | "unicode-1-1-utf-8"),
                Some(_) => false,
              };
              if !utf8 || args.len() > 1 {
                return Err(self.residue_with(made.span, "a `TextDecoder` with a label or options", "`new TextDecoder()` decodes UTF-8, which the build reads"));
              }
            }
            let value = match call.arguments.first() {
              Some(a) => self.expr(crate::syntax::arg_expr(a))?,
              None => Expr::lit_str(""),
            };
            return Ok(Expr::Ext { module: "encoding".to_owned(), name: method.clone(), args: vec![value] });
          }
          _ => {}
        }
      }
    }
    if matches!(method.as_str(), "formatToParts" | "resolvedOptions") {
      if let Some(made) = self.formatter_of(member.object()) {
        match (platform_object(&made), method.as_str()) {
          (Some(formatter @ ("NumberFormat" | "DateTimeFormat" | "ListFormat" | "RelativeTimeFormat")), "formatToParts") => return self.format_to_parts(formatter, &made, call),
          (Some(formatter @ ("NumberFormat" | "DateTimeFormat" | "Collator" | "ListFormat" | "Segmenter" | "DisplayNames" | "RelativeTimeFormat" | "PluralRules" | "DurationFormat")), "resolvedOptions") => return self.resolved_options(formatter, &made),
          _ => {}
        }
      }
    }
    if matches!(method.as_str(), "format" | "select" | "compare" | "of") {
      if let Some(made) = self.formatter_of(member.object()) {
        let called = match (platform_object(&made), method.as_str()) {
          (Some(formatter @ ("NumberFormat" | "DateTimeFormat" | "ListFormat" | "RelativeTimeFormat" | "DurationFormat")), "format") | (Some(formatter @ "PluralRules"), "select") | (Some(formatter @ "Collator"), "compare") | (Some(formatter @ "DisplayNames"), "of") => Some(formatter),
          _ => None,
        };
        if let Some(formatter) = called {
          let mut values = Vec::with_capacity(call.arguments.len());
          for a in &call.arguments {
            values.push(self.expr(crate::syntax::arg_expr(a))?);
          }
          let expr = self.intl_call(formatter, &made.arguments, values, made.span)?;
          return Ok(self.candidate(call.span, expr));
        }
      }
    }
    if matches!(method.as_str(), "toArray" | "take" | "drop") && (iterator_made(member.object()) || self.kind_of(member.object()) == Some(Kind::Iterator)) {
      let target = self.expr(member.object())?;
      let mut args = self.plain_args(call)?.into_iter();
      return Ok(match method.as_str() {
        "toArray" => Expr::Array(vec![Entry::Spread(target)]),
        "take" => Expr::Builtin { name: Builtin::Slice, args: vec![target, Expr::Lit(Lit::Int(0)), Expr::Num(Box::new(args.next().unwrap_or(Expr::Lit(Lit::Undefined))))] },
        _ => Expr::Builtin { name: Builtin::Slice, args: vec![target, Expr::Num(Box::new(args.next().unwrap_or(Expr::Lit(Lit::Undefined))))] },
      });
    }
    if self.kind_of(member.object()) == Some(Kind::Buffer) && method == "slice" {
      let mut args = vec![self.expr(member.object())?];
      args.extend(self.plain_args(call)?);
      return Ok(crate::opaque_object(Expr::Builtin { name: Builtin::Slice, args }));
    }
    if self.kind_of(member.object()) == Some(Kind::View) && method.starts_with("get") {
      let target = self.expr(member.object())?;
      let mut args = vec![target, Expr::lit_str(&method[3..])];
      args.extend(self.plain_args(call)?);
      return Ok(Expr::Builtin { name: Builtin::ViewGet, args });
    }
    if self.kind_of(member.object()) == Some(Kind::Locale) {
      let target = self.expr(member.object())?;
      let tag = Expr::Field(Box::new(target), "$tag".to_owned());
      return match method.as_str() {
        "toString" | "toJSON" => Ok(tag),
        "maximize" | "minimize" => {
          let changed = Expr::Ext { module: "intl".to_owned(), name: "locale".to_owned(), args: vec![tag, Expr::lit_str(&method)] };
          Ok(Expr::Ext { module: "intl".to_owned(), name: "locale".to_owned(), args: vec![changed, Expr::lit_str("parse")] })
        }
        "getCalendars" | "getHourCycles" | "getNumberingSystems" | "getTextInfo" | "getTimeZones" | "getWeekInfo" => Ok(Expr::Ext { module: "intl".to_owned(), name: "locale".to_owned(), args: vec![tag, Expr::lit_str(&method)] }),
        other => Err(self.residue(member.span(), format!("`.{other}()` of an `Intl.Locale`; `maximize`, `minimize`, `toString`, `getCalendars`, `getHourCycles`, `getNumberingSystems`, `getTextInfo`, `getTimeZones` and `getWeekInfo` are read"))),
      };
    }
    if self.kind_of(member.object()) == Some(Kind::WeakRef) && method == "deref" {
      let target = self.expr(member.object())?;
      return Ok(Expr::Field(Box::new(target), "$weakRef".to_owned()));
    }
    if self.kind_of(member.object()) == Some(Kind::Registry) && matches!(method.as_str(), "register" | "unregister") {
      let mut stmts = Vec::new();
      for a in &call.arguments {
        stmts.push(Stmt::Expr(self.expr(crate::syntax::arg_expr(a))?));
      }
      stmts.push(Stmt::Return(if method == "register" { Expr::Lit(Lit::Undefined) } else { Expr::Lit(Lit::Bool(false)) }));
      return Ok(Expr::Block(stmts));
    }
    if self.kind_of(member.object()).is_some_and(Kind::weak) && !matches!(method.as_str(), "get" | "has" | "set" | "add" | "delete" | "getOrInsert" | "getOrInsertComputed") {
      return Err(self.residue(member.span(), format!("`.{method}()` of a `{}`, which has no such method", self.kind_of(member.object()).map_or("", Kind::name))));
    }
    match self.kind_of(member.object()).map(Kind::collection) {
      Some(Kind::Map) => {
        let target = self.expr(member.object())?;
        let key = |this: &mut Self| -> Lowered<Expr> {
          let arg = call.arguments.first().ok_or_else(|| this.residue(call.span, format!("`{method}` without a key")))?;
          Ok(Expr::Builtin { name: Builtin::MapKey, args: vec![this.expr(crate::syntax::arg_expr(arg))?] })
        };
        return match method.as_str() {
          "get" => Ok(Expr::Index(Box::new(target), Box::new(key(self)?))),
          "has" => Ok(Expr::Builtin { name: Builtin::HasKey, args: vec![target, key(self)?] }),
          "keys" => Ok(Expr::Keys(Box::new(target))),
          "values" => Ok(Expr::Values(Box::new(target))),
          "entries" => Ok(Expr::Entries(Box::new(target))),
          other => Err(self.residue(member.span(), format!("`.{other}()` of a `Map`"))),
        };
      }
      Some(Kind::Set) => {
        let target = self.expr(member.object())?;
        return match method.as_str() {
          "has" => {
            let arg = call.arguments.first().ok_or_else(|| self.residue(call.span, "`has` without a value"))?;
            Ok(Expr::Builtin { name: Builtin::Includes, args: vec![target, self.expr(crate::syntax::arg_expr(arg))?] })
          }
          "values" | "keys" => Ok(target),
          "entries" => {
            let item = self.temp();
            let pair = Expr::Array(vec![Entry::Item(Expr::Var(item.clone())), Entry::Item(Expr::Var(item.clone()))]);
            Ok(Expr::Map(Box::new(target), Box::new(Expr::Lambda { params: vec![item], body: Box::new(pair) })))
          }
          "union" | "intersection" | "difference" | "symmetricDifference" | "isSubsetOf" | "isSupersetOf" | "isDisjointFrom" => {
            let [arg] = call.arguments.as_slice() else { return Err(self.residue(call.span, format!("`{method}` takes one set"))) };
            let given = crate::syntax::arg_expr(arg);
            let other = match self.kind_of(given) {
              Some(Kind::Set) => self.expr(given)?,
              Some(Kind::Map) => Expr::Keys(Box::new(self.expr(given)?)),
              _ => return Err(self.residue(given.span(), format!("`{method}` of something the build cannot tell is a `Set` or a `Map`"))),
            };
            Ok(self.set_method(&method, target, other))
          }
          other => Err(self.residue(member.span(), format!("`.{other}()` of a `Set`"))),
        };
      }
      Some(Kind::Query) if method != "toString" => {
        let target = self.expr(member.object())?;
        return match method.as_str() {
          "get" => {
            let arg = call.arguments.first().ok_or_else(|| self.residue(call.span, "`get` without a key"))?;
            Ok(Expr::Builtin { name: Builtin::FormGet, args: vec![target, Expr::Str(Box::new(self.expr(crate::syntax::arg_expr(arg))?))] })
          }
          "has" | "getAll" => {
            let arg = call.arguments.first().ok_or_else(|| self.residue(call.span, format!("`{method}` without a key")))?;
            let name = if method == "has" { Builtin::FormHas } else { Builtin::FormGetAll };
            Ok(Expr::Builtin { name, args: vec![target, Expr::Str(Box::new(self.expr(crate::syntax::arg_expr(arg))?))] })
          }
          "entries" => Ok(Expr::Builtin { name: Builtin::FormEntries, args: vec![target] }),
          "keys" | "values" => {
            let pair = self.temp();
            let at = if method == "keys" { 0.0 } else { 1.0 };
            Ok(Expr::Map(Box::new(Expr::Builtin { name: Builtin::FormEntries, args: vec![target] }), Box::new(Expr::Lambda { params: vec![pair.clone()], body: Box::new(Expr::Index(Box::new(Expr::Var(pair)), Box::new(Expr::Lit(Lit::Float(at))))) })))
          }
          "set" | "append" | "delete" | "sort" => Err(self.residue(member.span(), format!("the value `.{method}()` of a `URLSearchParams` answers; it is lowered as a statement of its own"))),
          other => Err(self.residue(member.span(), format!("`.{other}()` of a `URLSearchParams`"))),
        };
      }
      _ => {}
    }

    if matches!(method.as_str(), "call" | "apply" | "bind") {
      if let Some(f) = self.function_value(member.object())? {
        return self.function_method(&method, f, call);
      }
    }
    let target = Box::new(self.unwrapped(member.object())?);
    let lambda = |this: &mut Self, i: usize| -> Lowered<Box<Expr>> {
      let a = call.arguments.get(i).ok_or_else(|| this.residue(call.span, format!("`{method}` takes a function")))?;
      if let js::Expression::Identifier(id) = crate::syntax::arg_expr(a) {
        let name = id.name.as_str();
        let unbound = !this.scope.iter().any(|(n, _)| n == name) && this.root_of(id).is_none() && !this.globals.iter().any(|(n, _)| n == name);
        if unbound && matches!(name, "Boolean" | "String" | "Number" | "BigInt" | "parseFloat" | "encodeURIComponent" | "decodeURIComponent") {
          let item = this.temp();
          let read = Box::new(Expr::Var(item.clone()));
          let body = match name {
            "Boolean" => Expr::Not(Box::new(Expr::Not(read))),
            "String" => Expr::Str(read),
            "Number" => Expr::Num(read),
            "BigInt" => Expr::BigInt(read),
            "parseFloat" => Expr::Builtin { name: Builtin::ParseFloat, args: vec![*read] },
            "encodeURIComponent" => Expr::Builtin { name: Builtin::EncodeUriComponent, args: vec![*read] },
            _ => Expr::Builtin { name: Builtin::DecodeUriComponent, args: vec![*read] },
          };
          return Ok(Box::new(Expr::Lambda { params: vec![item], body: Box::new(body) }));
        }
        if unbound && name == "parseInt" {
          let (item, index) = (this.temp(), this.temp());
          let body = Expr::Builtin { name: Builtin::ParseInt, args: vec![Expr::Var(item.clone()), Expr::Var(index.clone())] };
          return Ok(Box::new(Expr::Lambda { params: vec![item, index], body: Box::new(body) }));
        }
        if let Some(function) = this.local_fns.iter().rev().find(|(n, _)| n == name).map(|(_, f)| *f) {
          return this.function_expr(&function).map(Box::new);
        }
        let bound = this.scope.iter().rev().find(|(n, _)| n == name).or_else(|| this.globals.iter().rev().find(|(n, _)| n == name)).map(|(_, e)| e.clone());
        match &bound {
          Some(Expr::Lambda { .. } | Expr::Fix { .. }) => return this.expr(crate::syntax::arg_expr(a)).map(Box::new),
          Some(Expr::Var(v)) if v == name => return this.expr(crate::syntax::arg_expr(a)).map(Box::new),
          Some(_) => {}
          None if this.root_of(id).is_none() => {
            this.unbound = Some(name.to_owned());
            return Err(this.residue(crate::syntax::arg_expr(a).span(), format!("`{method}` takes a function; `{name}` is not bound here")));
          }
          None => {}
        }
      }
      if let Some(m) = (strip_parens(crate::syntax::arg_expr(a))).as_member_expression() {
        if let Some(compare) = this.collator_compare(m)? {
          return Ok(Box::new(compare));
        }
      }
      let js::Expression::ArrowFunctionExpression(arrow) = crate::syntax::arg_expr(a) else {
        return Err(this.residue(crate::syntax::arg_expr(a).span(), format!("`{method}` takes an arrow function written in place or the name of a function")));
      };
      let f = this.lambda(arrow)?;
      if let Expr::Lambda { body, .. } = &f {
        if matches!(&**body, Expr::Builtin { name: Builtin::WriteBack, .. }) {
          let written = crate::component::written_params(&crate::params_of(&arrow.params), match crate::syntax::arrow_body_of(arrow) {
            crate::syntax::ArrowBody::Block(b) => b.body,
            crate::syntax::ArrowBody::Expr(_) => &[],
          });
          let name = written.first().map(|(_, p)| p.clone()).unwrap_or_default();
          return Err(this.residue_with(arrow.span, format!("a write to a field of `{name}`, which this body did not declare: `{method}` would change the item in place"), "return a new value from the callback"));
        }
      }
      Ok(Box::new(f))
    };
    match method.as_str() {
      "map" => Ok(Expr::Map(target, lambda(self, 0)?)),
      "filter" => Ok(Expr::Filter(target, lambda(self, 0)?)),
      "find" => Ok(Expr::Find(target, lambda(self, 0)?)),
      "findIndex" => Ok(Expr::FindIndex(target, lambda(self, 0)?)),
      "some" => Ok(Expr::Some(target, lambda(self, 0)?)),
      "every" => Ok(Expr::Every(target, lambda(self, 0)?)),
      "flatMap" => Ok(Expr::FlatMap(target, lambda(self, 0)?)),
      "flat" if call.arguments.is_empty() || matches!(&*crate::syntax::arg_expr(&call.arguments[0]), js::Expression::NumericLiteral(n) if n.value == 1.0) => Ok(Expr::FlatMap(target, Box::new(Expr::Lambda { params: vec!["$item".to_owned()], body: Box::new(Expr::Var("$item".to_owned())) }))),
      "toString" if call.arguments.is_empty() && self.kind_of(member.object()) == Some(Kind::Date) => {
        if self.on_render_path() && !self.reaches(Reach::Terminal) {
          return Err(self.browser_only(member.span(), "`.toString()` of a `Date`, which reads the viewer's time zone"));
        }
        Ok(Expr::Ext { module: "intl".to_owned(), name: "dateTimeString".to_owned(), args: vec![*target] })
      }
      "toString" if call.arguments.is_empty() => Ok(Expr::Str(target)),
      "toString" => Ok(Expr::Builtin { name: Builtin::Radix, args: vec![*target, self.expr(&crate::syntax::arg_expr(&call.arguments[0]))?] }),
      "getTime" | "valueOf" if self.kind_of(member.object()) == Some(Kind::Date) => Ok(Expr::Num(target)),
      "getTime" | "valueOf" => Ok(*target),
      "toJSON" if self.kind_of(member.object()) == Some(Kind::Url) => Ok(*target),
      "toISOString" | "toJSON" => Ok(Expr::Builtin { name: Builtin::IsoString, args: vec![*target] }),
      "getUTCFullYear" | "getUTCMonth" | "getUTCDate" | "getUTCDay" | "getUTCHours" | "getUTCMinutes" | "getUTCSeconds" | "getUTCMilliseconds" | "getMilliseconds" => {
        let part = match method.as_str() {
          "getUTCFullYear" => "year",
          "getUTCMonth" => "month",
          "getUTCDate" => "date",
          "getUTCDay" => "day",
          "getUTCHours" => "hours",
          "getUTCMinutes" => "minutes",
          "getUTCSeconds" => "seconds",
          _ => "milliseconds",
        };
        Ok(Expr::Builtin { name: Builtin::DatePart, args: vec![*target, Expr::lit_str(part)] })
      }
      "getFullYear" | "getYear" | "getMonth" | "getDate" | "getDay" | "getHours" | "getMinutes" | "getSeconds" if (self.reaches(Reach::Terminal) || !self.on_render_path()) && call.arguments.is_empty() => {
        let part = match method.as_str() {
          "getFullYear" | "getYear" => "year",
          "getMonth" => "month",
          "getDate" => "date",
          "getDay" => "day",
          "getHours" => "hours",
          "getMinutes" => "minutes",
          _ => "seconds",
        };
        let at = self.temp();
        let offset = Expr::Ext { module: "time".to_owned(), name: "localOffset".to_owned(), args: vec![Expr::Var(at.clone())] };
        let local = Expr::Arith(ArithOp::Sub, Box::new(Expr::Var(at.clone())), Box::new(Expr::Arith(ArithOp::Mul, Box::new(offset), Box::new(Expr::Lit(Lit::Int(60_000))))));
        let mut body = Expr::Builtin { name: Builtin::DatePart, args: vec![local, Expr::lit_str(part)] };
        if method == "getYear" {
          body = Expr::Arith(ArithOp::Sub, Box::new(body), Box::new(Expr::Lit(Lit::Float(1900.0))));
        }
        Ok(Expr::Apply { f: Box::new(Expr::Lambda { params: vec![at], body: Box::new(body) }), args: vec![*target] })
      }
      "getTimezoneOffset" if self.reaches(Reach::Terminal) || !self.on_render_path() => Ok(Expr::Ext { module: "time".to_owned(), name: "localOffset".to_owned(), args: vec![*target] }),
      "toDateString" if (self.reaches(Reach::Terminal) || !self.on_render_path()) && call.arguments.is_empty() => Ok(self.local_date_string(*target)),
      "toTemporalInstant" if self.kind_of(member.object()) == Some(Kind::Date) => {
        let b = self.build();
        let ast = b.ast();
        let span = call.span;
        let name = |n: &str| js::IdentifierName::new(span, b.str(n), &ast);
        let instant = js::Expression::new_static_member_expression(span, b.ident("Temporal", span), name("Instant"), false, &ast);
        let from = js::Expression::new_static_member_expression(span, instant, name("fromEpochMilliseconds"), false, &ast);
        let time = js::Expression::new_static_member_expression(span, b.copy(member.object()), name("getTime"), false, &ast);
        let ms = js::Expression::new_call_expression(span, time, None, b.vec([]), false, &ast);
        let made = js::Expression::new_call_expression(span, from, None, b.vec([js::Argument::from(ms)]), false, &ast);
        self.expr(b.alloc(made))
      }
      "toTimeString" if (self.reaches(Reach::Terminal) || !self.on_render_path()) && call.arguments.is_empty() => Ok(Expr::Ext { module: "intl".to_owned(), name: "timeString".to_owned(), args: vec![*target] }),
      "toLocaleTimeString" if call.arguments.len() == 2 && self.kind_of(member.object()) == Some(Kind::Date) => {
        if let Some(expr) = self.date_in_zone(&call.arguments, (*target).clone(), "time")? {
          return Ok(self.candidate(call.span, expr));
        }
        let expr = self.intl_format("DateTimeFormat", &call.arguments, *target, call.span)?;
        Ok(self.candidate(call.span, expr))
      }
      "toLocaleTimeString" if (self.reaches(Reach::Terminal) || !self.on_render_path()) && call.arguments.len() <= 1 => {
        let locale = match call.arguments.first().map(|a| strip_parens(crate::syntax::arg_expr(a))) {
          None => Expr::Lit(Lit::Null),
          Some(js::Expression::Identifier(id)) if id.name.as_str() == "undefined" => Expr::Lit(Lit::Null),
          Some(js::Expression::StringLiteral(tag)) => Expr::lit_str(crate::lossy(tag.value)),
          Some(other) => self.expr(other)?,
        };
        Ok(Expr::Ext { module: "intl".to_owned(), name: "localTime".to_owned(), args: vec![*target, locale] })
      }
      "setFullYear" | "setYear" | "setMonth" | "setDate" | "setHours" | "setMinutes" | "setSeconds" | "setMilliseconds" if self.kind_of(member.object()) == Some(Kind::Date) && self.on_render_path() && !self.reaches(Reach::Terminal) => {
        Err(self.browser_only(member.span(), format!("`.{method}()`, which reads the viewer's time zone")))
      }
      "getFullYear" | "getYear" | "getMonth" | "getDate" | "getDay" | "getHours" | "getMinutes" | "getSeconds" | "getTimezoneOffset" | "toLocaleTimeString" | "toTimeString" | "toDateString" => {
        let utc = method.replacen("get", "getUTC", 1);
        let mut residue = self.browser_only(member.span(), format!("`.{method}()`, which reads the viewer's time zone"));
        residue.hint = Some(format!("as text under an element the browser fills it in when the component hydrates; the server has no viewer, so `.{utc}()` and `toLocaleDateString(undefined, {{ dateStyle }})` read UTC, which both agree on"));
        Err(residue)
      }
      "anchor" | "big" | "blink" | "bold" | "fixed" | "fontcolor" | "fontsize" | "italics" | "link" | "small" | "strike" | "sub" | "sup" => {
        let (tag, attribute) = match method.as_str() {
          "anchor" => ("a", Some("name")),
          "bold" => ("b", None),
          "fixed" => ("tt", None),
          "fontcolor" => ("font", Some("color")),
          "fontsize" => ("font", Some("size")),
          "italics" => ("i", None),
          "link" => ("a", Some("href")),
          other => (other, None),
        };
        let mut parts = Vec::new();
        match attribute {
          Some(attribute) => {
            let value = match call.arguments.first() {
              Some(a) => Expr::Str(Box::new(self.expr(crate::syntax::arg_expr(a))?)),
              None => Expr::lit_str("undefined"),
            };
            parts.push(Expr::lit_str(format!("<{tag} {attribute}=\"")));
            parts.push(Expr::Builtin { name: Builtin::ReplaceAll, args: vec![value, Expr::lit_str("\""), Expr::lit_str("&quot;")] });
            parts.push(Expr::lit_str("\">"));
          }
          None => parts.push(Expr::lit_str(format!("<{tag}>"))),
        }
        parts.push(Expr::Str(target));
        parts.push(Expr::lit_str(format!("</{tag}>")));
        Ok(Expr::Template(parts))
      }
      "hasOwnProperty" | "propertyIsEnumerable" => {
        let key = match call.arguments.first() {
          Some(a) => Expr::Str(Box::new(self.expr(crate::syntax::arg_expr(a))?)),
          None => Expr::lit_str("undefined"),
        };
        let (object, name) = (self.temp(), self.temp());
        let var = |n: &String| Expr::Var(n.clone());
        let own = Expr::Builtin { name: Builtin::HasKey, args: vec![var(&object), var(&name)] };
        let answer = if method == "hasOwnProperty" {
          own
        } else {
          let length = Expr::Compare(CompareOp::Eq, Box::new(var(&name)), Box::new(Expr::lit_str("length")));
          let string = Expr::Compare(CompareOp::Eq, Box::new(Expr::Builtin { name: Builtin::TypeOf, args: vec![var(&object)] }), Box::new(Expr::lit_str("string")));
          let indexed = Expr::Logic(LogicOp::Or, Box::new(Expr::Builtin { name: Builtin::IsArray, args: vec![var(&object)] }), Box::new(string));
          Expr::Logic(LogicOp::And, Box::new(own), Box::new(Expr::Not(Box::new(Expr::Logic(LogicOp::And, Box::new(length), Box::new(indexed))))))
        };
        Ok(Expr::Block(vec![Stmt::Let { name: object, expr: *target }, Stmt::Let { name, expr: key }, Stmt::Return(answer)]))
      }
      "toLocaleUpperCase" | "toLocaleLowerCase" => {
        let name = if method == "toLocaleUpperCase" { "upper" } else { "lower" };
        let server_default = self.reaches(Reach::Terminal) || !self.on_render_path();
        let intl = |name: &str, args: Vec<Expr>| Expr::Ext { module: "intl".to_owned(), name: name.to_owned(), args };
        let given = call.arguments.first().map(|a| strip_parens(crate::syntax::arg_expr(a)));
        let args = match given.map(|given| (written_locale(given), given)) {
          Some((Some(Some(tag)), _)) => vec![*target, Expr::lit_str(tag)],
          None | Some((Some(None), _)) if server_default => vec![*target, intl("systemLocale", Vec::new())],
          None | Some((Some(None), _)) => return Err(self.browser_only(member.span(), format!("`.{method}()`, which reads the viewer's locale"))),
          Some((None, computed)) => {
            let locales = self.expr(computed)?;
            let fallback = if server_default { intl("systemLocale", Vec::new()) } else { intl("localeTag", Vec::new()) };
            vec![*target, locales, fallback]
          }
        };
        Ok(intl(name, args))
      }
      "localeCompare" => {
        let other = match call.arguments.first() {
          Some(arg) => self.expr(crate::syntax::arg_expr(arg))?,
          None => Expr::lit_str("undefined"),
        };
        self.intl_call("Collator", call.arguments.get(1..).unwrap_or_default(), vec![*target, other], call.span)
      }
      "toLocaleDateString" => {
        let made = &call.arguments[..call.arguments.len().min(2)];
        let expr = self.intl_format("DateTimeFormat", made, *target, call.span)?;
        Ok(self.candidate(call.span, expr))
      }
      "toSorted" | "sort" | "toReversed" | "reverse" => {
        if matches!(method.as_str(), "sort" | "reverse") && !fresh_array(member.object()) {
          return Err(self.residue_with(member.span(), format!("`.{method}()` of an array something else holds, which it rewrites in place"), format!("`[...items].{method}()` or `items.slice().{method}()` gives the same order without the write")));
        }
        if method.contains("everse") {
          return Ok(Expr::Builtin { name: Builtin::Reverse, args: vec![*target] });
        }
        let f = match call.arguments.first() {
          None => Box::new(Expr::Lit(Lit::Null)),
          Some(_) => lambda(self, 0)?,
        };
        Ok(Expr::Sort(target, f))
      }
      "slice" | "at" | "indexOf" | "lastIndexOf" | "charAt" | "charCodeAt" | "codePointAt" | "concat" | "padStart" | "padEnd" | "substring" => {
        let name = match method.as_str() {
          "slice" => Builtin::Slice,
          "at" => Builtin::At,
          "indexOf" => Builtin::IndexOf,
          "lastIndexOf" => Builtin::LastIndexOf,
          "charAt" => Builtin::CharAt,
          "charCodeAt" => Builtin::CharCodeAt,
          "codePointAt" => Builtin::CodePointAt,
          "concat" => Builtin::Concat,
          "padStart" => Builtin::PadStart,
          "padEnd" => Builtin::PadEnd,
          _ => Builtin::Substring,
        };
        self.builtin_call(name, vec![*target], call)
      }
      "findLast" | "findLastIndex" | "reduceRight" => {
        let f = lambda(self, 0)?;
        let init = match call.arguments.get(1) {
          Some(a) if method == "reduceRight" => Some(self.expr(crate::syntax::arg_expr(a))?),
          _ => None,
        };
        Ok(self.from_the_end(&method, *target, f, init))
      }
      "substr" | "toPrecision" | "toExponential" | "normalize" => {
        let name = match method.as_str() {
          "substr" => Builtin::Substr,
          "toPrecision" => Builtin::ToPrecision,
          "toExponential" => Builtin::ToExponential,
          _ => Builtin::Normalize,
        };
        let mut args = vec![*target];
        args.extend(self.plain_args(call)?);
        Ok(Expr::Builtin { name, args })
      }
      "isWellFormed" | "toWellFormed" | "toUTCString" | "toGMTString" if call.arguments.is_empty() => {
        let name = match method.as_str() {
          "isWellFormed" => Builtin::IsWellFormed,
          "toWellFormed" => Builtin::ToWellFormed,
          _ => Builtin::UtcString,
        };
        Ok(Expr::Builtin { name, args: vec![*target] })
      }
      "exec" => {
        let [arg] = call.arguments.as_slice() else { return Err(self.residue(call.span, "`exec` takes the string to search")) };
        let subject = self.expr(crate::syntax::arg_expr(arg))?;
        if matches!(strip_parens(member.object()), js::Expression::RegExpLiteral(_)) {
          let ran = Expr::Builtin { name: Builtin::ExecFrom, args: vec![*target, subject] };
          return Ok(Expr::Index(Box::new(ran), Box::new(Expr::Lit(Lit::Float(0.0)))));
        }
        Ok(Expr::Builtin { name: Builtin::Exec, args: vec![*target, subject] })
      }
      "toSpliced" => Ok(Expr::Builtin { name: Builtin::ToSpliced, args: [vec![*target], self.spliced_args(call)?].concat() }),
      "with" if call.arguments.len() == 2 => {
        let mut args = vec![*target];
        args.extend(self.plain_args(call)?);
        Ok(Expr::Builtin { name: Builtin::ArrayWith, args })
      }
      "fill" | "copyWithin" => {
        if !fresh_array(member.object()) {
          return Err(self.residue_with(member.span(), format!("`.{method}()` of an array something else holds, which it rewrites in place"), format!("`[...items].{method}()` or `items.slice().{method}()` gives the same items without the write")));
        }
        let name = if method == "fill" { Builtin::Fill } else { Builtin::CopyWithin };
        let mut args = vec![*target];
        args.extend(self.plain_args(call)?);
        Ok(Expr::Builtin { name, args })
      }
      "entries" | "keys" | "values" if call.arguments.is_empty() => Ok(Expr::Builtin { name: Builtin::Iterate, args: vec![Expr::lit_str(method.as_str()), *target] }),
      "reduce" => {
        let rewritten = match (call.arguments.first().map(|a| strip_parens(crate::syntax::arg_expr(a))), call.arguments.get(1).map(|a| strip_parens(crate::syntax::arg_expr(a)))) {
          (Some(js::Expression::ArrowFunctionExpression(arrow)), Some(js::Expression::ObjectExpression(_))) => fresh_accumulator(self.build(), arrow),
          _ => None,
        };
        let f = match rewritten {
          Some(arrow) => Box::new(self.lambda(&arrow)?),
          None => lambda(self, 0)?,
        };
        let Some(init) = call.arguments.get(1) else {
          let items = self.temp();
          let whole = Expr::Var(items.clone());
          let f = match *f {
            Expr::Lambda { params, body } if params.len() > 2 => {
              let (index, array) = (self.temp(), self.temp());
              let mut binds = vec![Stmt::Let { name: params[2].clone(), expr: Expr::Arith(ArithOp::Add, Box::new(Expr::Var(index.clone())), Box::new(Expr::Lit(Lit::Int(1)))) }];
              if let Some(all) = params.get(3) {
                binds.push(Stmt::Let { name: all.clone(), expr: whole.clone() });
              }
              binds.push(Stmt::Return(*body));
              Box::new(Expr::Lambda { params: vec![params[0].clone(), params[1].clone(), index, array], body: Box::new(Expr::Block(binds)) })
            }
            f => Box::new(f),
          };
          let rest = Expr::Builtin { name: Builtin::Slice, args: vec![whole.clone(), Expr::Lit(Lit::Int(1))] };
          let reduced = Expr::Reduce(Box::new(rest), Box::new(Expr::Index(Box::new(whole.clone()), Box::new(Expr::Lit(Lit::Int(0))))), f);
          let empty = Expr::Builtin { name: Builtin::Throw, args: vec![Expr::lit_str("Reduce of empty array with no initial value")] };
          return Ok(Expr::Block(vec![Stmt::Let { name: items, expr: *target }, Stmt::Return(Expr::Ternary(Box::new(Expr::Length(Box::new(whole))), Box::new(reduced), Box::new(empty)))]));
        };
        let init = Box::new(self.expr(crate::syntax::arg_expr(init))?);
        Ok(Expr::Reduce(target, init, f))
      }
      "trimStart" | "trimEnd" | "trimLeft" | "trimRight" if call.arguments.is_empty() => {
        let side = if matches!(method.as_str(), "trimStart" | "trimLeft") { "start" } else { "end" };
        Ok(Expr::Builtin { name: Builtin::Trim, args: vec![*target, Expr::lit_str(side)] })
      }
      "toLocaleString" if self.kind_of(member.object()) == Some(Kind::Date) => {
        let made = &call.arguments[..call.arguments.len().min(2)];
        let options = made.get(1).is_some_and(|o| !matches!(strip_parens(crate::syntax::arg_expr(o)), js::Expression::Identifier(id) if id.name.as_str() == "undefined"));
        if options {
          if let Some(expr) = self.date_in_zone(made, (*target).clone(), "both")? {
            return Ok(self.candidate(call.span, expr));
          }
          let expr = self.intl_format("DateTimeFormat", made, *target, call.span)?;
          return Ok(self.candidate(call.span, expr));
        }
        if self.on_render_path() && !self.reaches(Reach::Terminal) {
          return Err(self.browser_only(member.span(), "`.toLocaleString()` of a `Date`, which reads the viewer's time zone and locale"));
        }
        let locale = match made.first().map(|a| strip_parens(crate::syntax::arg_expr(a))) {
          None => Expr::Lit(Lit::Null),
          Some(js::Expression::Identifier(id)) if id.name.as_str() == "undefined" => Expr::Lit(Lit::Null),
          Some(other) => self.expr(other)?,
        };
        Ok(Expr::Ext { module: "intl".to_owned(), name: "localDate".to_owned(), args: vec![*target, locale, Expr::Lit(Lit::Bool(true))] })
      }
      "toLocaleString" => {
        // An array formats each item with its own `toLocaleString` and joins them with commas; a number or a bigint is a number format; anything else is its text.
        let (held, item) = (self.temp(), self.temp());
        let numeric = |value: Expr| {
          let kind = Expr::Builtin { name: Builtin::TypeOf, args: vec![value] };
          Expr::Logic(LogicOp::Or, Box::new(Expr::Compare(CompareOp::Eq, Box::new(kind.clone()), Box::new(Expr::lit_str("number")))), Box::new(Expr::Compare(CompareOp::Eq, Box::new(kind), Box::new(Expr::lit_str("bigint")))))
        };
        let number = self.intl_format("NumberFormat", &call.arguments, Expr::Var(held.clone()), call.span)?;
        let item_number = self.intl_format("NumberFormat", &call.arguments, Expr::Var(item.clone()), call.span)?;
        let item_text = Expr::Ternary(Box::new(numeric(Expr::Var(item.clone()))), Box::new(item_number), Box::new(Expr::Ternary(Box::new(Expr::Builtin { name: Builtin::IsNullish, args: vec![Expr::Var(item.clone())] }), Box::new(Expr::lit_str("")), Box::new(Expr::Str(Box::new(Expr::Var(item.clone())))))));
        let items = Expr::Map(Box::new(Expr::Var(held.clone())), Box::new(Expr::Lambda { params: vec![item], body: Box::new(item_text) }));
        let joined = Expr::Builtin { name: Builtin::Join, args: vec![items, Expr::lit_str(",")] };
        let scalar = Expr::Ternary(Box::new(numeric(Expr::Var(held.clone()))), Box::new(number), Box::new(Expr::Str(Box::new(Expr::Var(held.clone())))));
        let scalar = if self.never_a_date(member.object()) {
          scalar
        } else {
          // A `Date` the build could not see is one: in a body it is written in this machine's zone, as node writes it; on a render path the viewer's zone is not the server's, so the server writes it in UTC.
          let ms = Expr::Builtin { name: Builtin::TaggedPayload, args: vec![Expr::Var(held.clone()), Expr::lit_str(snapfire_fsr_ir::render::typed::DATE)] };
          let locale = match call.arguments.first() {
            Some(a) => self.expr(crate::syntax::arg_expr(a))?,
            None => Expr::Lit(Lit::Undefined),
          };
          let date = if self.on_render_path() && !self.reaches(Reach::Terminal) {
            Expr::Ext { module: "intl".to_owned(), name: "numeric".to_owned(), args: vec![ms.clone(), locale, Expr::lit_str("both")] }
          } else {
            Expr::Ext { module: "intl".to_owned(), name: "localDate".to_owned(), args: vec![ms.clone(), locale, Expr::Lit(Lit::Bool(true))] }
          };
          Expr::Ternary(Box::new(Expr::Builtin { name: Builtin::IsNullish, args: vec![ms] }), Box::new(scalar), Box::new(date))
        };
        let formatted = Expr::Block(vec![Stmt::Let { name: held.clone(), expr: *target }, Stmt::Return(Expr::Ternary(Box::new(Expr::Builtin { name: Builtin::IsArray, args: vec![Expr::Var(held)] }), Box::new(joined), Box::new(scalar)))]);
        Ok(self.candidate(call.span, formatted))
      }
      "toFixed" | "repeat" | "join" | "trim" | "toUpperCase" | "toLowerCase" | "includes" => {
        let name = match method.as_str() {
          "toFixed" => Builtin::ToFixed,
          "repeat" => Builtin::Repeat,
          "join" => Builtin::Join,
          "trim" => Builtin::Trim,
          "toUpperCase" => Builtin::Upper,
          "toLowerCase" => Builtin::Lower,
          _ => Builtin::Includes,
        };
        let mut args = vec![*target];
        for a in &call.arguments {
          args.push(self.expr(crate::syntax::arg_expr(a))?);
        }
        Ok(Expr::Builtin { name, args })
      }
      "test" | "match" | "matchAll" | "search" => {
        let arg = call.arguments.first().ok_or_else(|| self.residue(call.span, format!("`{method}` without an argument")))?;
        let arg = self.expr(crate::syntax::arg_expr(arg))?;
        Ok(match method.as_str() {
          "test" => Expr::Builtin { name: Builtin::RegexTest, args: vec![*target, arg] },
          "match" => Expr::Builtin { name: Builtin::Match, args: vec![*target, arg] },
          "matchAll" => Expr::Builtin { name: Builtin::MatchAll, args: vec![*target, arg] },
          _ => Expr::Builtin { name: Builtin::Search, args: vec![*target, arg] },
        })
      }
      "replace" | "replaceAll" if call.arguments.len() == 2 && matches!(&*crate::syntax::arg_expr(&call.arguments[1]), js::Expression::ArrowFunctionExpression(_)) => {
        let js::Expression::ArrowFunctionExpression(arrow) = &*crate::syntax::arg_expr(&call.arguments[1]) else { unreachable!("checked") };
        let pattern = match (&*crate::syntax::arg_expr(&call.arguments[0]), method.as_str()) {
          (js::Expression::StringLiteral(s), "replaceAll") => regex_value(self, &escape_regex(crate::lossy(s.value).as_ref()), "g", call.span)?,
          (js::Expression::RegExpLiteral(re), "replaceAll") if !re.regex.flags.contains(js::RegExpFlags::G) => return Err(self.residue(call.span, "`replaceAll` of a regular expression without `g`, which JavaScript throws on")),
          (pattern, "replaceAll") if !matches!(pattern, js::Expression::RegExpLiteral(_)) => return Err(self.residue(call.span, "`replaceAll` of a pattern the build cannot read with a function")),
          (pattern, _) => self.expr(pattern)?,
        };
        let f = self.lambda(arrow)?;
        Ok(Expr::ReplaceWith(target, Box::new(pattern), Box::new(f)))
      }
      "replaceAll" => {
        let [from, to] = call.arguments.as_slice() else { return Err(self.residue(call.span, format!("`replaceAll` takes 2 arguments, got {}", call.arguments.len()))) };
        let to = self.expr(crate::syntax::arg_expr(to))?;
        match crate::syntax::arg_expr(from) {
          js::Expression::RegExpLiteral(re) => {
            if !re.regex.flags.contains(js::RegExpFlags::G) {
              return Err(self.residue(call.span, "`replaceAll` of a regular expression without `g`, which JavaScript throws on"));
            }
            let pattern = self.expr(crate::syntax::arg_expr(from))?;
            Ok(Expr::Builtin { name: Builtin::Replace, args: vec![*target, pattern, to] })
          }
          other => Ok(Expr::Builtin { name: Builtin::ReplaceAll, args: vec![*target, self.expr(other)?, to] }),
        }
      }
      "split" | "startsWith" | "endsWith" | "replace" => {
        let (name, arity) = match method.as_str() {
          "split" => (Builtin::Split, 1),
          "startsWith" => (Builtin::StartsWith, 1),
          "endsWith" => (Builtin::EndsWith, 1),
          _ => (Builtin::Replace, 2),
        };
        // The `position` that `startsWith` and `endsWith` take in JavaScript
        // would otherwise lower to an argument the interpreter ignores.
        let limited = name == Builtin::Split && call.arguments.len() == 2;
        if call.arguments.len() != arity && !limited {
          let plural = if arity == 1 { "" } else { "s" };
          return Err(self.residue(call.span, format!("`{method}` takes {arity} argument{plural}, got {}", call.arguments.len())));
        }
        let mut args = vec![*target];
        for a in &call.arguments {
          args.push(self.expr(crate::syntax::arg_expr(a))?);
        }
        Ok(Expr::Builtin { name, args })
      }
      other if self.kind_of(member.object()).is_some_and(crate::standard::uncalled) => {
        let kind = self.kind_of(member.object()).expect("checked");
        Err(self.residue(member.span(), format!("`.{other}()` of {}, which the build does not lower", kind.described())))
      }
      other if self.formatter_of(member.object()).is_some() => {
        let formatter = self.formatter_of(member.object()).and_then(|made| platform_object(&made)).unwrap_or("formatter");
        Err(self.residue(member.span(), format!("`.{other}()` of an `Intl.{formatter}`, which the build does not lower")))
      }
      other if !STANDARD_METHODS.contains(&other) => {
        let mut args = Vec::with_capacity(call.arguments.len());
        for a in &call.arguments {
          args.push(self.expr(crate::syntax::arg_expr(a))?);
        }
        Ok(Expr::Apply { f: Box::new(Expr::Field(target, other.to_owned())), args })
      }
      other => Err(self.residue_with(
        member.span(),
        format!("`.{other}()`, which is not a builtin"),
        "the builtins are `map`, `filter`, `find`, `findIndex`, `findLast`, `findLastIndex`, `some`, `every`, `reduce`, `reduceRight`, `flatMap`, `flat`, `toSorted`, `toReversed`, `toSpliced`, `with`, `entries`, `keys`, `values`, `slice`, `at`, `indexOf`, `lastIndexOf`, `charAt`, `charCodeAt`, `codePointAt`, `concat`, `join`, `includes`, `trim`, `repeat`, `toFixed`, `toPrecision`, `toExponential`, `toString`, `toUpperCase`, `toLowerCase`, `padStart`, `padEnd`, `substring`, `substr`, `normalize`, `isWellFormed`, `toWellFormed`, `split`, `startsWith`, `endsWith`, `replace`, `replaceAll`, `match`, `matchAll`, `search`, `test`, `exec`, `localeCompare`, `toLocaleString`, `toLocaleUpperCase`, `toLocaleLowerCase`, `trimStart`, `trimEnd`, `valueOf`, `getTime`, `toISOString`, `toUTCString` and `toJSON`, with `sort`, `reverse`, `fill` and `copyWithin` on a fresh array; anything else goes in a module-level helper the build can read",
      )),
    }
  }
}

impl Lowerer<'_> {
  /// `when.toDateString()` in this machine's time zone, `Thu Sep 12 2024`; `Invalid Date` for an invalid instant.
  fn local_date_string(&mut self, when: Expr) -> Expr {
    let (at, local) = (self.temp(), self.temp());
    let offset = Expr::Ext { module: "time".to_owned(), name: "localOffset".to_owned(), args: vec![Expr::Var(at.clone())] };
    let shifted = Expr::Arith(ArithOp::Sub, Box::new(Expr::Var(at.clone())), Box::new(Expr::Arith(ArithOp::Mul, Box::new(offset), Box::new(Expr::Lit(Lit::Float(60_000.0))))));
    let part = |name: &str| Expr::Builtin { name: Builtin::DatePart, args: vec![Expr::Var(local.clone()), Expr::lit_str(name)] };
    let names = |list: &[&str]| Expr::Array(list.iter().map(|n| Entry::Item(Expr::lit_str(*n))).collect());
    let padded = |e: Expr, width: f64| Expr::Builtin { name: Builtin::PadStart, args: vec![Expr::Str(Box::new(e)), Expr::Lit(Lit::Float(width)), Expr::lit_str("0")] };
    let text = Expr::Template(vec![
      Expr::Index(Box::new(names(&["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"])), Box::new(part("day"))),
      Expr::lit_str(" "),
      Expr::Index(Box::new(names(&["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"])), Box::new(part("month"))),
      Expr::lit_str(" "),
      padded(part("date"), 2.0),
      Expr::lit_str(" "),
      padded(part("year"), 4.0),
    ]);
    let invalid = Expr::Builtin { name: Builtin::IsNaN, args: vec![Expr::Var(at.clone())] };
    Expr::Block(vec![
      Stmt::Let { name: at.clone(), expr: when },
      Stmt::Return(Expr::Ternary(Box::new(invalid), Box::new(Expr::lit_str("Invalid Date")), Box::new(Expr::Block(vec![Stmt::Let { name: local, expr: shifted }, Stmt::Return(text)])))),
    ])
  }

  /// The arguments of `toSpliced` or `splice` after the array: the start, the count to remove and the items to put in as one array. No argument at all removes nothing.
  pub(crate) fn spliced_args(&mut self, call: &js::CallExpression) -> Lowered<Vec<Expr>> {
    let mut args = self.plain_args(call)?.into_iter();
    let start = args.next();
    let delete = match (&start, args.next()) {
      (_, Some(delete)) => delete,
      (Some(_), None) => Expr::Lit(Lit::Undefined),
      (None, None) => Expr::Lit(Lit::Int(0)),
    };
    let rest = Expr::Array(args.map(Entry::Item).collect());
    Ok(vec![start.unwrap_or(Expr::Lit(Lit::Int(0))), delete, rest])
  }

  /// `name` applied to `leading` and then the call's arguments. With a spread among them, [`Builtin::Spread`] applies it to the list they make.
  fn builtin_call(&mut self, name: Builtin, leading: Vec<Expr>, call: &js::CallExpression) -> Lowered<Expr> {
    let mut entries: Vec<Entry> = leading.into_iter().map(Entry::Item).collect();
    for a in &call.arguments {
      let value = self.expr(crate::syntax::arg_expr(a))?;
      entries.push(if crate::syntax::is_spread(a) { Entry::Spread(value) } else { Entry::Item(value) });
    }
    if entries.iter().any(|e| matches!(e, Entry::Spread(_))) {
      return Ok(Expr::Builtin { name: Builtin::Spread, args: vec![Expr::lit_str(name.atom()), Expr::Array(entries)] });
    }
    let args = entries.into_iter().filter_map(|e| match e {
      Entry::Item(value) => Some(value),
      _ => None,
    });
    Ok(Expr::Builtin { name, args: args.collect() })
  }

  /// `e` as a function value when it is one: a function written in place or a name bound to one.
  fn function_value(&mut self, e: &js::Expression) -> Lowered<Option<Expr>> {
    match strip_parens(e) {
      js::Expression::ArrowFunctionExpression(_) | js::Expression::FunctionExpression(_) => self.expr(e).map(Some),
      js::Expression::Identifier(id) => {
        let name = id.name.as_str();
        if let Some(bound) = self.scope.iter().rev().find(|(n, _)| n == name).map(|(_, b)| b.clone()) {
          let function = matches!(bound, Expr::Lambda { .. } | Expr::Fix { .. }) || self.function_source(e).is_some();
          return Ok(function.then_some(bound));
        }
        if let Some(function) = self.local_fns.iter().rev().find(|(n, _)| n == name).map(|(_, f)| *f) {
          return self.function_expr(&function).map(Some);
        }
        Ok(self.globals.iter().rev().find(|(n, _)| n == name).map(|(_, b)| b.clone()).filter(|b| matches!(b, Expr::Lambda { .. } | Expr::Fix { .. })))
      }
      _ => Ok(None),
    }
  }

  /// `f.call(this, ...args)`, `f.apply(this, args)` and `f.bind(this, ...args)` of a function value. `this` is not read: a function the build lowers has none.
  fn function_method(&mut self, method: &str, f: Expr, call: &js::CallExpression) -> Lowered<Expr> {
    let mut rest = Vec::new();
    for a in call.arguments.iter().skip(1) {
      let value = self.expr(crate::syntax::arg_expr(a))?;
      rest.push(if crate::syntax::is_spread(a) { Entry::Spread(value) } else { Entry::Item(value) });
    }
    let f = Box::new(f);
    match method {
      "apply" => {
        let list = rest.into_iter().next().map(|e| match e {
          Entry::Item(e) | Entry::Spread(e) => e,
          other => Expr::Array(vec![other]),
        });
        let list = list.map_or(Expr::Array(Vec::new()), |list| Expr::Coalesce(Box::new(list), Box::new(Expr::Array(Vec::new()))));
        Ok(Expr::ApplyOf(f, Box::new(list)))
      }
      "bind" if rest.is_empty() => Ok(*f),
      "bind" => {
        let more = format!("...{}", self.temp());
        rest.push(Entry::Spread(Expr::Var(more.clone())));
        Ok(Expr::Lambda { params: vec![more], body: Box::new(Expr::ApplyOf(f, Box::new(Expr::Array(rest)))) })
      }
      _ => Ok(Expr::ApplyOf(f, Box::new(Expr::Array(rest)))),
    }
  }

  /// `a.union(b)` and the other `Set` methods, `a` and `b` being sets as arrays. The result keeps the order JavaScript gives it: `intersection` walks the smaller set.
  fn set_method(&mut self, method: &str, a: Expr, b: Expr) -> Expr {
    let (sa, sb) = (self.temp(), self.temp());
    let var = |n: &String| Expr::Var(n.clone());
    let keep = |this: &mut Self, from: &String, other: &String, inside: bool| {
      let x = this.temp();
      let found = Expr::Builtin { name: Builtin::Includes, args: vec![var(other), var(&x)] };
      let test = if inside { found } else { Expr::Not(Box::new(found)) };
      Expr::Filter(Box::new(var(from)), Box::new(Expr::Lambda { params: vec![x], body: Box::new(test) }))
    };
    let set = |e: Expr| Expr::Builtin { name: Builtin::AsSet, args: vec![e] };
    let answer = match method {
      "union" => set(Expr::Builtin { name: Builtin::Unique, args: vec![Expr::Builtin { name: Builtin::Concat, args: vec![var(&sa), var(&sb)] }] }),
      "intersection" => {
        let smaller = Expr::Compare(CompareOp::Le, Box::new(Expr::Length(Box::new(var(&sa)))), Box::new(Expr::Length(Box::new(var(&sb)))));
        let (from_a, from_b) = (keep(self, &sa, &sb, true), keep(self, &sb, &sa, true));
        set(Expr::Ternary(Box::new(smaller), Box::new(from_a), Box::new(from_b)))
      }
      "difference" => set(keep(self, &sa, &sb, false)),
      "symmetricDifference" => {
        let (only_a, only_b) = (keep(self, &sa, &sb, false), keep(self, &sb, &sa, false));
        set(Expr::Builtin { name: Builtin::Concat, args: vec![only_a, only_b] })
      }
      "isSubsetOf" | "isSupersetOf" | "isDisjointFrom" => {
        let (from, other) = if method == "isSupersetOf" { (&sb, &sa) } else { (&sa, &sb) };
        let x = self.temp();
        let found = Expr::Lambda { params: vec![x.clone()], body: Box::new(Expr::Builtin { name: Builtin::Includes, args: vec![var(other), var(&x)] }) };
        match method {
          "isDisjointFrom" => Expr::Not(Box::new(Expr::Some(Box::new(var(from)), Box::new(found)))),
          _ => Expr::Every(Box::new(var(from)), Box::new(found)),
        }
      }
      other => unreachable!("`{other}` is not a set method"),
    };
    Expr::Block(vec![Stmt::Let { name: sa, expr: a }, Stmt::Let { name: sb, expr: b }, Stmt::Return(answer)])
  }

  /// `value` walked as `JSON.stringify` walks it under a replacer: a function is called with each key and value, the top value under `""`; an array keeps the fields of each object it names, in its order.
  fn replaced(&mut self, value: Expr, replacer: Expr) -> Expr {
    let (f, walk, key, given, v, pair, i, x, k) = (self.temp(), self.temp(), self.temp(), self.temp(), self.temp(), self.temp(), self.temp(), self.temp(), self.temp());
    let var = |n: &String| Expr::Var(n.clone());
    let lit = |s: &str| Expr::lit_str(s);
    let builtin = |name: Builtin, args: Vec<Expr>| Expr::Builtin { name, args };
    let recur = |key: Expr, value: Expr| Expr::Apply { f: Box::new(var(&walk)), args: vec![key, value] };
    let is_function = Expr::Compare(CompareOp::Eq, Box::new(builtin(Builtin::TypeOf, vec![var(&f)])), Box::new(lit("function")));
    let called = Expr::Ternary(Box::new(is_function), Box::new(Expr::Apply { f: Box::new(var(&f)), args: vec![var(&key), var(&given)] }), Box::new(var(&given)));
    let items = Expr::Map(Box::new(var(&v)), Box::new(Expr::Lambda { params: vec![x.clone(), i.clone()], body: Box::new(recur(Expr::Str(Box::new(var(&i))), var(&x))) }));
    let every_field = Expr::Map(
      Box::new(Expr::Entries(Box::new(var(&v)))),
      Box::new(Expr::Lambda {
        params: vec![pair.clone()],
        body: Box::new(Expr::Array(vec![
          Entry::Item(Expr::Index(Box::new(var(&pair)), Box::new(Expr::Lit(Lit::Float(0.0))))),
          Entry::Item(recur(Expr::Index(Box::new(var(&pair)), Box::new(Expr::Lit(Lit::Float(0.0)))), Expr::Index(Box::new(var(&pair)), Box::new(Expr::Lit(Lit::Float(1.0)))))),
        ])),
      }),
    );
    let named = builtin(Builtin::Unique, vec![Expr::Map(Box::new(var(&f)), Box::new(Expr::Lambda { params: vec![x.clone()], body: Box::new(Expr::Str(Box::new(var(&x)))) }))]);
    let present = Expr::Filter(Box::new(named), Box::new(Expr::Lambda { params: vec![k.clone()], body: Box::new(builtin(Builtin::HasKey, vec![var(&v), var(&k)])) }));
    let listed = Expr::Map(Box::new(present), Box::new(Expr::Lambda { params: vec![k.clone()], body: Box::new(Expr::Array(vec![Entry::Item(var(&k)), Entry::Item(recur(var(&k), Expr::Index(Box::new(var(&v)), Box::new(var(&k)))))])) }));
    let fields = Expr::Ternary(Box::new(builtin(Builtin::IsArray, vec![var(&f)])), Box::new(listed), Box::new(every_field));
    let object = Expr::Logic(
      LogicOp::And,
      Box::new(Expr::Compare(CompareOp::Eq, Box::new(builtin(Builtin::TypeOf, vec![var(&v)])), Box::new(lit("object")))),
      Box::new(Expr::Compare(CompareOp::Ne, Box::new(var(&v)), Box::new(Expr::Lit(Lit::Null)))),
    );
    let shaped = Expr::Ternary(
      Box::new(builtin(Builtin::IsArray, vec![var(&v)])),
      Box::new(items),
      Box::new(Expr::Ternary(Box::new(object), Box::new(builtin(Builtin::FromEntries, vec![fields])), Box::new(var(&v)))),
    );
    let body = Expr::Block(vec![Stmt::Let { name: v.clone(), expr: called }, Stmt::Return(shaped)]);
    let walker = Expr::Fix { name: walk.clone(), f: Box::new(Expr::Lambda { params: vec![key, given], body: Box::new(body) }) };
    Expr::Block(vec![Stmt::Let { name: f, expr: replacer }, Stmt::Let { name: walk.clone(), expr: walker }, Stmt::Return(recur(lit(""), value))])
  }

  /// The arguments of a call, each lowered; a spread argument is refused.
  pub(crate) fn plain_args(&mut self, call: &js::CallExpression) -> Lowered<Vec<Expr>> {
    let mut args = Vec::with_capacity(call.arguments.len());
    for a in &call.arguments {
      if crate::syntax::is_spread(a) {
        return Err(self.residue(crate::syntax::arg_expr(a).span(), "a spread argument"));
      }
      args.push(self.expr(crate::syntax::arg_expr(a))?);
    }
    Ok(args)
  }

  /// `findLast`, `findLastIndex` or `reduceRight` of `items` with `f`: the forward method over the reversed items, `f` handed each item's index in the original order and the original array.
  fn from_the_end(&mut self, method: &str, items: Expr, f: Box<Expr>, init: Option<Expr>) -> Expr {
    let (all, last, reversed) = (self.temp(), self.temp(), self.temp());
    let var = |name: &String| Expr::Var(name.clone());
    let original = |i: Expr| Expr::Arith(ArithOp::Sub, Box::new(var(&last)), Box::new(i));
    let mut steps = vec![
      Stmt::Let { name: all.clone(), expr: items },
      Stmt::Let { name: last.clone(), expr: Expr::Arith(ArithOp::Sub, Box::new(Expr::Length(Box::new(var(&all)))), Box::new(Expr::Lit(Lit::Float(1.0)))) },
      Stmt::Let { name: reversed.clone(), expr: Expr::Builtin { name: Builtin::Reverse, args: vec![var(&all)] } },
    ];
    let (x, i, acc) = (self.temp(), self.temp(), self.temp());
    let calls = |shift: i128| {
      let index = if shift == 0 { var(&i) } else { Expr::Arith(ArithOp::Add, Box::new(var(&i)), Box::new(Expr::Lit(Lit::Float(shift as f64)))) };
      Expr::Apply { f: f.clone(), args: vec![var(&x), original(index), var(&all)] }
    };
    let result = match method {
      "findLast" => Expr::Find(Box::new(var(&reversed)), Box::new(Expr::Lambda { params: vec![x.clone(), i.clone()], body: Box::new(calls(0)) })),
      "findLastIndex" => {
        let found = self.temp();
        steps.push(Stmt::Let { name: found.clone(), expr: Expr::FindIndex(Box::new(var(&reversed)), Box::new(Expr::Lambda { params: vec![x.clone(), i.clone()], body: Box::new(calls(0)) })) });
        let missing = Expr::Compare(CompareOp::Lt, Box::new(var(&found)), Box::new(Expr::Lit(Lit::Float(0.0))));
        Expr::Ternary(Box::new(missing), Box::new(Expr::Lit(Lit::Float(-1.0))), Box::new(original(var(&found))))
      }
      _ => {
        let reducing = |shift: i128| {
          let index = if shift == 0 { var(&i) } else { Expr::Arith(ArithOp::Add, Box::new(var(&i)), Box::new(Expr::Lit(Lit::Float(shift as f64)))) };
          Expr::Lambda { params: vec![acc.clone(), x.clone(), i.clone()], body: Box::new(Expr::Apply { f: f.clone(), args: vec![var(&acc), var(&x), original(index), var(&all)] }) }
        };
        match init {
          Some(init) => Expr::Reduce(Box::new(var(&reversed)), Box::new(init), Box::new(reducing(0))),
          None => {
            let rest = Expr::Builtin { name: Builtin::Slice, args: vec![var(&reversed), Expr::Lit(Lit::Float(1.0))] };
            let first = Expr::Index(Box::new(var(&reversed)), Box::new(Expr::Lit(Lit::Float(0.0))));
            let reduced = Expr::Reduce(Box::new(rest), Box::new(first), Box::new(reducing(1)));
            let empty = Expr::Builtin { name: Builtin::Throw, args: vec![Expr::lit_str("Reduce of empty array with no initial value")] };
            Expr::Ternary(Box::new(Expr::Length(Box::new(var(&reversed)))), Box::new(reduced), Box::new(empty))
          }
        }
      }
    };
    steps.push(Stmt::Return(result));
    Expr::Block(steps)
  }

  /// `Object.assign` onto an object written in place, `Object.freeze`, `Object.hasOwn`, `Object.is` and `Object.groupBy`.
  fn object_static(&mut self, method: &str, call: &js::CallExpression) -> Lowered<Expr> {
    match method {
      "assign" => {
        let Some(first) = call.arguments.first() else { return Err(self.residue(call.span, "`Object.assign` without a target")) };
        if !matches!(strip_parens(crate::syntax::arg_expr(first)), js::Expression::ObjectExpression(_)) || crate::syntax::is_spread(first) {
          return Err(self.residue_with(call.span, "`Object.assign` onto an object something else holds, which it writes in place", "`{ ...target, ...source }` gives the same object without the write"));
        }
        Ok(Expr::Object(self.plain_args(call)?.into_iter().map(Entry::Spread).collect()))
      }
      "freeze" => {
        let [value] = call.arguments.as_slice() else { return Err(self.residue(call.span, "`Object.freeze` takes one value")) };
        self.expr(crate::syntax::arg_expr(value))
      }
      "hasOwn" | "is" => {
        let args = self.plain_args(call)?;
        if args.len() != 2 {
          return Err(self.residue(call.span, format!("`Object.{method}` takes two arguments")));
        }
        let name = if method == "hasOwn" { Builtin::HasKey } else { Builtin::SameValue };
        Ok(Expr::Builtin { name, args })
      }
      _ => {
        let mut args = self.plain_args(call)?.into_iter();
        let (Some(items), Some(f), None) = (args.next(), args.next(), args.next()) else { return Err(self.residue(call.span, "`Object.groupBy` takes the items and a function")) };
        let (acc, item, index, key) = (self.temp(), self.temp(), self.temp(), self.temp());
        let var = |name: &String| Expr::Var(name.clone());
        let earlier = Expr::Coalesce(Box::new(Expr::Index(Box::new(var(&acc)), Box::new(var(&key)))), Box::new(Expr::Array(Vec::new())));
        let group = Expr::Array(vec![Entry::Spread(earlier), Entry::Item(var(&item))]);
        let body = Expr::Block(vec![
          Stmt::Let { name: key.clone(), expr: Expr::Str(Box::new(Expr::Apply { f: Box::new(f), args: vec![var(&item), var(&index)] })) },
          Stmt::Return(Expr::Object(vec![Entry::Spread(var(&acc)), Entry::Computed(var(&key), group)])),
        ]);
        Ok(Expr::Reduce(Box::new(items), Box::new(Expr::Object(Vec::new())), Box::new(Expr::Lambda { params: vec![acc, item, index], body: Box::new(body) })))
      }
    }
  }

  /// `Promise.allSettled(list)`: each item as `{ status: "fulfilled", value }`. An item whose making throws is `{ status: "rejected", reason }`. A list written in place settles each element; a `map` settles each call of its function.
  fn all_settled(&mut self, call: &js::CallExpression) -> Lowered<Expr> {
    let [arg] = call.arguments.as_slice() else { return Err(self.residue(call.span, "`Promise.allSettled` takes one list")) };
    let fulfilled = |value: Expr| Expr::Object(vec![Entry::Field("status".to_owned(), Expr::lit_str("fulfilled")), Entry::Field("value".to_owned(), value)]);
    let settle = |this: &mut Self, value: Expr| {
      let reason = this.temp();
      let rejected = Expr::Object(vec![Entry::Field("status".to_owned(), Expr::lit_str("rejected")), Entry::Field("reason".to_owned(), Expr::Var(reason.clone()))]);
      Expr::Block(vec![Stmt::Try { body: vec![Stmt::Return(fulfilled(value))], name: Some(reason), handler: Some(vec![Stmt::Return(rejected)]), finally: Vec::new() }])
    };
    if let js::Expression::ArrayExpression(array) = strip_parens(crate::syntax::arg_expr(arg)) {
      let mut items = Vec::with_capacity(array.elements.len());
      for element in &array.elements {
        let Some(e) = element.as_expression() else { return Err(self.residue(array.span, "`Promise.allSettled` of a list with a hole or a spread")) };
        let value = self.expr(e)?;
        items.push(Entry::Item(settle(self, value)));
      }
      return Ok(Expr::Array(items));
    }
    Ok(match self.expr(crate::syntax::arg_expr(arg))? {
      Expr::Map(over, f) => {
        let (item, index) = (self.temp(), self.temp());
        let each = settle(self, Expr::Apply { f, args: vec![Expr::Var(item.clone()), Expr::Var(index.clone())] });
        Expr::Map(over, Box::new(Expr::Lambda { params: vec![item, index], body: Box::new(each) }))
      }
      list => {
        let item = self.temp();
        Expr::Map(Box::new(list), Box::new(Expr::Lambda { params: vec![item.clone()], body: Box::new(fulfilled(Expr::Var(item))) }))
      }
    })
  }

  /// A `svelte/store` call, a store being the value it holds when the server
  /// subscribes: `writable(v)`, `readable(v)`, `readonly(s)`, `get(s)` and
  /// `svelte/motion`'s `spring(v)` and `tweened(v)` the value, `derived(source,
  /// fn)` `fn` of the source's value. `crossfade` makes the pair of transitions
  /// only the browser runs and `createEventDispatcher` the function only a
  /// handler calls. A start function
  /// or a `derived` callback that sets the value runs on subscribe, so either
  /// is refused.
  /// The context a key names, under `framework`: a string key as `framework:` and the string, a name as `framework:` and the file and name it is declared under, followed through its import.
  pub(crate) fn context_key(&self, framework: &str, key: &js::Expression) -> Lowered<String> {
    match crate::meaning::bare(key) {
      js::Expression::StringLiteral(s) => Ok(format!("{framework}:{}", lossy(s.value))),
      js::Expression::Identifier(id) => Ok(match self.meanings.as_ref().and_then(|meanings| meanings.of_ident(id)).and_then(|meant| meant.origin()) {
        Some((declared, name)) => format!("{framework}:{declared}#{name}"),
        None => format!("{framework}:{}", id.name),
      }),
      other => Err(self.residue(other.span(), "a context key that is not a string or a name")),
    }
  }

  pub(crate) fn svelte_store(&mut self, store: &str, call: &js::CallExpression) -> Lowered<Expr> {
    let arg = |i: usize| call.arguments.get(i).map(syntax::arg_expr);
    match store {
      "writable" | "readable" => {
        if call.arguments.len() > 1 {
          return Err(self.residue(call.span, format!("a `{store}` store with a start function, which sets its value when it is subscribed")));
        }
        match arg(0) {
          Some(value) => self.expr(value),
          None => Ok(Expr::Lit(Lit::Null)),
        }
      }
      "spring" | "tweened" => match arg(0) {
        Some(value) => self.expr(value),
        None => Ok(Expr::Lit(Lit::Null)),
      },
      "createEventDispatcher" => Ok(Expr::Handler(snapfire_fsr_ir::render::BROWSER_HANDLER)),
      "getContext" => {
        let key = arg(0).ok_or_else(|| self.residue(call.span, "`getContext` without a key"))?;
        let key = self.context_key("svelte", key)?;
        Ok(match self.provided.iter().rev().find(|(provided, _)| *provided == key) {
          Some((_, value)) => value.clone(),
          None => Expr::Context(key),
        })
      }
      "setContext" => Err(self.residue(call.span, "`setContext` inside an expression; it is a statement of the script")),
      "crossfade" => {
        let browser = || Entry::Item(Expr::Handler(snapfire_fsr_ir::render::BROWSER_HANDLER));
        Ok(Expr::Array(vec![browser(), browser()]))
      }
      "readonly" | "get" => match arg(0) {
        Some(value) => self.expr(value),
        None => Ok(Expr::Lit(Lit::Null)),
      },
      _ => {
        let (Some(source), Some(callback)) = (arg(0), arg(1)) else {
          return Err(self.residue(call.span, "a `derived` store without its source and callback"));
        };
        let params = match strip_parens(callback) {
          js::Expression::ArrowFunctionExpression(arrow) => params_of(&arrow.params).len(),
          js::Expression::FunctionExpression(f) => params_of(&f.params).len(),
          _ => return Err(self.residue(callback.span(), "a `derived` callback that is not a function written out")),
        };
        if params != 1 {
          return Err(self.residue(callback.span(), "a `derived` callback that sets its value, which runs when the store is subscribed"));
        }
        let f = self.expr(callback)?;
        let source = self.expr(source)?;
        Ok(Expr::Apply { f: Box::new(f), args: vec![source] })
      }
    }
  }
}

/// The locale a case call writes in place, `Some(None)` for none: `undefined` or an empty array. A string or an array of strings gives its first. `None` for a locale the code computes.
fn written_locale(given: &js::Expression) -> Option<Option<String>> {
  match given {
    js::Expression::Identifier(id) if id.name.as_str() == "undefined" => Some(None),
    js::Expression::StringLiteral(tag) => Some(Some(crate::lossy(tag.value))),
    js::Expression::ArrayExpression(list) => {
      let tags: Option<Vec<String>> = list.elements.iter().map(|e| match e {
        js::ArrayExpressionElement::StringLiteral(tag) => Some(crate::lossy(tag.value)),
        _ => None,
      }).collect();
      Some(tags?.into_iter().next())
    }
    _ => None,
  }
}

/// Whether `e` makes an iterator: `values()`, `keys()` or `entries()` of a value, `Iterator.from(x)` or an iterator helper over one. A body's iterators are the arrays they would yield.
fn iterator_made(e: &js::Expression) -> bool {
  let js::Expression::CallExpression(call) = strip_parens(e) else { return false };
  let Some(member) = call.callee.as_member_expression() else { return false };
  let name = match crate::syntax::prop(member) {
    crate::syntax::MemberProp::Ident(p) => p.name.as_str(),
    _ => return false,
  };
  match name {
    "values" | "keys" | "entries" => true,
    "from" => matches!(member.object(), js::Expression::Identifier(id) if id.name.as_str() == "Iterator"),
    "map" | "filter" | "take" | "drop" | "flatMap" => iterator_made(member.object()),
    _ => false,
  }
}
