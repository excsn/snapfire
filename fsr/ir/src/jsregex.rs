//! JavaScript regular expressions over `regress`, which parses and matches ECMAScript syntax. Callers see `JsRegex` and `Found`, never the engine's types, so the engine can be replaced here alone. The engine backtracks without a step budget, so the caps below are what bound a match: a pattern length, a subject length and a refusal of nested unbounded quantifiers.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::{Arc, OnceLock};

use parking_lot::Mutex;
use snapfire_fsr_core::{Value, ValueMap};

/// The field a lowered regular expression keeps its source under.
pub const PATTERN: &str = "$regex";
/// The field it keeps its flags under.
pub const FLAGS: &str = "$flags";
/// The longest pattern compiled, in characters.
pub const MAX_PATTERN: usize = 1024;
/// The longest subject matched, in bytes.
pub const MAX_INPUT: usize = 1 << 20;

const CACHED: usize = 512;

/// A regular expression as a value: its source and flags.
pub fn value(pattern: &str, flags: &str) -> Value {
  let mut map = ValueMap::default();
  map.insert(PATTERN.to_owned(), Value::str(pattern));
  map.insert(FLAGS.to_owned(), Value::str(flags));
  Value::Map(map)
}

/// The source and flags of a value that is a regular expression.
pub fn of(value: &Value) -> Option<(&str, &str)> {
  let Value::Map(map) = value else { return None };
  match (map.get(PATTERN), map.get(FLAGS)) {
    (Some(Value::Str(pattern)), Some(Value::Str(flags))) => Some((pattern, flags)),
    _ => None,
  }
}

/// A compiled pattern.
#[derive(Debug)]
pub struct JsRegex {
  re: regress::Regex,
  unicode: bool,
}

/// One match as byte ranges into the subject: the whole match, then each group, `None` for a group that did not take part.
#[derive(Debug, Clone)]
pub struct Found {
  groups: Vec<Option<Range<usize>>>,
  names: Vec<(Box<str>, Option<Range<usize>>)>,
}

impl Found {
  pub fn whole(&self) -> Range<usize> {
    self.groups[0].clone().expect("a match has its whole")
  }

  /// The number of groups counting the whole match.
  pub fn len(&self) -> usize {
    self.groups.len()
  }

  pub fn is_empty(&self) -> bool {
    false
  }

  pub fn group(&self, index: usize) -> Option<Range<usize>> {
    self.groups.get(index).cloned().flatten()
  }

  pub fn named(&self, name: &str) -> Option<Range<usize>> {
    self.names.iter().find(|(n, _)| &**n == name).and_then(|(_, r)| r.clone())
  }

  /// Whether the pattern names any group, which decides whether `$<` in a replacement is a reference.
  pub fn has_names(&self) -> bool {
    !self.names.is_empty()
  }

  /// The whole match and each group as text, `None` for a group that did not take part.
  pub fn texts<'a, 's: 'a>(&'a self, subject: &'s str) -> impl Iterator<Item = Option<&'s str>> + 'a {
    self.groups.iter().map(move |g| g.clone().map(|r| &subject[r]))
  }
}

impl JsRegex {
  pub fn is_match(&self, subject: &str) -> Result<bool, String> {
    within_cap(subject)?;
    if !self.needs_units(subject) {
      return Ok(self.re.find(subject).is_some());
    }
    let units: Vec<u16> = subject.encode_utf16().collect();
    Ok(self.re.find_from_ucs2(&units, 0).next().is_some())
  }

  pub fn first(&self, subject: &str) -> Result<Option<Found>, String> {
    Ok(self.matches(subject, 1)?.pop())
  }

  /// Every match left to right, as a global expression walks the subject.
  pub fn all(&self, subject: &str) -> Result<Vec<Found>, String> {
    self.matches(subject, usize::MAX)
  }

  /// Without `u` or `v` JavaScript matches UTF-16 code units, which differs from matching characters only when the subject holds one past U+FFFF.
  fn needs_units(&self, subject: &str) -> bool {
    !self.unicode && subject.chars().any(|c| c > '\u{FFFF}')
  }

