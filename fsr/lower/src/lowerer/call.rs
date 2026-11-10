use crate::*;

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
    let applied = matches!(callee, js::Expression::Identifier(id) if self.scope.iter().any(|(n, _)| *n == id.name.as_str()) || self.local_fns.iter().any(|(n, _)| *n == id.name.as_str()) || self.globals.iter().any(|(n, _)| *n == id.name.as_str()));
    for arg in &call.arguments {
      if crate::syntax::is_spread(arg) && !math && !applied {
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
                return Err(self.browser_only(call.span, format!("`{name}` reads the current time on a render path, which the server and the browser read at different moments")));
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
          "String" if matches!(call.arguments.as_slice(), [only] if !crate::syntax::is_spread(only)) => Ok(Expr::Str(Box::new(self.as_primitive(&crate::syntax::arg_expr(&call.arguments[0]))?))),
          "String" => Ok(Expr::Str(one(self)?)),
          "Number" if matches!(call.arguments.as_slice(), [only] if !crate::syntax::is_spread(only)) => Ok(Expr::Num(Box::new(self.as_primitive(&crate::syntax::arg_expr(&call.arguments[0]))?))),
          "Number" => Ok(Expr::Num(one(self)?)),
          "BigInt" => Ok(Expr::BigInt(one(self)?)),
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
          name if ERROR_CONSTRUCTORS.contains(&name) => {
            let message = match call.arguments.first() {
              Some(a) => Expr::Str(Box::new(self.expr(crate::syntax::arg_expr(a))?)),
              None => Expr::lit_str(""),
            };
            Ok(Expr::Object(vec![Entry::Field("name".to_owned(), Expr::lit_str(name)), Entry::Field("message".to_owned(), message)]))
          }
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
      if matches!(crate::meaning::bare(callee), js::Expression::ArrowFunctionExpression(_) | js::Expression::FunctionExpression(_)) {
        return Err(self.residue_with(
          callee.span(),
          "a function called where it is written",
          "the build does not run a function to produce a value; write the value as an expression or move the steps into a named function the body calls",
        ));
      }
      let f = Box::new(self.expr(callee)?);
      return self.applied(f, call);
    };
    if let Some(applied) = self.function_member(member, call)? {
      return Ok(applied);
    }
    let method = self.member_name(member).ok_or_else(|| self.residue(member.span(), "a computed method name"))?;
    if self.kind_of(member.object()) == Some(Kind::Scope) {
      return self.scope_call(&method, member, call);
    }

    if let js::Expression::Identifier(obj) = member.object() {
      let global = obj.name.as_str();
      if !self.scope.iter().any(|(n, _)| n == global) && !self.globals.iter().any(|(n, _)| n == global) {
        match global {
          "Object" => {
            let a = call.arguments.first().ok_or_else(|| self.residue(call.span, format!("`Object.{method}` takes one argument")))?;
            let target = Box::new(self.expr(crate::syntax::arg_expr(a))?);
            return match method.as_str() {
              "entries" => Ok(Expr::Entries(target)),
              "fromEntries" => Ok(Expr::Builtin { name: Builtin::FromEntries, args: vec![*target] }),
              "keys" => Ok(Expr::Keys(target)),
              "values" => Ok(Expr::Values(target)),
              _ => Err(self.residue(member.span(), format!("`Object.{method}`"))),
            };
          }
          "Math" if crate::JS_MATH.contains(&method.as_str()) => {
            let mut args = vec![Expr::lit_str(method.as_str())];
            for a in &call.arguments {
              if crate::syntax::is_spread(a) {
                return Err(self.residue(crate::syntax::arg_expr(a).span(), "a spread argument"));
              }
              args.push(self.expr(crate::syntax::arg_expr(a))?);
            }
            return Ok(Expr::Builtin { name: Builtin::Math, args });
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
          "Math" if method == "random" && self.reaches(Reach::Terminal) && call.arguments.is_empty() => {
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
            let mut args = Vec::with_capacity(call.arguments.len());
            for a in &call.arguments {
              if crate::syntax::is_spread(a) {
                return Err(self.residue(crate::syntax::arg_expr(a).span(), "a spread argument"));
              }
              args.push(self.expr(crate::syntax::arg_expr(a))?);
            }
            return Ok(Expr::Builtin { name, args });
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
            if call.arguments.len() > 3 || call.arguments.get(1).is_some_and(|a| !matches!(crate::syntax::arg_expr(a), js::Expression::NullLiteral(_))) {
              return Err(self.residue(call.span, "`JSON.stringify` with a replacer; it takes a value, `null` and an indent"));
            }
            let mut args = Vec::with_capacity(call.arguments.len());
            for a in &call.arguments {
              args.push(self.expr(crate::syntax::arg_expr(a))?);
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

    if let Some((service, _via_ctx)) = self.service_of(member.object()) {
      let args = self.object_args(call, "service")?;
      return Ok(Expr::Call { service, method, args });
    }

    if let Some((module, _via_ctx)) = self.native_of(member.object()) {
      let args = self.object_args(call, "native")?;
      // `sync` is set by the build from the Rust signature, which this reader cannot see.
      return Ok(Expr::NativeCall { module, method, args, sync: false });
    }

    if method == "fill" {
      if let Some(length) = self.array_of_length(member.object()) {
        let length = self.expr(length)?;
        let value = match call.arguments.first() {
          Some(arg) if call.arguments.len() == 1 => self.expr(crate::syntax::arg_expr(arg))?,
          Some(_) => return Err(self.residue(call.span, "`fill` with a start or an end")),
          None => Expr::Lit(Lit::Null),
        };
        let range = Box::new(Expr::Builtin { name: Builtin::Range, args: vec![length] });
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
    if matches!(method.as_str(), "format" | "select" | "compare") {
      if let Some(made) = self.formatter_of(member.object()) {
        let called = match (platform_object(&made), method.as_str()) {
          (Some(formatter @ ("NumberFormat" | "DateTimeFormat" | "ListFormat" | "RelativeTimeFormat")), "format") | (Some(formatter @ "PluralRules"), "select") | (Some(formatter @ "Collator"), "compare") => Some(formatter),
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
    match self.kind_of(member.object()) {
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
          other => Err(self.residue(member.span(), format!("`.{other}()` of a `URLSearchParams`"))),
        };
      }
      _ => {}
    }

    let target = Box::new(self.expr(member.object())?);
    let lambda = |this: &mut Self, i: usize| -> Lowered<Box<Expr>> {
      let a = call.arguments.get(i).ok_or_else(|| this.residue(call.span, format!("`{method}` takes a function")))?;
      if let js::Expression::Identifier(id) = crate::syntax::arg_expr(a) {
        let name = id.name.as_str();
        if name == "Boolean" && !this.scope.iter().any(|(n, _)| n == name) && this.root_of(id).is_none() && !this.globals.iter().any(|(n, _)| n == name) {
          let item = this.temp();
          return Ok(Box::new(Expr::Lambda { params: vec![item.clone()], body: Box::new(Expr::Not(Box::new(Expr::Not(Box::new(Expr::Var(item)))))) }));
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
      "toString" if call.arguments.is_empty() => Ok(Expr::Str(target)),
      "toString" => Ok(Expr::Builtin { name: Builtin::Radix, args: vec![*target, self.expr(&crate::syntax::arg_expr(&call.arguments[0]))?] }),
      "getTime" | "valueOf" => Ok(*target),
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
      "getFullYear" | "getMonth" | "getDate" | "getDay" | "getHours" | "getMinutes" | "getSeconds" if self.reaches(Reach::Terminal) && call.arguments.is_empty() => {
        let part = match method.as_str() {
          "getFullYear" => "year",
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
        let body = Expr::Builtin { name: Builtin::DatePart, args: vec![local, Expr::lit_str(part)] };
        Ok(Expr::Apply { f: Box::new(Expr::Lambda { params: vec![at], body: Box::new(body) }), args: vec![*target] })
      }
      "getTimezoneOffset" if self.reaches(Reach::Terminal) => Ok(Expr::Ext { module: "time".to_owned(), name: "localOffset".to_owned(), args: vec![*target] }),
      "toLocaleTimeString" if (self.reaches(Reach::Terminal) || !self.on_render_path()) && call.arguments.len() <= 1 => {
        let locale = match call.arguments.first().map(|a| strip_parens(crate::syntax::arg_expr(a))) {
          None => Expr::Lit(Lit::Null),
          Some(js::Expression::Identifier(id)) if id.name.as_str() == "undefined" => Expr::Lit(Lit::Null),
          Some(js::Expression::StringLiteral(tag)) => Expr::lit_str(crate::lossy(tag.value)),
          Some(other) => return Err(self.residue_with(other.span(), "a locale that is not written in place", "write the locale, `\"en-US\"`; `undefined` takes the system's")),
        };
        Ok(Expr::Ext { module: "intl".to_owned(), name: "localTime".to_owned(), args: vec![*target, locale] })
      }
      "getFullYear" | "getMonth" | "getDate" | "getDay" | "getHours" | "getMinutes" | "getSeconds" | "getTimezoneOffset" | "toLocaleTimeString" | "toTimeString" | "toDateString" => {
        let utc = method.replacen("get", "getUTC", 1);
        let mut residue = self.browser_only(member.span(), format!("`.{method}()`, which reads the viewer's time zone"));
        residue.hint = Some(format!("as text under an element the browser fills it in when the component hydrates; the server has no viewer, so `.{utc}()` and `toLocaleDateString(undefined, {{ dateStyle }})` read UTC, which both agree on"));
        Err(residue)
      }
      "localeCompare" => {
        let other = match call.arguments.first() {
          Some(arg) => self.expr(crate::syntax::arg_expr(arg))?,
          None => return Err(self.residue(call.span, "`localeCompare` without a string to compare with")),
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
        let mut args = vec![*target];
        for a in &call.arguments {
          if crate::syntax::is_spread(a) {
            return Err(self.residue(crate::syntax::arg_expr(a).span(), "a spread argument"));
          }
          args.push(self.expr(crate::syntax::arg_expr(a))?);
        }
        Ok(Expr::Builtin { name, args })
      }
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
      "toLocaleString" => {
        let formatted = self.intl_format("NumberFormat", &call.arguments, *target, call.span)?;
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
        "the builtins are `map`, `filter`, `find`, `findIndex`, `some`, `every`, `reduce`, `flatMap`, `flat`, `toSorted`, `toReversed`, `slice`, `at`, `indexOf`, `lastIndexOf`, `charAt`, `charCodeAt`, `codePointAt`, `concat`, `join`, `includes`, `trim`, `repeat`, `toFixed`, `toString`, `toUpperCase`, `toLowerCase`, `padStart`, `padEnd`, `substring`, `split`, `startsWith`, `endsWith`, `replace`, `replaceAll`, `match`, `matchAll`, `search`, `test`, `localeCompare`, `toLocaleString`, `trimStart`, `trimEnd`, `valueOf`, `getTime`, `toISOString` and `toJSON`, with `sort` and `reverse` on a fresh array; anything else goes in a module-level helper the build can read",
      )),
    }
  }
}

impl Lowerer<'_> {
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
