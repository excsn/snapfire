use std::cell::Cell;
use std::collections::BTreeSet;

use crate::config::full_id;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
  NextLine,
  Line,
  Disable,
  Enable,
}

/// A suppression comment.
#[derive(Debug)]
pub(crate) struct Directive {
  kind: Kind,
  /// `None` names every rule.
  rules: Option<BTreeSet<String>>,
  pub reason: Option<String>,
  pub start: usize,
  /// The 1-based lines the comment starts and ends on.
  line: usize,
  end_line: usize,
  pub written: String,
  used: Cell<bool>,
}

impl Directive {
  pub fn used(&self) -> bool {
    self.used.get()
  }

  /// Whether this comment silences anything; an `enable` never does.
  pub fn silences(&self) -> bool {
    self.kind != Kind::Enable
  }

  /// The rules the comment names, or `None` when it names every rule.
  pub fn rules(&self) -> Option<&BTreeSet<String>> {
    self.rules.as_ref()
  }

  fn names(&self, rule: &str) -> bool {
    self.rules.as_ref().is_none_or(|rules| rules.contains(rule))
  }
}

/// The prefixes a suppression comment starts with, longest first so `disable-next-line` is not read as `disable`.
const PREFIXES: [(&str, Kind); 8] = [
  ("fsr-lint-disable-next-line", Kind::NextLine),
  ("eslint-disable-next-line", Kind::NextLine),
  ("fsr-lint-disable-line", Kind::Line),
  ("eslint-disable-line", Kind::Line),
  ("fsr-lint-disable", Kind::Disable),
  ("eslint-disable", Kind::Disable),
  ("fsr-lint-enable", Kind::Enable),
  ("eslint-enable", Kind::Enable),
];

/// The text inside a comment's delimiters: `//`, `/* */`, `<!-- -->` or `{# #}`.
fn inner(text: &str) -> &str {
  let text = text.trim();
  if let Some(rest) = text.strip_prefix("//") {
    return rest;
  }
  for (open, close) in [("/*", "*/"), ("<!--", "-->"), ("{#", "#}")] {
    if let Some(rest) = text.strip_prefix(open) {
      return rest.strip_suffix(close).unwrap_or(rest);
    }
  }
  text
}

/// The suppression comments among `comments`, each a comment's text with delimiters and its byte offsets.
pub(crate) fn directives<'c>(comments: impl IntoIterator<Item = (&'c str, usize, usize)>, lines: &Lines) -> Vec<Directive> {
  let mut out = Vec::new();
  for (text, start, end) in comments {
    let body = inner(text).trim();
    let Some((prefix, kind)) = PREFIXES.iter().find(|(prefix, _)| body.strip_prefix(prefix).is_some_and(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))) else {
      continue;
    };
    let rest = &body[prefix.len()..];
    let (names, reason) = match rest.find("--").filter(|at| *at == 0 || rest[..*at].ends_with(char::is_whitespace)) {
      Some(at) => (&rest[..at], Some(rest[at + 2..].trim().to_owned()).filter(|r| !r.is_empty())),
      None => (rest, None),
    };
    let rules: BTreeSet<String> = names.split(',').map(str::trim).filter(|n| !n.is_empty()).map(full_id).collect();
    out.push(Directive {
      kind: *kind,
      rules: Some(rules).filter(|r| !r.is_empty()),
      reason,
      start,
      line: lines.line(start),
      end_line: lines.line(end.saturating_sub(1).max(start)),
      written: text.trim().to_owned(),
      used: Cell::new(false),
    });
  }
  out
}

/// The comment that silences `rule` at `offset` on `line`, marked used.
pub(crate) fn silencing<'d>(directives: &'d [Directive], rule: &str, offset: usize, line: usize) -> Option<&'d Directive> {
  let by_line = directives.iter().find(|d| d.names(rule) && ((d.kind == Kind::NextLine && line == d.end_line + 1) || (d.kind == Kind::Line && line == d.line)));
  let found = by_line.or_else(|| {
    let mut all: Option<&Directive> = None;
    let mut except: BTreeSet<&str> = BTreeSet::new();
    let mut named: Option<&Directive> = None;
    for d in directives.iter().filter(|d| d.start < offset && matches!(d.kind, Kind::Disable | Kind::Enable)) {
      match (d.kind, &d.rules) {
        (Kind::Disable, None) => {
          all = Some(d);
          except.clear();
          named = None;
        }
        (Kind::Disable, Some(rules)) if rules.contains(rule) => match all {
          Some(_) => {
            except.remove(rule);
          }
          None => named = Some(d),
        },
        (Kind::Enable, None) => {
          all = None;
          named = None;
        }
        (Kind::Enable, Some(rules)) if rules.contains(rule) => match all {
          Some(_) => {
            except.insert(rule);
          }
          None => named = None,
        },
        _ => {}
      }
    }
    named.or(all.filter(|_| !except.contains(rule)))
  });
  if let Some(d) = found {
    d.used.set(true);
  }
  found
}

/// Byte offset to 1-based line and column.
pub(crate) struct Lines<'s> {
  source: &'s str,
  starts: Vec<usize>,
}

impl<'s> Lines<'s> {
  pub fn new(source: &'s str) -> Self {
    let starts = std::iter::once(0).chain(source.match_indices('\n').map(|(i, _)| i + 1)).collect();
    Lines { source, starts }
  }

  pub fn line(&self, offset: usize) -> usize {
    self.starts.partition_point(|start| *start <= offset)
  }

  /// The byte offset of a 1-based line and column, the column counted in characters.
  pub fn offset(&self, line: usize, column: usize) -> usize {
    let Some(start) = self.starts.get(line.saturating_sub(1)).copied() else { return self.source.len() };
    let rest = &self.source[start..];
    start + rest.char_indices().nth(column.saturating_sub(1)).map_or(rest.len(), |(i, _)| i)
  }

  /// The column in characters.
  pub fn column(&self, offset: usize) -> usize {
    let start = self.starts[self.line(offset) - 1];
    let offset = offset.min(self.source.len());
    self.source.get(start..offset).map_or(1, |s| s.chars().count() + 1)
  }
}
