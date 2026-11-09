use super::*;

/// Runs `f` with the hoist path extended by `step`: the position of the
/// iteration whose values are recorded inside or the keyed placement that
/// rendered the component.
pub(super) fn in_step<T>(env: &mut Env, step: Step, f: impl FnOnce(&mut Env) -> T) -> T {
  if let Some(h) = &mut env.hoists {
    h.path.push(step);
  }
  let result = f(env);
  if let Some(h) = &mut env.hoists {
    h.path.pop();
  }
  result
}

/// Runs `f` with the hoist module set to `module`. The path carries on, so a
/// component placed from a caller's loop keys its ids below that iteration.
pub(super) fn in_module<T>(env: &mut Env, module: &str, f: impl FnOnce(&mut Env) -> T) -> T {
  let outer = env.hoists.as_mut().map(|h| std::mem::replace(&mut h.module, module.to_owned()));
  let result = f(env);
  if let (Some(h), Some(module)) = (&mut env.hoists, outer) {
    h.module = module;
  }
  result
}

pub(super) fn call(env: &mut Env, module: &str, f: impl FnOnce(&mut Env) -> Result<(), Fail>) -> Result<(), Fail> {
  if env.calls == MAX_CALLS {
    return Err(Fail::internal(format!("`{module}` is nested {MAX_CALLS} components deep, which a render refuses: a component that renders itself needs a case that renders no further")));
  }
  env.calls += 1;
  let result = crate::interp::on_enough_stack(|| f(env));
  env.calls -= 1;
  result
}

pub(super) fn render_component<'a>(env: &mut Env, component: &'a Component, library: &'a Components, slots: &mut Vec<Slot<'a>>, out: &mut Out) -> Result<(), Fail> {
  let outer = env.in_framework;
  env.in_framework |= component.owner.hydrates();
  let entered = env.hoists.as_ref().map(|h| h.path_key()).unwrap_or_default();
  let outer_path = std::mem::replace(&mut env.component_path, entered);
  let result = render_component_body(env, component, library, slots, out);
  env.component_path = outer_path;
  env.in_framework = outer;
  result
}

pub(super) fn render_component_body<'a>(env: &mut Env, component: &'a Component, library: &'a Components, slots: &mut Vec<Slot<'a>>, out: &mut Out) -> Result<(), Fail> {
  let depth = env.scope.len();
  let caller = env.markup;
  env.markup = Markup::of(component.owner, env.frameworks, caller);
  let path = env.hoists.as_ref().map(|h| h.path_key()).unwrap_or_default();
  if env.server_mode {
    let module = env.hoists.as_ref().map(|h| h.module.clone()).unwrap_or_default();
    env.entered.push((path.clone(), module));
  }
  if env.probe.as_ref().is_some_and(|probe| probe.found.is_none() && probe.path == path) {
    let module = env.hoists.as_ref().map(|h| h.module.clone()).unwrap_or_default();
    let props = env.scope.iter().rev().find(|(name, _)| name == "$props").and_then(|(_, value)| match value {
      Value::Map(map) => Some(map.clone()),
      _ => None,
    });
    if let Some(probe) = &mut env.probe {
      probe.found = Some((module, props.unwrap_or_default()));
    }
  }
  bind_body(env, component, &path, depth, out)?;
  // Rendering enters Svelte where Svelte's `hydrate` mounts it.
  let svelte_root = env.markup.is_svelte() && !caller.is_svelte();
  if svelte_root {
    out.markup(svelte::ROOT_OPEN);
  }
  render(env, &component.render, library, slots, out)?;
  if svelte_root {
    out.markup(svelte::ROOT_CLOSE);
  }
  env.scope.truncate(depth);
  env.markup = caller;
  Ok(())
}

