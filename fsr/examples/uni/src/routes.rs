use snapfire_fsr_core::{DataSourceId, ModuleId, NodeId, PlanNode, SlotName};

/// The layout with its two children: the page in `content` and the tape
/// beside it in `tape`, which is the slot an htmx region asks for by name.
fn layout_over(content: PlanNode) -> PlanNode {
  let mut tape = PlanNode::new(NodeId(9), ModuleId::new("tape.tera", "default"));
  tape.data_source = Some(DataSourceId("tape_loader".into()));

  let mut layout = PlanNode::new(NodeId(1), ModuleId::new("layout.tera", "default"));
  layout.data_source = Some(DataSourceId("layout_loader".into()));
  layout.children.push((SlotName("content".into()), content));
  layout.children.push((SlotName("tape".into()), tape));

  // The document is a segment of its own, so the layout beneath it is one the
  // navigator can morph in place: an island in a segment that renders `<html>`
  // has no region to be patched inside.
  let mut document = PlanNode::new(NodeId(0), ModuleId::new("document.tera", "default"));
  document.children.push((SlotName("content".into()), layout));
  document
}

pub fn board_plan() -> PlanNode {
  let mut page = PlanNode::new(NodeId(2), ModuleId::new("board.tera", "default"));
  page.data_source = Some(DataSourceId("board_loader".into()));
  layout_over(page)
}

pub fn news_plan() -> PlanNode {
  let mut page = PlanNode::new(NodeId(3), ModuleId::new("news.tera", "default"));
  page.data_source = Some(DataSourceId("news_loader".into()));
  layout_over(page)
}
