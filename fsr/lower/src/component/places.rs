use super::*;

impl<'a, 'p> ComponentLowerer<'a, 'p> {
  /// `const Lazy = island(Chart, { when: "visible" })` at module scope, when
  /// `name` is such a `Lazy`: the component and the timing.
  pub(super) fn island_alias(&mut self, name: &str) -> Lowered<Option<(String, Option<String>, Option<String>)>> {
    match island_alias_of(self.lowerer.parsed, &self.meanings(), name) {
      Ok(alias) => Ok(alias.map(|a| (a.target, a.when, a.mode))),
      Err((span, message)) => Err(self.lowerer.residue(span, message)),
    }
  }

  /// `<Slot name="modal" />` in a layout: the plan child of that name, in a
  /// region navigation fills and empties.
  pub(super) fn slot_element(&mut self, el: &'p js::JSXElement<'p>) -> Lowered<Tmpl> {
    if !self.layout_root {
      return Err(self.lowerer.residue(el.span, "`<Slot>` outside a layout"));
    }
    let mut name = None;
    for attr in &el.opening_element.attributes {
      let js::JSXAttributeItem::Attribute(attr) = attr else { return Err(self.lowerer.residue(el.span, "a spread on `<Slot>`")) };
      if attr_name(&attr.name) != "name" {
        return Err(self.lowerer.residue(attr.span, "`<Slot>` takes `name` and nothing else"));
      }
      match self.attr_value(attr)? {
        Expr::Lit(Lit::Str(value)) => name = Some(value),
        _ => return Err(self.lowerer.residue(attr.span, "a slot's `name` is written out")),
      }
    }
    let Some(name) = name else { return Err(self.lowerer.residue(el.span, "`<Slot>` needs a `name`")) };
    let fallback = self.children(&el.children)?;
    if fallback.is_empty() {
      return Ok(self.slot(&name));
    }
    Ok(self.slot_with_fallback(&name, fallback))
  }

  /// `<Link href="/x" full into="modal" prefetch="none" keep={false}>` from the
  /// client library: an `<a>` carrying the data attributes the navigator reads.
  pub(super) fn link_element(&mut self, el: &'p js::JSXElement<'p>) -> Lowered<Tmpl> {
    let attrs = self.jsx_attrs(el)?;
    let children = self.children(&el.children)?;
    placements::link(self, &attrs, children)
  }

  /// Whether an `<img>`'s `src` is `<asset>.src` of an imported image, which
  /// the rewrite lowers as a `Picture`, and not escaped with `data-sf-raw`.
  pub(super) fn img_names_an_asset(&self, el: &'p js::JSXElement<'p>) -> bool {
    let escaped = el.opening_element.attributes.iter().any(|attr| matches!(attr, js::JSXAttributeItem::Attribute(attr) if attr_name(&attr.name) == RAW_ESCAPE));
    !escaped && self.img_asset_path(el).is_some()
  }

  /// The imported image an `<img src={x.src}>` names, by its app-relative path.
  pub(super) fn img_asset_path(&self, el: &'p js::JSXElement<'p>) -> Option<String> {
    for attr in &el.opening_element.attributes {
      let js::JSXAttributeItem::Attribute(attr) = attr else { continue };
      if attr_name(&attr.name) != "src" {
        continue;
      }
      let Some(js::JSXAttributeValue::ExpressionContainer(c)) = &attr.value else { return None };
      let Some(e) = c.expression.as_expression() else { return None };
      let Some(m) = (&*e).as_member_expression() else { return None };
      let (js::Expression::Identifier(obj), crate::syntax::MemberProp::Ident(prop)) = (m.object(), crate::syntax::prop(m)) else { return None };
      if prop.name.as_str() != "src" {
        return None;
      }
      return placements::asset_import(&self.lowerer, obj.name.as_str());
    }
    None
  }

