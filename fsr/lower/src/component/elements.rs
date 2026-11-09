use super::*;

impl<'a, 'p> ComponentLowerer<'a, 'p> {
  /// `as_child` says the element sits directly among JSX children, where a
  /// rewrite of it must be braced, rather than in an expression.
  pub(super) fn element(&mut self, el: &'p js::JSXElement<'p>, as_child: bool) -> Lowered<Tmpl> {
    let tagged = self.lowerer.meanings.as_ref().and_then(|meanings| meanings.of_tag(&el.opening_element.name));
    let provider = tagged.as_ref().and_then(|meant| meant.as_provider().map(|(id, react)| (id.to_owned(), react)));
    let named_provider = match &el.opening_element.name {
      js::JSXElementName::MemberExpression(m) if m.property.name.as_str() == "Provider" => Some(jsx_object_name(&m.object)),
      _ => None,
    };
    if provider.is_some() || named_provider.is_some() {
      let loc = self.lowerer.parsed.loc(el.span.start);
      let shown = named_provider.unwrap_or_else(|| jsx_tag_name(&el.opening_element.name));
      self.providers.push((shown, loc, provider.as_ref().map(|(_, react)| *react)));
      let children = Tmpl::Fragment(self.children(&el.children)?);
      let Some((id, _)) = provider else { return Ok(children) };
      let mut value = Expr::Lit(Lit::Null);
      for attr in &el.opening_element.attributes {
        if let js::JSXAttributeItem::Attribute(attr) = attr {
          if attr_name(&attr.name) == "value" {
            value = self.attr_value(attr)?;
          }
        }
      }
      return Ok(Tmpl::Let { name: format!("{CONTEXT_PREFIX}{id}"), expr: value, then: Box::new(children) });
    }
    let (name, member) = match &el.opening_element.name {
      js::JSXElementName::IdentifierReference(id) => (id.name.to_string(), false),
      js::JSXElementName::Identifier(id) => (id.name.to_string(), false),
      js::JSXElementName::ThisExpression(_) => ("this".to_owned(), false),
      js::JSXElementName::MemberExpression(m) => match &m.object {
        js::JSXMemberExpressionObject::IdentifierReference(obj) => (format!("{}.{}", obj.name, m.property.name), true),
        js::JSXMemberExpressionObject::ThisExpression(_) => (format!("this.{}", m.property.name), true),
        js::JSXMemberExpressionObject::MemberExpression(_) => return Err(self.lowerer.residue(m.span, "a member expression as a tag more than one level deep")),
      },
      js::JSXElementName::NamespacedName(n) => return Err(self.lowerer.residue(n.span, "a namespaced tag")),
    };
    let chosen_tag = match &el.opening_element.name {
      js::JSXElementName::IdentifierReference(id) if !member && self.chosen.get(&name).is_none() => {
        let meanings = self.meanings();
        let local = !matches!(meanings.ctxt(id), crate::Ctxt::Top | crate::Ctxt::Unresolved);
        local.then(|| self.lowerer.scope.iter().rev().find(|(n, _)| *n == name).map(|(_, e)| e.clone())).flatten()
      }
      _ => None,
    };
    let is_component = (member || name.chars().next().is_some_and(|c| c.is_ascii_uppercase())) && chosen_tag.is_none();
    if is_component {
      return self.component_ref(&name, el, as_child);
    }
    let name = if chosen_tag.is_some() { fills::CHOSEN_TAG.to_owned() } else { name };
    if name == "img" && self.rewrite_images && self.img_names_an_asset(el) {
      return self.picture_element(el, true);
    }
    let mut attrs = Vec::new();
    let mut select_value = None;
    let mut bound = false;
    let mut escaped_asset: Option<String> = None;
    let (mut text_value, mut text_default) = (None, None);
    for attr in &el.opening_element.attributes {
      let attr = match attr {
        js::JSXAttributeItem::Attribute(attr) => attr,
        js::JSXAttributeItem::SpreadAttribute(spread) => {
          bound = true;
          attrs.push(Entry::Spread(self.lowerer.expr(&spread.argument)?));
          continue;
        }
      };
      let raw = attr_name(&attr.name);
      if raw == "ref" {
        bound = true;
        continue;
      }
      // React's own: hydration reads it, the document never does.
      if raw == "suppressHydrationWarning" {
        continue;
      }
      if name == "img" && raw == RAW_ESCAPE {
        escaped_asset = self.img_asset_path(el);
        continue;
      }
      // React 19 attaches a function an `on…` prop of a custom element holds as a listener for that event, whatever its case.
      if is_handler_name(&raw) || (name.contains('-') && raw.len() > 2 && raw.starts_with("on") && self.passes_function(attr)) {
        bound = true;
        let event = raw[2..].to_ascii_lowercase();
        if let Some(reference) = self.handler_reference(attr) {
          attrs.push(Entry::Field(format!("{HANDLER_ATTR}{event}"), reference));
          continue;
        }
        match self.handler_attr(attr) {
          Ok(index) => attrs.push(Entry::Field(format!("{HANDLER_ATTR}{event}"), Expr::Lit(Lit::Int(index as i128)))),
          Err(residue) if self.unbound_is_followable() => return Err(residue),
          Err(residue) => {
            self.lowerer.unbound = None;
            if !attrs.iter().any(|e| matches!(e, Entry::Field(n, _) if n == UNLOWERED_ATTR)) {
              attrs.push(Entry::Field(UNLOWERED_ATTR.to_owned(), Expr::lit_str(format!("{}:{}: {}", residue.line, residue.column, residue.message))));
            }
          }
        }
        continue;
      }
      if raw == "key" {
        attrs.push(Entry::Field(KEY_ATTR.to_owned(), self.attr_value(attr)?));
        continue;
      }
      if (raw == "action" || raw == "formAction") && self.passes_function(attr) {
        bound = true;
        if !attrs.iter().any(|e| matches!(e, Entry::Field(n, _) if n == UNLOWERED_ATTR)) {
          let (line, column) = self.lowerer.parsed.loc(attr.span.start);
          attrs.push(Entry::Field(UNLOWERED_ATTR.to_owned(), Expr::lit_str(format!("{line}:{column}: a form action, which the browser runs"))));
        }
        attrs.push(Entry::Field(raw, Expr::lit_str(snapfire_fsr_ir::render::FUNCTION_FORM_ACTION)));
        continue;
      }
      let value = self.attr_value(attr)?;
      match raw.as_str() {
        "dangerouslySetInnerHTML" => attrs.push(Entry::Field(RAW_ATTR.to_owned(), inner_html(value))),
        "style" => attrs.push(Entry::Field("style".to_owned(), self.style(attr)?)),
        "value" | "defaultValue" if name == "select" => select_value = Some(value),
        "value" if name == "textarea" => text_value = Some(value),
        "defaultValue" if name == "textarea" => text_default = Some(value),
        "value" if name == "option" => {
          attrs.push(Entry::Field("value".to_owned(), value.clone()));
          if let Some(selected) = self.select_value.clone() {
            let option = Expr::Str(Box::new(value));
            let listed = self.lowerer.temp();
            let lists = Expr::Some(Box::new(selected.clone()), Box::new(Expr::Lambda { params: vec![listed.clone()], body: Box::new(Expr::Compare(CompareOp::Eq, Box::new(Expr::Str(Box::new(Expr::Var(listed)))), Box::new(option.clone()))) }));
            let names = Expr::Compare(CompareOp::Eq, Box::new(option), Box::new(Expr::Str(Box::new(selected.clone()))));
            attrs.push(Entry::Field("selected".to_owned(), Expr::Ternary(Box::new(Expr::Builtin { name: Builtin::IsArray, args: vec![selected] }), Box::new(lists), Box::new(names))));
          }
        }
        _ => attrs.push(Entry::Field(html_attr_name(&raw).to_owned(), value)),
      }
    }
    if let Some(path) = escaped_asset {
      if let Some(facts) = self.assets.image(&path, &ImageRequest::default()) {
        for (name, value) in [("width", facts.width), ("height", facts.height)] {
          if !attrs.iter().any(|e| matches!(e, Entry::Field(n, _) if n == name)) {
            attrs.push(Entry::Field(name.to_owned(), Expr::Lit(Lit::Int(i128::from(value)))));
          }
        }
      }
    }
    let outer = if name == "select" { std::mem::replace(&mut self.select_value, select_value) } else { self.select_value.take() };
    let children = self.children(&el.children);
    self.select_value = outer;
    let mut children = children?;
    if children.is_empty() && self.spreads_props(&el.opening_element.attributes) {
      children.push(self.slot("content"));
    }
    let text = match (text_value, text_default) {
      (Some(value), Some(default)) => Some(Expr::Coalesce(Box::new(value), Box::new(default))),
      (value, default) => value.or(default),
    };
    if let Some(text) = text.filter(|_| children.is_empty()) {
      children.push(Tmpl::Expr(text));
    }
    if bound {
      attrs.push(Entry::Field(hoist::BOUND_ATTR.to_owned(), Expr::Lit(Lit::Bool(true))));
    }
    if let Some(candidates) = &mut self.lowerer.hoisting {
      if !children.is_empty() && el.closing_element.is_some() {
        let range = self.lowerer.parsed.range(el.span);
        let open = self.lowerer.parsed.range(el.opening_element.span);
        attrs.push(candidates.chunk(range, open, as_child));
      }
    }
    if let Some(module) = self.elements.get(&name) {
      attrs.push(Entry::Field(snapfire_fsr_ir::render::SHADOW_ATTR.to_owned(), Expr::lit_str(module.clone())));
    }
    if let Some(tag) = chosen_tag {
      attrs.insert(0, Entry::Field(fills::CHOSEN_TAG.to_owned(), tag));
    }
    Ok(Tmpl::Element { tag: name, attrs, children })
  }

