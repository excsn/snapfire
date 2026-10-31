//! The front end for Markdown and MDX: markdown-rs parses, the rules here walk its tree. The rules are
//! those of `@eslint/markdown`'s recommended set that concern correctness, under its names.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use markdown::mdast::Node;

use crate::{Level, RuleInfo};

/// A finding by byte offset.
pub(crate) struct Found {
  pub rule: &'static str,
  pub start: usize,
  pub message: String,
}

macro_rules! rule {
  ($id:literal, $what:literal, $why:literal) => {
    RuleInfo { id: $id, default: Level::Error, recommended: true, category: "correctness", fixable: false, docs: concat!("### What it does\n\n", $what, "\n\n### Why is this bad?\n\n", $why, "\n") }
  };
}

pub(crate) const RULES: [RuleInfo; 11] = [
  rule!("markdown/fenced-code-language", "Requires a language on a fenced code block.", "The language picks the highlighting and tells a reader what the block is."),
  rule!("markdown/heading-increment", "Requires each heading to be at most one level deeper than the one before it. A `title` in the front matter counts as the first heading; `frontmatterTitle` names another key by a regular expression over its line. With `\"\"` none counts.", "A skipped level breaks the outline that a table of contents and a screen reader read."),
  rule!("markdown/no-duplicate-definitions", "Disallows two link definitions with the same label.", "Only the first is used; the second is dead and usually a mistake."),
  rule!("markdown/no-empty-definitions", "Disallows a link definition with no URL or only `#`.", "A link through it goes nowhere."),
  rule!("markdown/no-empty-images", "Disallows an image with no URL.", "It renders as a broken image."),
  rule!("markdown/no-empty-links", "Disallows a link with no URL or only `#`.", "It goes nowhere."),
  rule!("markdown/no-missing-atx-heading-space", "Requires a space after the `#` of a heading.", "Without it the line is a paragraph that starts with `#`, not a heading."),
  rule!("markdown/no-missing-link-fragments", "Requires a `#fragment` link to name a heading of the document.", "The link scrolls nowhere."),
  rule!("markdown/no-multiple-h1", "Disallows more than one top-level heading. A `title` in the front matter counts as one; `frontmatterTitle` names another key by a regular expression over its line. With `\"\"` none counts.", "A document has one title; a second `#` is usually meant as `##`."),
  rule!("markdown/no-reversed-media-syntax", "Disallows `(text)[url]`, the link syntax reversed.", "It renders as the literal text."),
  rule!("markdown/require-alt-text", "Requires alternative text on an image.", "A screen reader has nothing to say for it."),
];

/// For each of the two rules that read it, the front matter line that counts as the first heading, as
/// `frontmatterTitle` sets it: `None` when no line does.
pub(crate) struct Titles {
  pub h1: Option<lazy_regex::Regex>,
  pub heading: Option<lazy_regex::Regex>,
}

/// `@eslint/markdown`'s `frontmatterTitle` default: a `title` key, quoted or not, in YAML or TOML.
const TITLE: &str = r#"^\s*['"]?title['"]?\s*[:=]"#;

impl Titles {
  /// The pattern a rule's options give, its first option's `frontmatterTitle`, else the default.
  pub(crate) fn of(options: impl Fn(&str) -> Option<serde_json::Value>) -> Self {
    let pattern = |rule: &str| {
      let given = options(rule).and_then(|options| options.get(0)?.get("frontmatterTitle")?.as_str().map(str::to_owned));
      match given.as_deref() {
        Some("") => None,
        Some(given) => lazy_regex::Regex::new(given).ok(),
        None => lazy_regex::Regex::new(TITLE).ok(),
      }
    };
    Self { h1: pattern("markdown/no-multiple-h1"), heading: pattern("markdown/heading-increment") }
  }
}

impl Default for Titles {
  fn default() -> Self {
    Self::of(|_| None)
  }
}

/// Whether this front end reads `path`, and as MDX.
pub(crate) fn reads(path: &Path) -> Option<bool> {
  match path.extension().and_then(|e| e.to_str()) {
    Some("md" | "markdown") => Some(false),
    Some("mdx") => Some(true),
    _ => None,
  }
}