  /// `<Picture src={hero} alt="…" sizes="…" priority widths={[…]} quality={…} source="cms">`,
  /// or an `<img>` of an imported asset under the rewrite: a `<picture>`
  /// with a `<source>` per format and the hashed original as its `<img>`,
  /// an `<img>` alone for an image served as it is, and for a string `src`
  /// an `<img>` whose `srcset` a named source's template writes.
  pub(super) fn picture_element(&mut self, el: &'p js::JSXElement<'p>, from_img: bool) -> Lowered<Tmpl> {
    let attrs = self.jsx_attrs(el)?;
    placements::picture(self, &attrs, At::Span(el.span), from_img)
  }

  pub(super) fn remember_head(&mut self, row: HeadRow) {
    if !self.heads.contains(&row) {
      self.heads.push(row);
    }
  }

  pub(super) fn island_timing(&self, value: Expr, span: Span) -> Lowered<String> {
    placements::timing(self, value, At::Span(span))
  }

  /// `lowered` as an island placement, keeping the id its placement took, so
  /// the bundle's copy of this component carries the same region key.
  pub(super) fn island_of(&self, lowered: Tmpl, when: Option<String>, mode: Option<String>, span: Span) -> Lowered<Tmpl> {
    match lowered {
      Tmpl::Component { module, props, children, id, .. } => Ok(Tmpl::Island { module, props, children, when, mode, id, define: false }),
      _ => Err(self.lowerer.residue(span, "an island must be a component, not an element")),
    }
  }

