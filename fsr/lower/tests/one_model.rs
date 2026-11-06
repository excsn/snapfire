use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_ir::render::{prepare, Components};
use snapfire_fsr_ir::Interpreter;
use snapfire_fsr_lower::component::ComponentSet;

static NEXT: AtomicU32 = AtomicU32::new(0);

fn lowered(page: &str) -> (Components, ComponentSet) {
  lowered_with(&[("routes/page.tsx", page)])
}

fn lowered_with(files: &[(&str, &str)]) -> (Components, ComponentSet) {
  let dir = snapfire_fsr_test_internal::scratch_root().join(format!("fsr_one_{}_{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
  for (name, text) in files {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
  }
  let mut set = ComponentSet::new(&dir);
  set.lower("routes/page.tsx#default").unwrap();
  set.settle_proofs();
  set.settle_fills();
  let _ = std::fs::remove_dir_all(&dir);
  let library = set.components.iter().map(|(m, c)| (m.clone(), Arc::new(prepare(c)))).collect();
  (library, set)
}

fn twice(page: &str) -> (String, String) {
  let (library, set) = lowered(page);
  let i = Interpreter::default().with_consts(Some(Arc::new(set.consts.clone())));
  let m = "routes/page.tsx#default";
  let c = library[m].clone();
  let one = i.render_module(m, &c, &ValueMap::default(), &library).map(|r| r.html).unwrap_or_else(|e| format!("FAIL {}", e.message));
  let two = i.render_module(m, &c, &ValueMap::default(), &library).map(|r| r.html).unwrap_or_else(|e| format!("FAIL {}", e.message));
  (one, two)
}

#[test]
fn a_component_writing_a_local_object_starts_fresh_each_render() {
  let (one, two) = twice("export default function P() {\n  const o = { n: 0 };\n  o.n += 1;\n  return <p>{o.n}</p>;\n}\n");
  assert_eq!((one.as_str(), two.as_str()), ("<p>1</p>", "<p>1</p>"));
}

#[test]
fn a_big_module_constant_a_body_writes_does_not_reach_the_next_request() {
  let rows: Vec<String> = (0..60).map(|i| format!("{{ id: {i}, name: \"row {i}\" }}")).collect();
  let page = format!("const C = [{}];\nexport default function P() {{\n  const first = C[0];\n  first.id += 1;\n  return <p>{{C[0].id}}</p>;\n}}\n", rows.join(", "));
  let (one, two) = twice(&page);
  assert_eq!(one, two, "the second render saw the first's write");
}

#[test]
fn a_child_writing_a_module_constant_passed_by_a_parent_does_not_reach_the_next_request() {
  let (one, two) = twice("const C = { n: 0 };\nfunction Child({ c }) {\n  c.n += 1;\n  return <i>{c.n}</i>;\n}\nexport default function P() {\n  return <div><Child c={C} /></div>;\n}\n");
  assert_eq!(one, two, "the second render saw the first's write");
}

#[test]
fn a_child_writing_a_parents_object_reaches_it() {
  let (one, _) = twice("function Child({ o }) {\n  o.n = 2;\n  return <i>{o.n}</i>;\n}\nfunction Show({ o }) {\n  return <b>{o.n}</b>;\n}\nexport default function P() {\n  const o = { n: 1 };\n  return <div><Child o={o} /><Show o={o} /></div>;\n}\n");
  assert_eq!(one, "<div><i>2</i><b>2</b></div>");
}

#[test]
fn a_child_pushing_to_the_array_a_parent_is_mapping_renders_what_was_there_when_it_started() {
  let (one, _) = twice("function Item({ list, n }) {\n  list.push(n + 100);\n  return <i>{n}</i>;\n}\nexport default function P() {\n  const list = [1, 2];\n  return <div>{list.map((n) => <Item key={n} list={list} n={n} />)}</div>;\n}\n");
  assert_eq!(one, "<div><i>1</i><i>2</i></div>");
}

#[test]
fn a_write_to_a_constant_read_by_a_body_reaches_every_name_in_that_body_and_not_the_next_request() {
  let (one, two) = twice("const C = [{ n: 1 }];\nfunction Child({ c }) {\n  const r = c[0];\n  r.n += 1;\n  return <i>{c[0].n}:{r.n}:{String(r === c[0])}</i>;\n}\nexport default function P() {\n  return <div><Child c={C} /></div>;\n}\n");
  assert_eq!((one.as_str(), two.as_str()), ("<div><i>2:2:true</i></div>", "<div><i>2:2:true</i></div>"));
}

#[test]
fn a_store_write_in_a_step_reaches_the_render_that_follows() {
  let store = "import { key } from \"@snapfire/fsr/client/store\";\nexport const count = key<number>(\"probe/count\");\n";
  let page = "import { useStore } from \"@snapfire/fsr/client/react\";\nimport { count } from \"@src/store\";\nexport default function Page() {\n  const [c, setC] = useStore(count, 0);\n  const box = { n: 0 };\n  box.n = c;\n  return <div><p>{box.n}</p><button onClick={() => setC(c + 1)}>add</button></div>;\n}\n";
  let (library, set) = lowered_with(&[("src/store.ts", store), ("routes/page.tsx", page)]);
  let i = Interpreter::default().with_consts(Some(Arc::new(set.consts.clone())));
  let m = "routes/page.tsx#default";
  let stepped = i.island_step(m, &library[m], &ValueMap::default(), &ValueMap::default(), Some(snapfire_fsr_ir::render::HandlerRef::own(0)), &Value::Null, &library).unwrap_or_else(|e| panic!("{}", e.message));
  assert_eq!(stepped.store.get("probe/count"), Some(&Value::F64(1.0)), "{:?}", stepped.store);
  assert!(stepped.rendered.html.contains("<p>1</p>"), "{}", stepped.rendered.html);
}

#[test]
fn a_local_array_a_component_pushes_into_is_a_new_array_on_each_call() {
  let (one, _) = twice("function Section() {\n  const items = [];\n  for (let i = 0; i < 2; i++) {\n    items.push(<li key={i}>{i}</li>);\n  }\n  return <ul>{items}</ul>;\n}\nexport default function P() {\n  return <div><Section /><Section /><Section /></div>;\n}\n");
  assert_eq!(one, "<div><ul><li>0</li><li>1</li></ul><ul><li>0</li><li>1</li></ul><ul><li>0</li><li>1</li></ul></div>");
}

#[test]
fn a_local_object_a_component_writes_is_a_new_object_on_each_call() {
  let (one, _) = twice("function Tally() {\n  const o = { n: 0 };\n  o.n += 1;\n  return <i>{o.n}</i>;\n}\nexport default function P() {\n  return <div><Tally /><Tally /></div>;\n}\n");
  assert_eq!(one, "<div><i>1</i><i>1</i></div>");
}

#[test]
fn two_evaluations_of_one_object_literal_are_two_objects() {
  let (one, _) = twice("const make = () => ({ n: 1 });\nexport default function P() {\n  return <p>{String(make() === make())}</p>;\n}\n");
  assert_eq!(one, "<p>false</p>");
}

#[test]
fn every_read_of_a_module_constant_is_the_same_object() {
  let (one, _) = twice("const C = [{ id: 1 }, { id: 2 }];\nexport default function P() {\n  const s = C[0];\n  return <p>{String(s === C[0])}{C.map((c) => (c === s ? \"y\" : \"n\")).join(\"\")}</p>;\n}\n");
  assert_eq!(one, "<p>trueyn</p>");
}

#[test]
fn a_module_constant_reached_through_a_helper_is_the_same_object() {
  let (one, _) = twice("const C = [{ id: 1 }, { id: 2 }];\nconst pick = (items, i) => items[i];\nexport default function P() {\n  return <p>{String(pick(C, 0) === C[0])}</p>;\n}\n");
  assert_eq!(one, "<p>true</p>");
}

#[test]
fn a_multiple_select_marks_every_option_its_value_lists() {
  let (one, _) = twice("export default function P() {\n  return <select multiple defaultValue={[\"b\", \"c\"]}><option value=\"a\">A</option><option value=\"b\">B</option><option value=\"c\">C</option></select>;\n}\n");
  assert_eq!(one.matches("selected").count(), 2, "{one}");
  assert!(one.contains("<option value=\"b\" selected=\"\">") && one.contains("<option value=\"c\" selected=\"\">"), "{one}");
}

#[test]
fn a_single_select_marks_the_option_its_value_names() {
  let (one, _) = twice("export default function P() {\n  return <select defaultValue=\"b\"><option value=\"a\">A</option><option value=\"b\">B</option></select>;\n}\n");
  assert_eq!(one.matches("selected").count(), 1, "{one}");
  assert!(one.contains("<option value=\"b\" selected=\"\">"), "{one}");
}

#[test]
fn an_object_placed_as_a_child_fails_as_react_does() {
  let (one, _) = twice("const person = { name: \"G\", theme: \"dark\" };\nexport default function P() {\n  return <h1>{person}</h1>;\n}\n");
  assert!(one.starts_with("FAIL") && one.contains("not valid as a React child") && one.contains("{name, theme}"), "{one}");
}
