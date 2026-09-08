//! Statements and whole components, both directions.

use crate::ast::{Body, Component, Handler, Stmt, Tmpl};

use super::atoms::*;
use super::expr::{expr_from_sx, expr_to_sx};
use super::tmpl::{tmpl_from_sx, tmpl_to_sx};
use super::syntax::Sx;
use super::{Res, SexprError};

pub(super) fn body_to_sx(body: &Body) -> Vec<Sx> {
  body.iter().map(stmt_to_sx).collect()
}

pub fn stmt_to_sx(stmt: &Stmt) -> Sx {
  match stmt {
    Stmt::Let { name, expr } => form("let", vec![Sx::Sym(name.clone()), expr_to_sx(expr)]),
    Stmt::If { cond, then, r#else } => {
      let mut rest = vec![expr_to_sx(cond), Sx::list(body_to_sx(then))];
      if !r#else.is_empty() {
        rest.push(Sx::list(body_to_sx(r#else)));
      }
      form("if", rest)
    }
    Stmt::ForOf { name, over, body } => {
      let mut rest = vec![Sx::Sym(name.clone()), expr_to_sx(over)];
      rest.extend(body_to_sx(body));
      form("for-of", rest)
    }
    Stmt::Return(expr) => form("ret", vec![expr_to_sx(expr)]),
    Stmt::Guard { cond, kind, message } => form(
      "guard",
      vec![expr_to_sx(cond), Sx::Sym(kind.clone()), Sx::Str(message.clone())],
    ),
    Stmt::SessionSet { key, path, value } => form(
      "session-set",
      vec![
        Sx::Sym(key.clone()),
        Sx::list(path.iter().map(expr_to_sx).collect()),
        expr_to_sx(value),
      ],
    ),
    Stmt::SessionDelete { key, path } => form(
      "session-del",
      vec![Sx::Sym(key.clone()), Sx::list(path.iter().map(expr_to_sx).collect())],
    ),
    Stmt::Expr(expr) => form("do", vec![expr_to_sx(expr)]),
  }
}

/// A component as the sections it has, each tagged, so an absent section is an
/// absent form rather than a hole to count past. The plan puts the module id
/// between the head and these.
pub fn component_sections(component: &Component) -> Vec<Sx> {
  let mut rest = Vec::new();
  if !component.state.is_empty() {
    rest.push(form("state", component.state.iter().map(|s| Sx::Sym(s.clone())).collect()));
  }
  if !component.body.is_empty() {
    rest.push(form("body", body_to_sx(&component.body)));
  }
  rest.push(form("render", vec![tmpl_to_sx(&component.render)]));
  for handler in &component.handlers {
    let mut on = vec![Sx::Sym(handler.event.clone())];
    on.extend(body_to_sx(&handler.body));
    rest.push(form("on", on));
  }
  rest
}

pub fn component_to_sx(component: &Component) -> Sx {
  form("component", component_sections(component))
}

pub(super) fn body_from_sx(items: &[Sx]) -> Res<Body> {
  items.iter().map(stmt_from_sx).collect()
}

pub fn stmt_from_sx(sx: &Sx) -> Res<Stmt> {
  let items = sx.as_list()?;
  let head = sx.head().ok_or_else(|| SexprError::shape("a statement starts with a symbol"))?;
  Ok(match head {
    "let" => {
      let a = args(items, head, 2)?;
      Stmt::Let { name: sym_of(&a[0])?, expr: expr_from_sx(&a[1])? }
    }
    "if" => {
      let a = at_least(items, head, 2)?;
      if a.len() > 3 {
        return Err(SexprError::shape("a statement `if` takes a condition, a body and at most one else"));
      }
      Stmt::If {
        cond: expr_from_sx(&a[0])?,
        then: body_from_sx(a[1].as_list()?)?,
        r#else: match a.get(2) {
          Some(other) => body_from_sx(other.as_list()?)?,
          None => Vec::new(),
        },
      }
    }
    "for-of" => {
      let a = at_least(items, head, 2)?;
      Stmt::ForOf { name: sym_of(&a[0])?, over: expr_from_sx(&a[1])?, body: body_from_sx(&a[2..])? }
    }
    "ret" => Stmt::Return(expr_from_sx(&args(items, head, 1)?[0])?),
    "guard" => {
      let a = args(items, head, 3)?;
      Stmt::Guard { cond: expr_from_sx(&a[0])?, kind: sym_of(&a[1])?, message: str_of(&a[2])? }
    }
    "session-set" => {
      let a = args(items, head, 3)?;
      Stmt::SessionSet {
        key: sym_of(&a[0])?,
        path: a[1].as_list()?.iter().map(expr_from_sx).collect::<Res<_>>()?,
        value: expr_from_sx(&a[2])?,
      }
    }
    "session-del" => {
      let a = args(items, head, 2)?;
      Stmt::SessionDelete {
        key: sym_of(&a[0])?,
        path: a[1].as_list()?.iter().map(expr_from_sx).collect::<Res<_>>()?,
      }
    }
    "do" => Stmt::Expr(expr_from_sx(&args(items, head, 1)?[0])?),
    other => return Err(SexprError::shape(format!("`{other}` is not a statement form"))),
  })
}

pub fn component_from_sx(sx: &Sx) -> Res<Component> {
  let items = sx.as_list()?;
  if sx.head() != Some("component") {
    return Err(SexprError::shape("a component is `(component ...)`"));
  }
  component_from_sections(&items[1..])
}

pub fn component_from_sections(items: &[Sx]) -> Res<Component> {
  let mut out = Component { body: Vec::new(), render: Tmpl::Fragment(Vec::new()), state: Vec::new(), handlers: Vec::new() };
  let mut rendered = false;
  for section in items {
    let inner = section.as_list()?;
    match section.head() {
      Some("state") => out.state = inner[1..].iter().map(sym_of).collect::<Res<_>>()?,
      Some("body") => out.body = body_from_sx(&inner[1..])?,
      Some("render") => {
        out.render = tmpl_from_sx(&args(inner, "render", 1)?[0])?;
        rendered = true;
      }
      Some("on") => {
        let a = at_least(inner, "on", 1)?;
        out.handlers.push(Handler { event: sym_of(&a[0])?, body: body_from_sx(&a[1..])? });
      }
      Some(other) => return Err(SexprError::shape(format!("`{other}` is not a component section"))),
      None => return Err(SexprError::shape("a component section starts with a symbol")),
    }
  }
  if !rendered {
    return Err(SexprError::shape("a component needs a `(render ...)` section"));
  }
  Ok(out)
}
