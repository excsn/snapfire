use futures_util::stream;
use snapfire_fsr_core::{Data, ModuleId, Node, SlotName};
use snapfire_fsr_runtime::{Chunk, Evaluator, NodeChunks};

/// The document around a client-rendered route: doctype, the head slot, the
/// mount point, the content slot. It emits no application markup.
pub struct DocumentShell;

impl Evaluator for DocumentShell {
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

/// What the head slot carries: the title, the stylesheets, the inlined
/// import map and the entry module, built once at boot.
pub fn head(title: &str, styles: &[String], import_map: Option<&str>, entry: Option<&str>) -> Node {
  let mut head = String::new();
  head.push_str("<meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">");
  if !title.is_empty() {
    head.push_str("<title>");
    head.push_str(&escape(title));
    head.push_str("</title>");
  }
  for href in styles {
    head.push_str("<link rel=\"stylesheet\" href=\"");
    head.push_str(&escape(href));
    head.push_str("\">");
  }
  if let Some(map) = import_map {
    head.push_str("<script type=\"importmap\">");
    head.push_str(map);
    head.push_str("</script>");
  }
  if let Some(entry) = entry {
    head.push_str("<script type=\"module\" src=\"");
    head.push_str(&escape(entry));
    head.push_str("\"></script>");
  }
  Node::raw(head)
}

fn escape(text: &str) -> String {
  text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}
