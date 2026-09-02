//! The IR: the form a loader or action body takes once the build has read it,
//! and the interpreter that runs it. See `_private_docs/IR.md`.

pub mod ast;
pub mod bind;
pub mod interp;

pub use ast::{
  ArithOp, Body, CompareOp, Entry, Expr, Lit, LogicOp, Stmt, ParseError,
};
pub use bind::{IrAction, IrSource};
pub use interp::{Clock, Fail, Interpreter, Outcome};
