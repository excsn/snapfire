use crate::*;

impl<'a> Lowerer<'a> {
  /// `expr` lowered, with the stack grown when source nests deeper than the thread's stack holds frames for.
  pub(crate) fn expr(&mut self, expr: &js::Expression) -> Lowered<Expr> {
    snapfire_fsr_ir::on_enough_stack(|| self.expr_on_stack(expr))
  }

  fn expr_on_stack(&mut self, expr: &js::Expression) -> Lowered<Expr> {
    if !self.premade.is_empty() {
      let span = expr.span();
      if let Some(made) = self.premade.get(&(crate::lo(span), crate::hi(span))) {
        return Ok(made.clone());
      }
    }
    if self.object_write(expr) {
      if heap_pass() {
        return self.heap_write(expr);
      }
      want_heap();
    }
    if heap_pass() {
      if let js::Expression::CallExpression(call) = crate::meaning::bare(expr) {
        {
      let callee = &call.callee;
          if let Some(m) = (crate::meaning::bare(callee)).as_member_expression() {
            if self.member_name(m).as_deref() == Some("forEach") && call.arguments.len() == 1 {
              let kind = self.kind_of(m.object()).map(Kind::collection);
              let over = self.expr(m.object())?;
              let f = self.expr(&crate::syntax::arg_expr(&call.arguments[0]))?;
              let (ran, pair) = (self.temp(), self.temp());
              let at = |i: f64| Expr::Index(Box::new(Expr::Var(pair.clone())), Box::new(Expr::Lit(Lit::Float(i))));
              // A `Map` and a `URLSearchParams` call back with the value then the key, a `Set` with the value twice.
              let (items, callback) = match kind {
                Some(Kind::Map) => (Expr::Entries(Box::new(over)), Expr::Lambda { params: vec![pair.clone()], body: Box::new(Expr::Apply { f: Box::new(f), args: vec![at(1.0), at(0.0)] }) }),
                Some(Kind::Query) => (Expr::Builtin { name: Builtin::FormEntries, args: vec![over] }, Expr::Lambda { params: vec![pair.clone()], body: Box::new(Expr::Apply { f: Box::new(f), args: vec![at(1.0), at(0.0)] }) }),
                Some(Kind::Set) => (over, Expr::Lambda { params: vec![pair.clone()], body: Box::new(Expr::Apply { f: Box::new(f), args: vec![Expr::Var(pair.clone()), Expr::Var(pair.clone())] }) }),
                _ => (over, f),
              };
              return Ok(Expr::Block(vec![Stmt::Let { name: ran, expr: Expr::Map(Box::new(items), Box::new(callback)) }, Stmt::Return(Expr::Lit(Lit::Undefined))]));
            }
          }
        }
      }
    }
    match expr {
      js::Expression::ParenthesizedExpression(p) => self.expr(&p.expression),
      js::Expression::AwaitExpression(a) => {
        let awaited = self.expr(&a.argument)?;
        Ok(if crate::made_deferred() { Expr::Builtin { name: Builtin::Settle, args: vec![awaited] } } else { awaited })
      }
      js::Expression::CallExpression(_) if self.promise_all(expr).is_some() => {
        let list = self.promise_all(expr).expect("checked by the guard");
        self.expr(list)
      }
      js::Expression::TSAsExpression(a) => self.expr(&a.expression),
      js::Expression::TSNonNullExpression(a) => self.expr(&a.expression),
      js::Expression::TSSatisfiesExpression(a) => self.expr(&a.expression),
      js::Expression::TSTypeAssertion(a) => self.expr(&a.expression),

      js::Expression::Identifier(id) => self.ident(id),
      lit if lit.is_literal() => self.lit(lit),
      js::Expression::TaggedTemplateExpression(tagged) if (crate::meaning::bare(&tagged.tag)).as_member_expression().is_some_and(|m| matches!(m.object(), js::Expression::Identifier(id) if id.name.as_str() == "String" && self.parsed.ctxt(id) == crate::Ctxt::Unresolved) && self.member_name(m).as_deref() == Some("raw")) => {
        // `String.raw` writes each piece as it stands in the source, escapes and all.
        let mut parts = Vec::new();
        for (i, quasi) in tagged.quasi.quasis.iter().enumerate() {
          if !quasi.value.raw.is_empty() {
            parts.push(Expr::Lit(Lit::Str(quasi.value.raw.to_string())));
          }
          if let Some(e) = tagged.quasi.expressions.get(i) {
            parts.push(self.expr(e)?);
          }
        }
        Ok(Expr::Template(parts))
      }
      js::Expression::TemplateLiteral(tpl) => {
        let mut parts = Vec::new();
        for (i, quasi) in tpl.quasis.iter().enumerate() {
          let text = quasi.value.cooked.map(crate::lossy).unwrap_or_else(|| quasi.value.raw.to_string());
          if !text.is_empty() {
            parts.push(Expr::Lit(Lit::Str(text)));
          }
          if let Some(e) = tpl.expressions.get(i) {
            parts.push(self.as_text(e)?);
          }
        }
        Ok(Expr::Template(parts))
      }
      js::Expression::ObjectExpression(obj) => {
        let mut entries = Vec::new();
        for prop in &obj.properties {
          entries.push(match prop {
            js::ObjectPropertyKind::SpreadProperty(s) => Entry::Spread(self.expr(&s.argument)?),
            js::ObjectPropertyKind::ObjectProperty(p) => match crate::syntax::prop_of(p) {
              crate::syntax::Prop::Shorthand(id) => Entry::Field(id.name.to_string(), self.ident(id)?),
              crate::syntax::Prop::KeyValue(kv) => match kv.key.as_expression() {
                Some(key) if kv.computed => Entry::Computed(self.as_text(key)?, self.expr(&kv.value)?),
                _ => {
                  let key = prop_name(&kv.key, kv.computed).ok_or_else(|| self.residue(kv.key.span(), "a numeric property name"))?;
                  Entry::Field(key, self.expr(&kv.value)?)
                }
              },
              other => return Err(self.residue(other.span(), "an accessor in an object literal")),
            },
          });
        }
        Ok(Expr::Object(entries))
      }
      js::Expression::ArrayExpression(arr) => {
        let mut entries = Vec::new();
        for elem in &arr.elements {
          let (spread, item) = match elem {
            js::ArrayExpressionElement::Elision(_) => continue,
            js::ArrayExpressionElement::SpreadElement(s) => (true, &s.argument),
            other => (false, other.to_expression()),
          };
          if let (true, Some(length)) = (spread, self.array_of_length(item)) {
            let range = Box::new(Expr::Builtin { name: Builtin::Range, args: vec![self.expr(length)?] });
            entries.push(Entry::Spread(Expr::Map(range, Box::new(Expr::Lambda { params: vec!["$hole".to_owned()], body: Box::new(Expr::Lit(Lit::Undefined)) }))));
            continue;
          }
          let value = self.expr(item)?;
          entries.push(match spread {
            true if self.kind_of(item) == Some(Kind::Map) => Entry::Spread(Expr::Entries(Box::new(value))),
            true => Entry::Spread(value),
            false => Entry::Item(value),
          });
        }
        Ok(Expr::Array(entries))
      }
      js::Expression::UnaryExpression(u) => match u.operator {
        js::UnaryOperator::LogicalNot => Ok(Expr::Not(Box::new(self.expr(&u.argument)?))),
        js::UnaryOperator::UnaryNegation => match &u.argument {
          js::Expression::NumericLiteral(n) => Ok(Expr::Lit(Lit::Float(-n.value))),
          js::Expression::BigIntLiteral(b) => Ok(Expr::Lit(Lit::Int(-self.bigint(b)?))),
          other => Ok(Expr::Arith(ArithOp::Sub, Box::new(Expr::Lit(Lit::Float(0.0))), Box::new(self.expr(other)?))),
        },
        js::UnaryOperator::UnaryPlus => Ok(Expr::Num(Box::new(self.as_primitive(&u.argument)?))),
        js::UnaryOperator::BitwiseNot if self.kind_of(&u.argument) == Some(Kind::BigInt) => Err(self.residue(u.span, "the `~` operator on a `bigint`")),
        js::UnaryOperator::BitwiseNot => Ok(Expr::Arith(ArithOp::BitXor, Box::new(self.expr(&u.argument)?), Box::new(Expr::Lit(Lit::Float(-1.0))))),
        js::UnaryOperator::Delete => Err(self.residue(u.span, "`delete` inside an expression")),
        js::UnaryOperator::Typeof if matches!(&u.argument, js::Expression::Identifier(id) if self.unbound_name(id)) => {
          let js::Expression::Identifier(id) = &u.argument else { unreachable!("matched") };
          let name = id.name.as_str();
          if STANDARD_FUNCTIONS.contains(&name) {
            return Ok(Expr::lit_str("function"));
          }
          if STANDARD_OBJECTS.contains(&name) {
            return Ok(Expr::lit_str("object"));
          }
          if BROWSER_GLOBALS.contains(&name) && self.on_render_path() && !self.reaches(Reach::Terminal) {
            return Err(self.browser_only(u.span, format!("`typeof {name}`, which the server and the browser answer differently")));
          }
          Ok(Expr::lit_str("undefined"))
        }
        js::UnaryOperator::Typeof => Ok(match self.kind_of(&u.argument) {
          Some(Kind::BigInt) => Expr::lit_str("bigint"),
          Some(Kind::Map | Kind::Set | Kind::Date | Kind::Query | Kind::Url | Kind::Response | Kind::Scope | Kind::Temporal | Kind::WeakMap | Kind::WeakSet | Kind::WeakRef | Kind::Registry | Kind::Wrapper | Kind::Buffer | Kind::View | Kind::Stack | Kind::Proxy | Kind::Locale | Kind::Typed | Kind::Iterator | Kind::Error | Kind::RegExp | Kind::Request) => Expr::lit_str("object"),
          Some(Kind::Symbol) => Expr::lit_str("symbol"),
          Some(Kind::Browser) | None => Expr::Builtin { name: Builtin::TypeOf, args: vec![self.expr(&u.argument)?] },
        }),
        other => Err(self.residue(u.span, format!("the `{}` operator", other.as_str()))),
      },
      js::Expression::BinaryExpression(bin) if bin.operator == js::BinaryOperator::Instanceof => self.instance_of(bin),
      js::Expression::BinaryExpression(bin) if is_bitwise(bin.operator) && (self.kind_of(&bin.left) == Some(Kind::BigInt) || self.kind_of(&bin.right) == Some(Kind::BigInt)) => Err(self.residue(bin.span, format!("the `{}` operator on a `bigint`", bin.operator.as_str()))),
      js::Expression::LogicalExpression(logic) => {
        let l = Box::new(self.expr(&logic.left)?);
        let r = Box::new(self.expr(&logic.right)?);
        Ok(match logic.operator {
          js::LogicalOperator::And => Expr::Logic(LogicOp::And, l, r),
          js::LogicalOperator::Or => Expr::Logic(LogicOp::Or, l, r),
          js::LogicalOperator::Coalesce => Expr::Coalesce(l, r),
        })
      }
      js::Expression::BinaryExpression(bin) => {
        let collection = |this: &Self, e: &js::Expression| matches!(this.kind_of(e).map(Kind::collection), Some(Kind::Map | Kind::Set));
        let two_objects = matches!(bin.operator, js::BinaryOperator::Equality | js::BinaryOperator::Inequality) && collection(self, &bin.left) && collection(self, &bin.right);
        let converts = !two_objects && matches!(bin.operator, js::BinaryOperator::Addition | js::BinaryOperator::Subtraction | js::BinaryOperator::Multiplication | js::BinaryOperator::Division | js::BinaryOperator::Remainder | js::BinaryOperator::Exponential | js::BinaryOperator::Equality | js::BinaryOperator::Inequality | js::BinaryOperator::LessThan | js::BinaryOperator::LessEqualThan | js::BinaryOperator::GreaterThan | js::BinaryOperator::GreaterEqualThan);
        let text = matches!(bin.operator, js::BinaryOperator::Addition | js::BinaryOperator::Equality | js::BinaryOperator::Inequality);
        let l = Box::new(match (converts, text) {
          (true, true) => self.as_text(&bin.left)?,
          (true, false) => self.as_primitive(&bin.left)?,
          _ => self.expr(&bin.left)?,
        });
        let r = Box::new(match (converts, text) {
          (true, true) => self.as_text(&bin.right)?,
          (true, false) => self.as_primitive(&bin.right)?,
          _ => self.expr(&bin.right)?,
        });
        Ok(match bin.operator {
          js::BinaryOperator::Addition => Expr::Arith(ArithOp::Add, l, r),
          js::BinaryOperator::Subtraction => Expr::Arith(ArithOp::Sub, l, r),
          js::BinaryOperator::Multiplication => Expr::Arith(ArithOp::Mul, l, r),
          js::BinaryOperator::Division if self.kind_of(&bin.left) == Some(Kind::BigInt) || self.kind_of(&bin.right) == Some(Kind::BigInt) => Expr::Arith(ArithOp::Quot, l, r),
          js::BinaryOperator::Division => Expr::Arith(ArithOp::Div, l, r),
          js::BinaryOperator::Remainder => Expr::Arith(ArithOp::Rem, l, r),
          js::BinaryOperator::Exponential => Expr::Arith(ArithOp::Pow, l, r),
          js::BinaryOperator::Equality | js::BinaryOperator::Inequality if matches!(*r, Expr::Lit(Lit::Null | Lit::Undefined)) || matches!(*l, Expr::Lit(Lit::Null | Lit::Undefined)) => {
            let other = if matches!(*r, Expr::Lit(Lit::Null | Lit::Undefined)) { l } else { r };
            let nullish = Expr::Builtin { name: Builtin::IsNullish, args: vec![*other] };
            if bin.operator == js::BinaryOperator::Equality { nullish } else { Expr::Not(Box::new(nullish)) }
          }
          js::BinaryOperator::Equality => Expr::Builtin { name: Builtin::LooseEq, args: vec![*l, *r] },
          js::BinaryOperator::Inequality => Expr::Not(Box::new(Expr::Builtin { name: Builtin::LooseEq, args: vec![*l, *r] })),
          js::BinaryOperator::StrictEquality => Expr::Compare(CompareOp::Eq, l, r),
          js::BinaryOperator::StrictInequality => Expr::Compare(CompareOp::Ne, l, r),
          js::BinaryOperator::LessThan => Expr::Compare(CompareOp::Lt, l, r),
          js::BinaryOperator::LessEqualThan => Expr::Compare(CompareOp::Le, l, r),
          js::BinaryOperator::GreaterThan => Expr::Compare(CompareOp::Gt, l, r),
          js::BinaryOperator::GreaterEqualThan => Expr::Compare(CompareOp::Ge, l, r),
          js::BinaryOperator::In if self.kind_of(&bin.right) == Some(Kind::Proxy) => self.proxy_trap("has", *r, vec![Expr::Str(l)], |target, args| Expr::Builtin { name: Builtin::HasKey, args: vec![target, args[0].clone()] }),
          js::BinaryOperator::In => Expr::Builtin { name: Builtin::HasKey, args: vec![*r, *l] },
          js::BinaryOperator::BitwiseAnd => Expr::Arith(ArithOp::BitAnd, l, r),
          js::BinaryOperator::BitwiseOR => Expr::Arith(ArithOp::BitOr, l, r),
          js::BinaryOperator::BitwiseXOR => Expr::Arith(ArithOp::BitXor, l, r),
          js::BinaryOperator::ShiftLeft => Expr::Arith(ArithOp::Shl, l, r),
          js::BinaryOperator::ShiftRight => Expr::Arith(ArithOp::Shr, l, r),
          js::BinaryOperator::ShiftRightZeroFill => Expr::Arith(ArithOp::UShr, l, r),
          other => return Err(self.residue(bin.span, format!("the `{}` operator", other.as_str()))),
        })
      }
      js::Expression::ConditionalExpression(c) => Ok(Expr::Ternary(
        Box::new(self.expr(&c.test)?),
        Box::new(self.expr(&c.consequent)?),
        Box::new(self.expr(&c.alternate)?),
      )),
      member_expr if member_expr.is_member_expression() => { let member = member_expr.as_member_expression().expect("matched as a member"); self.member(member) },
      js::Expression::CallExpression(call) => self.call(call),
      js::Expression::ArrowFunctionExpression(arrow) => self.lambda(arrow),
      js::Expression::ChainExpression(o) => match &o.expression {
        js::ChainElement::CallExpression(call) if call.optional => Err(self.residue_with(call.span, "an optional call of a function value", "write `f ? f(x) : null` or call a helper the build can follow")),
        js::ChainElement::CallExpression(call) => self.optional_call(call),
        js::ChainElement::TSNonNullExpression(e) => self.expr(&e.expression),
        member => self.member(member.as_member_expression().expect("a chain element is a call, a `!` or a member")),
      },
      js::Expression::NewExpression(n) => self.new_expr(n),
      js::Expression::FunctionExpression(f) => self.function_expr(f),
      js::Expression::AssignmentExpression(a) if self.cell_of_target(&a.left).is_some() => match self.effect(expr)? {
        Stmt::Expr(write) => Ok(write),
        _ => Err(self.residue(a.span, "an assignment inside an expression")),
      },
      js::Expression::AssignmentExpression(a) => Err(self.residue(a.span, "an assignment inside an expression")),
      js::Expression::ThisExpression(_) if self.scope.iter().any(|(n, _)| n == "this") => Ok(self.scope.iter().rev().find(|(n, _)| n == "this").map(|(_, e)| e.clone()).expect("checked")),
      other => {
        let (what, hint) = describe_expr(other);
        Err(self.residue_with(other.span(), what, hint))
      }
    }
  }

