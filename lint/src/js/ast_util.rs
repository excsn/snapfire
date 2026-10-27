// Ported from oxlint (crates/oxc_linter/src/ast_util.rs) at oxc crates_v0.155.0, MIT License, Copyright (c) 2024-present VoidZero Inc. & Contributors.



use oxc_ast::{
    AstKind,
    ast::*,
};
use oxc_ecmascript::{ToBoolean, WithoutGlobalReferenceInformation};
use oxc_semantic::{AstNode, AstNodes, NodeId, ReferenceId, Semantic, SymbolId};
use oxc_span::GetSpan;
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

/// Return the innermost `Function` or `ArrowFunctionExpression` Node
/// enclosing the specified node
pub fn get_enclosing_function<'a, 'b>(
    node: &'b AstNode<'a>,
    semantic: &'b Semantic<'a>,
) -> Option<&'b AstNode<'a>> {
    let mut current_node = node;
    loop {
        if matches!(current_node.kind(), AstKind::Program(_)) {
            return None;
        }
        if matches!(current_node.kind(), AstKind::Function(_) | AstKind::ArrowFunctionExpression(_))
        {
            return Some(current_node);
        }
        current_node = semantic.nodes().parent_node(current_node.id());
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

/// Iterate over parents of `node`, skipping nodes that are also ignored by
/// [`Expression::get_inner_expression`].
pub fn iter_outer_expressions<'a, 's>(
    nodes: &'s AstNodes<'a>,
    node_id: NodeId,
) -> impl Iterator<Item = AstKind<'a>> + 's {
    nodes.ancestor_kinds(node_id).filter(|parent| {
        !matches!(
            parent,
            AstKind::ParenthesizedExpression(_)
                | AstKind::TSAsExpression(_)
                | AstKind::TSSatisfiesExpression(_)
                | AstKind::TSInstantiationExpression(_)
                | AstKind::TSNonNullExpression(_)
                | AstKind::TSTypeAssertion(_)
        )
    })
}

pub fn get_declaration_of_variable<'a, 'b>(
    ident: &IdentifierReference<'a>,
    semantic: &'b Semantic<'a>,
) -> Option<&'b AstNode<'a>> {
    let symbol_id = get_symbol_id_of_variable(ident, semantic)?;
    let symbol_table = semantic.scoping();
    Some(semantic.nodes().get_node(symbol_table.symbol_declaration(symbol_id)))
}

pub fn get_declaration_from_reference_id<'a, 'b>(
    reference_id: ReferenceId,
    semantic: &'b Semantic<'a>,
) -> Option<&'b AstNode<'a>> {
    let symbol_table = semantic.scoping();
    let symbol_id = symbol_table.get_reference(reference_id).symbol_id()?;
    Some(semantic.nodes().get_node(symbol_table.symbol_declaration(symbol_id)))
}

pub fn get_symbol_id_of_variable(
    ident: &IdentifierReference,
    semantic: &Semantic<'_>,
) -> Option<SymbolId> {
    semantic.scoping().get_reference(ident.reference_id()).symbol_id()
}

/// Get the innermost static member expression in an assignment target.
///
/// For example, both `this.state.a.b.c` and `this.state.a[key].c` return `this.state`.
pub fn get_outer_member_expression<'a, 'b>(
    assignment: &'b SimpleAssignmentTarget<'a>,
) -> Option<&'b StaticMemberExpression<'a>> {
    let mut member = assignment.as_member_expression()?;

    while let Some(object) = member.object().as_member_expression() {
        member = object;
    }

    match member {
        MemberExpression::StaticMemberExpression(expression) => Some(expression),
        _ => None,
    }
}

/// Check if a node's span is contained within any argument span in a call expression
#[inline]
pub fn is_node_within_call_argument<'a>(
    node: &AstNode<'a>,
    call: &CallExpression<'a>,
    target_arg_index: usize,
) -> bool {
    // Early exit for out-of-bounds index
    if target_arg_index >= call.arguments.len() {
        return false;
    }

    let target_arg = &call.arguments[target_arg_index]; // Direct indexing, no Option unwrap
    let node_span = node.span();
    let arg_span = target_arg.span();
    node_span.start >= arg_span.start && node_span.end <= arg_span.end
}

pub fn is_method_call<'a>(
    call_expr: &CallExpression<'a>,
    objects: Option<&[&'a str]>,
    methods: Option<&[&'a str]>,
    min_arg_count: Option<usize>,
    max_arg_count: Option<usize>,
) -> bool {
    if let Some(min_arg_count) = min_arg_count
        && call_expr.arguments.len() < min_arg_count
    {
        return false;
    }

    if let Some(max_arg_count) = max_arg_count
        && call_expr.arguments.len() > max_arg_count
    {
        return false;
    }

    let Some(member_expr) = call_expr.callee.get_member_expr() else {
        return false;
    };

    if let Some(objects) = objects {
        let Expression::Identifier(ident) = member_expr.object().get_inner_expression() else {
            return false;
        };
        if !objects.contains(&ident.name.as_str()) {
            return false;
        }
    }

    if let Some(methods) = methods {
        let Some(static_property_name) = member_expr.static_property_name() else {
            return false;
        };
        if !methods.iter().any(|method| *method == static_property_name) {
            return false;
        }
    }

    true
}

pub fn could_be_error(expr: &Expression) -> bool {
    match expr.get_inner_expression() {
        Expression::NewExpression(_)
        | Expression::AwaitExpression(_)
        | Expression::CallExpression(_)
        | Expression::ChainExpression(_)
        | Expression::YieldExpression(_)
        | Expression::PrivateFieldExpression(_)
        | Expression::StaticMemberExpression(_)
        | Expression::ComputedMemberExpression(_)
        | Expression::TaggedTemplateExpression(_)
        | Expression::Identifier(_) => true,
        Expression::AssignmentExpression(expr) => {
            if matches!(expr.operator, AssignmentOperator::Assign | AssignmentOperator::LogicalAnd)
            {
                return could_be_error(&expr.right);
            }

            if matches!(
                expr.operator,
                AssignmentOperator::LogicalOr | AssignmentOperator::LogicalNullish
            ) {
                return expr.left.get_expression().is_none_or(could_be_error)
                    || could_be_error(&expr.right);
            }

            false
        }
        Expression::SequenceExpression(expr) => expr.expressions.last().is_some_and(could_be_error),
        Expression::LogicalExpression(expr) => {
            if matches!(expr.operator, LogicalOperator::And) {
                return could_be_error(&expr.right);
            }

            could_be_error(&expr.left) || could_be_error(&expr.right)
        }
        Expression::ConditionalExpression(expr) => {
            could_be_error(&expr.consequent) || could_be_error(&expr.alternate)
        }
        _ => false,
    }
}
