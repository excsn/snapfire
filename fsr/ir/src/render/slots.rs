use super::*;

pub(super) fn render_slot<'a>(env: &mut Env, name: &str, library: &'a Components, slots: &mut Vec<Slot<'a>>, out: &mut Out) -> Result<(), Fail> {
  let Some(slot) = slots.pop() else {
    out.html.push_str(&slot_mark(name));
    return Ok(());
  };
  render_slot_content(env, slot, library, slots, out)
}

/// A component's `{name}` where a placement may pass markup for the prop
/// `name`: the placement's `$slot:name` fill rendered as its children would be,
/// else the prop's value `read`.
pub(super) fn render_slot_read<'a>(env: &mut Env, name: &str, read: &crate::ast::Expr, library: &'a Components, slots: &mut Vec<Slot<'a>>, out: &mut Out) -> Result<(), Fail> {
  let fill = slots.last().filter(|frame| !frame.island).and_then(|frame| {
    frame.children.iter().find_map(|c| match c {
      Tmpl::Let { name: wrapper, then, .. } if wrapper.strip_prefix(SLOT_CONTENT_PREFIX) == Some(name) => Some(&**then),
      _ => None,
    })
  });
  let Some(fill) = fill else {
    let value = env.eval_sync(read)?;
    return render_value(env, &value, library, out);
  };
  let mut frame = slots.pop().expect("a fill was found in it");
  let children = std::mem::replace(&mut frame.children, std::slice::from_ref(fill));
  let result = render_slot_content(env, frame, library, slots, out);
  if let Some(frame) = slots.last_mut() {
    frame.children = children;
  }
  result
}

/// The caller's content for the slot `name` among a placement's children: the named `$slot:` wrapper's body, for `default` every child that is not a wrapper. `None` when the caller gave none.
pub(super) fn slot_content<'a>(children: &'a [Tmpl], name: &str) -> Option<(Option<&'a crate::ast::Expr>, Vec<&'a Tmpl>)> {
  if let Some(found) = children.iter().find_map(|c| match c {
    Tmpl::Let { name: wrapper, expr, then } if wrapper.strip_prefix(SLOT_CONTENT_PREFIX) == Some(name) => Some((Some(expr), &**then)),
    _ => None,
  }) {
    return Some((found.0, vec![found.1]));
  }
  if name != "default" {
    return None;
  }
  let plain: Vec<&Tmpl> = children.iter().filter(|c| !matches!(c, Tmpl::Let { name, .. } if name.starts_with(SLOT_CONTENT_PREFIX))).collect();
  (!plain.is_empty()).then_some((None, plain))
}

/// A `<slot>` that names itself, hands props or holds a fallback: the caller's content for it with its props bound under `SLOT_PROPS`, else the fallback. An island's children keep their region and fill its default slot (every slot under Vue's markup).
pub(super) fn render_vue_slot<'a>(env: &mut Env, name: &str, props: &crate::ast::Expr, fallback: &'a Tmpl, library: &'a Components, slots: &mut Vec<Slot<'a>>, out: &mut Out) -> Result<(), Fail> {
  let vue = env.markup.is_vue();
  if slots.last().is_some_and(|frame| frame.island) && (vue || name == "default") {
    return render_slot(env, "content", library, slots, out);
  }
  let given = env.eval_sync(props)?;
  let content = slots.last().filter(|frame| !frame.island).and_then(|frame| slot_content(frame.children, name));
  let content = match content {
    Some((Some(when), content)) => {
      let caller = slots.last().map(|frame| (*frame.scope).clone()).unwrap_or_default();
      let inner = std::mem::replace(&mut env.scope, caller);
      let holds = env.eval_sync(when).map(|v| truthy(&v));
      env.scope = inner;
      holds?.then_some(content)
    }
    Some((None, content)) => Some(content),
    None => None,
  };
  let result = match (content, slots.pop()) {
    (Some(content), Some(mut slot)) => {
      slot.placed = true;
      let mut scope = (*slot.scope).clone();
      scope.push((Name::new(SLOT_PROPS), given));
      let inner = std::mem::replace(&mut env.scope, scope);
      let outer_framework = std::mem::replace(&mut env.in_framework, slot.framework);
      let outer_path = std::mem::replace(&mut env.component_path, slot.owner_path.clone());
      let (stamps, slotted) = content_stamps(env);
      let outer_slotted = std::mem::replace(&mut env.slotted, slotted);
      let outer_stamps = std::mem::replace(&mut env.slot_stamps, Some(stamps));
      let outer_scoped = std::mem::replace(&mut env.scoped, slot.scoped.clone());
      let mut result = Ok(());
      for child in content {
        result = render(env, child, library, slots, out);
        if result.is_err() {
          break;
        }
      }
      env.scope = inner;
      env.in_framework = outer_framework;
      env.component_path = outer_path;
      env.slotted = outer_slotted;
      env.slot_stamps = outer_stamps;
      env.scoped = outer_scoped;
      slots.push(slot);
      result
    }
    (_, popped) => {
      if let Some(slot) = popped {
        slots.push(slot);
      }
      render(env, fallback, library, slots, out)
    }
  };
  result
}