  /// A residue for a value only the browser knows, marked so a consumer that can leave it to the browser lowers null in its place.
  pub(crate) fn browser_only(&mut self, span: Span, reason: impl Into<String>) -> Residue {
    let reason = reason.into();
    self.browser_value = Some(reason.clone());
    self.residue_with(span, reason, "as text under an element the browser fills it in when the component hydrates; anywhere else compute it on the server, from `ctx.now` in the loader or a prop")
  }

  pub(crate) fn ident(&mut self, id: &js::IdentifierReference) -> Lowered<Expr> {
    let name = id.name.as_str();
    if let Some((_, bound)) = self.scope.iter().rev().find(|(n, _)| n == name) {
      if self.formatters.iter().rev().any(|(n, held, _)| n == name && held == bound) {
        return Err(self.residue_with(id.span, format!("`{name}`, an `Intl` formatter or a text coder kept as a value"), "call its method on the name it is bound to, `formatter.format(n)`"));
      }
      if self.kinds.iter().rev().any(|(n, held, kind)| n == name && held == bound && *kind == Kind::Browser) {
        return Err(self.browser_only(id.span, format!("`{name}`, a value only the browser knows")));
      }
      return Ok(bound.clone());
    }
    if let Some((_, bound)) = self.globals.iter().rev().find(|(n, _)| n == name) {
      return Ok(bound.clone());
    }
    if self.reaches(Reach::Process) && self.meanings.as_ref().and_then(|meanings| meanings.of_ident(id)) == Some(&meaning::Meaning::Library(meaning::Construct::Suspend)) {
      return Ok(Expr::Object(vec![Entry::Field(crate::terminal::PRIMITIVE.to_owned(), Expr::lit_str("suspendTerminal"))]));
    }
    if let Some(meaning::Meaning::Library(meaning::Construct::NamedKey(key))) = self.meanings.as_ref().and_then(|meanings| meanings.of_ident(id)) {
      return Ok(Expr::lit_str(*key));
    }
    match self.root_of(id) {
      Some(Root::Input) => Ok(Expr::Input),
      Some(Root::Now) => Ok(Expr::Now),
      Some(Root::Locale) => Ok(Expr::Locale),
      Some(Root::Locales) => Ok(Expr::Locales),
      Some(Root::Path) => Ok(Expr::Path),
      Some(Root::Host) => Ok(Expr::Host),
      Some(Root::Origin) => Ok(Expr::Origin),
      Some(Root::Address) => Ok(Expr::Address),
      Some(Root::Identity) => Ok(Expr::Identity(Vec::new())),
      Some(Root::Params) => Ok(Expr::ParamsAll),
      Some(Root::Query) => Ok(Expr::QueryAll),
      Some(Root::Session | Root::Config) => Err(self.residue(id.span, format!("`{name}` as a whole; read one of its fields"))),
      Some(Root::Services) => Err(self.residue(id.span, "`services` as a value; call a method on it")),
      Some(Root::Native) => Err(self.residue(id.span, "`native` as a value; call a method on it")),
      Some(Root::Ctx) => Err(self.residue(id.span, "`ctx` as a value; read one of its fields")),
      Some(Root::Next) => Err(self.residue(id.span, "`next` as a value; call it")),
      None => match name {
        "undefined" => Ok(Expr::Lit(Lit::Undefined)),
        "Infinity" => Ok(Expr::Arith(ArithOp::Div, Box::new(Expr::Lit(Lit::Float(1.0))), Box::new(Expr::Lit(Lit::Float(0.0))))),
        "NaN" => Ok(Expr::Arith(ArithOp::Div, Box::new(Expr::Lit(Lit::Float(0.0))), Box::new(Expr::Lit(Lit::Float(0.0))))),
        _ if self.browser_functions.iter().any(|f| f == name) => Ok(Expr::Handler(snapfire_fsr_ir::render::BROWSER_HANDLER)),
        _ if self.after_body && self.declared_below.iter().any(|(declared, _)| declared == name) => {
          let (_, bound) = self.declared_below.iter().rev().find(|(declared, _)| declared == name).cloned().expect("checked");
          self.read_below.push((bound.clone(), id.span));
          Ok(Expr::Var(bound))
        }
        "process" if self.parsed.ctxt(id) == crate::Ctxt::Unresolved => Err(self.residue_with(id.span, "`process`, which is not FSR's", NOT_FSR)),
        _ => {
          self.unbound = Some(name.to_owned());
          match crate::component::find_import(self.parsed, name) {
            Some((source, _)) if node_module(&source) => Err(self.residue_with(id.span, format!("`{name}` from `{source}`, which is not FSR's"), NOT_FSR)),
            Some((source, export)) => match meaning::module_absent(&source, &export) {
              Some(why) => Err(self.residue(id.span, why)),
              None => Err(self.residue(id.span, format!("`{name}` comes from `{source}`, which the build cannot follow"))),
            },
            None => Err(self.residue(id.span, format!("`{name}` is not bound here; an import the build cannot follow or a name from outside the body"))),
          }
        }
      },
    }
  }

