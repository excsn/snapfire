//! The platform's own globals beside `Intl`: `btoa`, `atob`, `TextEncoder`, `TextDecoder`, `structuredClone` and `crypto.randomUUID()`.

use std::path::PathBuf;
use std::sync::Arc;

use snapfire_fsr_core::ValueMap;
use snapfire_fsr_ir::render::{prepare, Components};
use snapfire_fsr_ir::Interpreter;
use snapfire_fsr_lower::component::ComponentSet;

fn app(tag: &str, files: &[(&str, &str)]) -> PathBuf {
  let dir = std::env::temp_dir().join(format!("fsr_platform_{}_{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  for (name, text) in files {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
  }
  dir
}

fn page(tag: &str, body: &str) -> Result<String, String> {
  let dir = app(tag, &[("routes/page.tsx", &format!("export default function Page() {{\n{body}\n}}\n"))]);
  let mut set = ComponentSet::new(&dir);
  let result = set.lower("routes/page.tsx#default").map_err(|e| e.to_string());
  std::fs::remove_dir_all(&dir).unwrap();
  result?;
  set.settle_fills();
  if let Some((module, residue)) = set.browser_only.first() {
    return Err(format!("{module}: {}", residue.message));
  }
  if let Some((_, text)) = set.browser_texts.first() {
    return Err(text.clone());
  }
  let library: Components = set.components.iter().map(|(module, component)| (module.clone(), Arc::new(prepare(component)))).collect();
  let module = "routes/page.tsx#default";
  Interpreter::default().with_consts(Some(Arc::new(set.consts.clone()))).render_module(module, &library[module], &ValueMap::default(), &library).map(|r| r.html).map_err(|e| format!("{e:?}"))
}

#[test]
fn base64_and_utf8_render_on_the_server() {
  assert_eq!(page("base64", "  return <p>{btoa(\"hi\")} {atob(\"6Q==\")}</p>;").as_deref(), Ok("<p>aGk= é</p>"));
  assert_eq!(page("encoder", "  const bytes = new TextEncoder().encode(\"h€\");\n  return <p>{bytes.length} {new TextDecoder().decode(bytes)}</p>;").as_deref(), Ok("<p>4 h€</p>"));
  let out = page("decoder_label", "  return <p>{new TextDecoder(\"latin1\").decode([104])}</p>;");
  assert!(out.as_ref().is_err_and(|e| e.contains("`TextDecoder` with a label")), "{out:?}");
}

#[test]
fn a_structured_clone_is_a_copy_the_original_does_not_see_written() {
  let out = page("clone", "  const a = { n: [1] };\n  const b = structuredClone(a);\n  b.n.push(2);\n  return <p>{a.n.length} {b.n.length}</p>;");
  assert_eq!(out.as_deref(), Ok("<p>1 2</p>"));
}

#[test]
fn a_random_uuid_is_made_in_a_body_and_left_to_the_browser_in_a_render() {
  let dir = app("uuid", &[("routes/a/page.loader.ts", "export async function load() {\n  return { id: crypto.randomUUID() };\n}\n")]);
  let body = ComponentSet::new(&dir).lower_loader("routes/a/page.loader.ts").unwrap_or_else(|e| panic!("{e}"));
  assert!(format!("{body:?}").contains("name: \"uuid\""), "{body:?}");
  let out = page("uuid_render", "  return <p>{crypto.randomUUID()}</p>;");
  assert!(out.as_ref().is_err_and(|e| e.contains("different ids")), "{out:?}");
}