/// Binds the body's `let`s, running it again while a pass asks for new state (`RERUN_PREFIX`). Its own function so the passes' locals are off the stack while the tree renders.
pub(super) fn bind_body(env: &mut Env, component: &Component, path: &str, depth: usize, out: &mut Out) -> Result<(), Fail> {
  let mut next: Vec<(String, Value)> = Vec::new();
  let issued = env.emitted.len();
  for pass in 1.. {
    let mut asked = false;
    env.emitted.truncate(issued);
    for stmt in &component.body {
      let Stmt::Let { name, expr } = stmt else {
        return Err(Fail::internal("a component body is `let`s only"));
      };
      match directive(name) {
        Some(Directive::Hold(state)) => {
          let value = env.eval_sync(expr)?;
          out.state.insert(state_key(path, state), value);
          continue;
        }
        Some(Directive::Rerun(state)) => {
          if let Value::Seq(value) = env.eval_sync(expr)? {
            let value = value.first().cloned().unwrap_or(Value::Null);
            match next.iter_mut().find(|(n, _)| n == state) {
              Some(held) => held.1 = value,
              None => next.push((state.to_owned(), value)),
            }
            asked = true;
          }
          continue;
        }
        None => {}
      }
      let held_here = component.state.contains(name) && component.store_key(name).is_none();
      let value = match next.iter().find(|(n, _)| n == name) {
        Some((_, value)) => value.clone(),
        None => match if held_here { env.state.as_ref().and_then(|s| s.get(&state_key(path, name))).cloned() } else { None } {
          Some(held) => held,
          None => env.eval_sync(expr)?,
        },
      };
      if held_here {
        out.state.insert(state_key(path, name), value.clone());
      }
      env.scope.push((Name::new(name), value));
    }
    if !asked {
      break;
    }
    if pass == RERUN_LIMIT {
      return Err(Fail::internal("Too many re-renders. React limits the number of renders to prevent an infinite loop."));
    }
    env.scope.truncate(depth);
  }
  Ok(())
}

/// Evaluates attribute or prop entries into one map in order, later entries winning; attributes are keyed by HTML spelling so a spread's `className` and a literal `class` are one key.
/// A component's or an island's props, built straight into the map the callee
/// reads. The map is the deduplication, so there is no intermediate vector and
/// no linear scan for a name already written.
pub(super) fn props(env: &mut Env, entries: &[Entry]) -> Result<ValueMap, Fail> {
  let mut map = snapfire_fsr_core::Fields::with_capacity(entries.len());
  for entry in entries {
    match entry {
      Entry::Baked(_) => {}
      Entry::Field(name, _) if name == UNLOWERED_ATTR => {}
      Entry::Field(name, expr) => {
        let value = env.eval_sync(expr)?;
        if name != "children" {
          map.insert(name.as_str(), value);
        }
      }
      Entry::Spread(expr) => match env.eval_sync(expr)? {
        Value::Map(spread) => {
          for (name, value) in spread {
            if name != "children" {
              map.insert(name, value);
            }
          }
        }
        Value::Null | Value::Undefined => {}
        other => return Err(crate::interp::type_error("spread", "an object", &other)),
      },
      Entry::Computed(key, expr) => {
        let key = stringify(&env.eval_sync(key)?)?;
        let value = env.eval_sync(expr)?;
        if key != "children" {
          map.insert(key, value);
        }
      }
      Entry::Item(_) => return Err(Fail::internal("an item entry among props")),
    }
  }
  Ok(ValueMap::from(map))
}

