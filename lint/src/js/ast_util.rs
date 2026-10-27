// Derived from oxc_linter at oxc crates_v0.155.0, MIT License, Copyright (c) 2024-present VoidZero Inc. & Contributors.



use oxc_ast::{
    AstKind,
    ast::*,
};
use oxc_ecmascript::{ToBoolean, WithoutGlobalReferenceInformation};
use oxc_semantic::{AstNode, AstNodes, Semantic};
use oxc_syntax::operator::{AssignmentOperator, BinaryOperator, LogicalOperator, UnaryOperator};


/// Get a variable declarator's declaration kind, or `None` if its parent is not a variable declaration.
pub fn variable_declaration_kind(
    declarator: &VariableDeclarator<'_>,
    nodes: &AstNodes<'_>,
) -> Option<VariableDeclarationKind> {
    nodes
        .parent_kind(declarator.node_id())
        .as_variable_declaration()
        .map(|declaration| declaration.kind)
}

/// Test if an AST node is a boolean value that never changes. Specifically we
/// test for:
/// 1. Literal booleans (`true` or `false`)
/// 2. Unary `!` expressions with a constant value
/// 3. Constant booleans created via the `Boolean` global function
pub fn is_static_boolean<'a>(expr: &Expression<'a>, semantic: &Semantic<'a>) -> bool {
    match expr {
        Expression::BooleanLiteral(_) => true,
        Expression::CallExpression(call_expr) => call_expr.is_constant(true, semantic),
        Expression::UnaryExpression(unary_expr) => {
            unary_expr.operator == UnaryOperator::LogicalNot
                && unary_expr.argument.is_constant(true, semantic)
        }
        _ => false,
    }
}

/// Checks if a branch node of `LogicalExpression` short circuits the whole condition
fn is_logical_identity(op: LogicalOperator, expr: &Expression) -> bool {
    match expr {
        expr if expr.is_literal() => {
            let boolean_value = expr.to_boolean(&WithoutGlobalReferenceInformation {});
            (op == LogicalOperator::Or && boolean_value == Some(true))
                || (op == LogicalOperator::And && boolean_value == Some(false))
        }
        Expression::UnaryExpression(unary_expr) => op.is_and() && unary_expr.operator.is_void(),
        Expression::LogicalExpression(logical_expr) => {
            op == logical_expr.operator
                && (is_logical_identity(logical_expr.operator, &logical_expr.left)
                    || is_logical_identity(logical_expr.operator, &logical_expr.right))
        }
        Expression::AssignmentExpression(assign_expr) => {
            matches!(
                assign_expr.operator,
                AssignmentOperator::LogicalAnd | AssignmentOperator::LogicalOr
            ) && ((op == LogicalOperator::And
                && assign_expr.operator == AssignmentOperator::LogicalAnd)
                || (op == LogicalOperator::Or
                    && assign_expr.operator == AssignmentOperator::LogicalOr))
                && is_logical_identity(op, &assign_expr.right)
        }
        Expression::ParenthesizedExpression(expr) => is_logical_identity(op, &expr.expression),
        _ => false,
    }
}

