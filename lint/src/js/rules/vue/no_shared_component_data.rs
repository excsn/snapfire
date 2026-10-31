// Ported from oxlint (crates/oxc_linter/src/rules/vue/no_shared_component_data.rs) at oxc crates_v0.155.0, MIT License, Copyright (c) 2024-present VoidZero Inc. & Contributors.

use oxc_ast::{AstKind, ast::Expression};
use oxc_diagnostics::OxcDiagnostic;
use crate::js::declare_rule;
use oxc_span::Span;

use crate::js::{
    AstNode, context::LintContext, rule::Rule,
    utils::is_vue_component_options_object_excluding_instance,
};

fn no_shared_component_data_diagnostic(span: Span) -> OxcDiagnostic {
    OxcDiagnostic::warn("`data` property in component must be a function.").with_label(span)
}

#[derive(Debug, Default, Clone)]
pub struct NoSharedComponentData;

declare_rule!(
    /// ### What it does
    ///
    /// Enforce that the `data` property of a Vue component definition is a
    /// function.
    ///
    /// ### Why is this bad?
    ///
    /// When `data` is declared as an object literal, the same object is shared
    /// across every instance of the component, which causes cross-instance
    /// state pollution. Returning a fresh object from a function avoids that.
    ///
    /// This rule targets component definitions reached through
    /// `Vue.component(...)`, `Vue.extend(...)`, `Vue.mixin(...)`,
    /// `app.component(...)`, `app.mixin(...)`, `component(...)`,
    /// `createApp(...)`, `defineComponent(...)`, `defineNuxtComponent(...)`,
    /// and `export default {}` inside `.vue` files. `new Vue({...})` is not
    /// covered (the instance does not share `data` between components).
    ///
    /// ### Examples
    ///
    /// Examples of **incorrect** code for this rule:
    /// ```vue
    /// <script>
    /// Vue.component('some-comp', {
    ///   data: {
    ///     foo: 'bar'
    ///   }
    /// })
    /// </script>
    /// ```
    ///
    /// Examples of **correct** code for this rule:
    /// ```vue
    /// <script>
    /// Vue.component('some-comp', {
    ///   data() {
    ///     return { foo: 'bar' }
    ///   }
    /// })
    /// </script>
    /// ```
    NoSharedComponentData,
    "vue",
    "no-shared-component-data",
    Correctness,
    recommended = true,
    fix = RuleFixMeta::FixPending,
);

impl Rule for NoSharedComponentData {
    fn run<'a>(&self, node: &AstNode<'a>, ctx: &LintContext<'a>) {
        let AstKind::ObjectProperty(prop) = node.kind() else { return };

        if !prop.key.is_specific_static_name("data") {
            return;
        }

        if !matches!(prop.value.get_inner_expression(), Expression::ObjectExpression(_)) {
            return;
        }

        let parent = ctx.nodes().parent_node(node.id());
        if !matches!(parent.kind(), AstKind::ObjectExpression(_)) {
            return;
        }

        if !is_vue_component_options_object_excluding_instance(parent, ctx) {
            return;
        }

        ctx.diagnostic(no_shared_component_data_diagnostic(prop.span));
    }
}

#[test]
fn test() {
    use std::path::PathBuf;

    use crate::js::tester::Tester;

    let pass = vec![
        // `new Vue({...})` is excluded — instances don't share `data` across components.
        (
            "
                new Vue({
                  data: {
                    foo: 'bar'
                  }
                })
            ",
            None,
            None,
            Some(PathBuf::from("test.js")),
        ),
        (
            "
                new Vue({
                  data: function () {
                    return { foo: 'bar' }
                  }
                })
            ",
            None,
            None,
            Some(PathBuf::from("test.js")),
        ),
        (
            "
                new Vue({
                  ...data,
                  data () {
                    return { foo: 'bar' }
                  }
                })
            ",
            None,
            None,
            Some(PathBuf::from("test.js")),
        ),
        (
            "
                Vue.component('some-comp', {
                  data: function () {
                    return { foo: 'bar' }
                  }
                })
            ",
            None,
            None,
            Some(PathBuf::from("test.js")),
        ),
        (
            "
                export default {
                  data: {
                    foo: 'bar'
                  }
                }
            ",
            None,
            None,
            Some(PathBuf::from("test.js")),
        ),
        (
            "
                Vue.component({
                  data: {
                    foo: 'bar'
                  }
                }, 'some-comp')
            ",
            None,
            None,
            Some(PathBuf::from("test.js")),
        ),
        (
            "
                Vue.component({
                  data: {
                    foo: 'bar'
                  }
                }, {})
            ",
            None,
            None,
            Some(PathBuf::from("test.js")),
        ),
        (
            "
                <script>
                export default {
                  data: function () {
                    return { foo: 'bar' }
                  }
                }
                </script>
            ",
            None,
            None,
            Some(PathBuf::from("test.vue")),
        ),
        (
            "
                <script>
                export default {
                  ...foo
                }
                </script>
            ",
            None,
            None,
            Some(PathBuf::from("test.vue")),
        ),
        (
            "
                <script>
                export default {
                  data
                }
                </script>
            ",
            None,
            None,
            Some(PathBuf::from("test.vue")),
        ),
        (
            "
                <script>
                export default {
                  data: () => ({ foo: 'bar' })
                }
                </script>
            ",
            None,
            None,
            Some(PathBuf::from("test.vue")),
        ),
        (
            "
                <script>
                defineComponent({
                  data () { return { foo: 'bar' } }
                })
                </script>
            ",
            None,
            None,
            Some(PathBuf::from("test.vue")),
        ),
        (
            "
                <template>
                  {{ Vue.component('some-comp', { data: { foo: 'bar' } }) }}
                </template>
            ",
            None,
            None,
            Some(PathBuf::from("test.vue")),
        ),
    ];

    let fail = vec![
        (
            "
                Vue.component('some-comp', {
                  data: {
                    foo: 'bar'
                  }
                })
            ",
            None,
            None,
            Some(PathBuf::from("test.js")),
        ),
        (
            "
                app.component('some-comp', {
                  data: {
                    foo: 'bar'
                  }
                })
            ",
            None,
            None,
            Some(PathBuf::from("test.js")),
        ),
        (
            "
                <script>
                export default {
                  data: {
                    foo: 'bar'
                  }
                }
                </script>
            ",
            None,
            None,
            Some(PathBuf::from("test.vue")),
        ),
        (
            "
                <script>
                export default {
                  data: /*a*/ (/*b*/{
                    foo: 'bar'
                  })
                }
                </script>
            ",
            None,
            None,
            Some(PathBuf::from("test.vue")),
        ),
        (
            "
                <script>
                Vue.extend({
                  data: { foo: 'bar' }
                })
                </script>
            ",
            None,
            None,
            Some(PathBuf::from("test.vue")),
        ),
        (
            "
                <script>
                createApp({
                  data: { foo: 'bar' }
                })
                </script>
            ",
            None,
            None,
            Some(PathBuf::from("test.vue")),
        ),
        (
            "
                <script>
                defineComponent({
                  data: { foo: 'bar' }
                })
                </script>
            ",
            None,
            None,
            Some(PathBuf::from("test.vue")),
        ),
        (
            "
                <script>
                Vue.mixin({
                  data: { foo: 'bar' }
                })
                </script>
            ",
            None,
            None,
            Some(PathBuf::from("test.vue")),
        ),
    ];

    Tester::new(NoSharedComponentData::NAME, NoSharedComponentData::PLUGIN, pass, fail)
        .test();
}
