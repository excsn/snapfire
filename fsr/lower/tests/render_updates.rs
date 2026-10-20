//! A React component that sets its own state while it renders, rendered on the server as React's renderer does: the body runs again with the new state until no setter fires. Only the last pass's tree is written.

use std::path::PathBuf;
use std::sync::Arc;

use snapfire_fsr_core::ValueMap;
use snapfire_fsr_ir::render::{prepare, Components};
use snapfire_fsr_ir::Interpreter;
use snapfire_fsr_lower::component::ComponentSet;

fn app(tag: &str, files: &[(&str, &str)]) -> PathBuf {
  let dir = std::env::temp_dir().join(format!("fsr_render_updates_{}_{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  for (name, text) in files {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
  }
  dir
}

fn html(tag: &str, page: &str) -> Result<String, String> {
  let dir = app(tag, &[("routes/page.tsx", page)]);
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
fn a_setter_called_while_rendering_renders_again_and_the_pass_that_called_it_reads_the_old_state() {
  let page = "import { useState } from \"react\";\nexport default function Page() {\n  const count = 5;\n  const [prevCount, setPrevCount] = useState(0);\n  const [trend, setTrend] = useState(null);\n  if (prevCount !== count) {\n    setPrevCount(count);\n    setTrend(count > prevCount ? \"increasing\" : \"decreasing\");\n  }\n  return (\n    <>\n      <h1>{count}</h1>\n      {trend && <p>The count is {trend}</p>}\n    </>\n  );\n}\n";
  assert_eq!(html("trend", page).as_deref(), Ok("<h1>5</h1><p>The count is increasing</p>"));
}

#[test]
fn updaters_queued_in_one_pass_apply_in_order_to_the_state_the_next_pass_reads() {
  let page = "import { useState } from \"react\";\nexport default function Page() {\n  const [n, setN] = useState(0);\n  if (n < 2) {\n    setN((x) => x + 1);\n    setN((x) => x + 1);\n  }\n  return <p>{n}</p>;\n}\n";
  assert_eq!(html("updaters", page).as_deref(), Ok("<p>2</p>"));
}

#[test]
fn the_body_runs_again_until_no_setter_fires() {
  let page = "import { useState } from \"react\";\nexport default function Page() {\n  const [n, setN] = useState(0);\n  const [log, setLog] = useState(\"\");\n  if (n < 5) {\n    setN(n + 1);\n    setLog(log + n);\n  }\n  return <p>{n}:{log}</p>;\n}\n";
  assert_eq!(html("passes", page).as_deref(), Ok("<p>5:01234</p>"));
}

#[test]
fn a_setter_that_fires_on_every_pass_fails_the_render_past_reacts_limit() {
  let page = "import { useState } from \"react\";\nexport default function Page() {\n  const [n, setN] = useState(0);\n  setN(n + 1);\n  return <p>{n}</p>;\n}\n";
  let err = html("forever", page).unwrap_err();
  assert!(err.contains("Too many re-renders"), "{err}");
}
