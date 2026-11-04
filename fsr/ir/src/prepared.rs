use std::sync::Arc;

use snapfire_fsr_core::Value;
use snapfire_fsr_runtime::RequestCtx;

use crate::ast::{Body, Consts};
use crate::compiled::Program;
use crate::interp::{Fail, Interpreter, Outcome, Prepared};
use crate::native::{plan_hash, Native, NativeBody};

/// A lowered body as the host runs it, whatever row it answers: its [`Program`] built once, the plan a compiled body must have been written from and, in a production build, that compiled body.
#[derive(Clone)]
pub struct PreparedBody {
  body: Arc<Body>,
  program: Option<Arc<Program>>,
  plan: u64,
  native: Option<NativeBody>,
}

impl PreparedBody {
  /// `body` read with `consts`, the constants of the plan it came from.
  pub fn new(body: Body, consts: Option<&Consts>) -> Self {
    let plan = plan_hash(&body, consts);
    let program = Program::of(&body).map(Arc::new);
    Self { body: Arc::new(body), program, plan, native: None }
  }

  /// Runs `native` in place of the body. [`Self::compiled_from`] attaches one only when its plan matches.
  pub fn with_native(mut self, native: NativeBody) -> Self {
    self.native = Some(native);
    self
  }

  /// The body with the first of `compiled` written from its plan attached. With none written from it, the body is returned as it is.
  pub fn compiled_from(self, compiled: &[&Native]) -> Self {
    match compiled.iter().find(|native| native.plan == self.plan) {
      Some(native) => self.with_native(native.body),
      None => self,
    }
  }

  /// The [`plan_hash`] of the body and the constants it was read with.
  pub fn plan(&self) -> u64 {
    self.plan
  }

  pub fn is_compiled(&self) -> bool {
    self.native.is_some()
  }

  pub fn body(&self) -> &Body {
    &self.body
  }

  pub async fn run(&self, interpreter: &Interpreter, ctx: &RequestCtx, input: Option<Value>) -> Result<Outcome, Fail> {
    let prepared = match (self.native, self.program.as_deref()) {
      (Some(native), _) => Prepared::Native(native),
      (None, Some(program)) => Prepared::Compiled(program),
      (None, None) => Prepared::No,
    };
    interpreter.run_with(&self.body, prepared, ctx, input).await
  }
}

impl std::ops::Deref for PreparedBody {
  type Target = Body;

  fn deref(&self) -> &Body {
    &self.body
  }
}

impl From<Body> for PreparedBody {
  fn from(body: Body) -> Self {
    Self::new(body, None)
  }
}
