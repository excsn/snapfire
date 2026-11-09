//! Reading and writing oxc's tree in the shapes the lowerers match on.

use oxc_allocator::{Allocator, CloneIn};
use oxc_ast::ast as js;
use oxc_ast::builder::AstBuilder;
use oxc_span::{GetSpan, Span};

/// What follows the dot or sits in the brackets of a member expression.
#[derive(Clone, Copy)]
pub(crate) enum MemberProp<'a> {
  /// `obj.name`.
  Ident(&'a js::IdentifierName<'a>),
  /// `obj[expr]`.
  Computed(&'a js::Expression<'a>),
  /// `obj.#name`.
  Private(&'a js::PrivateIdentifier<'a>),
}

/// What follows the object of a member read.
pub(crate) fn prop<'a>(member: &'a js::MemberExpression<'a>) -> MemberProp<'a> {
  match member {
    js::MemberExpression::StaticMemberExpression(m) => MemberProp::Ident(&m.property),
    js::MemberExpression::ComputedMemberExpression(m) => MemberProp::Computed(&m.expression),
    js::MemberExpression::PrivateFieldExpression(m) => MemberProp::Private(&m.field),
  }
}

/// The expression an argument passes or spreads.
pub(crate) fn arg_expr<'s, 'a>(a: &'s js::Argument<'a>) -> &'s js::Expression<'a> {
  match a {
    js::Argument::SpreadElement(s) => &s.argument,
    other => other.to_expression(),
  }
}

/// Builds nodes in an arena.
#[derive(Clone, Copy)]
pub(crate) struct Build<'a> {
  pub(crate) arena: &'a Allocator,
}

impl<'a> Build<'a> {
  pub(crate) fn new(arena: &'a Allocator) -> Self {
    Self { arena }
  }

  pub(crate) fn ast(&self) -> AstBuilder<'a> {
    AstBuilder::new(self.arena)
  }

  pub(crate) fn str(&self, text: &str) -> &'a str {
    self.arena.alloc_str(text)
  }

  pub(crate) fn vec<T>(&self, items: impl IntoIterator<Item = T>) -> oxc_allocator::Vec<'a, T> {
    oxc_allocator::Vec::from_iter_in(items, &self.arena)
  }

  pub(crate) fn boxed<T>(&self, value: T) -> oxc_allocator::Box<'a, T> {
    oxc_allocator::Box::new_in(value, &self.arena)
  }

  /// Statements kept in the arena as one slice.
  pub(crate) fn stmts(&self, stmts: impl IntoIterator<Item = js::Statement<'a>>) -> &'a [js::Statement<'a>] {
    self.alloc(self.vec(stmts)).as_slice()
  }

  pub(crate) fn alloc<T>(&self, value: T) -> &'a T {
    self.arena.alloc(value)
  }

  /// A copy of `node` in this arena, keeping what each identifier names.
  pub(crate) fn copy<T: CloneIn<'a>>(&self, node: &T) -> T::Cloned {
    node.clone_in_with_semantic_ids(self.arena)
  }

  /// An identifier the build writes, which names nothing the module analysed.
  pub(crate) fn ident(&self, name: &str, span: Span) -> js::Expression<'a> {
    js::Expression::new_identifier(span, self.str(name), &self.ast())
  }

  pub(crate) fn ident_ref(&self, name: &str, span: Span) -> js::IdentifierReference<'a> {
    js::IdentifierReference::new(span, self.str(name), &self.ast())
  }

  pub(crate) fn binding(&self, name: &str, span: Span) -> js::BindingPattern<'a> {
    js::BindingPattern::new_binding_identifier(span, self.str(name), &self.ast())
  }

  pub(crate) fn boolean(&self, value: bool, span: Span) -> js::Expression<'a> {
    js::Expression::new_boolean_literal(span, value, &self.ast())
  }

  pub(crate) fn expr_stmt(&self, expr: js::Expression<'a>) -> js::Statement<'a> {
    js::Statement::new_expression_statement(expr.span(), expr, &self.ast())
  }

  pub(crate) fn block(&self, stmts: impl IntoIterator<Item = js::Statement<'a>>, span: Span) -> js::Statement<'a> {
    js::Statement::new_block_statement(span, self.vec(stmts), &self.ast())
  }

  pub(crate) fn number(&self, value: f64, span: Span) -> js::Expression<'a> {
    js::Expression::new_numeric_literal(span, value, None, js::NumberBase::Decimal, &self.ast())
  }

  pub(crate) fn binary(&self, left: js::Expression<'a>, op: js::BinaryOperator, right: js::Expression<'a>, span: Span) -> js::Expression<'a> {
    js::Expression::new_binary_expression(span, left, op, right, &self.ast())
  }

  /// A read of a name the module binds, keeping what `id` names.
  pub(crate) fn read(&self, id: &js::IdentifierReference) -> js::Expression<'a> {
    js::Expression::Identifier(self.boxed(self.copy(id)))
  }

  pub(crate) fn return_stmt(&self, value: Option<js::Expression<'a>>, span: Span) -> js::Statement<'a> {
    js::Statement::new_return_statement(span, value, &self.ast())
  }

  /// The value an update target reads before it is written: a name or a member, copied.
  pub(crate) fn updated(&self, target: &js::SimpleAssignmentTarget) -> js::Expression<'a> {
    match target {
      js::SimpleAssignmentTarget::AssignmentTargetIdentifier(id) => self.read(id),
      other => match other.as_member_expression() {
        Some(member) => js::Expression::from(self.copy(member)),
        None => self.ident("undefined", other.span()),
      },
    }
  }

  pub(crate) fn paren(&self, expr: js::Expression<'a>, span: Span) -> js::Expression<'a> {
    js::Expression::new_parenthesized_expression(span, expr, &self.ast())
  }
}

