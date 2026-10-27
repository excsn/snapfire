// Derived from oxc_linter's tester.rs at oxc crates_v0.155.0, MIT License, Copyright (c) 2024-present VoidZero Inc. & Contributors.

use std::{fmt::Write, path::PathBuf};

use oxc_span::SourceType;
use serde_json::Value;

use super::{FixKind, RULES, fixer::Fixer, globals::OxlintGlobals};

#[derive(Eq, PartialEq)]
enum TestResult {
    Passed,
    Failed,
    Fixed(String),
}

#[derive(Debug, Clone, Default)]
pub struct TestCase {
    source: String,
    rule_config: Option<Value>,
    eslint_config: Option<Value>,
    path: Option<PathBuf>,
}

impl From<&str> for TestCase {
    fn from(source: &str) -> Self {
        Self { source: source.to_string(), ..Self::default() }
    }
}

impl From<String> for TestCase {
    fn from(source: String) -> Self {
        Self { source, ..Self::default() }
    }
}

impl From<(&str, Option<Value>)> for TestCase {
    fn from((source, rule_config): (&str, Option<Value>)) -> Self {
        Self { source: source.to_string(), rule_config, ..Self::default() }
    }
}

impl From<(&str, Option<Value>, Option<Value>)> for TestCase {
    fn from((source, rule_config, eslint_config): (&str, Option<Value>, Option<Value>)) -> Self {
        Self { source: source.to_string(), rule_config, eslint_config, ..Self::default() }
    }
}