/// The entries in order with a later one of a name taking its value. Beside
/// them, whether each name came from data (a spread or a computed key); such a
/// name never names one of the renderer's own attributes.
pub(super) fn entries<'a>(env: &mut Env, entries: &'a [Entry], attrs: bool) -> Result<(Vec<(Cow<'a, str>, Value)>, Vec<bool>), Fail> {
  let mut out: Vec<(Cow<'a, str>, Value)> = Vec::with_capacity(entries.len());
  let mut from_data: Vec<bool> = Vec::with_capacity(entries.len());
  let mut put = |name: Cow<'a, str>, value: Value, data: bool| {
    let name = match attrs {
      true => match html_attr_name(&name) {
        html if html.len() == name.len() && html == name => name,
        html => Cow::Owned(html.to_owned()),
      },
      false => name,
    };
    if let Some(at) = out.iter().position(|(n, _)| *n == name) {
      out[at].1 = value;
      from_data[at] = data;
    } else {
      out.push((name, value));
      from_data.push(data);
    }
  };
  for entry in entries {
    match entry {
      Entry::Baked(text) => put(Cow::Borrowed(text.as_str()), Value::Null, false),
      Entry::Field(name, expr) => put(Cow::Borrowed(name.as_str()), env.eval_sync(expr)?, false),
      Entry::Spread(expr) => match env.eval_sync(expr)? {
        Value::Map(map) => {
          for (name, value) in map {
            if !is_baked_name(&name) {
              put(Cow::Owned(name.to_string()), value, true);
            }
          }
        }
        Value::Null | Value::Undefined => {}
        other => return Err(crate::interp::type_error("spread", "an object", &other)),
      },
      Entry::Computed(key, expr) => {
        let key = stringify(&env.eval_sync(key)?)?;
        let value = env.eval_sync(expr)?;
        if !is_baked_name(&key) {
          put(Cow::Owned(key), value, true);
        }
      }
      Entry::Item(_) => return Err(Fail::internal("an item entry among attributes")),
    }
  }
  Ok((out, from_data))
}

/// The larger arms are functions of their own. This recurses once per level
/// of the tree and a debug build gives every arm's locals a slot in its one
/// frame, so an arm inline here costs its stack at every level.
pub(super) fn render<'a>(env: &mut Env, tmpl: &'a Tmpl, library: &'a Components, slots: &mut Vec<Slot<'a>>, out: &mut Out) -> Result<(), Fail> {
  match tmpl {
    Tmpl::Text(text) => out.text(text, env.markup),
    Tmpl::Marker(markup) => out.markup(markup),
    Tmpl::Html(_) | Tmpl::Place { .. } | Tmpl::Outlet { .. } => render_aside(env, tmpl, library, slots, out)?,
    Tmpl::Expr(expr) => render_expr(env, expr, library, out)?,
    Tmpl::Element { tag, attrs, children } => render_stamped_element(env, tag, attrs, children, library, slots, out)?,
    Tmpl::Baked { open, tag, children } => {
      write_baked_open(env, open, out)?;
      if let Some(tag) = tag {
        let context = enter(env, tag);
        for child in children {
          render(env, child, library, slots, out)?;
        }
        (env.in_svg, env.in_noscript) = context;
        out.close_tag(tag);
      }
    }
    Tmpl::Fragment(children) => {
      env.stamp.clear();
      env.fallthrough = None;
      for child in children {
        render(env, child, library, slots, out)?;
      }
    }
    Tmpl::If { cond, then, r#else } => {
      if env.truth_of(cond)? {
        render(env, then, library, slots, out)?;
        if let Some(other) = r#else {
          record_untaken(env, other, library, slots);
        }
      } else {
        if let Some(other) = r#else {
          render(env, other, library, slots, out)?;
        }
        record_untaken(env, then, library, slots);
      }
    }
    Tmpl::For { over, params, body } => render_for(env, over, params, body, library, slots, out)?,
    Tmpl::Let { name, expr, then } if name.starts_with(SLOT_OUT_PREFIX) => render_vue_slot(env, &name[SLOT_OUT_PREFIX.len()..], expr, then, library, slots, out)?,
    Tmpl::Let { name, expr, .. } if name.starts_with(SLOT_READ_PREFIX) => render_slot_read(env, &name[SLOT_READ_PREFIX.len()..], expr, library, slots, out)?,
    Tmpl::Let { name, .. } if name.starts_with(SLOT_CONTENT_PREFIX) => {}
    Tmpl::Let { name, then, .. } if name == ARRAY_ITEMS => render(env, then, library, slots, out)?,
    Tmpl::Let { name, expr, then } => {
      let value = env.eval_sync(expr)?;
      let depth = env.scope.len();
      let provided = env.contexts.len();
      if let Some(context) = name.strip_prefix(CONTEXT_PREFIX) {
        env.contexts.push((context.to_owned(), value.clone()));
      }
      env.scope.push((Name::new(name), value));
      render(env, then, library, slots, out)?;
      env.scope.truncate(depth);
      env.contexts.truncate(provided);
    }
    Tmpl::Component { module, props, children, id, keyed } => render_call(env, module, props, children, *id, *keyed, library, slots, out)?,
    Tmpl::Island { .. } => render_island(env, tmpl, library, slots, out)?,
    Tmpl::Slot(name) => render_slot(env, name, library, slots, out)?,
  }
  Ok(())
}

