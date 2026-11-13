use super::*;

/// A style object the way React's server renderer prints it: each name as its CSS spelling, `name:value` joined by `;`, null and empty values skipped, a number in `px` unless the property is unitless or the number is zero.
pub(super) fn style_text(map: &ValueMap) -> Result<String, Fail> {
  let mut out = String::new();
  for (name, value) in map {
    let unitless = name.starts_with("--") || is_unitless(name);
    let name = &css_name(name);
    let text = match value {
      Value::Null | Value::Undefined | Value::Bool(_) => continue,
      Value::Str(s) if s.trim().is_empty() => continue,
      Value::Str(s) => s.trim().to_owned(),
      Value::Int(0) => "0".to_owned(),
      Value::F64(f) if *f == 0.0 => "0".to_owned(),
      Value::Int(_) | Value::F64(_) | Value::F32(_) | Value::UInt(_) => {
        let n = stringify(value)?;
        if unitless { n } else { format!("{n}px") }
      }
      other => stringify(other)?,
    };
    if !out.is_empty() {
      out.push(';');
    }
    out.push_str(name);
    out.push(':');
    out.push_str(&text);
  }
  Ok(out)
}

/// Whether `name`, in the spelling a style object writes or in CSS spelling,
/// is one React leaves unitless. A plan from an older build holds CSS
/// spellings, which the set does not.
pub(super) fn is_unitless(name: &str) -> bool {
  if UNITLESS.contains(&name) {
    return true;
  }
  if !name.contains('-') {
    return false;
  }
  let mut camel = String::with_capacity(name.len());
  let mut upper = false;
  for c in name.chars() {
    match c {
      '-' => upper = true,
      c if upper => {
        camel.push(c.to_ascii_uppercase());
        upper = false;
      }
      c => camel.push(c),
    }
  }
  let mut lowered = camel.clone();
  if let Some(first) = lowered.get_mut(0..1) {
    first.make_ascii_lowercase();
  }
  UNITLESS.contains(&camel.as_str()) || UNITLESS.contains(&lowered.as_str())
}

