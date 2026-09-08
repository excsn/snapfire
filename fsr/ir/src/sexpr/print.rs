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

pub(super) fn inline(form: &Sx) -> String {
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

pub(super) fn write_form(out: &mut String, form: &Sx, indent: usize) {
  let flat = inline(form);
  if indent + flat.len() <= WIDTH || matches!(form, Sx::Sym(_) | Sx::Str(_)) {
    out.push_str(&flat);
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
        if let Some(next) = items.get(1) {
          let flat = inline(next);
          if flat.len() <= 32 && !matches!(next, Sx::List(_)) {
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
    _ => out.push_str(&flat),
  }
}