/// `Html`, `Place` and `Outlet`, which share one arm so `render`'s frame keeps one slot for them.
pub(super) fn render_aside<'a>(env: &mut Env, tmpl: &'a Tmpl, library: &'a Components, slots: &mut Vec<Slot<'a>>, out: &mut Out) -> Result<(), Fail> {
  match tmpl {
    Tmpl::Place { target, children } => render_place(env, target, children, library, slots, out),
    Tmpl::Html(expr) => render_html(env, expr, out),
    Tmpl::Outlet { name, hydrated } => {
      if env.in_framework && !hydrated {
        return Err(Fail::internal(format!("the outlet for `#{name}` is in markup the browser hydrates and nothing says that framework keeps what is written there")));
      }
      out.markup(&format!("{OUTLET_MARK}{name}\u{0}"));
      Ok(())
    }
    _ => unreachable!("render passes only `Html`, `Place` and `Outlet`"),
  }
}

pub(super) fn render_html(env: &mut Env, expr: &crate::ast::Expr, out: &mut Out) -> Result<(), Fail> {
  match env.eval_sync(expr)? {
    Value::Null | Value::Undefined => {}
    value if env.markup.is_svelte() => out.markup(&svelte::display(&value)?),
    value => out.markup(&stringify(&value)?),
  }
  Ok(())
}

pub(super) fn render_expr(env: &mut Env, expr: &crate::ast::Expr, library: &Components, out: &mut Out) -> Result<(), Fail> {
  let markup = env.markup;
  match env.place(expr) {
    Some(value) if !holds_made(value) => interpolate(value, out, markup),
    Some(value) => {
      let value = value.clone();
      render_value(env, &value, library, out)
    }
    None => {
      let value = env.eval_sync(expr)?;
      render_value(env, &value, library, out)
    }
  }
}

pub(super) fn render_element<'a>(env: &mut Env, tag: &str, attrs: &[Entry], children: &'a [Tmpl], library: &'a Components, slots: &mut Vec<Slot<'a>>, out: &mut Out) -> Result<(), Fail> {
  let mut open = String::with_capacity(tag.len() + 32);
  open.push('<');
  open.push_str(tag);
  let mut bound = Vec::new();
  let mut raw: Option<String> = None;
  let markup = env.markup;
  let (evaluated, from_data) = entries(env, attrs, !markup.is_vue() && !markup.is_svelte())?;
  if markup.may_hoist(tag) && !env.server_mode && !env.in_svg && !env.in_noscript && markup.hoists(tag, &evaluated) {
    return Err(hoisted_in_place(env, tag));
  }
  let shadow = match evaluated.iter().zip(&from_data).find(|((name, _), data)| !**data && name.as_ref() == SHADOW_ATTR) {
    Some(((_, module), _)) => Some((stringify(module)?, evaluated.iter().filter(|(name, _)| !name.starts_with('$')).map(|(name, value)| (name.to_string(), value.clone())).collect::<ValueMap>())),
    None => None,
  };
  let mut selected = false;
  let flags = match markup.is_svelte() {
    true => evaluated.iter().zip(&from_data).find(|((name, _), data)| !**data && name.as_ref() == SVELTE_FLAGS).map_or(0, |((_, v), _)| match v {
      Value::Int(n) => *n as u32,
      _ => 0,
    }),
    false => 0,
  };
  let spread = match markup.is_svelte() && from_data.iter().any(|data| *data) {
    true => Some((flags, evaluated.iter().zip(&from_data).filter(|(_, data)| **data).map(|((name, _), _)| name.to_string()).collect::<Vec<String>>())),
    false => None,
  };
  for ((name, value), data) in evaluated.into_iter().zip(from_data) {
    if name.starts_with(' ') || name.is_empty() {
      open.push_str(&name);
      continue;
    }
    if let (true, Some((flags, names))) = (data, &spread) {
      svelte::spread_attribute(&name, &value, *flags, names, &mut open)?;
      continue;
    }
    if data && name.starts_with('$') {
      continue;
    }
    if shadow.is_some() && !is_scalar(&value) {
      continue;
    }
    if let Some(event) = name.strip_prefix(HANDLER_ATTR) {
      if env.server_mode {
        let path = &env.component_path;
        match (&value, handler_token(&value)) {
          (_, Some(token)) => bound.push(format!("{event}:{token}")),
          (Value::Int(_), None) if path.is_empty() => bound.push(format!("{event}:{}", stringify(&value)?)),
          (Value::Int(_), None) => bound.push(format!("{event}:{path}/{}", stringify(&value)?)),
          _ => {}
        }
      }
      continue;
    }
    if handler_token(&value).is_some() {
      continue;
    }
    if name == KEY_ATTR {
      if env.server_mode {
        attribute(markup, tag, "data-sf-key", &value, &mut open)?;
      }
      continue;
    }
    if name == RAW_ATTR {
      raw = Some(match value {
        Value::Null | Value::Undefined => String::new(),
        value => stringify(&value)?,
      });
      continue;
    }
    if markup.is_svelte() {
      if name == SVELTE_SELECTED {
        selected = truthy(&value);
      } else if !name.starts_with('$') {
        attribute(markup, tag, &name, &value, &mut open)?;
      }
      continue;
    }
    if skipped_attr(&name) {
      continue;
    }
    attribute(markup, tag, &name, &value, &mut open)?;
  }
  if selected {
    open.push_str(" selected=\"\"");
  }
  if !bound.is_empty() {
    attribute(markup, tag, "data-sf-on", &Value::str(bound.join(" ")), &mut open)?;
  }
  if VOID.contains(&tag) {
    open.push_str(if matches!(markup, Markup::React(_)) || (markup.is_svelte() && flags & svelte::DYNAMIC_ELEMENT == 0) { "/>" } else { ">" });
    out.markup(&open);
    return Ok(());
  }
  open.push('>');
  out.markup(&open);
  if let Some((module, props)) = shadow {
    render_shadow(env, &module, props, library, slots, out)?;
  }
  let context = enter(env, tag);
  match chunk_id(attrs) {
    Some(id) => {
      let mut inner = Out::default();
      match &raw {
        Some(html) => inner.markup(html),
        None => {
          for child in children {
            render(env, child, library, slots, &mut inner)?;
          }
        }
      }
      out.islands.extend(std::mem::take(&mut inner.islands));
      out.markup(&inner.html);
      if let Some(hoists) = &mut env.hoists {
        hoists.record(id, Value::str(inner.html));
      }
    }
    None => match &raw {
      Some(html) => out.markup(html),
      None => {
        for child in children {
          render(env, child, library, slots, out)?;
        }
      }
    },
  }
  (env.in_svg, env.in_noscript) = context;
  out.close_tag(tag);
  Ok(())
}

