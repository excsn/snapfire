//! The statements a handler holds: its own `let`s, written in a branch,
//! after one and with `++` or `--`, run as JavaScript runs them.

use std::path::PathBuf;
use std::sync::Arc;

use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_ir::render::{prepare, Components, HandlerRef};
use snapfire_fsr_ir::Interpreter;
use snapfire_fsr_lower::component::ComponentSet;

fn app(tag: &str, page: &str) -> PathBuf {
  let dir = std::env::temp_dir().join(format!("fsr_handler_statements_{}_{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  std::fs::create_dir_all(dir.join("routes")).unwrap();
  std::fs::write(dir.join("routes/page.tsx"), page).unwrap();
  dir
}

fn steps(tag: &str, page: &str, handler: usize, times: usize) -> Result<Vec<String>, String> {
  let dir = app(tag, page);
  let mut set = ComponentSet::new(&dir);
  let result = set.lower("routes/page.tsx#default").map_err(|e| e.to_string());
  std::fs::remove_dir_all(&dir).unwrap();
  result?;
  set.settle_fills();
  let library: Components = set.components.iter().map(|(module, component)| (module.clone(), Arc::new(prepare(component)))).collect();
  let module = "routes/page.tsx#default";
  let interpreter = Interpreter::default().with_consts(Some(Arc::new(set.consts.clone())));
  let mut state = ValueMap::default();
  let mut out = Vec::new();
  for _ in 0..times {
    let step = interpreter.island_step(module, &library[module], &ValueMap::default(), &state, Some(HandlerRef::own(handler)), &Value::Null, &library).map_err(|e| format!("{e:?}"))?;
    state = step.state;
    out.push(step.rendered.html);
  }
  Ok(out)
}

#[test]
fn a_handler_writes_its_own_let_in_a_branch_and_after_it() {
  let page = "import { useState } from 'react';\n\nexport default function Count() {\n  const [count, setCount] = useState(1);\n  const [up, setUp] = useState(true);\n  const step = () => {\n    let next = count;\n    if (up) {\n      next += 2;\n    } else {\n      next--;\n    }\n    next = next * 10;\n    setCount(next);\n    setUp(!up);\n  };\n  return <button onClick={step}>{count}</button>;\n}\n";
  let html = steps("branch", page, 0, 2).unwrap();
  assert!(html[0].contains(">30</button>"), "{}", html[0]);
  assert!(html[1].contains(">290</button>"), "{}", html[1]);
}

#[test]
fn a_handler_writes_its_own_let_after_an_early_return() {
  let page = "import { useState } from 'react';\n\nexport default function Count() {\n  const [count, setCount] = useState(0);\n  const step = () => {\n    let next = count + 1;\n    if (next > 2) {\n      setCount(0);\n      return;\n    }\n    next = next * 10;\n    setCount(next);\n  };\n  return <button onClick={step}>{count}</button>;\n}\n";
  let html = steps("early", page, 0, 3).unwrap();
  assert!(html[0].contains(">10</button>"), "{}", html[0]);
  assert!(html[1].contains(">0</button>"), "{}", html[1]);
  assert!(html[2].contains(">10</button>"), "{}", html[2]);
}

#[test]
fn a_ternary_statement_in_a_handler_runs_one_arm() {
  let page = "import { useState } from 'react';\n\nexport default function Count() {\n  const [count, setCount] = useState(0);\n  const step = () => {\n    count === 0 ? setCount(5) : setCount(0);\n  };\n  return <button onClick={step}>{count}</button>;\n}\n";
  let html = steps("ternary", page, 0, 2).unwrap();
  assert!(html[0].contains(">5</button>"), "{}", html[0]);
  assert!(html[1].contains(">0</button>"), "{}", html[1]);
}
