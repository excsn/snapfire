use super::*;

/// An element whose tag `choice` holds: that element when it names one, the component when it references one.
pub(super) fn render_chosen<'a>(env: &mut Env, attrs: &[Entry], children: &'a [Tmpl], library: &'a Components, slots: &mut Vec<Slot<'a>>, out: &mut Out) -> Result<(), Fail> {
  let Some((Entry::Field(_, choice), rest)) = attrs.split_first() else {
    return Err(Fail::internal("a chosen tag without its choice"));
  };
  match env.eval_sync(choice)? {
    Value::Str(tag) if tag.starts_with(|c: char| c.is_ascii_lowercase()) && tag.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') => render_stamped_element(env, tag.as_str(), rest, children, library, slots, out),
    Value::Variant { tag, payload: Some(module) } if tag == COMPONENT_VALUE => match &*module {
      Value::Str(module) => render_call(env, module.as_str(), rest, children, 0, false, library, slots, out),
      other => Err(crate::interp::type_error("a tag", "a component", other)),
    },
    other => Err(crate::interp::type_error("a tag", "an element name or a component", &other)),
  }
}

/// `attrs` with `scope` before the element's own `data-v-` attribute, or last.
pub(super) fn render_stamped_element<'a>(env: &mut Env, tag: &str, attrs: &[Entry], children: &'a [Tmpl], library: &'a Components, slots: &mut Vec<Slot<'a>>, out: &mut Out) -> Result<(), Fail> {
  if tag == CHOSEN_TAG {
    return render_chosen(env, attrs, children, library, slots, out);
  }
  if tag == SUSPENSE_TAG {
    let [content, fallback] = children else { return Err(Fail::internal("a Suspense boundary holds its content and its fallback")) };
    let saved = out.clone();
    let depth = env.scope.len();
    let rendered = render(env, content, library, slots, out);
    let Err(fail) = rendered else { return Ok(()) };
    let deferred = fail.thrown.as_deref().filter(|thrown| matches!(thrown, Value::Map(map) if map.contains_key(DEFERRED))).cloned();
    let Some(deferred) = deferred else { return Err(fail) };
    *out = saved;
    env.scope.truncate(depth);
    env.emitted.push(Emitted { path: env.component_path.clone(), kind: SUSPENDED.to_owned(), value: deferred });
    return render(env, fallback, library, slots, out);
  }
  if tag == BOUNDARY_TAG {
    let kind = attrs.iter().find_map(|entry| match entry {
      Entry::Field(name, kind) if name == "kind" => Some(kind),
      _ => None,
    });
    let kind = match kind.map(|kind| env.eval_sync(kind)).transpose()? {
      Some(Value::Str(kind)) => kind,
      _ => return Err(Fail::internal("a boundary without its kind")),
    };
    out.markup(&format!("<!--{kind}-->"));
    for child in children {
      render(env, child, library, slots, out)?;
    }
    out.markup(&format!("<!--/{kind}-->"));
    return Ok(());
  }
  let root = std::mem::take(&mut env.stamp);
  let fallen = env.fallthrough.take();
  if root.is_empty() && env.slotted.is_empty() && fallen.is_none() {
    return render_element(env, tag, attrs, children, library, slots, out);
  }
  let attrs = match fallen {
    Some(fallen) => fall_onto(attrs, fallen)?,
    None => attrs.to_vec(),
  };
  let attrs = stamped(&attrs, &root, &env.slotted);
  render_element(env, tag, &attrs, children, library, slots, out)
}

/// A baked element's opening tag with the stamps waiting for it: the root's before the element's own scope, the slotted ones last.
pub(super) fn write_baked_open(env: &mut Env, open: &str, out: &mut Out) -> Result<(), Fail> {
  let root = std::mem::take(&mut env.stamp);
  let fallen = env.fallthrough.take();
  if root.is_empty() && env.slotted.is_empty() && fallen.is_none() {
    out.markup(open);
    return Ok(());
  }
  let mut open = match fallen {
    Some(fallen) => fall_onto_baked(open, &fallen)?,
    None => open.to_owned(),
  };
  if !root.is_empty() {
    let at = open.rfind(" data-v-").unwrap_or(open.len() - 1);
    open.insert_str(at, &format!(" {}", root.join(" ")));
  }
  if !env.slotted.is_empty() {
    let at = open.len() - 1;
    open.insert_str(at, &format!(" {}", env.slotted.join(" ")));
  }
  out.markup(&open);
  Ok(())
}

