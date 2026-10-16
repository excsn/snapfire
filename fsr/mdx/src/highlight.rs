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
#[non_exhaustive]
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

/// The fence names a terminal session reads as, GitHub's `ShellSession` among them.
pub const SESSIONS: &[&str] = &["shell-session", "console", "sh-session", "shellsession"];

fn is_session(lang: &str) -> bool {
  SESSIONS.contains(&lang.to_ascii_lowercase().as_str())
}

/// Whether a fence's language names a grammar.
pub fn knows(lang: &str) -> bool {
  is_session(lang) || syntax(lang).is_some()
}

/// `code` as highlighted lines, each its own tree of spans with every scope
/// open at its start reopened, the newline left out. `None` when `lang` names
/// no grammar.
pub fn lines(code: &str, lang: &str) -> Option<Vec<Vec<Piece>>> {
  if is_session(lang) {
    return session(code);
  }
  let syntax = syntax(lang)?;
  let mut state = ParseState::new(syntax);
  let mut stack = ScopeStack::new();
  LinesWithEndings::from(code).map(|line| highlight_line(&mut state, &mut stack, line)).collect()
}

/// One line through a grammar whose state carries over from the line before.
fn highlight_line(state: &mut ParseState, stack: &mut ScopeStack, line: &str) -> Option<Vec<Piece>> {
  let ops = state.parse_line(line, syntaxes()).ok()?;
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
  Some(frames.pop().map(|(_, pieces)| pieces).unwrap_or_default())
}

/// The bytes of a line's prompt, the space after it included: an optional
/// `(env)` and one of `$ # % > ❯ ➜` followed by a space or the line's end, as
/// GitHub's `ShellSession` grammar reads them.
fn prompt_of(line: &str) -> Option<usize> {
  let mut at = 0;
  if let Some(rest) = line.strip_prefix('(') {
    let close = rest.find(')')?;
    at = 1 + close + 1;
    at += line[at..].len() - line[at..].trim_start_matches(' ').len();
  }
  let mark = line[at..].chars().next()?;
  if !matches!(mark, '$' | '#' | '%' | '>' | '❯' | '➜') {
    return None;
  }
  let after = at + mark.len_utf8();
  match line[after..].chars().next() {
    None => Some(after),
    Some(' ') => Some(after + 1),
    Some(_) => None,
  }
}

/// A terminal session: each line after a prompt is a command through the Bash
/// grammar, a command ending in `\\` going on to the next line, and every other
/// line output.
fn session(code: &str) -> Option<Vec<Vec<Piece>>> {
  let bash = syntaxes().find_syntax_by_token("bash")?;
  let mut out = Vec::new();
  let mut command: Option<(ParseState, ScopeStack)> = None;
  for line in LinesWithEndings::from(code) {
    let text = line.trim_end_matches(['\n', '\r']);
    let continued = command.is_some();
    let prompt = if continued { Some(0) } else { prompt_of(text) };
    let Some(prompt) = prompt else {
      out.push(match text.is_empty() {
        true => Vec::new(),
        false => vec![Piece::Span { classes: format!("{PREFIX}meta {PREFIX}output"), children: vec![Piece::Text(text.to_owned())] }],
      });
      continue;
    };
    let (state, stack) = command.get_or_insert_with(|| (ParseState::new(bash), ScopeStack::new()));
    let mut children = Vec::new();
    if prompt > 0 {
      let mark = text[..prompt].trim_end();
      let (env, sign) = mark.split_at(mark.len() - mark.chars().last().map(char::len_utf8).unwrap_or(0));
      if !env.is_empty() {
        children.push(Piece::Text(env.to_owned()));
      }
      children.push(Piece::Span { classes: format!("{PREFIX}punctuation {PREFIX}definition {PREFIX}prompt"), children: vec![Piece::Text(sign.to_owned())] });
      children.push(Piece::Text(text[mark.len()..prompt].to_owned()));
    }
    children.extend(highlight_line(state, stack, &line[prompt..])?);
    out.push(vec![Piece::Span { classes: format!("{PREFIX}meta {PREFIX}prompt"), children }]);
    if !text.trim_end().ends_with('\\') {
      command = None;
    }
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
  fn a_terminal_session_scopes_prompts_commands_and_output_apart() {
    let lines = lines("$ cargo build --release\n   Compiling fsr\n(venv) % echo \"one\" \\\n  two\n\n# rm -rf target\n", "shell-session").expect("a session");
    assert_eq!(lines.len(), 6);
    let Piece::Span { classes, children } = &lines[0][0] else { panic!("a prompt span") };
    assert_eq!(classes, "hl-meta hl-prompt");
    assert_eq!(children[0], Piece::Span { classes: "hl-punctuation hl-definition hl-prompt".into(), children: vec![Piece::Text("$".into())] });
    assert_eq!(children[1], Piece::Text(" ".into()));
    assert!(matches!(&children[2], Piece::Span { classes, .. } if classes.starts_with("hl-source hl-shell")), "the command through the Bash grammar: {:?}", children[2]);
    assert_eq!(flat(&lines[0]), "$ cargo build --release");
    assert_eq!(lines[1], vec![Piece::Span { classes: "hl-meta hl-output".into(), children: vec![Piece::Text("   Compiling fsr".into())] }]);
    assert_eq!(flat(&lines[2]), "(venv) % echo \"one\" \\");
    let Piece::Span { classes, .. } = &lines[3][0] else { panic!("a continuation") };
    assert_eq!(classes, "hl-meta hl-prompt", "a command ending in a backslash goes on to the next line");
    assert!(lines[4].is_empty());
    assert_eq!(flat(&lines[5]), "# rm -rf target");
    for lang in ["console", "shellsession", "sh-session", "Shell-Session"] {
      assert!(knows(lang), "{lang}");
    }
    assert_eq!(super::lines("$notaprompt\n", "console").expect("a session")[0], vec![Piece::Span { classes: "hl-meta hl-output".into(), children: vec![Piece::Text("$notaprompt".into())] }], "a prompt is followed by a space");
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
