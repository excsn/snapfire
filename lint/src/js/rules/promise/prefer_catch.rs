// Ported from oxlint (crates/oxc_linter/src/rules/promise/prefer_catch.rs) at oxc crates_v0.155.0, MIT License, Copyright (c) 2024-present VoidZero Inc. & Contributors.

use oxc_ast::{AstKind, ast::Expression};
use oxc_diagnostics::OxcDiagnostic;
use crate::js::declare_rule;
use oxc_span::Span;

use crate::js::{AstNode, context::LintContext, rule::Rule};

fn prefer_catch_diagnostic(span: Span) -> OxcDiagnostic {
    OxcDiagnostic::warn("Prefer `catch` to `then(a, b)` or `then(null, b)`")
        .with_help(
            "Handle promise errors in a `catch` instead of using the second argument of `then`.",
        )
        .with_label(span)
}

#[derive(Debug, Default, Clone)]
pub struct PreferCatch;

declare_rule!(
    /// ### What it does
    ///
    /// Prefer `catch` to `then(a, b)` and `then(null, b)`. This rule disallows the passing of an
    /// argument into the second parameter of `then` calls for handling promise errors.
    ///
    /// ### Why is this bad?
    ///
    /// A `then` call with two arguments can make it more difficult to recognize that a catch error
    /// handler is present. Another issue with using the second argument in `then` calls is that
    /// the ordering of promise error handling is less obvious.
    ///
    /// For example on first glance it may appear that `prom.then(fn1, fn2)` is equivalent to
    /// `prom.then(fn1).catch(fn2)`. However they aren't equivalent. In fact
    /// `prom.catch(fn2).then(fn1)` is the equivalent. This kind of confusion is a good reason for
    /// preferring explicit `catch` calls over passing an argument to the second parameter of
    /// `then` calls.
    ///
    /// ### Examples
    ///
    /// Examples of **incorrect** code for this rule:
    /// ```js
    /// prom.then(fn1, fn2)
    ///
    /// prom.then(null, fn2)
    /// ```
    ///
    /// Examples of **correct** code for this rule:
    /// ```js
    /// prom.catch(fn2).then(fn1)
    ///
    /// prom.catch(fn2)
    /// ```
    PreferCatch,
    "promise",
    "prefer-catch",
    Style,
    recommended = false,
    fix = RuleFixMeta::FixPending,
);

impl Rule for PreferCatch {
    fn run<'a>(&self, node: &AstNode<'a>, ctx: &LintContext<'a>) {
        let AstKind::ExpressionStatement(expr) = node.kind() else {
            return;
        };

        let Expression::CallExpression(ref call_expr) = expr.expression else {
            return;
        };

        let Some(member_expr) = call_expr.callee.get_member_expr() else {
            return;
        };

        let is_promise_then_call =
            member_expr.static_property_name().is_some_and(|prop_name| prop_name == "then");

        if is_promise_then_call && call_expr.arguments.len() >= 2 {
            ctx.diagnostic(prefer_catch_diagnostic(call_expr.span));
        }
    }
}

#[test]
fn test() {
    use crate::js::tester::Tester;

    let pass = vec![
        "prom.then()",
        "prom.then(fn)",
        "prom.then(fn1).then(fn2)",
        "prom.then(() => {})",
        "prom.then(function () {})",
        "prom.catch()",
        "prom.catch(handleErr).then(handle)",
        "prom.catch(handleErr)",
    ];

    let fail = vec![
        "prom.then(fn1, fn2)",
        "prom.then(fn1, (fn2))",
        "prom.then(null, fn2)",
        "prom.then(undefined, fn2)",
        "function foo() { prom.then(x => {}, () => {}) }",
        "function foo() {
		   prom.then(function a() { }, function b() {}).then(fn1, fn2)
	     }",
    ];

    /* Pending
    let fix = vec![
        ("prom.then(fn1, fn2)", "prom.catch(fn2).then(fn1)", None),
        ("prom.then(fn1, (fn2))", "prom.catch(fn2).then(fn1)", None),
        ("prom.then(null, fn2)", "prom.catch(fn2)", None),
        ("prom.then(undefined, fn2)", "prom.catch(fn2)", None),
        (
            "function foo() { prom.then(x => {}, () => {}) }",
            "function foo() { prom.catch(() => {}).then(x => {}) }",
            None,
        ),
        (
            "function foo() {
			   prom.then(function a() { }, function b() {}).then(fn1, fn2)
			 }",
            "function foo() {
			   prom.catch(function b() {}).then(function a() { }).catch(fn2).then(fn1)
			 }",
            None,
        ),
    ];
    */
    Tester::new(PreferCatch::NAME, PreferCatch::PLUGIN, pass, fail)
        //    .expect_fix(fix)
        .test();
}
