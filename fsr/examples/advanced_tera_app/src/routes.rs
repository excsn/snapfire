use snapfire_fsr_core::{CacheKey, DataSourceId, ModuleId, NodeId, PlanNode, SlotName};

fn layout_over(content: PlanNode) -> PlanNode {
  let mut layout = PlanNode::new(NodeId(0), ModuleId::new("layout.tera", "default"));
  layout.data_source = Some(DataSourceId("layout_loader".into()));
  layout.children.push((SlotName("content".into()), content));
  layout
}

pub fn index_plan() -> PlanNode {
  let mut page = PlanNode::new(NodeId(4), ModuleId::new("index.tera", "default"));
  page.data_source = Some(DataSourceId("chrome_loader".into()));
  layout_over(page)
}

pub fn hydrate_plan() -> PlanNode {
  let mut page = PlanNode::new(NodeId(5), ModuleId::new("hydrate.tera", "default"));
  page.data_source = Some(DataSourceId("hydrate_loader".into()));
  layout_over(page)
}

pub fn login_plan() -> PlanNode {
  let mut page = PlanNode::new(NodeId(3), ModuleId::new("login.tera", "default"));
  page.data_source = Some(DataSourceId("chrome_loader".into()));
  layout_over(page)
}

pub fn dash_plan() -> PlanNode {
  let mut page = PlanNode::new(NodeId(1), ModuleId::new("page.tera", "default"));
  page.data_source = Some(DataSourceId("servers_loader".into()));
  page.cache_key = Some(CacheKey("dash_page".into()));
  page.error = Some(ModuleId::new("error_section.tera", "default"));
  layout_over(page)
}

pub fn slow_plan() -> PlanNode {
  let mut chart = PlanNode::new(NodeId(2), ModuleId::new("chart_section.tera", "default"));
  chart.data_source = Some(DataSourceId("slow_chart_loader".into()));
  chart.deferred = true;
  chart.fallback = Some(ModuleId::new("chart_loading.tera", "default"));

  let mut page = PlanNode::new(NodeId(1), ModuleId::new("stream_page.tera", "default"));
  page.data_source = Some(DataSourceId("servers_loader".into()));
  page.children.push((SlotName("chart".into()), chart));

  layout_over(page)
}
