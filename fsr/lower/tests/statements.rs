//! Array construction and statements a component body or a helper writes,
//! each rendered on the server as the browser would compute it.

use std::path::PathBuf;
use std::sync::Arc;

use snapfire_fsr_core::ValueMap;
use snapfire_fsr_ir::render::{prepare, Components};
use snapfire_fsr_ir::Interpreter;
use snapfire_fsr_lower::component::ComponentSet;

fn app(tag: &str, files: &[(&str, &str)]) -> PathBuf {
  let dir = std::env::temp_dir().join(format!("fsr_statements_{}_{tag}", std::process::id()));
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
fn array_of_a_length_filled_holds_the_value_at_every_index() {
  let page = "import { useState } from \"react\";\nexport default function Page() {\n  const [squares, setSquares] = useState(Array(3).fill(null));\n  const marks = new Array(2).fill(\"x\");\n  return <p onClick={() => setSquares(squares)}>{squares.length}:{squares.map((s, i) => <i key={i}>{s === null ? \"-\" : s}</i>)}:{marks.join(\"\")}</p>;\n}\n";
  assert_eq!(html("fill", page).as_deref(), Ok("<p>3:<i>-</i><i>-</i><i>-</i>:xx</p>"));
}

#[test]
fn array_of_a_length_without_fill_is_not_lowered() {
  let page = "export default function Page() {\n  const holes = Array(3);\n  return <p>{holes.length}</p>;\n}\n";
  assert!(html("holes", page).is_err());
}

const WINNER: &str = "function calculateWinner(squares) {\n  const lines = [[0, 1, 2], [3, 4, 5], [0, 3, 6]];\n  for (let i = 0; i < lines.length; i++) {\n    const [a, b, c] = lines[i];\n    if (squares[a] && squares[a] === squares[b] && squares[a] === squares[c]) {\n      return squares[a];\n    }\n  }\n  return null;\n}\n";

fn board(squares: &str) -> String {
  format!("{WINNER}export default function Page() {{\n  const winner = calculateWinner({squares});\n  return <p>{{winner ? \"Winner: \" + winner : \"Next\"}}</p>;\n}}\n")
}

#[test]
fn a_loop_that_returns_gives_the_first_value_returned() {
  assert_eq!(html("win_row", &board("[\"X\", \"X\", \"X\", \"O\", \"O\", null, null, null, null]")).as_deref(), Ok("<p>Winner: X</p>"));
  assert_eq!(html("win_col", &board("[\"O\", \"X\", \"X\", \"O\", \"X\", null, \"O\", null, null]")).as_deref(), Ok("<p>Winner: O</p>"));
  assert_eq!(html("win_none", &board("[\"O\", \"X\", null, null, null, null, null, null, null]")).as_deref(), Ok("<p>Next</p>"));
}

#[test]
fn what_follows_a_return_in_an_iteration_runs_only_while_nothing_returned() {
  let page = "function firstBig(xs) {\n  let seen = 0;\n  for (const x of xs) {\n    if (x > 10) {\n      return x + \"@\" + seen;\n    }\n    seen = seen + 1;\n  }\n  return \"none@\" + seen;\n}\nexport default function Page() {\n  return <p>{firstBig([1, 2, 30, 40])}|{firstBig([1, 2])}</p>;\n}\n";
  assert_eq!(html("after_return", page).as_deref(), Ok("<p>30@2|none@2</p>"));
}

#[test]
fn for_each_runs_its_callback_over_every_item_and_return_skips_the_rest_of_one() {
  let page = "const products = [{ name: \"a\", stocked: true }, { name: \"b\", stocked: false }, { name: \"c\", stocked: true }];\nexport default function Page() {\n  const rows = [];\n  products.forEach((product, i) => {\n    if (!product.stocked) {\n      return;\n    }\n    rows.push(<li key={product.name}>{i}{product.name}</li>);\n  });\n  return <ul>{rows}</ul>;\n}\n";
  assert_eq!(html("for_each", page).as_deref(), Ok("<ul><li>0a</li><li>2c</li></ul>"));
}

#[test]
fn reverse_sort_pop_and_shift_rewrite_the_local_they_are_called_on() {
  let page = "export default function Page() {\n  const a = [1, 2, 3];\n  a.reverse();\n  const b = [3, 1, 2];\n  b.sort((x, y) => x - y);\n  const c = [1, 2, 3];\n  c.pop();\n  c.shift();\n  return <p>{a.join(\",\")}|{b.join(\",\")}|{c.join(\",\")}</p>;\n}\n";
  assert_eq!(html("in_place", page).as_deref(), Ok("<p>3,2,1|1,2,3|2</p>"));
}

#[test]
fn a_callback_a_custom_hook_only_hands_to_the_browser_lowers_the_hook() {
  let interval = "import { useEffect } from \"react\";\nexport function useInterval(onTick, delay) {\n  useEffect(() => {\n    const id = setInterval(onTick, delay);\n    return () => clearInterval(id);\n  }, [onTick, delay]);\n}\n";
  let page = "import { useState } from \"react\";\nimport { useInterval } from \"@src/useInterval\";\nexport default function Page() {\n  const [count, setCount] = useState(0);\n  useInterval(() => {\n    setCount((c) => c + 1);\n  }, 1000);\n  return <h1>{count}</h1>;\n}\n";
  let dir = app("hook_callback", &[("src/useInterval.ts", interval), ("routes/page.tsx", page)]);
  let mut set = ComponentSet::new(&dir);
  let result = set.lower("routes/page.tsx#default");
  std::fs::remove_dir_all(&dir).unwrap();
  assert!(result.is_ok(), "{result:?}");
  assert!(set.browser_only.is_empty(), "{:?}", set.browser_only);
}
