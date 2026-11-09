use super::*;

/// The children `Children.toArray` sees in `children`, as values, into `out`: text as a string, each element, component or fragment a markup value made in the scope it renders in, an array's items in turn, a `{children}` passed through as what its own placement passed.
pub(super) fn child_values(env: &mut Env, children: &[Tmpl], slots: &[Slot<'_>], frame: usize, out: &mut Vec<Value>) -> Result<(), Fail> {
  for child in children {
    match child {
      Tmpl::Let { name, then, .. } if name.strip_prefix(SLOT_CONTENT_PREFIX) == Some("default") => child_value(env, then, slots, frame, out)?,
      Tmpl::Let { name, .. } if name.starts_with(SLOT_CONTENT_PREFIX) => {}
      other => child_value(env, other, slots, frame, out)?,
    }
  }
  Ok(())
}

pub(super) fn child_value(env: &mut Env, t: &Tmpl, slots: &[Slot<'_>], frame: usize, out: &mut Vec<Value>) -> Result<(), Fail> {
  match t {
    Tmpl::Text(text) if text.is_empty() => {}
    Tmpl::Marker(_) | Tmpl::Outlet { .. } => {}
    Tmpl::Text(text) => out.push(Value::str(text.as_str())),
    Tmpl::Element { .. } | Tmpl::Component { .. } | Tmpl::Island { .. } | Tmpl::Baked { .. } | Tmpl::Fragment(_) | Tmpl::Html(_) | Tmpl::Place { .. } => out.push(env.make(&Arc::new(t.clone()))?),
    Tmpl::Let { name, then, .. } if name == ARRAY_ITEMS => match &**then {
      Tmpl::Fragment(items) => child_values(env, items, slots, frame, out)?,
      other => child_value(env, other, slots, frame, out)?,
    },
    Tmpl::Let { name, expr, then } => {
      let value = env.eval_sync(expr)?;
      env.scope.push((Name::new(name), value));
      let result = child_value(env, then, slots, frame, out);
      env.scope.pop();
      result?;
    }
    Tmpl::If { cond, then, r#else } => match crate::interp::truthy(&env.eval_sync(cond)?) {
      true => child_value(env, then, slots, frame, out)?,
      false => {
        if let Some(other) = r#else {
          child_value(env, other, slots, frame, out)?;
        }
      }
    },
    Tmpl::For { over, params, body } => {
      let items = match env.eval_sync(over)? {
        Value::Seq(items) if env.heap_live => ValueSeq::from(items.to_vec()),
        Value::Seq(items) => items,
        other => return Err(crate::interp::type_error("map", "an array", &other)),
      };
      let depth = env.scope.len();
      for (i, item) in items.iter().enumerate() {
        env.scope.truncate(depth);
        for (param, value) in params.iter().zip([item.clone(), Value::F64(i as f64)]) {
          env.scope.push((Name::new(param), value));
        }
        child_value(env, body, slots, frame, out)?;
      }
      env.scope.truncate(depth);
    }
    Tmpl::Expr(e) => flatten_child(env.eval_sync(e)?, out),
    Tmpl::Slot(_) => {
      if let Some((at, slot)) = frame.checked_sub(1).and_then(|at| slots.get(at).map(|slot| (at, slot))) {
        let scope = std::mem::replace(&mut env.scope, (*slot.scope).clone());
        let path = std::mem::replace(&mut env.component_path, slot.owner_path.clone());
        let framework = std::mem::replace(&mut env.in_framework, slot.framework);
        let scoped = std::mem::replace(&mut env.scoped, slot.scoped.clone());
        let keys = match (&slot.keys, &mut env.hoists) {
          (Some((module, path)), Some(h)) => Some((std::mem::replace(&mut h.module, module.clone()), std::mem::replace(&mut h.path, path.clone()))),
          _ => None,
        };
        let result = child_values(env, slot.children, slots, at, out);
        if let (Some((module, path)), Some(h)) = (keys, &mut env.hoists) {
          h.module = module;
          h.path = path;
        }
        env.scope = scope;
        env.component_path = path;
        env.in_framework = framework;
        env.scoped = scoped;
        result?;
      }
    }
  }
  Ok(())
}

pub(super) fn flatten_child(value: Value, out: &mut Vec<Value>) {
  match value {
    Value::Null | Value::Undefined | Value::Bool(_) => {}
    Value::Seq(items) => items.into_items().into_iter().for_each(|item| flatten_child(item, out)),
    other => out.push(other),
  }
}

/// Whether `value` is a markup value, `isValidElement`.
pub(crate) fn is_made(value: &Value) -> bool {
  made_index(value).is_some()
}

impl Env {
  /// `cloneElement(element, props)`: the markup value with `props` set on its root, later than what it was written with so they win; anything else as it is.
  pub(crate) fn clone_element(&mut self, args: Vec<Value>) -> Result<Value, Fail> {
    let mut args = args.into_iter();
    let element = args.next().unwrap_or(Value::Null);
    let Some(index) = made_index(&element) else { return Ok(element) };
    let props = match args.next() {
      Some(Value::Map(map)) => map,
      _ => ValueMap::default(),
    };
    let Some(mut made) = self.made.as_ref().and_then(|table| table.get(index)).cloned() else {
      return Err(Fail::internal("a markup value from another render"));
    };
    let extra: Vec<Entry> = props.iter().filter(|(name, _)| name.as_str() != "children").map(|(name, value)| Entry::Field(name.to_string(), crate::ast::Expr::Folded(value.clone()))).collect();
    let Some(cloned) = with_props(&made.tmpl, &extra) else { return Ok(element) };
    made.tmpl = Arc::new(cloned);
    let table = self.made.as_mut().expect("read above");
    table.push(made);
    Ok(made_value(table.len() - 1))
  }
}

/// `t`'s root with `extra` after its own attributes or props, a baked open tag read back into the attributes it was baked from; `None` for a root that takes none.
pub(super) fn with_props(t: &Tmpl, extra: &[Entry]) -> Option<Tmpl> {
  Some(match t {
    Tmpl::Element { tag, attrs, children } => Tmpl::Element { tag: tag.clone(), attrs: attrs.iter().chain(extra).cloned().collect(), children: children.clone() },
    Tmpl::Baked { open, children, .. } => {
      let (tag, attrs) = unbake(open)?;
      Tmpl::Element { tag, attrs: attrs.into_iter().chain(extra.iter().cloned()).collect(), children: children.clone() }
    }
    Tmpl::Component { module, props, children, id, keyed } => Tmpl::Component { module: module.clone(), props: props.iter().chain(extra).cloned().collect(), children: children.clone(), id: *id, keyed: *keyed },
    Tmpl::Fragment(items) if items.len() == 1 => with_props(&items[0], extra)?,
    _ => return None,
  })
}

/// A baked open tag, `<p class="a" hidden>`, as its tag and the attributes it was baked from.
pub(super) fn unbake(open: &str) -> Option<(String, Vec<Entry>)> {
  let body = open.strip_prefix('<')?.trim_end_matches('>').trim_end_matches('/').trim_end();
  let (tag, mut rest) = body.split_once(' ').map(|(t, r)| (t, r)).unwrap_or((body, ""));
  let mut attrs = Vec::new();
  loop {
    rest = rest.trim_start();
    if rest.is_empty() {
      break;
    }
    let end = rest.find(|c: char| c == '=' || c == ' ').unwrap_or(rest.len());
    let name = &rest[..end];
    rest = &rest[end..];
    let value = match rest.strip_prefix("=\"") {
      Some(quoted) => {
        let close = quoted.find('"')?;
        let text = quoted[..close].replace("&quot;", "\"").replace("&#x27;", "'").replace("&#39;", "'").replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&");
        rest = &quoted[close + 1..];
        crate::ast::Lit::Str(text)
      }
      None => crate::ast::Lit::Bool(true),
    };
    attrs.push(Entry::Field(name.to_owned(), crate::ast::Expr::Lit(value)));
  }
  Some((tag.to_owned(), attrs))
}

/// How many children `Children.count` (`empties`) or `Children.toArray` sees in `children`, evaluated in the scope they render in: `null`, `undefined` and booleans count one each under `empties` and none otherwise, an array counts its items, `<>…</>` counts one and a `{children}` passed through counts what its own placement passed, `frame` being how many slot frames are open around it.
pub(super) fn count_children(env: &mut Env, children: &[Tmpl], slots: &[Slot<'_>], frame: usize, empties: bool) -> Result<usize, Fail> {
  let mut n = 0;
  for child in children {
    n += match child {
      Tmpl::Let { name, then, .. } if name.strip_prefix(SLOT_CONTENT_PREFIX) == Some("default") => count_child(env, then, slots, frame, empties)?,
      Tmpl::Let { name, .. } if name.starts_with(SLOT_CONTENT_PREFIX) => 0,
      other => count_child(env, other, slots, frame, empties)?,
    };
  }
  Ok(n)
}

pub(super) fn count_child(env: &mut Env, t: &Tmpl, slots: &[Slot<'_>], frame: usize, empties: bool) -> Result<usize, Fail> {
  Ok(match t {
    Tmpl::Text(text) => usize::from(!text.is_empty()),
    Tmpl::Marker(_) | Tmpl::Outlet { .. } => 0,
    Tmpl::Element { .. } | Tmpl::Component { .. } | Tmpl::Island { .. } | Tmpl::Baked { .. } | Tmpl::Html(_) | Tmpl::Place { .. } => 1,
    Tmpl::Fragment(items) => usize::from(empties || !items.is_empty()),
    Tmpl::Let { name, then, .. } if name == ARRAY_ITEMS => match &**then {
      Tmpl::Fragment(items) => count_children(env, items, slots, frame, empties)?,
      other => count_child(env, other, slots, frame, empties)?,
    },
    Tmpl::Let { name, expr, then } => {
      let value = env.eval_sync(expr)?;
      env.scope.push((Name::new(name), value));
      let n = count_child(env, then, slots, frame, empties);
      env.scope.pop();
      n?
    }
    Tmpl::If { cond, then, r#else } => match crate::interp::truthy(&env.eval_sync(cond)?) {
      true => count_child(env, then, slots, frame, empties)?,
      false => match r#else {
        Some(other) => count_child(env, other, slots, frame, empties)?,
        None => usize::from(empties),
      },
    },
    Tmpl::For { over, params, body } => {
      let items = match env.eval_sync(over)? {
        Value::Seq(items) if env.heap_live => ValueSeq::from(items.to_vec()),
        Value::Seq(items) => items,
        other => return Err(crate::interp::type_error("map", "an array", &other)),
      };
      let depth = env.scope.len();
      let mut n = 0;
      for (i, item) in items.iter().enumerate() {
        env.scope.truncate(depth);
        for (param, value) in params.iter().zip([item.clone(), Value::F64(i as f64)]) {
          env.scope.push((Name::new(param), value));
        }
        n += count_child(env, body, slots, frame, empties)?;
      }
      env.scope.truncate(depth);
      n
    }
    Tmpl::Expr(e) => count_value(&env.eval_sync(e)?, empties),
    Tmpl::Slot(_) => match frame.checked_sub(1).and_then(|at| slots.get(at).map(|slot| (at, slot))) {
      Some((at, slot)) => {
        let inner = std::mem::replace(&mut env.scope, (*slot.scope).clone());
        let n = count_children(env, slot.children, slots, at, empties);
        env.scope = inner;
        n?
      }
      None => 0,
    },
  })
}

pub(super) fn count_value(value: &Value, empties: bool) -> usize {
  match value {
    Value::Null | Value::Undefined | Value::Bool(_) => usize::from(empties),
    Value::Seq(items) => items.iter().map(|item| count_value(item, empties)).sum(),
    _ => 1,
  }
}
