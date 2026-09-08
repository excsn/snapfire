//! The source, action and handler rows, each a head, its fixed terms and
//! whatever tagged sections it carries.

use snapfire_fsr_ir::sexpr::Sx;
use super::shape::Res;

use super::shape::*;
use crate::{ActionEntry, HandlerEntry, SourceEntry};

pub(super) fn source_to_sx(row: &SourceEntry) -> Sx {
  let mut rest = vec![sym(row.id.clone()), owner_sym(row.owner)];
  rest.extend(opt_form("module", &row.module));
  rest.extend(opt_form("export", &row.export));
  rest.extend(opt_form("reason", &row.reason));
  rest.extend(body_form("body", &row.body));
  rest.extend(body_form("meta", &row.meta));
  rest.extend(body_form("store", &row.store));
  form("source", rest)
}

pub(super) fn action_to_sx(row: &ActionEntry) -> Sx {
  let mut rest = vec![sym(row.id.clone()), owner_sym(row.owner)];
  rest.extend(opt_form("module", &row.module));
  rest.extend(opt_form("export", &row.export));
  rest.extend(opt_form("input", &row.input));
  rest.extend(opt_form("reason", &row.reason));
  rest.extend(body_form("body", &row.body));
  form("action", rest)
}

pub(super) fn handler_to_sx(row: &HandlerEntry) -> Sx {
  let mut rest = vec![
    sym(row.id.clone()),
    sym(row.method.clone()),
    sym(row.pattern.clone()),
    owner_sym(row.owner),
  ];
  rest.extend(opt_form("module", &row.module));
  rest.extend(opt_form("input", &row.input));
  rest.extend(opt_form("reason", &row.reason));
  rest.extend(body_form("body", &row.body));
  form("handler", rest)
}

/// The named sections of a row, each `(name ...)`, so a row reads whatever
/// order they were written in and an absent one stays absent.

pub(super) fn source_from_sx(items: &[Sx]) -> Res<SourceEntry> {
  if items.len() < 3 {
    return Err(err("a source row needs an id and an owner"));
  }
  let mut row = SourceEntry {
    id: as_sym(&items[1])?,
    owner: owner_of(&items[2])?,
    module: None,
    export: None,
    reason: None,
    body: None,
    meta: None,
    store: None,
  };
  for (head, values) in sections(items, 3)? {
    match head.as_str() {
      "module" => row.module = Some(one_of(&values, "module")?),
      "export" => row.export = Some(one_of(&values, "export")?),
      "reason" => row.reason = Some(one_of(&values, "reason")?),
      "body" => row.body = Some(body_of(&values)?),
      "meta" => row.meta = Some(body_of(&values)?),
      "store" => row.store = Some(body_of(&values)?),
      other => return Err(err(format!("`{other}` is not a source section"))),
    }
  }
  Ok(row)
}

pub(super) fn action_from_sx(items: &[Sx]) -> Res<ActionEntry> {
  if items.len() < 3 {
    return Err(err("an action row needs an id and an owner"));
  }
  let mut row = ActionEntry {
    id: as_sym(&items[1])?,
    owner: owner_of(&items[2])?,
    module: None,
    export: None,
    input: None,
    reason: None,
    body: None,
  };
  for (head, values) in sections(items, 3)? {
    match head.as_str() {
      "module" => row.module = Some(one_of(&values, "module")?),
      "export" => row.export = Some(one_of(&values, "export")?),
      "input" => row.input = Some(one_of(&values, "input")?),
      "reason" => row.reason = Some(one_of(&values, "reason")?),
      "body" => row.body = Some(body_of(&values)?),
      other => return Err(err(format!("`{other}` is not an action section"))),
    }
  }
  Ok(row)
}

pub(super) fn handler_from_sx(items: &[Sx]) -> Res<HandlerEntry> {
  if items.len() < 5 {
    return Err(err("a handler row needs an id, a method, a pattern and an owner"));
  }
  let mut row = HandlerEntry {
    id: as_sym(&items[1])?,
    method: as_sym(&items[2])?,
    pattern: as_sym(&items[3])?,
    owner: owner_of(&items[4])?,
    module: None,
    input: None,
    reason: None,
    body: None,
  };
  for (head, values) in sections(items, 5)? {
    match head.as_str() {
      "module" => row.module = Some(one_of(&values, "module")?),
      "input" => row.input = Some(one_of(&values, "input")?),
      "reason" => row.reason = Some(one_of(&values, "reason")?),
      "body" => row.body = Some(body_of(&values)?),
      other => return Err(err(format!("`{other}` is not a handler section"))),
    }
  }
  Ok(row)
}
