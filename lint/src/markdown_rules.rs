//! The rest of `@eslint/markdown`'s rules over markdown-rs's tree: label references, reference-like URLs, spaces
//! inside emphasis, unused definitions, fence metadata, bare URLs, duplicate headings, HTML and table columns.

use std::collections::HashSet;

use markdown::mdast::Node;
use serde_json::Value;

use crate::markdown::Found;

/// GitHub's alert markers, `> [!NOTE]`, which read as a label but are not one.
const GITHUB_ALERTS: [&str; 5] = ["!NOTE", "!TIP", "!IMPORTANT", "!WARNING", "!CAUTION"];

/// A rule's options: the list ESLint takes after the level.
pub(crate) type Options<'o> = &'o dyn Fn(&str) -> Option<Value>;

fn option<'v>(options: &'v Option<Value>, key: &str) -> Option<&'v Value> {
  options.as_ref()?.get(0)?.get(key)
}

fn start(node: &Node) -> usize {
  node.position().map_or(0, |p| p.start.offset)
}

fn end(node: &Node) -> usize {
  node.position().map_or(0, |p| p.end.offset)
}

/// micromark's identifier: whitespace runs as one space, trimmed, lowercased.
pub(crate) fn normalize(label: &str) -> String {
  label.split([' ', '\t', '\n', '\r']).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" ").to_lowercase()
}

/// Every node of the tree in document order, each with its parent.
fn nodes<'n>(root: &'n Node) -> Vec<(&'n Node, Option<&'n Node>)> {
  let mut out = Vec::new();
  fn go<'n>(node: &'n Node, parent: Option<&'n Node>, out: &mut Vec<(&'n Node, Option<&'n Node>)>) {
    out.push((node, parent));
    for child in node.children().into_iter().flatten() {
      go(child, Some(node), out);
    }
  }
  go(root, None, &mut out);
  out
}

/// Whether the byte at `at` is preceded by an even number of backslashes, none included.
fn unescaped(text: &str, at: usize) -> bool {
  text.as_bytes()[..at].iter().rev().take_while(|b| **b == b'\\').count() % 2 == 0
}

