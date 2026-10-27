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
  pub mod no_console;
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
  pub mod no_throw_literal;
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
  pub mod no_use_before_define;
  pub mod no_useless_backreference;
  pub mod no_useless_catch;
  pub mod no_useless_escape;
  pub mod no_with;
  pub mod preserve_caught_error;
  pub mod require_yield;
  pub mod use_isnan;
  pub mod valid_typeof;
}

pub mod import {
  pub mod no_anonymous_default_export;
}

pub mod jsx_a11y {
  pub mod alt_text;
  pub mod anchor_has_content;
  pub mod anchor_is_valid;
  pub mod aria_activedescendant_has_tabindex;
  pub mod aria_props;
  pub mod aria_proptypes;
  pub mod aria_role;
  pub mod aria_unsupported_elements;
  pub mod autocomplete_valid;
  pub mod click_events_have_key_events;
  pub mod heading_has_content;
  pub mod html_has_lang;
  pub mod iframe_has_title;
  pub mod img_redundant_alt;
  pub mod interactive_supports_focus;
  pub mod label_has_associated_control;
  pub mod media_has_caption;
  pub mod mouse_events_have_key_events;
  pub mod no_access_key;
  pub mod no_autofocus;
  pub mod no_distracting_elements;
  pub mod no_interactive_element_to_noninteractive_role;
  pub mod no_noninteractive_element_interactions;
  pub mod no_noninteractive_element_to_interactive_role;
  pub mod no_noninteractive_tabindex;
  pub mod no_redundant_roles;
  pub mod no_static_element_interactions;
  pub mod role_has_required_aria_props;
  pub mod role_supports_aria_props;
  pub mod scope;
  pub mod tabindex_no_positive;
}

pub mod react {
  pub mod display_name;
  pub mod exhaustive_deps;
  pub mod jsx_key;
  pub mod jsx_no_comment_textnodes;
  pub mod jsx_no_duplicate_props;
  pub mod jsx_no_target_blank;
  pub mod jsx_no_undef;
  pub mod no_array_index_key;
  pub mod no_children_prop;
  pub mod no_danger_with_children;
  pub mod no_direct_mutation_state;
  pub mod no_find_dom_node;
  pub mod no_is_mounted;
  pub mod no_render_return_value;
  pub mod no_string_refs;
  pub mod no_unescaped_entities;
  pub mod no_unknown_property;
  pub mod react_in_jsx_scope;
  pub mod require_render_return;
  pub mod rules_of_hooks;
}

