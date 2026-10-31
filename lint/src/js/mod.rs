//! The front end for JavaScript and TypeScript: oxc parses, the rules ported from oxlint run.

use std::path::Path;

use oxc_allocator::Allocator;
use oxc_parser::{ParseOptions, Parser};
use oxc_semantic::SemanticBuilder;
use oxc_span::SourceType;

mod aria;
mod ast_util;
mod context;
mod fixer;
pub(crate) mod frameworks;
mod globals;
mod loader;
mod module_record;
mod rule;
mod rules;
mod settings;
#[cfg(test)]
mod tester;
mod utils;

pub(crate) use context::LintContext;
pub(crate) use fixer::FixKind;
pub(crate) use module_record::ModuleRecord;
pub(crate) use oxc_semantic::AstNode;
pub(crate) use rules::RULES;
pub(crate) use settings::OxlintSettings;

use rule::{Rule, RuleMeta};

/// Implements [`RuleMeta`] for a rule: its doc comments become its documentation.
macro_rules! declare_rule {
  ($(#[doc = $doc:expr])* $rule:ident, $plugin:literal, $name:literal, $category:ident, recommended = $recommended:literal, fix = $fix:expr $(,)?) => {
    #[allow(unused_imports)]
    use $crate::js::{fixer::FixKind, rule::{RuleCategory, RuleFixMeta, RuleMeta}};
    #[allow(unused_imports)]
    use oxc_semantic::AstTypesBitset;

    impl $crate::js::rule::RuleMeta for $rule {
      const ID: &'static str = concat!($plugin, "/", $name);
      #[cfg(test)]
      const NAME: &'static str = $name;
      #[cfg(test)]
      const PLUGIN: &'static str = $plugin;
      const CATEGORY: $crate::js::rule::RuleCategory = $crate::js::rule::RuleCategory::$category;
      const RECOMMENDED: bool = $recommended;
      const FIX: $crate::js::rule::RuleFixMeta = $fix;
      const DOCS: &'static str = concat!($($doc, "\n"),*);
    }
  };
  ($rule:ident, $plugin:literal, $name:literal, $category:ident, recommended = $recommended:literal, fix = $fix:expr, docs = $docs:path $(,)?) => {
    #[allow(unused_imports)]
    use $crate::js::{fixer::FixKind, rule::{RuleCategory, RuleFixMeta, RuleMeta}};
    #[allow(unused_imports)]
    use oxc_semantic::AstTypesBitset;

    impl $crate::js::rule::RuleMeta for $rule {
      const ID: &'static str = concat!($plugin, "/", $name);
      #[cfg(test)]
      const NAME: &'static str = $name;
      #[cfg(test)]
      const PLUGIN: &'static str = $plugin;
      const CATEGORY: $crate::js::rule::RuleCategory = $crate::js::rule::RuleCategory::$category;
      const RECOMMENDED: bool = $recommended;
      const FIX: $crate::js::rule::RuleFixMeta = $fix;
      const DOCS: &'static str = $docs;
    }
  };
}
pub(crate) use declare_rule;

/// A rule with its options read, ready to run on any file.
pub(crate) trait Run: Send + Sync {
  fn should_run(&self, host: &context::ContextHost) -> bool;
  fn run_once(&self, ctx: &LintContext);
  fn run<'a>(&self, node: &AstNode<'a>, ctx: &LintContext<'a>);
}

impl<T: Rule + Send + Sync> Run for T {
  fn should_run(&self, host: &context::ContextHost) -> bool {
    Rule::should_run(self, host)
  }

  fn run_once(&self, ctx: &LintContext) {
    Rule::run_once(self, ctx);
  }

  fn run<'a>(&self, node: &AstNode<'a>, ctx: &LintContext<'a>) {
    Rule::run(self, node, ctx);
  }
}

/// One rule in the registry.
pub(crate) struct Entry {
  /// `eslint/no-debugger`.
  pub id: &'static str,
  pub docs: &'static str,
  pub recommended: bool,
  /// `correctness`, `suspicious`, `pedantic` and the rest of clippy's categories.
  pub category: &'static str,
  /// Whether the rule offers a fix or a suggestion for some of what it finds.
  pub fixable: bool,
  /// The rule with its ESLint options, or its defaults when there are none.
  pub build: fn(Option<serde_json::Value>) -> Result<Box<dyn Run>, serde_json::Error>,
}

pub(crate) const fn entry<T: Rule + RuleMeta + Send + Sync + 'static>() -> Entry {
  Entry { id: T::ID, docs: T::DOCS, recommended: T::RECOMMENDED, category: T::CATEGORY.as_str(), fixable: matches!(T::FIX, rule::RuleFixMeta::Fixable(_) | rule::RuleFixMeta::Conditional(_)), build: build::<T> }
}

