// Ported from oxlint (crates/oxc_linter/src/rules/unicorn/no_abusive_eslint_disable.rs) at oxc crates_v0.155.0, MIT License, Copyright (c) 2024-present VoidZero Inc. & Contributors.

use oxc_diagnostics::OxcDiagnostic;
use crate::js::declare_rule;
use oxc_span::Span;

use crate::js::{context::LintContext, rule::Rule};

/// Which form of suppression comment was written.
#[derive(Debug, Clone, Copy)]
enum DirectivePrefix {
    Eslint,
    Fsr,
}

fn no_abusive_eslint_disable_diagnostic(
    span: Span,
    directive_prefix: DirectivePrefix,
) -> OxcDiagnostic {
    let message = match directive_prefix {
        DirectivePrefix::Eslint => {
            "Unexpected `eslint-disable` comment that does not specify any rules to disable."
        }
        DirectivePrefix::Fsr => {
            "Unexpected `fsr-lint-disable` comment that does not specify any rules to disable."
        }
    };
    OxcDiagnostic::warn(message)
        .with_help("Specify the rules you want to disable.")
        .with_label(span)
}

#[derive(Debug, Default, Clone)]
pub struct NoAbusiveEslintDisable;

declare_rule!(
    /// ### What it does
    ///
    /// Disallows `fsr-lint-disable` or `eslint-disable` comments without specifying rules.
    ///
    /// ### Why is this bad?
    ///
    /// A general `fsr-lint-disable` or `eslint-disable` comment suppresses all lint errors, not just the intended one,
    /// potentially hiding useful warnings and making debugging harder.
    ///
    /// ### Examples
    ///
    /// Examples of **incorrect** code for this rule:
    ///
    /// ```javascript
    /// /* eslint-disable */
    /// console.log(message);
    ///
    /// console.log(message); // eslint-disable-line
    ///
    /// // eslint-disable-next-line
    /// console.log(message);
    /// ```
    ///
    /// ```javascript
    /// /* fsr-lint-disable */
    /// console.log(message);
    ///
    /// console.log(message); // fsr-lint-disable-line
    ///
    /// // fsr-lint-disable-next-line
    /// console.log(message);
    /// ```
    ///
    /// Examples of **correct** code for this rule:
    ///
    /// ```javascript
    /// /* eslint-disable no-console */
    /// console.log(message);
    ///
    /// console.log(message); // eslint-disable-line no-console
    ///
    /// // eslint-disable-next-line no-console
    /// console.log(message);
    /// ```
    ///
    /// ```javascript
    /// /* fsr-lint-disable no-console */
    /// console.log(message);
    ///
    /// console.log(message); // fsr-lint-disable-line no-console
    ///
    /// // fsr-lint-disable-next-line no-console
    /// console.log(message);
    /// ```
    NoAbusiveEslintDisable,
    "unicorn",
    "no-abusive-eslint-disable",
    Restriction,
    recommended = true,
    fix = RuleFixMeta::None,
);

impl Rule for NoAbusiveEslintDisable {
    fn run_once(&self, ctx: &LintContext) {
        let source = ctx.source_text();
        let comments: Vec<(&str, usize, usize)> = ctx
            .semantic()
            .comments()
            .iter()
            .map(|comment| (&source[comment.span.start as usize..comment.span.end as usize], comment.span.start as usize, comment.span.end as usize))
            .collect();
        let lines = crate::suppress::Lines::new(source);
        for directive in crate::suppress::directives(comments.iter().copied(), &lines) {
            if !directive.silences() {
                continue;
            }
            let prefix = if directive.written.contains("fsr-lint-") { DirectivePrefix::Fsr } else { DirectivePrefix::Eslint };
            let span = Span::new(directive.start as u32, (directive.start + directive.written.len()) as u32);
            match directive.rules() {
                None => ctx.diagnostic(no_abusive_eslint_disable_diagnostic(span, prefix)),
                Some(_) => {
                    if written_names(&directive.written).iter().any(|rule| !is_valid_rule_name(rule)) {
                        ctx.diagnostic(no_abusive_eslint_disable_diagnostic(span, prefix));
                    }
                }
            }
        }
    }
}

