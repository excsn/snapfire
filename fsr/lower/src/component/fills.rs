use std::collections::{HashMap, HashSet};

use snapfire_fsr_ir::ast::{Builtin, Component, Entry, Expr, Lit, LogicOp, Stmt};
use snapfire_fsr_ir::render::{ARRAY_ITEMS, CONTEXT_PREFIX, SLOT_CONTENT_PREFIX, SLOT_READ_PREFIX};
use snapfire_fsr_ir::Tmpl;

/// The tag of an element whose tag a local chose, `<Heading>` over `const { Heading } = table[key]`; its `$tag` attribute holds the choice until [`place_tags`] writes one element per tag it can take. One whose tags it cannot list stays, and the renderer reads the choice.
pub(super) use snapfire_fsr_ir::render::CHOSEN_TAG;

/// Each element whose tag a local chose as a branch over the tags it can take, listed from the literals the choice reads through the body's `let`s and the set's named constants; `Err` when the list is not complete or holds something other than a lowercase tag name.
pub(super) fn place_tags(component: &mut Component, consts: &snapfire_fsr_ir::ast::Consts) -> Result<(), &'static str> {
  let lets: HashMap<String, Expr> = component.body.iter().filter_map(|stmt| match stmt {
    Stmt::Let { name, expr } => Some((name.clone(), expr.clone())),
    _ => None,
  }).collect();
  let mut failed = None;
  choose_tags(&mut component.render, &lets, consts, &mut failed);
  match failed {
    Some(why) => Err(why),
    None => Ok(()),
  }
}

fn choose_tags(t: &mut Tmpl, lets: &HashMap<String, Expr>, consts: &snapfire_fsr_ir::ast::Consts, failed: &mut Option<&'static str>) {
  match t {
    Tmpl::Element { tag, attrs, children } if tag == CHOSEN_TAG => {
      children.iter_mut().for_each(|c| choose_tags(c, lets, consts, failed));
      let Some(Entry::Field(_, choice)) = attrs.first().cloned() else { return };
      let rest: Vec<Entry> = attrs[1..].to_vec();
      let mut tags = Vec::new();
      if !strings_of(&choice, lets, consts, &mut tags, 0) || tags.is_empty() {
        return;
      }
      if !tags.iter().all(|tag| tag.starts_with(|c: char| c.is_ascii_lowercase()) && tag.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')) {
        failed.get_or_insert("a tag chosen at render that is not a lowercase element name");
        return;
      }
      let children = std::mem::take(children);
      let element = |tag: &str| Tmpl::Element { tag: tag.to_owned(), attrs: rest.clone(), children: children.clone() };
      let (last, first) = tags.split_last().expect("not empty");
      let mut out = element(last);
      for tag in first.iter().rev() {
        out = Tmpl::If { cond: Expr::Compare(snapfire_fsr_ir::ast::CompareOp::Eq, Box::new(choice.clone()), Box::new(Expr::Lit(Lit::Str(tag.clone())))), then: Box::new(element(tag)), r#else: Some(Box::new(out)) };
      }
      *t = out;
    }
    Tmpl::Element { children, .. } | Tmpl::Fragment(children) | Tmpl::Place { children, .. } | Tmpl::Island { children, .. } | Tmpl::Baked { children, .. } | Tmpl::Component { children, .. } => children.iter_mut().for_each(|c| choose_tags(c, lets, consts, failed)),
    Tmpl::If { then, r#else, .. } => {
      choose_tags(then, lets, consts, failed);
      if let Some(e) = r#else {
        choose_tags(e, lets, consts, failed);
      }
    }
    Tmpl::For { body, .. } | Tmpl::Let { then: body, .. } => choose_tags(body, lets, consts, failed),
    Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Outlet { .. } | Tmpl::Html(_) | Tmpl::Expr(_) | Tmpl::Slot(_) => {}
  }
}