/// A root element's attributes with what fell through onto it, as Vue's `mergeProps` merges them: `class` joined after the root's own, any other name taking the root's place or following its attributes, before its scope. Literal attributes printed in place are split one to a piece first.
pub(super) fn fall_onto(attrs: &[Entry], fallen: ValueMap) -> Result<Vec<Entry>, Fail> {
  use crate::ast::Expr;
  let mut pieces: Vec<Entry> = Vec::new();
  for entry in attrs {
    match entry {
      Entry::Baked(text) => {
        for (name, value) in attr_text(text)? {
          pieces.push(Entry::Baked(match value {
            Some(value) => format!(" {name}=\"{value}\""),
            None => format!(" {name}"),
          }));
        }
      }
      other => pieces.push(other.clone()),
    }
  }
  let named = |entry: &Entry, want: &str| match entry {
    Entry::Field(own, _) => own == want,
    Entry::Baked(text) => text[1..].split('=').next() == Some(want),
    _ => false,
  };
  for (name, value) in fallen {
    match pieces.iter().position(|entry| named(entry, &name)) {
      Some(at) if name == "class" => match &mut pieces[at] {
        Entry::Field(_, own) => {
          let held = std::mem::replace(own, Expr::Lit(crate::ast::Lit::Null));
          *own = Expr::Array(vec![Entry::Item(held), Entry::Item(Expr::Folded(value))]);
        }
        Entry::Baked(text) => {
          let mut added = String::new();
          escape_attr(&vue::class_text(&value)?, &mut added);
          if !added.is_empty() {
            let own = text.trim_end_matches('"').split_once("=\"").map_or(String::new(), |(_, own)| own.to_owned());
            *text = if own.is_empty() { format!(" class=\"{added}\"") } else { format!(" class=\"{own} {added}\"") };
          }
        }
        _ => {}
      },
      Some(at) => pieces[at] = Entry::Field(name.to_string(), Expr::Folded(value)),
      None => {
        let at = pieces.iter().position(|entry| match entry {
          Entry::Field(own, _) => own.starts_with("data-v-"),
          Entry::Baked(text) => text.starts_with(" data-v-"),
          _ => false,
        });
        pieces.insert(at.unwrap_or(pieces.len()), Entry::Field(name.to_string(), Expr::Folded(value)));
      }
    }
  }
  Ok(pieces)
}

/// Attributes this renderer printed, ` name` or ` name="text"` with the text escaped, one after another.
pub(super) fn attr_text(text: &str) -> Result<Vec<(String, Option<String>)>, Fail> {
  let mut rest = text.strip_prefix(' ').unwrap_or(text);
  let mut attrs = Vec::new();
  while !rest.is_empty() {
    let end = rest.find([' ', '=']).unwrap_or(rest.len());
    let name = rest[..end].to_owned();
    rest = &rest[end..];
    let value = match rest.strip_prefix("=\"") {
      Some(quoted) => {
        let close = quoted.find('"').ok_or_else(|| Fail::internal("a printed attribute without its closing quote"))?;
        rest = &quoted[close + 1..];
        Some(quoted[..close].to_owned())
      }
      None => None,
    };
    rest = rest.strip_prefix(' ').unwrap_or(rest);
    attrs.push((name, value));
  }
  Ok(attrs)
}