impl From<(&str, Option<Value>, Option<Value>, Option<PathBuf>)> for TestCase {
    fn from(
        (source, rule_config, eslint_config, path): (
            &str,
            Option<Value>,
            Option<Value>,
            Option<PathBuf>,
        ),
    ) -> Self {
        Self { source: source.to_string(), rule_config, eslint_config, path }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ExpectFixKind {
    /// We expect no fix to be applied
    #[default]
    None,
    /// We expect some fix to be applied, but don't care what kind it is
    Any,
    /// We expect a fix of a certain [`FixKind`] to be applied
    Specific(FixKind),
}

impl ExpectFixKind {
    #[inline]
    pub fn is_none(self) -> bool {
        matches!(self, Self::None)
    }

    #[inline]
    pub fn is_some(self) -> bool {
        !self.is_none()
    }
}

impl From<FixKind> for ExpectFixKind {
    fn from(kind: FixKind) -> Self {
        Self::Specific(kind)
    }
}
impl From<ExpectFixKind> for FixKind {
    fn from(expected_kind: ExpectFixKind) -> Self {
        match expected_kind {
            ExpectFixKind::None => FixKind::None,
            ExpectFixKind::Any => FixKind::All,
            ExpectFixKind::Specific(kind) => kind,
        }
    }
}

impl From<Option<FixKind>> for ExpectFixKind {
    fn from(maybe_kind: Option<FixKind>) -> Self {
        match maybe_kind {
            Some(kind) => Self::Specific(kind),
            None => Self::Any, // intentionally not None
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExpectFixTestCase {
    /// Source code being tested
    source: String,
    expected: Vec<ExpectFix>,
    rule_config: Option<Value>,
    path: Option<PathBuf>,
    eslint_config: Option<Value>,
}

#[derive(Debug, Clone)]
struct ExpectFix {
    /// Expected source code after fix has been applied
    expected: String,
    kind: ExpectFixKind,
}

impl<S: Into<String>> From<(S, S, Option<Value>)> for ExpectFixTestCase {
    fn from(value: (S, S, Option<Value>)) -> Self {
        Self {
            source: value.0.into(),
            expected: vec![ExpectFix { expected: value.1.into(), kind: ExpectFixKind::Any }],
            rule_config: value.2,
            path: None,
            eslint_config: None,
        }
    }
}

impl<S: Into<String>> From<(S, S)> for ExpectFixTestCase {
    fn from(value: (S, S)) -> Self {
        Self {
            source: value.0.into(),
            expected: vec![ExpectFix { expected: value.1.into(), kind: ExpectFixKind::Any }],
            rule_config: None,
            path: None,
            eslint_config: None,
        }
    }
}

impl<S: Into<String>> From<(S, (S, S))> for ExpectFixTestCase {
    fn from(value: (S, (S, S))) -> Self {
        Self {
            source: value.0.into(),
            expected: vec![
                ExpectFix { expected: value.1.0.into(), kind: ExpectFixKind::Any },
                ExpectFix { expected: value.1.1.into(), kind: ExpectFixKind::Any },
            ],
            rule_config: None,
            path: None,
            eslint_config: None,
        }
    }
}

impl<S: Into<String>> From<(S, (S, S), Option<Value>)> for ExpectFixTestCase {
    fn from(value: (S, (S, S), Option<Value>)) -> Self {
        Self {
            source: value.0.into(),
            expected: vec![
                ExpectFix { expected: value.1.0.into(), kind: ExpectFixKind::Any },
                ExpectFix { expected: value.1.1.into(), kind: ExpectFixKind::Any },
            ],
            rule_config: value.2,
            path: None,
            eslint_config: None,
        }
    }
}

impl<S: Into<String>> From<(S, (S, S, S))> for ExpectFixTestCase {
    fn from(value: (S, (S, S, S))) -> Self {
        Self {
            source: value.0.into(),
            expected: vec![
                ExpectFix { expected: value.1.0.into(), kind: ExpectFixKind::Any },
                ExpectFix { expected: value.1.1.into(), kind: ExpectFixKind::Any },
                ExpectFix { expected: value.1.2.into(), kind: ExpectFixKind::Any },
            ],
            rule_config: None,
            path: None,
            eslint_config: None,
        }
    }
}

impl<S: Into<String>> From<(S, (S, S, S, S))> for ExpectFixTestCase {
    fn from(value: (S, (S, S, S, S))) -> Self {
        Self {
            source: value.0.into(),
            expected: vec![
                ExpectFix { expected: value.1.0.into(), kind: ExpectFixKind::Any },
                ExpectFix { expected: value.1.1.into(), kind: ExpectFixKind::Any },
                ExpectFix { expected: value.1.2.into(), kind: ExpectFixKind::Any },
                ExpectFix { expected: value.1.3.into(), kind: ExpectFixKind::Any },
            ],
            rule_config: None,
            path: None,
            eslint_config: None,
        }
    }
}

impl<S, F> From<(S, S, Option<Value>, F)> for ExpectFixTestCase
where
    S: Into<String>,
    F: Into<ExpectFixKind>,
{
    fn from((source, expected, config, kind): (S, S, Option<Value>, F)) -> Self {
        Self {
            source: source.into(),
            expected: vec![ExpectFix { expected: expected.into(), kind: kind.into() }],
            rule_config: config,
            path: None,
            eslint_config: None,
        }
    }
}

impl<S> From<(S, S, Option<Value>, Option<Value>)> for ExpectFixTestCase
where
    S: Into<String>,
{
    fn from(
        (source, expected, config, eslint_config): (S, S, Option<Value>, Option<Value>),
    ) -> Self {
        Self {
            source: source.into(),
            expected: vec![ExpectFix { expected: expected.into(), kind: ExpectFixKind::Any }],
            rule_config: config,
            path: None,
            eslint_config,
        }
    }
}

impl<S: Into<String>> From<(S, S, Option<Value>, Option<PathBuf>)> for ExpectFixTestCase {
    fn from((source, expected, config, path): (S, S, Option<Value>, Option<PathBuf>)) -> Self {
        Self {
            source: source.into(),
            expected: vec![ExpectFix { expected: expected.into(), kind: ExpectFixKind::Any }],
            rule_config: config,
            path,
            eslint_config: None,
        }
    }
}

impl<S: Into<String>> From<(S, S, Option<Value>, Option<PathBuf>, Option<Value>)>
    for ExpectFixTestCase
{
    fn from(
        (source, expected, config, path, eslint_config): (
            S,
            S,
            Option<Value>,
            Option<PathBuf>,
            Option<Value>,
        ),
    ) -> Self {
        Self {
            source: source.into(),
            expected: vec![ExpectFix { expected: expected.into(), kind: ExpectFixKind::Any }],
            rule_config: config,
            path,
            eslint_config,
        }
    }
}

pub struct Tester {
    rule_name: &'static str,
    plugin_name: &'static str,
    rule_path: PathBuf,
    expect_pass: Vec<TestCase>,
    expect_fail: Vec<TestCase>,
    expect_fix: Option<Vec<ExpectFixTestCase>>,
    output: String,
}

impl Tester {
    pub fn new<T: Into<TestCase>>(
        rule_name: &'static str,
        plugin_name: &'static str,
        expect_pass: Vec<T>,
        expect_fail: Vec<T>,
    ) -> Self {
        let rule_path = PathBuf::from(rule_name.replace('-', "_")).with_extension("tsx");
        Self {
            rule_name,
            plugin_name,
            rule_path,
            expect_pass: expect_pass.into_iter().map(Into::into).collect(),
            expect_fail: expect_fail.into_iter().map(Into::into).collect(),
            expect_fix: None,
            output: String::new(),
        }
    }

    pub fn change_rule_path(mut self, path: &str) -> Self {
        self.rule_path = PathBuf::from(path);
        self
    }

    /// Change the extension of the path, without the dot. The default is `tsx`.
    pub fn change_rule_path_extension(mut self, ext: &str) -> Self {
        self.rule_path = self.rule_path.with_extension(ext);
        self
    }

    /// oxlint names its snapshot files with this; there are no snapshots here.
    pub fn with_snapshot_suffix(self, _suffix: &'static str) -> Self {
        self
    }

    #[must_use]
    pub fn expect_fix<F: Into<ExpectFixTestCase>>(mut self, expect_fix: Vec<F>) -> Self {
        assert!(!expect_fix.is_empty(), "You must provide at least one fixer test case to `expect_fix`");
        self.expect_fix = Some(expect_fix.into_iter().map(Into::into).collect());
        self
    }

    /// Pass when the rule reports a fix capability but this test has no fix cases.
    #[must_use]
    pub fn intentionally_allow_no_fix_tests(mut self) -> Self {
        self.expect_fix = Some(vec![]);
        self
    }

    #[expect(clippy::print_stdout)]
    pub fn test(&mut self) {
        let failed = self.test_pass();
        let passed = self.test_fail();
        let fix_failures = self.test_fix();
        if !failed.is_empty() {
            println!("{}", format_test_failures("expected to pass, but failed", &failed));
        }
        if !passed.is_empty() {
            println!("{}", format_test_failures("expected to fail, but passed", &passed));
        }
        if !fix_failures.is_empty() {
            println!("{}", format_fix_failures(&fix_failures));
        }
        assert!(
            failed.is_empty() && passed.is_empty() && fix_failures.is_empty(),
            "Some tests failed for rule {}/{} (see output above)",
            self.plugin_name,
            self.rule_name
        );
    }

    fn test_pass(&mut self) -> Vec<TestFailure> {
        let mut failed = vec![];
        for TestCase { source, rule_config, eslint_config, path } in self.expect_pass.clone() {
            let start = self.output.len();
            if self.run(&source, rule_config.clone(), eslint_config, path, ExpectFixKind::None, 0) != TestResult::Passed {
                failed.push(TestFailure::ExpectedToPass { source, rule_config, diagnostic: self.output[start..].to_string() });
            }
        }
        failed
    }

    fn test_fail(&mut self) -> Vec<TestFailure> {
        let mut passed = vec![];
        for TestCase { source, rule_config, eslint_config, path } in self.expect_fail.clone() {
            if self.run(&source, rule_config.clone(), eslint_config, path, ExpectFixKind::None, 0) != TestResult::Failed {
                passed.push(TestFailure::ExpectedToFail { source, rule_config });
            }
        }
        passed
    }

    #[expect(clippy::cast_possible_truncation)]
    fn test_fix(&mut self) -> Vec<FixFailure> {
        let mut failures = vec![];
        let Some(fix_test_cases) = self.expect_fix.clone() else {
            return failures;
        };
        for ExpectFixTestCase { source, expected, rule_config, path, eslint_config } in fix_test_cases {
            for (index, expect) in expected.iter().enumerate() {
                let actual = match self.run(&source, rule_config.clone(), eslint_config.clone(), path.clone(), expect.kind, index as u8) {
                    TestResult::Fixed(fixed) if fixed == expect.expected => continue,
                    TestResult::Fixed(fixed) => fixed,
                    TestResult::Passed => String::from("<test passed, no fix applied>"),
                    TestResult::Failed => String::from("<test failed, no fix applied>"),
                };
                failures.push(FixFailure { source: source.clone(), expected: expect.expected.clone(), actual });
            }
        }
        failures
    }

    fn run(&mut self, source_text: &str, rule_config: Option<Value>, eslint_config: Option<Value>, path: Option<PathBuf>, fix_kind: ExpectFixKind, fix_index: u8) -> TestResult {
        if let Some(config) = rule_config.as_ref() {
            assert!(config.is_array(), "Rule config for {}/{} must be an array or None, got: {config}", self.plugin_name, self.rule_name);
        }
        let id = format!("{}/{}", self.plugin_name, self.rule_name);
        let entry = RULES.iter().find(|entry| entry.id == id).unwrap_or_else(|| panic!("no rule {id}"));
        let rule = (entry.build)(rule_config).unwrap_or_else(|e| panic!("{id}: {e}"));
        let globals = eslint_config.as_ref().and_then(|c| c.get("globals")).map(|g| serde_json::from_value::<OxlintGlobals>(g.clone()).expect("globals")).unwrap_or_default();
        let env = eslint_config.as_ref().and_then(|c| c.get("env")).and_then(Value::as_object).map(|env| env.iter().filter(|(_, on)| on.as_bool() == Some(true)).map(|(name, _)| name.clone()).collect()).unwrap_or_default();
        let path = path.unwrap_or_else(|| self.rule_path.clone());
        let rules = [(entry.id, rule.as_ref())];
        let (outcome, messages) = super::lint_with(&path, source_text, &rules, FixKind::from(fix_kind), globals, env, false);
        let lines = crate::suppress::Lines::new(source_text);
        let directives = crate::suppress::directives(outcome.comments.iter().map(|c| (c.text.as_str(), c.start, c.end)), &lines);
        let messages: Vec<_> = messages.into_iter().filter(|m| crate::suppress::silencing(&directives, entry.id, m.span.start as usize, lines.line(m.span.start as usize)).is_none()).collect();
        if let Some(parse) = outcome.reported.iter().find(|r| r.rule == "lint/parse") {
            let _ = writeln!(self.output, "  {id}: the source does not parse: {}", parse.message);
            return TestResult::Failed;
        }
        if messages.is_empty() {
            return TestResult::Passed;
        }
        if fix_kind.is_some() {
            let fixed = Fixer::new(source_text, messages, SourceType::from_path(&path).ok()).with_fix_index(fix_index).fix();
            return TestResult::Fixed(fixed.fixed_code.to_string());
        }
        for message in &messages {
            let _ = writeln!(self.output, "  {}: {}", id, message.error.message);
        }
        TestResult::Failed
    }
}

struct FixFailure {
    /// Test source code
    source: String,
    /// Expected source code after fix
    expected: String,
    /// Actual source code after fix
    actual: String,
}

enum TestFailure {
    ExpectedToPass {
        /// Test source code
        source: String,
        /// Rule configuration used in the test
        rule_config: Option<Value>,
        /// Error/diagnostic output produced by the test
        diagnostic: String,
    },
    ExpectedToFail {
        /// Test source code
        source: String,
        /// Rule configuration used in the test
        rule_config: Option<Value>,
    },
}

/// Format source code for display in test failure output.
/// If the source has more than `max_lines` lines, it will be truncated.
/// Otherwise, multi-line sources are displayed with proper indentation.
fn format_test_source(source: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let line_count = lines.len();

    if line_count <= 1 {
        // Single line: display inline
        source.to_string()
    } else if line_count <= max_lines {
        // Multi-line but within limit: display with indentation
        let mut result = String::new();
        for (i, line) in lines.iter().enumerate() {
            if i == 0 {
                result.push_str("| ");
            } else {
                result.push_str("      │ ");
            }
            result.push_str(line);
            result.push('\n');
        }
        result
    } else {
        // Too many lines: truncate with summary
        let mut result = String::new();
        for (i, line) in lines.iter().take(max_lines).enumerate() {
            if i == 0 {
                result.push_str("| ");
            } else {
                result.push_str("      │ ");
            }
            result.push_str(line);
            result.push('\n');
        }
        writeln!(result, "      │ ... ({} more line(s))", line_count - max_lines).unwrap();
        result
    }
}

/// Format a code block for display, showing inline for single lines or with pipes for multi-line.
fn format_code_block(output: &mut String, code: &str) {
    let lines: Vec<&str> = code.lines().collect();
    if lines.len() <= 1 {
        // Single line: display inline
        let _ = writeln!(output, "{code}");
    } else {
        // Multi-line: display with pipes
        let _ = writeln!(output);
        for line in lines {
            let _ = writeln!(output, "      │ {line}");
        }
    }
}

fn format_test_failures(reason: &str, failures: &[TestFailure]) -> String {
    let count = failures.len();
    let mut output = String::new();
    let _ =
        writeln!(output, "\n{count} test case{} {reason}:\n", if count == 1 { "" } else { "s" });

    for (index, failure) in failures.iter().enumerate() {
        match failure {
            TestFailure::ExpectedToPass { diagnostic, rule_config, source } => {
                let formatted_source = format_test_source(source, 10);
                let _ = writeln!(output, "  {:>2}. {formatted_source}", index + 1);
                let _ = writeln!(
                    output,
                    "      Diagnostic:\n{}",
                    diagnostic.replace('\n', "\n      ")
                );
                if let Some(config) = &rule_config {
                    // Format config compactly on one line if possible
                    let config_str = serde_json::to_string(config).unwrap_or_default();
                    let _ = writeln!(output, "      config: {config_str}");
                }
            }
            TestFailure::ExpectedToFail { rule_config, source } => {
                let formatted_source = format_test_source(source, 10);
                let _ = writeln!(output, "  {:>2}. {formatted_source}", index + 1);
                if let Some(config) = &rule_config {
                    // Format config compactly on one line if possible
                    let config_str = serde_json::to_string(config).unwrap_or_default();
                    let _ = writeln!(output, "      config: {config_str}");
                }
            }
        }
    }
    output
}

fn format_fix_failures(failures: &[FixFailure]) -> String {
    let count = failures.len();
    let mut output = String::new();
    let _ = writeln!(
        output,
        "\n{count} fix{} did not produce expected output:\n",
        if count == 1 { "" } else { "es" }
    );

    for (index, failure) in failures.iter().enumerate() {
        let _ = write!(output, "  {:>2}.    Input: ", index + 1);
        format_code_block(&mut output, &failure.source);
        let _ = write!(output, "      Expected: ");
        format_code_block(&mut output, &failure.expected);
        let _ = write!(output, "        Actual: ");
        format_code_block(&mut output, &failure.actual);
        let _ = writeln!(output);
    }
    output
}
