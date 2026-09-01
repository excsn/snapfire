use futures_util::stream;
use snapfire_fsr_core::{Data, ModuleId, Node, SlotName};
use snapfire_fsr_runtime::{Chunk, Evaluator, NodeChunks};

pub const SHELL: &str = "shell";

/// The document around a client-rendered route. It emits no application markup:
/// the head slot, then the slot the route's own tree lands in.
pub struct ShellEvaluator;

impl Evaluator for ShellEvaluator {
  fn evaluate(&self, _module: &ModuleId, _props: &Data) -> NodeChunks {
    Box::pin(stream::iter([
      Ok(Chunk::Node(Node::raw("<!doctype html><html lang=\"en\"><head>"))),
      Ok(Chunk::Slot(SlotName("head".into()))),
      Ok(Chunk::Node(Node::raw("</head><body><div id=\"app\">"))),
      Ok(Chunk::Slot(SlotName("content".into()))),
      Ok(Chunk::Node(Node::raw("</div></body></html>"))),
    ]))
  }
}
