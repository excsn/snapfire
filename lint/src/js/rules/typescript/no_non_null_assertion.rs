// Ported from oxlint (crates/oxc_linter/src/rules/typescript/no_non_null_assertion.rs) at oxc crates_v0.155.0, MIT License, Copyright (c) 2024-present VoidZero Inc. & Contributors.

use oxc_ast::AstKind;
use oxc_diagnostics::OxcDiagnostic;
use crate::js::declare_rule;
use oxc_span::Span;

use crate::js::{
    AstNode,
    context::{ContextHost, LintContext},
    rule::Rule,
};

#[derive(Debug, Default, Clone)]
pub struct NoNonNullAssertion;

declare_rule!(
    /// ### What it does
    ///
    /// Disallow non-null assertions using the `!` postfix operator.
    ///
    /// ### Why is this bad?
    ///
    /// TypeScript's `!` non-null assertion operator asserts to the type system that an expression is non-nullable, as in not `null` or `undefined`. Using assertions to tell the type system new information is often a sign that code is not fully type-safe. It's generally better to structure program logic so that TypeScript understands when values may be nullable.
    ///
    /// ### Examples
    ///
    /// Examples of **incorrect** code for this rule:
    /// ```ts
    /// x!;
    /// x!.y;
    /// x.y!;
    /// ```
    ///
    /// Examples of **correct** code for this rule:
    /// ```ts
    /// x;
    /// x?.y;
    /// x.y;
    /// ```
    NoNonNullAssertion,
    "typescript",
    "no-non-null-assertion",
    Restriction,
    recommended = false,
    fix = RuleFixMeta::FixPending,
);

fn no_non_null_assertion_diagnostic(span: Span, is_member_expression: bool) -> OxcDiagnostic {
    let diagnostic = OxcDiagnostic::warn("Forbidden non-null assertion.")
        .with_note(
            "The non-null assertion operator (`!`) removes `null` and `undefined` from the type. For example, it changes `number | undefined` to `number`.",
        )
        .with_label(span);

    if is_member_expression {
        diagnostic.with_help("Consider using the optional chain operator `?.` instead. `x!.y` is equivalent to `x.y` at runtime and will throw if `x` is `null` or `undefined`, but `x?.y` will return `undefined`.")
    } else {
        diagnostic
    }
}

impl Rule for NoNonNullAssertion {
    fn run<'a>(&self, node: &AstNode<'a>, ctx: &LintContext<'a>) {
        let AstKind::TSNonNullExpression(expr) = node.kind() else { return };
        let is_member_expression = ctx.nodes().parent_kind(node.id()).is_member_expression_kind();
        ctx.diagnostic(no_non_null_assertion_diagnostic(expr.span, is_member_expression));
    }

    fn should_run(&self, ctx: &ContextHost) -> bool {
        ctx.source_type().is_typescript()
    }
}

#[test]
fn test() {
    use crate::js::tester::Tester;

    let pass = vec!["x;", "x.y;", "x.y.z;", "x?.y.z;", "x?.y?.z;", "!x;"];

    let fail = vec![
        "x!;",
        "x!.y;",
        "x.y!;",
        "!x!.y;",
        "x!.y?.z;",
        "x![y];",
        "x![y]?.z;",
        "x.y.z!();",
        "x.y?.z!();",
        "x!!!;",
        "x!!.y;",
        "x.y!!;",
        "x.y.z!!();",
        "x!?.[y].z;",
        "x!?.y.z;",
        "x.y.z!?.();",
        "
            x!
            .y
                  ",
        "
            x!
            // comment
            .y
                  ",
        "
            x!
             // comment
                . /* comment */
                  y
                  ",
        "
            x!
             // comment
                 /* comment */ ['y']
                  ",
        "
            document.querySelector('input')!.files = new FileList();
                  ",
        "
            hoge.files = document.querySelector('input')!.files;
                  ",
    ];

    Tester::new(NoNonNullAssertion::NAME, NoNonNullAssertion::PLUGIN, pass, fail)
        .test();
}