fn build<T: Rule + Send + Sync + 'static>(options: Option<serde_json::Value>) -> Result<Box<dyn Run>, serde_json::Error> {
  match options {
    None => Ok(Box::new(T::default())),
    Some(options) => T::from_configuration(options).map(|rule| Box::new(rule) as Box<dyn Run>),
  }
}

/// What a file's rules see besides its source: ESLint's `globals`, `env` and `settings`.
#[derive(Debug, Clone)]
pub(crate) struct Setup {
  pub globals: globals::OxlintGlobals,
  pub env: Vec<String>,
  pub settings: OxlintSettings,
  /// Whether the block is the file's first.
  pub first: bool,
}

impl Default for Setup {
  fn default() -> Self {
    Self { globals: globals::OxlintGlobals::default(), env: Vec::new(), settings: OxlintSettings::default(), first: true }
  }
}

/// A comment as written, delimiters included, with its byte range.
pub(crate) struct Comment {
  pub text: String,
  pub start: usize,
  pub end: usize,
}

/// What one rule reported, by byte range.
pub(crate) struct Reported {
  pub rule: &'static str,
  pub start: usize,
  pub end: usize,
  pub message: String,
  pub help: Option<String>,
}

pub(crate) struct Outcome {
  pub reported: Vec<Reported>,
  pub comments: Vec<Comment>,
}

impl Outcome {
  /// Takes in `part`, found in a block starting at byte `start` of the file.
  fn extend(&mut self, part: Outcome, start: usize) {
    self.reported.extend(part.reported.into_iter().map(|r| Reported { start: r.start + start, end: r.end + start, ..r }));
    self.comments.extend(part.comments.into_iter().map(|c| Comment { start: c.start + start, end: c.end + start, ..c }));
  }
}

/// Whether this front end reads `path`.
pub(crate) fn reads(path: &Path) -> bool {
  path.extension().and_then(|e| e.to_str()).is_some_and(|ext| matches!(ext, "js" | "mjs" | "cjs" | "jsx" | "ts" | "mts" | "cts" | "tsx") || loader::LINT_PARTIAL_LOADER_EXTENSIONS.contains(&ext))
}

/// The file's source type, with JSX on for JavaScript as oxlint does, since a `.js` file often holds JSX.
fn source_type(path: &Path) -> SourceType {
  let source_type = SourceType::from_path(path).unwrap_or_default();
  match source_type.is_javascript() {
    true => source_type.with_jsx(true),
    false => source_type,
  }
}

