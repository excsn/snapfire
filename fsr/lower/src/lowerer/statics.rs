use crate::*;

use snapfire_fsr_ir::render::typed;

/// The constructors whose `prototype` a body may name.
const PROTOTYPES: &[&str] = &["Object", "Array", "String", "Number", "Boolean", "Function", "Map", "Set", "Date", "RegExp", "Promise", "Error", "BigInt", "Symbol"];

/// `throw value`, its message read from the value.
fn thrown(value: Expr) -> Expr {
  let message = Expr::Coalesce(Box::new(Expr::Field(Box::new(value.clone()), "message".to_owned())), Box::new(value.clone()));
  Expr::Builtin { name: Builtin::Throw, args: vec![message, value] }
}

/// `C.prototype` of a built-in constructor `C`.
pub(crate) fn prototype(class: &str) -> Expr {
  Expr::Builtin { name: Builtin::Tagged, args: vec![Expr::lit_str(typed::PROTOTYPE), Expr::lit_str(class)] }
}

impl<'a> Lowerer<'a> {
  /// `String.raw(strings, ...subs)`: each raw string followed by the substitution after it, the last raw string alone.
  pub(crate) fn string_raw(&mut self, call: &js::CallExpression) -> Lowered<Expr> {
    let mut args = self.plain_args(call)?.into_iter();
    let strings = args.next().unwrap_or(Expr::Lit(Lit::Undefined));
    let subs = Expr::Array(args.map(Entry::Item).collect());
    let (raw, list, piece, at) = (self.temp(), self.temp(), self.temp(), self.temp());
    let var = |n: &String| Expr::Var(n.clone());
    let next = Expr::Arith(ArithOp::Add, Box::new(var(&at)), Box::new(Expr::Lit(Lit::Int(1))));
    let more = Expr::Compare(CompareOp::Lt, Box::new(next), Box::new(Expr::Length(Box::new(var(&raw)))));
    let given = Expr::Compare(CompareOp::Lt, Box::new(var(&at)), Box::new(Expr::Length(Box::new(var(&list)))));
    let sub = Expr::Ternary(Box::new(given), Box::new(Expr::Str(Box::new(Expr::Index(Box::new(var(&list)), Box::new(var(&at)))))), Box::new(Expr::lit_str("")));
    let item = Expr::Ternary(Box::new(more), Box::new(Expr::Template(vec![Expr::Str(Box::new(var(&piece))), sub])), Box::new(Expr::Str(Box::new(var(&piece)))));
    let joined = Expr::Builtin { name: Builtin::Join, args: vec![Expr::Map(Box::new(var(&raw)), Box::new(Expr::Lambda { params: vec![piece, at], body: Box::new(item) })), Expr::lit_str("")] };
    Ok(Expr::Block(vec![
      Stmt::Let { name: raw, expr: Expr::Index(Box::new(strings), Box::new(Expr::lit_str("raw"))) },
      Stmt::Let { name: list, expr: subs },
      Stmt::Return(joined),
    ]))
  }

  /// `Map.groupBy(items, f)`: a `Map` from each key `f` gives to the items that gave it, in the order the keys first came.
  pub(crate) fn map_group_by(&mut self, call: &js::CallExpression) -> Lowered<Expr> {
    let mut args = self.plain_args(call)?.into_iter();
    let (Some(items), Some(f), None) = (args.next(), args.next(), args.next()) else { return Err(self.residue(call.span, "`Map.groupBy` takes the items and a function")) };
    let (acc, item, index, key) = (self.temp(), self.temp(), self.temp(), self.temp());
    let var = |name: &String| Expr::Var(name.clone());
    let earlier = Expr::Coalesce(Box::new(Expr::Index(Box::new(var(&acc)), Box::new(var(&key)))), Box::new(Expr::Array(Vec::new())));
    let group = Expr::Array(vec![Entry::Spread(earlier), Entry::Item(var(&item))]);
    let body = Expr::Block(vec![
      Stmt::Let { name: key.clone(), expr: Expr::Builtin { name: Builtin::MapKey, args: vec![Expr::Apply { f: Box::new(f), args: vec![var(&item), var(&index)] }] } },
      Stmt::Return(Expr::Object(vec![Entry::Spread(var(&acc)), Entry::Computed(var(&key), group)])),
    ]);
    let grouped = Expr::Reduce(Box::new(items), Box::new(Expr::Object(Vec::new())), Box::new(Expr::Lambda { params: vec![acc, item, index], body: Box::new(body) }));
    Ok(Kind::Map.made(grouped))
  }

