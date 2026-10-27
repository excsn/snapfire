// Ported from oxlint (crates/oxc_linter/src/rules/jsx_a11y/tabindex_no_positive.rs) at oxc crates_v0.155.0, MIT License, Copyright (c) 2024-present VoidZero Inc. & Contributors.

use oxc_ast::{AstKind, ast::JSXAttributeItem};
use oxc_diagnostics::OxcDiagnostic;
use crate::js::declare_rule;
use oxc_span::{GetSpan, Span};

use crate::js::{
    AstNode,
    context::LintContext,
    rule::Rule,
    utils::{has_jsx_prop_ignore_case, parse_jsx_value},
};

fn tabindex_no_positive_diagnostic(span: Span) -> OxcDiagnostic {
    OxcDiagnostic::warn("Avoid positive integer values for `tabIndex`.")
        .with_help("Change the `tabIndex` prop to a non-positive value.")
        .with_label(span)
}

#[derive(Debug, Default, Clone)]
pub struct TabindexNoPositive;

declare_rule!(
    /// ### What it does
    ///
    /// Enforces that positive values for the `tabIndex` attribute are not used
    /// in JSX.
    ///
    /// ### Why is this bad?
    ///
    /// Using `tabIndex` values greater than `0` can make navigation and
    /// interaction difficult for keyboard and assistive technology users,
    /// disrupting the logical order of content.
    ///
    /// ### Examples
    ///
    /// Examples of **incorrect** code for this rule:
    /// ```jsx
    /// <span tabIndex="1">foo</span>
    /// ```
    ///
    /// Examples of **correct** code for this rule:
    /// ```jsx
    /// <span tabIndex="0">foo</span>
    /// <span tabIndex="-1">bar</span>
    /// ```
    TabindexNoPositive,
    "jsx-a11y",
    "tabindex-no-positive",
    Correctness,
    recommended = true,
    fix = RuleFixMeta::Fixable(FixKind::Dangerous.union(FixKind::Suggestion)),
);

impl Rule for TabindexNoPositive {
    fn run<'a>(&self, node: &AstNode<'a>, ctx: &LintContext<'a>) {
        let AstKind::JSXOpeningElement(jsx_el) = node.kind() else {
            return;
        };
        if let Some(tab_index_prop) = has_jsx_prop_ignore_case(jsx_el, "tabIndex") {
            check_and_diagnose(tab_index_prop, ctx);
        }
    }
}

fn check_and_diagnose(attr: &JSXAttributeItem, ctx: &LintContext<'_>) {
    match attr {
        JSXAttributeItem::Attribute(attr) => attr.value.as_ref().map_or((), |value| {
            if let Ok(parsed_value) = parse_jsx_value(value)
                && parsed_value > 0.0
            {
                ctx.diagnostic_with_dangerous_suggestion(
                    tabindex_no_positive_diagnostic(attr.span),
                    |fixer| fixer.replace(value.span(), r#""0""#),
                );
            }
        }),
        JSXAttributeItem::SpreadAttribute(_) => {}
    }
}

#[test]
fn test() {
    use crate::js::tester::Tester;

    let pass = vec![
        r"<div />;",
        r"<div {...props} />",
        r#"<div id="main" />"#,
        r"<div tabIndex={undefined} />",
        r"<div tabIndex={`${undefined}`} />",
        r"<div tabIndex={`${undefined}${undefined}`} />",
        r"<div tabIndex={0} />",
        r"<div tabIndex={-1} />",
        r"<div tabIndex={null} />",
        r"<div tabIndex={bar()} />",
        r"<div tabIndex={bar} />",
        r#"<div tabIndex={"foobar"} />"#,
        r#"<div tabIndex="0" />"#,
        r#"<div tabIndex="-1" />"#,
        r#"<div tabIndex="-5" />"#,
        r#"<div tabIndex="-5.5" />"#,
        r"<div tabIndex={-5.5} />",
        r"<div tabIndex={-5} />",
    ];

    let fail = vec![
        r#"<div tabIndex="1" />"#,
        r"<div tabIndex={1} />",
        r#"<div tabIndex={"1"} />"#,
        r"<div tabIndex={`1`} />",
        r"<div tabIndex={1.589} />",
    ];

    let fix = vec![
        (r#"<div tabIndex="1" />"#, r#"<div tabIndex="0" />"#),
        (r"<div tabIndex={1} />", r#"<div tabIndex="0" />"#),
        (r#"<div tabIndex={"1"} />"#, r#"<div tabIndex="0" />"#),
        (r"<div tabIndex={`1`} />", r#"<div tabIndex="0" />"#),
        (r"<div tabIndex={1.589} />", r#"<div tabIndex="0" />"#),
    ];

    Tester::new(TabindexNoPositive::NAME, TabindexNoPositive::PLUGIN, pass, fail)
        .expect_fix(fix)
        .test();
}
