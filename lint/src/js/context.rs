// Derived from oxc_linter's context/mod.rs and context/host.rs at oxc crates_v0.155.0, MIT License, Copyright (c) 2024-present VoidZero Inc. & Contributors.

use std::{cell::RefCell, ffi::OsStr, ops::Deref, path::Path, rc::Rc};

use javascript_globals::{GLOBALS, GLOBALS_BUILTIN, GLOBALS_ES2026};
use oxc_ast::ast::IdentifierReference;
use oxc_cfg::ControlFlowGraph;
use oxc_diagnostics::OxcDiagnostic;
use oxc_semantic::{IsGlobalReference, Semantic};
use oxc_span::Span;

use super::{
  fixer::{Fix, FixKind, Message, PossibleFixes, RuleFix, RuleFixer},
  globals::{GlobalValue, OxlintGlobals},
  module_record::ModuleRecord,
};

/// The environments a real run's files see: an FSR page runs in a browser.
pub const ENV: [&str; 2] = ["browser", "es2026"];

/// What every rule reads about one file.
pub struct ContextHost<'a> {
  semantic: Semantic<'a>,
  module_record: ModuleRecord,
  file_path: Box<Path>,
  globals: OxlintGlobals,
  /// The environments whose globals the file sees beside the builtins, as ESLint's `env` names them.
  env: Vec<String>,
  fix: FixKind,
  messages: RefCell<Vec<(&'static str, Message)>>,
}

impl<'a> ContextHost<'a> {
  /// # Panics
  /// If `semantic` was built without a control flow graph.
  pub fn new(semantic: Semantic<'a>, module_record: ModuleRecord, file_path: &Path, fix: FixKind, globals: OxlintGlobals, env: Vec<String>) -> Self {
    assert!(semantic.cfg().is_some(), "the linter reads `Semantic::cfg`; build it with `SemanticBuilder::with_cfg`");
    Self { semantic, module_record, file_path: file_path.into(), globals, env, fix, messages: RefCell::default() }
  }

  /// A context reporting under `rule`, the rule's full id.
  pub fn spawn(self: &Rc<Self>, rule: &'static str) -> LintContext<'a> {
    LintContext { host: Rc::clone(self), rule }
  }

  pub fn semantic(&self) -> &Semantic<'a> {
    &self.semantic
  }

  pub fn source_type(&self) -> &oxc_span::SourceType {
    self.semantic.source_type()
  }

  /// The extension of the file being linted, without its dot.
  pub fn file_extension(&self) -> Option<&OsStr> {
    self.file_path.extension()
  }

  /// Every rule's messages, each with the rule that reported it.
  pub fn take_messages(&self) -> Vec<(&'static str, Message)> {
    std::mem::take(&mut self.messages.borrow_mut())
  }
}

/// One rule's view of the file being linted.
#[derive(Clone)]
#[must_use]
pub struct LintContext<'a> {
  host: Rc<ContextHost<'a>>,
  rule: &'static str,
}

impl<'a> Deref for LintContext<'a> {
  type Target = Semantic<'a>;

  #[inline]
  fn deref(&self) -> &Self::Target {
    &self.host.semantic
  }
}