  /// `BigInt.asUintN(bits, n)`: `n` modulo 2 to the `bits`. `BigInt.asIntN` reads the top bit as the sign.
  pub(crate) fn big_int_wrap(&mut self, method: &str, call: &js::CallExpression) -> Lowered<Expr> {
    let mut args = self.plain_args(call)?.into_iter();
    let (Some(bits), Some(n)) = (args.next(), args.next()) else { return Err(self.residue(call.span, format!("`BigInt.{method}` takes a width and a BigInt"))) };
    let (width, modulus, rest) = (self.temp(), self.temp(), self.temp());
    let var = |name: &String| Expr::Var(name.clone());
    let big = |e: Expr| Expr::BigInt(Box::new(e));
    let arith = |op: ArithOp, a: Expr, b: Expr| Expr::Arith(op, Box::new(a), Box::new(b));
    let mut stmts = vec![
      Stmt::Let { name: width.clone(), expr: Expr::Num(Box::new(bits)) },
      Stmt::Let { name: modulus.clone(), expr: arith(ArithOp::Pow, big(Expr::Lit(Lit::Int(2))), big(var(&width))) },
      Stmt::Let { name: rest.clone(), expr: arith(ArithOp::Rem, arith(ArithOp::Add, arith(ArithOp::Rem, n, var(&modulus)), var(&modulus)), var(&modulus)) },
    ];
    if method == "asUintN" {
      stmts.push(Stmt::Return(var(&rest)));
    } else {
      let half = arith(ArithOp::Quot, var(&modulus), big(Expr::Lit(Lit::Int(2))));
      let signed = Expr::Logic(LogicOp::And, Box::new(Expr::Compare(CompareOp::Gt, Box::new(var(&width)), Box::new(Expr::Lit(Lit::Int(0))))), Box::new(Expr::Compare(CompareOp::Ge, Box::new(var(&rest)), Box::new(half))));
      stmts.push(Stmt::Return(Expr::Ternary(Box::new(signed), Box::new(arith(ArithOp::Sub, var(&rest), var(&modulus))), Box::new(var(&rest)))));
    }
    Ok(Expr::Block(stmts))
  }