  /// A literal: a string, a number, a bigint, a boolean, `null` or a regular expression.
  pub(crate) fn lit(&self, lit: &js::Expression) -> Lowered<Expr> {
    Ok(Expr::Lit(match lit {
      js::Expression::StringLiteral(s) => Lit::Str(crate::lossy(s.value)),
      js::Expression::NumericLiteral(n) => Lit::Float(n.value),
      js::Expression::BigIntLiteral(b) => Lit::Int(self.bigint(b)?),
      js::Expression::BooleanLiteral(b) => Lit::Bool(b.value),
      js::Expression::NullLiteral(_) => Lit::Null,
      js::Expression::RegExpLiteral(re) => {
        let raw = re.raw.as_ref().map(|raw| raw.as_str()).unwrap_or_default();
        let flags = raw.rsplit_once('/').map(|(_, flags)| flags).unwrap_or_default();
        return regex_value(self, re.regex.pattern.text.as_str(), flags, re.span);
      }
      other => return Err(self.residue(other.span(), "a literal the build does not read")),
    }))
  }

  pub(crate) fn bigint(&self, b: &js::BigIntLiteral) -> Lowered<i128> {
    b.value
      .to_string()
      .parse::<i128>()
      .map_err(|_| self.residue(b.span, "a bigint literal outside 128 bits"))
  }

