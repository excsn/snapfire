//! The front end for Tera templates. Tera itself decides whether a template parses; the rules here read
//! the inside of its delimiters, which is all they need.

use std::path::Path;

use crate::{Level, RuleInfo};

/// A finding by byte offset.
pub(crate) struct Found {
  pub rule: &'static str,
  pub start: usize,
  pub message: String,
}

pub(crate) const RULES: [RuleInfo; 3] = [
  RuleInfo {
    id: "tera/no-debug",
    default: Level::Error,
    category: "correctness",
    fixable: false,
    docs: "### What it does\n\nDisallows `__tera_context`.\n\n### Why is this bad?\n\nIt prints every value the template was given, which is a debugging aid left in a page and can show what a visitor should not see.\n",
  },
  RuleInfo {
    id: "tera/no-safe-filter",
    default: Level::Warn,
    category: "suspicious",
    fixable: false,
    docs: "### What it does\n\nFlags the `safe` filter.\n\n### Why is this bad?\n\n`safe` turns escaping off, so a value holding markup a visitor wrote runs in the page. Keep it to values the application made, and say why with a suppression comment.\n",
  },
  RuleInfo {
    id: "tera/no-unused-set",
    default: Level::Warn,
    category: "correctness",
    fixable: false,
    docs: "### What it does\n\nFlags a `{% set %}` whose name nothing in the template reads.\n\n### Why is this bad?\n\nIt is dead, or the name it is read under is misspelled. `set_global` is not checked, since an included template may read it.\n",
  },
];

/// Whether this front end reads `path`.
pub(crate) fn reads(path: &Path) -> bool {
  path.extension().and_then(|e| e.to_str()) == Some("tera")
}

#[derive(Debug, PartialEq)]
enum Token<'s> {
  Ident(&'s str),
  Str,
  Punct(char),
}

/// The inside of one `{{ }}` or `{% %}`, as tokens with their byte offsets.
struct Tag<'s> {
  statement: bool,
  tokens: Vec<(Token<'s>, usize)>,
}

fn tags(source: &str) -> Vec<Tag<'_>> {
  let mut out = Vec::new();
  let bytes = source.as_bytes();
  let mut i = 0;
  while i + 1 < bytes.len() {
    if bytes[i] != b'{' || !matches!(bytes[i + 1], b'{' | b'%' | b'#') {
      i += 1;
      continue;
    }
    let open = bytes[i + 1];
    let close: &[u8] = match open {
      b'{' => b"}}",
      b'%' => b"%}",
      _ => b"#}",
    };
    let start = i + 2;
    if open == b'#' {
      i = source[start..].find("#}").map_or(bytes.len(), |n| start + n + 2);
      continue;
    }
    let mut tokens = Vec::new();
    let mut j = start;
    while j < bytes.len() {
      if bytes[j..].starts_with(close) {
        break;
      }
      let c = bytes[j];
      match c {
        b'"' | b'\'' | b'`' => {
          let at = j;
          j += 1;
          while j < bytes.len() && bytes[j] != c {
            j += if bytes[j] == b'\\' { 2 } else { 1 };
          }
          j += 1;
          tokens.push((Token::Str, at));
        }
        c if c.is_ascii_alphabetic() || c == b'_' => {
          let at = j;
          while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
            j += 1;
          }
          tokens.push((Token::Ident(&source[at..j]), at));
        }
        c if c.is_ascii_whitespace() || c.is_ascii_digit() => j += 1,
        c => {
          tokens.push((Token::Punct(c as char), j));
          j += 1;
        }
      }
    }
    out.push(Tag { statement: open == b'%', tokens });
    i = j + 2;
  }
  out
}

/// Tera's own verdict: the byte offset and message of a syntax error, if the template has one.
fn syntax_error(path: &Path, source: &str) -> Option<(usize, String)> {
  let mut tera = tera::Tera::default();
  let error = tera.add_raw_template(&path.display().to_string(), source).err()?;
  let mut current: Option<&(dyn std::error::Error + 'static)> = Some(&error);
  while let Some(e) = current {
    if let Some(tera_error) = e.downcast_ref::<tera::Error>()
      && let tera::ErrorKind::SyntaxError(report) = tera_error.kind()
    {
      return Some((report.span().range.start, report.message().to_owned()));
    }
    current = e.source();
  }
  None
}

/// The findings in one template, or its syntax error.
pub(crate) fn lint(path: &Path, source: &str) -> Result<Vec<Found>, (usize, String)> {
  if let Some(error) = syntax_error(path, source) {
    return Err(error);
  }
  let tags = tags(source);
  let mut found = Vec::new();
  let mut sets = Vec::new();
  for (index, tag) in tags.iter().enumerate() {
    for (k, (token, at)) in tag.tokens.iter().enumerate() {
      match token {
        Token::Ident("__tera_context") => found.push(Found { rule: "tera/no-debug", start: *at, message: "`__tera_context` prints every value the template was given".to_owned() }),
        Token::Ident("safe") if k > 0 && tag.tokens[k - 1].0 == Token::Punct('|') => found.push(Found { rule: "tera/no-safe-filter", start: *at, message: "`safe` turns escaping off for this value".to_owned() }),
        _ => {}
      }
    }
    if tag.statement
      && let [(Token::Ident("set"), _), (Token::Ident(name), at), rest @ ..] = tag.tokens.as_slice()
      && matches!(rest.first(), None | Some((Token::Punct('='), _)))
    {
      sets.push((index, *name, *at));
    }
  }
  for (index, name, at) in sets {
    let read = tags.iter().enumerate().any(|(i, tag)| tag.tokens.iter().enumerate().any(|(k, (token, _))| *token == Token::Ident(name) && !(i == index && k == 1)));
    if !read {
      found.push(Found { rule: "tera/no-unused-set", start: at, message: format!("`{name}` is set and never read") });
    }
  }
  found.sort_by_key(|f| f.start);
  Ok(found)
}

/// The `{# #}` comments of a template, with their byte offsets.
pub(crate) fn comments(source: &str) -> Vec<(String, usize, usize)> {
  let mut out = Vec::new();
  let mut from = 0;
  while let Some(start) = source[from..].find("{#").map(|i| from + i) {
    let Some(end) = source[start..].find("#}").map(|i| start + i + 2) else { break };
    out.push((source[start..end].to_owned(), start, end));
    from = end;
  }
  out
}