/// Enters `tag` for the context React 19 reads before moving `<title>`,
/// `<meta>` and `<link>` into the head. Returns the context it replaced.
pub(super) fn enter(env: &mut Env, tag: &str) -> (bool, bool) {
  let held = (env.in_svg, env.in_noscript);
  match tag {
    "svg" => env.in_svg = true,
    "foreignObject" => env.in_svg = false,
    "noscript" => env.in_noscript = true,
    _ => {}
  }
  held
}

/// React 19 moves this element into the document head, so the markup the
/// server wrote and the tree the browser builds would not agree.
pub(super) fn hoisted_in_place(env: &Env, tag: &str) -> Fail {
  let writer = match env.hoists.as_ref().map(|h| h.module.as_str()) {
    Some(module) if !module.is_empty() => format!("`{module}`"),
    _ => "a component".to_owned(),
  };
  Fail::internal(format!("{writer} writes `<{tag}>` in markup the browser hydrates; React 19 moves it into the document head, so it belongs in a loader's `meta` export"))
}

/// A custom element's shadow template, written as a declarative shadow root
/// at the start of the element. It renders with the element's attributes as
/// its props and under plain markup, since the parser takes the shadow root
/// and no framework sees it. The caller writes the light children after it,
/// so a `{children}` in the template is nothing: `<slot>` shows them. Nothing
/// in it is recorded among an island's hoisted values, since the island's
/// browser half never renders a shadow root.
pub(super) fn render_shadow<'a>(env: &mut Env, module: &str, props: ValueMap, library: &'a Components, slots: &mut Vec<Slot<'a>>, out: &mut Out) -> Result<(), Fail> {
  let component = library.get(module).ok_or_else(|| Fail::internal(format!("`{module}` is not a lowered component")))?;
  let depth = env.scope.len();
  let outer = Rc::new(std::mem::replace(&mut env.scope, component_scope(Value::Map(props), Value::Map(ValueMap::default()))));
  let hoists = env.hoists.take();
  slots.push(Slot { children: &[], scope: Rc::clone(&outer), keys: None, island: false, placed: false, framework: false, owner_path: String::new(), scoped: None });
  let markup = std::mem::replace(&mut env.markup, Markup::Plain);
  shadow_open(component.shadow.unwrap_or_default(), out);
  let result = in_module(env, module, |env| call(env, module, |env| render_component(env, component, library, slots, out)));
  env.markup = markup;
  env.hoists = hoists;
  slots.pop();
  env.scope = Rc::try_unwrap(outer).unwrap_or_else(|held| (*held).clone());
  env.scope.truncate(depth);
  result?;
  out.close_tag("template");
  Ok(())
}