/// Checks if a  node has a constant truthiness value.
/// `inBooleanPosition`:
///   `true` if checking the test of a condition.
///   `false` in all other cases.
///   When `false`, checks if -- for both string and number --
///   if coerced to that type, the value will be constant.
pub trait IsConstant<'a, 'b> {
    fn is_constant(&self, in_boolean_position: bool, semantic: &Semantic<'a>) -> bool;
}

impl<'a> IsConstant<'a, '_> for Expression<'a> {
    fn is_constant(&self, in_boolean_position: bool, semantic: &Semantic<'a>) -> bool {
        match self {
            Self::ArrowFunctionExpression(_)
            | Self::FunctionExpression(_)
            | Self::ClassExpression(_)
            | Self::ObjectExpression(_) => true,
            Self::TemplateLiteral(template) => {
                let test_quasis = in_boolean_position
                    && template.quasis.iter().any(|quasi| {
                        quasi.value.cooked.as_ref().is_some_and(|cooked| !cooked.is_empty())
                    });
                let test_expressions =
                    template.expressions.iter().all(|expr| expr.is_constant(false, semantic));
                test_quasis || test_expressions
            }
            Self::ArrayExpression(expr) => {
                if in_boolean_position {
                    return true;
                }
                expr.elements.iter().all(|element| element.is_constant(false, semantic))
            }
            Self::UnaryExpression(expr) => match expr.operator {
                UnaryOperator::Void => true,
                UnaryOperator::Typeof if in_boolean_position => true,
                UnaryOperator::LogicalNot => expr.argument.is_constant(true, semantic),
                _ => expr.argument.is_constant(false, semantic),
            },
            Self::BinaryExpression(expr) => {
                expr.operator != BinaryOperator::In
                    && expr.left.is_constant(false, semantic)
                    && expr.right.is_constant(false, semantic)
            }
            Self::LogicalExpression(expr) => {
                let is_left_constant = expr.left.is_constant(in_boolean_position, semantic);
                let is_right_constant = expr.right.is_constant(in_boolean_position, semantic);
                let is_left_short_circuit =
                    is_left_constant && is_logical_identity(expr.operator, &expr.left);
                let is_right_short_circuit = in_boolean_position
                    && is_right_constant
                    && is_logical_identity(expr.operator, &expr.right);
                (is_left_constant && is_right_constant)
                    || is_left_short_circuit
                    || is_right_short_circuit
            }
            Self::NewExpression(_) => in_boolean_position,
            Self::AssignmentExpression(expr) => match expr.operator {
                AssignmentOperator::Assign => expr.right.is_constant(in_boolean_position, semantic),
                AssignmentOperator::LogicalAnd if in_boolean_position => {
                    is_logical_identity(LogicalOperator::And, &expr.right)
                }
                AssignmentOperator::LogicalOr if in_boolean_position => {
                    is_logical_identity(LogicalOperator::Or, &expr.right)
                }
                _ => false,
            },
            Self::SequenceExpression(sequence_expr) => sequence_expr
                .expressions
                .iter()
                .last()
                .is_some_and(|last| last.is_constant(in_boolean_position, semantic)),
            Self::CallExpression(call_expr) => call_expr.is_constant(in_boolean_position, semantic),
            Self::ParenthesizedExpression(paren_expr) => {
                paren_expr.expression.is_constant(in_boolean_position, semantic)
            }
            Self::Identifier(ident) => {
                ident.name == "undefined" && semantic.is_reference_to_global_variable(ident)
            }
            _ if self.is_literal() => true,
            _ => false,
        }
    }
}

impl<'a> IsConstant<'a, '_> for CallExpression<'a> {
    fn is_constant(&self, _in_boolean_position: bool, semantic: &Semantic<'a>) -> bool {
        if let Expression::Identifier(ident) = &self.callee
            && ident.name == "Boolean"
            && self.arguments.iter().next().is_none_or(|first| first.is_constant(true, semantic))
        {
            return semantic.is_reference_to_global_variable(ident);
        }
        false
    }
}

impl<'a> IsConstant<'a, '_> for Argument<'a> {
    fn is_constant(&self, in_boolean_position: bool, semantic: &Semantic<'a>) -> bool {
        match self {
            Self::SpreadElement(element) => element.is_constant(in_boolean_position, semantic),
            match_expression!(Self) => {
                self.to_expression().is_constant(in_boolean_position, semantic)
            }
        }
    }
}

impl<'a> IsConstant<'a, '_> for ArrayExpressionElement<'a> {
    fn is_constant(&self, in_boolean_position: bool, semantic: &Semantic<'a>) -> bool {
        match self {
            Self::SpreadElement(element) => element.is_constant(in_boolean_position, semantic),
            match_expression!(Self) => {
                self.to_expression().is_constant(in_boolean_position, semantic)
            }
            Self::Elision(_) => true,
        }
    }
}

impl<'a> IsConstant<'a, '_> for SpreadElement<'a> {
    fn is_constant(&self, in_boolean_position: bool, semantic: &Semantic<'a>) -> bool {
        self.argument.is_constant(in_boolean_position, semantic)
    }
}

/// Jump to the outer most of chained parentheses if any
pub fn outermost_paren<'a, 'b>(
    node: &'b AstNode<'a>,
    semantic: &'b Semantic<'a>,
) -> &'b AstNode<'a> {
    let mut node = node;

    loop {
        let parent = semantic.nodes().parent_node(node.id());
        if let AstKind::ParenthesizedExpression(_) = parent.kind() {
            node = parent;
            continue;
        }

        break;
    }

    node
}

pub fn outermost_paren_parent<'a, 'b>(
    node: &'b AstNode<'a>,
    semantic: &'b Semantic<'a>,
) -> Option<&'b AstNode<'a>> {
    semantic
        .nodes()
        .ancestors(node.id())
        .find(|parent| !matches!(parent.kind(), AstKind::ParenthesizedExpression(_)))
}