  /// `<Suspense>` as its children inside React's completed boundary, the fallback left out since nothing on the server waits; `<Activity>` as its children inside an Activity boundary unless `mode` is `"hidden"`, which React's server renders as nothing.
  pub(super) fn boundary(&mut self, which: &str, el: &'p js::JSXElement<'p>) -> Lowered<Tmpl> {
    let children = Tmpl::Fragment(self.children(&el.children)?);
    if which == "Suspense" && self.lowerer.reaches(Reach::Terminal) {
      let fallback = el.opening_element.attributes.iter().find_map(|attr| match attr {
        js::JSXAttributeItem::Attribute(attr) if attr_name(&attr.name) == "fallback" => attr_expression(attr),
        _ => None,
      });
      let fallback = match fallback {
        Some(e) => self.child_expr(e)?,
        None => Tmpl::Fragment(Vec::new()),
      };
      return Ok(Tmpl::Element { tag: snapfire_fsr_ir::render::SUSPENSE_TAG.to_owned(), attrs: Vec::new(), children: vec![children, fallback] });
    }
    let kind = if which == "Suspense" { "$" } else { "&" };
    let boundary = Tmpl::Element { tag: snapfire_fsr_ir::render::BOUNDARY_TAG.to_owned(), attrs: vec![Entry::Field("kind".to_owned(), Expr::lit_str(kind))], children: vec![children] };
    if which == "Suspense" {
      return Ok(boundary);
    }
    let mode = el.opening_element.attributes.iter().find_map(|attr| match attr {
      js::JSXAttributeItem::Attribute(attr) if attr_name(&attr.name) == "mode" => Some(attr),
      _ => None,
    });
    let Some(mode) = mode else { return Ok(boundary) };
    let mode = self.attr_value(mode)?;
    Ok(Tmpl::If { cond: Expr::Compare(CompareOp::Ne, Box::new(mode), Box::new(Expr::lit_str("hidden"))), then: Box::new(boundary), r#else: None })
  }

  /// `as_child` says the element sits among JSX children, where a keyed
  /// placement's wrap must be braced.
  pub(super) fn component_ref(&mut self, name: &str, el: &'p js::JSXElement<'p>, as_child: bool) -> Lowered<Tmpl> {
    if name == "Fragment" || name == "React.Fragment" {
      return Ok(Tmpl::Fragment(self.children(&el.children)?));
    }
    if let Some(crate::meaning::Meaning::Library(crate::meaning::Construct::Boundary(which))) = self.meanings().of_tag(&el.opening_element.name) {
      return self.boundary(which, el);
    }
    match self.placement(el) {
      Some("Island") => return self.island_element(el),
      Some("Slot") => return self.slot_element(el),
      Some("Link") => return self.link_element(el),
      Some("Picture") => return self.picture_element(el, false),
      Some("Mount") => return self.mount_element(el),
      Some(which @ ("Box" | "Text" | "Newline" | "Spacer")) => {
        let attrs = self.jsx_attrs(el)?;
        let children = self.children(&el.children)?;
        return placements::terminal(self, which, &attrs, children);
      }
      Some("Place") => {
        let attrs = self.jsx_attrs(el)?;
        let has_children = el.children.iter().any(|c| !matches!(c, js::JSXChild::Text(t) if jsx_text(t.value.as_str()).is_empty()));
        return placements::place(self, &attrs, has_children, At::Span(el.span));
      }
      Some("Embed") => {
        let attrs = self.jsx_attrs(el)?;
        return placements::embed(self, &attrs, At::Span(el.span));
      }
      _ => {}
    }
    let chosen = self.chosen.get(name).cloned();
    if let (js::JSXElementName::IdentifierReference(id), None) = (&el.opening_element.name, &chosen) {
      let meanings = self.meanings();
      if !matches!(meanings.ctxt(id), crate::Ctxt::Top | crate::Ctxt::Unresolved) {
        return Err(self.lowerer.residue(el.opening_element.name.span(), format!("`{name}` is not a component this file declares or imports")));
      }
    }
    if let Some((target, when, mode)) = self.island_alias(name)? {
      let lowered = self.component_ref(&target, el, false)?;
      return self.island_of(lowered, when, mode, el.span);
    }
    let transformed_target = match &chosen {
      Some(_) => true,
      None => matches!(self.meanings().bindings.get(name), Some(crate::meaning::Meaning::Own { file, name: export }) if is_transformed(file) && export == "default"),
    };
    let mut overrides: Vec<Override> = Vec::new();
    let mut props = Vec::new();
    let mut browser_props: Vec<String> = Vec::new();
    let placed_module = format!("{}#{name}", self.file);
    for attr in &el.opening_element.attributes {
      let attr = match attr {
        js::JSXAttributeItem::Attribute(attr) => attr,
        js::JSXAttributeItem::SpreadAttribute(spread) => {
          props.push(Entry::Spread(self.lowerer.expr(&spread.argument)?));
          continue;
        }
      };
      let raw = attr_name(&attr.name);
      if raw == "key" || raw == "ref" {
        continue;
      }
      if raw == "components" && transformed_target {
        overrides = self.component_overrides(attr)?;
        continue;
      }
      if raw == "children" {
        return Err(self.lowerer.residue(attr.span, "`children` as a prop; pass them between the tags"));
      }
      if let Some(lowered) = self.handler_prop(attr, &raw) {
        match lowered {
          Ok(index) => props.push(Entry::Field(raw, Expr::Handler(index as u32))),
          Err(residue) if self.unbound_is_followable() => return Err(residue),
          Err(_) if !is_handler_name(&raw) && self.function_prop(attr)?.is_some() => {
            self.lowerer.unbound = None;
            let value = self.function_prop(attr)?.expect("lowered just above");
            self.note_function(&placed_module, &raw, &value, attr);
            props.push(Entry::Field(raw, value));
          }
          Err(residue) => {
            self.lowerer.unbound = None;
            if !props.iter().any(|e| matches!(e, Entry::Field(n, _) if n == UNLOWERED_ATTR)) {
              props.push(Entry::Field(UNLOWERED_ATTR.to_owned(), Expr::lit_str(format!("{}:{}: {}", residue.line, residue.column, residue.message))));
            }
          }
        }
        continue;
      }
      if is_handler_name(&raw) {
        if let Some(handed) = self.handler_reference(attr) {
          props.push(Entry::Field(raw, handed));
        }
        continue;
      }
      if let Some(name) = self.browser_function_named(attr) {
        if !props.iter().any(|e| matches!(e, Entry::Field(n, _) if n == UNLOWERED_ATTR)) {
          let (line, column) = self.lowerer.parsed.loc(attr.span.start);
          props.push(Entry::Field(UNLOWERED_ATTR.to_owned(), Expr::lit_str(format!("{line}:{column}: `{name}`, a function only the browser calls"))));
        }
        props.push(Entry::Field(raw, Expr::Handler(snapfire_fsr_ir::render::BROWSER_HANDLER)));
        continue;
      }
      // A prop only the browser knows goes in as null; the child reads it as a browser value and this component hydrates to compute it.
      self.lowerer.browser_value = None;
      let value = match self.attr_value(attr) {
        Ok(value) => value,
        Err(residue) => match self.lowerer.browser_value.take() {
          Some(reason) => {
            let (line, column) = self.lowerer.parsed.loc(attr.span.start);
            let wrap = attr_expression(attr).map(|e| self.lowerer.parsed.range(e.span()));
            self.browser_texts.push(BrowserText { wrap, line, column, reason });
            browser_props.push(raw.clone());
            Expr::Var(BROWSER_VAR.to_owned())
          }
          None if self.unbound_is_followable() => return Err(residue),
          None => match self.unlowered_function(&placed_module, &raw, attr, &residue) {
            Some((value, why)) => {
              self.lowerer.unbound = None;
              if !props.iter().any(|e| matches!(e, Entry::Field(n, _) if n == UNLOWERED_ATTR)) {
                props.push(Entry::Field(UNLOWERED_ATTR.to_owned(), Expr::lit_str(why)));
              }
              value
            }
            None => return Err(residue),
          },
        },
      };
      if fills::holds_function(&value) {
        self.note_function(&placed_module, &raw, &value, attr);
      }
      props.push(Entry::Field(raw, value));
    }
    let mut passed = Vec::new();
    for attr in &el.opening_element.attributes {
      let js::JSXAttributeItem::Attribute(attr) = attr else { continue };
      let Some(e) = attr_expression(attr) else { continue };
      if let Some(kind) = self.lowerer.kind_of(e) {
        passed.push((attr_name(&attr.name), kind));
      }
    }
    for prop in browser_props {
      if !passed.iter().any(|(p, _)| *p == prop) {
        passed.push((prop, crate::Kind::Browser));
      }
    }
    let children = self.children(&el.children)?;
    let loc = self.lowerer.parsed.loc(el.span.start);
    let at = self.lowerer.parsed.range(el.opening_element.name.span()).end;
    let range = self.lowerer.parsed.range(el.span);
    let id = match &mut self.lowerer.hoisting {
      Some(candidates) => candidates.island(at, range, as_child),
      None => 0,
    };
    if let Some((key, choices)) = chosen {
      let mut tree = Tmpl::Fragment(Vec::new());
      for (text, target) in choices.iter().rev() {
        let placed = format!("{}#default", self.variant_of(target, &overrides));
        self.refs.push((placed.clone(), loc));
        self.placed_kinds.push((placed.clone(), passed.clone()));
        let cond = Expr::Compare(CompareOp::Eq, Box::new(Expr::Str(Box::new(key.clone()))), Box::new(Expr::lit_str(text.clone())));
        let then = Tmpl::Component { module: format!("{}#{placed}", self.file), props: props.clone(), children: children.clone(), id, keyed: false };
        tree = Tmpl::If { cond, then: Box::new(then), r#else: Some(Box::new(tree)) };
      }
      return Ok(tree);
    }
    if !overrides.is_empty() {
      let Some(crate::meaning::Meaning::Own { file: target, .. }) = self.meanings().bindings.get(name).cloned() else { unreachable!("a transformed target") };
      let placed = format!("{}#default", self.variant_of(&target, &overrides));
      self.refs.push((placed.clone(), loc));
      self.placed_kinds.push((placed.clone(), passed));
      return Ok(Tmpl::Component { module: format!("{}#{placed}", self.file), props, children, id, keyed: false });
    }
    self.refs.push((name.to_owned(), loc));
    self.placed_kinds.push((name.to_owned(), passed));
    Ok(Tmpl::Component { module: format!("{}#{name}", self.file), props, children, id, keyed: false })
  }

  pub(super) fn attr_value(&mut self, attr: &'p js::JSXAttribute<'p>) -> Lowered<Expr> {
    match &attr.value {
      None => Ok(Expr::Lit(Lit::Bool(true))),
      Some(js::JSXAttributeValue::StringLiteral(s)) => Ok(Expr::Lit(Lit::Str(decode_entities(&crate::lossy(s.value))))),
      Some(js::JSXAttributeValue::ExpressionContainer(c)) => match c.expression.as_expression() {
        Some(e) if self.holds_markup(e) => Ok(Expr::Markup(std::sync::Arc::new(self.child_expr(e)?))),
        Some(e) => {
          self.premake_expr(e)?;
          self.lowerer.expr(e)
        }
        None => Err(self.lowerer.residue(c.expression.span(), "an empty attribute expression")),
      },
      Some(other) => Err(self.lowerer.residue(other.span(), "an element as an attribute value")),
    }
  }

  /// `style={{ a: x, bTwo: y }}` as an object keyed by CSS name, which the
  /// renderer serialises the way React does.
  pub(super) fn style(&mut self, attr: &'p js::JSXAttribute<'p>) -> Lowered<Expr> {
    let attr = self.jsx_attr(attr)?;
    placements::style(self, &attr)
  }

  /// An attribute as the shared placements read it.
  pub(super) fn jsx_attr(&self, attr: &'p js::JSXAttribute<'p>) -> Lowered<Attr<'p>> {
    let value = match &attr.value {
      None => Value::Present,
      Some(js::JSXAttributeValue::StringLiteral(s)) => Value::Text(decode_entities(&crate::lossy(s.value))),
      Some(js::JSXAttributeValue::ExpressionContainer(c)) => match c.expression.as_expression() {
        Some(e) => Value::Script(e),
        None => return Err(self.lowerer.residue(c.expression.span(), "an empty attribute expression")),
      },
      Some(other) => return Err(self.lowerer.residue(other.span(), "an element as an attribute value")),
    };
    Ok(Attr { name: attr_name(&attr.name), value, at: At::Span(attr.span) })
  }

  /// An element's attributes as the shared placements read them, handlers
  /// left out: they belong to the browser.
  pub(super) fn jsx_attrs(&self, el: &'p js::JSXElement<'p>) -> Lowered<Vec<Attr<'p>>> {
    let mut out = Vec::new();
    for attr in &el.opening_element.attributes {
      match attr {
        js::JSXAttributeItem::Attribute(attr) if is_handler_name(&attr_name(&attr.name)) => {}
        js::JSXAttributeItem::Attribute(attr) => out.push(self.jsx_attr(attr)?),
        js::JSXAttributeItem::SpreadAttribute(spread) => out.push(Attr { name: String::new(), value: Value::Spread(&spread.argument), at: At::Span(spread.argument.span()) }),
      }
    }
    Ok(out)
  }

  pub(super) fn children(&mut self, children: &'p [js::JSXChild<'p>]) -> Lowered<Vec<Tmpl>> {
    let mut out = Vec::new();
    for child in children {
      match child {
        js::JSXChild::Text(text) => {
          let cleaned = jsx_text(text.value.as_str());
          if !cleaned.is_empty() {
            out.push(Tmpl::Text(cleaned));
          }
        }
        js::JSXChild::ExpressionContainer(c) => match c.expression.as_expression() {
          None => {}
          Some(e) => {
            self.lowerer.browser_value = None;
            match self.child_expr(e) {
              Ok(child) => out.push(child),
              // Text only the browser knows: the server writes nothing and the browser copy computes it once mounted, so its first render matches. Markup the value would choose is structure, which stays a residue.
              Err(residue) => match self.lowerer.browser_value.take() {
                Some(reason) if !self.holds_markup(e) => {
                  let (line, column) = self.lowerer.parsed.loc(e.span().start);
                  self.browser_texts.push(BrowserText { wrap: Some(self.lowerer.parsed.range(e.span())), line, column, reason });
                  out.push(Tmpl::Expr(Expr::Var(BROWSER_VAR.to_owned())));
                }
                _ => return Err(residue),
              },
            }
          }
        },
        js::JSXChild::Element(el) => out.push(self.element(el, true)?),
        js::JSXChild::Fragment(frag) => out.push(Tmpl::Fragment(self.children(&frag.children)?)),
        js::JSXChild::Spread(s) => return Err(self.lowerer.residue(s.span, "a spread child")),
      }
    }
    Ok(out)
  }
}
