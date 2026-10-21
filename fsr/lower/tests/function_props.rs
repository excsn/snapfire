//! A function passed as a prop to a component that renders in the browser
//! reaches it as a module reference: an arrow is exported from its file by
//! the rewrite, a declared function by its name and the island mounts with
//! the reference where the function was.

use std::path::PathBuf;
use std::sync::Arc;

use snapfire_fsr_core::{RefKind, Value, ValueMap};
use snapfire_fsr_ir::render::{prepare, Components, Rendered};
use snapfire_fsr_ir::Interpreter;
use snapfire_fsr_lower::component::ComponentSet;

fn app(tag: &str, files: &[(&str, &str)]) -> PathBuf {
  let dir = std::env::temp_dir().join(format!("fsr_function_props_{}_{tag}", std::process::id()));
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
  rendered: Rendered,
}

fn lower(tag: &str, files: &[(&str, &str)]) -> Result<Lowered, String> {
  let dir = app(tag, files);
  let mut set = ComponentSet::new(&dir);
  let result = set.lower("routes/page.tsx#default").map_err(|e| e.to_string());
  std::fs::remove_dir_all(&dir).unwrap();
  result?;
  set.settle_fills();
  let library: Components = set.components.iter().map(|(module, component)| (module.clone(), Arc::new(prepare(component)))).collect();
  let module = "routes/page.tsx#default";
  let rendered = Interpreter::default().with_consts(Some(Arc::new(set.consts.clone()))).render_module(module, &library[module], &ValueMap::default(), &library).map_err(|e| format!("{e:?}"))?;
  Ok(Lowered { set, rendered })
}

const TABS: &str = "import { useState } from 'react';\n\nexport default function Tabs({ ids, getHeader, renderContent }) {\n  const [selected, setSelected] = useState(ids[0]);\n  return (\n    <>\n      {ids.map((id) => (<button key={id} onClick={() => setSelected(id)}>{getHeader(id)}</button>))}\n      <div>{renderContent(selected)}</div>\n    </>\n  );\n}\n";

fn module_ref(value: &Value) -> Option<&str> {
  match value {
    Value::Ref { kind: RefKind::Module, id } => Some(id.as_str()),
    _ => None,
  }
}

#[test]
fn an_arrow_closing_over_nothing_is_exported_and_mounted_as_a_reference() {
  let page = "import Tabs from '@src/Tabs';\n\nexport default function Page() {\n  return <Tabs ids={['a', 'b']} getHeader={(id) => id.toUpperCase()} renderContent={(id) => <p>This is {id}.</p>} />;\n}\n";
  let out = lower("arrow", &[("src/Tabs.js", TABS), ("routes/page.tsx", page)]).unwrap();
  let island = &out.rendered.islands[0];
  assert!(island.body.html.contains("<button>A</button>") && island.body.html.contains("<p>This is a.</p>"), "{}", island.body.html);
  let header = module_ref(&island.props["getHeader"]).expect("a module reference");
  let (file, name) = header.split_once('#').unwrap();
  assert_eq!(file, "routes/page.tsx");
  assert!(name.starts_with("__sfF"), "{header}");
  assert!(module_ref(&island.props["renderContent"]).is_some(), "{:?}", island.props);
  let (_, rewritten) = out.set.rewritten().into_iter().find(|(f, _)| f == "routes/page.tsx").expect("the page is rewritten");
  assert!(rewritten.contains(&format!("export const {name} = (id) => id.toUpperCase();")), "{rewritten}");
  assert_eq!(out.set.function_modules(), vec!["routes/page.tsx".to_owned()]);
}

#[test]
fn an_arrow_reading_a_local_of_the_page_is_refused() {
  let page = "import Tabs from '@src/Tabs';\n\nexport default function Page() {\n  const mark = '!';\n  return <Tabs ids={['a']} getHeader={(id) => id + mark} renderContent={(id) => <p>{id}</p>} />;\n}\n";
  let err = match lower("closure", &[("src/Tabs.js", TABS), ("routes/page.tsx", page)]) {
    Ok(_) => panic!("lowered"),
    Err(err) => err,
  };
  assert!(err.contains("`getHeader`") && err.contains("closes over `mark`"), "{err}");
}

#[test]
fn a_function_the_file_declares_is_named_and_exported() {
  let page = "import Tabs from '@src/Tabs';\n\nfunction header(id) {\n  return id + '!';\n}\n\nexport default function Page() {\n  return <Tabs ids={['a']} getHeader={header} renderContent={(id) => <p>{id}</p>} />;\n}\n";
  let out = lower("declared", &[("src/Tabs.js", TABS), ("routes/page.tsx", page)]).unwrap();
  let island = &out.rendered.islands[0];
  assert!(island.body.html.contains("<button>a!</button>"), "{}", island.body.html);
  assert_eq!(module_ref(&island.props["getHeader"]), Some("routes/page.tsx#header"));
  let (_, rewritten) = out.set.rewritten().into_iter().find(|(f, _)| f == "routes/page.tsx").expect("the page is rewritten");
  assert!(rewritten.contains("export { header };"), "{rewritten}");
}