/// An object literal's property in the shapes JavaScript writes it.
#[derive(Clone, Copy)]
pub(crate) enum Prop<'a> {
  /// `{ name }`.
  Shorthand(&'a js::IdentifierReference<'a>),
  /// `{ key: value }`, and a method the module's methods pass rewrote to one.
  KeyValue(&'a js::ObjectProperty<'a>),
  /// `{ get key() {} }`.
  Getter(&'a js::ObjectProperty<'a>),
  /// `{ set key(v) {} }`.
  Setter(&'a js::ObjectProperty<'a>),
  /// `{ key() {} }` in an expression parsed after the module.
  Method(&'a js::ObjectProperty<'a>),
}

impl<'a> Prop<'a> {
  pub(crate) fn span(self) -> Span {
    match self {
      Prop::Shorthand(id) => id.span,
      Prop::KeyValue(p) | Prop::Getter(p) | Prop::Setter(p) | Prop::Method(p) => p.span,
    }
  }
}

pub(crate) fn prop_of<'a>(p: &'a js::ObjectProperty<'a>) -> Prop<'a> {
  match p.kind {
    js::PropertyKind::Get => Prop::Getter(p),
    js::PropertyKind::Set => Prop::Setter(p),
    js::PropertyKind::Init if p.method => Prop::Method(p),
    js::PropertyKind::Init => match (&p.value, p.shorthand) {
      (js::Expression::Identifier(id), true) => Prop::Shorthand(id),
      _ => Prop::KeyValue(p),
    },
  }
}

/// A destructuring property in the shapes JavaScript writes it.
#[derive(Clone, Copy)]
pub(crate) enum PatProp<'a> {
  /// `{ key: pattern }`.
  KeyValue(&'a js::BindingProperty<'a>),
  /// `{ name }` or `{ name = value }`.
  Assign(AssignProp<'a>),
  /// `{ ...rest }`.
  Rest(&'a js::BindingRestElement<'a>),
}

