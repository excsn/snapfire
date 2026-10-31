// Ported from oxlint (crates/oxc_linter/src/rules/jsdoc/require_throws_description.rs) at oxc crates_v0.155.0, MIT License, Copyright (c) 2024-present VoidZero Inc. & Contributors.

use oxc_diagnostics::OxcDiagnostic;
use crate::js::declare_rule;
use oxc_span::Span;

use crate::js::{
    context::LintContext,
    rule::Rule,
    utils::{should_ignore_as_internal, should_ignore_as_private},
};

fn require_throws_description_diagnostic(span: Span) -> OxcDiagnostic {
    OxcDiagnostic::warn("Missing JSDoc `@throws` description.")
        .with_help("Add description comment to `@throws` tag.")
        .with_label(span)
}

#[derive(Debug, Default, Clone)]
pub struct RequireThrowsDescription;

declare_rule!(
    /// ### What it does
    ///
    /// Requires a description for `@throws` tags.
    ///
    /// ### Why is this bad?
    ///
    /// A `@throws` tag should explain the condition or reason an error may be thrown.
    ///
    /// ### Examples
    ///
    /// Examples of **incorrect** code for this rule:
    /// ```js
    /// /**
    ///  * @throws {Error}
    ///  */
    /// function quux () {
    ///   throw new Error('error');
    /// }
    /// ```
    ///
    /// Examples of **correct** code for this rule:
    /// ```js
    /// /**
    ///  * @throws {Error} Has a description
    ///  */
    /// function quux () {
    ///   throw new Error('error');
    /// }
    /// ```
    RequireThrowsDescription,
    "jsdoc",
    "require-throws-description",
    Style,
    recommended = false,
    fix = RuleFixMeta::None,
);

impl Rule for RequireThrowsDescription {
    fn run_once(&self, ctx: &LintContext) {
        let settings = &ctx.settings().jsdoc;

        for jsdoc in ctx
            .jsdoc()
            .iter_all()
            .filter(|jsdoc| !should_ignore_as_internal(jsdoc, settings))
            .filter(|jsdoc| !should_ignore_as_private(jsdoc, settings))
        {
            for tag in jsdoc.tags() {
                let tag_name = tag.kind;

                if !matches!(tag_name.parsed(), "throws" | "exception") {
                    continue;
                }

                let (_, comment_part) = tag.type_comment();

                if comment_part.parsed().is_empty() {
                    ctx.diagnostic(require_throws_description_diagnostic(tag_name.span));
                }
            }
        }
    }
}

#[test]
fn test() {
    use crate::js::tester::Tester;

    let pass = vec![
        "
                    /**
                     * @throws {SomeType} Has a description
                     */
                  ",
        "
                    /**
                     * @throws Has a description
                     */
                  ",
        "
                    /**
                     * @exception {SomeType} Has a description
                     */
                  ",
        "
                    /**
                     * @exception Has a description
                     */
                  ",
    ];

    let fail = vec![
        "
                    /**
                     * @throws
                     */
                  ",
        "
                    /**
                     * @exception
                     */
                  ",
        "
                    /**
                     * @throws {SomeType}
                     */
                  ",
        "
                    /**
                     * @exception {SomeType}
                     */
                  ",
    ];

    Tester::new(RequireThrowsDescription::NAME, RequireThrowsDescription::PLUGIN, pass, fail)
        .test();
}
