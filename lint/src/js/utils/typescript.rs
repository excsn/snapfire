// Ported from oxlint (crates/oxc_linter/src/utils/typescript.rs) at oxc crates_v0.155.0, MIT License, Copyright (c) 2024-present VoidZero Inc. & Contributors.

//! Shared TypeScript utilities and types for linter rules


use oxc_ast::AstKind;
use oxc_semantic::AstNodes;
use oxc_syntax::node::NodeId;

/// Returns `true` when `node_id` has a TypeScript ambient declaration ancestor
/// such as `declare module`, `declare namespace`, or `declare global`,
/// including `global {}` nested inside ambient modules or namespaces.
pub fn has_ambient_typescript_ancestor(node_id: NodeId, nodes: &AstNodes) -> bool {
    nodes.ancestors(node_id).any(|ancestor| match ancestor.kind() {
        AstKind::TSExternalModuleDeclaration(module) => module.declare,
        AstKind::TSNamespaceDeclaration(namespace) => namespace.declare,
        // `TSGlobalDeclaration`s are only valid inside ambient declarations, hence
        // we do not need to check `declare` as it only tracks an explicit `declare global`.
        AstKind::TSGlobalDeclaration(_) => true,
        _ => false,
    })
}