pub(super) fn shadow_open(shadow: ShadowRoot, out: &mut Out) {
  out.markup(match shadow.mode {
    ShadowMode::Open => "<template shadowrootmode=\"open\"",
    ShadowMode::Closed => "<template shadowrootmode=\"closed\"",
  });
  if shadow.delegates_focus {
    out.markup(" shadowrootdelegatesfocus");
  }
  if shadow.clonable {
    out.markup(" shadowrootclonable");
  }
  if shadow.serializable {
    out.markup(" shadowrootserializable");
  }
  out.markup(">");
}

pub(super) fn render_for<'a>(env: &mut Env, over: &crate::ast::Expr, params: &[String], body: &'a Tmpl, library: &'a Components, slots: &mut Vec<Slot<'a>>, out: &mut Out) -> Result<(), Fail> {
  let items = match env.eval_sync(over)? {
    Value::Seq(items) if env.heap_live => ValueSeq::from(items.to_vec()),
    Value::Seq(items) => items,
    other => return Err(crate::interp::type_error("map", "an array", &other)),
  };
  let depth = env.scope.len();
  for i in 0..items.len() {
    env.scope.truncate(depth);
    for (param, value) in params.iter().zip([items[i].clone(), Value::F64(i as f64)]) {
      env.scope.push((Name::new(param), value));
    }
    in_step(env, Step::Iteration(i), |env| render(env, body, library, slots, out))?;
  }
  env.scope.truncate(depth);
  Ok(())
}

/// Records the chunks marked `UNTAKEN_ATTR` in a branch the render did not
/// take: the browser may take it later, and a chunk reads nothing of the
/// state the branch turned on. Loops and bindings on the way are evaluated
/// as the render would, so the keys match. A chunk whose render fails is
/// left unrecorded: its branch guards a read the browser cannot take either.
/// The render's cells as they stand, for a pass beside the render that reads them as they are at its point and whose writes are dropped.
pub(super) fn hold_cells(env: &Env) -> (Option<Vec<Value>>, Vec<(String, usize)>) {
  (env.cells.clone(), env.module_cells.clone())
}

pub(super) fn restore_cells(env: &mut Env, (cells, module_cells): (Option<Vec<Value>>, Vec<(String, usize)>)) {
  env.cells = cells;
  env.module_cells = module_cells;
}

pub(super) fn record_untaken<'a>(env: &mut Env, tmpl: &'a Tmpl, library: &'a Components, slots: &mut Vec<Slot<'a>>) {
  if env.hoists.is_none() || !holds_chunk(tmpl, UNTAKEN_ATTR) {
    return;
  }
  let held = hold_cells(env);
  let _ = walk_chunks(env, tmpl, UNTAKEN_ATTR, library, slots);
  restore_cells(env, held);
}