  /// `Promise.try(f, ...args)`, `Promise.any(list)` and `Promise.race(list)`. A body holds no promise values, a call being its answer, so `try` is the call itself. `any` is the first item of the list that does not throw, else an `AggregateError` of every reason. `race` is the first item, the others still made.
  pub(crate) fn promise_static(&mut self, method: &str, call: &js::CallExpression) -> Lowered<Expr> {
    if method == "try" {
      let Some(first) = call.arguments.first() else { return Err(self.residue(call.span, "`Promise.try` takes a function")) };
      let b = self.build();
      let ast = b.ast();
      let rest = b.vec(call.arguments.iter().skip(1).map(|a| b.copy(a)));
      let called = js::Expression::new_call_expression(call.span, b.copy(crate::syntax::arg_expr(first)), None, rest, false, &ast);
      return self.expr(b.alloc(called));
    }
    let [list] = call.arguments.as_slice() else { return Err(self.residue(call.span, format!("`Promise.{method}` takes one list"))) };
    let js::Expression::ArrayExpression(array) = strip_parens(crate::syntax::arg_expr(list)) else {
      let items = self.expr(crate::syntax::arg_expr(list))?;
      return Ok(if method == "race" { Expr::Builtin { name: Builtin::At, args: vec![items, Expr::Lit(Lit::Int(0))] } } else { self.first_settled(vec![], Some(items)) });
    };
    let mut items = Vec::with_capacity(array.elements.len());
    for element in &array.elements {
      let Some(e) = element.as_expression() else { return Err(self.residue(array.span, format!("`Promise.{method}` of a list with a hole or a spread"))) };
      items.push(self.expr(e)?);
    }
    if method == "any" {
      return Ok(self.first_settled(items, None));
    }
    let Some((first, others)) = items.split_first() else { return Err(self.residue(array.span, "`Promise.race` of an empty list, which never settles")) };
    let (outcome, reason) = (self.temp(), self.temp());
    let settled = |ok: bool, value: Expr| Expr::Array(vec![Entry::Item(Expr::Lit(Lit::Bool(ok))), Entry::Item(value)]);
    let mut stmts = vec![Stmt::Let {
      name: outcome.clone(),
      expr: Expr::Block(vec![Stmt::Try { body: vec![Stmt::Return(settled(true, first.clone()))], name: Some(reason.clone()), handler: Some(vec![Stmt::Return(settled(false, Expr::Var(reason)))]), finally: Vec::new() }]),
    }];
    for other in others {
      stmts.push(Stmt::Try { body: vec![Stmt::Expr(other.clone())], name: None, handler: Some(Vec::new()), finally: Vec::new() });
    }
    let value = Expr::Index(Box::new(Expr::Var(outcome.clone())), Box::new(Expr::Lit(Lit::Int(1))));
    stmts.push(Stmt::If { cond: Expr::Index(Box::new(Expr::Var(outcome)), Box::new(Expr::Lit(Lit::Int(0)))), then: vec![Stmt::Return(value.clone())], r#else: vec![Stmt::Expr(thrown(value))] });
    Ok(Expr::Block(stmts))
  }

  /// The first of `items` that does not throw, each tried in turn, else an `AggregateError` holding every reason. `list` stands for items already made, whose first is the answer.
  fn first_settled(&mut self, items: Vec<Expr>, list: Option<Expr>) -> Expr {
    let reasons = self.temp();
    let failed = |reasons: &String| Stmt::Expr(thrown(crate::error_object(Expr::Object(vec![
      Entry::Field("name".to_owned(), Expr::lit_str("AggregateError")),
      Entry::Field("message".to_owned(), Expr::lit_str("All promises were rejected")),
      Entry::Field("errors".to_owned(), Expr::Var(reasons.clone())),
    ]))));
    if let Some(list) = list {
      let empty = Expr::Compare(CompareOp::Eq, Box::new(Expr::Length(Box::new(Expr::Var(reasons.clone())))), Box::new(Expr::Lit(Lit::Int(0))));
      let first = Expr::Builtin { name: Builtin::At, args: vec![Expr::Var(reasons.clone()), Expr::Lit(Lit::Int(0))] };
      return Expr::Block(vec![Stmt::Let { name: reasons.clone(), expr: list }, Stmt::If { cond: empty, then: vec![failed(&reasons)], r#else: Vec::new() }, Stmt::Return(first)]);
    }
    let mut stmts = vec![Stmt::Let { name: reasons.clone(), expr: Expr::Array(Vec::new()) }];
    for item in items {
      let reason = self.temp();
      let kept = Expr::Array(vec![Entry::Spread(Expr::Var(reasons.clone())), Entry::Item(Expr::Var(reason.clone()))]);
      stmts.push(Stmt::Try { body: vec![Stmt::Return(item)], name: Some(reason), handler: Some(vec![Stmt::Set { name: reasons.clone(), expr: kept }]), finally: Vec::new() });
    }
    stmts.push(failed(&reasons));
    Expr::Block(stmts)
  }

  /// `Array.fromAsync(items, f?)`: the items each awaited, which in a body are the items themselves.
  pub(crate) fn from_async(&mut self, call: &js::CallExpression) -> Lowered<Expr> {
    let mut args = self.plain_args(call)?.into_iter();
    let Some(items) = args.next() else { return Err(self.residue(call.span, "`Array.fromAsync` takes the items")) };
    let items = Expr::Array(vec![Entry::Spread(items)]);
    Ok(match args.next() {
      Some(f) => Expr::Map(Box::new(items), Box::new(f)),
      None => items,
    })
  }

  /// `Symbol.for(key)` and `Symbol.keyFor(symbol)`: a registered symbol is its key, so two of the same key are the same symbol.
  pub(crate) fn symbol_static(&mut self, method: &str, call: &js::CallExpression) -> Lowered<Expr> {
    let given = match call.arguments.first() {
      Some(a) => self.expr(crate::syntax::arg_expr(a))?,
      None => Expr::Lit(Lit::Undefined),
    };
    Ok(if method == "for" {
      Expr::Builtin { name: Builtin::Tagged, args: vec![Expr::lit_str(typed::SYMBOL), Expr::Str(Box::new(given))] }
    } else {
      Expr::Builtin { name: Builtin::TaggedPayload, args: vec![given, Expr::lit_str(typed::SYMBOL)] }
    })
  }

  /// `JSON.rawJSON(text)` and `JSON.isRawJSON(value)`.
  pub(crate) fn raw_json(&mut self, method: &str, call: &js::CallExpression) -> Lowered<Expr> {
    let given = match call.arguments.first() {
      Some(a) => self.expr(crate::syntax::arg_expr(a))?,
      None => Expr::Lit(Lit::Undefined),
    };
    Ok(if method == "rawJSON" {
      Expr::Builtin { name: Builtin::Tagged, args: vec![Expr::lit_str(typed::RAW_JSON), Expr::Str(Box::new(given))] }
    } else {
      Expr::Builtin { name: Builtin::IsTagged, args: vec![given, Expr::lit_str(typed::RAW_JSON)] }
    })
  }

  /// The JavaScript a `Reflect` call means, lowered: `get` a read, `has` the `in` operator, `set` and `deleteProperty` a write, `apply` a call, `construct` a `new` and `ownKeys` `Object.getOwnPropertyNames`.
  pub(crate) fn reflect(&mut self, method: &str, call: &js::CallExpression) -> Lowered<Expr> {
    let b = self.build();
    let ast = b.ast();
    let span = call.span;
    let arg = |i: usize| call.arguments.get(i).map(|a| b.copy(crate::syntax::arg_expr(a)));
    let undefined = || b.ident("undefined", span);
    let member = |object: js::Expression<'a>, key: js::Expression<'a>| js::Expression::new_computed_member_expression(span, object, key, false, &ast);
    let rewritten = match method {
      "get" => member(arg(0).unwrap_or_else(undefined), arg(1).unwrap_or_else(undefined)),
      "has" => b.binary(arg(1).unwrap_or_else(undefined), js::BinaryOperator::In, arg(0).unwrap_or_else(undefined), span),
      "set" => {
        let target = member(arg(0).unwrap_or_else(undefined), arg(1).unwrap_or_else(undefined));
        let js::Expression::ComputedMemberExpression(target) = target else { unreachable!("built as a computed member") };
        let assign = js::Expression::new_assignment_expression(span, js::AssignmentOperator::Assign, js::AssignmentTarget::ComputedMemberExpression(target), arg(2).unwrap_or_else(undefined), &ast);
        js::Expression::new_sequence_expression(span, b.vec([assign, b.boolean(true, span)]), &ast)
      }
      "deleteProperty" => js::Expression::new_unary_expression(span, js::UnaryOperator::Delete, member(arg(0).unwrap_or_else(undefined), arg(1).unwrap_or_else(undefined)), &ast),
      "apply" | "construct" => {
        let f = arg(0).unwrap_or_else(undefined);
        let list = call.arguments.get(if method == "apply" { 2 } else { 1 }).map(|a| crate::syntax::arg_expr(a));
        let passed = match list.map(strip_parens) {
          Some(js::Expression::ArrayExpression(items)) => b.vec(items.elements.iter().map(|item| match item {
            js::ArrayExpressionElement::SpreadElement(s) => js::Argument::new_spread_element(span, b.copy(&s.argument), &ast),
            other => match other.as_expression() {
              Some(e) => js::Argument::from(b.copy(e)),
              None => js::Argument::from(b.ident("undefined", span)),
            },
          })),
          Some(other) => b.vec([js::Argument::new_spread_element(span, b.copy(other), &ast)]),
          None => b.vec([]),
        };
        if method == "apply" {
          js::Expression::new_call_expression(span, f, None, passed, false, &ast)
        } else {
          js::Expression::new_new_expression(span, f, None, passed, &ast)
        }
      }
      "ownKeys" | "getPrototypeOf" | "isExtensible" | "preventExtensions" | "getOwnPropertyDescriptor" | "defineProperty" => {
        let name = match method {
          "ownKeys" => "getOwnPropertyNames",
          other => other,
        };
        let callee = js::Expression::new_static_member_expression(span, b.ident("Object", span), js::IdentifierName::new(span, b.str(name), &ast), false, &ast);
        let called = js::Expression::new_call_expression(span, callee, None, b.vec(call.arguments.iter().map(|a| b.copy(a))), false, &ast);
        if matches!(method, "preventExtensions" | "defineProperty") {
          // `Object`'s answer the object and `Reflect`'s whether they succeeded, which on a body's plain data they do.
          let done = self.expr(b.alloc(called))?;
          return Ok(Expr::Block(vec![Stmt::Expr(done), Stmt::Return(Expr::Lit(Lit::Bool(true)))]));
        }
        called
      }
      other => return Err(self.residue(span, format!("`Reflect.{other}`"))),
    };
    self.expr(b.alloc(rewritten))
  }

  /// The built-in constructor `member` reads the `prototype` of, `Array` in `Array.prototype`, when nothing here binds its name.
  pub(crate) fn builtin_prototype(&self, member: &js::MemberExpression) -> Option<&'static str> {
    let js::Expression::Identifier(id) = member.object() else { return None };
    let class = PROTOTYPES.iter().copied().find(|c| *c == id.name.as_str())?;
    let bound = self.scope.iter().any(|(n, _)| n == class) || self.globals.iter().any(|(n, _)| n == class);
    (!bound && self.member_name(member).as_deref() == Some("prototype")).then_some(class)
  }

  /// `Object.getOwnPropertyNames`, `create`, `getOwnPropertyDescriptor`, `getPrototypeOf`, `seal`, `preventExtensions`, `isFrozen`, `isSealed` and `isExtensible`. A body's objects are plain data with no accessors, so every own property is a writable, enumerable and configurable value, an array's `length` apart. Whether an object is frozen is known where the call shows it.
  pub(crate) fn object_reflect(&mut self, method: &str, call: &js::CallExpression) -> Lowered<Expr> {
    let first = call.arguments.first().map(|a| crate::syntax::arg_expr(a));
    let var = |n: &String| Expr::Var(n.clone());
    match method {
      "create" => {
        let plain = match first.map(|e| strip_parens(e)) {
          Some(js::Expression::NullLiteral(_)) => true,
          Some(e) => e.as_member_expression().is_some_and(|m| self.builtin_prototype(m) == Some("Object")),
          None => false,
        };
        if !plain || call.arguments.len() > 1 {
          return Err(self.residue_with(call.span, "`Object.create` of a prototype other than `null` or `Object.prototype`", "a body's objects carry no prototype; build the object with its own fields"));
        }
        Ok(Expr::Object(Vec::new()))
      }
      "seal" | "preventExtensions" => match first {
        Some(e) => self.expr(e),
        None => Ok(Expr::Lit(Lit::Undefined)),
      },
      "getOwnPropertyNames" => {
        let Some(e) = first else { return Err(self.residue(call.span, "`Object.getOwnPropertyNames` takes one object")) };
        let held = self.temp();
        let value = self.expr(e)?;
        let keys = Expr::Keys(Box::new(var(&held)));
        let index = self.temp();
        let indices = Expr::Map(Box::new(Expr::Builtin { name: Builtin::Range, args: vec![Expr::Length(Box::new(var(&held)))] }), Box::new(Expr::Lambda { params: vec![index.clone()], body: Box::new(Expr::Str(Box::new(var(&index)))) }));
        let with_length = Expr::Array(vec![Entry::Spread(indices), Entry::Item(Expr::lit_str("length"))]);
        Ok(Expr::Block(vec![Stmt::Let { name: held.clone(), expr: value }, Stmt::Return(Expr::Ternary(Box::new(Expr::Builtin { name: Builtin::IsArray, args: vec![var(&held)] }), Box::new(with_length), Box::new(keys)))]))
      }
      "getOwnPropertyDescriptor" => {
        let (Some(e), Some(k)) = (first, call.arguments.get(1)) else { return Err(self.residue(call.span, "`Object.getOwnPropertyDescriptor` takes an object and a key")) };
        let (held, key) = (self.temp(), self.temp());
        let value = self.expr(e)?;
        let k = self.expr(crate::syntax::arg_expr(k))?;
        let field = |name: &str, value: Expr| Entry::Field(name.to_owned(), value);
        let own = Expr::Object(vec![field("value", Expr::Index(Box::new(var(&held)), Box::new(var(&key)))), field("writable", Expr::Lit(Lit::Bool(true))), field("enumerable", Expr::Lit(Lit::Bool(true))), field("configurable", Expr::Lit(Lit::Bool(true)))]);
        let length = Expr::Object(vec![field("value", Expr::Length(Box::new(var(&held)))), field("writable", Expr::Lit(Lit::Bool(true))), field("enumerable", Expr::Lit(Lit::Bool(false))), field("configurable", Expr::Lit(Lit::Bool(false)))]);
        let is_length = Expr::Logic(LogicOp::And, Box::new(Expr::Builtin { name: Builtin::IsArray, args: vec![var(&held)] }), Box::new(Expr::Compare(CompareOp::Eq, Box::new(var(&key)), Box::new(Expr::lit_str("length")))));
        let answer = Expr::Ternary(Box::new(is_length), Box::new(length), Box::new(Expr::Ternary(Box::new(Expr::Builtin { name: Builtin::HasKey, args: vec![var(&held), var(&key)] }), Box::new(own), Box::new(Expr::Lit(Lit::Undefined)))));
        Ok(Expr::Block(vec![Stmt::Let { name: held, expr: value }, Stmt::Let { name: key, expr: Expr::Str(Box::new(k)) }, Stmt::Return(answer)]))
      }
      "getPrototypeOf" => {
        let Some(e) = first else { return Err(self.residue(call.span, "`Object.getPrototypeOf` takes one value")) };
        if let Some(kind @ (Kind::Map | Kind::Set | Kind::Date)) = self.kind_of(e) {
          let value = self.expr(e)?;
          let held = self.temp();
          return Ok(Expr::Block(vec![Stmt::Let { name: held, expr: value }, Stmt::Return(prototype(kind.name()))]));
        }
        let held = self.temp();
        let value = self.expr(e)?;
        let kind = Expr::Builtin { name: Builtin::TypeOf, args: vec![var(&held)] };
        let is = |of: &str| Expr::Compare(CompareOp::Eq, Box::new(kind.clone()), Box::new(Expr::lit_str(of)));
        let pick = |cond: Expr, class: &str, rest: Expr| Expr::Ternary(Box::new(cond), Box::new(prototype(class)), Box::new(rest));
        let answer = pick(
          Expr::Builtin { name: Builtin::IsArray, args: vec![var(&held)] },
          "Array",
          pick(is("string"), "String", pick(is("number"), "Number", pick(is("boolean"), "Boolean", pick(is("function"), "Function", pick(is("bigint"), "BigInt", prototype("Object")))))),
        );
        Ok(Expr::Block(vec![Stmt::Let { name: held, expr: value }, Stmt::Return(answer)]))
      }
      _ => {
        let Some(e) = first.map(|e| strip_parens(e)) else { return Ok(Expr::Lit(Lit::Bool(method != "isExtensible"))) };
        let made_by = |e: &js::Expression| -> Option<String> {
          let js::Expression::CallExpression(c) = strip_parens(e) else { return None };
          let m = c.callee.as_member_expression()?;
          matches!(m.object(), js::Expression::Identifier(id) if id.name.as_str() == "Object").then(|| self.member_name(m)).flatten()
        };
        let empty = |e: &js::Expression| match strip_parens(e) {
          js::Expression::ObjectExpression(o) => o.properties.is_empty(),
          js::Expression::ArrayExpression(a) => a.elements.is_empty(),
          _ => false,
        };
        let answer = match (e, made_by(e).as_deref()) {
          (js::Expression::StringLiteral(_) | js::Expression::NumericLiteral(_) | js::Expression::BooleanLiteral(_) | js::Expression::NullLiteral(_) | js::Expression::BigIntLiteral(_), _) => method != "isExtensible",
          (js::Expression::Identifier(id), _) if id.name.as_str() == "undefined" => method != "isExtensible",
          (_, Some("freeze")) => method != "isExtensible",
          (c, Some(sealing @ ("seal" | "preventExtensions"))) => {
            let js::Expression::CallExpression(c) = c else { unreachable!("made_by read a call") };
            let inner_empty = c.arguments.first().is_some_and(|a| empty(crate::syntax::arg_expr(a)));
            match method {
              "isExtensible" => false,
              "isSealed" => sealing == "seal" || inner_empty,
              _ => inner_empty,
            }
          }
          (js::Expression::ObjectExpression(_) | js::Expression::ArrayExpression(_) | js::Expression::NewExpression(_), _) => method == "isExtensible",
          _ => return Err(self.residue_with(call.span, format!("`Object.{method}` of a value the build cannot tell was frozen, sealed or kept from growing"), "a body's objects are plain data; `Object.freeze` written where the value is made says so")),
        };
        let value = self.expr(e)?;
        let held = self.temp();
        Ok(Expr::Block(vec![Stmt::Let { name: held, expr: value }, Stmt::Return(Expr::Lit(Lit::Bool(answer)))]))
      }
    }
  }

  /// `Promise.withResolvers()`: `{ promise, resolve, reject }`, the promise an object holding its state that `resolve` and `reject` overwrite and an `await` settles. A body runs to its end without waiting, so a promise read before anything resolves it is an error.
  pub(crate) fn with_resolvers(&mut self) -> Expr {
    crate::note_deferred();
    if !crate::heap_pass() {
      crate::want_heap();
    }
    let (cell, value, reason) = (self.temp(), self.temp(), self.temp());
    let var = |n: &String| Expr::Var(n.clone());
    let settle = |state: &str, field: &str, given: &String| {
      let next = Expr::Object(vec![Entry::Spread(var(&cell)), Entry::Field("$state".to_owned(), Expr::lit_str(state)), Entry::Field(field.to_owned(), var(given))]);
      Expr::Lambda { params: vec![given.clone()], body: Box::new(Expr::Builtin { name: Builtin::Overwrite, args: vec![var(&cell), next] }) }
    };
    let made = Expr::Object(vec![Entry::Field(typed::DEFERRED.to_owned(), Expr::Lit(Lit::Bool(true))), Entry::Field("$state".to_owned(), Expr::lit_str("pending"))]);
    let answer = Expr::Object(vec![
      Entry::Field("promise".to_owned(), var(&cell)),
      Entry::Field("resolve".to_owned(), settle("fulfilled", "value", &value)),
      Entry::Field("reject".to_owned(), settle("rejected", "reason", &reason)),
    ]);
    Expr::Block(vec![Stmt::Let { name: cell, expr: made }, Stmt::Return(answer)])
  }
}
