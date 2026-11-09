use super::*;

pub(super) fn component_scope(props: Value, given: Value) -> Vec<(Name, Value)> {
  vec![(Name::new_static("$props"), props), (Name::new(SLOTS_GIVEN), given)]
}

/// The `SLOTS_GIVEN` object for a placement's children, each wrapper's value
/// read in the caller's scope.
pub(super) fn given_slots(env: &mut Env, children: &[Tmpl]) -> Result<Value, Fail> {
  let mut given = snapfire_fsr_core::Fields::default();
  for child in children {
    match child {
      Tmpl::Let { name, expr, .. } if name.starts_with(SLOT_CONTENT_PREFIX) => {
        let value = env.eval_sync(expr)?;
        if truthy(&value) {
          given.insert(name[SLOT_CONTENT_PREFIX.len()..].to_owned(), value);
        }
      }
      _ => {
        given.insert("default".to_owned(), Value::Bool(true));
      }
    }
  }
  Ok(Value::Map(ValueMap::from(given)))
}

pub(super) fn chunk_id(attrs: &[Entry]) -> Option<u32> {
  attrs.iter().find_map(|entry| match entry {
    Entry::Field(name, crate::ast::Expr::Lit(crate::ast::Lit::Int(id))) | Entry::Field(name, crate::ast::Expr::Folded(Value::Int(id))) if name == CHUNK_ATTR => Some(*id as u32),
    _ => None,
  })
}

/// A child expression the way React prints one: strings and numbers as text,
/// `null` and booleans as nothing, an array as its items in turn. Under
/// Vue's rules it is `toDisplayString`: a boolean is its word and an array or
/// an object its JSON.
pub(super) fn interpolate(value: &Value, out: &mut Out, markup: Markup) -> Result<(), Fail> {
  if markup.is_vue() {
    out.text(&vue::display(value)?, markup);
    return Ok(());
  }
  if markup.is_svelte() {
    out.text(&svelte::display(value)?, markup);
    return Ok(());
  }
  match value {
    Value::Null | Value::Undefined | Value::Bool(_) => Ok(()),
    _ if handler_token(value).is_some() => Ok(()),
    Value::Seq(items) => {
      for item in items {
        interpolate(item, out, markup)?;
      }
      Ok(())
    }
    Value::Map(map) if map.kind() == snapfire_fsr_core::Collection::Plain => {
      let keys: Vec<&str> = map.keys().map(|k| k.as_str()).collect();
      Err(Fail::internal(format!("Objects are not valid as a React child (found: object with keys {{{}}})", keys.join(", "))))
    }
    other => {
      out.text(&crate::interp::scalar_str(other)?, markup);
      Ok(())
    }
  }
}

/// Whether `name` may be written into a tag. A name is printed as it stands,
/// and a `{...spread}` or a computed key takes one from a runtime value, so a
/// name carrying a space or a quote would close the attribute and start
/// another: `{"x onerror=alert(1)": 1}` spread onto an element was an event
/// handler. Anything holding a character that ends a name is dropped, the way
/// React drops one it cannot write.
pub(super) fn writable_attr_name(name: &str) -> bool {
  !name.is_empty()
    && !name.chars().any(|c| {
      c.is_whitespace() || c.is_control() || matches!(c, '"' | '\'' | '>' | '<' | '/' | '=' | '&')
    })
}

/// The shape `Entry::Baked` text takes, which the tag writer prints verbatim; a name from data must never take it.
pub(super) fn is_baked_name(name: &str) -> bool {
  name.is_empty() || name.starts_with(' ')
}

/// ` name="text"`, the text escaped.
pub(super) fn write_attr(name: &str, text: &str, out: &mut String) {
  out.push(' ');
  out.push_str(name);
  out.push_str("=\"");
  escape_attr(text, out);
  out.push('"');
}

/// An attribute of `tag` the way `markup`'s rules print one: `null` omits it.
/// Under React a custom element's attribute follows that major's own rules.
/// Under plain markup a custom element takes a scalar like any element and
/// refuses anything else, since nothing will set it as a property. On every
/// other element `false` omits it, except where React writes it as `"false"`,
/// `true` on a boolean attribute writes
/// `name=""`, a `style` with nothing in it is omitted and anything else is
/// stringified.
pub(super) fn attribute(markup: Markup, tag: &str, name: &str, value: &Value, out: &mut String) -> Result<(), Fail> {
  if !writable_attr_name(name) {
    return Ok(());
  }
  if markup.is_vue() {
    return vue::attribute(name, value, out);
  }
  if markup.is_svelte() {
    return svelte::attribute(name, value, out);
  }
  if matches!(value, Value::Null | Value::Undefined) {
    return Ok(());
  }
  if name != "style" && is_custom_element(tag) {
    match markup {
      Markup::React(major) => return major.custom_attribute(name, value, out),
      Markup::Plain if !is_scalar(value) => return Err(unset_property(tag, name, value)),
      Markup::Plain | Markup::Vue(_) | Markup::Svelte(_) => {}
    }
  }
  if matches!(value, Value::Bool(false)) && !markup.writes_false(name) {
    return Ok(());
  }
  if name == "style" {
    let css = match value {
      Value::Map(map) => style_text(map)?,
      other => stringify(other)?,
    };
    if css.is_empty() {
      return Ok(());
    }
    out.push_str(" style=\"");
    escape_attr(&css, out);
    out.push('"');
    return Ok(());
  }
  if markup.is_boolean(name) {
    if truthy(value) {
      write_attr(name, "", out);
    }
    return Ok(());
  }
  if matches!(value, Value::Bool(true)) && markup.drops_true(name) {
    return Ok(());
  }
  if matches!(value, Value::Str(text) if text.is_empty()) && markup.drops_empty(tag, name) {
    return Ok(());
  }
  let text = crate::interp::scalar_str(value)?;
  if is_url_attr(name) && is_javascript_url(&text) && text != FUNCTION_FORM_ACTION {
    write_attr(name, BLOCKED_JAVASCRIPT_URL, out);
    return Ok(());
  }
  write_attr(name, &text, out);
  Ok(())
}

pub(super) fn is_url_attr(name: &str) -> bool {
  ["href", "src", "action", "formaction", "xlink:href"].iter().any(|n| name.eq_ignore_ascii_case(n))
}

/// React's test: leading C0 controls and spaces, then `javascript:` with tabs and newlines allowed between letters, case-insensitive.
pub(super) fn is_javascript_url(url: &str) -> bool {
  let mut chars = url.chars().skip_while(|c| *c <= '\u{1F}' || *c == ' ');
  for expected in "javascript:".chars() {
    let next = loop {
      match chars.next() {
        Some('\r' | '\n' | '\t') if expected != 'j' => continue,
        other => break other,
      }
    };
    match next {
      Some(c) if c.eq_ignore_ascii_case(&expected) => {}
      _ => return false,
    }
  }
  true
}

pub(super) fn is_scalar(value: &Value) -> bool {
  matches!(value, Value::Null | Value::Undefined | Value::Bool(_) | Value::Int(_) | Value::UInt(_) | Value::F32(_) | Value::F64(_) | Value::Str(_))
}

/// A non-scalar on a custom element in markup no framework hydrates. Nothing
/// in the browser will set it as a property, so the render names it.
pub(super) fn unset_property(tag: &str, name: &str, value: &Value) -> Fail {
  Fail::internal(format!("`{name}` on `<{tag}>` holds a value of kind `{}` and nothing hydrates this markup to set it as a property; pass a string or give the element a template under `elements/`", crate::bind::kind_name(value)))
}
