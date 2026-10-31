// Derived from oxc_linter's utils at oxc crates_v0.155.0, MIT License, Copyright (c) 2024-present VoidZero Inc. & Contributors.

use oxc_ast::{ast::{Expression, MemberExpression}, match_member_expression};
use oxc_span::{ContentEq, Span};

use crate::js::LintContext;
use oxc_syntax::{
    identifier::{is_identifier_part, is_identifier_start},
    precedence::{GetPrecedence, Precedence},
};

mod config;
mod jsdoc;
mod control_flow;
mod number;
mod promise;
mod react;
mod regex;
mod static_value;
mod this_expression;
mod typescript;
mod unicorn;
mod vue;
pub mod vue_casing;

pub use self::{config::*, control_flow::*, jsdoc::*, number::*, promise::*, react::*, regex::*, static_value::*, this_expression::*, typescript::*, unicorn::*, vue::*};

/// Pads replacement text with spaces when needed to preserve token boundaries
/// with neighboring source characters.
pub fn pad_fix_with_token_boundary(source_text: &str, span: Span, replacement: &mut String) {
    if replacement.is_empty() {
        return;
    }

    let source_bytes = source_text.as_bytes();
    let replacement_bytes = replacement.as_bytes();
    let needs_pad_start = span.start > 0
        && is_identifier_part(source_bytes[span.start as usize - 1] as char)
        && is_identifier_part(replacement.chars().next().unwrap());
    let needs_pad_end = (span.end as usize) < source_bytes.len()
        && is_identifier_start(source_bytes[span.end as usize] as char)
        && !replacement_bytes.last().unwrap().is_ascii_whitespace();

    if needs_pad_start {
        replacement.insert(0, ' ');
    }
    if needs_pad_end {
        replacement.push(' ');
    }
}

/// Returns the precedence of an expression if it has one.
///
/// Returns `None` for "atomic" expressions (literals, identifiers, etc.) that have
/// the highest precedence and never need parentheses when used as operands.
///
/// This is useful for determining if parentheses are needed when transforming code.
/// If `get_precedence(expr)` returns `None`, the expression never needs parentheses.
/// If it returns `Some(p)`, compare `p` against the context's precedence to decide.
pub fn get_precedence(expr: &Expression) -> Option<Precedence> {
    match expr {
        Expression::SequenceExpression(e) => Some(e.precedence()),
        Expression::AssignmentExpression(e) => Some(e.precedence()),
        Expression::YieldExpression(e) => Some(e.precedence()),
        Expression::ConditionalExpression(e) => Some(e.precedence()),
        Expression::LogicalExpression(e) => Some(e.precedence()),
        Expression::BinaryExpression(e) => Some(e.precedence()),
        Expression::UnaryExpression(e) => Some(e.precedence()),
        Expression::UpdateExpression(e) => Some(e.precedence()),
        Expression::AwaitExpression(e) => Some(e.precedence()),
        Expression::NewExpression(e) => Some(e.precedence()),
        Expression::CallExpression(e) => Some(e.precedence()),
        match_member_expression!(Expression) => Some(expr.to_member_expression().precedence()),
        Expression::TSAsExpression(_)
        | Expression::TSSatisfiesExpression(_)
        | Expression::TSTypeAssertion(_)
        | Expression::ArrowFunctionExpression(_) => Some(Precedence::Lowest),
        // Literals, identifiers, and other atomic expressions have highest precedence
        _ => None,
    }
}

