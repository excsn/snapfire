// Ported from oxlint (crates/oxc_linter/src/rules/promise/avoid_new.rs) at oxc crates_v0.155.0, MIT License, Copyright (c) 2024-present VoidZero Inc. & Contributors.

use oxc_ast::{AstKind, ast::Expression};
use oxc_diagnostics::OxcDiagnostic;
use crate::js::declare_rule;
use oxc_span::Span;

use crate::js::{AstNode, context::LintContext, rule::Rule};

fn avoid_new_promise_diagnostic(span: Span) -> OxcDiagnostic {
    OxcDiagnostic::warn("Avoid creating new promises")
        .with_help(
            "Use `async`/`await` instead, or return an existing promise from a library function.",
        )
        .with_label(span)
}

#[derive(Debug, Default, Clone)]
pub struct AvoidNew;

declare_rule!(
    /// ### What it does
    ///
    /// Disallow creating promises with `new Promise()`.
    ///
    /// ### Why is this bad?
    ///
    /// Many cases that use `new Promise()` could be refactored to use an
    /// `async` function. `async` is considered more idiomatic in modern JavaScript.
    ///
    /// ### Examples
    ///
    /// Examples of **incorrect** code for this rule:
    /// ```javascript
    /// function foo() {
    ///     return new Promise((resolve, reject) => { /* ... */ });
    /// }
    /// ```
    ///
    /// Examples of **correct** code for this rule:
    /// ```javascript
    /// async function foo() {
    ///     // ...
    /// }
    /// const bar = await Promise.all([baz(), bang()]);
    /// ```
    AvoidNew,
    "promise",
    "avoid-new",
    Style,
    recommended = false,
    fix = RuleFixMeta::None,
);

impl Rule for AvoidNew {
    fn run<'a>(&self, node: &AstNode<'a>, ctx: &LintContext<'a>) {
        let AstKind::NewExpression(expr) = node.kind() else {
            return;
        };

        let Expression::Identifier(ident) = &expr.callee else {
            return;
        };

        if ident.name == "Promise" && ctx.is_reference_to_global_variable(ident) {
            ctx.diagnostic(avoid_new_promise_diagnostic(expr.span));
        }
    }
}

#[test]
fn test() {
    use crate::js::tester::Tester;

    let pass = vec![
        "Promise.resolve()",
        "Promise.reject()",
        "Promise.all()",
        "new Horse()",
        "new PromiseLikeThing()",
        "new Promise.resolve()",
        "var Promise = a; new Promise()",
        "function f(FakePromise) { const Promise = FakePromise; new Promise(function (r) { r() }) }; Promise.resolve()",
    ];

    let fail = vec![
        "var x = new Promise(function (x, y) {})",
        "new Promise()",
        "Thing(new Promise(() => {}))",
    ];

    Tester::new(AvoidNew::NAME, AvoidNew::PLUGIN, pass, fail).test();
}