  fn matches(&self, subject: &str, limit: usize) -> Result<Vec<Found>, String> {
    within_cap(subject)?;
    if !self.needs_units(subject) {
      return self.re.find_iter(subject).take(limit).map(|m| found(&m, |r| Ok(r))).collect();
    }
    let units: Vec<u16> = subject.encode_utf16().collect();
    let offsets = byte_offsets(subject);
    let byte = |at: usize| offsets[at].ok_or_else(|| "a match that splits a surrogate pair, which no Rust string can hold".to_owned());
    self.re.find_from_ucs2(&units, 0).take(limit).map(|m| found(&m, |r| Ok(byte(r.start)?..byte(r.end)?))).collect()
  }
}

fn within_cap(subject: &str) -> Result<(), String> {
  if subject.len() > MAX_INPUT {
    return Err(format!("a regular expression over a subject longer than {MAX_INPUT} bytes"));
  }
  Ok(())
}

/// The byte offset of each UTF-16 index, `None` between the halves of a surrogate pair.
fn byte_offsets(subject: &str) -> Vec<Option<usize>> {
  let mut out = Vec::with_capacity(subject.len() + 1);
  for (at, c) in subject.char_indices() {
    out.push(Some(at));
    if c.len_utf16() == 2 {
      out.push(None);
    }
  }
  out.push(Some(subject.len()));
  out
}

fn found(m: &regress::Match, map: impl Fn(Range<usize>) -> Result<Range<usize>, String>) -> Result<Found, String> {
  let groups = std::iter::once(Some(m.range())).chain(m.captures.iter().cloned()).map(|g| g.map(&map).transpose()).collect::<Result<_, _>>()?;
  let names = m.named_groups().map(|(name, r)| Ok((name.into(), r.map(&map).transpose()?))).collect::<Result<_, String>>()?;
  Ok(Found { groups, names })
}

fn cache() -> &'static Mutex<HashMap<(String, String), Arc<JsRegex>>> {
  static CACHE: OnceLock<Mutex<HashMap<(String, String), Arc<JsRegex>>>> = OnceLock::new();
  CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The compiled expression, built once per source and flags; the error names what is refused.
pub fn compiled(pattern: &str, flags: &str) -> Result<Arc<JsRegex>, String> {
  let key = (pattern.to_owned(), flags.to_owned());
  if let Some(found) = cache().lock().get(&key) {
    return Ok(found.clone());
  }
  if pattern.chars().count() > MAX_PATTERN {
    return Err(format!("a pattern longer than {MAX_PATTERN} characters"));
  }
  let mut unicode = false;
  let mut sets = false;
  for flag in flags.chars() {
    match flag {
      'd' | 'g' | 'i' | 'm' | 's' => {}
      'u' => unicode = true,
      'v' => (unicode, sets) = (true, true),
      'y' => return Err("the sticky flag `y`".to_owned()),
      other => return Err(format!("the flag `{other}`")),
    }
  }
  bounded_repetition(pattern, sets)?;
  let re = regress::Regex::with_flags(pattern, flags).map_err(|e| format!("a pattern JavaScript would not compile: {e}"))?;
  let compiled = Arc::new(JsRegex { re, unicode });
  let mut held = cache().lock();
  if held.len() >= CACHED {
    held.clear();
  }
  held.insert(key, compiled.clone());
  Ok(compiled)
}

/// How an atom repeats: `Fixed` must match once each time its alternative does, `Varying` matches a number of times the subject decides, `Empty` may match nothing.
#[derive(Clone, Copy, PartialEq)]
enum Atom {
  Fixed,
  Varying,
  Empty,
}

#[derive(Default)]
struct Group {
  start: usize,
  zero_width: bool,
  fixed: bool,
  varying: bool,
  any_fixed: bool,
  loose: bool,
}

impl Group {
  fn add(&mut self, atom: Atom) {
    match atom {
      Atom::Fixed => self.fixed = true,
      Atom::Varying => self.varying = true,
      Atom::Empty => {}
    }
  }

