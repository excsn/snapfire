// Ported from oxlint (crates/oxc_linter/src/rules/unicorn/consistent_template_literal_escape.rs) at oxc crates_v0.155.0, MIT License, Copyright (c) 2024-present VoidZero Inc. & Contributors.

use oxc_ast::AstKind;
use oxc_diagnostics::OxcDiagnostic;
use crate::js::declare_rule;
use oxc_span::Span;

use crate::js::{AstNode, context::LintContext, rule::Rule};

fn consistent_template_literal_escape_diagnostic(span: Span) -> OxcDiagnostic {
    OxcDiagnostic::warn("Invalid escape sequence in template literal.")
        .with_help("Use '\\${' to escape '${' in template literals.")
        .with_label(span)
}

#[derive(Debug, Default, Clone)]
pub struct ConsistentTemplateLiteralEscape;

declare_rule!(
    /// ### What it does
    ///
    /// Enforce consistent style for escaping `${` in template literals.
    ///
    /// ### Why is this bad?
    /// Using `\${` instead of `${` can improve readability and prevent confusion.
    ///
    /// ### Examples
    ///
    /// Examples of **incorrect** code for this rule:
    /// ```js
    ///const foo = `$\{a}`;
    /// ```
    ///
    /// ```js
    ///const foo = `\$\{a}`;
    /// ```
    ///
    /// Examples of **correct** code for this rule:
    /// ```js
    /// const foo = `\${a}`;
    /// ```
    ConsistentTemplateLiteralEscape,
    "unicorn",
    "consistent-template-literal-escape",
    Style,
    recommended = false,
    fix = RuleFixMeta::Fixable(FixKind::SafeFix),
);

impl Rule for ConsistentTemplateLiteralEscape {
    fn run<'a>(&self, node: &AstNode<'a>, ctx: &LintContext<'a>) {
        let AstKind::TemplateLiteral(template_literal) = node.kind() else { return };

        if let AstKind::TaggedTemplateExpression(_) =
            ctx.nodes().parent_kind(template_literal.node_id())
        {
            return;
        }

        for quasis in &template_literal.quasis {
            let value = quasis.value.raw.as_ref();
            for (start, _) in value.match_indices("$\\{") {
                let backslash_count =
                    value.as_bytes()[..start].iter().rev().take_while(|&&b| b == b'\\').count();
                let start = if backslash_count % 2 == 1 { start - 1 } else { start };
                let end = start + if backslash_count % 2 == 1 { 4 } else { 3 };
                let start = u32::try_from(start).expect("Unable to convert to u32");
                let end = u32::try_from(end).expect("Unable to convert to u32");
                let error_span = Span::new(quasis.span.start + start, quasis.span.start + end);

                ctx.diagnostic_with_fix(
                    consistent_template_literal_escape_diagnostic(error_span),
                    |fixer| fixer.replace(error_span, "\\${"),
                );
            }
        }
    }
}

#[test]
fn test() {
    use crate::js::tester::Tester;

    let pass = vec![
        r"const foo = `\${a}`",
        "const foo = `hello`",
        "const foo = `$`",
        "const foo = `{`",
        "const foo = ``",
        "const foo = `${a}`",
        "const foo = `${a}${b}`",
        r"const foo = String.raw`$\{a}`",
        r"const foo = html`$\{a}`",
        r"const foo = `\\\${a}`",
        r"const foo = '$\{a}'",
        r"const foo = `ééé\${a}`",
    ];

    let fail = vec![
        r"const foo = `$\{a}`",
        r"const foo = `\$\{a}`",
        r"const foo = `$\{a} and $\{b}`",
        r"const foo = `\\$\{a}`",
        r"const foo = `\\\$\{a}`",
        r"const foo = `$\{a}${expr}`",
        r"const foo = `${expr}$\{a}`",
        r"const foo = `$\{a}${expr}$\{b}`",
        r"const foo = `ééé$\{a}`",
        r"const foo = `ééé\\$\{a}`",
    ];
    let fix = vec![
        (r"const foo = `$\{a}`", r"const foo = `\${a}`"),
        (r"const foo = `\$\{a}`", r"const foo = `\${a}`"),
        (r"const foo = `$\{a} and $\{b}`", r"const foo = `\${a} and \${b}`"),
        (r"const foo = `\\$\{a}`", r"const foo = `\\\${a}`"),
        (r"const foo = `\\\$\{a}`", r"const foo = `\\\${a}`"),
        (r"const foo = `$\{a}${expr}`", r"const foo = `\${a}${expr}`"),
        (r"const foo = `${expr}$\{a}`", r"const foo = `${expr}\${a}`"),
        (r"const foo = `$\{a}${expr}$\{b}`", r"const foo = `\${a}${expr}\${b}`"),
        (r"const foo = `ééé$\{a}`", r"const foo = `ééé\${a}`"),
        (r"const foo = `ééé\\$\{a}`", r"const foo = `ééé\\\${a}`"),
    ];

    Tester::new(
        ConsistentTemplateLiteralEscape::NAME,
        ConsistentTemplateLiteralEscape::PLUGIN,
        pass,
        fail,
    )
    .expect_fix(fix)
    .test();
}