fn options(mdx: bool) -> markdown::ParseOptions {
  let mut constructs = match mdx {
    true => markdown::Constructs::mdx(),
    false => markdown::Constructs::default(),
  };
  let gfm = markdown::Constructs::gfm();
  constructs.gfm_autolink_literal = gfm.gfm_autolink_literal;
  constructs.gfm_footnote_definition = gfm.gfm_footnote_definition;
  constructs.gfm_label_start_footnote = gfm.gfm_label_start_footnote;
  constructs.gfm_strikethrough = gfm.gfm_strikethrough;
  constructs.gfm_table = gfm.gfm_table;
  constructs.gfm_task_list_item = gfm.gfm_task_list_item;
  constructs.frontmatter = true;
  markdown::ParseOptions { constructs, ..markdown::ParseOptions::default() }
}

/// The findings in one document. A document that does not parse is one `lint/parse` finding.
pub(crate) fn lint(source: &str, mdx: bool, titles: &Titles) -> Result<Vec<Found>, (usize, String)> {
  let root = markdown::to_mdast(source, &options(mdx)).map_err(|message| {
    let at = match message.place.as_deref() {
      Some(markdown::message::Place::Point(point)) => point.offset,
      Some(markdown::message::Place::Position(position)) => position.start.offset,
      None => 0,
    };
    (at, message.reason)
  })?;
  let mut walk = Walk::default();
  walk.front(&root, titles);
  walk.node(&root, source);
  walk.finish(source);
  let mut found = walk.found;
  found.sort_by_key(|f| f.start);
  Ok(found)
}

#[derive(Default)]
struct Walk {
  found: Vec<Found>,
  titled: bool,
  depth: Option<u8>,
  h1: usize,
  slugs: HashSet<String>,
  fragments: Vec<(usize, String)>,
  definitions: HashMap<String, usize>,
}

fn start(node: &Node) -> usize {
  node.position().map_or(0, |p| p.start.offset)
}

fn text(node: &Node) -> String {
  match node {
    Node::Text(t) => t.value.clone(),
    Node::InlineCode(c) => c.value.clone(),
    other => other.children().map(|c| c.iter().map(text).collect()).unwrap_or_default(),
  }
}

/// GitHub's heading anchor: lowercase, punctuation dropped, spaces to dashes.
fn slug(heading: &str) -> String {
  heading.trim().to_lowercase().chars().filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-' || *c == '_').map(|c| if c == ' ' { '-' } else { c }).collect()
}

impl Walk {
  fn find(&mut self, rule: &'static str, start: usize, message: impl Into<String>) {
    self.found.push(Found { rule, start, message: message.into() });
  }

  /// Counts the front matter's title as the first heading for each rule `titles` says it is one for.
  fn front(&mut self, root: &Node, titles: &Titles) {
    let Some(front) = root.children().and_then(|children| children.first()) else { return };
    let value = match front {
      Node::Yaml(yaml) => &yaml.value,
      Node::Toml(toml) => &toml.value,
      _ => return,
    };
    let names = |pattern: &Option<lazy_regex::Regex>| pattern.as_ref().is_some_and(|pattern| value.lines().any(|line| pattern.is_match(line)));
    if names(&titles.h1) {
      self.titled = true;
      self.h1 = 1;
    }
    if names(&titles.heading) {
      self.depth = Some(1);
    }
  }

