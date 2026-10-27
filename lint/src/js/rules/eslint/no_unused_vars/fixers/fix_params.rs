// Ported from oxlint (crates/oxc_linter/src/rules/eslint/no_unused_vars/fixers/fix_params.rs) at oxc crates_v0.155.0, MIT License, Copyright (c) 2024-present VoidZero Inc. & Contributors.

use oxc_ast::ast::FormalParameter;

use super::{NoUnusedVars, Symbol};
use crate::js::fixer::{RuleFix, RuleFixer};

impl NoUnusedVars {
    /// Rename an unused function parameter to match `argsIgnorePattern`.
    ///
    /// ```js
    /// function foo(unused) {}
    /// function foo2(unused = 1) {}
    /// ```
    ///
    /// becomes:
    ///
    /// ```js
    /// function foo(_unused) {}
    /// function foo2(_unused = 1) {}
    /// ```
    pub(in super::super) fn rename_unused_function_parameter<'a>(
        &self,
        fixer: RuleFixer<'_, 'a>,
        symbol: &Symbol<'_, 'a>,
        param: &FormalParameter<'a>,
    ) -> RuleFix {
        if param.has_modifier()
            || param.pattern.is_destructuring_pattern()
            || param.pattern.get_binding_identifier().is_none()
        {
            return fixer.noop();
        }

        if let Some(new_name) = self.get_unused_arg_name(symbol) {
            return symbol.rename_with_fixer(fixer, &new_name).dangerously();
        }

        fixer.noop()
    }
}
