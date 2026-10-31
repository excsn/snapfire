// Ported from oxlint (crates/oxc_linter/src/rules/unicorn/no_this_assignment.rs) at oxc crates_v0.155.0, MIT License, Copyright (c) 2024-present VoidZero Inc. & Contributors.

use oxc_ast::{
    AstKind,
    ast::{AssignmentTarget, BindingPattern, Expression},
};
use oxc_diagnostics::OxcDiagnostic;
use crate::js::declare_rule;
use oxc_span::Span;

use crate::js::{AstNode, context::LintContext, rule::Rule};

fn no_this_assignment_diagnostic(span: Span, ident_name: &str) -> OxcDiagnostic {
    OxcDiagnostic::warn(format!("Do not assign `this` to `{ident_name}`"))
        .with_help("Reference `this` directly instead of assigning it to a variable.")
        .with_label(span)
}

#[derive(Debug, Default, Clone)]
pub struct NoThisAssignment;

declare_rule!(
    /// ### What it does
    ///
    /// Disallow assigning `this` to a variable.
    ///
    /// ### Why is this bad?
    ///
    /// Assigning `this` to a variable is unnecessary and confusing.
    ///
    /// ### Examples
    ///
    /// Examples of **incorrect** code for this rule:
    /// ```javascript
    /// const foo = this;
    /// class Bar {
    ///     method() {
    ///         foo.baz();
    ///     }
    /// }
    ///
    /// new Bar().method();
    /// ```
    ///
    /// Examples of **correct** code for this rule:
    /// ```javascript
    /// class Bar {
    ///     constructor(fooInstance) {
    ///         this.fooInstance = fooInstance;
    ///     }
    ///     method() {
    ///         this.fooInstance.baz();
    ///     }
    /// }
    ///
    /// new Bar(this).method();
    /// ```
    NoThisAssignment,
    "unicorn",
    "no-this-assignment",
    Pedantic,
    recommended = true,
    fix = RuleFixMeta::None,
);

impl Rule for NoThisAssignment {
    fn run<'a>(&self, node: &AstNode<'a>, ctx: &LintContext<'a>) {
        match node.kind() {
            AstKind::VariableDeclarator(variable_decl) => {
                let Some(init) = &variable_decl.init else {
                    return;
                };

                if !matches!(init.without_parentheses(), Expression::ThisExpression(_)) {
                    return;
                }

                let BindingPattern::BindingIdentifier(binding_ident) = &variable_decl.id else {
                    return;
                };

                ctx.diagnostic(no_this_assignment_diagnostic(
                    variable_decl.span,
                    binding_ident.name.as_str(),
                ));
            }
            AstKind::AssignmentExpression(assignment_expr) => {
                if !matches!(
                    assignment_expr.right.without_parentheses(),
                    Expression::ThisExpression(_)
                ) {
                    return;
                }

                let AssignmentTarget::AssignmentTargetIdentifier(ident) = &assignment_expr.left
                else {
                    return;
                };

                ctx.diagnostic(no_this_assignment_diagnostic(
                    assignment_expr.span,
                    ident.name.as_str(),
                ));
            }
            _ => {}
        }
    }
}

#[test]
fn test() {
    use crate::js::tester::Tester;

    let pass = vec![
        "const {property} = this;",
        "const property = this.property;",
        "const [element] = this;",
        "const element = this[0];",
        "([element] = this);",
        "element = this[0];",
        "property = this.property;",
        "const [element] = [this];",
        "([element] = [this]);",
        "const {property} = {property: this};",
        "({property} = {property: this});",
        "const self = true && this;",
        "const self = false || this;",
        "const self = false ?? this;",
        "foo.bar = this;",
        "function foo(a = this) {}",
        "function foo({a = this}) {}",
        "function foo([a = this]) {}",
        "class A {
                foo = this;
            }",
        "class A {
                static foo = this;
            }",
    ];

    let fail = vec![
        "const foo = this;",
        "let foo;foo = this;",
        "var foo = bar, baz = this;",
        "var foo = (bar), baz = (this);",
    ];

    Tester::new(NoThisAssignment::NAME, NoThisAssignment::PLUGIN, pass, fail).test();
}
