//! Code highlighted at build time: syntect over bat's grammars, written as
//! nested spans whose classes are the TextMate scope atoms, `hl-` prefixed.
//! No colours are written; a stylesheet from [`theme_css`] supplies them.

use std::sync::OnceLock;

use syntect::html::ClassStyle;
use syntect::parsing::{BasicScopeStackOp, ParseState, Scope, ScopeStack, SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;

/// The prefix on every class the highlighter writes.
pub const PREFIX: &str = "hl-";

/// The class on a highlighted `<pre>`, which a theme's stylesheet gives its background.
pub const CODE_CLASS: &str = "hl-code";

/// One piece of a highlighted line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Piece {
  Text(String),
  Span { classes: String, children: Vec<Piece> },
}

fn syntaxes() -> &'static SyntaxSet {
  static SET: OnceLock<SyntaxSet> = OnceLock::new();
  SET.get_or_init(two_face::syntax::extra_newlines)
}

fn syntax(lang: &str) -> Option<&'static SyntaxReference> {
  let set = syntaxes();
  let lower = lang.to_ascii_lowercase();
  match lower.as_str() {
    "jsx" => set.find_syntax_by_extension("tsx"),
    "text" | "txt" | "plain" | "plaintext" => None,
    other => set.find_syntax_by_token(other),
  }
}

/// Whether a fence's language names a grammar.
pub fn knows(lang: &str) -> bool {
  syntax(lang).is_some()
}

/// `code` as highlighted lines, each its own tree of spans with every scope
/// open at its start reopened, the newline left out. `None` when `lang` names
/// no grammar.
pub fn lines(code: &str, lang: &str) -> Option<Vec<Vec<Piece>>> {
  let syntax = syntax(lang)?;
  let set = syntaxes();
  let mut state = ParseState::new(syntax);
  let mut stack = ScopeStack::new();
  let mut out = Vec::new();
  for line in LinesWithEndings::from(code) {
    let ops = state.parse_line(line, set).ok()?;
    let text = line.strip_suffix('\n').unwrap_or(line);
    let text = text.strip_suffix('\r').unwrap_or(text);
    let mut frames: Vec<(String, Vec<Piece>)> = vec![(String::new(), Vec::new())];
    for scope in stack.as_slice() {
      frames.push((classes(*scope), Vec::new()));
    }
    let mut last = 0;
    for (at, op) in ops {
      let at = at.min(text.len());
      if at > last {
        push_text(&mut frames, &text[last..at]);
        last = at;
      }
      stack
        .apply_with_hook(&op, |basic, _| match basic {
          BasicScopeStackOp::Push(scope) => frames.push((classes(scope), Vec::new())),
          BasicScopeStackOp::Pop => close(&mut frames),
        })
        .ok()?;
    }
    if last < text.len() {
      push_text(&mut frames, &text[last..]);
    }
    while frames.len() > 1 {
      close(&mut frames);
    }
    out.push(frames.pop().map(|(_, pieces)| pieces).unwrap_or_default());
  }
  Some(out)
}

fn push_text(frames: &mut [(String, Vec<Piece>)], text: &str) {
  let Some((_, children)) = frames.last_mut() else { return };
  match children.last_mut() {
    Some(Piece::Text(held)) => held.push_str(text),
    _ => children.push(Piece::Text(text.to_owned())),
  }
}

fn close(frames: &mut Vec<(String, Vec<Piece>)>) {
  if frames.len() < 2 {
    return;
  }
  let (classes, children) = frames.pop().expect("two frames");
  if !children.is_empty() {
    frames.last_mut().expect("a parent").1.push(Piece::Span { classes, children });
  }
}

fn classes(scope: Scope) -> String {
  scope.build_string().split('.').map(|atom| format!("{PREFIX}{atom}")).collect::<Vec<_>>().join(" ")
}

/// The theme names [`theme_css`] reads.
pub fn themes() -> Vec<&'static str> {
  use two_face::theme::EmbeddedThemeName as N;
  [N::Github, N::InspiredGithub, N::OneHalfLight, N::OneHalfDark, N::Nord, N::Dracula, N::SolarizedLight, N::SolarizedDark, N::GruvboxLight, N::GruvboxDark, N::CatppuccinLatte, N::CatppuccinMocha, N::MonokaiExtended, N::TwoDark, N::Zenburn, N::Base16OceanLight, N::Base16OceanDark]
    .into_iter()
    .map(|n| n.as_name())
    .collect()
}

/// The stylesheet that colours highlighted code in theme `name`, every rule's
/// selectors prefixed with `scope` and a space when `scope` is not empty.
/// `None` when no theme has that name.
pub fn theme_css(name: &str, scope: &str) -> Option<String> {
  let set = two_face::theme::LazyThemeSet::from(two_face::theme::extra());
  let theme = set.get(name)?;
  let css = syntect::html::css_for_theme_with_class_style(theme, ClassStyle::SpacedPrefixed { prefix: PREFIX }).ok()?;
  if scope.is_empty() {
    return Some(css);
  }
  let mut out = String::new();
  for line in css.lines() {
    match line.strip_suffix(" {") {
      Some(selectors) if !line.starts_with(' ') => {
        let scoped: Vec<String> = selectors.split(", ").map(|s| format!("{scope} {s}")).collect();
        out.push_str(&scoped.join(", "));
        out.push_str(" {\n");
      }
      _ => {
        out.push_str(line);
        out.push('\n');
      }
    }
  }
  Some(out)
}

#[cfg(test)]
mod tests {
  use super::*;

  fn flat(pieces: &[Piece]) -> String {
    pieces
      .iter()
      .map(|p| match p {
        Piece::Text(t) => t.clone(),
        Piece::Span { children, .. } => flat(children),
      })
      .collect()
  }

  #[test]
  fn a_line_keeps_its_text_inside_scoped_spans() {
    let lines = lines("const a: number = 1;\nlet b = \"two\";\n", "ts").expect("typescript");
    assert_eq!(lines.len(), 2);
    assert_eq!(flat(&lines[0]), "const a: number = 1;");
    assert_eq!(flat(&lines[1]), "let b = \"two\";");
    let Piece::Span { classes, .. } = &lines[0][0] else { panic!("a span first") };
    assert!(classes.starts_with("hl-source hl-ts"), "{classes}");
  }

  #[test]
  fn tsx_jsx_and_toml_have_grammars() {
    for lang in ["ts", "tsx", "typescript", "jsx", "js", "rust", "rs", "toml", "json", "sh", "bash", "css", "html", "yaml"] {
      assert!(knows(lang), "{lang}");
    }
    assert!(!knows("text"));
    assert!(!knows("nonsense-language"));
  }

  #[test]
  fn a_comment_spanning_lines_reopens_on_the_second() {
    let lines = lines("/* one\ntwo */ x\n", "js").expect("javascript");
    let second = &lines[1];
    let Piece::Span { children, .. } = &second[0] else { panic!("source span") };
    let Piece::Span { classes, .. } = &children[0] else { panic!("comment span") };
    assert!(classes.contains("hl-comment"), "{classes}");
  }

  #[test]
  fn a_theme_css_is_scoped() {
    let css = theme_css("GitHub", "[data-sf-scheme=\"dark\"]").expect("a theme");
    assert!(css.contains("[data-sf-scheme=\"dark\"] .hl-code {"), "{css}");
    assert!(css.contains("[data-sf-scheme=\"dark\"] .hl-comment"), "{css}");
    assert!(theme_css("No Such Theme", "").is_none());
    assert!(themes().contains(&"OneHalfDark"));
  }
}
