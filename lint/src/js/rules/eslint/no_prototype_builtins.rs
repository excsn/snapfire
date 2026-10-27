// Ported from oxlint (crates/oxc_linter/src/rules/eslint/no_prototype_builtins.rs) at oxc crates_v0.155.0, MIT License, Copyright (c) 2024-present VoidZero Inc. & Contributors.

use oxc_ast::AstKind;
use oxc_diagnostics::OxcDiagnostic;
use crate::js::declare_rule;
use oxc_span::{GetSpan, Span};
use oxc_str::JSStr;

use crate::js::{AstNode, context::LintContext, rule::Rule};

fn no_prototype_builtins_diagnostic(method_name: &str, span: Span) -> OxcDiagnostic {
    OxcDiagnostic::warn(format!(
        "do not access Object.prototype method {method_name:?} from target object"
    ))
    .with_help(format!(
        "to avoid prototype pollution, use `Object.prototype.{method_name}.call` instead"
    ))
    .with_label(span)
}

#[derive(Debug, Default, Clone)]
pub struct NoPrototypeBuiltins;

declare_rule!(
    /// ### What it does
    ///
    /// Disallow calling some `Object.prototype` methods directly on objects.
    ///
    /// ### Why is this bad?
    ///
    /// In ECMAScript 5.1, `Object.create` was added, which enables the creation of objects with a specified [[Prototype]].
    /// `Object.create(null)` is a common pattern used to create objects that will be used as a Map.
    /// This can lead to errors when it is assumed that objects will have properties from `Object.prototype`. This rule prevents calling some `Object.prototype` methods directly from an object.
    /// Additionally, objects can have properties that shadow the builtins on `Object.prototype`, potentially causing unintended behavior or denial-of-service security vulnerabilities.
    /// For example, it would be unsafe for a webserver to parse JSON input from a client and call `hasOwnProperty` directly on the resulting object, because a malicious client could send a JSON value like {"hasOwnProperty": 1} and cause the server to crash.
    ///
    /// To avoid subtle bugs like this, it’s better to always call these methods from `Object.prototype`. For example, `foo.hasOwnProperty("bar")` should be replaced with `Object.prototype.hasOwnProperty.call(foo, "bar")`.
    ///
    /// ### Examples
    ///
    /// Examples of **incorrect** code for this rule:
    /// ```javascript
    /// var hasBarProperty = foo.hasOwnProperty("bar");
    /// var isPrototypeOfBar = foo.isPrototypeOf(bar);
    /// var barIsEnumerable = foo.propertyIsEnumerable("bar");
    /// ```
    NoPrototypeBuiltins,
    "eslint",
    "no-prototype-builtins",
    Pedantic,
    recommended = true,
    fix = RuleFixMeta::FixPending,
);

const DISALLOWED_PROPS: &[&str; 3] = &["hasOwnProperty", "isPrototypeOf", "propertyIsEnumerable"];

impl Rule for NoPrototypeBuiltins {
    fn run<'a>(&self, node: &AstNode<'a>, ctx: &LintContext<'a>) {
        let AstKind::CallExpression(expr) = node.kind() else {
            return;
        };
        let Some(member_expr) = expr.callee.get_member_expr() else {
            return;
        };
        let Some(prop_name) = member_expr.static_property_name().and_then(JSStr::as_str) else {
            return;
        };
        if DISALLOWED_PROPS.contains(&prop_name) {
            ctx.diagnostic(no_prototype_builtins_diagnostic(prop_name, member_expr.span()));
        }
    }
}

#[test]
fn test() {
    use crate::js::tester::Tester;

    let pass = vec![
        "Object.prototype.hasOwnProperty.call(foo, 'bar')",
        "Object.prototype.isPrototypeOf.call(foo, 'bar')",
        "Object.prototype.propertyIsEnumerable.call(foo, 'bar')",
        "Object.prototype.hasOwnProperty.apply(foo, ['bar'])",
        "Object.prototype.isPrototypeOf.apply(foo, ['bar'])",
        "Object.prototype.propertyIsEnumerable.apply(foo, ['bar'])",
        "foo.hasOwnProperty",
        "foo.hasOwnProperty.bar()",
        "foo(hasOwnProperty)",
        "hasOwnProperty(foo, 'bar')",
        "isPrototypeOf(foo, 'bar')",
        "propertyIsEnumerable(foo, 'bar')",
        "({}.hasOwnProperty.call(foo, 'bar'))",
        "({}.isPrototypeOf.call(foo, 'bar'))",
        "({}.propertyIsEnumerable.call(foo, 'bar'))",
        "({}.hasOwnProperty.apply(foo, ['bar']))",
        "({}.isPrototypeOf.apply(foo, ['bar']))",
        "({}.propertyIsEnumerable.apply(foo, ['bar']))",
        "foo[hasOwnProperty]('bar')",
        "foo['HasOwnProperty']('bar')",
        "foo[`isPrototypeOff`]('bar')", // { "ecmaVersion": 2015 },
        "foo?.['propertyIsEnumerabl']('bar')", // { "ecmaVersion": 2020 },
        "foo[1]('bar')",
        "foo[null]('bar')",
        "class C { #hasOwnProperty; foo() { obj.#hasOwnProperty('bar'); } }", // { "ecmaVersion": 2022 },
        "foo['hasOwn' + 'Property']('bar')",
        "foo[`hasOwnProperty${''}`]('bar')", // { "ecmaVersion": 2015 }
    ];

    let fail = vec![
        "foo.hasOwnProperty('bar')",
        "foo.isPrototypeOf('bar')",
        "foo.propertyIsEnumerable('bar')",
        "foo.bar.hasOwnProperty('bar')",
        "foo.bar.baz.isPrototypeOf('bar')",
        "foo['hasOwnProperty']('bar')",
        "foo[`isPrototypeOf`]('bar').baz", // { "ecmaVersion": 2015 },
        r#"foo.bar["propertyIsEnumerable"]('baz')"#,
        "(function(Object) {return foo.hasOwnProperty('bar');})",
        "foo.hasOwnProperty('bar')", // { "globals": { "Object": "off", }, },
        "foo?.hasOwnProperty('bar')", // { "ecmaVersion": 2020 },
        "foo?.bar.hasOwnProperty('baz')", // { "ecmaVersion": 2020 },
        "foo.hasOwnProperty?.('bar')", // { "ecmaVersion": 2020 },
        "foo?.hasOwnProperty('bar').baz", // { "ecmaVersion": 2020 },
        "foo.hasOwnProperty('bar')?.baz", // { "ecmaVersion": 2020 },
        "(a,b).hasOwnProperty('bar')", // { "ecmaVersion": 2020 },
        "(foo?.hasOwnProperty)('bar')", // { "ecmaVersion": 2020 },
        "(foo?.hasOwnProperty)?.('bar')", // { "ecmaVersion": 2020 },
        "foo?.['hasOwnProperty']('bar')", // { "ecmaVersion": 2020 },
        "(foo?.[`hasOwnProperty`])('bar')", // { "ecmaVersion": 2020 }
    ];

    Tester::new(NoPrototypeBuiltins::NAME, NoPrototypeBuiltins::PLUGIN, pass, fail)
        .test();
}