pub(super) fn render_slot_content<'a>(env: &mut Env, mut slot: Slot<'a>, library: &'a Components, slots: &mut Vec<Slot<'a>>, out: &mut Out) -> Result<(), Fail> {
  slot.placed = true;
  let inner = std::mem::replace(&mut env.scope, (*slot.scope).clone());
  let outer_framework = std::mem::replace(&mut env.in_framework, slot.framework);
  let outer_path = std::mem::replace(&mut env.component_path, slot.owner_path.clone());
  let result = match slot.island {
    true => render_children(env, slot.children, slot.keys.as_ref(), library, slots, out, CHILDREN_OPEN, "sf-s"),
    false => {
      let callee_keys = match (&slot.keys, &mut env.hoists) {
        (Some((module, path)), Some(h)) => Some((std::mem::replace(&mut h.module, module.clone()), std::mem::replace(&mut h.path, path.clone()))),
        _ => None,
      };
      let (stamps, slotted) = content_stamps(env);
      let outer_slotted = std::mem::replace(&mut env.slotted, slotted);
      let outer_stamps = std::mem::replace(&mut env.slot_stamps, Some(stamps));
      let outer_scoped = std::mem::replace(&mut env.scoped, slot.scoped.clone());
      let mut result = Ok(());
      for child in slot.children {
        let child = match child {
          Tmpl::Let { name, then, .. } if name.strip_prefix(SLOT_CONTENT_PREFIX) == Some("default") => &**then,
          other => other,
        };
        result = render(env, child, library, slots, out);
        if result.is_err() {
          break;
        }
      }
      env.slotted = outer_slotted;
      env.slot_stamps = outer_stamps;
      env.scoped = outer_scoped;
      if let (Some((module, path)), Some(h)) = (callee_keys, &mut env.hoists) {
        h.module = module;
        h.path = path;
      }
      result
    }
  };
  env.scope = inner;
  env.in_framework = outer_framework;
  env.component_path = outer_path;
  slots.push(slot);
  result
}

/// An island's children, in a region opened by `open` and closed as `tag`:
/// `CHILDREN_OPEN` where the template places them, `CHILDREN_HELD_OPEN`
/// where it did not. The browser adopts the region's markup and never renders
/// the children. Nothing they hoist is kept and an island among them keys
/// under `keys`, the caller that wrote them.
#[allow(clippy::too_many_arguments)]
pub(super) fn render_children<'a>(env: &mut Env, children: &'a [Tmpl], keys: Option<&(String, Vec<Step>)>, library: &'a Components, slots: &mut Vec<Slot<'a>>, out: &mut Out, open: &str, tag: &str) -> Result<(), Fail> {
  let caller = keys.map(|(module, path)| {
    let mut hoists = Hoists::new(module.to_owned());
    hoists.path = path.clone();
    hoists
  });
  let held = std::mem::replace(&mut env.hoists, caller);
  let outer_framework = std::mem::replace(&mut env.in_framework, false);
  out.markup(open);
  let start = out.html.len();
  let mut result = Ok(());
  for child in children {
    result = render(env, child, library, slots, out);
    if result.is_err() {
      break;
    }
  }
  if env.island == Some(crate::ast::Owner::Svelte) {
    let content = out.html.split_off(start);
    let marked = svelte::tree(&content, &mut out.islands, library);
    out.markup(&marked);
  }
  out.close_tag(tag);
  env.in_framework = outer_framework;
  env.hoists = held;
  result
}
