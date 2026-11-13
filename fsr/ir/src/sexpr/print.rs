//! Printing: one form per top-level line, wrapped at [`WIDTH`].

use super::syntax::{plain_symbol, write_escaped, Sx};

/// The column a list is allowed to reach before its items break onto their own
/// lines.
pub(super) const WIDTH: usize = 100;

/// Every form, one per top-level line, wrapped at [`WIDTH`].
pub fn print(forms: &[Sx]) -> String {
  let mut out = String::new();
  for form in forms {
    write_form(&mut out, form, 0);
    out.push('\n');
  }
  out
}

pub(crate) fn inline(form: &Sx) -> String {
  let mut out = String::new();
  write_inline(&mut out, form);
  out
}

pub(super) fn write_inline(out: &mut String, form: &Sx) {
  match form {
    Sx::Sym(s) => {
      if plain_symbol(s) {
        out.push_str(s);
      } else {
        write_escaped(out, s, '|');
      }
    }
    Sx::Str(s) => write_escaped(out, s, '"'),
    Sx::Interp(inner) => {
      out.push('{');
      write_inline(out, inner);
      out.push('}');
    }
    Sx::List(items) => {
      out.push('(');
      for (i, item) in items.iter().enumerate() {
        if i > 0 {
          out.push(' ');
        }
        write_inline(out, item);
      }
      out.push(')');
    }
  }
}

/// Whether `form` written on one line takes at most `budget` bytes. It stops
/// as soon as the line is too long, so checking a large form costs no more
/// than the budget.
fn fits(form: &Sx, budget: &mut usize) -> bool {
  let take = |n: usize, budget: &mut usize| match budget.checked_sub(n) {
    Some(left) => {
      *budget = left;
      true
    }
    None => false,
  };
  match form {
    Sx::Sym(_) | Sx::Str(_) => take(inline(form).len(), budget),
    Sx::Interp(inner) => take(2, budget) && fits(inner, budget),
    Sx::List(items) => take(2 + items.len().saturating_sub(1), budget) && items.iter().all(|item| fits(item, budget)),
  }
}

pub(super) fn write_form(out: &mut String, form: &Sx, indent: usize) {
  if matches!(form, Sx::Sym(_) | Sx::Str(_)) || fits(form, &mut WIDTH.saturating_sub(indent)) {
    write_inline(out, form);
    return;
  }
  match form {
    Sx::Interp(inner) => {
      out.push('{');
      write_form(out, inner, indent + 1);
      out.push('}');
    }
    Sx::List(items) => {
      out.push('(');
      // The head and, where it is short, the item after it stay on the opening
      // line: `(el div` reads as one thing and breaking it helps nobody.
      let mut first = 1;
      if let Some(head) = items.first() {
        write_inline(out, head);
        if let Some(next) = items.get(1).filter(|next| !matches!(next, Sx::List(_))) {
          let flat = inline(next);
          if flat.len() <= 32 {
            out.push(' ');
            out.push_str(&flat);
            first = 2;
          }
        }
      }
      let pad = indent + 2;
      for item in &items[first.min(items.len())..] {
        out.push('\n');
        for _ in 0..pad {
          out.push(' ');
        }
        write_form(out, item, pad);
      }
      out.push(')');
    }
    _ => write_inline(out, form),
  }
}
