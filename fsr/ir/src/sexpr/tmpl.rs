//! Render trees, both directions.

use crate::ast::Tmpl;

use super::atoms::*;
use super::expr::{entry_from_sx, entry_to_sx, expr_from_sx, expr_to_sx};
use super::syntax::Sx;
use super::{read_all, Res, SexprError};

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
    Tmpl::Component { module, props, children, id, keyed } => {
      let mut rest = vec![
        Sx::Sym(module.clone()),
        Sx::Sym(id.to_string()),
        Sx::list(props.iter().map(entry_to_sx).collect()),
      ];
      rest.extend(children.iter().map(tmpl_to_sx));
      form(if *keyed { "keyed-comp" } else { "comp" }, rest)
    }
    Tmpl::Island { module, props, children, when, mode, id, define } => {
      if *define {
        let mut rest = vec![Sx::Sym(module.clone()), Sx::Sym(id.to_string()), opt_sx(when)];
        rest.extend(children.iter().map(tmpl_to_sx));
        return form("define-island", rest);
      }
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
    Tmpl::Marker(markup) => form("marker", vec![Sx::Str(markup.clone())]),
    Tmpl::Outlet { name, hydrated } => form(if *hydrated { "outlet-hydrated" } else { "outlet" }, vec![Sx::Str(name.clone())]),
    Tmpl::Place { target, children } => form("place", std::iter::once(Sx::Str(target.clone())).chain(children.iter().map(tmpl_to_sx)).collect()),
    Tmpl::Html(expr) => form("html", vec![expr_to_sx(expr)]),
    Tmpl::Baked { open, tag, children } => {
      let mut rest = vec![Sx::Str(open.clone()), opt_sx(tag)];
      rest.extend(children.iter().map(tmpl_to_sx));
      form("baked", rest)
    }
  }
}

pub fn tmpl_from_sx(sx: Sx) -> Res<Tmpl> {
  let items = match sx {
    Sx::Str(s) => return Ok(Tmpl::Text(s)),
    Sx::Interp(inner) => return Ok(Tmpl::Expr(expr_from_sx(&inner)?)),
    Sx::Sym(s) => {
      return Err(SexprError::shape(format!("`{s}` is not a template; text is quoted")))
    }
    Sx::List(items) => items,
  };
  let (head, mut rest) = into_form(items, "a template form starts with a symbol")?;
  let head = head.as_str();
  Ok(match head {
    "el" => {
      no_fewer(&rest, head, 2)?;
      Tmpl::Element {
        tag: into_sym(next(&mut rest))?,
        attrs: read_all(next(&mut rest).as_list()?, entry_from_sx)?,
        children: read_each(rest, tmpl_from_sx)?,
      }
    }
    "<>" => Tmpl::Fragment(read_each(rest, tmpl_from_sx)?),
    "if" => {
      no_fewer(&rest, head, 2)?;
      if rest.len() > 3 {
        return Err(SexprError::shape("a template `if` takes a condition, a branch and at most one else"));
      }
      Tmpl::If {
        cond: expr_from_sx(&next(&mut rest))?,
        then: Box::new(tmpl_from_sx(next(&mut rest))?),
        r#else: rest.next().map(tmpl_from_sx).transpose()?.map(Box::new),
      }
    }
    "for" => {
      exactly(&rest, head, 3)?;
      Tmpl::For {
        params: read_all(next(&mut rest).as_list()?, sym_of)?,
        over: expr_from_sx(&next(&mut rest))?,
        body: Box::new(tmpl_from_sx(next(&mut rest))?),
      }
    }
    "let" => {
      exactly(&rest, head, 3)?;
      Tmpl::Let {
        name: into_sym(next(&mut rest))?,
        expr: expr_from_sx(&next(&mut rest))?,
        then: Box::new(tmpl_from_sx(next(&mut rest))?),
      }
    }
    "comp" | "keyed-comp" => {
      no_fewer(&rest, head, 3)?;
      Tmpl::Component {
        module: into_sym(next(&mut rest))?,
        id: u32_of(&next(&mut rest))?,
        props: read_all(next(&mut rest).as_list()?, entry_from_sx)?,
        children: read_each(rest, tmpl_from_sx)?,
        keyed: head == "keyed-comp",
      }
    }
    "island" => {
      no_fewer(&rest, head, 5)?;
      Tmpl::Island {
        module: into_sym(next(&mut rest))?,
        id: u32_of(&next(&mut rest))?,
        when: into_opt(next(&mut rest))?,
        mode: into_opt(next(&mut rest))?,
        props: read_all(next(&mut rest).as_list()?, entry_from_sx)?,
        children: read_each(rest, tmpl_from_sx)?,
        define: false,
      }
    }
    "define-island" => {
      no_fewer(&rest, head, 3)?;
      Tmpl::Island {
        module: into_sym(next(&mut rest))?,
        id: u32_of(&next(&mut rest))?,
        when: into_opt(next(&mut rest))?,
        mode: None,
        props: Vec::new(),
        children: read_each(rest, tmpl_from_sx)?,
        define: true,
      }
    }
    "slot" => {
      exactly(&rest, head, 1)?;
      Tmpl::Slot(into_sym(next(&mut rest))?)
    }
    "marker" => {
      exactly(&rest, head, 1)?;
      Tmpl::Marker(into_str(next(&mut rest))?)
    }
    "outlet" | "outlet-hydrated" => {
      exactly(&rest, head, 1)?;
      let name = into_str(next(&mut rest))?;
      if !snapfire_fsr_core::is_place_name(&name) {
        return Err(SexprError::shape(format!("`{name}` is not a place name")));
      }
      Tmpl::Outlet { name, hydrated: head == "outlet-hydrated" }
    }
    "place" => {
      no_fewer(&rest, head, 1)?;
      let target = into_str(next(&mut rest))?;
      if snapfire_fsr_core::Place::parse(&target).is_none() {
        return Err(SexprError::shape(format!("`{target}` is not a place a `place` form can write to")));
      }
      Tmpl::Place { target, children: read_each(rest, tmpl_from_sx)? }
    }
    "html" => {
      exactly(&rest, head, 1)?;
      Tmpl::Html(expr_from_sx(&next(&mut rest))?)
    }
    "baked" => {
      no_fewer(&rest, head, 2)?;
      Tmpl::Baked {
        open: into_str(next(&mut rest))?,
        tag: into_opt(next(&mut rest))?,
        children: read_each(rest, tmpl_from_sx)?,
      }
    }
    other => return Err(SexprError::shape(format!("`{other}` is not a template form"))),
  })
}