/// Renders every element carrying `mark` in `tmpl` into nothing but the
/// hoist table, with the loops and bindings on the way evaluated as the
/// render would, so the keys match.
pub(super) fn walk_chunks<'a>(env: &mut Env, tmpl: &'a Tmpl, mark: &str, library: &'a Components, slots: &mut Vec<Slot<'a>>) -> Result<(), Fail> {
  match tmpl {
    Tmpl::Element { attrs, children, .. } => match attrs.iter().any(|entry| matches!(entry, Entry::Field(name, _) if name == mark)) {
      true => {
        let mut scratch = Out::default();
        let depth = env.scope.len();
        let stamp = std::mem::take(&mut env.stamp);
        let placed = env.placed.len();
        let result = render(env, tmpl, library, slots, &mut scratch);
        env.placed.truncate(placed);
        env.stamp = stamp;
        env.scope.truncate(depth);
        result
      }
      false => children.iter().try_for_each(|child| walk_chunks(env, child, mark, library, slots)),
    },
    Tmpl::Fragment(children) | Tmpl::Baked { children, .. } | Tmpl::Place { children, .. } => children.iter().try_for_each(|child| walk_chunks(env, child, mark, library, slots)),
    Tmpl::If { then, r#else, .. } => {
      walk_chunks(env, then, mark, library, slots)?;
      r#else.as_ref().map_or(Ok(()), |other| walk_chunks(env, other, mark, library, slots))
    }
    Tmpl::For { over, params, body } => {
      if !holds_chunk(body, mark) {
        return Ok(());
      }
      let Value::Seq(items) = env.eval_sync(over)? else { return Ok(()) };
      let items = if env.heap_live { ValueSeq::from(items.to_vec()) } else { items };
      let depth = env.scope.len();
      for i in 0..items.len() {
        env.scope.truncate(depth);
        for (param, value) in params.iter().zip([items[i].clone(), Value::F64(i as f64)]) {
          env.scope.push((Name::new(param), value));
        }
        let result = in_step(env, Step::Iteration(i), |env| walk_chunks(env, body, mark, library, slots));
        env.scope.truncate(depth);
        result?;
      }
      Ok(())
    }
    Tmpl::Let { name, expr, then } if !name.starts_with(SLOT_OUT_PREFIX) && !name.starts_with(SLOT_CONTENT_PREFIX) && !name.starts_with(CONTEXT_PREFIX) => {
      if !holds_chunk(then, mark) {
        return Ok(());
      }
      let value = env.eval_sync(expr)?;
      let depth = env.scope.len();
      env.scope.push((Name::new(name), value));
      let result = walk_chunks(env, then, mark, library, slots);
      env.scope.truncate(depth);
      result
    }
    Tmpl::Let { .. } | Tmpl::Text(_) | Tmpl::Expr(_) | Tmpl::Html(_) | Tmpl::Component { .. } | Tmpl::Island { .. } | Tmpl::Slot(_) | Tmpl::Marker(_) | Tmpl::Outlet { .. } => Ok(()),
  }
}