/// The findings of `rules` with their safe fixes, each with its rule, and the file's comments.
pub(crate) fn fixes(path: &Path, source: &str, rules: &[(&'static str, &dyn Run)], settings: &OxlintSettings) -> (Vec<Comment>, Vec<(&'static str, usize, fixer::Message)>) {
  let setup = Setup { globals: globals::OxlintGlobals::default(), env: context::ENV.iter().map(ToString::to_string).collect(), settings: settings.clone(), first: true };
  let (outcome, messages) = lint_with(path, source, rules, &[], FixKind::SafeFix, setup, true);
  (outcome.comments, messages.into_iter().map(|(rule, m)| (rule, m.span.start as usize, m)).collect())
}

/// `source` with the fixes `messages` carry applied, and how many were.
pub(crate) fn apply(path: &Path, source: &str, messages: Vec<fixer::Message>) -> (String, usize) {
  let carrying = |m: &fixer::Message| !m.fixes.is_empty();
  let wanted = messages.iter().filter(|m| carrying(m)).count();
  let result = fixer::Fixer::new(source, messages, SourceType::from_path(path).ok()).fix();
  if !result.fixed {
    return (source.to_owned(), 0);
  }
  let left = result.messages.iter().filter(|m| carrying(m)).count();
  (result.fixed_code.into_owned(), wanted - left)
}

/// ESLint's `settings` as the rules read them.
pub(crate) fn settings(json: &serde_json::Map<String, serde_json::Value>) -> Result<OxlintSettings, serde_json::Error> {
  serde_json::from_value(serde_json::Value::Object(json.clone()))
}

/// Run `rules` on one file. A parse error is reported under `lint/parse` and stops the rules.
pub(crate) fn lint(path: &Path, source: &str, rules: &[(&'static str, &dyn Run)], checks: &[(&'static str, &dyn crate::Check)], settings: &OxlintSettings) -> Outcome {
  lint_with(path, source, rules, checks, FixKind::None, Setup { globals: globals::OxlintGlobals::default(), env: context::ENV.iter().map(ToString::to_string).collect(), settings: settings.clone(), first: true }, true).0
}

fn lint_with(path: &Path, source: &str, rules: &[(&'static str, &dyn Run)], checks: &[(&'static str, &dyn crate::Check)], fix: FixKind, setup: Setup, check_syntax: bool) -> (Outcome, Vec<(&'static str, fixer::Message)>) {
  let ext = path.extension().and_then(|e| e.to_str()).unwrap_or_default();
  let sections = loader::PartialLoader::parse(ext, source).unwrap_or_else(|| vec![loader::JavaScriptSource::new(source, source_type(path))]);
  let allocators: Vec<Allocator> = sections.iter().map(|_| Allocator::default()).collect();
  let mut outcome = Outcome { reported: Vec::new(), comments: Vec::new() };
  let mut built: Vec<(usize, &str, std::rc::Rc<context::ContextSubHost<'_>>)> = Vec::new();
  for (index, section) in sections.iter().enumerate() {
    let start = section.start as usize;
    match analyse(path, section, &allocators[index], check_syntax) {
      Analysed::Refused(part) => outcome.extend(part, start),
      Analysed::Built(comments, sub) => {
        outcome.comments.extend(comments.into_iter().map(|c| Comment { start: c.start + start, end: c.end + start, ..c }));
        built.push((index, section.source_text, std::rc::Rc::new(sub)));
      }
    }
  }
  let mut messages = Vec::new();
  for (index, text, sub) in &built {
    let start = sections[*index].start as usize;
    let others = built.iter().filter(|(other, _, _)| other != index).map(|(_, _, sub)| std::rc::Rc::clone(sub)).collect();
    let host = std::rc::Rc::new(context::ContextHost::new(std::rc::Rc::clone(sub), others, &allocators[*index], path, fix, Setup { first: *index == 0, ..setup.clone() }));
    for (id, rule) in rules {
      if !rule.should_run(&host) {
        continue;
      }
      let ctx = host.spawn(id);
      rule.run_once(&ctx);
      for node in ctx.nodes().iter() {
        rule.run(node, &ctx);
      }
    }
    let mut found = host.take_messages();
    let mut reported: Vec<Reported> = found
      .iter()
      .map(|(rule, message)| Reported { rule, start: message.span.start as usize, end: message.span.end as usize, message: message.error.message.to_string(), help: message.error.help.as_ref().map(ToString::to_string) })
      .collect();
    let file = crate::File { path, source: text, semantic: sub.semantic() };
    for (id, check) in checks {
      check.check(&file, &mut |start, message| reported.push(Reported { rule: id, start, end: start, message, help: None }));
    }
    outcome.extend(Outcome { reported, comments: Vec::new() }, start);
    if start != 0 {
      for (_, message) in &mut found {
        message.move_offset(start as u32);
      }
    }
    messages.extend(found);
  }
  (outcome, messages)
}

/// One script block parsed and analysed, or refused with its `lint/parse` findings.
enum Analysed<'a> {
  Refused(Outcome),
  Built(Vec<Comment>, context::ContextSubHost<'a>),
}

fn analyse<'a>(path: &Path, section: &loader::JavaScriptSource<'a>, allocator: &'a Allocator, check_syntax: bool) -> Analysed<'a> {
  let source = section.source_text;
  let parsed = Parser::new(allocator, source, section.source_type).with_options(ParseOptions { parse_regular_expression: true, allow_return_outside_function: true, ..ParseOptions::default() }).parse();
  let comments: Vec<Comment> = parsed.program.comments.iter().map(|c| Comment { text: c.span.source_text(source).to_owned(), start: c.span.start as usize, end: c.span.end as usize }).collect();
  let parse_error = |diagnostics: &[oxc_diagnostics::OxcDiagnostic], comments: Vec<Comment>| {
    let reported = diagnostics
      .iter()
      .map(|d| {
        let (start, end) = d.labels.first().map_or((0, 0), |l| (l.offset() as usize, (l.offset() + l.len()) as usize));
        Reported { rule: "lint/parse", start, end, message: d.message.to_string(), help: d.help.as_ref().map(ToString::to_string) }
      })
      .collect();
    Analysed::Refused(Outcome { reported, comments })
  };
  if !parsed.diagnostics.is_empty() || parsed.fatal_error {
    return parse_error(&parsed.diagnostics, comments);
  }
  let program = allocator.alloc(parsed.program);
  let built = SemanticBuilder::new_linter().with_check_syntax_error(check_syntax).build(program);
  if !built.diagnostics.is_empty() {
    return parse_error(&built.diagnostics, comments);
  }
  let mut semantic = built.semantic;
  semantic.set_irregular_whitespaces(parsed.irregular_whitespaces);
  let module_record = ModuleRecord::new(path, &parsed.module_record, &semantic);
  Analysed::Built(comments, context::ContextSubHost::new(semantic, module_record, section.framework_options))
}