  /// A member read. Context roots become reads; anything else is a field.
  pub(crate) fn member(&mut self, member: &js::MemberExpression) -> Lowered<Expr> {
    if let Some(compare) = self.collator_compare(member)? {
      return Ok(compare);
    }
    if let (js::Expression::Identifier(id), Some(key)) = (member.object(), self.member_name(member)) {
      if id.name.as_str() == "Symbol" && !self.scope.iter().any(|(n, _)| n == "Symbol") && crate::WELL_KNOWN_SYMBOLS.contains(&key.as_str()) {
        return Ok(crate::well_known_symbol(&key));
      }
      if key == "BYTES_PER_ELEMENT" && !self.scope.iter().any(|(n, _)| n == id.name.as_str()) {
        if let Some(element) = id.name.as_str().strip_suffix("Array").filter(|_| crate::forms::TYPED_ARRAYS.contains(&id.name.as_str())) {
          return Ok(Expr::Builtin { name: Builtin::TypedField, args: vec![Expr::lit_str(element), Expr::lit_str("BYTES_PER_ELEMENT")] });
        }
      }
    }
    if let Some(class) = self.builtin_prototype(member) {
      return Ok(crate::lowerer::statics::prototype(class));
    }
    if let (js::Expression::Identifier(id), Some(key)) = (member.object(), self.member_name(member)) {
      if !self.scope.iter().any(|(n, _)| n == id.name.as_str()) && self.meanings.as_ref().and_then(|m| m.of_ident(id)) == Some(&crate::meaning::Meaning::Config(None)) {
        return Ok(Expr::Config(key));
      }
    }
    if let Some(read) = self.class_static_read(member)? {
      return Ok(read);
    }

    let prop = |this: &mut Self| -> Lowered<Result<String, Expr>> {
      Ok(match crate::syntax::prop(member) {
        crate::syntax::MemberProp::Ident(id) => Ok(id.name.to_string()),
        crate::syntax::MemberProp::Computed(c) => match c {
          js::Expression::StringLiteral(s) => Ok(crate::lossy(s.value)),
          other => Err(this.expr(other)?),
        },
        crate::syntax::MemberProp::Private(p) => return Err(this.residue(p.span, "a private name")),
      })
    };

    if let js::Expression::Identifier(id) = member.object() {
      if id.name.as_str() == "Math" && !self.scope.iter().any(|(n, _)| n == "Math") && !self.globals.iter().any(|(n, _)| n == "Math") {
        if let crate::syntax::MemberProp::Ident(name) = crate::syntax::prop(member) {
          use std::f64::consts;
          let value = match name.name.as_str() {
            "PI" => Some(consts::PI),
            "E" => Some(consts::E),
            "LN2" => Some(consts::LN_2),
            "LN10" => Some(consts::LN_10),
            "LOG2E" => Some(consts::LOG2_E),
            "LOG10E" => Some(consts::LOG10_E),
            "SQRT2" => Some(consts::SQRT_2),
            "SQRT1_2" => Some(consts::FRAC_1_SQRT_2),
            _ => None,
          };
          if let Some(value) = value {
            return Ok(Expr::Lit(Lit::Float(value)));
          }
        }
      }
      if id.name.as_str() == "Number" && !self.scope.iter().any(|(n, _)| n == "Number") && !self.globals.iter().any(|(n, _)| n == "Number") {
        if let crate::syntax::MemberProp::Ident(name) = crate::syntax::prop(member) {
          let divide = |a: f64, b: f64| Expr::Arith(ArithOp::Div, Box::new(Expr::Lit(Lit::Float(a))), Box::new(Expr::Lit(Lit::Float(b))));
          let value = match name.name.as_str() {
            "MAX_SAFE_INTEGER" => Some(Expr::Lit(Lit::Float(9_007_199_254_740_991.0))),
            "MIN_SAFE_INTEGER" => Some(Expr::Lit(Lit::Float(-9_007_199_254_740_991.0))),
            "EPSILON" => Some(Expr::Lit(Lit::Float(f64::EPSILON))),
            "MAX_VALUE" => Some(Expr::Lit(Lit::Float(f64::MAX))),
            "MIN_VALUE" => Some(Expr::Lit(Lit::Float(5e-324))),
            "POSITIVE_INFINITY" => Some(divide(1.0, 0.0)),
            "NEGATIVE_INFINITY" => Some(divide(-1.0, 0.0)),
            "NaN" => Some(divide(0.0, 0.0)),
            _ => None,
          };
          if let Some(value) = value {
            return Ok(value);
          }
        }
      }
      if !self.scope.iter().any(|(n, _)| n == id.name.as_str()) {
        match self.root_of(id) {
          Some(Root::Params) => {
            let Ok(name) = prop(self)? else { return Err(self.residue(member.span(), "a computed param name")) };
            return Ok(Expr::Param(name));
          }
          Some(Root::Query) => {
            let Ok(name) = prop(self)? else { return Err(self.residue(member.span(), "a computed query key")) };
            return Ok(Expr::Query(name));
          }
          Some(Root::Session) => {
            let Ok(name) = prop(self)? else { return Err(self.residue(member.span(), "a computed session key")) };
            return Ok(self.session_read(name));
          }
          Some(Root::Identity) => {
            let Ok(name) = prop(self)? else { return Err(self.residue(member.span(), "a computed identity field")) };
            return Ok(Expr::Identity(vec![name]));
          }
          Some(Root::Config) => {
            let Ok(name) = prop(self)? else { return Err(self.residue(member.span(), "a computed config key")) };
            return Ok(Expr::Config(name));
          }
          Some(Root::Ctx) => {
            let Ok(name) = prop(self)? else { return Err(self.residue(member.span(), "a computed context field")) };
            return match name.as_str() {
              "input" => Ok(Expr::Input),
              "now" => Ok(Expr::Now),
              "locale" => Ok(Expr::Locale),
              "locales" => Ok(Expr::Locales),
              "path" => Ok(Expr::Path),
              "host" => Ok(Expr::Host),
              "origin" => Ok(Expr::Origin),
              "address" => Ok(Expr::Address),
              "identity" => Ok(Expr::Identity(Vec::new())),
              "params" | "query" | "session" | "config" => Err(self.residue(member.span(), format!("`ctx.{name}` as a whole; read one of its fields"))),
              "services" => Err(self.residue(member.span(), "`ctx.services` as a value; call a method on it")),
              "native" => Err(self.residue(member.span(), "`ctx.native` as a value; call a method on it")),
              _ => Err(self.residue(member.span(), format!("`{name}` is not a field of the context"))),
            };
          }
          Some(Root::Services) => return Err(self.residue(member.span(), "a service as a value; call a method on it")),
          Some(Root::Native) => return Err(self.residue(member.span(), "a native module as a value; call a method on it")),
          _ => {}
        }
      }
    }

    if let Some(inner) = (member.object()).as_member_expression() {
      if let js::Expression::Identifier(id) = inner.object() {
        if !self.scope.iter().any(|(n, _)| n == id.name.as_str()) {
          if let Some(Root::Ctx) = self.root_of(id) {
            let via = self.member_name(inner);
            let Ok(name) = prop(self)? else { return Err(self.residue(member.span(), "a computed context field")) };
            match via.as_deref() {
              Some("params") => return Ok(Expr::Param(name)),
              Some("query") => return Ok(Expr::Query(name)),
              Some("session") => return Ok(self.session_read(name)),
              Some("identity") => return Ok(Expr::Identity(vec![name])),
              _ => {}
            }
          }
          if let Some(Root::Identity) = self.root_of(id) {
            let Some(first) = self.member_name(inner) else { return Err(self.residue(inner.span(), "a computed identity field")) };
            let Ok(name) = prop(self)? else { return Err(self.residue(member.span(), "a computed identity field")) };
            return Ok(Expr::Identity(vec![first, name]));
          }
        }
      }
    }

    if self.member_kind(member) == Some(Kind::Browser) {
      let shown = match (member.object(), crate::syntax::prop(member)) {
        (js::Expression::Identifier(obj), crate::syntax::MemberProp::Ident(prop)) => format!("`{}.{}`", obj.name, prop.name),
        _ => "a member".to_owned(),
      };
      return Err(self.browser_only(member.span(), format!("{shown}, a value only the browser knows")));
    }
    if let Some(made) = self.formatter_of(member.object()) {
      let coder = platform_object(made);
      if matches!(coder, Some("TextEncoder" | "TextDecoder")) {
        // A decoder with a label other than UTF-8 or with options is refused where it is made, so these are what every one the build takes reads.
        return match (coder, self.member_name(member).as_deref()) {
          (_, Some("encoding")) => Ok(Expr::lit_str("utf-8")),
          (Some("TextDecoder"), Some("fatal" | "ignoreBOM")) => Ok(Expr::Lit(Lit::Bool(false))),
          (_, name) => Err(self.residue(member.span(), format!("`.{}` of a `{}`", name.unwrap_or("[computed]"), coder.unwrap_or_default()))),
        };
      }
    }
    if self.kind_of(member.object()) == Some(Kind::Symbol) && self.member_name(member).as_deref() == Some("description") {
      let symbol = self.expr(member.object())?;
      return Ok(crate::symbol_description(symbol));
    }
    if self.member_name(member).as_deref() == Some("__proto__") {
      let b = self.build();
      let ast = b.ast();
      let span = member.span();
      let callee = js::Expression::new_static_member_expression(span, b.ident("Object", span), js::IdentifierName::new(span, b.str("getPrototypeOf"), &ast), false, &ast);
      let call = js::Expression::new_call_expression(span, callee, None, b.vec([js::Argument::from(b.copy(member.object()))]), false, &ast);
      return self.expr(b.alloc(call));
    }
    if let Some(name @ ("name" | "length" | "caller" | "arguments")) = self.member_name(member).as_deref() {
      if let Some(read) = self.function_property(member.object(), name) {
        return Ok(read);
      }
    }
    let collection = self.kind_of(member.object()).filter(|k| *k != Kind::Wrapper);
    let target = self.unwrapped(member.object())?;
    if collection == Some(Kind::Response) {
      return match prop(self)? {
        Ok(name) if matches!(name.as_str(), "status" | "ok") => Ok(Expr::Builtin { name: Builtin::ResponseMember, args: vec![target, Expr::lit_str(&name)] }),
        Ok(name) => Err(self.residue(member.span(), format!("`.{name}` of a `Response`; `status`, `ok` and `headers.get` are read"))),
        Err(_) => Err(self.residue(member.span(), "a computed member of a `Response`")),
      };
    }
    if collection == Some(Kind::Proxy) {
      let key = match prop(self)? {
        Ok(name) => Expr::lit_str(&name),
        Err(computed) => Expr::Str(Box::new(computed)),
      };
      return Ok(self.proxy_trap("get", target, vec![key], |target, args| Expr::Index(Box::new(target), Box::new(args[0].clone()))));
    }
    if matches!(collection, Some(Kind::Buffer | Kind::View)) {
      if let Some(name) = self.member_name(member) {
        match (collection, name.as_str()) {
          (_, "byteLength") => return Ok(Expr::Length(Box::new(target))),
          (Some(Kind::View), "buffer") => return Ok(target),
          (Some(Kind::View), "byteOffset") => return Ok(Expr::Lit(Lit::Float(0.0))),
          _ => {}
        }
      }
    }
    if collection == Some(Kind::Temporal) {
      return match prop(self)? {
        Ok(name) => Ok(Expr::Ext { module: "temporal".to_owned(), name: "get".to_owned(), args: vec![target, Expr::lit_str(&name)] }),
        Err(_) => Err(self.residue(member.span(), "a computed field of a Temporal value")),
      };
    }
    if collection == Some(Kind::Url) {
      return match prop(self)? {
        Ok(name) if name == "href" => Ok(target),
        Ok(name) if name == "searchParams" => Ok(Expr::Builtin { name: Builtin::UrlPart, args: vec![target, Expr::lit_str("search")] }),
        Ok(name) if URL_PARTS.contains(&name.as_str()) => Ok(Expr::Builtin { name: Builtin::UrlPart, args: vec![target, Expr::lit_str(&name)] }),
        Ok(name) => Err(self.residue(member.span(), format!("`.{name}` of a `URL`"))),
        Err(_) => Err(self.residue(member.span(), "a computed part of a `URL`")),
      };
    }
    match prop(self)? {
      Ok(name) if name == "length" => Ok(Expr::Length(Box::new(target))),
      Ok(name) if name == "size" && matches!(collection.map(Kind::collection), Some(Kind::Map | Kind::Set)) => Ok(Expr::Length(Box::new(target))),
      Ok(name) if collection == Some(Kind::Typed) && matches!(name.as_str(), "byteLength" | "byteOffset" | "BYTES_PER_ELEMENT") => Ok(Expr::Builtin { name: Builtin::TypedField, args: vec![target, Expr::lit_str(&name)] }),
      Ok(name) if collection.is_some_and(|kind| crate::standard::unread(kind, &name)) => Err(self.residue(member.span(), format!("`.{name}` of {}, which the build does not read", collection.expect("checked").described()))),
      Ok(name) if collection.is_none() && self.may_be_getter(&name) && !crate::classes::plain_root(&target) => {
        let t = self.temp();
        Ok(crate::classes::getter_dispatch(t, target, &name))
      }
      Ok(name) => Ok(Expr::Field(Box::new(target), name)),
      Err(key) => Ok(Expr::Index(Box::new(target), Box::new(key))),
    }
  }

