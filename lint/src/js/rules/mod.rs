//! The rules, by plugin. Each file is ported from oxlint and keeps its notice. Written by
//! lint/scripts/port_oxlint.py registry.

use super::{Entry, entry};

pub mod eslint {
  pub mod constructor_super;
  pub mod for_direction;
  pub mod getter_return;
  pub mod no_async_promise_executor;
  pub mod no_case_declarations;
  pub mod no_class_assign;
  pub mod no_compare_neg_zero;
  pub mod no_cond_assign;
  pub mod no_const_assign;
  pub mod no_constant_binary_expression;
  pub mod no_constant_condition;
  pub mod no_control_regex;
  pub mod no_debugger;
  pub mod no_delete_var;
  pub mod no_dupe_class_members;
  pub mod no_dupe_else_if;
  pub mod no_dupe_keys;
  pub mod no_duplicate_case;
  pub mod no_empty;
  pub mod no_empty_character_class;
  pub mod no_empty_pattern;
  pub mod no_empty_static_block;
  pub mod no_ex_assign;
  pub mod no_extra_boolean_cast;
  pub mod no_fallthrough;
  pub mod no_func_assign;
  pub mod no_global_assign;
  pub mod no_import_assign;
  pub mod no_invalid_regexp;
  pub mod no_irregular_whitespace;
  pub mod no_loss_of_precision;
  pub mod no_misleading_character_class;
  pub mod no_new_native_nonconstructor;
  pub mod no_nonoctal_decimal_escape;
  pub mod no_obj_calls;
  pub mod no_prototype_builtins;
  pub mod no_redeclare;
  pub mod no_regex_spaces;
  pub mod no_self_assign;
  pub mod no_setter_return;
  pub mod no_shadow_restricted_names;
  pub mod no_sparse_arrays;
  pub mod no_this_before_super;
  pub mod no_unassigned_vars;
  pub mod no_unexpected_multiline;
  pub mod no_unreachable;
  pub mod no_unsafe_finally;
  pub mod no_unsafe_negation;
  pub mod no_unsafe_optional_chaining;
  pub mod no_unused_expressions;
  pub mod no_unused_labels;
  pub mod no_unused_private_class_members;
  pub mod no_unused_vars;
  pub mod no_useless_backreference;
  pub mod no_useless_catch;
  pub mod no_useless_escape;
  pub mod no_with;
  pub mod preserve_caught_error;
  pub mod require_yield;
  pub mod use_isnan;
  pub mod valid_typeof;
}

/// Every rule, sorted by id.
pub(crate) static RULES: &[Entry] = &[
  entry::<eslint::constructor_super::ConstructorSuper>(),
  entry::<eslint::for_direction::ForDirection>(),
  entry::<eslint::getter_return::GetterReturn>(),
  entry::<eslint::no_async_promise_executor::NoAsyncPromiseExecutor>(),
  entry::<eslint::no_case_declarations::NoCaseDeclarations>(),
  entry::<eslint::no_class_assign::NoClassAssign>(),
  entry::<eslint::no_compare_neg_zero::NoCompareNegZero>(),
  entry::<eslint::no_cond_assign::NoCondAssign>(),
  entry::<eslint::no_const_assign::NoConstAssign>(),
  entry::<eslint::no_constant_binary_expression::NoConstantBinaryExpression>(),
  entry::<eslint::no_constant_condition::NoConstantCondition>(),
  entry::<eslint::no_control_regex::NoControlRegex>(),
  entry::<eslint::no_debugger::NoDebugger>(),
  entry::<eslint::no_delete_var::NoDeleteVar>(),
  entry::<eslint::no_dupe_class_members::NoDupeClassMembers>(),
  entry::<eslint::no_dupe_else_if::NoDupeElseIf>(),
  entry::<eslint::no_dupe_keys::NoDupeKeys>(),
  entry::<eslint::no_duplicate_case::NoDuplicateCase>(),
  entry::<eslint::no_empty::NoEmpty>(),
  entry::<eslint::no_empty_character_class::NoEmptyCharacterClass>(),
  entry::<eslint::no_empty_pattern::NoEmptyPattern>(),
  entry::<eslint::no_empty_static_block::NoEmptyStaticBlock>(),
  entry::<eslint::no_ex_assign::NoExAssign>(),
  entry::<eslint::no_extra_boolean_cast::NoExtraBooleanCast>(),
  entry::<eslint::no_fallthrough::NoFallthrough>(),
  entry::<eslint::no_func_assign::NoFuncAssign>(),
  entry::<eslint::no_global_assign::NoGlobalAssign>(),
  entry::<eslint::no_import_assign::NoImportAssign>(),
  entry::<eslint::no_invalid_regexp::NoInvalidRegexp>(),
  entry::<eslint::no_irregular_whitespace::NoIrregularWhitespace>(),
  entry::<eslint::no_loss_of_precision::NoLossOfPrecision>(),
  entry::<eslint::no_misleading_character_class::NoMisleadingCharacterClass>(),
  entry::<eslint::no_new_native_nonconstructor::NoNewNativeNonconstructor>(),
  entry::<eslint::no_nonoctal_decimal_escape::NoNonoctalDecimalEscape>(),
  entry::<eslint::no_obj_calls::NoObjCalls>(),
  entry::<eslint::no_prototype_builtins::NoPrototypeBuiltins>(),
  entry::<eslint::no_redeclare::NoRedeclare>(),
  entry::<eslint::no_regex_spaces::NoRegexSpaces>(),
  entry::<eslint::no_self_assign::NoSelfAssign>(),
  entry::<eslint::no_setter_return::NoSetterReturn>(),
  entry::<eslint::no_shadow_restricted_names::NoShadowRestrictedNames>(),
  entry::<eslint::no_sparse_arrays::NoSparseArrays>(),
  entry::<eslint::no_this_before_super::NoThisBeforeSuper>(),
  entry::<eslint::no_unassigned_vars::NoUnassignedVars>(),
  entry::<eslint::no_unexpected_multiline::NoUnexpectedMultiline>(),
  entry::<eslint::no_unreachable::NoUnreachable>(),
  entry::<eslint::no_unsafe_finally::NoUnsafeFinally>(),
  entry::<eslint::no_unsafe_negation::NoUnsafeNegation>(),
  entry::<eslint::no_unsafe_optional_chaining::NoUnsafeOptionalChaining>(),
  entry::<eslint::no_unused_expressions::NoUnusedExpressions>(),
  entry::<eslint::no_unused_labels::NoUnusedLabels>(),
  entry::<eslint::no_unused_private_class_members::NoUnusedPrivateClassMembers>(),
  entry::<eslint::no_unused_vars::NoUnusedVars>(),
  entry::<eslint::no_useless_backreference::NoUselessBackreference>(),
  entry::<eslint::no_useless_catch::NoUselessCatch>(),
  entry::<eslint::no_useless_escape::NoUselessEscape>(),
  entry::<eslint::no_with::NoWith>(),
  entry::<eslint::preserve_caught_error::PreserveCaughtError>(),
  entry::<eslint::require_yield::RequireYield>(),
  entry::<eslint::use_isnan::UseIsnan>(),
  entry::<eslint::valid_typeof::ValidTypeof>(),
];
