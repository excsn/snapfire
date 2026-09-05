//! The IR: the form a loader or action body takes once the build has read it,
//! and the interpreter that runs it. See `_private_docs/design/IR.md`.

pub mod ast;
pub mod bind;
pub mod interp;
pub mod render;

pub use ast::{
  ArithOp, Body, Builtin, CompareOp, Component, Entry, Expr, Lit, LogicOp, Stmt, Tmpl, ParseError, body_params_read, body_reads_ambient, body_reads_request, body_visit};
pub use bind::{rendered_nodes, IrAction, IrEvaluator, IrMeta, IrSource, IrStore};
pub use interp::{Clock, Fail, Interpreter, Outcome};
