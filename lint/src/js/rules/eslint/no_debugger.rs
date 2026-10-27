// Ported from oxlint (crates/oxc_linter/src/rules/eslint/no_debugger.rs) at oxc crates_v0.155.0, MIT License, Copyright (c) 2024-present VoidZero Inc. & Contributors.

use oxc_ast::AstKind;
use oxc_diagnostics::OxcDiagnostic;
use crate::js::declare_rule;
use oxc_span::Span;

use crate::js::{AstNode, ast_util::outermost_paren_parent, context::LintContext, rule::Rule};

fn no_debugger_diagnostic(span: Span) -> OxcDiagnostic {
    OxcDiagnostic::warn("`debugger` statement is not allowed").with_label(span)
}

const REMOVE_DEBUGGER: &str = "Remove the debugger statement";

#[derive(Debug, Default, Clone)]
pub struct NoDebugger;

declare_rule!(
    /// ### What it does
    ///
    /// Checks for usage of the `debugger` statement.
    ///
    /// ### Why is this bad?
    ///
    /// `debugger` statements do not affect functionality when a debugger isn't attached.
    /// They're most commonly an accidental debugging leftover.
    ///
    /// ### Examples
    ///
    /// Examples of **incorrect** code for this rule:
    /// ```javascript
    /// async function main() {
    ///   const data = await getData();
    ///   const result = complexCalculation(data);
    ///   debugger;
    /// }
    /// ```
    ///
    /// Examples of **correct** code for this rule:
    /// ```javascript
    /// async function main() {
    ///   const data = await getData();
    ///   const result = complexCalculation(data);
    /// }
    /// ```
    NoDebugger,
    "eslint",
    "no-debugger",
    Correctness,
    recommended = true,
    fix = RuleFixMeta::Fixable(FixKind::Suggestion),
);

impl Rule for NoDebugger {
    fn run<'a>(&self, node: &AstNode<'a>, ctx: &LintContext<'a>) {
        if let AstKind::DebuggerStatement(stmt) = node.kind() {
            // mark the fix as a suggestion, so that it won't be applied automatically by the editor auto-fix on save feature
            ctx.diagnostic_with_suggestion(no_debugger_diagnostic(stmt.span), |fixer| {
                let Some(parent) = outermost_paren_parent(node, ctx) else {
                    return fixer.delete(&stmt.span).with_message(REMOVE_DEBUGGER);
                };

                // For statements like `if (foo) debugger;`, we can't just
                // delete the `debugger` statement; we need to replace it with an empty block.
                match parent.kind() {
                    AstKind::IfStatement(_)
                    | AstKind::WhileStatement(_)
                    | AstKind::ForStatement(_)
                    | AstKind::ForInStatement(_)
                    | AstKind::ForOfStatement(_) => {
                        fixer.replace(stmt.span, "{}").with_message(REMOVE_DEBUGGER)
                    }
                    // NOTE: no need to check for
                    // AstKind::ArrowFunctionExpression because
                    // `const x = () => debugger` is a parse error
                    _ => fixer.delete(&stmt.span).with_message(REMOVE_DEBUGGER),
                }
            });
        }
    }
}

#[test]
fn test() {
    use crate::js::tester::Tester;

    let pass = vec!["var test = { debugger: 1 }; test.debugger;"];

    let fail = vec!["if (foo) debugger"];
    let fix = vec![
        ("let x; debugger; let y;", "let x;  let y;"),
        ("if (foo) debugger", "if (foo) {}"),
        ("for (;;) debugger", "for (;;) {}"),
        ("while (i > 0) debugger", "while (i > 0) {}"),
        ("if (foo) { debugger; }", "if (foo) {  }"),
        ("if (foo) { debugger }", "if (foo) {  }"),
    ];

    Tester::new(NoDebugger::NAME, NoDebugger::PLUGIN, pass, fail)
        .expect_fix(fix)
        .test();
}