/// The rule names a suppression comment lists as written, before ` -- ` and the comment's closing delimiter.
fn written_names(written: &str) -> Vec<&str> {
    let body = written.trim_start_matches("//").trim_start_matches("/*").trim_end_matches("*/").trim();
    let rest = body.split_once(char::is_whitespace).map_or("", |(_, rest)| rest);
    let rest = rest.split(" --").next().unwrap_or(rest);
    rest.split(',').map(str::trim).filter(|name| !name.is_empty()).collect()
}

fn is_valid_rule_name(rule_name: &str) -> bool {
    let segment_count = rule_name.split('/').count();
    if rule_name.starts_with('@') {
        segment_count == 2 || segment_count == 3
    } else {
        segment_count <= 2
    }
}

#[test]
fn test() {
    use crate::js::tester::Tester;

    let pass = vec![
        "eval();",
        "eval(); // eslint-disable-line no-eval",
        "eval(); // eslint-disable-line no-eval, no-console",
        "eval(); //eslint-disable-line no-eval",
        "eval(); //     eslint-disable-line no-eval",
        "eval(); //	eslint-disable-line no-eval",
        "eval(); /* eslint-disable-line no-eval */",
        "eval(); //	fsr-lint-disable-line no-eval",
        "eval(); /* fsr-lint-disable-line no-eval */",
        "eval(); // eslint-disable-line plugin/rule",
        "eval(); // eslint-disable-line @scope/plugin/rule-name",
        "eval(); // fsr-lint-disable-line plugin/rule",
        "eval(); // fsr-lint-disable-line @scope/plugin/rule-name",
        "eval(); // eslint-disable-line no-eval, @scope/plugin/rule-name",
        "eval(); // eslint-disable-line @scope/rule-name",
        "eval(); // eslint-disable-line no-eval, @scope/rule-name",
        "eval(); // eslint-line-disable",
        "eval(); // some comment",
        "/* eslint-disable no-eval */",
        "/* fsr-lint-disable no-eval */",
        r"
        /* eslint-disable unicorn/no-abusive-eslint-disable */
        eval(); // eslint-disable-line
        ",
        r"
        /* fsr-lint-disable unicorn/no-abusive-eslint-disable */
        eval(); // fsr-lint-disable-line
        ",
        r"
        foo();
        // eslint-disable-line no-eval
        eval();
        ",
        r"
        foo();
        /* eslint-disable no-eval */
        eval();
        ",
        r"
        foo();
        /* eslint-disable-next-line no-eval */
        eval();
        ",
        r"
        foo();
        /* fsr-lint-disable-next-line no-eval */
        eval();
        ",
    ];

    let fail = vec![
        r"
        // eslint-disable-next-line @scopewithoutplugin
        eval();
        ",
        "// fsr-lint-disable-next-line @scopewithoutplugin\neval();",
        "eval(); // eslint-disable-line",
        "eval(); // fsr-lint-disable-line",
        r"
        foo();
        eval(); // eslint-disable-line
        ",
        "/* eslint-disable */",
        "/* fsr-lint-disable */",
        r"
        foo();
        /* eslint-disable */
        eval();
        ",
        r"
        foo();
        /* fsr-lint-disable */
        eval();
        ",
        r"
        foo();
        /* eslint-disable-next-line */
        eval();
        ",
        r"
        // eslint-disable-next-line
        eval();
        ",
        r"
        foo();
        /* fsr-lint-disable-next-line */
        eval();
        ",
        r"
        // fsr-lint-disable-next-line
        eval();
        ",
    ];

    Tester::new(NoAbusiveEslintDisable::NAME, NoAbusiveEslintDisable::PLUGIN, pass, fail)
        .test();
}
