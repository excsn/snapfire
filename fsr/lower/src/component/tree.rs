use super::*;

/// Replaces each placed `file#Name` with the module it resolved to; a
/// placement whose name is an island alias becomes the island the alias
/// declared.
/// The ids of the placements that became islands, whose sites the rewrite keeps.
pub(super) fn island_ids(tmpl: &Tmpl, out: &mut Vec<u32>) {
  match tmpl {
    // `Baked` is what a loaded component becomes, so the lowerer never meets one.
    Tmpl::Baked { .. } => {}
    Tmpl::Island { id, children, .. } => {
      out.push(*id);
      children.iter().for_each(|c| island_ids(c, out));
    }
    Tmpl::Component { children, .. } | Tmpl::Element { children, .. } | Tmpl::Fragment(children) | Tmpl::Place { children, .. } => children.iter().for_each(|c| island_ids(c, out)),
    Tmpl::If { then, r#else, .. } => {
      island_ids(then, out);
      if let Some(other) = r#else {
        island_ids(other, out);
      }
    }
    Tmpl::For { body, .. } => island_ids(body, out),
    Tmpl::Let { then, .. } => island_ids(then, out),
    Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Outlet { .. } | Tmpl::Html(_) | Tmpl::Expr(_) | Tmpl::Slot(_) => {}
  }
}

/// Marks each placement in `placed` whose component `keys` as keyed and
/// collects its id. A placement with no site stays unkeyed, since the
/// rewrite could not wrap it to match. So does one inside a kept chunk: the
/// browser takes the chunk's markup whole and a chunk holds no island. A
/// placement whose component is `stateful` is keyed on the server's side
/// alone, which gives each instance inside a server island the address its
/// state and handlers ride under; nothing hoists below it, so the browser's
/// path needs no step there.
pub(super) fn mark_keyed(tmpl: &mut Tmpl, placed: &[u32], keys: &dyn Fn(&str) -> bool, stateful: &dyn Fn(&str) -> bool, out: &mut Vec<u32>) {
  match tmpl {
    Tmpl::Baked { .. } => {}
    Tmpl::Element { attrs, .. } if attrs.iter().any(|a| matches!(a, Entry::Field(name, _) if name == hoist::CHUNK_ATTR)) => {}
    Tmpl::Component { module, children, id, keyed, .. } => {
      if placed.contains(id) && keys(module) {
        *keyed = true;
        out.push(*id);
      } else if stateful(module) {
        *keyed = true;
      }
      children.iter_mut().for_each(|c| mark_keyed(c, placed, keys, stateful, out));
    }
    Tmpl::Island { children, .. } | Tmpl::Element { children, .. } | Tmpl::Fragment(children) | Tmpl::Place { children, .. } => children.iter_mut().for_each(|c| mark_keyed(c, placed, keys, stateful, out)),
    Tmpl::If { then, r#else, .. } => {
      mark_keyed(then, placed, keys, stateful, out);
      if let Some(other) = r#else {
        mark_keyed(other, placed, keys, stateful, out);
      }
    }
    Tmpl::For { body, .. } => mark_keyed(body, placed, keys, stateful, out),
    Tmpl::Let { then, .. } => mark_keyed(then, placed, keys, stateful, out),
    Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Outlet { .. } | Tmpl::Html(_) | Tmpl::Expr(_) | Tmpl::Slot(_) => {}
  }
}

/// Every module `tmpl` places, inline or as an island.
pub(super) fn placed_modules(tmpl: &Tmpl, out: &mut Vec<String>) {
  match tmpl {
    Tmpl::Component { module, children, .. } | Tmpl::Island { module, children, .. } => {
      out.push(module.clone());
      children.iter().for_each(|c| placed_modules(c, out));
    }
    Tmpl::Element { children, .. } | Tmpl::Fragment(children) | Tmpl::Place { children, .. } | Tmpl::Baked { children, .. } => children.iter().for_each(|c| placed_modules(c, out)),
    Tmpl::If { then, r#else, .. } => {
      placed_modules(then, out);
      if let Some(e) = r#else {
        placed_modules(e, out);
      }
    }
    Tmpl::For { body, .. } => placed_modules(body, out),
    Tmpl::Let { then, .. } => placed_modules(then, out),
    Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Outlet { .. } | Tmpl::Html(_) | Tmpl::Expr(_) | Tmpl::Slot(_) => {}
  }
}

