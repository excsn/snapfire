// Ported from oxlint (crates/oxc_linter/src/rules/eslint/no_empty_static_block.rs) at oxc crates_v0.155.0, MIT License, Copyright (c) 2024-present VoidZero Inc. & Contributors.

use oxc_ast::AstKind;
use oxc_diagnostics::OxcDiagnostic;
use crate::js::declare_rule;
use oxc_span::Span;

use crate::js::{AstNode, context::LintContext, rule::Rule};

fn no_empty_static_block_diagnostic(span: Span) -> OxcDiagnostic {
    OxcDiagnostic::warn("Unexpected empty static blocks")
        .with_help("Remove this empty block or add content to it.")
        .with_label(span)
}

#[derive(Debug, Default, Clone)]
pub struct NoEmptyStaticBlock;

declare_rule!(
    /// ### What it does
    ///
    /// Disallow empty static blocks.
    ///
    /// ### Why is this bad?
    ///
    /// Empty block statements, while not technically errors, usually occur due
    /// to refactoring that wasn’t completed.  They can cause confusion when
    /// reading code.
    ///
    /// ### Examples
    ///
    /// Examples of **incorrect** code for this rule:
    /// ```js
    /// class Foo {
    ///     static {
    ///     }
    /// }
    /// ```
    ///
    /// Examples of **correct** code for this rule:
    /// ```js
    /// class Foo {
    ///     static {
    ///         // blocks with comments are allowed
    ///     }
    /// }
    /// class Bar {
    ///     static {
    ///         doSomething();
    ///     }
    /// }
    /// ```
    NoEmptyStaticBlock,
    "eslint",
    "no-empty-static-block",
    Correctness,
    recommended = true,
    fix = RuleFixMeta::Fixable(FixKind::Suggestion),
);

impl Rule for NoEmptyStaticBlock {
    fn run<'a>(&self, node: &AstNode<'a>, ctx: &LintContext<'a>) {
        if let AstKind::StaticBlock(static_block) = node.kind()
            && static_block.body.is_empty()
        {
            if ctx.has_comments_between(static_block.span) {
                return;
            }
            ctx.diagnostic_with_suggestion(
                no_empty_static_block_diagnostic(static_block.span),
                |fixer| fixer.delete(&static_block.span),
            );
        }
    }
}

#[test]
fn test() {
    use crate::js::tester::Tester;

    let pass = vec![
        "class Foo { static { bar(); } }",
        "class Foo { static { /* comments */ } }",
        "class Foo { static {
			// comment
			} }",
        "class Foo { static { bar(); } static { bar(); } }",
    ];

    let fail = vec![
        "class Foo { static {} }",
        "class Foo { static { } }",
        "class Foo { static {

			 } }",
        "class Foo { static { bar(); } static {} }",
    ];

    let fix = vec![
        ("class Foo { static {} }", "class Foo {  }"),
        ("class Foo { static { } }", "class Foo {  }"),
        (
            "class Foo { static {

			 } }",
            "class Foo {  }",
        ),
        ("class Foo { static { bar(); } static {} }", "class Foo { static { bar(); }  }"),
    ];

    Tester::new(NoEmptyStaticBlock::NAME, NoEmptyStaticBlock::PLUGIN, pass, fail)
        .expect_fix(fix)
        .test();
}
