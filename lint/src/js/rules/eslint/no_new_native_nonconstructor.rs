// Ported from oxlint (crates/oxc_linter/src/rules/eslint/no_new_native_nonconstructor.rs) at oxc crates_v0.155.0, MIT License, Copyright (c) 2024-present VoidZero Inc. & Contributors.

use oxc_ast::{AstKind, ast::Expression};
use oxc_diagnostics::OxcDiagnostic;
use crate::js::declare_rule;
use oxc_span::Span;

use crate::js::{AstNode, context::LintContext, rule::Rule};

fn no_new_native_nonconstructor_diagnostic(fn_name: &str, span: Span) -> OxcDiagnostic {
    OxcDiagnostic::warn(format!("`{fn_name}` cannot be called as a constructor."))
        .with_help(format!("Remove the `new` operator to call `{fn_name}` as a function."))
        .with_label(span)
}

#[derive(Debug, Default, Clone)]
pub struct NoNewNativeNonconstructor;

declare_rule!(
    /// ### What it does
    ///
    /// Disallow `new` operators with global non-constructor functions (`Symbol`, `BigInt`).
    ///
    /// This rule can be disabled for TypeScript code, as the TypeScript compiler
    /// enforces this check.
    ///
    /// ### Why is this bad?
    ///
    /// Both `new Symbol` and `new BigInt` throw a type error because they are
    /// functions and not classes.  It is easy to make this mistake by assuming
    /// the uppercase letters indicate classes.
    ///
    /// ### Examples
    ///
    /// Examples of **incorrect** code for this rule:
    /// ```js
    /// // throws a TypeError
    /// let foo = new Symbol("foo");
    ///
    /// // throws a TypeError
    /// let result = new BigInt(9007199254740991);
    /// ```
    ///
    /// Examples of **correct** code for this rule:
    /// ```js
    /// let foo = Symbol("foo");
    ///
    /// let result = BigInt(9007199254740991);
    /// ```
    NoNewNativeNonconstructor,
    "eslint",
    "no-new-native-nonconstructor",
    Correctness,
    recommended = true,
    fix = RuleFixMeta::None,
);

impl Rule for NoNewNativeNonconstructor {
    fn run<'a>(&self, node: &AstNode<'a>, ctx: &LintContext<'a>) {
        let AstKind::NewExpression(expr) = node.kind() else {
            return;
        };
        let Expression::Identifier(ident) = &expr.callee else {
            return;
        };
        if matches!(ident.name.as_str(), "Symbol" | "BigInt")
            && ctx.is_reference_to_global_variable(ident)
        {
            let start = expr.span.start;
            let end = start + 3;
            ctx.diagnostic(no_new_native_nonconstructor_diagnostic(
                ident.name.as_str(),
                Span::new(start, end),
            ));
        }
    }
}

#[test]
fn test() {
    use crate::js::tester::Tester;

    let pass = vec![
        "var foo = Symbol('foo');",
        "function bar(Symbol) { var baz = new Symbol('baz');}",
        "function Symbol() {} new Symbol();",
        "new foo(Symbol);",
        "new foo(bar, Symbol);",
        "var foo = BigInt(9007199254740991);",
        "function bar(BigInt) { var baz = new BigInt(9007199254740991);}",
        "function BigInt() {} new BigInt();",
        "new foo(BigInt);",
        "new foo(bar, BigInt);",
        "function f(FakeSymbol) { const Symbol = FakeSymbol; new Symbol(); }; Symbol('foo');",
    ];

    let fail = vec![
        "var foo = new Symbol('foo');",
        "function bar() { return function Symbol() {}; } var baz = new Symbol('baz');",
        "var foo = new BigInt(9007199254740991);",
        "function bar() { return function BigInt() {}; } var baz = new BigInt(9007199254740991);",
    ];

    Tester::new(NoNewNativeNonconstructor::NAME, NoNewNativeNonconstructor::PLUGIN, pass, fail)
        .test();
}
