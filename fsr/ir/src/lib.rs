//! The IR: the form a loader or action body takes once the build has read it,
//! and the interpreter that runs it.

pub mod ast;
pub mod bind;
pub mod catalog;
pub mod ext;
pub mod interp;
pub mod std;
pub mod render;
pub mod sexpr;
mod shadow;

pub use ast::{
  ArithOp, Body, Builtin, CompareOp, Component, Entry, Expr, HydratedBy, Lit, LogicOp, ShadowMode, ShadowRoot, Stmt, Tmpl, ParseError, body_free_vars, body_params_read, body_reads_ambient, body_reads_request, body_visit};
pub use shadow::ShadowRootError;
pub use bind::{rendered_nodes, IrAction, IrEvaluator, IrMeta, IrSource, IrStore};
pub use catalog::Catalogs;
pub use ext::{standard_reach, Ambient, Extension, Extensions, Reach, STANDARD};
pub use interp::{Clock, Fail, Interpreter, Outcome};
pub use render::{Frameworks, ReactMajor};
