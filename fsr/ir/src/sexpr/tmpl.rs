//! Render trees, both directions.

use crate::ast::Tmpl;

use super::atoms::*;
use super::expr::{entry_from_sx, entry_to_sx, expr_from_sx, expr_to_sx};
use super::syntax::Sx;
use super::{Res, SexprError};

pub(super) fn opt_sx(value: &Option<String>) -> Sx {
  match value {
    Some(s) => Sx::Str(s.clone()),
    None => Sx::sym("nil"),
  }
}

pub fn tmpl_to_sx(tmpl: &Tmpl) -> Sx {
  match tmpl {
    Tmpl::Text(text) => Sx::Str(text.clone()),
    Tmpl::Expr(expr) => Sx::Interp(Box::new(expr_to_sx(expr))),
    Tmpl::Element { tag, attrs, children } => {
      let mut rest = vec![Sx::Sym(tag.clone()), Sx::list(attrs.iter().map(entry_to_sx).collect())];
      rest.extend(children.iter().map(tmpl_to_sx));
      form("el", rest)
    }
    Tmpl::Fragment(children) => form("<>", children.iter().map(tmpl_to_sx).collect()),
    Tmpl::If { cond, then, r#else } => {
      let mut rest = vec![expr_to_sx(cond), tmpl_to_sx(then)];
      if let Some(other) = r#else {
        rest.push(tmpl_to_sx(other));
      }
      form("if", rest)
    }
    Tmpl::For { over, params, body } => form(
      "for",
      vec![
        Sx::list(params.iter().map(|p| Sx::Sym(p.clone())).collect()),
        expr_to_sx(over),
        tmpl_to_sx(body),
      ],
    ),
    Tmpl::Let { name, expr, then } => {
      form("let", vec![Sx::Sym(name.clone()), expr_to_sx(expr), tmpl_to_sx(then)])
    }
    Tmpl::Component { module, props, children, id } => {
      let mut rest = vec![
        Sx::Sym(module.clone()),
        Sx::Sym(id.to_string()),
        Sx::list(props.iter().map(entry_to_sx).collect()),
      ];
      rest.extend(children.iter().map(tmpl_to_sx));
      form("comp", rest)
    }
    Tmpl::Island { module, props, children, when, mode, id } => {
      let mut rest = vec![
        Sx::Sym(module.clone()),
        Sx::Sym(id.to_string()),
        opt_sx(when),
        opt_sx(mode),
        Sx::list(props.iter().map(entry_to_sx).collect()),
      ];
      rest.extend(children.iter().map(tmpl_to_sx));
      form("island", rest)
    }
    Tmpl::Slot(name) => form("slot", vec![Sx::Sym(name.clone())]),
    Tmpl::Baked { open, tag, children } => {
      let mut rest = vec![Sx::Str(open.clone()), opt_sx(tag)];
      rest.extend(children.iter().map(tmpl_to_sx));
      form("baked", rest)
    }
  }
}

pub fn tmpl_from_sx(sx: &Sx) -> Res<Tmpl> {
  let items = match sx {
    Sx::Str(s) => return Ok(Tmpl::Text(s.clone())),
    Sx::Interp(inner) => return Ok(Tmpl::Expr(expr_from_sx(inner)?)),
    Sx::Sym(s) => {
      return Err(SexprError::shape(format!("`{s}` is not a template; text is quoted")))
    }
    Sx::List(items) => items,
  };
  let head = sx.head().ok_or_else(|| SexprError::shape("a template form starts with a symbol"))?;
  Ok(match head {
    "el" => {
      let a = at_least(items, head, 2)?;
      Tmpl::Element {
        tag: sym_of(&a[0])?,
        attrs: a[1].as_list()?.iter().map(entry_from_sx).collect::<Res<_>>()?,
        children: a[2..].iter().map(tmpl_from_sx).collect::<Res<_>>()?,
      }
    }
    "<>" => Tmpl::Fragment(items[1..].iter().map(tmpl_from_sx).collect::<Res<_>>()?),
    "if" => {
      let a = at_least(items, head, 2)?;
      if a.len() > 3 {
        return Err(SexprError::shape("a template `if` takes a condition, a branch and at most one else"));
      }
      Tmpl::If {
        cond: expr_from_sx(&a[0])?,
        then: Box::new(tmpl_from_sx(&a[1])?),
        r#else: a.get(2).map(tmpl_from_sx).transpose()?.map(Box::new),
      }
    }
    "for" => {
      let a = args(items, head, 3)?;
      Tmpl::For {
        params: a[0].as_list()?.iter().map(sym_of).collect::<Res<_>>()?,
        over: expr_from_sx(&a[1])?,
        body: Box::new(tmpl_from_sx(&a[2])?),
      }
    }
    "let" => {
      let a = args(items, head, 3)?;
      Tmpl::Let {
        name: sym_of(&a[0])?,
        expr: expr_from_sx(&a[1])?,
        then: Box::new(tmpl_from_sx(&a[2])?),
      }
    }
    "comp" => {
      let a = at_least(items, head, 3)?;
      Tmpl::Component {
        module: sym_of(&a[0])?,
        id: u32_of(&a[1])?,
        props: a[2].as_list()?.iter().map(entry_from_sx).collect::<Res<_>>()?,
        children: a[3..].iter().map(tmpl_from_sx).collect::<Res<_>>()?,
      }
    }
    "island" => {
      let a = at_least(items, head, 5)?;
      Tmpl::Island {
        module: sym_of(&a[0])?,
        id: u32_of(&a[1])?,
        when: opt_of(&a[2])?,
        mode: opt_of(&a[3])?,
        props: a[4].as_list()?.iter().map(entry_from_sx).collect::<Res<_>>()?,
        children: a[5..].iter().map(tmpl_from_sx).collect::<Res<_>>()?,
      }
    }
    "slot" => Tmpl::Slot(sym_of(&args(items, head, 1)?[0])?),
    "baked" => {
      let a = at_least(items, head, 2)?;
      Tmpl::Baked {
        open: str_of(&a[0])?,
        tag: opt_of(&a[1])?,
        children: a[2..].iter().map(tmpl_from_sx).collect::<Res<_>>()?,
      }
    }
    other => return Err(SexprError::shape(format!("`{other}` is not a template form"))),
  })
}