  /// `use(X)` or `useContext(X)`, however the call names them, with `X` a context: the nearest provider's value, else the default. `None` for any other call.
  pub(crate) fn context_read(&mut self, call: &js::CallExpression, callee: &js::Expression) -> Lowered<Option<Expr>> {
    let Some(meanings) = self.meanings.clone() else { return Ok(None) };
    let Some(hook) = meanings.of_expr(callee).and_then(|meant| meant.hook()) else { return Ok(None) };
    if hook != "use" && hook != "useContext" {
      return Ok(None);
    }
    match call.arguments.first().and_then(|a| meanings.of_expr(crate::syntax::arg_expr(a))) {
      Some(meaning::Meaning::Context { id, default, .. }) => Ok(Some(Expr::Coalesce(Box::new(Expr::Context(id)), Box::new(default)))),
      _ => Err(match hook {
        "use" => self.residue(call.span, "`use` of something other than a `createContext` value; a promise is awaited in the browser"),
        _ => self.residue(call.span, "`useContext` of something the build cannot follow to a `createContext`"),
      }),
    }
  }

  /// The length of `Array(n)` or `new Array(n)` with `Array` the global.
  pub(crate) fn array_of_length<'s, 'e>(&self, expr: &'s js::Expression<'e>) -> Option<&'s js::Expression<'e>> {
    let (callee, args): (&'s js::Expression<'e>, &'s [js::Argument<'e>]) = match strip_parens(expr) {
      js::Expression::CallExpression(call) => (&call.callee, &call.arguments[..]),
      js::Expression::NewExpression(made) => (&made.callee, &made.arguments[..]),
      _ => return None,
    };
    let js::Expression::Identifier(id) = strip_parens(callee) else { return None };
    let global = self.meanings.as_ref().is_none_or(|meanings| meanings.ctxt(id) == Ctxt::Unresolved);
    match args {
      [length] if id.name.as_str() == "Array" && global && !crate::syntax::is_spread(length) => Some(crate::syntax::arg_expr(length)),
      _ => None,
    }
  }

  /// What React's server renderer returns for a hook whose value does not wait on the browser: `useDeferredValue`'s initial value when given and its value otherwise, `useFormStatus`'s status of no form pending, `useLinkStatus`'s of no link pending and `useSyncExternalStore`'s server snapshot.
  pub(crate) fn server_hook_value(&mut self, hook: &str, call: &js::CallExpression) -> Lowered<Expr> {
    match hook {
      "useDeferredValue" => match call.arguments.get(1).or(call.arguments.first()) {
        Some(a) => self.expr(crate::syntax::arg_expr(a)),
        None => Ok(Expr::Lit(Lit::Null)),
      },
      "useFormStatus" => Ok(Expr::Object(["pending", "data", "method", "action"].map(|key| Entry::Field(key.to_owned(), Expr::Lit(if key == "pending" { Lit::Bool(false) } else { Lit::Null }))).to_vec())),
      "useLinkStatus" => Ok(Expr::Object(vec![Entry::Field("pending".to_owned(), Expr::Lit(Lit::Bool(false)))])),
      _ => match call.arguments.get(2) {
        Some(server) => Ok(Expr::Apply { f: Box::new(self.expr(crate::syntax::arg_expr(server))?), args: Vec::new() }),
        None => Err(self.residue(call.span, "`useSyncExternalStore` without `getServerSnapshot`, which React's server renderer refuses too")),
      },
    }
  }
}