/// Every string `e` can be, into `out`; false when one of its values is not a literal the build can see.
fn strings_of(e: &Expr, lets: &HashMap<String, Expr>, consts: &snapfire_fsr_ir::ast::Consts, out: &mut Vec<String>, depth: usize) -> bool {
  if depth > 16 {
    return false;
  }
  match e {
    Expr::Lit(Lit::Str(s)) => {
      if !out.contains(s) {
        out.push(s.clone());
      }
      true
    }
    Expr::Var(n) => lets.get(n).is_some_and(|v| strings_of(v, lets, consts, out, depth + 1)),
    Expr::Const(key) => consts.get(key).is_some_and(|v| strings_of(v, lets, consts, out, depth + 1)),
    Expr::Ternary(_, a, b) => strings_of(a, lets, consts, out, depth + 1) && strings_of(b, lets, consts, out, depth + 1),
    Expr::Field(base, field) => {
      let mut objects = Vec::new();
      objects_of(base, lets, consts, &mut objects, depth + 1) && !objects.is_empty() && objects.iter().all(|entries| {
        entries.iter().find_map(|entry| match entry {
          Entry::Field(k, v) if k == field => Some(v),
          _ => None,
        }).is_some_and(|v| strings_of(v, lets, consts, out, depth + 1))
      })
    }
    _ => false,
  }
}

/// The object literals `e` can be: itself, what a name holds, an entry of one an index reads, either side of a branch.
fn objects_of(e: &Expr, lets: &HashMap<String, Expr>, consts: &snapfire_fsr_ir::ast::Consts, out: &mut Vec<Vec<Entry>>, depth: usize) -> bool {
  if depth > 16 {
    return false;
  }
  match e {
    Expr::Object(entries) if entries.iter().all(|entry| matches!(entry, Entry::Field(..))) => {
      out.push(entries.clone());
      true
    }
    Expr::Var(n) => lets.get(n).is_some_and(|v| objects_of(v, lets, consts, out, depth + 1)),
    Expr::Const(key) => consts.get(key).is_some_and(|v| objects_of(v, lets, consts, out, depth + 1)),
    Expr::Ternary(_, a, b) => objects_of(a, lets, consts, out, depth + 1) && objects_of(b, lets, consts, out, depth + 1),
    Expr::Index(base, _) => {
      let mut tables = Vec::new();
      objects_of(base, lets, consts, &mut tables, depth + 1) && tables.iter().flatten().all(|entry| match entry {
        Entry::Field(_, v) => objects_of(v, lets, consts, out, depth + 1),
        _ => false,
      })
    }
    _ => false,
  }
}

/// Places every `Expr::Markup` the component holds where the tree can take it; the rest stay values, rendered where they are interpolated. `Err` says why one cannot be either.
pub(super) fn place_markup(component: &mut Component) -> Result<(), &'static str> {
  if crate::heap_pass() {
    return Ok(());
  }
  let original = component.body.clone();
  let mut held: HashMap<String, Tmpl> = HashMap::new();
  let mut kept = Vec::new();
  for stmt in std::mem::take(&mut component.body) {
    match &stmt {
      Stmt::Let { name, expr } => match tree_of(expr, &held) {
        Some(t) => {
          held.insert(name.clone(), t);
        }
        None => kept.push(stmt),
      },
      _ => kept.push(stmt),
    }
  }
  component.body = kept;
  place(&mut component.render, &held);
  let mut place_inside = |e: &mut Expr| {
    if let Expr::Markup(t) = e {
      let t = std::sync::Arc::make_mut(t);
      place(t, &held);
      plain_reads(t);
    }
  };
  for stmt in &mut component.body {
    stmt_exprs_mut(stmt, &mut place_inside);
  }
  component.render.visit_mut(&mut place_inside);
  let mut restored: HashMap<String, Stmt> = HashMap::new();
  loop {
    let mut read = HashSet::new();
    let mut reads = |e: &Expr| {
      if let Expr::Var(n) = e {
        if held.contains_key(n) && !restored.contains_key(n) {
          read.insert(n.clone());
        }
      }
    };
    component.body.iter().chain(restored.values()).for_each(|stmt| stmt_exprs(stmt, &mut reads));
    component.render.visit(&mut reads);
    if read.is_empty() {
      break;
    }
    for stmt in &original {
      if let Stmt::Let { name, .. } = stmt {
        if read.contains(name) {
          let mut stmt = stmt.clone();
          stmt_exprs_mut(&mut stmt, &mut place_inside);
          restored.insert(name.clone(), stmt);
        }
      }
    }
  }
  if !restored.is_empty() {
    let mut kept = std::mem::take(&mut component.body).into_iter();
    for stmt in &original {
      match stmt {
        Stmt::Let { name, .. } if held.contains_key(name) => {
          if let Some(back) = restored.remove(name) {
            component.body.push(back);
          }
        }
        _ => component.body.extend(kept.next()),
      }
    }
    component.body.extend(kept);
  }
  let mut unready = false;
  let mut check = |e: &Expr| {
    if let Expr::Markup(t) = e {
      unready |= reads_slots(t);
    }
  };
  component.body.iter().for_each(|stmt| stmt_exprs(stmt, &mut check));
  component.render.visit(&mut check);
  match unready {
    true => Err("markup held as a value that places the component's children, which the build renders only in place"),
    false => Ok(()),
  }
}