#[derive(Clone, Copy)]
pub(crate) struct AssignProp<'a> {
  pub(crate) key: &'a js::BindingIdentifier<'a>,
  pub(crate) value: Option<&'a js::Expression<'a>>,
  pub(crate) span: Span,
}

impl<'a> PatProp<'a> {
  pub(crate) fn span(self) -> Span {
    match self {
      PatProp::KeyValue(p) => p.span,
      PatProp::Assign(a) => a.span,
      PatProp::Rest(r) => r.span,
    }
  }
}

/// The properties of an object pattern in order, the rest last.
pub(crate) fn pat_props<'a>(object: &'a js::ObjectPattern<'a>) -> Vec<PatProp<'a>> {
  let mut out: Vec<PatProp<'a>> = object
    .properties
    .iter()
    .map(|p| match (&p.value, p.shorthand) {
      (js::BindingPattern::BindingIdentifier(key), true) => PatProp::Assign(AssignProp { key, value: None, span: p.span }),
      (js::BindingPattern::AssignmentPattern(assign), true) => match &assign.left {
        js::BindingPattern::BindingIdentifier(key) => PatProp::Assign(AssignProp { key, value: Some(&assign.right), span: p.span }),
        _ => PatProp::KeyValue(p),
      },
      _ => PatProp::KeyValue(p),
    })
    .collect();
  if let Some(rest) = &object.rest {
    out.push(PatProp::Rest(rest));
  }
  out
}

/// An element of an array pattern: a pattern, or the rest that ends it.
#[derive(Clone, Copy)]
pub(crate) enum Elem<'a> {
  Pat(&'a js::BindingPattern<'a>),
  Rest(&'a js::BindingRestElement<'a>),
}

/// The elements of an array pattern in order, a hole as `None`, the rest last.
pub(crate) fn pat_elems<'a>(array: &'a js::ArrayPattern<'a>) -> Vec<Option<Elem<'a>>> {
  let mut out: Vec<Option<Elem<'a>>> = array.elements.iter().map(|e| e.as_ref().map(Elem::Pat)).collect();
  if let Some(rest) = &array.rest {
    out.push(Some(Elem::Rest(rest)));
  }
  out
}

/// A node a visitor can walk, so a walk reads `node.walk(&mut visitor)` whatever the node's type.
pub(crate) trait Walk<'a> {
  fn walk<V: oxc_ast_visit::Visit<'a>>(&self, v: &mut V);
}

/// A node a mutating visitor can walk.
pub(crate) trait WalkMut<'a> {
  fn walk_mut<V: oxc_ast_visit::VisitMut<'a>>(&mut self, v: &mut V);
}

macro_rules! walks {
  ($($ty:ident => $visit:ident, $visit_mut:ident;)*) => {
    $(
      impl<'a> Walk<'a> for js::$ty<'a> {
        fn walk<V: oxc_ast_visit::Visit<'a>>(&self, v: &mut V) {
          v.$visit(self);
        }
      }
      impl<'a> WalkMut<'a> for js::$ty<'a> {
        fn walk_mut<V: oxc_ast_visit::VisitMut<'a>>(&mut self, v: &mut V) {
          v.$visit_mut(self);
        }
      }
    )*
  };
}

