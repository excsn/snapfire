use snapfire_fsr_core::{CacheKey, DataSourceId, ModuleId, NodeId, PlanNode, SlotName};
use snapfire_fsr_runtime::{EntryId, MatchitMatcher, TableResolver};

pub const DASH: EntryId = EntryId(0);
pub const SLOW: EntryId = EntryId(1);
pub const LOGIN: EntryId = EntryId(2);
pub const INDEX: EntryId = EntryId(3);

/// Metadata is data on the route: each entry names the source whose data the
/// head is computed from, before rendering starts.
pub fn metadata_source(entry: EntryId) -> Option<DataSourceId> {
  match entry {
    DASH | SLOW => Some(DataSourceId("meta_loader".into())),
    _ => None,
  }
}

pub fn matcher() -> MatchitMatcher {
  let mut matcher = MatchitMatcher::new();
  matcher.insert("/dash/{section}", DASH).expect("route pattern");
  matcher.insert("/slow/{section}", SLOW).expect("route pattern");
  matcher.insert("/login", LOGIN).expect("route pattern");
  matcher.insert("/", INDEX).expect("route pattern");
  matcher
}

pub fn resolver() -> TableResolver {
  let mut resolver = TableResolver::new();
  resolver.insert(DASH, dash_plan());
  resolver.insert(SLOW, slow_plan());
  resolver.insert(LOGIN, login_plan());
  resolver.insert(INDEX, index_plan());
  resolver
}

fn layout_over(content: PlanNode) -> PlanNode {
  let mut layout = PlanNode::new(NodeId(0), ModuleId::new("layout.tera", "default"));
  layout.data_source = Some(DataSourceId("layout_loader".into()));
  layout.children.push((SlotName("content".into()), content));
  layout
}

fn index_plan() -> PlanNode {
  let mut page = PlanNode::new(NodeId(4), ModuleId::new("index.tera", "default"));
  page.data_source = Some(DataSourceId("chrome_loader".into()));
  layout_over(page)
}

fn login_plan() -> PlanNode {
  let mut page = PlanNode::new(NodeId(3), ModuleId::new("login.tera", "default"));
  page.data_source = Some(DataSourceId("chrome_loader".into()));
  layout_over(page)
}

fn dash_plan() -> PlanNode {
  let mut page = PlanNode::new(NodeId(1), ModuleId::new("page.tera", "default"));
  page.data_source = Some(DataSourceId("servers_loader".into()));
  page.cache_key = Some(CacheKey("dash_page".into()));
  page.error = Some(ModuleId::new("error_section.tera", "default"));
  layout_over(page)
}

fn slow_plan() -> PlanNode {
  let mut chart = PlanNode::new(NodeId(2), ModuleId::new("chart_section.tera", "default"));
  chart.data_source = Some(DataSourceId("slow_chart_loader".into()));
  chart.deferred = true;
  chart.fallback = Some(ModuleId::new("chart_loading.tera", "default"));

  let mut page = PlanNode::new(NodeId(1), ModuleId::new("stream_page.tera", "default"));
  page.data_source = Some(DataSourceId("servers_loader".into()));
  page.children.push((SlotName("chart".into()), chart));

  layout_over(page)
}