/// Compares two expressions to see if they are the same.
pub fn is_same_expression(left: &Expression, right: &Expression, ctx: &LintContext) -> bool {
    if let Expression::ChainExpression(left_chain_expr) = left
        && let Some(right_member_expr) = right.as_member_expression()
        && let Some(v) = left_chain_expr.expression.as_member_expression()
    {
        return is_same_member_expression(v, right_member_expr, ctx);
    }

    if let Some(left_chain_expr) = left.as_member_expression()
        && let Expression::ChainExpression(right_member_expr) = right
        && let Some(v) = right_member_expr.expression.as_member_expression()
    {
        return is_same_member_expression(left_chain_expr, v, ctx);
    }

    match (left, right) {
        // super // this
        (Expression::Super(_), Expression::Super(_))
        | (Expression::ThisExpression(_), Expression::ThisExpression(_))
        | (Expression::NullLiteral(_), Expression::NullLiteral(_)) => return true,

        (Expression::Identifier(left_ident), Expression::Identifier(right_ident)) => {
            return left_ident.name == right_ident.name;
        }

        (Expression::StringLiteral(left_str), Expression::StringLiteral(right_str)) => {
            return left_str.value == right_str.value;
        }
        (Expression::StringLiteral(string_lit), Expression::TemplateLiteral(template_lit))
        | (Expression::TemplateLiteral(template_lit), Expression::StringLiteral(string_lit)) => {
            return template_lit.single_quasi().is_some_and(|val| val == string_lit.value);
        }
        (Expression::TemplateLiteral(left_str), Expression::TemplateLiteral(right_str)) => {
            return left_str.quasis.content_eq(&right_str.quasis)
                && left_str.expressions.len() == right_str.expressions.len()
                && left_str
                    .expressions
                    .iter()
                    .zip(right_str.expressions.iter())
                    .all(|(left, right)| is_same_expression(left, right, ctx));
        }
        (Expression::NumericLiteral(left_num), Expression::NumericLiteral(right_num)) => {
            return left_num.raw == right_num.raw;
        }
        (Expression::RegExpLiteral(left_regexp), Expression::RegExpLiteral(right_regexp)) => {
            return left_regexp.regex.pattern.text == right_regexp.regex.pattern.text
                && left_regexp.regex.flags == right_regexp.regex.flags;
        }
        (Expression::BooleanLiteral(left_bool), Expression::BooleanLiteral(right_bool)) => {
            return left_bool.value == right_bool.value;
        }

        (
            Expression::BinaryExpression(left_bin_expr),
            Expression::BinaryExpression(right_bin_expr),
        ) => {
            return left_bin_expr.operator == right_bin_expr.operator
                && is_same_expression(
                    left_bin_expr.left.get_inner_expression(),
                    right_bin_expr.left.get_inner_expression(),
                    ctx,
                )
                && is_same_expression(
                    left_bin_expr.right.get_inner_expression(),
                    right_bin_expr.right.get_inner_expression(),
                    ctx,
                );
        }

        (
            Expression::LogicalExpression(left_logical_expr),
            Expression::LogicalExpression(right_logical_expr),
        ) => {
            return left_logical_expr.operator == right_logical_expr.operator
                && is_same_expression(
                    left_logical_expr.left.get_inner_expression(),
                    right_logical_expr.left.get_inner_expression(),
                    ctx,
                )
                && is_same_expression(
                    left_logical_expr.right.get_inner_expression(),
                    right_logical_expr.right.get_inner_expression(),
                    ctx,
                );
        }

        (
            Expression::UnaryExpression(left_unary_expr),
            Expression::UnaryExpression(right_unary_expr),
        ) => {
            return left_unary_expr.operator == right_unary_expr.operator
                && is_same_expression(
                    left_unary_expr.argument.get_inner_expression(),
                    right_unary_expr.argument.get_inner_expression(),
                    ctx,
                );
        }

        (
            Expression::ChainExpression(left_chain_expr),
            Expression::ChainExpression(right_chain_expr),
        ) => {
            if let Some(left_member_expr) = left_chain_expr.expression.as_member_expression()
                && let Some(right_member_expr) = right_chain_expr.expression.as_member_expression()
            {
                return is_same_member_expression(left_member_expr, right_member_expr, ctx);
            }
        }
        _ => {}
    }

    if let (Some(left_member_expr), Some(right_member_expr)) =
        (left.as_member_expression(), right.as_member_expression())
    {
        return is_same_member_expression(left_member_expr, right_member_expr, ctx);
    }

    false
}

/// Checks if `haystack` starts with `prefix`, ignoring ASCII case.
pub fn starts_with_ignore_case(haystack: &[u8], prefix: &str) -> bool {
    haystack.get(..prefix.len()).is_some_and(|start| start.eq_ignore_ascii_case(prefix.as_bytes()))
}

pub fn is_same_member_expression(
    left: &MemberExpression,
    right: &MemberExpression,
    ctx: &LintContext,
) -> bool {
    let left_static_property_name = left.static_property_name();
    let right_static_property_name = right.static_property_name();

    match (left_static_property_name, right_static_property_name) {
        (Some(left_static_property_name), Some(right_static_property_name)) => {
            if left_static_property_name != right_static_property_name {
                return false;
            }
        }
        (Some(_), None) | (None, Some(_)) => {
            return false;
        }
        (None, None) => match (left, right) {
            (
                MemberExpression::PrivateFieldExpression(left),
                MemberExpression::PrivateFieldExpression(right),
            ) => {
                return left.field.name == right.field.name
                    && is_same_expression(&left.object, &right.object, ctx);
            }
            (MemberExpression::PrivateFieldExpression(_), _)
            | (_, MemberExpression::PrivateFieldExpression(_)) => return false,
            _ => {}
        },
    }

    if let (
        MemberExpression::ComputedMemberExpression(left),
        MemberExpression::ComputedMemberExpression(right),
    ) = (left, right)
    {
        // TODO(camc314): refactor this to go through `is_same_reference` and introduce some sort of `context` to indicate how the two values should be compared.
        match (&left.expression, &right.expression) {
            // x['/regex/'] === x[/regex/]
            // x[/regex/] === x['/regex/']
            (Expression::StringLiteral(string_lit), Expression::RegExpLiteral(regex_lit))
            | (Expression::RegExpLiteral(regex_lit), Expression::StringLiteral(string_lit)) => {
                if string_lit.value != regex_lit.raw.as_ref().unwrap().as_str() {
                    return false;
                }
            }
            // ex) x[`/regex/`] === x[/regex/]
            // ex) x[/regex/] === x[`/regex/`]
            (Expression::TemplateLiteral(template_lit), Expression::RegExpLiteral(regex_lit))
            | (Expression::RegExpLiteral(regex_lit), Expression::TemplateLiteral(template_lit)) => {
                if !template_lit
                    .single_quasi()
                    .is_some_and(|val| val == regex_lit.raw.as_ref().unwrap().as_str())
                {
                    return false;
                }
            }
            _ => {
                if !is_same_expression(
                    left.expression.get_inner_expression(),
                    right.expression.get_inner_expression(),
                    ctx,
                ) {
                    return false;
                }
            }
        }
    }

    is_same_expression(
        left.object().get_inner_expression(),
        right.object().get_inner_expression(),
        ctx,
    )
}