walks! {
  Program => visit_program, visit_program;
  Statement => visit_statement, visit_statement;
  Expression => visit_expression, visit_expression;
  BindingPattern => visit_binding_pattern, visit_binding_pattern;
  ArrowFunctionExpression => visit_arrow_function_expression, visit_arrow_function_expression;
  FunctionBody => visit_function_body, visit_function_body;
  CallExpression => visit_call_expression, visit_call_expression;
  NewExpression => visit_new_expression, visit_new_expression;
  JSXElement => visit_jsx_element, visit_jsx_element;
  JSXFragment => visit_jsx_fragment, visit_jsx_fragment;
  JSXChild => visit_jsx_child, visit_jsx_child;
  JSXAttributeItem => visit_jsx_attribute_item, visit_jsx_attribute_item;
  Class => visit_class, visit_class;
  ObjectProperty => visit_object_property, visit_object_property;
  ObjectExpression => visit_object_expression, visit_object_expression;
  Argument => visit_argument, visit_argument;
  FormalParameters => visit_formal_parameters, visit_formal_parameters;
  FormalParameter => visit_formal_parameter, visit_formal_parameter;
  BlockStatement => visit_block_statement, visit_block_statement;
  VariableDeclaration => visit_variable_declaration, visit_variable_declaration;
  VariableDeclarator => visit_variable_declarator, visit_variable_declarator;
  MemberExpression => visit_member_expression, visit_member_expression;
  AssignmentTarget => visit_assignment_target, visit_assignment_target;
  SimpleAssignmentTarget => visit_simple_assignment_target, visit_simple_assignment_target;
  TemplateLiteral => visit_template_literal, visit_template_literal;
  SwitchCase => visit_switch_case, visit_switch_case;
  CatchClause => visit_catch_clause, visit_catch_clause;
  ForStatement => visit_for_statement, visit_for_statement;
  ForStatementInit => visit_for_statement_init, visit_for_statement_init;
  ForStatementLeft => visit_for_statement_left, visit_for_statement_left;
  ForOfStatement => visit_for_of_statement, visit_for_of_statement;
  IfStatement => visit_if_statement, visit_if_statement;
  TryStatement => visit_try_statement, visit_try_statement;
  ReturnStatement => visit_return_statement, visit_return_statement;
  ExpressionStatement => visit_expression_statement, visit_expression_statement;
  ImportDeclaration => visit_import_declaration, visit_import_declaration;
  TSType => visit_ts_type, visit_ts_type;
  TSTypeAnnotation => visit_ts_type_annotation, visit_ts_type_annotation;
}

impl<'a> Walk<'a> for js::Function<'a> {
  fn walk<V: oxc_ast_visit::Visit<'a>>(&self, v: &mut V) {
    v.visit_function(self, oxc_syntax::scope::ScopeFlags::Function);
  }
}

impl<'a> WalkMut<'a> for js::Function<'a> {
  fn walk_mut<V: oxc_ast_visit::VisitMut<'a>>(&mut self, v: &mut V) {
    v.visit_function(self, oxc_syntax::scope::ScopeFlags::Function);
  }
}

impl<'a, T: Walk<'a>> Walk<'a> for [T] {
  fn walk<V: oxc_ast_visit::Visit<'a>>(&self, v: &mut V) {
    self.iter().for_each(|node| node.walk(v));
  }
}

impl<'a, T: Walk<'a>> Walk<'a> for Vec<T> {
  fn walk<V: oxc_ast_visit::Visit<'a>>(&self, v: &mut V) {
    self.iter().for_each(|node| node.walk(v));
  }
}

impl<'a, T: Walk<'a>> Walk<'a> for oxc_allocator::Vec<'a, T> {
  fn walk<V: oxc_ast_visit::Visit<'a>>(&self, v: &mut V) {
    self.iter().for_each(|node| node.walk(v));
  }
}

impl<'a, T: Walk<'a>> Walk<'a> for Option<T> {
  fn walk<V: oxc_ast_visit::Visit<'a>>(&self, v: &mut V) {
    if let Some(node) = self {
      node.walk(v);
    }
  }
}

impl<'a, T: Walk<'a>> Walk<'a> for oxc_allocator::Box<'a, T> {
  fn walk<V: oxc_ast_visit::Visit<'a>>(&self, v: &mut V) {
    (**self).walk(v);
  }
}

impl<'a, T: WalkMut<'a>> WalkMut<'a> for [T] {
  fn walk_mut<V: oxc_ast_visit::VisitMut<'a>>(&mut self, v: &mut V) {
    self.iter_mut().for_each(|node| node.walk_mut(v));
  }
}

impl<'a, T: WalkMut<'a>> WalkMut<'a> for Vec<T> {
  fn walk_mut<V: oxc_ast_visit::VisitMut<'a>>(&mut self, v: &mut V) {
    self.iter_mut().for_each(|node| node.walk_mut(v));
  }
}