impl<'a> LintContext<'a> {
  #[inline]
  pub fn semantic(&self) -> &Semantic<'a> {
    &self.host.semantic
  }

  #[inline]
  pub fn module_record(&self) -> &ModuleRecord {
    &self.host.module_record
  }

  #[inline]
  pub fn cfg(&self) -> &ControlFlowGraph {
    self.host.semantic.cfg().expect("built with a control flow graph")
  }

  /// The source text `span` covers.
  pub fn source_range(&self, span: Span) -> &'a str {
    span.source_text(self.host.semantic.source_text())
  }

  /// The offset from `start` of the next `token` before `end`, skipping comments.
  #[expect(clippy::cast_possible_truncation)]
  pub fn find_next_token_within(&self, start: u32, end: u32, token: &str) -> Option<u32> {
    let source = self.source_range(Span::new(start, end));
    source.match_indices(token).find(|(a, _)| !self.is_inside_comment(start + *a as u32)).map(|(a, _)| a as u32)
  }

  /// The offset from `start` of the last `token` before `end`, skipping comments.
  #[expect(clippy::cast_possible_truncation)]
  pub fn find_prev_token_within(&self, start: u32, end: u32, token: &str) -> Option<u32> {
    let source = self.source_range(Span::new(start, end));
    source.rmatch_indices(token).find(|(a, _)| !self.is_inside_comment(start + *a as u32)).map(|(a, _)| a as u32)
  }

  #[inline]
  pub fn globals(&self) -> &OxlintGlobals {
    &self.host.globals
  }

  pub fn is_reference_to_global_variable(&self, ident: &IdentifierReference) -> bool {
    let name = ident.name.as_str();
    ident.is_global_reference(self.scoping()) && !self.globals().get(name).is_some_and(|value| *value == GlobalValue::Off)
  }

  pub fn get_global_variable_value(&self, name: &str) -> Option<GlobalValue> {
    if !self.scoping().root_unresolved_references().contains_key(name) {
      return None;
    }
    if let Some(value) = self.globals().get(name) {
      return Some(*value);
    }
    if GLOBALS_BUILTIN.contains_key(name) {
      return Some(GlobalValue::Readonly);
    }
    self.host.env.iter().find_map(|env| GLOBALS.get(env.as_str()).and_then(|env| env.get(name))).map(|value| GlobalValue::from(*value))
  }

  pub fn is_ecma_script_global(&self, var: &str) -> bool {
    if !self.scoping().root_unresolved_references().contains_key(var) {
      return false;
    }
    GLOBALS_ES2026.contains_key(var) || GLOBALS_BUILTIN.contains_key(var)
  }

  fn add_diagnostic(&self, message: Message) {
    self.host.messages.borrow_mut().push((self.rule, message));
  }

  #[cold]
  #[inline(never)]
  pub fn diagnostic(&self, diagnostic: OxcDiagnostic) {
    self.add_diagnostic(Message::new(diagnostic, PossibleFixes::None));
  }

  #[inline]
  pub fn diagnostic_with_fix<C, F>(&self, diagnostic: OxcDiagnostic, fix: F)
  where
    C: Into<RuleFix>,
    F: FnOnce(RuleFixer<'_, 'a>) -> C,
  {
    self.diagnostic_with_fix_of_kind(diagnostic, FixKind::SafeFix, fix);
  }

  #[inline]
  pub fn diagnostic_with_suggestion<C, F>(&self, diagnostic: OxcDiagnostic, fix: F)
  where
    C: Into<RuleFix>,
    F: FnOnce(RuleFixer<'_, 'a>) -> C,
  {
    self.diagnostic_with_fix_of_kind(diagnostic, FixKind::Suggestion, fix);
  }

  #[inline]
  pub fn diagnostic_with_dangerous_fix<C, F>(&self, diagnostic: OxcDiagnostic, fix: F)
  where
    C: Into<RuleFix>,
    F: FnOnce(RuleFixer<'_, 'a>) -> C,
  {
    self.diagnostic_with_fix_of_kind(diagnostic, FixKind::DangerousFix, fix);
  }

  pub fn diagnostic_with_fix_of_kind<C, F>(&self, diagnostic: OxcDiagnostic, fix_kind: FixKind, fix: F)
  where
    C: Into<RuleFix>,
    F: FnOnce(RuleFixer<'_, 'a>) -> C,
  {
    let (diagnostic, fix) = self.create_fix(fix_kind, fix, diagnostic);
    match fix {
      Some(fix) => self.add_diagnostic(Message::new(diagnostic, PossibleFixes::Single(fix))),
      None => self.diagnostic(diagnostic),
    }
  }

  pub fn diagnostic_with_suggestions<I>(&self, diagnostic: OxcDiagnostic, suggestions: I)
  where
    I: IntoIterator<Item = RuleFix>,
  {
    let fixes: Vec<Fix> = suggestions.into_iter().filter(|fix| self.host.fix.can_apply(fix.kind()) && !fix.is_empty()).map(|fix| fix.into_fix(self.source_text())).collect();
    match fixes.is_empty() {
      true => self.diagnostic(diagnostic),
      false => self.add_diagnostic(Message::new(diagnostic, PossibleFixes::Multiple(fixes))),
    }
  }

  fn create_fix<C, F>(&self, fix_kind: FixKind, fix: F, diagnostic: OxcDiagnostic) -> (OxcDiagnostic, Option<Fix>)
  where
    C: Into<RuleFix>,
    F: FnOnce(RuleFixer<'_, 'a>) -> C,
  {
    let rule_fix: RuleFix = fix(RuleFixer::new(fix_kind, self)).into();
    let diagnostic = match (rule_fix.message(), &diagnostic.help) {
      (Some(message), None) => diagnostic.with_help(message.to_owned()),
      _ => diagnostic,
    };
    match self.host.fix.can_apply(rule_fix.kind()) && !rule_fix.is_empty() {
      true => {
        let fix = rule_fix.into_fix(self.source_text());
        (diagnostic, Some(fix))
      }
      false => (diagnostic, None),
    }
  }
}
