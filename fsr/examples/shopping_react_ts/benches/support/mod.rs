use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_ir::render::{HOISTED_PROP, Rendered};

/// What React renders for a page: the page root with its props and its own hoisted table, then each island as the browser would mount it, `mount_props` and all, with the islands inside it the same way.
pub fn render_tree(module: &str, props: ValueMap, body: &Rendered) -> Value {
  let mut tree = ValueMap::default();
  tree.insert("module".to_owned(), Value::str(module));
  tree.insert("props".to_owned(), Value::Map(props));
  tree.insert("islands".to_owned(), Value::seq(body.islands.iter().map(|island| render_tree(&island.module, island.mount_props(), &island.body)).collect::<Vec<_>>()));
  Value::Map(tree)
}

/// A page's tree, its props carrying the hoisted table the IR's render took.
pub fn page_tree(module: &str, props: &ValueMap, rendered: &Rendered) -> Value {
  let mut props = props.clone();
  if !rendered.hoisted.is_empty() {
    props.insert(HOISTED_PROP.to_owned(), Value::Map(rendered.hoisted.clone()));
  }
  render_tree(module, props, rendered)
}
