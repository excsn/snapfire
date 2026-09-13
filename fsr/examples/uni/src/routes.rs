use snapfire_fsr_core::{DataSourceId, ModuleId, NodeId, PlanNode, SlotName};

/// The layout with its two children: the page in `content` and the tape
/// beside it in `tape`, which is the slot an htmx region asks for by name.
fn layout_over(content: PlanNode) -> PlanNode {
  let mut tape = PlanNode::new(NodeId(9), ModuleId::new("tape.tera", "default"));
  tape.data_source = Some(DataSourceId("tape_loader".into()));

  let mut layout = PlanNode::new(NodeId(0), ModuleId::new("layout.tera", "default"));
  layout.data_source = Some(DataSourceId("layout_loader".into()));
  layout.children.push((SlotName("content".into()), content));
  layout.children.push((SlotName("tape".into()), tape));
  layout
}

pub fn board_plan() -> PlanNode {
  let mut page = PlanNode::new(NodeId(1), ModuleId::new("board.tera", "default"));
  page.data_source = Some(DataSourceId("board_loader".into()));
  layout_over(page)
}

pub fn news_plan() -> PlanNode {
  let mut page = PlanNode::new(NodeId(2), ModuleId::new("news.tera", "default"));
  page.data_source = Some(DataSourceId("news_loader".into()));
  layout_over(page)
}