  /// True when `name` is imported from the client library's React adapter.
  /// The dialect tag `name` stands for, whatever it was imported as.
  /// The placement a tag names, however the file reaches it; `None` for a component, a local among them.
  pub(super) fn placement(&self, el: &js::JSXElement) -> Option<&'static str> {
    self.meanings().of_tag(&el.opening_element.name).and_then(|meant| meant.placement())
  }

  /// `<Mount module=… props=… />`: the empty region the adapter's `Mount`
  /// renders and then fills in the browser, since the island it places is
  /// named at run time and the server renders none of it.
  pub(super) fn mount_element(&mut self, el: &'p js::JSXElement<'p>) -> Lowered<Tmpl> {
    let attrs = self.jsx_attrs(el)?;
    placements::mount(self, &attrs, At::Span(el.span))
  }

  /// `<Island when="visible"><Chart … /></Island>`: the one component child
  /// as an island with that timing.
  pub(super) fn island_element(&mut self, el: &'p js::JSXElement<'p>) -> Lowered<Tmpl> {
    let mut when = None;
    let mut mode = None;
    let mut define = None;
    for attr in &el.opening_element.attributes {
      let js::JSXAttributeItem::Attribute(attr) = attr else { return Err(self.lowerer.residue(el.span, "a spread on `<Island>`")) };
      match attr_name(&attr.name).as_str() {
        "when" => {
          let value = self.attr_value(attr)?;
          when = Some(self.island_timing(value, attr.span)?);
        }
        "mode" => {
          let value = self.attr_value(attr)?;
          mode = self.island_mode(value, attr.span)?;
        }
        "define" => {
          let value = self.attr_value(attr)?;
          let Expr::Lit(Lit::Str(specifier)) = value else {
            return Err(self.lowerer.residue(attr.span, "`define` is the module the definition lives in, written out"));
          };
          let resolved = crate::resolve_specifier(self.file, &specifier)
            .ok_or_else(|| self.lowerer.residue(attr.span, format!("`{specifier}`, which the build cannot follow")))?;
          let has_extension = matches!(resolved.rsplit_once('.'), Some((_, ext)) if matches!(ext, "ts" | "tsx" | "js" | "mjs"));
          define = Some(match has_extension {
            true => format!("{resolved}#default"),
            false => format!("{resolved}.ts#default"),
          });
        }
        _ => return Err(self.lowerer.residue(attr.span, "`<Island>` takes `when`, `mode` and `define` and nothing else")),
      }
    }
    let mut elements = el.children.iter().filter(|c| match c {
      js::JSXChild::Text(text) => !jsx_text(text.value.as_str()).is_empty(),
      _ => true,
    });
    let (Some(js::JSXChild::Element(child)), None) = (elements.next(), elements.next()) else {
      return Err(self.lowerer.residue(el.span, "`<Island>` wraps exactly one component"));
    };
    let lowered = self.element(child, false)?;
    if let Some(module) = define {
      if mode.is_some() {
        return Err(self.lowerer.residue(el.span, "`mode` on an `<Island define>`, which mounts nothing to round-trip"));
      }
      let Tmpl::Element { .. } = &lowered else {
        return Err(self.lowerer.residue(child.span, "`<Island define>` wraps an element, since its module defines one"));
      };
      let at = self.lowerer.parsed.range(el.opening_element.name.span()).end;
      let range = self.lowerer.parsed.range(el.span);
      let id = match &mut self.lowerer.hoisting {
        Some(candidates) => candidates.island(at, range, false),
        None => 0,
      };
      return Ok(Tmpl::Island { module, props: Vec::new(), children: vec![lowered], when, mode: None, id, define: true });
    }
    self.island_of(lowered, when, mode, child.span)
  }

  /// `"server"` for an island whose events round-trip to the server; `"browser"`, the default, is `None`.
  pub(super) fn island_mode(&self, value: Expr, span: Span) -> Lowered<Option<String>> {
    match value {
      Expr::Lit(Lit::Str(mode)) if mode == SERVER_MODE => Ok(Some(mode)),
      Expr::Lit(Lit::Str(mode)) if mode == "browser" => Ok(None),
      _ => Err(self.lowerer.residue(span, "an island's `mode` is \"browser\" or \"server\", written out")),
    }
  }

  /// A prop whose value is a function (an arrow, a `function` or the name of one the component declares) lowered as a handler of this component: its index or why it does not lower. `None` for a prop holding anything else.
  /// An arrow passed as a prop that is not an event, `render={(row) => <b>{row}</b>}`, as the function value it is; `None` when it is something else or lowers only as a handler.
  pub(super) fn function_prop(&mut self, attr: &'p js::JSXAttribute<'p>) -> Lowered<Option<Expr>> {
    let Some(js::JSXAttributeValue::ExpressionContainer(c)) = &attr.value else { return Ok(None) };
    let Some(e) = c.expression.as_expression() else { return Ok(None) };
    let mut inner: &'p js::Expression<'p> = e;
    while let js::Expression::ParenthesizedExpression(p) = inner {
      inner = &p.expression;
    }
    if !matches!(inner, js::Expression::ArrowFunctionExpression(_)) {
      return Ok(None);
    }
    self.premake_expr(inner)?;
    match self.lowerer.expr(inner) {
      Ok(value) => Ok(Some(value)),
      Err(_) => {
        self.lowerer.unbound = None;
        Ok(None)
      }
    }
  }

  /// Records where the browser gets the function `value` passed as `prop` to the placement `module`, when the build can place it.
  pub(super) fn note_function(&mut self, module: &str, prop: &str, value: &Expr, attr: &'p js::JSXAttribute<'p>) {
    let Some(js::JSXAttributeValue::ExpressionContainer(c)) = &attr.value else { return };
    let Some(e) = c.expression.as_expression() else { return };
    let mut needs = Vec::new();
    if let Some(id) = self.function_reference(e, &mut needs) {
      self.fn_refs.push(FnRef { module: module.to_owned(), prop: prop.to_owned(), value: value.clone(), id, needs, unlowered: None });
    }
  }

  /// A function this file holds that the server does not lower but the browser imports by name, as a handler value with the `UNLOWERED_ATTR` text the placement carries for it: a component that calls it only in an effect or a handler never calls it on the server. `None` when the browser cannot reach it either.
  pub(super) fn unlowered_function(&mut self, module: &str, prop: &str, attr: &'p js::JSXAttribute<'p>, residue: &Residue) -> Option<(Expr, String)> {
    let Some(js::JSXAttributeValue::ExpressionContainer(c)) = &attr.value else { return None };
    let Some(e) = c.expression.as_expression() else { return None };
    if !self.names_function(e) {
      return None;
    }
    let mut needs = Vec::new();
    let Some(Ok(id)) = self.function_reference(e, &mut needs) else { return None };
    let value = Expr::Handler(snapfire_fsr_ir::render::BROWSER_HANDLER);
    let why = format!("{}:{}: {}", residue.line, residue.column, residue.message);
    self.fn_refs.push(FnRef { module: module.to_owned(), prop: prop.to_owned(), value: value.clone(), id: Ok(id), needs, unlowered: Some(why.clone()) });
    Some((value, why))
  }

  /// Whether `e` is a function written in place, a function this file declares or a conditional over those. An import is left to the build's own following of it.
  pub(super) fn names_function(&self, e: &js::Expression) -> bool {
    match crate::meaning::bare(e) {
      js::Expression::ArrowFunctionExpression(_) | js::Expression::FunctionExpression(_) => true,
      js::Expression::Identifier(id) => find_function(self.lowerer.parsed, id.name.as_str()).is_some(),
      js::Expression::ConditionalExpression(c) => self.names_function(&c.consequent) && self.names_function(&c.alternate),
      _ => false,
    }
  }

  /// The module id `file#export` the browser imports the function `e` from, as an expression, else why the browser cannot reach it. An arrow or a `function` that closes over nothing of this component is hoisted into an export of this file; a function a module declares is named by its file and export; a conditional chooses between two. `None` for anything else, a function held in a local among them.
  pub(super) fn function_reference(&mut self, e: &'p js::Expression<'p>, needs: &mut Vec<Need>) -> Option<Result<Expr, String>> {
    match crate::meaning::bare(e) {
      js::Expression::ArrowFunctionExpression(arrow) => Some(self.hoisted_function(arrow.span, closes_over(self.lowerer.parsed, arrow), needs)),
      js::Expression::FunctionExpression(f) => Some(self.hoisted_function(f.span, closes_over(self.lowerer.parsed, &**f), needs)),
      js::Expression::Identifier(id) => {
        let name = id.name.as_str();
        if self.lowerer.scope.iter().any(|(n, _)| n == name) || self.handler_fns.contains_key(name) || self.handlers.iter().any(|h| h == name) {
          return Some(Err(format!("and `{name}` is a function only this component knows")));
        }
        match self.meanings().bindings.get(name).cloned() {
          Some(crate::meaning::Meaning::Own { file, name: export }) => {
            if file == self.file {
              needs.push(Need::Name(export.clone()));
            }
            needs.push(Need::File(file.clone()));
            Some(Ok(Expr::lit_str(format!("{file}#{export}"))))
          }
          _ => None,
        }
      }
      js::Expression::ConditionalExpression(c) => {
        let test = match self.lowerer.expr(&c.test) {
          Ok(test) => test,
          Err(residue) => return Some(Err(format!("and the choice between them reads {}", residue.message))),
        };
        let cons = self.function_reference(&c.consequent, needs)?;
        let alt = self.function_reference(&c.alternate, needs)?;
        match (cons, alt) {
          (Ok(cons), Ok(alt)) => Some(Ok(Expr::Ternary(Box::new(test), Box::new(cons), Box::new(alt)))),
          (Err(reason), _) | (_, Err(reason)) => Some(Err(reason)),
        }
      }
      _ => None,
    }
  }

  /// The id of a function written in place, once the rewrite exports it from this file under a name made of where it starts; `local` is a name of this component it reads, which keeps the browser from importing it.
  pub(super) fn hoisted_function(&mut self, span: Span, local: Option<String>, needs: &mut Vec<Need>) -> Result<Expr, String> {
    if let Some(local) = local {
      return Err(format!("and it closes over `{local}`, which only this component knows"));
    }
    let range = self.lowerer.parsed.range(span);
    let name = format!("__sfF{}", range.start);
    needs.push(Need::Export(range, name.clone()));
    needs.push(Need::File(self.file.to_owned()));
    Ok(Expr::lit_str(format!("{}#{name}", self.file)))
  }

  /// Whether an attribute's value is a function: an arrow, a `function` or a name the component declares as one.
  pub(super) fn passes_function(&self, attr: &js::JSXAttribute) -> bool {
    let Some(js::JSXAttributeValue::ExpressionContainer(c)) = &attr.value else { return false };
    let Some(e) = c.expression.as_expression() else { return false };
    match crate::meaning::bare(e) {
      js::Expression::ArrowFunctionExpression(_) | js::Expression::FunctionExpression(_) => true,
      js::Expression::Identifier(id) => self.handler_fns.contains_key(id.name.as_str()) || self.browser_function_named(attr).is_some(),
      _ => false,
    }
  }

  /// The setter, dispatcher or inner function an attribute passes by name.
  pub(super) fn browser_function_named(&self, attr: &js::JSXAttribute) -> Option<String> {
    let Some(js::JSXAttributeValue::ExpressionContainer(c)) = &attr.value else { return None };
    let Some(e) = c.expression.as_expression() else { return None };
    let js::Expression::Identifier(id) = crate::meaning::bare(e) else { return None };
    let name = id.name.as_str();
    (self.handlers.iter().any(|h| h == name) && !self.lowerer.scope.iter().any(|(bound, _)| bound == name)).then(|| name.to_owned())
  }

  pub(super) fn handler_prop(&mut self, attr: &'p js::JSXAttribute<'p>, name: &str) -> Option<Lowered<usize>> {
    let Some(js::JSXAttributeValue::ExpressionContainer(c)) = &attr.value else { return None };
    let Some(e) = c.expression.as_expression() else { return None };
    let mut inner: &'p js::Expression<'p> = e;
    while let js::Expression::ParenthesizedExpression(p) = inner {
      inner = &p.expression;
    }
    if let js::Expression::Identifier(id) = inner {
      if let Some((_, state)) = self.setters.iter().find(|(setter, _)| *setter == id.name.as_str()).cloned() {
        self.lowered_handlers.push(Handler { event: name.to_owned(), body: vec![Stmt::Return(Expr::Object(vec![Entry::Field(state, Expr::Var("$event".to_owned()))]))] });
        return Some(Ok(self.lowered_handlers.len() - 1));
      }
    }
    let function = match inner {
      js::Expression::ArrowFunctionExpression(_) | js::Expression::FunctionExpression(_) => true,
      js::Expression::Identifier(id) => self.handler_fns.contains_key(id.name.as_str()),
      _ => false,
    };
    if !function {
      return None;
    }
    Some(self.handler_body(inner).map(|body| {
      self.lowered_handlers.push(Handler { event: name.to_owned(), body });
      self.lowered_handlers.len() - 1
    }))
  }

  /// An `on*` attribute naming a handler the component was handed rather than one it declares: a prop or a local read by name or through a member, its value the reference the placement passed.
  pub(super) fn handler_reference(&mut self, attr: &'p js::JSXAttribute<'p>) -> Option<Expr> {
    let Some(js::JSXAttributeValue::ExpressionContainer(c)) = &attr.value else { return None };
    let Some(e) = c.expression.as_expression() else { return None };
    let mut root: &js::Expression = e;
    loop {
      match root {
        js::Expression::ParenthesizedExpression(p) => root = &p.expression,
        m_expr if m_expr.is_member_expression() => { let m = m_expr.as_member_expression().expect("matched as a member"); root = m.object() },
        _ => break,
      }
    }
    let js::Expression::Identifier(id) = root else { return None };
    let name = id.name.as_str();
    if self.handler_fns.contains_key(name) || !self.lowerer.scope.iter().any(|(bound, _)| bound == name) {
      return None;
    }
    self.lowerer.expr(e).ok()
  }

  /// An `on*` attribute's handler as a lowered body or why it is not one.
  /// A handler is `const`s, calls to state setters, writes to the store and calls to actions, `e.preventDefault()`
  /// aside; the body returns the state it set.
  pub(super) fn handler_attr(&mut self, attr: &'p js::JSXAttribute<'p>) -> Lowered<usize> {
    let Some(js::JSXAttributeValue::ExpressionContainer(c)) = &attr.value else { return Err(self.lowerer.residue(attr.span, "a handler that is not an expression")) };
    let Some(e) = c.expression.as_expression() else { return Err(self.lowerer.residue(attr.span, "an empty handler")) };
    let body = self.handler_body(e)?;
    self.lowered_handlers.push(Handler { event: attr_name(&attr.name)[2..].to_ascii_lowercase(), body });
    Ok(self.lowered_handlers.len() - 1)
  }
}
