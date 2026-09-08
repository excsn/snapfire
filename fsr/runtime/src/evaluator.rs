use futures_util::stream::{self, BoxStream};
use snapfire_fsr_core::{Data, ModuleId, Node, SlotName, Value};

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
      children: slot_regions(props),
      ssr: None,
    };
    Box::pin(stream::iter([Ok(Chunk::Node(node))]))
  }
}

/// The regions a module the browser owns must still offer its plan children:
/// one `<sf-s>` per name in `$slots`, bare for `content` and carrying
/// `data-sf-name` otherwise, which is the spelling a lowered layout's own
/// markup uses. Where the marker sits inside the component is unknown here and
/// does not matter, because the mounter copies the markup out of the region
/// rather than moving the element.
fn slot_regions(props: &Data) -> Vec<Node> {
  let Some(Value::Seq(slots)) = props.get("$slots") else {
    return Vec::new();
  };
  slots
    .iter()
    .filter_map(|slot| match slot {
      Value::Str(name) if is_slot_name(name) => Some(Node::Seq(vec![
        Node::raw(match name.as_str() {
          "content" => "<sf-s>".to_owned(),
          named => format!("<sf-s data-sf-name=\"{named}\">"),
        }),
        Node::Slot(SlotName(name.to_string())),
        Node::raw("</sf-s>"),
      ])),
      _ => None,
    })
    .collect()
}

fn is_slot_name(name: &str) -> bool {
  !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}