pub mod typescript {
  pub mod ban_ts_comment;
  pub mod no_duplicate_enum_values;
  pub mod no_empty_object_type;
  pub mod no_explicit_any;
  pub mod no_extra_non_null_assertion;
  pub mod no_misused_new;
  pub mod no_namespace;
  pub mod no_non_null_asserted_optional_chain;
  pub mod no_non_null_assertion;
  pub mod no_require_imports;
  pub mod no_this_alias;
  pub mod no_unnecessary_type_constraint;
  pub mod no_unsafe_declaration_merging;
  pub mod no_unsafe_function_type;
  pub mod no_wrapper_object_types;
  pub mod prefer_as_const;
  pub mod prefer_namespace_keyword;
  pub mod triple_slash_reference;
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
  entry::<eslint::no_console::NoConsole>(),
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
  entry::<eslint::no_throw_literal::NoThrowLiteral>(),
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
  entry::<eslint::no_use_before_define::NoUseBeforeDefine>(),
  entry::<eslint::no_useless_backreference::NoUselessBackreference>(),
  entry::<eslint::no_useless_catch::NoUselessCatch>(),
  entry::<eslint::no_useless_escape::NoUselessEscape>(),
  entry::<eslint::no_with::NoWith>(),
  entry::<eslint::preserve_caught_error::PreserveCaughtError>(),
  entry::<eslint::require_yield::RequireYield>(),
  entry::<eslint::use_isnan::UseIsnan>(),
  entry::<eslint::valid_typeof::ValidTypeof>(),
  entry::<import::no_anonymous_default_export::NoAnonymousDefaultExport>(),
  entry::<jsx_a11y::alt_text::AltText>(),
  entry::<jsx_a11y::anchor_has_content::AnchorHasContent>(),
  entry::<jsx_a11y::anchor_is_valid::AnchorIsValid>(),
  entry::<jsx_a11y::aria_activedescendant_has_tabindex::AriaActivedescendantHasTabindex>(),
  entry::<jsx_a11y::aria_props::AriaProps>(),
  entry::<jsx_a11y::aria_proptypes::AriaProptypes>(),
  entry::<jsx_a11y::aria_role::AriaRole>(),
  entry::<jsx_a11y::aria_unsupported_elements::AriaUnsupportedElements>(),
  entry::<jsx_a11y::autocomplete_valid::AutocompleteValid>(),
  entry::<jsx_a11y::click_events_have_key_events::ClickEventsHaveKeyEvents>(),
  entry::<jsx_a11y::heading_has_content::HeadingHasContent>(),
  entry::<jsx_a11y::html_has_lang::HtmlHasLang>(),
  entry::<jsx_a11y::iframe_has_title::IframeHasTitle>(),
  entry::<jsx_a11y::img_redundant_alt::ImgRedundantAlt>(),
  entry::<jsx_a11y::interactive_supports_focus::InteractiveSupportsFocus>(),
  entry::<jsx_a11y::label_has_associated_control::LabelHasAssociatedControl>(),
  entry::<jsx_a11y::media_has_caption::MediaHasCaption>(),
  entry::<jsx_a11y::mouse_events_have_key_events::MouseEventsHaveKeyEvents>(),
  entry::<jsx_a11y::no_access_key::NoAccessKey>(),
  entry::<jsx_a11y::no_autofocus::NoAutofocus>(),
  entry::<jsx_a11y::no_distracting_elements::NoDistractingElements>(),
  entry::<jsx_a11y::no_interactive_element_to_noninteractive_role::NoInteractiveElementToNoninteractiveRole>(),
  entry::<jsx_a11y::no_noninteractive_element_interactions::NoNoninteractiveElementInteractions>(),
  entry::<jsx_a11y::no_noninteractive_element_to_interactive_role::NoNoninteractiveElementToInteractiveRole>(),
  entry::<jsx_a11y::no_noninteractive_tabindex::NoNoninteractiveTabindex>(),
  entry::<jsx_a11y::no_redundant_roles::NoRedundantRoles>(),
  entry::<jsx_a11y::no_static_element_interactions::NoStaticElementInteractions>(),
  entry::<jsx_a11y::role_has_required_aria_props::RoleHasRequiredAriaProps>(),
  entry::<jsx_a11y::role_supports_aria_props::RoleSupportsAriaProps>(),
  entry::<jsx_a11y::scope::Scope>(),
  entry::<jsx_a11y::tabindex_no_positive::TabindexNoPositive>(),
  entry::<react::display_name::DisplayName>(),
  entry::<react::exhaustive_deps::ExhaustiveDeps>(),
  entry::<react::jsx_key::JsxKey>(),
  entry::<react::jsx_no_comment_textnodes::JsxNoCommentTextnodes>(),
  entry::<react::jsx_no_duplicate_props::JsxNoDuplicateProps>(),
  entry::<react::jsx_no_target_blank::JsxNoTargetBlank>(),
  entry::<react::jsx_no_undef::JsxNoUndef>(),
  entry::<react::no_array_index_key::NoArrayIndexKey>(),
  entry::<react::no_children_prop::NoChildrenProp>(),
  entry::<react::no_danger_with_children::NoDangerWithChildren>(),
  entry::<react::no_direct_mutation_state::NoDirectMutationState>(),
  entry::<react::no_find_dom_node::NoFindDomNode>(),
  entry::<react::no_is_mounted::NoIsMounted>(),
  entry::<react::no_render_return_value::NoRenderReturnValue>(),
  entry::<react::no_string_refs::NoStringRefs>(),
  entry::<react::no_unescaped_entities::NoUnescapedEntities>(),
  entry::<react::no_unknown_property::NoUnknownProperty>(),
  entry::<react::react_in_jsx_scope::ReactInJsxScope>(),
  entry::<react::require_render_return::RequireRenderReturn>(),
  entry::<react::rules_of_hooks::RulesOfHooks>(),
  entry::<typescript::ban_ts_comment::BanTsComment>(),
  entry::<typescript::no_duplicate_enum_values::NoDuplicateEnumValues>(),
  entry::<typescript::no_empty_object_type::NoEmptyObjectType>(),
  entry::<typescript::no_explicit_any::NoExplicitAny>(),
  entry::<typescript::no_extra_non_null_assertion::NoExtraNonNullAssertion>(),
  entry::<typescript::no_misused_new::NoMisusedNew>(),
  entry::<typescript::no_namespace::NoNamespace>(),
  entry::<typescript::no_non_null_asserted_optional_chain::NoNonNullAssertedOptionalChain>(),
  entry::<typescript::no_non_null_assertion::NoNonNullAssertion>(),
  entry::<typescript::no_require_imports::NoRequireImports>(),
  entry::<typescript::no_this_alias::NoThisAlias>(),
  entry::<typescript::no_unnecessary_type_constraint::NoUnnecessaryTypeConstraint>(),
  entry::<typescript::no_unsafe_declaration_merging::NoUnsafeDeclarationMerging>(),
  entry::<typescript::no_unsafe_function_type::NoUnsafeFunctionType>(),
  entry::<typescript::no_wrapper_object_types::NoWrapperObjectTypes>(),
  entry::<typescript::prefer_as_const::PreferAsConst>(),
  entry::<typescript::prefer_namespace_keyword::PreferNamespaceKeyword>(),
  entry::<typescript::triple_slash_reference::TripleSlashReference>(),
];
