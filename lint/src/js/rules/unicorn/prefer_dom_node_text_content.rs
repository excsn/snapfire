// Ported from oxlint (crates/oxc_linter/src/rules/unicorn/prefer_dom_node_text_content.rs) at oxc crates_v0.155.0, MIT License, Copyright (c) 2024-present VoidZero Inc. & Contributors.

use oxc_ast::{AstKind, ast::PropertyKey};
use oxc_diagnostics::OxcDiagnostic;
use crate::js::declare_rule;
use oxc_span::Span;

use crate::js::{AstNode, context::LintContext, rule::Rule};

fn prefer_dom_node_text_content_diagnostic(span: Span) -> OxcDiagnostic {
    OxcDiagnostic::warn("Prefer `.textContent` over `.innerText`.")
        .with_help("Replace `.innerText` with `.textContent`.")
        .with_label(span)
}

#[derive(Debug, Default, Clone)]
pub struct PreferDomNodeTextContent;

declare_rule!(
    /// ### What it does
    ///
    /// Enforces the use of `.textContent` over `.innerText` for DOM nodes.
    ///
    /// ### Why is this bad?
    ///
    /// There are some disadvantages of using `.innerText`.
    /// - `.innerText` returns rendered text and ignores hidden content, while `.textContent` returns the node's full text content.
    /// - `.innerText` can trigger reflow because it takes CSS styles into account.
    /// - `.innerText` is defined only for HTMLElement objects, while `.textContent` is defined for all Node objects.
    ///
    /// ### Examples
    ///
    /// Examples of **incorrect** code for this rule:
    /// ```javascript
    /// const text = foo.innerText;
    /// ```
    ///
    /// Examples of **correct** code for this rule:
    /// ```javascript
    /// const text = foo.textContent;
    /// ```
    PreferDomNodeTextContent,
    "unicorn",
    "prefer-dom-node-text-content",
    Style,
    recommended = true,
    fix = RuleFixMeta::Conditional(FixKind::Fix),
);

impl Rule for PreferDomNodeTextContent {
    fn run<'a>(&self, node: &AstNode<'a>, ctx: &LintContext<'a>) {
        match node.kind() {
            AstKind::StaticMemberExpression(member_expr) => {
                let (span, name) = member_expr.static_property_info();
                if name == "innerText" {
                    ctx.diagnostic_with_fix(
                        prefer_dom_node_text_content_diagnostic(span),
                        |fixer| fixer.replace(span, "textContent"),
                    );
                }
            }
            // `const {innerText} = node`, `const {innerText: text} = node`,
            // function params `function foo({innerText}) {}`, etc.
            AstKind::BindingProperty(prop) => {
                if let PropertyKey::StaticIdentifier(ident) = &prop.key
                    && ident.name == "innerText"
                {
                    ctx.diagnostic(prefer_dom_node_text_content_diagnostic(ident.span));
                }
            }
            // `({innerText: text} = node)`
            AstKind::AssignmentTargetPropertyProperty(prop) => {
                if let PropertyKey::StaticIdentifier(ident) = &prop.name
                    && ident.name == "innerText"
                {
                    ctx.diagnostic(prefer_dom_node_text_content_diagnostic(ident.span));
                }
            }
            // `({innerText} = node)` (shorthand)
            AstKind::AssignmentTargetPropertyIdentifier(prop) => {
                if prop.binding.name != "innerText" {
                    return;
                }

                // The parent is always an `ObjectAssignmentTarget`; only report when that
                // target is directly part of an assignment (or nested in another object
                // assignment target), matching the previous `IdentifierReference` logic.
                let object_target = ctx.nodes().parent_node(node.id());
                let grand_parent_node = ctx.nodes().parent_node(object_target.id());

                if matches!(
                    grand_parent_node.kind(),
                    AstKind::ExpressionStatement(_)
                        | AstKind::AssignmentExpression(_)
                        | AstKind::ObjectAssignmentTarget(_)
                ) {
                    ctx.diagnostic(prefer_dom_node_text_content_diagnostic(prop.binding.span));
                }
            }
            _ => {}
        }
    }
}

#[test]
fn test() {
    use crate::js::tester::Tester;

    let pass = vec![
        "innerText;",
        "node.textContent;",
        "node[innerText];",
        "innerText = true;",
        "node['innerText'];",
        "innerText.textContent",
        "const [innerText] = node;",
        "[innerText] = node;",
        "const {[innerText]: text} = node;",
        "({[innerText]: text} = node);",
        "const foo = {innerText}",
        "const foo = {innerText: text}",
    ];

    let fail = vec![
        "node.innerText;",
        "node?.innerText;",
        "node.innerText = 'foo';",
        "innerText.innerText;",
        "const {innerText} = node;",
        "const {innerText,} = node;",
        "const {innerText: text} = node;",
        r#"const {innerText = "default text"} = node;"#,
        r#"const {innerText: text = "default text"} = node;"#,
        "({innerText} = node);",
        "({innerText: text} = node);",
        r#"({innerText = "default text"} = node);"#,
        r#"({innerText: text = "default text"} = node);"#,
        "function foo({innerText}) {return innerText}",
        "for (const [{innerText}] of elements);",
    ];

    let fix = vec![
        ("node.innerText;", "node.textContent;"),
        ("node?.innerText;", "node?.textContent;"),
        ("node.innerText = 'foo';", "node.textContent = 'foo';"),
        ("innerText.innerText = 'foo';", "innerText.textContent = 'foo';"),
    ];

    Tester::new(PreferDomNodeTextContent::NAME, PreferDomNodeTextContent::PLUGIN, pass, fail)
        .expect_fix(fix)
        .test();
}