pub(crate) fn lint(root: &Node, source: &str, options: Options<'_>, out: &mut Vec<Found>) {
  let all = nodes(root);
  let find = |out: &mut Vec<Found>, rule: &'static str, at: usize, message: String| out.push(Found { rule, start: at, message });
  let definitions: HashSet<String> = all.iter().filter_map(|(n, _)| if let Node::Definition(d) = n { Some(normalize(&d.identifier)) } else { None }).collect();

  for (node, _) in &all {
    let Node::Text(_) = node else { continue };
    let (at, text) = (start(node), &source[start(node)..end(node)]);
    invalid_label_refs(source, at, text, out);
  }

  let allow: Vec<String> = option(&options("markdown/no-missing-label-refs"), "allowLabels").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(str::to_owned).collect();
  for (node, _) in &all {
    let Node::Text(_) = node else { continue };
    let (at, text) = (start(node), &source[start(node)..end(node)]);
    for (label, offset) in missing_label_refs(text) {
      if !definitions.contains(&label) && !allow.contains(&label) && !GITHUB_ALERTS.iter().any(|alert| label.eq_ignore_ascii_case(alert)) {
        find(out, "markdown/no-missing-label-refs", at + offset, format!("Label reference '{label}' not found."));
      }
    }
  }

  for (node, _) in &all {
    let (kind, prefix) = match node {
      Node::Link(_) => ("link", ""),
      Node::Image(_) => ("image", "!"),
      _ => continue,
    };
    if let Some(destination) = resource_destination(&source[start(node)..end(node)])
      && definitions.contains(&normalize(destination))
    {
      find(out, "markdown/no-reference-like-urls", start(node), format!("Unexpected resource {kind} ('{prefix}[text](url)') with URL that matches a definition identifier. Use '[text][id]' syntax instead."));
    }
  }

  let strikethrough = option(&options("markdown/no-space-in-emphasis"), "checkStrikethrough").and_then(Value::as_bool).unwrap_or(false);
  for (node, _) in &all {
    if matches!(node, Node::Heading(_) | Node::Paragraph(_) | Node::TableCell(_)) {
      space_in_emphasis(node, source, strikethrough, out);
    }
  }

  unused_definitions(&all, &options("markdown/no-unused-definitions"), out);

  let never = options("markdown/fenced-code-meta").and_then(|o| o.get(0)?.as_str().map(str::to_owned)).as_deref() == Some("never");
  for (node, _) in &all {
    let Node::Code(code) = node else { continue };
    let at = start(node);
    if !source[at..].starts_with("```") && !source[at..].starts_with("~~~") {
      continue;
    }
    match (never, &code.lang, &code.meta) {
      (false, Some(_), None) => find(out, "markdown/fenced-code-meta", at, "Missing code block metadata.".to_owned()),
      (true, _, Some(meta)) => {
        let line_end = source[at..].find('\n').map_or(source.len(), |i| at + i);
        let meta_at = source[at..line_end].rfind(meta.trim_end()).map_or(at, |i| at + i);
        find(out, "markdown/fenced-code-meta", meta_at, "Code block metadata is not allowed.".to_owned());
      }
      _ => {}
    }
  }

  bare_urls(root, source, out);
  duplicate_headings(&all, option(&options("markdown/no-duplicate-headings"), "checkSiblingsOnly").and_then(Value::as_bool).unwrap_or(false), out);

  let html = options("markdown/no-html");
  let allowed: Vec<String> = option(&html, "allowed").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(str::to_owned).collect();
  let ignore_case = option(&html, "allowedIgnoreCase").and_then(Value::as_bool).unwrap_or(false);
  for (node, _) in &all {
    let Node::Html(_) = node else { continue };
    let at = start(node);
    let text = strip_html_comments(&source[at..end(node)]);
    for tag in lazy_regex::regex!(r#"(?i)<(?P<name>[a-z0-9]+(?:-[a-z0-9]+)*)(?:\s(?:[^>"']|"[^"]*"|'[^']*')*)?/?>"#).captures_iter(&text) {
      let name = &tag["name"];
      let permitted = allowed.iter().any(|a| if ignore_case { a.eq_ignore_ascii_case(name) } else { a == name });
      if !permitted {
        find(out, "markdown/no-html", at + tag.get(0).map_or(0, |m| m.start()), format!("HTML element \"{name}\" is not allowed."));
      }
    }
  }

  let check_missing = option(&options("markdown/table-column-count"), "checkMissingCells").and_then(Value::as_bool).unwrap_or(false);
  for (node, _) in &all {
    let Node::Table(table) = node else { continue };
    let Some(expected) = table.children.first().and_then(|row| row.children()).map(Vec::len) else { continue };
    for row in table.children.iter().skip(1) {
      let cells = row.children().map(Vec::as_slice).unwrap_or_default();
      if cells.len() > expected {
        find(out, "markdown/table-column-count", start(&cells[expected]), format!("Table column count mismatch (Expected: {expected}, Actual: {}), extra data starting here will be ignored.", cells.len()));
      } else if check_missing && cells.len() < expected {
        find(out, "markdown/table-column-count", end(row).saturating_sub(1), format!("Table column count mismatch (Expected: {expected}, Actual: {}), row might be missing data.", cells.len()));
      }
    }
  }
}

/// `[label][ ]`: a reference whose second brackets hold only whitespace.
fn invalid_label_refs(source: &str, at: usize, text: &str, out: &mut Vec<Found>) {
  for m in lazy_regex::regex!(r"\]\[([^\]]+)\]").captures_iter(text) {
    let whole = m.get(0).expect("the match");
    if !m[1].chars().all(char::is_whitespace) {
      continue;
    }
    let close = at + whole.start();
    let Some(open) = source[..close].rfind('[') else { continue };
    let Some(label) = lazy_regex::regex!(r"!?\[([^\]]+)\]").captures(&source[open..at + whole.end()]).map(|c| c[1].trim().to_owned()) else { continue };
    out.push(Found { rule: "markdown/no-invalid-label-refs", start: close + 1, message: format!("Label reference '{label}' is invalid due to white space between [ and ].") });
  }
}

/// Each `[label]`, `[text][label]` or `[label][]` left as text, with the label trimmed and its offset in `text`.
fn missing_label_refs(text: &str) -> Vec<(String, usize)> {
  let bytes = text.as_bytes();
  let group = |from: usize, stop_at_open: bool| -> Option<usize> {
    let mut i = from;
    while i < bytes.len() {
      match bytes[i] {
        b'\\' => i += 2,
        b']' => return Some(i),
        b'[' if stop_at_open => return None,
        _ => i += 1,
      }
    }
    None
  };
  let mut out = Vec::new();
  let mut i = 0;
  while i < bytes.len() {
    if bytes[i] != b'[' || !unescaped(text, i) {
      i += 1;
      continue;
    }
    let Some(left_end) = group(i + 1, true) else {
      i += 1;
      continue;
    };
    let left = &text[i + 1..left_end];
    let mut after = left_end + 1;
    let mut right: Option<(usize, &str)> = None;
    if bytes.get(after) == Some(&b'[')
      && let Some(right_end) = group(after + 1, false)
    {
      right = Some((after + 1, &text[after + 1..right_end]));
      after = right_end + 1;
    }
    let tail_is_space = right.is_some_and(|(_, r)| !r.is_empty() && r.chars().all(char::is_whitespace));
    let both_empty = left.is_empty() && right.is_none_or(|(_, r)| r.is_empty());
    if !tail_is_space && !both_empty {
      let (offset, label) = match right {
        Some((offset, r)) if !r.is_empty() => (offset, r),
        _ => (i + 1, left),
      };
      out.push((label.trim().to_owned(), offset));
    }
    i = after;
  }
  out
}

