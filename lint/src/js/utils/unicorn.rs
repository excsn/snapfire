// Ported from oxlint (crates/oxc_linter/src/utils/unicorn.rs) at oxc crates_v0.155.0, MIT License, Copyright (c) 2024-present VoidZero Inc. & Contributors.

use smallvec::SmallVec;

use oxc_ast::{
    AstKind,
    ast::{
        BindingPattern, CallExpression, Expression, FormalParameters, FunctionBody,
        IdentifierReference, ImportDeclarationSpecifier, LogicalExpression, MemberExpression,
        Statement,
    },
};
use oxc_semantic::AstNode;
use oxc_span::Span;
use oxc_syntax::operator::LogicalOperator;

use crate::js::LintContext;

mod boolean;
pub use boolean::*;

mod optional_chain;
pub use optional_chain::*;

// Built-in Error constructors
// https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Error#Error_types
pub const BUILT_IN_ERRORS: [&str; 10] = [
    "Error",
    "EvalError",
    "RangeError",
    "ReferenceError",
    "SyntaxError",
    "TypeError",
    "URIError",
    "InternalError",
    "AggregateError",
    "SuppressedError",
];

/// Returns `true` when `ident` resolves to any import binding from `module_name`.
///
/// This checks semantic resolution first (`reference_id` -> `symbol_id`) and then validates:
/// - the symbol is an import binding,
/// - the enclosing declaration source matches `module_name`.
///
/// Named, default, and namespace imports are all supported.
pub fn is_import_from_module(
    ident: &IdentifierReference,
    module_name: &str,
    ctx: &LintContext,
) -> bool {
    let reference = ctx.scoping().get_reference(ident.reference_id());
    let Some(symbol_id) = reference.symbol_id() else {
        return false;
    };

    if !ctx.scoping().symbol_flags(symbol_id).is_import() {
        return false;
    }

    let declaration_id = ctx.scoping().symbol_declaration(symbol_id);
    let AstKind::ImportDeclaration(import_decl) = ctx.nodes().parent_kind(declaration_id) else {
        return false;
    };

    import_decl.source.value == module_name
}

/// Returns `true` when `ident` resolves to a named import with the given source module and
/// imported symbol name.
///
/// This checks semantic resolution first (`reference_id` -> `symbol_id`) and then validates:
/// - the symbol is an import binding,
/// - the enclosing declaration source matches `module_name`,
/// - one of the declaration's `ImportSpecifier`s maps to this symbol and `imported_name`.
///
/// Aliased named imports are supported (`import { Foo as Bar }` matches `Bar` for `"Foo"`).
/// Namespace/default imports are intentionally ignored.
pub fn is_import_symbol(
    ident: &IdentifierReference,
    module_name: &str,
    imported_name: &str,
    ctx: &LintContext,
) -> bool {
    if !is_import_from_module(ident, module_name, ctx) {
        return false;
    }

    let reference = ctx.scoping().get_reference(ident.reference_id());
    let Some(symbol_id) = reference.symbol_id() else {
        return false;
    };
    let declaration_id = ctx.scoping().symbol_declaration(symbol_id);
    let AstKind::ImportDeclaration(import_decl) = ctx.nodes().parent_kind(declaration_id) else {
        return false;
    };

    import_decl.specifiers.iter().flatten().any(|specifier| match specifier {
        ImportDeclarationSpecifier::ImportSpecifier(import_specifier) => {
            import_specifier.local.symbol_id() == symbol_id
                && import_specifier.imported.name() == imported_name
        }
        _ => false,
    })
}

pub fn is_node_value_not_dom_node(expr: &Expression) -> bool {
    matches!(
        expr,
        Expression::ArrayExpression(_)
            | Expression::ArrowFunctionExpression(_)
            | Expression::ClassExpression(_)
            | Expression::FunctionExpression(_)
            | Expression::ObjectExpression(_)
            | Expression::TemplateLiteral(_)
            | Expression::StringLiteral(_)
    )
}

pub fn is_empty_stmt(stmt: &Statement) -> bool {
    match stmt {
        Statement::BlockStatement(block_stmt) => {
            if block_stmt.body.is_empty() || block_stmt.body.iter().all(|node| is_empty_stmt(node))
            {
                return true;
            }
            false
        }
        Statement::EmptyStatement(_) => true,
        _ => false,
    }
}