impl<'a, T: WalkMut<'a>> WalkMut<'a> for oxc_allocator::Vec<'a, T> {
  fn walk_mut<V: oxc_ast_visit::VisitMut<'a>>(&mut self, v: &mut V) {
    self.iter_mut().for_each(|node| node.walk_mut(v));
  }
}

impl<'a, T: WalkMut<'a>> WalkMut<'a> for Option<T> {
  fn walk_mut<V: oxc_ast_visit::VisitMut<'a>>(&mut self, v: &mut V) {
    if let Some(node) = self {
      node.walk_mut(v);
    }
  }
}

impl<'a, T: WalkMut<'a>> WalkMut<'a> for oxc_allocator::Box<'a, T> {
  fn walk_mut<V: oxc_ast_visit::VisitMut<'a>>(&mut self, v: &mut V) {
    (**self).walk_mut(v);
  }
}

/// Whether an argument spreads its expression.
pub(crate) fn is_spread(a: &js::Argument) -> bool {
  matches!(a, js::Argument::SpreadElement(_))
}

/// The function a method property holds: `key() {}`, or `key: function () {}` once rewritten.
pub(crate) fn method_function<'a>(p: &'a js::ObjectProperty<'a>) -> Option<&'a js::Function<'a>> {
  match &p.value {
    js::Expression::FunctionExpression(f) => Some(f),
    _ => None,
  }
}

/// The binding pattern a destructuring assignment's target writes, `{ a, b: [c] }` of `({ a, b: [c] } = v)`; `None` for a target that writes a member.
pub(crate) fn target_pattern<'a>(b: Build<'a>, target: &js::AssignmentTargetPattern<'_>) -> Option<js::BindingPattern<'a>> {
  let ast = b.ast();
  match target {
    js::AssignmentTargetPattern::ObjectAssignmentTarget(o) => {
      let mut props = Vec::new();
      for p in &o.properties {
        match p {
          js::AssignmentTargetProperty::AssignmentTargetPropertyIdentifier(id) => {
            let name = id.binding.name.as_str();
            let binding = b.binding(name, id.binding.span);
            let value = match &id.init {
              Some(init) => js::BindingPattern::new_assignment_pattern(id.span, binding, b.copy(init), &ast),
              None => binding,
            };
            let key = js::PropertyKey::StaticIdentifier(b.boxed(js::IdentifierName::new(id.binding.span, b.str(name), &ast)));
            props.push(js::BindingProperty::new(id.span, key, value, true, false, &ast));
          }
          js::AssignmentTargetProperty::AssignmentTargetPropertyProperty(pp) => {
            let value = maybe_default(b, &pp.binding)?;
            props.push(js::BindingProperty::new(pp.span, b.copy(&pp.name), value, false, pp.computed, &ast));
          }
        }
      }
      let rest = match &o.rest {
        Some(r) => Some(b.boxed(js::BindingRestElement::new(r.span, target_binding(b, &r.target)?, &ast))),
        None => None,
      };
      Some(js::BindingPattern::new_object_pattern(o.span, b.vec(props), rest, &ast))
    }
    js::AssignmentTargetPattern::ArrayAssignmentTarget(a) => {
      let mut elements = Vec::new();
      for element in &a.elements {
        elements.push(match element {
          Some(element) => Some(maybe_default(b, element)?),
          None => None,
        });
      }
      let rest = match &a.rest {
        Some(r) => Some(b.boxed(js::BindingRestElement::new(r.span, target_binding(b, &r.target)?, &ast))),
        None => None,
      };
      Some(js::BindingPattern::new_array_pattern(a.span, b.vec(elements), rest, &ast))
    }
  }
}