/// True when an element carrying `mark` sits in `tmpl`, outside any
/// component's or island's children.
pub(super) fn holds_chunk(tmpl: &Tmpl, mark: &str) -> bool {
  match tmpl {
    Tmpl::Element { attrs, children, .. } => attrs.iter().any(|entry| matches!(entry, Entry::Field(name, _) if name == mark)) || children.iter().any(|child| holds_chunk(child, mark)),
    Tmpl::Fragment(children) | Tmpl::Baked { children, .. } | Tmpl::Place { children, .. } => children.iter().any(|child| holds_chunk(child, mark)),
    Tmpl::If { then, r#else, .. } => holds_chunk(then, mark) || r#else.as_ref().is_some_and(|other| holds_chunk(other, mark)),
    Tmpl::For { body, .. } => holds_chunk(body, mark),
    Tmpl::Let { then, .. } => holds_chunk(then, mark),
    Tmpl::Text(_) | Tmpl::Expr(_) | Tmpl::Html(_) | Tmpl::Component { .. } | Tmpl::Island { .. } | Tmpl::Slot(_) | Tmpl::Marker(_) | Tmpl::Outlet { .. } => false,
  }
}

/// The hoist table of an island the server holds back for a pending store
/// key: every chunk of `component` rendered from `props` alone, since a
/// chunk reads nothing of the store, with each body binding that evaluates
/// without it in scope. The browser mounts the island fresh and adopts them.
pub(super) fn held_chunks<'a>(env: &mut Env, module: &str, component: &'a Component, props: &ValueMap, library: &'a Components, slots: &mut Vec<Slot<'a>>) -> ValueMap {
  if !holds_chunk(&component.render, CHUNK_ATTR) {
    return ValueMap::default();
  }
  let depth = env.scope.len();
  let outer = std::mem::replace(&mut env.scope, component_scope(Value::Map(props.clone()), Value::Map(ValueMap::default())));
  let outer_hoists = env.hoists.replace(Hoists::new(module.to_owned()));
  let outer_framework = std::mem::replace(&mut env.in_framework, true);
  let caller = env.markup;
  env.markup = Markup::of(component.owner, env.frameworks, caller);
  for stmt in &component.body {
    let Stmt::Let { name, expr } = stmt else { break };
    if let Ok(value) = env.eval_sync(expr) {
      env.scope.push((Name::new(name), value));
    }
  }
  let held = hold_cells(env);
  let _ = walk_chunks(env, &component.render, CHUNK_ATTR, library, slots);
  restore_cells(env, held);
  env.markup = caller;
  env.in_framework = outer_framework;
  env.scope = outer;
  env.scope.truncate(depth);
  std::mem::replace(&mut env.hoists, outer_hoists).map(|h| h.table).unwrap_or_default()
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render_call<'a>(env: &mut Env, module: &str, props: &[Entry], children: &'a [Tmpl], id: u32, keyed: bool, library: &'a Components, slots: &mut Vec<Slot<'a>>, out: &mut Out) -> Result<(), Fail> {
  let Some(component) = library.get(module) else {
    if env.in_framework {
      env.unrendered = true;
      return Ok(());
    }
    return render_placed(env, module, props, children, &None, &None, id, false, library, slots, out);
  };
  if !env.in_framework && component.owner.hydrates() {
    return render_placed(env, module, props, children, &None, &None, id, false, library, slots, out);
  }
  let mut map = self::props(env, props)?;
  for stmt in component.body.iter().filter(|stmt| !matches!(stmt, Stmt::Let { name, .. } if name == "$heap")).take(3) {
    let Stmt::Let { name, .. } = stmt else { break };
    let empties = match name.as_str() {
      CHILD_COUNT => true,
      CHILD_ITEMS => false,
      CHILD_VALUES => {
        let mut values = Vec::new();
        child_values(env, children, slots, slots.len(), &mut values)?;
        map.insert(name.as_str(), Value::seq(values));
        continue;
      }
      _ => break,
    };
    let count = count_children(env, children, slots, slots.len(), empties)?;
    map.insert(name.as_str(), Value::F64(count as f64));
  }
  let fallthrough = match map.shift_remove(FALLTHROUGH_PROP) {
    Some(Value::Map(attrs)) if component.owner == crate::ast::Owner::Vue => Some(attrs),
    _ => None,
  };
  let stamp = match map.get(PARENT_SCOPE_PROP) {
    _ if component.owner != crate::ast::Owner::Vue => Vec::new(),
    parent => {
      let mut stamp = match parent {
        Some(Value::Str(scope)) => vec![scope.to_string()],
        _ => Vec::new(),
      };
      stamp.extend(env.slot_stamps.iter().flatten().cloned());
      stamp
    }
  };
  let depth = env.scope.len();
  let given = given_slots(env, children)?;
  let outer = Rc::new(std::mem::replace(&mut env.scope, component_scope(Value::Map(map), given)));
  let keys = env.hoists.as_ref().map(|h| (h.module.clone(), h.path.clone()));
  slots.push(Slot { children, scope: Rc::clone(&outer), keys, island: false, placed: false, framework: env.in_framework, owner_path: env.component_path.clone(), scoped: env.scoped.clone() });
  let held = std::mem::replace(&mut env.stamp, stamp);
  let held_fallthrough = std::mem::replace(&mut env.fallthrough, fallthrough);
  let held_slotted = std::mem::take(&mut env.slotted);
  let held_stamps = env.slot_stamps.take();
  let held_scoped = std::mem::replace(&mut env.scoped, component.scope.clone());
  let mut body =|env: &mut Env| in_module(env, module, |env| call(env, module, |env| render_component(env, component, library, slots, out)));
  let result = if keyed { in_step(env, Step::Placement(id), body) } else { body(env) };
  env.stamp = held;
  env.fallthrough = held_fallthrough;
  env.slotted = held_slotted;
  env.slot_stamps = held_stamps;
  env.scoped = held_scoped;
  slots.pop();
  env.scope = Rc::try_unwrap(outer).unwrap_or_else(|held| (*held).clone());
  env.scope.truncate(depth);
  result
}