/// The destination of an inline `[text](destination)` or `![alt](destination)` written with no title.
fn resource_destination(written: &str) -> Option<&str> {
  let rest = written.strip_prefix('!').unwrap_or(written).strip_prefix('[')?;
  let close = rest.find("](")?;
  let inner = rest[close + 2..].strip_suffix(')')?;
  let destination = inner.trim_start_matches([' ', '\t']).trim_start_matches(['\r', '\n']).trim_start_matches([' ', '\t']);
  if destination.starts_with('<') || destination.contains([' ', '\t', '(', ')']) || destination.is_empty() {
    return None;
  }
  Some(destination.trim_end())
}

/// Emphasis markers that stayed text because of the space beside them, per block.
fn space_in_emphasis(block: &Node, source: &str, strikethrough: bool, out: &mut Vec<Found>) {
  let (from, to) = (start(block), end(block));
  let mut masked: Vec<u8> = vec![b' '; to - from];
  for child in block.children().into_iter().flatten() {
    if let Node::Text(_) = child {
      masked[start(child) - from..end(child) - from].copy_from_slice(&source.as_bytes()[start(child)..end(child)]);
    }
  }
  let masked = String::from_utf8_lossy(&masked).into_owned();
  let markers: &[&str] = if strikethrough { &["***", "**", "*", "___", "__", "_", "~~", "~"] } else { &["***", "**", "*", "___", "__", "_"] };
  let mut found: Vec<(&str, usize)> = Vec::new();
  let mut i = 0;
  while i < masked.len() {
    match markers.iter().find(|m| masked[i..].starts_with(**m)).filter(|_| unescaped(&masked, i)) {
      Some(marker) => {
        found.push((marker, i));
        i += marker.len();
      }
      None => i += masked[i..].chars().next().map_or(1, char::len_utf8),
    }
  }
  for marker in markers {
    let at: Vec<usize> = found.iter().filter(|(m, _)| m == marker).map(|(_, at)| *at).collect();
    for pair in at.chunks_exact(2) {
      let (open, close) = (from + pair[0], from + pair[1]);
      let space = |c: Option<u8>| matches!(c, Some(b' ' | b'\t'));
      if space(source.as_bytes().get(open + marker.len()).copied()) {
        out.push(Found { rule: "markdown/no-space-in-emphasis", start: open, message: "Unexpected space around emphasis marker.".to_owned() });
      }
      if close > 0 && space(source.as_bytes().get(close - 1).copied()) {
        out.push(Found { rule: "markdown/no-space-in-emphasis", start: close - 1, message: "Unexpected space around emphasis marker.".to_owned() });
      }
    }
  }
}

fn unused_definitions(all: &[(&Node, Option<&Node>)], options: &Option<Value>, out: &mut Vec<Found>) {
  let strings = |key: &str, default: &[&str]| -> Vec<String> {
    match option(options, key).and_then(Value::as_array) {
      Some(list) => list.iter().filter_map(Value::as_str).map(normalize).collect(),
      None => default.iter().map(|s| normalize(s)).collect(),
    }
  };
  let allow = strings("allowDefinitions", &["//"]);
  let allow_footnotes = strings("allowFootnoteDefinitions", &[]);
  let check_footnotes = option(options, "checkFootnoteDefinitions").and_then(Value::as_bool).unwrap_or(true);
  let mut used: HashSet<String> = HashSet::new();
  let mut used_footnotes: HashSet<String> = HashSet::new();
  for (node, _) in all {
    match node {
      Node::LinkReference(r) => {
        used.insert(normalize(&r.identifier));
      }
      Node::ImageReference(r) => {
        used.insert(normalize(&r.identifier));
      }
      Node::FootnoteReference(r) => {
        used_footnotes.insert(normalize(&r.identifier));
      }
      _ => {}
    }
  }
  for (node, _) in all {
    match node {
      Node::Definition(d) => {
        let id = normalize(&d.identifier);
        if !used.contains(&id) && !allow.contains(&id) {
          let label = d.label.as_deref().unwrap_or(&d.identifier).trim().to_owned();
          out.push(Found { rule: "markdown/no-unused-definitions", start: start(node), message: format!("Unexpected unused definition `{id}` (label: `{label}`) found.") });
        }
      }
      Node::FootnoteDefinition(d) if check_footnotes => {
        let id = normalize(&d.identifier);
        if !used_footnotes.contains(&id) && !allow_footnotes.contains(&id) {
          let label = d.label.as_deref().unwrap_or(&d.identifier).to_owned();
          out.push(Found { rule: "markdown/no-unused-definitions", start: start(node), message: format!("Unexpected unused footnote definition `{id}` (label: `{label}`) found.") });
        }
      }
      _ => {}
    }
  }
}