/// A baked opening tag with what fell through onto it, merged as [`fall_onto`] merges an element's attributes. The tag is one this renderer wrote: a name, then ` name` or ` name="text"` with the text escaped, then `>`.
pub(super) fn fall_onto_baked(open: &str, fallen: &ValueMap) -> Result<String, Fail> {
  let inner = open.strip_prefix('<').and_then(|rest| rest.strip_suffix('>')).ok_or_else(|| Fail::internal("a baked tag that is not an opening tag"))?;
  let (tag, rest) = inner.split_once(' ').unwrap_or((inner, ""));
  let mut attrs = attr_text(rest)?;
  for (name, value) in fallen {
    let mut written = String::new();
    match name.as_str() {
      "class" => {
        let mut text = String::new();
        escape_attr(&vue::class_text(value)?, &mut text);
        if let Some((_, Some(own))) = attrs.iter_mut().find(|(own, _)| own == "class") {
          if !text.is_empty() {
            *own = if own.is_empty() { text } else { format!("{own} {text}") };
          }
          continue;
        }
        vue::attribute(name, value, &mut written)?;
      }
      _ => vue::attribute(name, value, &mut written)?,
    }
    let parsed = written.strip_prefix(' ').map(|piece| match piece.split_once("=\"") {
      Some((name, value)) => (name.to_owned(), Some(value.trim_end_matches('"').to_owned())),
      None => (piece.to_owned(), None),
    });
    match (attrs.iter().position(|(own, _)| own == name), parsed) {
      (Some(at), Some(piece)) => attrs[at] = piece,
      (Some(at), None) => {
        attrs.remove(at);
      }
      (None, Some(piece)) => {
        let at = attrs.iter().position(|(own, _)| own.starts_with("data-v-")).unwrap_or(attrs.len());
        attrs.insert(at, piece);
      }
      (None, None) => {}
    }
  }
  let mut out = format!("<{tag}");
  for (name, value) in attrs {
    match value {
      Some(value) => out.push_str(&format!(" {name}=\"{value}\"")),
      None => out.push_str(&format!(" {name}")),
    }
  }
  out.push('>');
  Ok(out)
}

/// `attrs` with `root` inserted before the element's own scope, where Vue writes a root's inherited stamps. `slotted` goes after everything.
pub(super) fn stamped(attrs: &[Entry], root: &[String], slotted: &[String]) -> Vec<Entry> {
  let present = |name: &str| Entry::Field(name.to_owned(), crate::ast::Expr::Lit(crate::ast::Lit::Bool(true)));
  let mut attrs = attrs.to_vec();
  if !root.is_empty() {
    let at = attrs.iter().rposition(|a| matches!(a, Entry::Field(n, crate::ast::Expr::Lit(crate::ast::Lit::Bool(true)) | crate::ast::Expr::Folded(Value::Bool(true))) if n.starts_with("data-v-"))).unwrap_or(attrs.len());
    for (i, name) in root.iter().enumerate() {
      attrs.insert(at + i, present(name));
    }
  }
  attrs.extend(slotted.iter().map(|name| present(name)));
  attrs
}

/// What slot content renders under, from the scoped style of the component whose `<slot>` it fills: the stamps a component placed in it takes on its root after its parent's scope and the slotted stamp every element in it carries, both that style's slotted stamp when it uses `:slotted()`.
pub(super) fn content_stamps(env: &Env) -> (Vec<String>, Vec<String>) {
  match &env.scoped {
    Some(scope) if scope.slotted => (vec![scope.slotted_attr()], vec![scope.slotted_attr()]),
    _ => (Vec::new(), Vec::new()),
  }
}

pub(super) fn render_island<'a>(env: &mut Env, tmpl: &'a Tmpl, library: &'a Components, slots: &mut Vec<Slot<'a>>, out: &mut Out) -> Result<(), Fail> {
  let Tmpl::Island { module, props, children, when, mode, id, define } = tmpl else { unreachable!("render_island is handed an island") };
  render_placed(env, module, props, children, when, mode, *id, *define, library, slots, out)
}

/// `children` rendered on their own and recorded in `places`, with a `PLACE_MARK` where they sat.
pub(super) fn render_place<'a>(env: &mut Env, target: &str, children: &'a [Tmpl], library: &'a Components, slots: &mut Vec<Slot<'a>>, out: &mut Out) -> Result<(), Fail> {
  let mut inner = Out::default();
  for child in children {
    render(env, child, library, slots, &mut inner)?;
  }
  let index = out.places.len();
  out.places.push(RenderedPlace { target: target.to_owned(), body: Rendered { html: inner.html, islands: inner.islands, places: inner.places, hoisted: ValueMap::default(), whole: true } });
  out.markup(&format!("{PLACE_MARK}{index}\u{0}"));
  Ok(())
}

