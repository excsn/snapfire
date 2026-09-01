use futures_util::stream::{self, BoxStream};
use snapfire_fsr_core::{Data, ModuleId, Node, SlotName};

/// One item of an evaluator's output stream. The stream is chunking of complete
/// output; evaluators never produce `Pending`, holes belong to the assembler.
/// `Slot` is the stitch point where a plan child's tree lands.
#[derive(Debug, Clone, PartialEq)]
pub enum Chunk {
  Node(Node),
  Slot(SlotName),
}

pub type NodeChunks = BoxStream<'static, Result<Chunk, EvalError>>;

#[derive(Debug, Clone, thiserror::Error)]
#[error("evaluate {module}: {message}")]
pub struct EvalError {
  pub module: String,
  pub message: String,
}

pub trait Evaluator: Send + Sync {
  fn evaluate(&self, module: &ModuleId, props: &Data) -> NodeChunks;
}

/// Declines to evaluate: emits a `Client` node so the browser mounts the
/// module. This is what makes "no server JS" a configuration.
pub struct NullEvaluator;

impl Evaluator for NullEvaluator {
  fn evaluate(&self, module: &ModuleId, props: &Data) -> NodeChunks {
    let node = Node::Client {
      module: module.clone(),
      props: props.clone(),
      children: Vec::new(),
      ssr: None,
    };
    Box::pin(stream::iter([Ok(Chunk::Node(node))]))
  }
}
