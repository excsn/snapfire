//! The store keys `@snapfire/fsr-client` names, read by `useStore` as the key strings they hold.

use std::path::PathBuf;
use std::sync::Arc;

use snapfire_fsr_core::ValueMap;
use snapfire_fsr_ir::render::{prepare, Components};
use snapfire_fsr_ir::Interpreter;
use snapfire_fsr_lower::component::ComponentSet;

fn html(tag: &str, files: &[(&str, &str)]) -> Result<String, String> {
  let dir: PathBuf = std::env::temp_dir().join(format!("fsr_client_keys_{}_{tag}", std::process::id()));
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
  Interpreter::default().with_consts(Some(Arc::new(set.consts.clone()))).render_module(module, &library[module], &ValueMap::default(), &library).map(|r| r.islands.iter().map(|island| island.body.html.clone()).collect::<Vec<_>>().join("")).map_err(|e| format!("{e:?}"))
}

#[test]
fn use_store_of_the_clients_online_and_update_keys_lowers() {
  let status = "import { online, updateReady } from \"@snapfire/fsr-client\";\nimport { useStore } from \"@snapfire/fsr-client/react\";\n\nexport default function Status() {\n  const [connected] = useStore(online, true);\n  const [ready] = useStore(updateReady, false);\n  return <p>{connected ? \"online\" : \"offline\"} {ready ? \"ready\" : \"current\"}</p>;\n}\n";
  let page = "import Status from \"@src/Status\";\n\nexport default function Page() {\n  return <main><Status /></main>;\n}\n";
  let out = html("keys", &[("src/Status.tsx", status), ("routes/page.tsx", page)]);
  assert!(out.as_ref().is_ok_and(|h| h.contains("online current")), "{out:?}");
}