/// True when `tmpl` holds the region a `<Mount>` writes.
pub(super) fn holds_mount(tmpl: &Tmpl) -> bool {
  match tmpl {
    Tmpl::Element { attrs, children, .. } => attrs.iter().any(|a| matches!(a, Entry::Field(name, _) if name == placements::MOUNT_ATTR)) || children.iter().any(holds_mount),
    Tmpl::Fragment(children) | Tmpl::Place { children, .. } | Tmpl::Component { children, .. } | Tmpl::Island { children, .. } => children.iter().any(holds_mount),
    Tmpl::If { then, r#else, .. } => holds_mount(then) || r#else.as_ref().is_some_and(|e| holds_mount(e)),
    Tmpl::For { body, .. } => holds_mount(body),
    Tmpl::Let { then, .. } => holds_mount(then),
    Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Outlet { .. } | Tmpl::Html(_) | Tmpl::Expr(_) | Tmpl::Slot(_) | Tmpl::Baked { .. } => false,
  }
}

/// `tmpl` with each component of another framework it renders inline as the
/// empty region `Mount` writes, its module added to `mounted`; `Err` with the
/// module of one that is given children, which cannot cross into it.
pub(super) fn mount_foreign(tmpl: Tmpl, mounted: &mut Vec<String>) -> Result<Tmpl, String> {
  let walk = |children: Vec<Tmpl>, mounted: &mut Vec<String>| children.into_iter().map(|c| mount_foreign(c, mounted)).collect::<Result<Vec<_>, _>>();
  Ok(match tmpl {
    Tmpl::Component { module, children, .. } if is_foreign(&module) => {
      if !children.is_empty() {
        return Err(module);
      }
      if !mounted.contains(&module) {
        mounted.push(module);
      }
      Tmpl::Element { tag: "sf-s".to_owned(), attrs: vec![Entry::Field("data-sf-island".to_owned(), Expr::lit_str(""))], children: Vec::new() }
    }
    Tmpl::Component { module, props, children, id, keyed } => Tmpl::Component { module, props, children: walk(children, mounted)?, id, keyed },
    Tmpl::Element { tag, attrs, children } => Tmpl::Element { tag, attrs, children: walk(children, mounted)? },
    Tmpl::Fragment(children) => Tmpl::Fragment(walk(children, mounted)?),
    Tmpl::If { cond, then, r#else } => Tmpl::If { cond, then: Box::new(mount_foreign(*then, mounted)?), r#else: r#else.map(|e| mount_foreign(*e, mounted).map(Box::new)).transpose()? },
    Tmpl::For { over, params, body } => Tmpl::For { over, params, body: Box::new(mount_foreign(*body, mounted)?) },
    Tmpl::Let { name, expr, then } => Tmpl::Let { name, expr, then: Box::new(mount_foreign(*then, mounted)?) },
    other => other,
  })
}

/// The first foreign component rendered inline rather than as an island.
pub(super) fn inline_foreign(tmpl: &Tmpl) -> Option<String> {
  match tmpl {
    Tmpl::Component { module, children, .. } => {
      if is_foreign(module) {
        return Some(module.clone());
      }
      children.iter().find_map(inline_foreign)
    }
    Tmpl::Island { children, .. } | Tmpl::Element { children, .. } | Tmpl::Fragment(children) | Tmpl::Place { children, .. } => children.iter().find_map(inline_foreign),
    Tmpl::Baked { children, .. } => children.iter().find_map(inline_foreign),
    Tmpl::If { then, r#else, .. } => inline_foreign(then).or_else(|| r#else.as_ref().and_then(|e| inline_foreign(e))),
    Tmpl::For { body, .. } => inline_foreign(body),
    Tmpl::Let { then, .. } => inline_foreign(then),
    Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Outlet { .. } | Tmpl::Html(_) | Tmpl::Expr(_) | Tmpl::Slot(_) => None,
  }
}

/// The tag and position that placed `module`, for a message about it.
pub(super) fn refs_by_module(modules: &HashMap<String, String>, module: &str, refs: &[(String, (usize, usize))]) -> Option<(String, (usize, usize))> {
  let (placed, _) = modules.iter().find(|(_, m)| m.as_str() == module)?;
  let name = placed.split_once('#').map(|(_, n)| n).unwrap_or(placed);
  refs.iter().find(|(n, _)| n == name).cloned()
}

pub(super) fn rewrite_modules(tmpl: Tmpl, modules: &HashMap<String, String>, islands: &HashMap<String, IslandTiming>) -> Tmpl {
  let mut tmpl = rewrite_tree(tmpl, modules, islands);
  tmpl.visit_mut(&mut |e| {
    if let Expr::Markup(markup) = e {
      *markup = std::sync::Arc::new(rewrite_tree((**markup).clone(), modules, islands));
    }
  });
  tmpl
}

pub(super) fn rewrite_tree(tmpl: Tmpl, modules: &HashMap<String, String>, islands: &HashMap<String, IslandTiming>) -> Tmpl {
  let walk = |children: Vec<Tmpl>| children.into_iter().map(|c| rewrite_tree(c, modules, islands)).collect();
  match tmpl {
    Tmpl::Component { module, props, children, id, keyed } => match islands.get(&module) {
      Some((when, mode)) => Tmpl::Island { module: modules.get(&module).cloned().unwrap_or(module), props, children: walk(children), when: when.clone(), mode: mode.clone(), id, define: false },
      None => Tmpl::Component { module: modules.get(&module).cloned().unwrap_or(module), props, children: walk(children), id, keyed },
    },
    Tmpl::Island { module, props, children, when, mode, id, define } => Tmpl::Island { module: modules.get(&module).cloned().unwrap_or(module), props, children: walk(children), when, mode, id, define },
    Tmpl::Element { tag, attrs, children } => Tmpl::Element { tag, attrs, children: walk(children) },
    Tmpl::Fragment(children) => Tmpl::Fragment(walk(children)),
    Tmpl::If { cond, then, r#else } => Tmpl::If { cond, then: Box::new(rewrite_tree(*then, modules, islands)), r#else: r#else.map(|e| Box::new(rewrite_tree(*e, modules, islands))) },
    Tmpl::For { over, params, body } => Tmpl::For { over, params, body: Box::new(rewrite_tree(*body, modules, islands)) },
    Tmpl::Let { name, expr, then } => Tmpl::Let { name, expr, then: Box::new(rewrite_tree(*then, modules, islands)) },
    other => other,
  }
}
