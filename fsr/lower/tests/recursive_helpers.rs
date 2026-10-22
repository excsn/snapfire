//! A markup helper that calls itself renders the tree it walks, as a component that places itself does.

use std::path::PathBuf;
use std::sync::Arc;

use snapfire_fsr_core::ValueMap;
use snapfire_fsr_ir::render::{prepare, Components};
use snapfire_fsr_ir::Interpreter;
use snapfire_fsr_lower::component::ComponentSet;

fn html(tag: &str, files: &[(&str, &str)]) -> Result<String, String> {
  let dir: PathBuf = std::env::temp_dir().join(format!("fsr_recursive_helpers_{}_{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  for (name, text) in files {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
  }
  let mut set = ComponentSet::new(&dir);
  let result = set.lower("routes/page.tsx#default").map_err(|e| e.to_string());
  std::fs::remove_dir_all(&dir).unwrap();
  result?;
  set.settle_fills();
  if let Some((module, residue)) = set.browser_only.first() {
    return Err(format!("{module}: {}", residue.message));
  }
  let library: Components = set.components.iter().map(|(module, component)| (module.clone(), Arc::new(prepare(component)))).collect();
  let module = "routes/page.tsx#default";
  Interpreter::default().with_consts(Some(Arc::new(set.consts.clone()))).render_module(module, &library[module], &ValueMap::default(), &library).map(|r| r.html).map_err(|e| format!("{e:?}"))
}

const TREE: &str = "{ type: \"p\", children: [{ type: \"text\", text: \"a\" }, { type: \"b\", children: [{ type: \"text\", text: \"b\" }] }] }";

#[test]
fn a_function_rendering_its_children_through_itself_renders_the_tree() {
  let page = format!("type Node = {{ type: string; text?: string; children?: Node[] }};\n\nfunction renderNode(node: Node, i: number): any {{\n  if (node.type === \"text\") return <span key={{i}}>{{node.text}}</span>;\n  const inner = (node.children ?? []).map((child, j) => renderNode(child, j));\n  return node.type === \"b\" ? <b key={{i}}>{{inner}}</b> : <p key={{i}}>{{inner}}</p>;\n}}\n\nexport default function Page() {{\n  return <main>{{renderNode({TREE}, 0)}}</main>;\n}}\n");
  assert_eq!(html("function", &[("routes/page.tsx", &page)]).as_deref(), Ok("<main><p><span>a</span><b><span>b</span></b></p></main>"));
}

#[test]
fn a_component_calling_itself_as_a_function_renders_the_tree() {
  let rich = "type Node = { type: string; text?: string; children?: Node[] };\n\nexport function RichText({ nodes }: { nodes: Node[] }): any {\n  return nodes.map((node, i) => node.type === \"text\" ? <span key={i}>{node.text}</span> : <b key={i}>{RichText({ nodes: node.children ?? [] })}</b>);\n}\n";
  let page = format!("import {{ RichText }} from \"@src/RichText\";\n\nexport default function Page() {{\n  return <main><RichText nodes={{[{TREE}]}} /></main>;\n}}\n");
  assert_eq!(html("component", &[("src/RichText.tsx", rich), ("routes/page.tsx", &page)]).as_deref(), Ok("<main><b><span>a</span><b><span>b</span></b></b></main>"));
}
