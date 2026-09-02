use std::sync::Arc;

use futures_util::future::BoxFuture;
use snapfire_fsr_core::{Data, Value};
use snapfire_fsr_runtime::{ActionError, ActionHandler, DataSource, LoadError, RequestCtx};

use crate::ast::Body;
use crate::interp::Interpreter;

/// A lowered loader answering a data source id. The body must return an
/// object; its fields are the source's data.
pub struct IrSource {
  id: String,
  body: Arc<Body>,
  interpreter: Interpreter,
}

impl IrSource {
  pub fn new(id: impl Into<String>, body: Body) -> Self {
    Self { id: id.into(), body: Arc::new(body), interpreter: Interpreter::default() }
  }

  pub fn with_interpreter(mut self, interpreter: Interpreter) -> Self {
    self.interpreter = interpreter;
    self
  }
}

impl DataSource for IrSource {
  fn load(&self, ctx: &RequestCtx) -> BoxFuture<'static, Result<Data, LoadError>> {
    let id = self.id.clone();
    let body = self.body.clone();
    let interpreter = self.interpreter.clone();
    let ctx = ctx.clone();
    Box::pin(async move {
      let outcome = interpreter
        .run(&body, &ctx, None)
        .await
        .map_err(|fail| LoadError { source_id: id.clone(), message: fail.message })?;
      match outcome.value {
        Value::Map(data) => Ok(data),
        other => Err(LoadError {
          source_id: id,
          message: format!("a loader must return an object, got {}", kind_name(&other)),
        }),
      }
    })
  }
}

/// A lowered action answering an action id.
pub struct IrAction {
  body: Arc<Body>,
  interpreter: Interpreter,
}

impl IrAction {
  pub fn new(body: Body) -> Self {
    Self { body: Arc::new(body), interpreter: Interpreter::default() }
  }

  pub fn with_interpreter(mut self, interpreter: Interpreter) -> Self {
    self.interpreter = interpreter;
    self
  }
}

impl ActionHandler for IrAction {
  fn call(&self, ctx: RequestCtx, input: Value) -> BoxFuture<'static, Result<Value, ActionError>> {
    let body = self.body.clone();
    let interpreter = self.interpreter.clone();
    Box::pin(async move {
      interpreter
        .run(&body, &ctx, Some(input))
        .await
        .map(|outcome| outcome.value)
        .map_err(|fail| ActionError::new(fail.kind, fail.message))
    })
  }
}

pub(crate) fn kind_name(value: &Value) -> &'static str {
  match value {
    Value::Null => "null",
    Value::Bool(_) => "bool",
    Value::Int(_) => "int",
    Value::UInt(_) => "uint",
    Value::F32(_) | Value::F64(_) => "float",
    Value::Str(_) => "string",
    Value::Bytes(_) => "bytes",
    Value::TypedArray(_) => "typed array",
    Value::Seq(_) => "array",
    Value::Map(_) => "object",
    Value::Variant { .. } => "variant",
    Value::Ref { .. } => "ref",
  }
}