/// `tmpl` with its literal open tags baked and every constant expression folded.
pub(super) fn prepare_tmpl(tmpl: Tmpl) -> Tmpl {
  let all = |children: Vec<Tmpl>| children.into_iter().map(prepare_tmpl).collect::<Vec<_>>();
  let fold = |mut expr: crate::ast::Expr| {
    fold_expr(&mut expr);
    expr
  };
  match tmpl {
    Tmpl::Element { tag, mut attrs, children } if tag == CHOSEN_TAG || tag == BOUNDARY_TAG || tag == SUSPENSE_TAG => {
      fold_entries(&mut attrs);
      Tmpl::Element { tag, attrs, children: all(children) }
    }
    Tmpl::Element { tag, attrs, children } => {
      let children = all(children);
      match baked_open(&tag, &attrs) {
        Some(open) => Tmpl::Baked { open, tag: Some(tag), children },
        None => {
          let mut attrs = baked_attrs(&tag, &attrs);
          fold_entries(&mut attrs);
          Tmpl::Element { tag, attrs, children }
        }
      }
    }
    Tmpl::Expr(expr) => Tmpl::Expr(fold(expr)),
    Tmpl::Html(expr) => Tmpl::Html(fold(expr)),
    Tmpl::Fragment(children) => Tmpl::Fragment(all(children)),
    Tmpl::If { cond, then, r#else } => Tmpl::If { cond: fold(cond), then: Box::new(prepare_tmpl(*then)), r#else: r#else.map(|other| Box::new(prepare_tmpl(*other))) },
    Tmpl::For { over, params, body } => Tmpl::For { over: fold(over), params, body: Box::new(prepare_tmpl(*body)) },
    Tmpl::Let { name, expr, then } => Tmpl::Let { name, expr: fold(expr), then: Box::new(prepare_tmpl(*then)) },
    Tmpl::Component { module, mut props, children, id, keyed } => {
      fold_entries(&mut props);
      Tmpl::Component { module, props, children: all(children), id, keyed }
    }
    Tmpl::Island { module, mut props, children, when, mode, id, define } => {
      fold_entries(&mut props);
      Tmpl::Island { module, props, children: all(children), when, mode, id, define }
    }
    Tmpl::Baked { open, tag, children } => Tmpl::Baked { open, tag, children: all(children) },
    Tmpl::Place { target, children } => Tmpl::Place { target, children: all(children) },
    other @ (Tmpl::Text(_) | Tmpl::Slot(_) | Tmpl::Marker(_) | Tmpl::Outlet { .. }) => other,
  }
}


pub(super) fn fold_entries(entries: &mut [Entry]) {
  for entry in entries {
    match entry {
      Entry::Field(_, e) | Entry::Item(e) | Entry::Spread(e) => fold_expr(e),
      Entry::Computed(k, v) => {
        fold_expr(k);
        fold_expr(v);
      }
      Entry::Baked(_) => {}
    }
  }
}

pub(super) fn lit_value(lit: &crate::ast::Lit) -> Value {
  use crate::ast::Lit;
  match lit {
    Lit::Null => Value::Null,
    Lit::Undefined => Value::Undefined,
    Lit::Bool(b) => Value::Bool(*b),
    Lit::Int(n) => Value::Int(*n),
    Lit::Float(f) => Value::F64(*f),
    Lit::Str(s) => Value::str(s.clone()),
  }
}

/// Folds `expr` bottom up: a literal becomes its value; an object or array
/// whose entries are all folded becomes the value they build; a template
/// whose parts are all folded becomes the string they print. Anything else
/// keeps its shape with its children folded.
pub(super) fn fold_expr(expr: &mut crate::ast::Expr) {
  use crate::ast::Expr;
  match expr {
    Expr::Lit(lit) => *expr = Expr::Folded(lit_value(lit).frozen()),
    Expr::Folded(_) | Expr::Var(_) | Expr::Param(_) | Expr::Query(_) | Expr::ParamsAll | Expr::QueryAll | Expr::Session(_) | Expr::Context(_) | Expr::Store(_) | Expr::Identity(_) | Expr::Locale | Expr::Locales | Expr::Path | Expr::Document | Expr::Host | Expr::Origin | Expr::Address | Expr::Config(_) | Expr::Input | Expr::Now | Expr::Const(_) | Expr::Handler(_) | Expr::HoistId(_) => {}
    Expr::Markup(tmpl) => *tmpl = Arc::new(prepare_tmpl(Arc::unwrap_or_clone(std::mem::replace(tmpl, Arc::new(Tmpl::Fragment(Vec::new())))))),
    Expr::Lambda { body, .. } => body.visit_mut(&mut |e| {
      if let Expr::Markup(tmpl) = e {
        *tmpl = Arc::new(prepare_tmpl(Arc::unwrap_or_clone(std::mem::replace(tmpl, Arc::new(Tmpl::Fragment(Vec::new()))))));
      }
    }),
    Expr::Block(body) => crate::ast::body_visit_mut(body, &mut |e| {
      if let Expr::Markup(tmpl) = e {
        *tmpl = Arc::new(prepare_tmpl(Arc::unwrap_or_clone(std::mem::replace(tmpl, Arc::new(Tmpl::Fragment(Vec::new()))))));
      }
    }),
    // An object or array literal is a new object each time it is evaluated, so only what it holds folds.
    Expr::Object(entries) | Expr::Array(entries) => fold_entries(entries),
    Expr::Template(parts) => {
      parts.iter_mut().for_each(fold_expr);
      if parts.iter().all(|part| matches!(part, Expr::Folded(_))) {
        let mut text = String::new();
        for part in parts.iter() {
          if let Expr::Folded(value) = part {
            match crate::interp::stringify(value) {
              Ok(piece) => text.push_str(&piece),
              Err(_) => return,
            }
          }
        }
        *expr = Expr::Folded(Value::str(text).frozen());
      }
    }
    Expr::Fix { f: e, .. } | Expr::Field(e, _) | Expr::Not(e) | Expr::Entries(e) | Expr::Keys(e) | Expr::Values(e) | Expr::Length(e) | Expr::Str(e) | Expr::Num(e) | Expr::BigInt(e) | Expr::Hoist { expr: e, .. } | Expr::Cell(e) | Expr::ReadCell(e) | Expr::WriteStore(_, e) => fold_expr(e),
    Expr::WriteCell(a, b) | Expr::ModuleCell { key: a, init: b } => {
      fold_expr(a);
      fold_expr(b);
    }
    Expr::Index(a, b) | Expr::Arith(_, a, b) | Expr::Compare(_, a, b) | Expr::Logic(_, a, b) | Expr::Coalesce(a, b) | Expr::Map(a, b) | Expr::Filter(a, b) | Expr::Find(a, b) | Expr::FindIndex(a, b) | Expr::Some(a, b) | Expr::Every(a, b) | Expr::Sort(a, b) | Expr::FlatMap(a, b) | Expr::ApplyOf(a, b) => {
      fold_expr(a);
      fold_expr(b);
    }
    Expr::Ternary(a, b, c) | Expr::Reduce(a, b, c) | Expr::ReplaceWith(a, b, c) => {
      fold_expr(a);
      fold_expr(b);
      fold_expr(c);
    }
    Expr::Builtin { args: items, .. } | Expr::Ext { args: items, .. } => items.iter_mut().for_each(fold_expr),
    Expr::Call { args, .. } | Expr::NativeCall { args, .. } => args.iter_mut().for_each(|(_, e)| fold_expr(e)),
    Expr::Next(e) | Expr::After(e) => fold_expr(e),
    Expr::Apply { f, args } => {
      fold_expr(f);
      args.iter_mut().for_each(fold_expr);
    }
  }
}

/// The open tag of an element whose attributes are all literals, none of them
/// a marker the renderer answers for, that every set of markup rules prints
/// alike: a component is baked once and renders under whichever rules its
/// caller is under. `None` keeps the element on the evaluating path, which is
/// never a different answer.
pub(super) fn baked_open(tag: &str, attrs: &[Entry]) -> Option<String> {
  if Markup::every().any(|markup| markup.may_hoist(tag)) || VOID.contains(&tag) {
    return None;
  }
  let mut open = String::with_capacity(tag.len() + 16);
  open.push('<');
  open.push_str(tag);
  let mut written: Vec<&str> = Vec::new();
  for entry in attrs {
    let Entry::Field(name, crate::ast::Expr::Lit(lit)) = entry else { return None };
    let name = html_attr_name(name);
    if name.starts_with('$') {
      return None;
    }
    if written.contains(&name) {
      return None;
    }
    written.push(name);
    if skipped_attr(name) {
      continue;
    }
    let value = match lit {
      crate::ast::Lit::Null => Value::Null,
      crate::ast::Lit::Undefined => Value::Undefined,
      crate::ast::Lit::Bool(b) => Value::Bool(*b),
      crate::ast::Lit::Int(n) => Value::Int(*n),
      crate::ast::Lit::Float(f) => Value::F64(*f),
      crate::ast::Lit::Str(text) => Value::str(text.as_str()),
    };
    let mut agreed: Option<String> = None;
    for markup in Markup::every() {
      let mut piece = String::new();
      attribute(markup, tag, name, &value, &mut piece).ok()?;
      match &agreed {
        Some(first) if *first != piece => return None,
        Some(_) => {}
        None => agreed = Some(piece),
      }
    }
    open.push_str(&agreed.unwrap_or_default());
  }
  open.push('>');
  Some(open)
}

/// `attrs` with each run of literal attributes every markup prints the same way printed once, as `Entry::Baked`, when nothing on the element could reorder or override one: no spread, no computed name, no repeated name, no `$` attribute and a tag no markup hoists.
pub(super) fn baked_attrs(tag: &str, attrs: &[Entry]) -> Vec<Entry> {
  if Markup::every().any(|markup| markup.may_hoist(tag)) {
    return attrs.to_vec();
  }
  let mut names: Vec<&str> = Vec::new();
  for entry in attrs {
    let Entry::Field(name, _) = entry else { return attrs.to_vec() };
    let name = html_attr_name(name);
    if name.starts_with('$') || names.contains(&name) {
      return attrs.to_vec();
    }
    names.push(name);
  }
  let mut out: Vec<Entry> = Vec::with_capacity(attrs.len());
  for entry in attrs {
    let Entry::Field(name, crate::ast::Expr::Lit(lit)) = entry else {
      out.push(entry.clone());
      continue;
    };
    let name = html_attr_name(name);
    let piece = if skipped_attr(name) {
      String::new()
    } else {
      let value = match lit {
        crate::ast::Lit::Null => Value::Null,
        crate::ast::Lit::Undefined => Value::Undefined,
        crate::ast::Lit::Bool(b) => Value::Bool(*b),
        crate::ast::Lit::Int(n) => Value::Int(*n),
        crate::ast::Lit::Float(f) => Value::F64(*f),
        crate::ast::Lit::Str(text) => Value::str(text.as_str()),
      };
      let mut agreed: Option<String> = None;
      let mut same = true;
      for markup in Markup::every() {
        let mut piece = String::new();
        if attribute(markup, tag, name, &value, &mut piece).is_err() {
          same = false;
          break;
        }
        match &agreed {
          Some(first) if *first != piece => {
            same = false;
            break;
          }
          Some(_) => {}
          None => agreed = Some(piece),
        }
      }
      if !same {
        out.push(entry.clone());
        continue;
      }
      agreed.unwrap_or_default()
    };
    match out.last_mut() {
      Some(Entry::Baked(run)) => run.push_str(&piece),
      _ => out.push(Entry::Baked(piece)),
    }
  }
  out
}

/// Attribute keys the browser owns or that name no attribute: handlers, `key`, `ref`, `children` and a spread's `dangerouslySetInnerHTML`.
pub(super) fn skipped_attr(name: &str) -> bool {
  name == "key" || name == "ref" || name == "children" || name == "dangerouslySetInnerHTML" || name.starts_with('$') || (name.len() > 2 && name.starts_with("on") && name.as_bytes()[2].is_ascii_uppercase())
}
