//! A setter, a dispatcher or an inner function read as a value on the
//! server, and a form whose action is a function.

use std::path::PathBuf;
use std::sync::Arc;

use snapfire_fsr_core::ValueMap;
use snapfire_fsr_ir::render::{prepare, Components, UNLOWERED_ATTR};
use snapfire_fsr_ir::Interpreter;
use snapfire_fsr_lower::component::ComponentSet;

fn app(tag: &str, files: &[(&str, &str)]) -> PathBuf {
  let dir = std::env::temp_dir().join(format!("fsr_browser_fns_{}_{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  for (name, text) in files {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
  }
  dir
}

fn lowered(tag: &str, files: &[(&str, &str)]) -> Result<ComponentSet, String> {
  let dir = app(tag, files);
  let mut set = ComponentSet::new(&dir);
  let result = set.lower("routes/page.tsx#default").map_err(|e| e.to_string());
  std::fs::remove_dir_all(&dir).unwrap();
  result?;
  set.settle_fills();
  Ok(set)
}

fn html(set: &ComponentSet) -> Result<String, String> {
  let library: Components = set.components.iter().map(|(module, component)| (module.clone(), Arc::new(prepare(component)))).collect();
  let module = "routes/page.tsx#default";
  Interpreter::default().with_consts(Some(Arc::new(set.consts.clone()))).render_module(module, &library[module], &ValueMap::default(), &library).map(|r| r.html).map_err(|e| format!("{e:?}"))
}

fn unlowered(set: &ComponentSet) -> Vec<String> {
  let page = set.components.iter().find(|(m, _)| m == "routes/page.tsx#default").map(|(_, c)| format!("{:?}", c.render)).unwrap_or_default();
  page.match_indices(UNLOWERED_ATTR).map(|(i, _)| page[i..].chars().take(120).collect()).collect()
}

const LIST: &str = "export default function List({ items, dispatch }) {\n  return <ul>{items.map((t) => <li key={t}><button onClick={() => dispatch({ type: \"done\", t })}>{t}</button></li>)}</ul>;\n}\n";

#[test]
fn a_dispatcher_passed_as_a_prop_renders_the_child_and_stays_the_browsers() {
  let page = "import { useReducer } from \"react\";\nimport List from \"@src/List\";\nfunction reducer(s, a) {\n  return s;\n}\nexport default function Page() {\n  const [items, dispatch] = useReducer(reducer, [\"a\", \"b\"]);\n  return <List items={items} dispatch={dispatch} />;\n}\n";
  let set = lowered("dispatch_prop", &[("src/List.tsx", LIST), ("routes/page.tsx", page)]).unwrap();
  assert_eq!(html(&set).as_deref(), Ok("<ul><li><button>a</button></li><li><button>b</button></li></ul>"));
  let marked = unlowered(&set);
  assert!(marked.iter().any(|m| m.contains("`dispatch`, a function only the browser calls")), "a server island cannot be handed the dispatcher: {marked:?}");
}

#[test]
fn a_dispatcher_and_a_setter_held_in_a_context_render_nothing_where_read() {
  let page = "import { createContext, useReducer, useState } from \"react\";\nexport const Dispatch = createContext(null);\nexport const User = createContext(null);\nfunction reducer(s, a) {\n  return s;\n}\nexport default function Page() {\n  const [n, dispatch] = useReducer(reducer, 1);\n  const [user, setUser] = useState(\"ada\");\n  return <Dispatch value={dispatch}><User value={{ user, setUser }}><p>{n} {user}</p></User></Dispatch>;\n}\n";
  let set = lowered("dispatch_ctx", &[("routes/page.tsx", page)]).unwrap();
  assert_eq!(html(&set).as_deref(), Ok("<p>1 ada</p>"));
}

#[test]
fn a_form_action_that_is_a_function_renders_reacts_placeholder_and_hydrates() {
  let page = "export default function Page() {\n  function search(formData) {\n    alert(formData.get(\"q\"));\n  }\n  return <form action={search}><input name=\"q\" /><button formAction={(d) => alert(d)}>Go</button></form>;\n}\n";
  let set = lowered("form_action", &[("routes/page.tsx", page)]).unwrap();
  assert_eq!(
    html(&set).as_deref(),
    Ok("<form action=\"javascript:throw new Error('React form unexpectedly submitted.')\"><input name=\"q\"><button formaction=\"javascript:throw new Error('React form unexpectedly submitted.')\">Go</button></form>")
  );
  let page_component = &set.components.iter().find(|(m, _)| m == "routes/page.tsx#default").unwrap().1;
  assert!(page_component.owner.hydrates(), "the browser runs the action, so the page hydrates");
}

#[test]
fn a_form_action_that_is_a_string_renders_as_written() {
  let page = "export default function Page() {\n  return <form action=\"/search\"><button>Go</button></form>;\n}\n";
  let set = lowered("form_url", &[("routes/page.tsx", page)]).unwrap();
  assert_eq!(html(&set).as_deref(), Ok("<form action=\"/search\"><button>Go</button></form>"));
}

const SLOW: &str = "export async function deliver(message) {\n  await new Promise((res) => setTimeout(res, 1000));\n  return message;\n}\n";

#[test]
fn a_handler_reaching_an_import_that_does_not_lower_is_the_browsers() {
  let page = "import { useState } from \"react\";\nimport { deliver } from \"@src/actions\";\nexport default function Page() {\n  const [sent, setSent] = useState(\"none\");\n  async function send() {\n    setSent(await deliver(\"hi\"));\n  }\n  return <button onClick={send}>{sent}</button>;\n}\n";
  let set = lowered("unfollowed_handler", &[("src/actions.ts", SLOW), ("routes/page.tsx", page)]).unwrap();
  assert_eq!(html(&set).as_deref(), Ok("<button>none</button>"));
  assert!(!unlowered(&set).is_empty(), "the browser runs the handler");
}

#[test]
fn a_render_reading_an_import_that_does_not_lower_still_renders_in_the_browser() {
  let page = "import { deliver } from \"@src/actions\";\nexport default function Page() {\n  return <p>{deliver(\"hi\")}</p>;\n}\n";
  assert!(lowered("unfollowed_render", &[("src/actions.ts", SLOW), ("routes/page.tsx", page)]).is_err());
}

#[test]
fn a_function_on_a_custom_elements_on_prop_is_a_listener_the_server_writes_nothing_for() {
  let page = "export default function Page() {\n  return <my-element onspeak={(e) => console.log(e.detail)} label=\"hi\"></my-element>;\n}\n";
  let set = lowered("custom_event", &[("routes/page.tsx", page)]).unwrap();
  assert_eq!(html(&set).as_deref(), Ok("<my-element label=\"hi\"></my-element>"));
  let page_component = &set.components.iter().find(|(m, _)| m == "routes/page.tsx#default").unwrap().1;
  assert!(page_component.owner.hydrates(), "the browser attaches the listener");
}
