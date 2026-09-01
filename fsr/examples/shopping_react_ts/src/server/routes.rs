use snapfire_fsr_core::{DataSourceId, ModuleId, NodeId, PlanNode, SlotName};
use snapfire_fsr_runtime::{EntryId, MatchitMatcher, TableResolver};

use crate::server::shell::SHELL;

pub const CATALOG: EntryId = EntryId(0);
pub const PRODUCT: EntryId = EntryId(1);

pub fn matcher() -> MatchitMatcher {
  let mut matcher = MatchitMatcher::new();
  matcher.insert("/", CATALOG).expect("route pattern");
  matcher.insert("/product/{id}", PRODUCT).expect("route pattern");
  matcher
}

pub fn resolver() -> TableResolver {
  let mut resolver = TableResolver::new();
  resolver.insert(CATALOG, page("catalog_loader", "Catalog", NodeId(1)));
  resolver.insert(PRODUCT, page("product_loader", "Product", NodeId(2)));
  resolver
}

/// Every route is one client component under the shell. No evaluator runs the
/// component, so the server ships data and the browser renders it.
fn page(source: &str, component: &str, id: NodeId) -> PlanNode {
  let mut content = PlanNode::new(id, ModuleId::new("app/main.tsx", component));
  content.data_source = Some(DataSourceId(source.into()));
  content.error = Some(ModuleId::new("app/main.tsx", "Failed"));

  let mut shell = PlanNode::new(NodeId(0), ModuleId::new(SHELL, "document"));
  shell.children.push((SlotName("content".into()), content));
  shell
}
