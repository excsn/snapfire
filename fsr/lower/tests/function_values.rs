//! An object the application builds with function-valued fields: a call of a field,
//! by name or through a computed key, applies that function.

use std::path::PathBuf;
use std::sync::Arc;

use snapfire_fsr_core::ValueMap;
use snapfire_fsr_ir::render::{prepare, Components};
use snapfire_fsr_ir::Interpreter;
use snapfire_fsr_lower::component::ComponentSet;

fn html(tag: &str, page: &str) -> Result<String, String> {
  let dir: PathBuf = std::env::temp_dir().join(format!("fsr_function_values_{}_{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  let path = dir.join("routes/page.tsx");
  std::fs::create_dir_all(path.parent().unwrap()).unwrap();
  std::fs::write(path, page).unwrap();
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

#[test]
fn a_method_of_an_object_the_module_built_is_called_by_name_and_through_a_computed_key() {
  let page = "const logger = {\n  info: (message: string) => `info: ${message}`,\n  warn: (message: string, extra: number) => `warn: ${message} ${extra}`,\n};\n\nexport default function Page() {\n  const level = \"warn\";\n  return <p>{logger.info(\"a\")} | {logger[level](\"b\", 2)} | {logger[\"info\"](\"c\")}</p>;\n}\n";
  assert_eq!(html("methods", page).as_deref(), Ok("<p>info: a | warn: b 2 | info: c</p>"));
}

#[test]
fn a_method_of_an_object_a_body_built_is_called_through_a_computed_key() {
  let page = "export default function Page() {\n  const handlers = { error: (m: string) => `E ${m}`, other: (m: string) => `O ${m}` };\n  const kind = \"x\" === \"x\" ? \"error\" : \"other\";\n  return <p>{handlers[kind](\"boom\")}</p>;\n}\n";
  assert_eq!(html("local", page).as_deref(), Ok("<p>E boom</p>"));
}