fn stmt_exprs(stmt: &Stmt, f: &mut dyn FnMut(&Expr)) {
  if let Stmt::Let { expr, .. } | Stmt::Set { expr, .. } | Stmt::Return(expr) | Stmt::Expr(expr) = stmt {
    expr.visit(f);
  }
}

fn stmt_exprs_mut(stmt: &mut Stmt, f: &mut dyn FnMut(&mut Expr)) {
  if let Stmt::Let { expr, .. } | Stmt::Set { expr, .. } | Stmt::Return(expr) | Stmt::Expr(expr) = stmt {
    expr.visit_mut(f);
  }
}

/// `t` with each read of a prop a placement may fill made a plain read: a markup value reads the prop, which holds the markup a placement filled it with.
fn plain_reads(t: &mut Tmpl) {
  if let Tmpl::Let { name, expr, .. } = t {
    if name.starts_with(SLOT_READ_PREFIX) {
      *t = Tmpl::Expr(expr.clone());
      return;
    }
  }
  match t {
    Tmpl::Element { children, .. } | Tmpl::Fragment(children) | Tmpl::Place { children, .. } | Tmpl::Island { children, .. } | Tmpl::Baked { children, .. } | Tmpl::Component { children, .. } => children.iter_mut().for_each(plain_reads),
    Tmpl::If { then, r#else, .. } => {
      plain_reads(then);
      if let Some(e) = r#else {
        plain_reads(e);
      }
    }
    Tmpl::For { body, .. } | Tmpl::Let { then: body, .. } => plain_reads(body),
    Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Outlet { .. } | Tmpl::Html(_) | Tmpl::Expr(_) | Tmpl::Slot(_) => {}
  }
}

