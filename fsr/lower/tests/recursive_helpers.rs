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

#[test]
fn a_data_function_that_calls_itself_through_map_sums_a_tree() {
  let page = "type Node = { value: number; children?: Node[] };\n\nconst sum = (node: Node): number => node.value + (node.children ?? []).map(sum).reduce((a, b) => a + b, 0);\n\nexport default function Page() {\n  return <p>{sum({ value: 1, children: [{ value: 2 }, { value: 3, children: [{ value: 4 }] }] })}</p>;\n}\n";
  assert_eq!(html("sum", &[("routes/page.tsx", page)]).as_deref(), Ok("<p>10</p>"));
}

#[test]
fn a_deep_merge_that_calls_itself_fills_a_config_from_its_defaults() {
  let page = "export function deepMergeWithDefaults(partial: unknown, defaults: unknown): unknown {\n  if (partial === null || partial === undefined) {\n    return defaults;\n  }\n  if (typeof defaults !== \"object\" || defaults === null) {\n    return partial;\n  }\n  if (Array.isArray(defaults)) {\n    return Array.isArray(partial) ? partial : defaults;\n  }\n  const result = { ...(partial as Record<string, unknown>) };\n  for (const key of Object.keys(defaults as Record<string, unknown>)) {\n    const defaultValue = (defaults as Record<string, unknown>)[key];\n    if (!(key in result)) {\n      result[key] = defaultValue;\n    } else if (typeof defaultValue === \"object\" && defaultValue !== null && !Array.isArray(defaultValue)) {\n      result[key] = deepMergeWithDefaults(result[key], defaultValue);\n    }\n  }\n  return result;\n}\n\nexport default function Page() {\n  const merged = deepMergeWithDefaults({ a: { x: 1 }, list: [1] }, { a: { x: 0, y: 2 }, b: \"b\", list: [] }) as any;\n  return <p>{merged.a.x} {merged.a.y} {merged.b} {merged.list.length}</p>;\n}\n";
  assert_eq!(html("merge", &[("routes/page.tsx", page)]).as_deref(), Ok("<p>1 2 b 1</p>"));
}

#[test]
fn a_function_that_never_stops_calling_itself_fails_the_render_at_the_bound() {
  let page = "const forever = (n: number): number => forever(n + 1);\n\nexport default function Page() {\n  return <p>{forever(0)}</p>;\n}\n";
  let err = html("forever", &[("routes/page.tsx", page)]).unwrap_err();
  assert!(err.contains("`forever` called itself") && err.contains("deep"), "{err}");
}