// ref: https://github.com/sindresorhus/eslint-plugin-unicorn/blob/v56.0.0/rules/utils/array-or-object-prototype-property.js
pub fn is_prototype_property(
    member_expr: &MemberExpression,
    property: &str,
    object: Option<&str>,
) -> bool {
    if member_expr.static_property_name().is_none_or(|name| name != property)
        || member_expr.optional()
    {
        return false;
    }

    // `Object.prototype.method` or `Array.prototype.method`
    if let Some(member_expr_obj) = member_expr.object().as_member_expression()
        && let Expression::Identifier(iden) = member_expr_obj.object()
        && member_expr_obj.static_property_name().is_some_and(|name| name == "prototype")
        && object.is_some_and(|val| val == iden.name)
        && !member_expr.optional()
        && !member_expr_obj.optional()
    {
        return true;
    }

    match object {
        // `[].method`
        Some("Array") => is_empty_array_expression(member_expr.object()),

        // `{}.method`
        Some("Object") => {
            if let Expression::ObjectExpression(obj_expr) = member_expr.object() {
                obj_expr.properties.is_empty()
            } else {
                false
            }
        }
        _ => false,
    }
}

pub fn is_empty_array_expression(expr: &Expression) -> bool {
    if let Expression::ArrayExpression(array_expr) = expr {
        array_expr.elements.is_empty()
    } else {
        false
    }
}

pub fn is_empty_object_expression(expr: &Expression) -> bool {
    if let Expression::ObjectExpression(object_expr) = expr {
        object_expr.properties.is_empty()
    } else {
        false
    }
}

pub fn is_logical_expression(node: &AstNode) -> bool {
    matches!(
        node.kind(),
        AstKind::LogicalExpression(LogicalExpression {
            operator: LogicalOperator::And | LogicalOperator::Or,
            ..
        })
    )
}

// gets the name of the first parameter of a function
pub fn get_first_parameter_name<'a>(arg: &'a FormalParameters) -> Option<&'a str> {
    let first_func_param = arg.items.first()?;
    let BindingPattern::BindingIdentifier(first_func_param) = &first_func_param.pattern else {
        return None;
    };
    Some(first_func_param.name.as_str())
}

pub fn get_return_identifier_name<'a>(body: &'a FunctionBody<'_>) -> Option<&'a str> {
    match body.statements.first()? {
        Statement::BlockStatement(block_stmt) => {
            let Statement::ReturnStatement(return_stmt) = block_stmt.body.first()? else {
                return None;
            };

            let Some(Expression::Identifier(ident)) = return_stmt.argument.as_ref() else {
                return None;
            };

            Some(ident.name.as_str())
        }
        Statement::ReturnStatement(return_stmt) => {
            let return_expr = return_stmt.argument.as_ref()?;
            match return_expr {
                Expression::Identifier(ident) => Some(ident.name.as_str()),
                _ => None,
            }
        }
        Statement::ExpressionStatement(expr_stmt) => {
            let Expression::Identifier(ident) = &expr_stmt.expression else {
                return None;
            };

            Some(ident.name.as_str())
        }
        _ => None,
    }
}

pub fn call_expr_member_expr_property_span(call_expr: &CallExpression) -> Span {
    call_expr
        .callee
        .as_member_expression()
        .and_then(MemberExpression::static_property_info)
        .map_or(call_expr.span, |(span, _)| span)
}

pub fn does_expr_match_any_path<'a, P, S>(mut expr: &Expression, paths: P) -> bool
where
    P: IntoIterator<Item = S>,
    S: IntoIterator<Item = &'a str>,
{
    // Member chains are short in practice; keep the segments on the stack.
    let mut path: SmallVec<[&str; 4]> = SmallVec::new();

    while let Some(member_expr) = expr.as_member_expression() {
        let MemberExpression::StaticMemberExpression(static_mem_expr) = member_expr else {
            return false;
        };
        path.push(static_mem_expr.property.name.as_str());

        expr = member_expr.object().get_inner_expression();
    }

    let Expression::Identifier(ident) = expr else { return false };
    path.push(ident.name.as_str());
    let path = path.iter().rev().copied();

    paths.into_iter().any(|expected_path| expected_path.into_iter().eq(path.clone()))
}

pub fn is_string_raw_tagged_template_expression(node: &AstKind) -> bool {
    if let AstKind::TaggedTemplateExpression(tagged_template_expression) = node
        && let Expression::StaticMemberExpression(member_expr) = &tagged_template_expression.tag
        && let Expression::Identifier(ident) = &member_expr.object
        && member_expr.property.name == "raw"
        && ident.name == "String"
    {
        return true;
    }
    false
}