#[test]
fn a_choice_between_imported_functions_mounts_the_one_taken() {
  let fns = "export function upper(id) {\n  return id.toUpperCase();\n}\n\nexport function lower(id) {\n  return id.toLowerCase();\n}\n";
  let page = "import Tabs from '@src/Tabs';\nimport { upper, lower } from '@src/fns';\n\nconst loud = ['a'].length > 0;\n\nexport default function Page() {\n  return <Tabs ids={['a']} getHeader={loud ? upper : lower} renderContent={(id) => <p>{id}</p>} />;\n}\n";
  let out = lower("choice", &[("src/Tabs.js", TABS), ("src/fns.js", fns), ("routes/page.tsx", page)]).unwrap();
  let island = &out.rendered.islands[0];
  assert!(island.body.html.contains("<button>A</button>"), "{}", island.body.html);
  assert_eq!(module_ref(&island.props["getHeader"]), Some("src/fns.js#upper"));
  assert!(out.set.function_modules().contains(&"src/fns.js".to_owned()), "{:?}", out.set.function_modules());
}

#[test]
fn a_method_in_an_object_literal_is_a_function_value() {
  let page = "export default function Page() {\n  const ops = { inc(n) { return n + 1; }, twice: function (n) { return n * 2; } };\n  return <p>{ops.inc(1)} {ops.twice(3)}</p>;\n}\n";
  let out = lower("method", &[("routes/page.tsx", page)]).unwrap();
  assert_eq!(out.rendered.html, "<p>2 6</p>");
}

#[test]
fn an_object_of_functions_in_a_component_body_is_a_table_of_handlers() {
  let page = "import { useState } from 'react';\n\nexport default function Page() {\n  const [n, setN] = useState(0);\n  const ops = { async bump(k) { await Promise.resolve(); setN(n + k); }, reset: () => setN(0) };\n  return <p><button onClick={() => ops.bump(1)}>{n}</button><button onClick={ops.reset}>r</button></p>;\n}\n";
  let out = lower("handler_table", &[("routes/page.tsx", page)]).unwrap();
  assert_eq!(out.rendered.html, "<p><button>0</button><button>r</button></p>");
  let (_, component) = out.set.components.iter().find(|(m, _)| m == "routes/page.tsx#default").unwrap();
  let table = component.body.iter().find_map(|stmt| match stmt {
    snapfire_fsr_ir::Stmt::Let { name, expr: snapfire_fsr_ir::Expr::Object(entries) } if name == "ops" => Some(entries.clone()),
    _ => None,
  });
  let table = table.expect("ops is bound to an object");
  assert_eq!(table.len(), 2);
  assert!(table.iter().all(|entry| matches!(entry, snapfire_fsr_ir::ast::Entry::Field(_, snapfire_fsr_ir::Expr::Handler(_)))), "{table:?}");
}

#[test]
fn a_childs_handler_calling_a_function_prop_its_parent_passed_runs_the_parents_handler_in_the_step() {
  let page = "import { useState } from 'react';\n\nfunction Picker({ onPick }) {\n  return <button onClick={() => onPick(3)}>pick</button>;\n}\n\nexport default function Page() {\n  const [picked, setPicked] = useState(0);\n  return <div><Picker onPick={(n) => setPicked(picked + n)} /><p>{picked}</p></div>;\n}\n";
  let out = lower("call_prop", &[("routes/page.tsx", page)]).unwrap();
  let library: Components = out.set.components.iter().map(|(module, component)| (module.clone(), Arc::new(prepare(component)))).collect();
  let module = "routes/page.tsx#default";
  let interpreter = Interpreter::default();
  let first = interpreter.island_step(module, &library[module], &ValueMap::default(), &ValueMap::default(), None, &Value::Null, &library).unwrap();
  let (picker, _) = first.components.iter().find(|(_, m)| m == "routes/page.tsx#Picker").expect("the picker renders");
  let picked = interpreter.island_step(module, &library[module], &ValueMap::default(), &first.state, Some(snapfire_fsr_ir::render::HandlerRef::at(picker.clone(), 0)), &Value::Null, &library).unwrap();
  assert!(picked.rendered.html.contains("<p>3</p>"), "{}", picked.rendered.html);
  let again = interpreter.island_step(module, &library[module], &ValueMap::default(), &picked.state, Some(snapfire_fsr_ir::render::HandlerRef::at(picker.clone(), 0)), &Value::Null, &library).unwrap();
  assert!(again.rendered.html.contains("<p>6</p>"), "{}", again.rendered.html);
}
