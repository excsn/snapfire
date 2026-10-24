//! The IR: the form a loader or action body takes once the build has read it,
//! and the interpreter that runs it.

pub mod ast;
pub mod bind;
pub mod catalog;
pub mod ext;
pub mod host;
pub mod interp;
pub mod jsmath;
pub mod jsregex;
pub mod std;
pub mod render;
pub mod sexpr;
mod shadow;

pub use ast::{
  ArithOp, Body, Builtin, CompareOp, Component, Entry, Expr, Lit, Owner, LogicOp, ScopedStyle, ShadowMode, ShadowRoot, Stmt, Tmpl, ParseError, body_free_vars, body_params_read, body_reads_ambient, body_reads_request, body_visit};
pub use shadow::ShadowRootError;
pub use bind::{rendered_nodes, IrAction, IrEvaluator, IrMeta, IrPaths, IrSource, IrStore};
pub use catalog::Catalogs;
pub use ext::{standard_reach, Ambient, Extension, Extensions, Field, Member, Reach, Signature, Ty, STANDARD};
pub use interp::{Clock, Derive, Fail, Interpreter, Outcome};
pub use render::{Frameworks, HandlerRef, ReactMajor, SvelteMajor, VueMajor};
pub use snapfire_fsr_core::{is_place_name, Place};