/// Whether `t` places a slot, which needs the frame of the placement around it.
fn reads_slots(t: &Tmpl) -> bool {
  match t {
    Tmpl::Slot(_) => true,
    Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Outlet { .. } | Tmpl::Html(_) | Tmpl::Expr(_) => false,
    Tmpl::Element { children, .. } | Tmpl::Fragment(children) | Tmpl::Place { children, .. } | Tmpl::Baked { children, .. } | Tmpl::Component { children, .. } | Tmpl::Island { children, .. } => children.iter().any(reads_slots),
    Tmpl::If { then, r#else, .. } => reads_slots(then) || r#else.as_deref().is_some_and(reads_slots),
    Tmpl::For { body, .. } => reads_slots(body),
    Tmpl::Let { then, .. } => reads_slots(then),
  }
}

/// The tree `expr` stands for when it is markup or chooses between markup.
fn tree_of(expr: &Expr, held: &HashMap<String, Tmpl>) -> Option<Tmpl> {
  match expr {
    Expr::Markup(t) => Some((**t).clone()),
    Expr::Var(n) => held.get(n).cloned(),
    Expr::Ternary(c, a, b) => {
      let (ta, tb) = (tree_of(a, held), tree_of(b, held));
      if ta.is_none() && tb.is_none() {
        return None;
      }
      Some(Tmpl::If { cond: (**c).clone(), then: Box::new(ta.unwrap_or_else(|| value(a))), r#else: Some(Box::new(tb.unwrap_or_else(|| value(b)))) })
    }
    Expr::Logic(LogicOp::And, c, m) => tree_of(m, held).map(|t| Tmpl::If { cond: (**c).clone(), then: Box::new(t), r#else: Some(Box::new(Tmpl::Expr((**c).clone()))) }),
    Expr::Array(entries) => {
      let trees: Vec<Option<Tmpl>> = entries.iter().map(|entry| match entry {
        Entry::Item(e) | Entry::Spread(e) => tree_of(e, held),
        _ => None,
      }).collect();
      if trees.iter().all(Option::is_none) {
        return None;
      }
      let mut items = Vec::new();
      for (entry, tree) in entries.iter().zip(trees) {
        match (entry, tree) {
          (_, Some(t)) => items.push(t),
          (Entry::Item(e) | Entry::Spread(e), None) => items.push(value(e)),
          _ => return None,
        }
      }
      Some(self::items(items))
    }
    Expr::Filter(over, keep) if truthiness(keep) => match tree_of(over, held)? {
      Tmpl::Let { name, then, .. } if name == ARRAY_ITEMS && matches!(*then, Tmpl::Fragment(_)) => {
        let Tmpl::Fragment(kept) = *then else { unreachable!() };
        Some(self::items(kept.into_iter().map(|item| match item {
          Tmpl::Expr(e) => Tmpl::If { cond: e.clone(), then: Box::new(Tmpl::Expr(e)), r#else: None },
          other => other,
        }).collect()))
      }
      other => Some(other),
    },
    Expr::Reduce(over, init, step) => fold_tree(over, init, step, None, held),
    Expr::Field(base, name) => match &**base {
      Expr::Reduce(over, init, step) => fold_tree(over, init, step, Some(name), held),
      _ => None,
    },
    Expr::Builtin { name: Builtin::Concat, args } => {
      let trees: Vec<Option<Tmpl>> = args.iter().map(|e| tree_of(e, held)).collect();
      if trees.iter().all(Option::is_none) {
        return None;
      }
      Some(self::items(args.iter().zip(trees).map(|(e, tree)| tree.unwrap_or_else(|| value(e))).collect()))
    }
    _ => None,
  }
}

/// A fold whose step only appends markup to its accumulator, `field` of it when the loop wrote several names: what it starts from, then a `Tmpl::For` over the items writing what each one appends.
fn fold_tree(over: &Expr, init: &Expr, step: &Expr, field: Option<&String>, held: &HashMap<String, Tmpl>) -> Option<Tmpl> {
  let Expr::Lambda { params, body } = step else { return None };
  let [acc, item] = params.as_slice() else { return None };
  let me = match field {
    None => Expr::Var(acc.clone()),
    Some(f) => Expr::Field(Box::new(Expr::Var(acc.clone())), f.clone()),
  };
  let field_of = |e: &Expr| -> Option<Expr> {
    match field {
      None => Some(e.clone()),
      Some(f) => match e {
        Expr::Object(entries) => entries.iter().find_map(|entry| match entry {
          Entry::Field(k, v) if k == f => Some(v.clone()),
          _ => None,
        }),
        _ => None,
      },
    }
  };
  let (init, body) = (field_of(init)?, field_of(body)?);
  let mut found = false;
  let each = appended(&body, &me, held, &mut found)?;
  let mut reads_acc = false;
  each.visit(&mut |e| {
    let mut free = Vec::new();
    e.free_vars(&mut free);
    reads_acc |= free.contains(acc);
  });
  if !found || reads_acc {
    return None;
  }
  let start = tree_of(&init, held).unwrap_or_else(|| value(&init));
  Some(self::items(vec![start, Tmpl::For { over: over.clone(), params: vec![item.clone()], body: Box::new(each) }]))
}

/// What one step of a fold appends to `me`: nothing when it is `me`, the items after a leading `...me`, a branch over either.
fn appended(e: &Expr, me: &Expr, held: &HashMap<String, Tmpl>, found: &mut bool) -> Option<Tmpl> {
  if e == me {
    return Some(Tmpl::Fragment(Vec::new()));
  }
  match e {
    Expr::Array(entries) if entries.first() == Some(&Entry::Spread(me.clone())) => {
      let mut items = Vec::new();
      for entry in &entries[1..] {
        match entry {
          Entry::Item(x) | Entry::Spread(x) => items.push(match tree_of(x, held) {
            Some(t) => {
              *found = true;
              t
            }
            None => value(x),
          }),
          _ => return None,
        }
      }
      Some(self::items(items))
    }
    Expr::Ternary(c, a, b) => Some(Tmpl::If { cond: (**c).clone(), then: Box::new(appended(a, me, held, found)?), r#else: Some(Box::new(appended(b, me, held, found)?)) }),
    _ => None,
  }
}

/// An array's items as markup: they render in order and `Children.count` counts each of them, where `<>…</>` counts as one.
pub(super) fn items(children: Vec<Tmpl>) -> Tmpl {
  Tmpl::Let { name: ARRAY_ITEMS.to_owned(), expr: Expr::Lit(Lit::Null), then: Box::new(Tmpl::Fragment(children)) }
}

/// `(x) => x` or `(x) => !!x`, which keeps an item when it is truthy.
fn truthiness(keep: &Expr) -> bool {
  let Expr::Lambda { params, body } = keep else { return false };
  let [param] = params.as_slice() else { return false };
  match &**body {
    Expr::Var(v) => v == param,
    Expr::Not(inner) => matches!(&**inner, Expr::Not(v) if matches!(&**v, Expr::Var(v) if v == param)),
    _ => false,
  }
}

fn value(e: &Expr) -> Tmpl {
  match e {
    Expr::Lit(Lit::Null) => Tmpl::Fragment(Vec::new()),
    other => Tmpl::Expr(other.clone()),
  }
}

/// A `let` the renderer reads by its name: a slot's fill, a slot read, a
/// Vue outlet and a context provider.
fn reserved(name: &str) -> bool {
  name.starts_with("$slot") || name.starts_with(CONTEXT_PREFIX)
}

fn place(t: &mut Tmpl, held: &HashMap<String, Tmpl>) {
  match t {
    Tmpl::Expr(e) => {
      if let Some(sub) = tree_of(e, held) {
        *t = sub;
        place(t, held);
      } else if let Expr::Field(base, name) = e {
        if matches!(&**base, Expr::Var(v) if v == "$props") {
          *t = Tmpl::Let { name: format!("{SLOT_READ_PREFIX}{name}"), expr: e.clone(), then: Box::new(Tmpl::Fragment(Vec::new())) };
        }
      }
    }
    Tmpl::Component { props, children, .. } => {
      let mut moved = Vec::new();
      for entry in props.iter_mut() {
        if let Entry::Field(k, e) = entry {
          if let Some(sub) = tree_of(e, held) {
            moved.push((k.clone(), sub.clone()));
            *e = Expr::Markup(std::sync::Arc::new(sub));
          }
        }
      }
      for child in children.iter_mut() {
        place(child, held);
      }
      for (k, mut sub) in moved {
        place(&mut sub, held);
        children.push(Tmpl::Let { name: format!("{SLOT_CONTENT_PREFIX}{k}"), expr: Expr::Lit(Lit::Bool(true)), then: Box::new(sub) });
      }
    }
    Tmpl::Let { name, expr, then } if !reserved(name) && tree_of(expr, held).is_some() => {
      let sub = tree_of(expr, held).expect("checked by the guard");
      let mut inner = held.clone();
      inner.insert(name.clone(), sub);
      let mut body = std::mem::replace(&mut **then, Tmpl::Fragment(Vec::new()));
      place(&mut body, &inner);
      *t = body;
    }
    Tmpl::Element { children, .. } | Tmpl::Fragment(children) | Tmpl::Place { children, .. } | Tmpl::Island { children, .. } | Tmpl::Baked { children, .. } => children.iter_mut().for_each(|c| place(c, held)),
    Tmpl::If { then, r#else, .. } => {
      place(then, held);
      if let Some(e) = r#else {
        place(e, held);
      }
    }
    Tmpl::For { params, body, .. } if params.iter().any(|p| held.contains_key(p)) => place(body, &unheld(held, params.iter())),
    Tmpl::Let { name, then, .. } if held.contains_key(name.as_str()) => place(then, &unheld(held, std::iter::once(&*name))),
    Tmpl::For { body, .. } | Tmpl::Let { then: body, .. } => place(body, held),
    Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Outlet { .. } | Tmpl::Html(_) | Tmpl::Slot(_) => {}
  }
}

/// `held` without the names a binding inside it shadows.
fn unheld<'n>(held: &HashMap<String, Tmpl>, names: impl Iterator<Item = &'n String>) -> HashMap<String, Tmpl> {
  let mut inner = held.clone();
  for name in names {
    inner.remove(name);
  }
  inner
}

/// Every (module, prop) a placement in `t` passes markup for.
pub(super) fn collect(t: &Tmpl, out: &mut HashSet<(String, String)>) {
  match t {
    Tmpl::Component { module, children, .. } => {
      for child in children {
        if let Tmpl::Let { name, .. } = child {
          if let Some(prop) = name.strip_prefix(SLOT_CONTENT_PREFIX) {
            out.insert((module.clone(), prop.to_owned()));
          }
        }
        collect(child, out);
      }
    }
    Tmpl::Element { children, .. } | Tmpl::Fragment(children) | Tmpl::Place { children, .. } | Tmpl::Island { children, .. } | Tmpl::Baked { children, .. } => children.iter().for_each(|c| collect(c, out)),
    Tmpl::If { then, r#else, .. } => {
      collect(then, out);
      if let Some(e) = r#else {
        collect(e, out);
      }
    }
    Tmpl::For { body, .. } | Tmpl::Let { then: body, .. } => collect(body, out),
    Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Outlet { .. } | Tmpl::Html(_) | Tmpl::Expr(_) | Tmpl::Slot(_) => {}
  }
}

/// `t` with each read of a prop of `module` that nothing fills made plain.
pub(super) fn settle(t: &mut Tmpl, module: &str, filled: &HashSet<(String, String)>) {
  if let Tmpl::Let { name, expr, .. } = t {
    if let Some(prop) = name.strip_prefix(SLOT_READ_PREFIX) {
      if !filled.contains(&(module.to_owned(), prop.to_owned())) {
        *t = Tmpl::Expr(expr.clone());
      }
      return;
    }
  }
  match t {
    Tmpl::Element { children, .. } | Tmpl::Fragment(children) | Tmpl::Place { children, .. } | Tmpl::Island { children, .. } | Tmpl::Baked { children, .. } | Tmpl::Component { children, .. } => children.iter_mut().for_each(|c| settle(c, module, filled)),
    Tmpl::If { then, r#else, .. } => {
      settle(then, module, filled);
      if let Some(e) = r#else {
        settle(e, module, filled);
      }
    }
    Tmpl::For { body, .. } | Tmpl::Let { then: body, .. } => settle(body, module, filled),
    Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Outlet { .. } | Tmpl::Html(_) | Tmpl::Expr(_) | Tmpl::Slot(_) => {}
  }
}

/// Whether `e` can evaluate to a function value: a lambda anywhere in it other than the callback a builtin or a call applies.
pub(super) fn holds_function(e: &Expr) -> bool {
  let mut callbacks: Vec<*const Expr> = Vec::new();
  e.visit(&mut |x| match x {
    Expr::Map(_, f) | Expr::Filter(_, f) | Expr::Find(_, f) | Expr::FindIndex(_, f) | Expr::Some(_, f) | Expr::Every(_, f) | Expr::Sort(_, f) | Expr::FlatMap(_, f) | Expr::Reduce(_, _, f) | Expr::ReplaceWith(_, _, f) | Expr::Apply { f, .. } | Expr::ApplyOf(f, _) => callbacks.push(&**f as *const Expr),
    _ => {}
  });
  let mut found = false;
  e.visit(&mut |x| found |= matches!(x, Expr::Lambda { .. } | Expr::Fix { .. }) && !callbacks.contains(&(x as *const Expr)));
  found
}

/// Gives every placement in `t` of a module `renders_in_browser` holds that passes a function value as a prop its `FUNCTIONS_PROP`, the module id the browser imports each such prop from, taken from `refs`; the indexes of the refs taken go to `used`. A function prop with no ref is the error, as is one whose ref says why the browser cannot reach it: the placement, the prop and the reason.
pub(super) fn reference_functions(t: &mut Tmpl, renders_in_browser: &dyn Fn(&str) -> bool, refs: &[super::FnRef], used: &mut Vec<usize>) -> Result<(), (String, String, Option<String>)> {
  let island = matches!(t, Tmpl::Island { .. });
  match t {
    Tmpl::Component { module, props, children, .. } | Tmpl::Island { module, props, children, .. } => {
      if island || renders_in_browser(module) {
        let mut ids: Vec<Entry> = Vec::new();
        for entry in props.iter() {
          let Entry::Field(name, e) = entry else { continue };
          match refs.iter().position(|r| r.module == *module && r.prop == *name && r.value == *e) {
            Some(index) => match &refs[index].id {
              Ok(id) => {
                ids.push(Entry::Field(name.clone(), id.clone()));
                used.push(index);
              }
              Err(reason) => return Err((module.clone(), name.clone(), Some(reason.clone()))),
            },
            None if holds_function(e) => return Err((module.clone(), name.clone(), None)),
            None => {}
          }
        }
        if !ids.is_empty() {
          for why in used.iter().filter_map(|i| refs[*i].unlowered.as_deref()) {
            props.retain(|e| !matches!(e, Entry::Field(n, Expr::Lit(snapfire_fsr_ir::ast::Lit::Str(s))) if n == snapfire_fsr_ir::render::UNLOWERED_ATTR && s == why));
          }
          props.push(Entry::Field(snapfire_fsr_ir::render::FUNCTIONS_PROP.to_owned(), Expr::Object(ids)));
        }
      }
      children.iter_mut().try_for_each(|c| reference_functions(c, renders_in_browser, refs, used))
    }
    Tmpl::Element { children, .. } | Tmpl::Fragment(children) | Tmpl::Place { children, .. } | Tmpl::Baked { children, .. } => children.iter_mut().try_for_each(|c| reference_functions(c, renders_in_browser, refs, used)),
    Tmpl::If { then, r#else, .. } => {
      reference_functions(then, renders_in_browser, refs, used)?;
      match r#else.as_deref_mut() {
        Some(e) => reference_functions(e, renders_in_browser, refs, used),
        None => Ok(()),
      }
    }
    Tmpl::For { body, .. } | Tmpl::Let { then: body, .. } => reference_functions(body, renders_in_browser, refs, used),
    Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Outlet { .. } | Tmpl::Html(_) | Tmpl::Expr(_) | Tmpl::Slot(_) => Ok(()),
  }
}

/// Each placement in `t` of a module `renders_in_browser` holds with the markup it fills a prop with given back as the prop, a markup value the island receives as a region of its own.
pub(super) fn unfill(t: &mut Tmpl, renders_in_browser: &dyn Fn(&str) -> bool) {
  match t {
    Tmpl::Component { module, props, children, .. } => {
      if renders_in_browser(module) {
        let mut kept = Vec::with_capacity(children.len());
        for child in std::mem::take(children) {
          match child {
            Tmpl::Let { name, then, .. } if name.starts_with(SLOT_CONTENT_PREFIX) && name != format!("{SLOT_CONTENT_PREFIX}default") => {
              let prop = &name[SLOT_CONTENT_PREFIX.len()..];
              if !props.iter().any(|entry| matches!(entry, Entry::Field(k, _) if k == prop)) {
                props.push(Entry::Field(prop.to_owned(), Expr::Markup(std::sync::Arc::new(*then))));
              }
            }
            other => kept.push(other),
          }
        }
        *children = kept;
      }
      children.iter_mut().for_each(|c| unfill(c, renders_in_browser));
    }
    Tmpl::Element { children, .. } | Tmpl::Fragment(children) | Tmpl::Place { children, .. } | Tmpl::Island { children, .. } | Tmpl::Baked { children, .. } => children.iter_mut().for_each(|c| unfill(c, renders_in_browser)),
    Tmpl::If { then, r#else, .. } => {
      unfill(then, renders_in_browser);
      if let Some(e) = r#else {
        unfill(e, renders_in_browser);
      }
    }
    Tmpl::For { body, .. } | Tmpl::Let { then: body, .. } => unfill(body, renders_in_browser),
    Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Outlet { .. } | Tmpl::Html(_) | Tmpl::Expr(_) | Tmpl::Slot(_) => {}
  }
}