  fn node(&mut self, node: &Node, source: &str) {
    match node {
      Node::Heading(heading) => {
        let at = start(node);
        if let Some(previous) = self.depth
          && heading.depth > previous + 1
        {
          self.find("markdown/heading-increment", at, format!("heading level {} follows level {previous}", heading.depth));
        }
        self.depth = Some(heading.depth);
        if heading.depth == 1 {
          self.h1 += 1;
          if self.h1 > 1 {
            let why = if self.titled { "the front matter's `title` is the first" } else { "the document already has one" };
            self.find("markdown/no-multiple-h1", at, format!("a second top-level heading; {why}"));
          }
        }
        self.slugs.insert(slug(&text(node)));
      }
      Node::Link(link) => {
        let at = start(node);
        match link.url.as_str() {
          "" | "#" => self.find("markdown/no-empty-links", at, "the link has no destination"),
          url if url.starts_with('#') => self.fragments.push((at, url[1..].to_owned())),
          _ => {}
        }
      }
      Node::Image(image) => {
        let at = start(node);
        if image.url.is_empty() {
          self.find("markdown/no-empty-images", at, "the image has no URL");
        }
        if image.alt.trim().is_empty() {
          self.find("markdown/require-alt-text", at, "the image has no alternative text");
        }
      }
      Node::Code(code) => {
        let at = start(node);
        let fenced = source.get(at..).is_some_and(|s| s.starts_with("```") || s.starts_with("~~~"));
        if fenced && code.lang.is_none() {
          self.find("markdown/fenced-code-language", at, "the code block names no language");
        }
      }
      Node::Definition(definition) => {
        let at = start(node);
        if definition.url.is_empty() || definition.url == "#" {
          self.find("markdown/no-empty-definitions", at, format!("`[{}]` has no URL", definition.identifier));
        }
        let label = definition.identifier.to_lowercase();
        if self.definitions.contains_key(&label) {
          self.find("markdown/no-duplicate-definitions", at, format!("`[{}]` is defined twice", definition.identifier));
        } else {
          self.definitions.insert(label, at);
        }
      }
      Node::Paragraph(_) => {
        let at = start(node);
        let end = node.position().map_or(at, |p| p.end.offset);
        let written = &source[at..end];
        let mut line_start = at;
        for line in written.split_inclusive('\n') {
          let hashes = line.len() - line.trim_start_matches('#').len();
          if (1..=6).contains(&hashes) && line[hashes..].starts_with(|c: char| !c.is_whitespace()) {
            self.find("markdown/no-missing-atx-heading-space", line_start, "a `#` with no space after it is not a heading");
          }
          line_start += line.len();
        }
        if let Some(reversed) = reversed(written) {
          self.find("markdown/no-reversed-media-syntax", at + reversed, "`(text)[url]` is a link written backwards: `[text](url)`");
        }
      }
      _ => {}
    }
    for child in node.children().into_iter().flatten() {
      self.node(child, source);
    }
  }

  fn finish(&mut self, source: &str) {
    let ids: HashSet<String> = source.match_indices("id=\"").filter_map(|(i, m)| source[i + m.len()..].split('"').next().map(str::to_owned)).collect();
    for (at, fragment) in std::mem::take(&mut self.fragments) {
      if !self.slugs.contains(&fragment.to_lowercase()) && !ids.contains(&fragment) {
        self.find("markdown/no-missing-link-fragments", at, format!("no heading here makes `#{fragment}`"));
      }
    }
  }
}

/// The offset of `(text)[url]` in a paragraph's source.
fn reversed(written: &str) -> Option<usize> {
  let mut from = 0;
  while let Some(open) = written[from..].find('(').map(|i| from + i) {
    if let Some(close) = written[open..].find(")[").map(|i| open + i)
      && !written[open + 1..close].contains(['(', ')', '\n'])
      && written[close + 2..].find(']').is_some_and(|end| end > 0 && !written[close + 2..close + 2 + end].contains([' ', '\n']))
    {
      return Some(open);
    }
    from = open + 1;
  }
  None
}

/// The suppression comments of a document: `<!-- -->`, and `{/* */}` in MDX.
pub(crate) fn comments(source: &str, mdx: bool) -> Vec<(String, usize, usize)> {
  let mut out = crate::templates::comments(source);
  if mdx {
    let mut from = 0;
    while let Some(start) = source[from..].find("{/*").map(|i| from + i) {
      let Some(end) = source[start..].find("*/}").map(|i| start + i + 3) else { break };
      out.push((source[start + 1..end - 1].to_owned(), start, end));
      from = end;
    }
  }
  out
}