/// The standard constructors and functions `typeof` answers `"function"` for where nothing binds their name.
const STANDARD_FUNCTIONS: &[&str] = &["Object", "Function", "Array", "Number", "Boolean", "String", "Symbol", "BigInt", "Date", "Promise", "RegExp", "Error", "AggregateError", "SuppressedError", "EvalError", "RangeError", "ReferenceError", "SyntaxError", "TypeError", "URIError", "Map", "Set", "WeakMap", "WeakSet", "WeakRef", "FinalizationRegistry", "ArrayBuffer", "SharedArrayBuffer", "DataView", "Int8Array", "Uint8Array", "Uint8ClampedArray", "Int16Array", "Uint16Array", "Int32Array", "Uint32Array", "Float16Array", "Float32Array", "Float64Array", "BigInt64Array", "BigUint64Array", "Proxy", "Iterator", "DisposableStack", "AsyncDisposableStack", "URL", "URLSearchParams", "TextEncoder", "TextDecoder", "Response", "Request", "Headers", "parseInt", "parseFloat", "isNaN", "isFinite", "encodeURI", "encodeURIComponent", "decodeURI", "decodeURIComponent", "structuredClone", "atob", "btoa", "fetch", "queueMicrotask", "setTimeout", "clearTimeout", "setInterval", "clearInterval"];

/// The standard namespace objects `typeof` answers `"object"` for where nothing binds their name.
const STANDARD_OBJECTS: &[&str] = &["Math", "JSON", "Reflect", "Intl", "Atomics", "globalThis", "Temporal", "console", "crypto"];

/// The globals a browser has and the server does not, which `typeof` reads as `"undefined"` in a body.
const BROWSER_GLOBALS: &[&str] = &["window", "document", "navigator", "localStorage", "sessionStorage", "location", "history", "self", "screen", "matchMedia", "requestAnimationFrame", "customElements", "HTMLElement", "Element", "Node", "Event", "CustomEvent", "IntersectionObserver", "ResizeObserver", "MutationObserver"];

impl Lowerer<'_> {
  /// Whether nothing in the program binds `id`'s name: no local, module name, root or import.
  pub(crate) fn unbound_name(&self, id: &js::IdentifierReference) -> bool {
    let name = id.name.as_str();
    !self.scope.iter().any(|(n, _)| n == name) && !self.globals.iter().any(|(n, _)| n == name) && self.root_of(id).is_none() && !self.cells.iter().any(|(n, _)| n == name) && matches!(self.parsed.ctxt(id), crate::Ctxt::Unresolved | crate::Ctxt::Written)
  }
}
