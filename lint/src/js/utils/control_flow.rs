// Derived from oxc_linter at oxc crates_v0.155.0, MIT License, Copyright (c) 2024-present VoidZero Inc. & Contributors.

use crate::js::context::LintContext;
use oxc_ast::AstKind;
use oxc_cfg::{
    EdgeType, ErrorEdgeKind, EvalConstConditionResult, Instruction, InstructionKind,
    graph::{
        Direction,
        visit::{Control, DfsEvent, EdgeRef, set_depth_first_search},
    },
};

pub fn effective_unreachable_blocks(ctx: &LintContext<'_>) -> Vec<bool> {
    let nodes = ctx.nodes();
    let cfg = ctx.cfg();
    let graph = cfg.graph();
    let mut unreachable = vec![true; cfg.basic_blocks.len()];
    let mut infinite_loops = Vec::new();

    for node in graph.node_indices() {
        let is_unreachable = cfg.basic_block(node).is_unreachable();
        unreachable[node.index()] = is_unreachable;

        if !is_unreachable
            && let Some(loop_) = cfg.is_infinite_loop_start(node, |instruction| match instruction {
                Instruction { kind: InstructionKind::Condition, node_id: Some(id) } => {
                    match nodes.kind(*id) {
                        AstKind::BooleanLiteral(lit) => EvalConstConditionResult::Eval(lit.value),
                        _ => EvalConstConditionResult::Fail,
                    }
                }
                _ => EvalConstConditionResult::NotFound,
            })
        {
            infinite_loops.push(loop_);
        }
    }

    for loop_ in infinite_loops {
        let starts: Vec<_> = graph
            .edges_directed(loop_.1, Direction::Outgoing)
            .filter(|edge| matches!(edge.weight(), EdgeType::Normal))
            .map(|edge| edge.target())
            .collect();

        let _: Control<()> = set_depth_first_search(graph, starts, |event| match event {
            DfsEvent::Discover(node, _) => {
                let mut incoming = graph.edges_directed(node, Direction::Incoming);
                if incoming.any(|edge| match edge.weight() {
                    EdgeType::NewFunction
                    | EdgeType::Finalize
                    | EdgeType::Error(ErrorEdgeKind::Explicit) => true,
                    EdgeType::Normal | EdgeType::Jump
                        if edge.source() != loop_.1 && !unreachable[edge.source().index()] =>
                    {
                        true
                    }
                    EdgeType::Jump
                        if cfg.basic_block(edge.source()).instructions().iter().any(
                            |instruction| matches!(instruction.kind, InstructionKind::Break(_)),
                        ) =>
                    {
                        true
                    }
                    _ => false,
                }) {
                    Control::Prune
                } else {
                    unreachable[node.index()] = true;
                    Control::Continue
                }
            }
            _ => Control::Continue,
        });
    }

    unreachable
}
