// Derived from oxc_linter's utils at oxc crates_v0.155.0, MIT License, Copyright (c) 2024-present VoidZero Inc. & Contributors.

use oxc_ast::{ast::Expression, match_member_expression};
use oxc_span::Span;
use oxc_syntax::{
    identifier::{is_identifier_part, is_identifier_start},
    precedence::{GetPrecedence, Precedence},
};

mod control_flow;
mod number;
mod regex;

pub use self::{control_flow::*, number::*, regex::*};

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
