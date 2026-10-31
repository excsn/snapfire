// Ported from oxlint (crates/oxc_linter/src/rules/vue/require_typed_ref.rs) at oxc crates_v0.155.0, MIT License, Copyright (c) 2024-present VoidZero Inc. & Contributors.

use oxc_ast::{AstKind, ast::Argument};
use oxc_diagnostics::OxcDiagnostic;
use crate::js::declare_rule;
use oxc_span::Span;

use crate::js::{AstNode, context::LintContext, rule::Rule};

fn require_typed_ref_diagnostic(span: Span, name: &str) -> OxcDiagnostic {
    let msg = format!(
        "Specify type parameter for `{name}` function, otherwise created variable will not be typechecked."
    );
    OxcDiagnostic::warn(msg)
        .with_help("Provide an explicit type parameter or an initial value.")
        .with_label(span)
}

#[derive(Debug, Default, Clone)]
pub struct RequireTypedRef;

declare_rule!(
    /// ### What it does
    ///
    /// Require `ref` and `shallowRef` functions to be strongly typed.
    ///
    /// ### Why is this bad?
    ///
    /// With TypeScript it is easy to prevent usage of `any` by using `noImplicitAny`.
    /// Unfortunately this rule is easily bypassed with Vue `ref()` function.
    /// Calling `ref()` function without a generic parameter or an initial value leads to ref having `Ref<any>` type.
    ///
    /// ### Examples
    ///
    /// Examples of **incorrect** code for this rule:
    /// ```typescript
    /// const count = ref();
    /// const name = shallowRef()
    /// ```
    ///
    /// Examples of **correct** code for this rule:
    /// ```typescript
    /// const count = ref<number>()
    /// const a = ref(0)
    /// ```
    RequireTypedRef,
    "vue",
    "require-typed-ref",
    Style,
    recommended = false,
    fix = RuleFixMeta::None,
);

impl Rule for RequireTypedRef {
    fn run<'a>(&self, node: &AstNode<'a>, ctx: &LintContext<'a>) {
        let AstKind::CallExpression(call_expr) = node.kind() else {
            return;
        };
        let Some(ident) = call_expr.callee.get_identifier_reference() else {
            return;
        };

        let name = ident.name;
        if name != "ref" && name != "shallowRef" {
            return;
        }

        let is_valid_first_arg = match call_expr.arguments.first() {
            Some(Argument::NullLiteral(_)) | None => false,
            Some(Argument::Identifier(ident)) if ident.name == "undefined" => false,
            _ => true,
        };

        if is_valid_first_arg {
            return;
        }

        if call_expr.type_arguments.is_none() {
            if let Some(variable_decl_parent) =
                ctx.nodes().ancestor_kinds(node.id()).find_map(|ancestor| {
                    if let AstKind::VariableDeclarator(var_decl) = ancestor {
                        Some(var_decl)
                    } else {
                        None
                    }
                })
                && variable_decl_parent.type_annotation.is_some()
            {
                return;
            }

            ctx.diagnostic(require_typed_ref_diagnostic(call_expr.span, &name));
        }
    }

    fn should_run(&self, ctx: &crate::js::context::ContextHost) -> bool {
        ctx.source_type().is_typescript()
    }
}

#[test]
fn test() {
    use crate::js::tester::Tester;
    use std::path::PathBuf;

    let pass = vec![
        (
            "
                    import { shallowRef } from 'vue'
                    const count = shallowRef(0)
                  ",
            None,
            None,
            Some(PathBuf::from("test.ts")),
        ),
        (
            "
                    import { ref } from 'vue'
                    const count = ref<number>()
                  ",
            None,
            None,
            Some(PathBuf::from("test.ts")),
        ),
        (
            "
                    import { ref } from 'vue'
                    const count = ref<number>(0)
                  ",
            None,
            None,
            Some(PathBuf::from("test.ts")),
        ),
        (
            "
                    import { ref } from 'vue'
                    const counter: Ref<number | undefined> = ref()
                  ",
            None,
            None,
            Some(PathBuf::from("test.ts")),
        ),
        (
            "
                    import { ref } from 'vue'
                    const count = ref(0)
                  ",
            None,
            None,
            Some(PathBuf::from("test.ts")),
        ),
        (
            "
                    import { ref } from 'vue'
                    function useCount() {
                      return {
                        count: ref<number>()
                      }
                    }
                  ",
            None,
            None,
            Some(PathBuf::from("test.ts")),
        ),
        (
            "
                  import { ref, defineComponent } from 'vue'
                  defineComponent({
                    setup() {
                      const count = ref<number>()
                      return { count }
                    }
                  })
                  ",
            None,
            None,
            Some(PathBuf::from("test.ts")),
        ),
        (
            "
                    <script setup>
                      import { ref } from 'vue'
                      const count = ref()
                    </script>
                  ",
            None,
            None,
            Some(PathBuf::from("test.vue")),
        ), // { "parser": require("vue-eslint-parser") },
        (
            "
                    <script>
                      import { ref } from 'vue'
                      export default {
                        setup() {
                          const count = ref()
                        }
                      }
                    </script>
                  ",
            None,
            None,
            Some(PathBuf::from("test.vue")),
        ), // { "parser": require("vue-eslint-parser") },
        (
            "
                    import { ref } from 'vue'
                    const count = ref()
                  ",
            None,
            None,
            Some(PathBuf::from("test.js")),
        ),
    ];

    let fail = vec![
        (
            "
                    import { ref } from 'vue'
                    const count = ref()
                  ",
            None,
            None,
            Some(PathBuf::from("test.ts")),
        ),
        (
            "
                    import { ref } from 'vue'
                    const count = ref(null)
                  ",
            None,
            None,
            Some(PathBuf::from("test.ts")),
        ),
        (
            "
                    import { ref } from 'vue'
                    const count = ref(undefined)
                  ",
            None,
            None,
            Some(PathBuf::from("test.ts")),
        ),
        (
            "
                    import { shallowRef } from 'vue'
                    const count = shallowRef()
                  ",
            None,
            None,
            Some(PathBuf::from("test.ts")),
        ),
        (
            "
                    import { ref } from 'vue'
                    function useCount() {
                      const count = ref()
                      return { count }
                    }
                  ",
            None,
            None,
            Some(PathBuf::from("test.ts")),
        ),
        (
            "
                    import { ref } from 'vue'
                    function useCount() {
                      return {
                        count: ref()
                      }
                    }
                  ",
            None,
            None,
            Some(PathBuf::from("test.ts")),
        ),
        (
            r#"
                    <script setup lang="ts">
                      import { ref } from 'vue'
                      const count = ref()
                    </script>
                  "#,
            None,
            None,
            Some(PathBuf::from("test.vue")),
        ), // { "parser": require("vue-eslint-parser") },
        (
            r#"
                    <script lang="ts">
                      import { ref } from 'vue'
                      export default {
                        setup() {
                          const count = ref()
                        }
                      }
                    </script>
                  "#,
            None,
            None,
            Some(PathBuf::from("test.vue")),
        ), // { "parser": require("vue-eslint-parser") },
        (
            "
                    import { ref, defineComponent } from 'vue'
                    defineComponent({
                      setup() {
                        const count = ref()
                        return { count }
                      }
                    })
                  ",
            None,
            None,
            Some(PathBuf::from("test.ts")),
        ),
    ];

    Tester::new(RequireTypedRef::NAME, RequireTypedRef::PLUGIN, pass, fail).test();
}
