//! A module-level `let` a function writes is state: a helper writing it keeps
//! its statements as a block lambda, a component binds it as a store key
//! seeded by replaying the module's initialisation. A render calling the
//! helper advances the request's copy; a handler's call reaches the store.

use std::path::PathBuf;
use std::sync::Arc;

use snapfire_fsr_core::ValueMap;
use snapfire_fsr_ir::render::{prepare, Components};
use snapfire_fsr_ir::{Expr, Interpreter, Stmt};
use snapfire_fsr_lower::component::ComponentSet;

fn app(tag: &str, files: &[(&str, &str)]) -> PathBuf {
  let dir = std::env::temp_dir().join(format!("fsr_module_state_{}_{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  for (name, text) in files {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
  }
  dir
}

struct Lowered {
  set: ComponentSet,
  html: String,
}

fn lower(tag: &str, files: &[(&str, &str)]) -> Result<Lowered, String> {
  let dir = app(tag, files);
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
  let html = Interpreter::default().with_consts(Some(Arc::new(set.consts.clone()))).render_module(module, &library[module], &ValueMap::default(), &library).map(|r| r.html).map_err(|e| format!("{e:?}"))?;
  Ok(Lowered { set, html })
}

const TODOS: &str = "let nextId = 0;\n\nexport function createTodo(text, completed = false) {\n  return {\n    id: nextId++,\n    text,\n    completed\n  };\n}\n\nexport const initialTodos = [\n  createTodo('Get apples', true),\n  createTodo('Get oranges', true),\n  createTodo('Get carrots'),\n];\n";

#[test]
fn a_constant_built_by_calls_that_advance_a_module_let_is_what_the_module_leaves_it_at() {
  let page = "import { initialTodos } from \"@src/todos\";\nexport default function Page() {\n  return <ul>{initialTodos.map(t => <li key={t.id}>{t.id}:{t.text}{t.completed ? \"!\" : \"\"}</li>)}</ul>;\n}\n";
  let out = lower("replay", &[("src/todos.js", TODOS), ("routes/page.tsx", page)]).unwrap();
  assert_eq!(out.html, "<ul><li>0:Get apples!</li><li>1:Get oranges!</li><li>2:Get carrots</li></ul>");
}

#[test]
fn a_component_reads_a_module_let_as_a_store_key_seeded_by_the_module() {
  let page = "import { nextId, initialTodos } from \"@src/todos\";\nexport default function Page() {\n  return <p>{nextId} after {initialTodos.length}</p>;\n}\n";
  let todos = TODOS.replace("let nextId = 0;", "export let nextId = 0;");
  let out = lower("seed", &[("src/todos.js", &todos), ("routes/page.tsx", page)]).unwrap();
  assert_eq!(out.html, "<p>3 after 3</p>");
  let (_, page) = out.set.components.iter().find(|(m, _)| m == "routes/page.tsx#default").unwrap();
  assert_eq!(page.stores, vec![("nextId".to_owned(), "src/todos.js#nextId".to_owned())]);
  assert!(page.state.contains(&"nextId".to_owned()), "{:?}", page.state);
  assert!(matches!(&page.body[0], Stmt::Let { name, expr: Expr::Coalesce(store, _) } if name == "nextId" && matches!(&**store, Expr::Store(key) if key == "src/todos.js#nextId")), "{:?}", page.body[0]);
}

#[test]
fn a_render_that_calls_a_helper_writing_module_state_advances_the_request_copy() {
  let page = "import { createTodo } from \"@src/todos\";\nexport default function Page() {\n  return <p>{createTodo(\"a\").id}{createTodo(\"b\").id}</p>;\n}\n";
  let out = lower("render_write", &[("src/todos.js", TODOS), ("routes/page.tsx", page)]).unwrap();
  assert_eq!(out.html, "<p>34</p>");
  let (_, page) = out.set.components.iter().find(|(m, _)| m == "routes/page.tsx#default").unwrap();
  assert_eq!(page.stores, vec![("nextId".to_owned(), "src/todos.js#nextId".to_owned())]);
}

#[test]
fn a_loader_reading_a_module_let_a_function_writes_is_refused() {
  let dir = app("loader", &[("src/todos.js", &TODOS.replace("let nextId = 0;", "export let nextId = 0;")), ("routes/page.loader.ts", "import { nextId } from \"@src/todos\";\nexport async function load() {\n  return { n: nextId };\n}\n")]);
  let err = ComponentSet::new(&dir).lower_loader("routes/page.loader.ts").unwrap_err().to_string();
  std::fs::remove_dir_all(&dir).unwrap();
  assert!(err.contains("a module-level `let` a function writes"), "{err}");
}

#[test]
fn a_helper_writing_only_its_own_locals_is_still_an_expression_lambda() {
  let page = "function total(xs) {\n  let n = 0;\n  for (const x of xs) {\n    n += x;\n  }\n  return n;\n}\nfunction next(c) {\n  let n = c;\n  n++;\n  return n * 10;\n}\nexport default function Page() {\n  return <p>{total([1, 2, 3])} {next(4)}</p>;\n}\n";
  let out = lower("locals", &[("routes/page.tsx", page)]).unwrap();
  assert_eq!(out.html, "<p>6 50</p>");
}

#[test]
fn a_handler_calling_the_helper_writes_the_store_key() {
  let page = "import { useState } from \"react\";\nimport { createTodo, initialTodos } from \"@src/todos\";\nexport default function Page() {\n  const [todos, setTodos] = useState(initialTodos);\n  return <div><button onClick={() => setTodos([...todos, createTodo(\"new\")])}>add</button><ul>{todos.map(t => <li key={t.id}>{t.text}</li>)}</ul></div>;\n}\n";
  let out = lower("handler", &[("src/todos.js", TODOS), ("routes/page.tsx", page)]).unwrap();
  assert!(out.html.contains("<li>Get apples</li><li>Get oranges</li><li>Get carrots</li>"), "{}", out.html);
  let (_, page) = out.set.components.iter().find(|(m, _)| m == "routes/page.tsx#default").unwrap();
  assert_eq!(page.stores, vec![("nextId".to_owned(), "src/todos.js#nextId".to_owned())], "the handler's write binds the key");
  let handler = &page.handlers[0];
  let mut sets = Vec::new();
  for stmt in &handler.body {
    stmt_sets(stmt, &mut sets);
  }
  assert!(sets.contains(&"nextId".to_owned()), "the inlined helper's write survives in the handler: {:?}", handler.body);
}

fn stmt_sets(stmt: &Stmt, out: &mut Vec<String>) {
  match stmt {
    Stmt::Set { name, .. } => out.push(name.clone()),
    Stmt::Let { expr, .. } | Stmt::Return(expr) | Stmt::Expr(expr) => expr.visit(&mut |e| {
      if let Expr::Block(body) = e {
        for s in body {
          stmt_sets(s, out);
        }
      }
    }),
    Stmt::If { then, r#else, .. } => {
      then.iter().for_each(|s| stmt_sets(s, out));
      r#else.iter().for_each(|s| stmt_sets(s, out));
    }
    _ => {}
  }
}

#[test]
fn a_module_let_declared_without_a_value_starts_undefined_and_a_function_may_write_it() {
  let page = "let seen;\n\nfunction see() {\n  if (!seen) {\n    seen = \"first\";\n  }\n  return seen;\n}\n\nexport default function Page() {\n  return <p>{see()}</p>;\n}\n";
  let out = lower("undeclared_value", &[("routes/page.tsx", page)]).unwrap();
  assert_eq!(out.html, "<p>first</p>");
}

#[test]
fn a_handler_calling_a_helper_that_sets_a_store_key_writes_the_key() {
  let counter = "import { key, set } from \"@snapfire/fsr-client/store\";\n\nexport const count = key<number>(\"demo/count\");\n\nexport function bump(n: number) {\n  set(count, n + 1);\n}\n\nexport const reset = () => set(count, 0);\n";
  let page = "import { useStore } from \"@snapfire/fsr-client/react\";\nimport { bump, count, reset } from \"@src/counter\";\nexport default function Page() {\n  const [n] = useStore(count, 0);\n  return <div><button onClick={() => bump(n)}>{n}</button><button onClick={() => { reset(); }}>reset</button></div>;\n}\n";
  let out = lower("helper_set", &[("src/counter.ts", counter), ("routes/page.tsx", page)]).unwrap();
  let library: Components = out.set.components.iter().map(|(module, component)| (module.clone(), Arc::new(prepare(component)))).collect();
  let module = "routes/page.tsx#default";
  let mut store = ValueMap::default();
  store.insert("demo/count".to_owned(), snapfire_fsr_core::Value::Int(4));
  let mut props = ValueMap::default();
  props.insert("$store".to_owned(), snapfire_fsr_core::Value::Map(store));
  let step = |handler: usize| Interpreter::default().island_step(module, &library[module], &props, &ValueMap::default(), Some(snapfire_fsr_ir::render::HandlerRef::own(handler)), &snapfire_fsr_core::Value::Null, &library).unwrap();
  let bumped = step(0);
  assert_eq!(bumped.store.get("demo/count"), Some(&snapfire_fsr_core::Value::F64(5.0)), "{:?}", bumped.store);
  assert!(bumped.rendered.html.contains(">5</button>"), "{}", bumped.rendered.html);
  assert_eq!(step(1).store.get("demo/count"), Some(&snapfire_fsr_core::Value::F64(0.0)));
}

#[test]
fn a_write_through_a_path_of_indexes_and_fields_on_a_local_rebinds_it_with_that_path_changed() {
  let page = "function opened(cells, x, y) {\n  cells[y][x].opened = true;\n  cells[y][x].count += 2;\n  cells[y][0] = { opened: false, count: 9 };\n  return cells;\n}\nexport default function Page() {\n  const next = opened([[{ opened: false, count: 0 }, { opened: false, count: 1 }]], 1, 0);\n  return <p>{next[0].map((c) => (c.opened ? \"#\" : \".\") + c.count).join(\" \")}</p>;\n}\n";
  let out = lower("path_write", &[("routes/page.tsx", page)]).unwrap();
  assert_eq!(out.html, "<p>.9 #3</p>");
}

#[test]
fn a_module_collection_a_function_changes_in_place_is_module_state() {
  let cache = "const seen = new Map();\n\nexport function remember(key, value) {\n  seen.set(key, value);\n  return seen.size;\n}\n\nexport function recall(key) {\n  return seen.get(key) ?? \"nothing\";\n}\n";
  let page = "import { useState } from \"react\";\nimport { recall, remember } from \"@src/cache\";\nexport default function Page() {\n  const [n, setN] = useState(0);\n  return <div><button onClick={() => setN(remember(\"a\", \"apple\"))}>{n}</button><p>{recall(\"a\")}</p></div>;\n}\n";
  let out = lower("mutated", &[("src/cache.js", cache), ("routes/page.tsx", page)]).unwrap();
  assert!(out.html.contains("<p>nothing</p>"), "{}", out.html);
  let (_, page) = out.set.components.iter().find(|(m, _)| m == "routes/page.tsx#default").unwrap();
  assert_eq!(page.stores, vec![("seen".to_owned(), "src/cache.js#seen".to_owned())], "the collection the handler changes is a store key");
  let library: Components = out.set.components.iter().map(|(module, component)| (module.clone(), Arc::new(prepare(component)))).collect();
  let module = "routes/page.tsx#default";
  let interpreter = Interpreter::default().with_consts(Some(Arc::new(out.set.consts.clone())));
  let remembered = interpreter.island_step(module, &library[module], &ValueMap::default(), &ValueMap::default(), Some(snapfire_fsr_ir::render::HandlerRef::own(0)), &snapfire_fsr_core::Value::Null, &library).unwrap();
  assert!(remembered.rendered.html.contains(">1</button><p>apple</p>"), "{}", remembered.rendered.html);
}

#[test]
fn a_helper_a_handler_calls_advances_the_module_let_in_the_store() {
  let page = "import { useState } from \"react\";\nimport { createTodo, initialTodos } from \"@src/todos\";\nexport default function Page() {\n  const [todos, setTodos] = useState(initialTodos);\n  return <div><button onClick={() => setTodos([...todos, createTodo(\"new\")])}>add</button><ul>{todos.map(t => <li key={t.id}>{t.id}:{t.text}</li>)}</ul></div>;\n}\n";
  let out = lower("advances", &[("src/todos.js", TODOS), ("routes/page.tsx", page)]).unwrap();
  let library: Components = out.set.components.iter().map(|(module, component)| (module.clone(), Arc::new(prepare(component)))).collect();
  let module = "routes/page.tsx#default";
  let interpreter = Interpreter::default().with_consts(Some(Arc::new(out.set.consts.clone())));
  let first = interpreter.island_step(module, &library[module], &ValueMap::default(), &ValueMap::default(), Some(snapfire_fsr_ir::render::HandlerRef::own(0)), &snapfire_fsr_core::Value::Null, &library).unwrap();
  assert_eq!(first.store.get("src/todos.js#nextId"), Some(&snapfire_fsr_core::Value::F64(4.0)), "{:?}", first.store);
  let mut props = ValueMap::default();
  props.insert("$store".to_owned(), snapfire_fsr_core::Value::Map(first.store.clone()));
  let second = interpreter.island_step(module, &library[module], &props, &first.state, Some(snapfire_fsr_ir::render::HandlerRef::own(0)), &snapfire_fsr_core::Value::Null, &library).unwrap();
  assert!(second.rendered.html.contains("<li data-sf-key=\"3\">3:new</li><li data-sf-key=\"4\">4:new</li>"), "{}", second.rendered.html);
}
