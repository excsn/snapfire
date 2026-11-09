//! Which binding each identifier of a parsed module names, from oxc's semantic
//! analysis.

use oxc_allocator::Allocator;
use oxc_ast::ast as js;
use oxc_semantic::{Scoping, SemanticBuilder};
use oxc_span::SourceType;
use oxc_syntax::node::NodeId;
use oxc_syntax::reference::{Reference, ReferenceFlags, ReferenceId};
use oxc_syntax::symbol::{SymbolFlags, SymbolId};

/// What an identifier is bound to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Ctxt {
  /// An import or a module-level declaration.
  Top,
  /// A name nothing in the module binds: a global.
  Unresolved,
  /// A binding inside a function, a block or a class.
  Local(SymbolId),
  /// A name the build wrote itself after the module was analysed.
  Written,
}

/// The scopes of one module.
pub(crate) struct Scope {
  scoping: Scoping,
  /// The symbol an identifier the build writes as module-level is bound to.
  top_symbol: SymbolId,
  top_reference: ReferenceId,
  /// The reference an identifier the build writes as naming nothing in the module carries.
  unresolved_reference: ReferenceId,
}

impl Scope {
  fn new(mut scoping: Scoping) -> Self {
    let root = scoping.root_scope_id();
    let top_symbol = scoping.create_symbol(oxc_span::Span::default(), oxc_str::Ident::from("$top"), SymbolFlags::empty(), root, NodeId::DUMMY);
    let top_reference = scoping.create_reference(Reference::new_with_symbol_id(NodeId::DUMMY, top_symbol, root, ReferenceFlags::Read));
    let unresolved_reference = scoping.create_reference(Reference::new(NodeId::DUMMY, root, ReferenceFlags::Read));
    Self { scoping, top_symbol, top_reference, unresolved_reference }
  }

  fn of_symbol(&self, symbol: SymbolId) -> Ctxt {
    match self.scoping.symbol_scope_id(symbol) == self.scoping.root_scope_id() {
      true => Ctxt::Top,
      false => Ctxt::Local(symbol),
    }
  }

  /// What `id` names where it is written.
  pub(crate) fn of(&self, id: &js::IdentifierReference) -> Ctxt {
    let Some(reference) = id.reference_id.get() else { return Ctxt::Written };
    if reference.index() >= self.scoping.references_len() {
      return Ctxt::Written;
    }
    match self.scoping.get_reference(reference).symbol_id() {
      Some(symbol) => self.of_symbol(symbol),
      None => Ctxt::Unresolved,
    }
  }

  /// What a binding binds.
  pub(crate) fn of_binding(&self, id: &js::BindingIdentifier) -> Ctxt {
    match id.symbol_id.get() {
      Some(symbol) if symbol.index() < self.scoping.symbols_len() => self.of_symbol(symbol),
      _ => Ctxt::Written,
    }
  }

  /// Marks `id`, which the build wrote, as naming a module-level binding.
  pub(crate) fn mark_top(&self, id: &js::IdentifierReference) {
    id.reference_id.set(Some(self.top_reference));
  }

  /// Marks `id`, which the build wrote, as naming nothing the module binds.
  pub(crate) fn mark_unresolved(&self, id: &js::IdentifierReference) {
    id.reference_id.set(Some(self.unresolved_reference));
  }

  /// Marks `id`, which the build wrote, as binding at module level.
  pub(crate) fn mark_top_binding(&self, id: &js::BindingIdentifier) {
    id.symbol_id.set(Some(self.top_symbol));
  }
}

/// `source` parsed into `arena` as `file` with its scopes analysed.
pub(crate) struct Analysed<'a> {
  pub program: js::Program<'a>,
  pub scope: Scope,
  pub errors: Vec<oxc_diagnostics::OxcDiagnostic>,
}

pub(crate) fn analyse<'a>(arena: &'a Allocator, source: &str, source_type: SourceType) -> Analysed<'a> {
  let text = arena.alloc_str(source);
  let parsed = oxc_parser::Parser::new(arena, text, source_type).parse();
  let scoping = SemanticBuilder::new().build(&parsed.program).semantic.into_scoping();
  Analysed { program: parsed.program, scope: Scope::new(scoping), errors: parsed.diagnostics.into_vec().into_iter().filter(|d| d.severity == oxc_diagnostics::Severity::Error).collect() }
}