  fn end_alternative(&mut self) {
    self.loose |= self.varying && !self.fixed;
    self.any_fixed |= self.fixed;
    self.fixed = false;
    self.varying = false;
  }
}

/// Refuses an unbounded quantifier over a group with an alternative made only of repeating atoms, `(a+)+` or `(\s*\w+)*`, which backtracks for time exponential in the subject. A group with a fixed atom in every alternative, `(\d+,)*`, passes.
fn bounded_repetition(pattern: &str, sets: bool) -> Result<(), String> {
  let chars: Vec<char> = pattern.chars().collect();
  let mut stack = vec![Group::default()];
  let mut pending: Option<(Atom, Option<(usize, bool)>)> = None;
  let mut i = 0;
  while i < chars.len() {
    let c = chars[i];
    let quantifier = match c {
      '*' => Some((0, None, i + 1)),
      '+' => Some((1, None, i + 1)),
      '?' => Some((0, Some(1), i + 1)),
      '{' => braces(&chars, i),
      _ => None,
    };
    if let (Some((min, max, next)), Some((atom, group))) = (quantifier, pending) {
      if max.is_none() && atom != Atom::Empty {
        if let Some((start, true)) = group {
          let source: String = chars[start..next].iter().collect();
          return Err(format!("a nested quantifier, `{source}`, which can backtrack for time exponential in the subject"));
        }
      }
      let repeated = match max {
        _ if atom == Atom::Empty => Atom::Empty,
        Some(0) => Atom::Empty,
        Some(1) if min == 0 => Atom::Empty,
        Some(max) if max == min => atom,
        _ => Atom::Varying,
      };
      pending = Some((repeated, None));
      i = if chars.get(next) == Some(&'?') { next + 1 } else { next };
      continue;
    }
    if let Some((atom, _)) = pending.take() {
      stack.last_mut().expect("the pattern's own group").add(atom);
    }
    match c {
      '\\' => {
        let next = chars.get(i + 1).copied();
        i += 2;
        match next {
          Some('b' | 'B') => pending = Some((Atom::Empty, None)),
          Some('p' | 'P' | 'u') if chars.get(i) == Some(&'{') => {
            i = skip_to(&chars, i, '}');
            pending = Some((Atom::Fixed, None));
          }
          Some('k') if chars.get(i) == Some(&'<') => {
            i = skip_to(&chars, i, '>');
            pending = Some((Atom::Fixed, None));
          }
          _ => pending = Some((Atom::Fixed, None)),
        }
        continue;
      }
      '[' => {
        i = class_end(&chars, i, sets);
        pending = Some((Atom::Fixed, None));
        continue;
      }
      '(' => {
        let rest: String = chars[i + 1..chars.len().min(i + 4)].iter().collect();
        let zero_width = ["?=", "?!", "?<=", "?<!"].iter().any(|p| rest.starts_with(p));
        stack.push(Group { start: i, zero_width, ..Group::default() });
        i = match rest.as_str() {
          _ if zero_width => i + 1 + if rest.starts_with("?<") { 3 } else { 2 },
          _ if rest.starts_with("?<") => skip_to(&chars, i, '>'),
          _ if rest.starts_with('?') => skip_to(&chars, i, ':'),
          _ => i + 1,
        };
        continue;
      }
      ')' if stack.len() > 1 => {
        let mut group = stack.pop().expect("checked");
        group.end_alternative();
        let atom = if group.zero_width {
          Atom::Empty
        } else if group.loose {
          Atom::Varying
        } else if group.any_fixed {
          Atom::Fixed
        } else {
          Atom::Empty
        };
        pending = Some((atom, Some((group.start, group.loose && !group.zero_width))));
      }
      '|' => stack.last_mut().expect("the pattern's own group").end_alternative(),
      '^' | '$' => pending = Some((Atom::Empty, None)),
      _ => pending = Some((Atom::Fixed, None)),
    }
    i += 1;
  }
  Ok(())
}

