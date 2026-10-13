use std::sync::LazyLock;

use regex::Regex;

use crate::Error;
use crate::utils::segment::segment;

static RANGE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(-?[0-9]+)\.\.(-?[0-9]+)(?:\.\.(-?[0-9]+))?$").unwrap());

/// Expands `bg-{red,blue}-{1..3}00` into every combination, as `@source inline(…)` does.
pub fn expand(pattern: &str) -> Result<Vec<String>, Error> {
  let Some(index) = pattern.find('{') else {
    return Ok(vec![pattern.to_owned()]);
  };

  let pre = &pattern[..index];
  let rest = &pattern[index..];

  let mut depth = 0i32;
  let mut end = rest.rfind('}');
  for (i, c) in rest.char_indices() {
    if c == '{' {
      depth += 1;
    } else if c == '}' {
      depth -= 1;
      if depth == 0 {
        end = Some(i);
        break;
      }
    }
  }
  let Some(end) = end else {
    return Err(Error::Message(format!("The pattern `{pattern}` is not balanced.")));
  };

  let inside = &rest[1..end];
  let post = &rest[end + 1..];

  let parts: Vec<String> = if RANGE.is_match(inside) {
    expand_sequence(inside)?
  } else {
    segment(inside, b',').into_iter().map(str::to_owned).collect()
  };

  let mut expanded_parts = Vec::new();
  for part in parts {
    expanded_parts.extend(expand(&part)?);
  }

  let mut result = Vec::new();
  for tail in expand(post)? {
    for part in &expanded_parts {
      result.push(format!("{pre}{part}{tail}"));
    }
  }
  Ok(result)
}

fn expand_sequence(seq: &str) -> Result<Vec<String>, Error> {
  let Some(caps) = RANGE.captures(seq) else {
    return Ok(vec![seq.to_owned()]);
  };
  let start: i64 = caps[1].parse().unwrap_or(0);
  let end: i64 = caps[2].parse().unwrap_or(0);
  let mut step: i64 = match caps.get(3) {
    Some(m) => m.as_str().parse().unwrap_or(0),
    None => {
      if start <= end {
        1
      } else {
        -1
      }
    }
  };
  if step == 0 {
    return Err(Error::Message("Step cannot be zero in sequence expansion.".to_owned()));
  }
  let increasing = start < end;
  if increasing && step < 0 {
    step = -step;
  }
  if !increasing && step > 0 {
    step = -step;
  }
  let mut result = Vec::new();
  let mut i = start;
  while if increasing { i <= end } else { i >= end } {
    result.push(i.to_string());
    i += step;
  }
  Ok(result)
}

#[cfg(test)]
mod tests {
  use super::expand;

  #[test]
  fn expands() {
    assert_eq!(expand("bg-{red,blue}-500").unwrap(), vec!["bg-red-500", "bg-blue-500"]);
    assert_eq!(expand("p-{1..3}").unwrap(), vec!["p-1", "p-2", "p-3"]);
    assert_eq!(expand("{a,b}-{1,2}").unwrap(), vec!["a-1", "b-1", "a-2", "b-2"]);
    assert_eq!(expand("w-{0..100..50}").unwrap(), vec!["w-0", "w-50", "w-100"]);
  }
}