/// GFM autolink literals, outside an inline HTML element pair.
fn bare_urls(root: &Node, source: &str, out: &mut Vec<Found>) {
  fn block(node: &Node, source: &str, open_tag: &mut Option<String>, held: &mut Vec<usize>, out: &mut Vec<Found>) {
    for child in node.children().into_iter().flatten() {
      match child {
        Node::Html(html) => {
          if let Some(tag) = lazy_regex::regex_captures!(r"^<(/?[^!>/\s][^/\s>]*)", &html.value).map(|(_, t)| t.to_lowercase()) {
            match tag.strip_prefix('/') {
              Some(closing) if open_tag.as_deref() == Some(closing) => {
                held.clear();
                *open_tag = None;
              }
              Some(_) => {}
              None if open_tag.is_none() => *open_tag = Some(tag),
              None => {}
            }
          }
        }
        Node::Link(link) => {
          let written = &source[start(child)..end(child)];
          if link.url == written || link.url == format!("http://{written}") || link.url == format!("mailto:{written}") {
            match open_tag {
              Some(_) => held.push(start(child)),
              None => out.push(Found { rule: "markdown/no-bare-urls", start: start(child), message: "Unexpected bare URL. Use autolink (<URL>) or link ([text](URL)) instead.".to_owned() }),
            }
          }
        }
        other => block(other, source, open_tag, held, out),
      }
    }
  }
  for (node, _) in nodes(root) {
    if matches!(node, Node::Heading(_) | Node::Paragraph(_) | Node::TableCell(_)) {
      let (mut open_tag, mut held) = (None, Vec::new());
      block(node, source, &mut open_tag, &mut held, out);
      out.extend(held.into_iter().map(|at| Found { rule: "markdown/no-bare-urls", start: at, message: "Unexpected bare URL. Use autolink (<URL>) or link ([text](URL)) instead.".to_owned() }));
    }
  }
}

fn duplicate_headings(all: &[(&Node, Option<&Node>)], siblings_only: bool, out: &mut Vec<Found>) {
  fn key(node: &Node, into: &mut Vec<String>, shown: &mut String) {
    for child in node.children().into_iter().flatten() {
      let (kind, value) = match child {
        Node::Text(t) => ("text", Some(t.value.as_str())),
        Node::InlineCode(c) => ("inlineCode", Some(c.value.as_str())),
        Node::Html(h) => ("html", Some(h.value.as_str())),
        Node::InlineMath(m) => ("inlineMath", Some(m.value.as_str())),
        Node::Emphasis(_) => ("emphasis", None),
        Node::Strong(_) => ("strong", None),
        Node::Delete(_) => ("delete", None),
        Node::Link(_) => ("link", None),
        Node::Image(_) => ("image", None),
        Node::Break(_) => ("break", None),
        _ => ("other", None),
      };
      into.push(match value {
        Some(v) if !v.is_empty() => format!("{kind}:{v}"),
        _ => kind.to_owned(),
      });
      if let Some(v) = value.filter(|_| kind != "html") {
        shown.push_str(v);
      }
      key(child, into, shown);
    }
  }
  let mut levels: Vec<HashSet<Vec<String>>> = vec![HashSet::new(); 7];
  let mut last = 1usize;
  for (node, _) in all {
    let Node::Heading(heading) = node else { continue };
    let depth = heading.depth as usize;
    let (mut parts, mut shown) = (Vec::new(), String::new());
    key(node, &mut parts, &mut shown);
    let set = match siblings_only {
      true => {
        if depth < last {
          for level in levels.iter_mut().take(last + 1).skip(depth + 1) {
            level.clear();
          }
        }
        last = depth;
        &mut levels[depth]
      }
      false => &mut levels[0],
    };
    if !set.insert(parts) {
      out.push(Found { rule: "markdown/no-duplicate-headings", start: start(node), message: format!("Duplicate heading \"{shown}\" found.") });
    }
  }
}

/// `text` with every `<!-- -->` comment blanked to spaces, its line endings kept.
fn strip_html_comments(text: &str) -> String {
  lazy_regex::regex!(r"(?s)<!--.*?-->").replace_all(text, |c: &lazy_regex::Captures| c[0].chars().map(|ch| if ch == '\n' || ch == '\r' { ch.to_string() } else { " ".repeat(ch.len_utf8()) }).collect::<String>()).into_owned()
}