/// An island: `<Island>` in the source, or a component a framework hydrates
/// that composition renders, which only its framework can render again.
/// The browser renders an island inside an island of its own framework in that island's tree, so the providers above it reach it. Any other island it mounts in a root of its own, which no provider outside reaches. The server renders both the same way.
#[allow(clippy::too_many_arguments)]
pub(super) fn render_placed<'a>(env: &mut Env, module: &str, props: &[Entry], children: &'a [Tmpl], when: &Option<String>, mode: &Option<String>, id: u32, define: bool, library: &'a Components, slots: &mut Vec<Slot<'a>>, out: &mut Out) -> Result<(), Fail> {
  let owner = island_owner(module, library);
  let joins = owner.is_some() && owner == env.island;
  let provided = if joins { env.contexts.clone() } else { std::mem::take(&mut env.contexts) };
  let enclosing = std::mem::replace(&mut env.island, owner);
  let result = render_placed_in(env, module, props, children, when, mode, id, define, library, slots, out);
  env.island = enclosing;
  env.contexts = provided;
  result
}

/// The framework an island's module renders with: its lowered component's owner, else, for a module lowered framework-neutral or not lowered at all, its file's. None for a module no framework renders.
pub(super) fn island_owner(module: &str, library: &Components) -> Option<crate::ast::Owner> {
  use crate::ast::Owner;
  match library.get(module).map(|c| c.owner) {
    Some(owner) if owner != Owner::Fsr => Some(owner),
    _ => {
      let file = module.split_once('#').map_or(module, |(file, _)| file);
      if file.ends_with(".vue") {
        Some(Owner::Vue)
      } else if file.ends_with(".svelte") {
        Some(Owner::Svelte)
      } else if file.ends_with(".tsx") || file.ends_with(".jsx") {
        Some(Owner::React)
      } else {
        None
      }
    }
  }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render_placed_in<'a>(env: &mut Env, module: &str, props: &[Entry], children: &'a [Tmpl], when: &Option<String>, mode: &Option<String>, id: u32, define: bool, library: &'a Components, slots: &mut Vec<Slot<'a>>, out: &mut Out) -> Result<(), Fail> {
  let key = env.hoists.as_ref().map(|h| h.island_key(id)).unwrap_or_default();
  let mut map = self::props(env, props)?;
  let functions = match map.shift_remove(FUNCTIONS_PROP) {
    Some(Value::Map(functions)) => functions,
    _ => ValueMap::default(),
  };
  let mut wire = map.clone();
  for (name, id) in functions.iter() {
    if let (true, Value::Str(id)) = (wire.contains_key(name.as_str()), id) {
      wire.insert(name.clone(), Value::Ref { kind: snapfire_fsr_core::RefKind::Module, id: id.as_str().to_owned() });
    }
  }
  if let Some((name, _)) = wire.iter().find(|(_, value)| crate::interp::holds_closure(value)) {
    return Err(Fail::internal(format!("a function passed as `{name}` to `{module}`, which renders in the browser; the server holds no function the browser can call")));
  }
  let mut marks = Vec::new();
  if wire.values().any(holds_made) {
    wire = match marked(&Value::Map(wire), &mut marks) {
      Value::Map(wire) => wire,
      _ => unreachable!("a map marks to a map"),
    };
  }
  if define {
    let mut inner = Out::default();
    for child in children {
      render(env, child, library, slots, &mut inner)?;
    }
    let index = out.islands.len();
    let body = Rendered { html: inner.html, islands: inner.islands, places: inner.places, hoisted: ValueMap::default(), whole: true };
    out.islands.push(RenderedIsland { module: module.to_owned(), props: ValueMap::default(), when: when.clone(), mode: None, state: ValueMap::default(), reads: Vec::new(), rendered_store: ValueMap::default(), awaits: Vec::new(), key, body, rendered: true });
    out.markup(&format!("{ISLAND_MARK}{index}\0"));
    return Ok(());
  }
  let keys = env.hoists.as_ref().map(|h| (h.module.clone(), h.path.clone()));
  // A component the server has no body for, a `.vue` file among them, is
  // placed with its props and its children and nothing else: the browser
  // mounts it rather than hydrating it and the page around it is whole
  // either way.
  let Some(component) = library.get(module) else {
    let mut inner = Out::default();
    if !children.is_empty() {
      render_children(env, children, keys.as_ref(), library, slots, &mut inner, CHILDREN_OPEN, "sf-s")?;
    }
    write_marked(env, &marks, &[], library, &mut inner)?;
    let index = out.islands.len();
    let awaits: Vec<String> = env.island_reads.get(module).into_iter().flatten().filter(|key| env.pending.contains(*key)).cloned().collect();
    out.islands.push(RenderedIsland { module: module.to_owned(), props: wire, when: when.clone(), mode: mode.clone(), state: ValueMap::default(), reads: Vec::new(), rendered_store: ValueMap::default(), awaits, key, body: Rendered { html: inner.html, islands: inner.islands, places: inner.places, hoisted: ValueMap::default(), whole: true }, rendered: false });
    out.markup(&format!("{ISLAND_MARK}{index}\u{0}"));
    return Ok(());
  };
  let awaits: Vec<String> = match env.pending.is_empty() {
    true => Vec::new(),
    false => cached_store_keys(env, component, library).iter().filter(|key| env.pending.contains(*key)).cloned().collect(),
  };
  if !awaits.is_empty() {
    let mut held = Out::default();
    if !children.is_empty() {
      render_children(env, children, keys.as_ref(), library, slots, &mut held, CHILDREN_OPEN, "sf-s")?;
    }
    write_marked(env, &marks, &[], library, &mut held)?;
    let reads = if mode.as_deref() == Some(SERVER_MODE) { with_sources((*cached_store_keys(env, component, library)).clone(), &env.derives) } else { Vec::new() };
    let index = out.islands.len();
    let hoisted = held_chunks(env, module, component, &map, library, slots);
    let body = Rendered { html: held.html, islands: held.islands, places: held.places, hoisted, whole: true };
    out.islands.push(RenderedIsland { module: module.to_owned(), props: wire, when: when.clone(), mode: mode.clone(), state: ValueMap::default(), reads, rendered_store: ValueMap::default(), awaits, key, body, rendered: false });
    out.markup(&format!("{ISLAND_MARK}{index}\u{0}"));
    return Ok(());
  }
  let depth = env.scope.len();
  let given = given_slots(env, children)?;
  let outer = Rc::new(std::mem::replace(&mut env.scope, component_scope(Value::Map(map.clone()), given)));
  slots.push(Slot { children, scope: Rc::clone(&outer), keys: keys.clone(), island: true, placed: false, framework: false, owner_path: String::new(), scoped: None });
  let mut inner = Out::default();
  let outer_hoists = env.hoists.replace(Hoists::new(module.to_owned()));
  let outer_mode = std::mem::replace(&mut env.server_mode, mode.as_deref() == Some(SERVER_MODE));
  let outer_state = env.state.take();
  let outer_framework = std::mem::replace(&mut env.in_framework, true);
  let outer_unrendered = std::mem::replace(&mut env.unrendered, false);
  let outer_marks = std::mem::replace(&mut env.marks, marks.clone());
  let outer_placed = std::mem::take(&mut env.placed);
  let result = call(env, module, |env| render_component(env, component, library, slots, &mut inner));
  env.marks = outer_marks;
  let placed = std::mem::replace(&mut env.placed, outer_placed);
  env.in_framework = outer_framework;
  let unrendered = std::mem::replace(&mut env.unrendered, outer_unrendered);
  let held = result.is_ok() && !children.is_empty() && component.owner.hydrates() && slots.last().is_some_and(|slot| slot.island && !slot.placed);
  let result = match held {
    true => {
      let island_scope = std::mem::replace(&mut env.scope, (*outer).clone());
      let rendered = render_children(env, children, keys.as_ref(), library, slots, &mut inner, CHILDREN_HELD_OPEN, "template");
      env.scope = island_scope;
      rendered
    }
    false => result,
  };
  env.state = outer_state;
  env.server_mode = outer_mode;
  let hoisted = std::mem::replace(&mut env.hoists, outer_hoists).map(|h| h.table).unwrap_or_default();
  slots.pop();
  env.scope = Rc::try_unwrap(outer).unwrap_or_else(|held| (*held).clone());
  env.scope.truncate(depth);
  result?;
  let index = out.islands.len();
  let read = cached_store_keys(env, component, library);
  let rendered_store = rendered_store(&env.store, &read);
  let reads = if mode.as_deref() == Some(SERVER_MODE) { with_sources((*read).clone(), &env.derives) } else { Vec::new() };
  if unrendered {
    let mut held = Out::default();
    if !children.is_empty() {
      render_children(env, children, keys.as_ref(), library, slots, &mut held, CHILDREN_OPEN, "sf-s")?;
    }
    write_marked(env, &marks, &[], library, &mut held)?;
    let body = Rendered { html: held.html, islands: held.islands, places: held.places, hoisted, whole: true };
    out.islands.push(RenderedIsland { module: module.to_owned(), props: wire, when: when.clone(), mode: mode.clone(), state: ValueMap::default(), reads, rendered_store, awaits: Vec::new(), key, body, rendered: false });
  } else {
    write_marked(env, &marks, &placed, library, &mut inner)?;
    out.islands.push(RenderedIsland { module: module.to_owned(), props: wire, when: when.clone(), mode: mode.clone(), state: inner.state, reads, rendered_store, awaits: Vec::new(), key, body: Rendered { html: inner.html, islands: inner.islands, places: inner.places, hoisted, whole: true }, rendered: true });
  }
  out.markup(&format!("{ISLAND_MARK}{index}\u{0}"));
  Ok(())
}