fn maybe_default<'a>(b: Build<'a>, target: &js::AssignmentTargetMaybeDefault<'_>) -> Option<js::BindingPattern<'a>> {
  match target {
    js::AssignmentTargetMaybeDefault::AssignmentTargetWithDefault(d) => Some(js::BindingPattern::new_assignment_pattern(d.span, target_binding(b, &d.binding)?, b.copy(&d.init), &b.ast())),
    other => target_binding(b, other.as_assignment_target()?),
  }
}

fn target_binding<'a>(b: Build<'a>, target: &js::AssignmentTarget<'_>) -> Option<js::BindingPattern<'a>> {
  match target {
    js::AssignmentTarget::AssignmentTargetIdentifier(id) => Some(b.binding(id.name.as_str(), id.span)),
    other => target_pattern(b, other.as_assignment_target_pattern()?),
  }
}

/// What a compound assignment computes, `left op right` for its operator: a binary or a logical expression; `None` for a plain `=`.
pub(crate) fn compound<'a>(b: Build<'a>, op: js::AssignmentOperator, left: js::Expression<'a>, right: js::Expression<'a>, span: Span) -> Option<js::Expression<'a>> {
  if let Some(op) = op.to_binary_operator() {
    return Some(js::Expression::new_binary_expression(span, left, op, right, &b.ast()));
  }
  op.to_logical_operator().map(|op| js::Expression::new_logical_expression(span, left, op, right, &b.ast()))
}

/// The expression an assignment target reads: a name or a member, copied into `b`'s arena; `None` for a pattern.
pub(crate) fn target_expr<'a>(b: Build<'a>, target: &js::AssignmentTarget<'_>) -> Option<js::Expression<'a>> {
  match target {
    js::AssignmentTarget::AssignmentTargetIdentifier(id) => Some(js::Expression::Identifier(b.boxed(b.copy(&**id)))),
    other => other.as_member_expression().map(|m| js::Expression::from(b.copy(m))),
  }
}

/// A module-level statement in the shapes the build reads it.
pub(crate) enum Top<'b, 'a> {
  /// `export default function`, named or not.
  DefaultFunction(&'b js::Function<'a>),
  /// `export default class`, named or not.
  DefaultClass(&'b js::Class<'a>),
  /// `export default <expression>`.
  DefaultExpr(&'b js::Expression<'a>),
  /// A declaration, exported or not.
  Decl(&'b js::Declaration<'a>),
  Other,
}

pub(crate) fn top<'b, 'a>(stmt: &'b js::Statement<'a>) -> Top<'b, 'a> {
  match stmt {
    js::Statement::ExportDefaultDeclaration(d) => match &d.declaration {
      js::ExportDefaultDeclarationKind::FunctionDeclaration(f) => Top::DefaultFunction(f),
      js::ExportDefaultDeclarationKind::ClassDeclaration(c) => Top::DefaultClass(c),
      js::ExportDefaultDeclarationKind::TSInterfaceDeclaration(_) => Top::Other,
      other => Top::DefaultExpr(other.to_expression()),
    },
    js::Statement::ExportDeclaration(e) => Top::Decl(&e.declaration),
    other => other.as_declaration().map_or(Top::Other, Top::Decl),
  }
}

/// A block's statements and where it stands.
#[derive(Clone, Copy)]
pub(crate) struct Block<'b, 'a> {
  pub(crate) body: &'b [js::Statement<'a>],
  pub(crate) span: Span,
}

/// An arrow's body: a block of statements or the one expression it returns.
#[derive(Clone, Copy)]
pub(crate) enum ArrowBody<'b, 'a> {
  Block(Block<'b, 'a>),
  Expr(&'b js::Expression<'a>),
}

pub(crate) fn arrow_body_of<'b, 'a>(arrow: &'b js::ArrowFunctionExpression<'a>) -> ArrowBody<'b, 'a> {
  match &arrow.body {
    js::ArrowFunctionBody::FunctionBody(body) => ArrowBody::Block(Block { body: &body.statements, span: body.span }),
    e => ArrowBody::Expr(e.to_expression()),
  }
}
