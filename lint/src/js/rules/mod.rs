//! The rules, by plugin. Each file is ported from oxlint and keeps its notice. Written by
//! lint/scripts/port_oxlint.py registry.

use super::{Entry, entry};

pub mod shared;

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

pub mod promise {
  pub mod always_return;
  pub mod avoid_new;
  pub mod catch_or_return;
  pub mod no_callback_in_promise;
  pub mod no_multiple_resolved;
  pub mod no_nesting;
  pub mod no_new_statics;
  pub mod no_promise_in_callback;
  pub mod no_return_in_finally;
  pub mod no_return_wrap;
  pub mod param_names;
  pub mod prefer_await_to_callbacks;
  pub mod prefer_await_to_then;
  pub mod prefer_catch;
  pub mod spec_only;
  pub mod valid_params;
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

pub mod unicorn {
  pub mod catch_error_name;
  pub mod consistent_assert;
  pub mod consistent_date_clone;
  pub mod consistent_empty_array_spread;
  pub mod consistent_existence_index_check;
  pub mod consistent_function_scoping;
  pub mod consistent_template_literal_escape;
  pub mod custom_error_definition;
  pub mod empty_brace_spaces;
  pub mod error_message;
  pub mod escape_case;
  pub mod explicit_length_check;
  pub mod explicit_timer_delay;
  pub mod filename_case;
  pub mod import_style;
  pub mod max_nested_calls;
  pub mod new_for_builtins;
  pub mod no_abusive_eslint_disable;
  pub mod no_accessor_recursion;
  pub mod no_anonymous_default_export;
  pub mod no_array_callback_reference;
  pub mod no_array_fill_with_reference_type;
  pub mod no_array_for_each;
  pub mod no_array_method_this_argument;
  pub mod no_array_reduce;
  pub mod no_array_reverse;
  pub mod no_array_sort;
  pub mod no_await_expression_member;
  pub mod no_await_in_promise_methods;
  pub mod no_confusing_array_with;
  pub mod no_console_spaces;
  pub mod no_document_cookie;
  pub mod no_empty_file;
  pub mod no_hex_escape;
  pub mod no_immediate_mutation;
  pub mod no_instanceof_array;
  pub mod no_instanceof_builtins;
  pub mod no_invalid_fetch_options;
  pub mod no_invalid_remove_event_listener;
  pub mod no_length_as_slice_end;
  pub mod no_lonely_if;
  pub mod no_magic_array_flat_depth;
  pub mod no_negated_condition;
  pub mod no_negation_in_equality_check;
  pub mod no_nested_ternary;
  pub mod no_new_array;
  pub mod no_new_buffer;
  pub mod no_null;
  pub mod no_object_as_default_parameter;
  pub mod no_process_exit;
  pub mod no_single_promise_in_promise_methods;
  pub mod no_static_only_class;
  pub mod no_thenable;
  pub mod no_this_assignment;
  pub mod no_typeof_undefined;
  pub mod no_unnecessary_array_flat_depth;
  pub mod no_unnecessary_array_splice_count;
  pub mod no_unnecessary_await;
  pub mod no_unnecessary_slice_end;
  pub mod no_unreadable_array_destructuring;
  pub mod no_unreadable_iife;
  pub mod no_useless_collection_argument;
  pub mod no_useless_error_capture_stack_trace;
  pub mod no_useless_fallback_in_spread;
  pub mod no_useless_iterator_to_array;
  pub mod no_useless_length_check;
  pub mod no_useless_promise_resolve_reject;
  pub mod no_useless_spread;
  pub mod no_useless_switch_case;
  pub mod no_useless_undefined;
  pub mod no_zero_fractions;
  pub mod number_literal_case;
  pub mod numeric_separators_style;
  pub mod prefer_add_event_listener;
  pub mod prefer_array_find;
  pub mod prefer_array_flat;
  pub mod prefer_array_flat_map;
  pub mod prefer_array_index_of;
  pub mod prefer_array_some;
  pub mod prefer_at;
  pub mod prefer_bigint_literals;
  pub mod prefer_blob_reading_methods;
  pub mod prefer_class_fields;
  pub mod prefer_classlist_toggle;
  pub mod prefer_code_point;
  pub mod prefer_date_now;
  pub mod prefer_default_parameters;
  pub mod prefer_dom_node_append;
  pub mod prefer_dom_node_dataset;
  pub mod prefer_dom_node_remove;
  pub mod prefer_dom_node_text_content;
  pub mod prefer_event_target;
  pub mod prefer_export_from;
  pub mod prefer_global_this;
  pub mod prefer_import_meta_properties;
  pub mod prefer_includes;
  pub mod prefer_keyboard_event_key;
  pub mod prefer_logical_operator_over_ternary;
  pub mod prefer_math_min_max;
  pub mod prefer_math_trunc;
  pub mod prefer_modern_dom_apis;
  pub mod prefer_modern_math_apis;
  pub mod prefer_module;
  pub mod prefer_native_coercion_functions;
  pub mod prefer_negative_index;
  pub mod prefer_number_coercion;
  pub mod prefer_number_properties;
  pub mod prefer_object_from_entries;
  pub mod prefer_optional_catch_binding;
  pub mod prefer_prototype_methods;
  pub mod prefer_query_selector;
  pub mod prefer_reflect_apply;
  pub mod prefer_regexp_test;
  pub mod prefer_response_static_json;
  pub mod prefer_set_has;
  pub mod prefer_set_size;
  pub mod prefer_single_call;
  pub mod prefer_spread;
  pub mod prefer_string_raw;
  pub mod prefer_string_replace_all;
  pub mod prefer_string_slice;
  pub mod prefer_string_starts_ends_with;
  pub mod prefer_string_trim_start_end;
  pub mod prefer_structured_clone;
  pub mod prefer_ternary;
  pub mod prefer_top_level_await;
  pub mod prefer_type_error;
  pub mod relative_url_style;
  pub mod require_array_join_separator;
  pub mod require_module_attributes;
  pub mod require_module_specifiers;
  pub mod require_number_to_fixed_digits_argument;
  pub mod require_post_message_target_origin;
  pub mod switch_case_braces;
  pub mod switch_case_break_position;
  pub mod text_encoding_identifier_case;
  pub mod throw_new_error;
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
  entry::<promise::always_return::AlwaysReturn>(),
  entry::<promise::avoid_new::AvoidNew>(),
  entry::<promise::catch_or_return::CatchOrReturn>(),
  entry::<promise::no_callback_in_promise::NoCallbackInPromise>(),
  entry::<promise::no_multiple_resolved::NoMultipleResolved>(),
  entry::<promise::no_nesting::NoNesting>(),
  entry::<promise::no_new_statics::NoNewStatics>(),
  entry::<promise::no_promise_in_callback::NoPromiseInCallback>(),
  entry::<promise::no_return_in_finally::NoReturnInFinally>(),
  entry::<promise::no_return_wrap::NoReturnWrap>(),
  entry::<promise::param_names::ParamNames>(),
  entry::<promise::prefer_await_to_callbacks::PreferAwaitToCallbacks>(),
  entry::<promise::prefer_await_to_then::PreferAwaitToThen>(),
  entry::<promise::prefer_catch::PreferCatch>(),
  entry::<promise::spec_only::SpecOnly>(),
  entry::<promise::valid_params::ValidParams>(),
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
  entry::<unicorn::catch_error_name::CatchErrorName>(),
  entry::<unicorn::consistent_assert::ConsistentAssert>(),
  entry::<unicorn::consistent_date_clone::ConsistentDateClone>(),
  entry::<unicorn::consistent_empty_array_spread::ConsistentEmptyArraySpread>(),
  entry::<unicorn::consistent_existence_index_check::ConsistentExistenceIndexCheck>(),
  entry::<unicorn::consistent_function_scoping::ConsistentFunctionScoping>(),
  entry::<unicorn::consistent_template_literal_escape::ConsistentTemplateLiteralEscape>(),
  entry::<unicorn::custom_error_definition::CustomErrorDefinition>(),
  entry::<unicorn::empty_brace_spaces::EmptyBraceSpaces>(),
  entry::<unicorn::error_message::ErrorMessage>(),
  entry::<unicorn::escape_case::EscapeCase>(),
  entry::<unicorn::explicit_length_check::ExplicitLengthCheck>(),
  entry::<unicorn::explicit_timer_delay::ExplicitTimerDelay>(),
  entry::<unicorn::filename_case::FilenameCase>(),
  entry::<unicorn::import_style::ImportStyle>(),
  entry::<unicorn::max_nested_calls::MaxNestedCalls>(),
  entry::<unicorn::new_for_builtins::NewForBuiltins>(),
  entry::<unicorn::no_abusive_eslint_disable::NoAbusiveEslintDisable>(),
  entry::<unicorn::no_accessor_recursion::NoAccessorRecursion>(),
  entry::<unicorn::no_anonymous_default_export::NoAnonymousDefaultExport>(),
  entry::<unicorn::no_array_callback_reference::NoArrayCallbackReference>(),
  entry::<unicorn::no_array_fill_with_reference_type::NoArrayFillWithReferenceType>(),
  entry::<unicorn::no_array_for_each::NoArrayForEach>(),
  entry::<unicorn::no_array_method_this_argument::NoArrayMethodThisArgument>(),
  entry::<unicorn::no_array_reduce::NoArrayReduce>(),
  entry::<unicorn::no_array_reverse::NoArrayReverse>(),
  entry::<unicorn::no_array_sort::NoArraySort>(),
  entry::<unicorn::no_await_expression_member::NoAwaitExpressionMember>(),
  entry::<unicorn::no_await_in_promise_methods::NoAwaitInPromiseMethods>(),
  entry::<unicorn::no_confusing_array_with::NoConfusingArrayWith>(),
  entry::<unicorn::no_console_spaces::NoConsoleSpaces>(),
  entry::<unicorn::no_document_cookie::NoDocumentCookie>(),
  entry::<unicorn::no_empty_file::NoEmptyFile>(),
  entry::<unicorn::no_hex_escape::NoHexEscape>(),
  entry::<unicorn::no_immediate_mutation::NoImmediateMutation>(),
  entry::<unicorn::no_instanceof_array::NoInstanceofArray>(),
  entry::<unicorn::no_instanceof_builtins::NoInstanceofBuiltins>(),
  entry::<unicorn::no_invalid_fetch_options::NoInvalidFetchOptions>(),
  entry::<unicorn::no_invalid_remove_event_listener::NoInvalidRemoveEventListener>(),
  entry::<unicorn::no_length_as_slice_end::NoLengthAsSliceEnd>(),
  entry::<unicorn::no_lonely_if::NoLonelyIf>(),
  entry::<unicorn::no_magic_array_flat_depth::NoMagicArrayFlatDepth>(),
  entry::<unicorn::no_negated_condition::NoNegatedCondition>(),
  entry::<unicorn::no_negation_in_equality_check::NoNegationInEqualityCheck>(),
  entry::<unicorn::no_nested_ternary::NoNestedTernary>(),
  entry::<unicorn::no_new_array::NoNewArray>(),
  entry::<unicorn::no_new_buffer::NoNewBuffer>(),
  entry::<unicorn::no_null::NoNull>(),
  entry::<unicorn::no_object_as_default_parameter::NoObjectAsDefaultParameter>(),
  entry::<unicorn::no_process_exit::NoProcessExit>(),
  entry::<unicorn::no_single_promise_in_promise_methods::NoSinglePromiseInPromiseMethods>(),
  entry::<unicorn::no_static_only_class::NoStaticOnlyClass>(),
  entry::<unicorn::no_thenable::NoThenable>(),
  entry::<unicorn::no_this_assignment::NoThisAssignment>(),
  entry::<unicorn::no_typeof_undefined::NoTypeofUndefined>(),
  entry::<unicorn::no_unnecessary_array_flat_depth::NoUnnecessaryArrayFlatDepth>(),
  entry::<unicorn::no_unnecessary_array_splice_count::NoUnnecessaryArraySpliceCount>(),
  entry::<unicorn::no_unnecessary_await::NoUnnecessaryAwait>(),
  entry::<unicorn::no_unnecessary_slice_end::NoUnnecessarySliceEnd>(),
  entry::<unicorn::no_unreadable_array_destructuring::NoUnreadableArrayDestructuring>(),
  entry::<unicorn::no_unreadable_iife::NoUnreadableIife>(),
  entry::<unicorn::no_useless_collection_argument::NoUselessCollectionArgument>(),
  entry::<unicorn::no_useless_error_capture_stack_trace::NoUselessErrorCaptureStackTrace>(),
  entry::<unicorn::no_useless_fallback_in_spread::NoUselessFallbackInSpread>(),
  entry::<unicorn::no_useless_iterator_to_array::NoUselessIteratorToArray>(),
  entry::<unicorn::no_useless_length_check::NoUselessLengthCheck>(),
  entry::<unicorn::no_useless_promise_resolve_reject::NoUselessPromiseResolveReject>(),
  entry::<unicorn::no_useless_spread::NoUselessSpread>(),
  entry::<unicorn::no_useless_switch_case::NoUselessSwitchCase>(),
  entry::<unicorn::no_useless_undefined::NoUselessUndefined>(),
  entry::<unicorn::no_zero_fractions::NoZeroFractions>(),
  entry::<unicorn::number_literal_case::NumberLiteralCase>(),
  entry::<unicorn::numeric_separators_style::NumericSeparatorsStyle>(),
  entry::<unicorn::prefer_add_event_listener::PreferAddEventListener>(),
  entry::<unicorn::prefer_array_find::PreferArrayFind>(),
  entry::<unicorn::prefer_array_flat::PreferArrayFlat>(),
  entry::<unicorn::prefer_array_flat_map::PreferArrayFlatMap>(),
  entry::<unicorn::prefer_array_index_of::PreferArrayIndexOf>(),
  entry::<unicorn::prefer_array_some::PreferArraySome>(),
  entry::<unicorn::prefer_at::PreferAt>(),
  entry::<unicorn::prefer_bigint_literals::PreferBigintLiterals>(),
  entry::<unicorn::prefer_blob_reading_methods::PreferBlobReadingMethods>(),
  entry::<unicorn::prefer_class_fields::PreferClassFields>(),
  entry::<unicorn::prefer_classlist_toggle::PreferClasslistToggle>(),
  entry::<unicorn::prefer_code_point::PreferCodePoint>(),
  entry::<unicorn::prefer_date_now::PreferDateNow>(),
  entry::<unicorn::prefer_default_parameters::PreferDefaultParameters>(),
  entry::<unicorn::prefer_dom_node_append::PreferDomNodeAppend>(),
  entry::<unicorn::prefer_dom_node_dataset::PreferDomNodeDataset>(),
  entry::<unicorn::prefer_dom_node_remove::PreferDomNodeRemove>(),
  entry::<unicorn::prefer_dom_node_text_content::PreferDomNodeTextContent>(),
  entry::<unicorn::prefer_event_target::PreferEventTarget>(),
  entry::<unicorn::prefer_export_from::PreferExportFrom>(),
  entry::<unicorn::prefer_global_this::PreferGlobalThis>(),
  entry::<unicorn::prefer_import_meta_properties::PreferImportMetaProperties>(),
  entry::<unicorn::prefer_includes::PreferIncludes>(),
  entry::<unicorn::prefer_keyboard_event_key::PreferKeyboardEventKey>(),
  entry::<unicorn::prefer_logical_operator_over_ternary::PreferLogicalOperatorOverTernary>(),
  entry::<unicorn::prefer_math_min_max::PreferMathMinMax>(),
  entry::<unicorn::prefer_math_trunc::PreferMathTrunc>(),
  entry::<unicorn::prefer_modern_dom_apis::PreferModernDomApis>(),
  entry::<unicorn::prefer_modern_math_apis::PreferModernMathApis>(),
  entry::<unicorn::prefer_module::PreferModule>(),
  entry::<unicorn::prefer_native_coercion_functions::PreferNativeCoercionFunctions>(),
  entry::<unicorn::prefer_negative_index::PreferNegativeIndex>(),
  entry::<unicorn::prefer_number_coercion::PreferNumberCoercion>(),
  entry::<unicorn::prefer_number_properties::PreferNumberProperties>(),
  entry::<unicorn::prefer_object_from_entries::PreferObjectFromEntries>(),
  entry::<unicorn::prefer_optional_catch_binding::PreferOptionalCatchBinding>(),
  entry::<unicorn::prefer_prototype_methods::PreferPrototypeMethods>(),
  entry::<unicorn::prefer_query_selector::PreferQuerySelector>(),
  entry::<unicorn::prefer_reflect_apply::PreferReflectApply>(),
  entry::<unicorn::prefer_regexp_test::PreferRegexpTest>(),
  entry::<unicorn::prefer_response_static_json::PreferResponseStaticJson>(),
  entry::<unicorn::prefer_set_has::PreferSetHas>(),
  entry::<unicorn::prefer_set_size::PreferSetSize>(),
  entry::<unicorn::prefer_single_call::PreferSingleCall>(),
  entry::<unicorn::prefer_spread::PreferSpread>(),
  entry::<unicorn::prefer_string_raw::PreferStringRaw>(),
  entry::<unicorn::prefer_string_replace_all::PreferStringReplaceAll>(),
  entry::<unicorn::prefer_string_slice::PreferStringSlice>(),
  entry::<unicorn::prefer_string_starts_ends_with::PreferStringStartsEndsWith>(),
  entry::<unicorn::prefer_string_trim_start_end::PreferStringTrimStartEnd>(),
  entry::<unicorn::prefer_structured_clone::PreferStructuredClone>(),
  entry::<unicorn::prefer_ternary::PreferTernary>(),
  entry::<unicorn::prefer_top_level_await::PreferTopLevelAwait>(),
  entry::<unicorn::prefer_type_error::PreferTypeError>(),
  entry::<unicorn::relative_url_style::RelativeUrlStyle>(),
  entry::<unicorn::require_array_join_separator::RequireArrayJoinSeparator>(),
  entry::<unicorn::require_module_attributes::RequireModuleAttributes>(),
  entry::<unicorn::require_module_specifiers::RequireModuleSpecifiers>(),
  entry::<unicorn::require_number_to_fixed_digits_argument::RequireNumberToFixedDigitsArgument>(),
  entry::<unicorn::require_post_message_target_origin::RequirePostMessageTargetOrigin>(),
  entry::<unicorn::switch_case_braces::SwitchCaseBraces>(),
  entry::<unicorn::switch_case_break_position::SwitchCaseBreakPosition>(),
  entry::<unicorn::text_encoding_identifier_case::TextEncodingIdentifierCase>(),
  entry::<unicorn::throw_new_error::ThrowNewError>(),
];