/// `keys` with the sources of each derived key among them, followed through
/// derived sources, sorted: what a server-mode island needs from the
/// browser's store to compute a derived key it reads after a step's writes.
pub(super) fn with_sources(keys: Vec<String>, derives: &[crate::interp::Derive]) -> Vec<String> {
  let mut out: std::collections::BTreeSet<String> = keys.into_iter().collect();
  loop {
    let more: Vec<String> = derives.iter().filter(|d| out.contains(&d.key)).flat_map(|d| d.sources.iter().cloned()).filter(|s| !out.contains(s)).collect();
    if more.is_empty() {
      break;
    }
    out.extend(more);
  }
  out.into_iter().collect()
}

/// The derived keys of `env`'s store computed again from it, sources first:
/// what a step's writes change before it renders. One without a body, which
/// the build could not lower, keeps what the browser sent. One the step wrote
/// keeps the write, which the browser reads ahead of a derived value.
pub(super) fn recompute_derived(env: &mut Env, written: &ValueMap) {
  let derives = env.derives.clone();
  for derived in derives.iter() {
    let Some(body) = derived.body.as_ref().filter(|_| !written.contains_key(&derived.key)) else { continue };
    match env.eval_sync(body) {
      Ok(value) => env.set_store(&derived.key, Some(value)),
      Err(_) => env.set_store(&derived.key, None),
    }
  }
}

/// The store keys `component` reads, in its body and its tree and in every
/// component it renders inline, sorted: what a server-mode island needs from
/// the browser's store on each step and what a hydrating one is told it was
/// rendered from.
/// [`store_keys`] of a prepared component, walked once per interpreter and held by the component's address, since the keys depend on the component and the library alone and a render places the same components every time.
pub(super) fn cached_store_keys(env: &Env, component: &Arc<Component>, library: &Components) -> Arc<Vec<String>> {
  let key = Arc::as_ptr(component) as usize;
  if let Some((_, keys)) = env.store_keys.lock().get(&key) {
    return keys.clone();
  }
  let keys = Arc::new(store_keys(component, library));
  env.store_keys.lock().insert(key, (component.clone(), keys.clone()));
  keys
}