/// `{min}`, `{min,}` or `{min,max}` at `at`, with the index past it; anything else is a literal brace.
fn braces(chars: &[char], at: usize) -> Option<(u32, Option<u32>, usize)> {
  let close = chars[at..].iter().position(|&c| c == '}')? + at;
  let body: String = chars[at + 1..close].iter().collect();
  let (min, max) = match body.split_once(',') {
    None => {
      let n = body.parse().ok()?;
      (n, Some(n))
    }
    Some((min, "")) => (min.parse().ok()?, None),
    Some((min, max)) => (min.parse().ok()?, Some(max.parse().ok()?)),
  };
  Some((min, max, close + 1))
}

fn skip_to(chars: &[char], from: usize, end: char) -> usize {
  chars[from..].iter().position(|&c| c == end).map_or(chars.len(), |p| from + p + 1)
}

/// The index past the class opening at `at`; under `v` a class nests.
fn class_end(chars: &[char], at: usize, sets: bool) -> usize {
  let mut depth = 0;
  let mut i = at;
  while i < chars.len() {
    match chars[i] {
      '\\' => i += 1,
      '[' if depth == 0 || sets => depth += 1,
      ']' => {
        depth -= 1;
        if depth == 0 {
          return i + 1;
        }
      }
      _ => {}
    }
    i += 1;
  }
  chars.len()
}

/// A UTF-8 byte offset into `s` as the UTF-16 index JavaScript reports.
pub fn utf16_index(s: &str, byte: usize) -> usize {
  s[..byte].encode_utf16().count()
}

/// `replace`'s replacement string for one match: `$$`, `$&`, `` $` ``, `$'`, `$1` to `$99` and `$<name>`, a group that did not take part as nothing.
pub fn expand(out: &mut String, replacement: &str, subject: &str, found: &Found) {
  let whole = found.whole();
  let text = |g: Option<Range<usize>>| g.map_or("", |r| &subject[r]);
  let bytes = replacement.as_bytes();
  let mut i = 0;
  while i < bytes.len() {
    if bytes[i] != b'$' || i + 1 >= bytes.len() {
      let ch = replacement[i..].chars().next().expect("in bounds");
      out.push(ch);
      i += ch.len_utf8();
      continue;
    }
    match bytes[i + 1] {
      b'$' => {
        out.push('$');
        i += 2;
      }
      b'&' => {
        out.push_str(&subject[whole.clone()]);
        i += 2;
      }
      b'`' => {
        out.push_str(&subject[..whole.start]);
        i += 2;
      }
      b'\'' => {
        out.push_str(&subject[whole.end..]);
        i += 2;
      }
      b'0'..=b'9' => {
        let one = (bytes[i + 1] - b'0') as usize;
        let two = bytes.get(i + 2).filter(|b| b.is_ascii_digit()).map(|b| one * 10 + (b - b'0') as usize);
        match two.filter(|n| *n >= 1 && *n < found.len()) {
          Some(n) => {
            out.push_str(text(found.group(n)));
            i += 3;
          }
          None if one >= 1 && one < found.len() => {
            out.push_str(text(found.group(one)));
            i += 2;
          }
          None => {
            out.push('$');
            i += 1;
          }
        }
      }
      b'<' if found.has_names() => match replacement[i + 2..].find('>') {
        Some(end) => {
          out.push_str(text(found.named(&replacement[i + 2..i + 2 + end])));
          i += 3 + end;
        }
        None => {
          out.push('$');
          i += 1;
        }
      },
      _ => {
        out.push('$');
        i += 1;
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_classes_mean_what_they_mean_in_javascript() {
    let re = compiled(r"\d+\w*", "").unwrap();
    assert!(!re.is_match("٣").unwrap(), "an Arabic-Indic digit is no `\\d`");
    assert!(re.is_match("3x").unwrap());
    assert!(!compiled("a.b", "").unwrap().is_match("a\u{2028}b").unwrap(), "`.` stops at a line terminator");
    assert!(compiled("a.b", "s").unwrap().is_match("a\nb").unwrap());
    assert!(!compiled("[]a", "").unwrap().is_match("a").unwrap(), "`[]` matches nothing");
    assert!(compiled("[[a]", "").unwrap().is_match("[").unwrap(), "`[` inside a class is itself");
  }
}
