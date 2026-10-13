const MATH_FUNCTIONS: &[&str] = &[
  "calc", "min", "max", "clamp", "mod", "rem", "sin", "cos", "tan", "asin", "acos", "atan", "atan2", "pow", "sqrt",
  "hypot", "log", "exp", "round",
];

pub fn has_math_fn(input: &str) -> bool {
  input.contains('(') && MATH_FUNCTIONS.iter().any(|f| input.contains(&format!("{f}(")))
}

fn is_operator(c: char) -> bool {
  matches!(c, '+' | '*' | '/' | '-')
}

/// Puts spaces around `+`, `-`, `*` and `/` inside math functions, as Tailwind does for
/// arbitrary values like `calc(100%-1rem)`.
pub fn add_whitespace_around_math_operators(input: &str) -> String {
  if !MATH_FUNCTIONS.iter().any(|f| input.contains(f)) {
    return input.to_owned();
  }

  let chars: Vec<char> = input.chars().collect();
  let at = |i: isize| -> Option<char> { if i < 0 { None } else { chars.get(i as usize).copied() } };

  let mut result = String::with_capacity(input.len() + 8);
  let mut formattable: Vec<bool> = Vec::new();
  let mut value_pos: Option<usize> = None;
  let mut last_value_pos: Option<usize> = None;

  let top = |f: &Vec<bool>| f.last().copied().unwrap_or(false);

  let mut i = 0usize;
  while i < chars.len() {
    let c = chars[i];

    if c.is_ascii_digit() || (value_pos.is_some() && (c == '%' || c.is_ascii_alphabetic())) {
      value_pos = Some(i);
    } else {
      last_value_pos = value_pos;
      value_pos = None;
    }

    if c == '(' {
      result.push(c);
      let mut start = i;
      let mut j = i as isize - 1;
      while j >= 0 {
        let inner = chars[j as usize];
        if inner.is_ascii_digit() || inner.is_ascii_lowercase() {
          start = j as usize;
        } else {
          break;
        }
        j -= 1;
      }
      let name: String = chars[start..i].iter().collect();
      formattable.push(MATH_FUNCTIONS.contains(&name.as_str()) || (top(&formattable) && name.is_empty()));
      i += 1;
      continue;
    } else if c == ')' {
      result.push(c);
      formattable.pop();
    } else if c == ',' && top(&formattable) {
      result.push_str(", ");
      i += 1;
      continue;
    } else if c == ' ' && top(&formattable) && result.ends_with(' ') {
      i += 1;
      continue;
    } else if is_operator(c) && top(&formattable) {
      let trimmed = result.trim_end();
      let mut rev = trimmed.chars().rev();
      let prev = rev.next();
      let prev_prev = rev.next();
      let next = at(i as isize + 1);

      if (matches!(prev, Some('e') | Some('E')) && prev_prev.is_some_and(|p| p.is_ascii_digit()))
        || prev.is_some_and(is_operator)
        || matches!(prev, Some('(') | Some(','))
      {
        result.push(c);
        i += 1;
        continue;
      } else if at(i as isize - 1) == Some(' ') {
        result.push(c);
        result.push(' ');
      } else if prev.is_some_and(|p| p.is_ascii_digit())
        || next.is_some_and(|n| n.is_ascii_digit())
        || prev == Some(')')
        || next == Some('(')
        || next.is_some_and(is_operator)
        || (last_value_pos.is_some() && last_value_pos == i.checked_sub(1))
      {
        result.push(' ');
        result.push(c);
        result.push(' ');
      } else {
        result.push(c);
      }
    } else {
      result.push(c);
    }
    i += 1;
  }

  result
}

#[cfg(test)]
mod tests {
  use super::add_whitespace_around_math_operators as f;

  #[test]
  fn spaces_operators_in_math_functions() {
    assert_eq!(f("calc(100%-1rem)"), "calc(100% - 1rem)");
    assert_eq!(f("calc(1px+2px)"), "calc(1px + 2px)");
    assert_eq!(f("calc(-3.4e-2*2)"), "calc(-3.4e-2 * 2)");
    assert_eq!(f("min(1px,2px)"), "min(1px, 2px)");
    assert_eq!(f("var(--a-b)"), "var(--a-b)");
    assert_eq!(f("calc(var(--a)-var(--b))"), "calc(var(--a) - var(--b))");
  }
}
