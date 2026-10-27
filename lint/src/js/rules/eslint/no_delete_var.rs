// Ported from oxlint (crates/oxc_linter/src/rules/eslint/no_delete_var.rs) at oxc crates_v0.155.0, MIT License, Copyright (c) 2024-present VoidZero Inc. & Contributors.

use oxc_ast::AstKind;
use oxc_diagnostics::OxcDiagnostic;
use crate::js::declare_rule;
use oxc_span::Span;
use oxc_syntax::operator::UnaryOperator;

use crate::js::{AstNode, context::LintContext, rule::Rule};

fn no_delete_var_diagnostic(span: Span) -> OxcDiagnostic {
    OxcDiagnostic::warn("Variables should not be deleted")
        .with_help("Assign `undefined` to the variable instead of using `delete`. The `delete` operator is intended for removing properties from objects, not for variables.")
        .with_label(span)
}

#[derive(Debug, Default, Clone)]
pub struct NoDeleteVar;

declare_rule!(
    /// ### What it does
    ///
    /// The purpose of the `delete` operator is to remove a property from an
    /// object.
    ///
    /// ### Why is this bad?
    ///
    /// Using the `delete` operator on a variable might lead to unexpected
    /// behavior.
    ///
    /// ### Examples
    ///
    /// Examples of **incorrect** code for this rule:
    /// ```javascript
    /// var x;
    /// delete x;
    /// ```
    ///
    /// Examples of **correct** code for this rule:
    /// ```javascript
    /// var x;
    ///
    /// var y;
    /// delete y.prop;
    /// ```
    NoDeleteVar,
    "eslint",
    "no-delete-var",
    Correctness,
    recommended = true,
    fix = RuleFixMeta::None,
);

impl Rule for NoDeleteVar {
    fn run<'a>(&self, node: &AstNode<'a>, ctx: &LintContext<'a>) {
        let AstKind::UnaryExpression(expr) = node.kind() else {
            return;
        };
        if expr.operator == UnaryOperator::Delete && expr.argument.is_identifier_reference() {
            ctx.diagnostic(no_delete_var_diagnostic(expr.span));
        }
    }
}

#[test]
fn test() {
    use crate::js::tester::Tester;

    let pass = vec!["delete x.prop;"];

    let fail = vec!["delete x"];

    Tester::new(NoDeleteVar::NAME, NoDeleteVar::PLUGIN, pass, fail).test();
}
