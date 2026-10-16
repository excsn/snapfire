//! Markup written where a value goes, each case against the same markup
//! written inline.

use std::path::PathBuf;
use std::sync::Arc;

use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_ir::render::{prepare, Components, Rendered, SLOT_READ_PREFIX};
use snapfire_fsr_ir::Interpreter;
use snapfire_fsr_lower::component::ComponentSet;

fn app(tag: &str, files: &[(&str, &str)]) -> PathBuf {
  let dir = std::env::temp_dir().join(format!("fsr_markup_{}_{tag}", std::process::id()));
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

fn rendered(tag: &str, files: &[(&str, &str)]) -> Result<Rendered, String> {
  let set = lowered(tag, files)?;
  let library: Components = set.components.iter().map(|(module, component)| (module.clone(), Arc::new(prepare(component)))).collect();
  let module = "routes/page.tsx#default";
  let mut props = ValueMap::default();
  props.insert("price".to_owned(), Value::F64(5.0));
  Interpreter::default().render_module(module, &library[module], &props, &library).map_err(|e| format!("{e:?}"))
}

fn html(tag: &str, files: &[(&str, &str)]) -> Result<String, String> {
  rendered(tag, files).map(|r| r.html)
}

const CARD: &str = "export function Card({ title, body }) {\n  return <section><h2>{title}</h2>{body}</section>;\n}\nexport function Boxed({ title, children }) {\n  return <section><h2>{title}</h2>{children}</section>;\n}\n";

const COUNTER: &str = "import { useState } from \"react\";\nexport function Counter() {\n  const [n, setN] = useState(0);\n  return <button onClick={() => setN(n + 1)}>{n}</button>;\n}\n";

#[test]
fn an_element_prop_renders_as_its_children_would() {
  let prop = html("prop", &[("src/Card.tsx", CARD), ("routes/page.tsx", "import { Card } from \"@src/Card\";\nexport default function Page() {\n  return <Card title=\"t\" body={<p>hi <b>you</b></p>} />;\n}\n")]);
  let inline = html("prop_inline", &[("src/Card.tsx", CARD), ("routes/page.tsx", "import { Boxed } from \"@src/Card\";\nexport default function Page() {\n  return <Boxed title=\"t\"><p>hi <b>you</b></p></Boxed>;\n}\n")]);
  assert!(prop.is_ok(), "{prop:?}");
  assert_eq!(prop, inline);
}

#[test]
fn a_local_renders_where_it_is_placed() {
  let local = html("local", &[("routes/page.tsx", "export default function Page() {\n  const hello = <p>hi</p>;\n  return <div>{hello}</div>;\n}\n")]);
  assert_eq!(local.as_deref(), Ok("<div><p>hi</p></div>"));
}

#[test]
fn a_local_chosen_by_a_branch_renders_the_branch_taken() {
  let src = |flag: &str| format!("export default function Page() {{\n  const on = {flag};\n  let c;\n  if (on) {{\n    c = <b>yes</b>;\n  }} else {{\n    c = <i>no</i>;\n  }}\n  return <div>{{c}}</div>;\n}}\n");
  assert_eq!(html("branch_t", &[("routes/page.tsx", &src("true"))]).as_deref(), Ok("<div><b>yes</b></div>"));
  assert_eq!(html("branch_f", &[("routes/page.tsx", &src("false"))]).as_deref(), Ok("<div><i>no</i></div>"));
}

#[test]
fn a_local_inside_a_map_callback_renders_for_each_item() {
  let out = html("map_local", &[("routes/page.tsx", "export default function Page() {\n  return <ul>{[\"a\", \"b\"].map((s) => {\n    let mark;\n    if (s === \"a\") {\n      mark = <b>{s}</b>;\n    } else {\n      mark = <i>{s}</i>;\n    }\n    return <li key={s}>{mark}</li>;\n  })}</ul>;\n}\n")]);
  assert_eq!(out.as_deref(), Ok("<ul><li><b>a</b></li><li><i>b</i></li></ul>"));
}

#[test]
fn a_map_held_in_a_name_renders_each_item() {
  let out = html("map", &[("routes/page.tsx", "export default function Page() {\n  const items = [\"a\", \"b\"].map((s) => <li key={s}>{s}</li>);\n  return <ul>{items}</ul>;\n}\n")]);
  assert_eq!(out.as_deref(), Ok("<ul><li>a</li><li>b</li></ul>"));
}

#[test]
fn a_forwarded_prop_renders_inside_the_component_it_was_passed_to() {
  let out = html("forward", &[
    ("src/Tip.tsx", "export function Inner({ children }) {\n  return <div class=\"tip\">{children}</div>;\n}\nexport function Outer({ content }) {\n  return <span><Inner>{content}</Inner></span>;\n}\n"),
    ("routes/page.tsx", "import { Outer } from \"@src/Tip\";\nexport default function Page() {\n  return <Outer content={<em>x</em>} />;\n}\n"),
  ]);
  assert_eq!(out.as_deref(), Ok("<span><div class=\"tip\"><em>x</em></div></span>"));
}

#[test]
fn markup_reads_the_scope_it_was_written_in() {
  let out = html("scope", &[
    ("src/Card.tsx", "export function Card({ body }) {\n  const name = \"callee\";\n  return <section data-name={name}>{body}</section>;\n}\n"),
    ("routes/page.tsx", "import { Card } from \"@src/Card\";\nexport default function Page() {\n  const name = \"caller\";\n  return <Card body={<b>{name}</b>} />;\n}\n"),
  ]);
  assert_eq!(out.as_deref(), Ok("<section data-name=\"callee\"><b>caller</b></section>"));
}

#[test]
fn a_prop_given_a_value_renders_the_value() {
  let out = html("value", &[("src/Card.tsx", CARD), ("routes/page.tsx", "import { Card } from \"@src/Card\";\nexport default function Page() {\n  return <div><Card title=\"a\" body={<b>m</b>} /><Card title=\"b\" body=\"text\" /></div>;\n}\n")]);
  assert_eq!(out.as_deref(), Ok("<div><section><h2>a</h2><b>m</b></section><section><h2>b</h2>text</section></div>"));
}

#[test]
fn a_bare_let_starts_as_null() {
  let out = html("bare", &[("routes/page.tsx", "export default function Page() {\n  let note;\n  return <p>{note ?? \"none\"}</p>;\n}\n")]);
  assert_eq!(out.as_deref(), Ok("<p>none</p>"));
}

#[test]
fn markup_in_a_data_object_is_refused_by_name() {
  let out = html("data", &[("routes/page.tsx", "export default function Page() {\n  const rows = [{ id: 1, content: <p>one</p> }];\n  return <ul>{rows.map((r) => <li key={r.id}>{r.content}</li>)}</ul>;\n}\n")]);
  assert!(out.as_ref().is_err_and(|e| e.contains("markup where the build cannot place it: inside data")), "{out:?}");
}

#[test]
fn markup_passed_as_a_prop_to_a_browser_component_is_refused_at_the_placement() {
  let tip = "import { useState } from \"react\";\nexport function Tip({ content }) {\n  const [open, setOpen] = useState(false);\n  return <span onClick={() => setOpen(!open)}>{open ? content : null}</span>;\n}\n";
  let out = html("island_prop", &[("src/Tip.tsx", tip), ("routes/page.tsx", "import { Tip } from \"@src/Tip\";\nexport default function Page() {\n  return <Tip content={<b>hi</b>} />;\n}\n")]);
  assert!(out.as_ref().is_err_and(|e| e.contains("markup passed as `content` to `Tip`, which renders in the browser")), "{out:?}");
}

#[test]
fn an_island_inside_an_element_prop_keys_as_it_does_inside_children() {
  let prop = rendered("isl_prop", &[("src/Card.tsx", CARD), ("src/Counter.tsx", COUNTER), ("routes/page.tsx", "import { Card } from \"@src/Card\";\nimport { Counter } from \"@src/Counter\";\nexport default function Page() {\n  return <Card title=\"t\" body={<Counter />} />;\n}\n")]);
  let inline = rendered("isl_inline", &[("src/Card.tsx", CARD), ("src/Counter.tsx", COUNTER), ("routes/page.tsx", "import { Boxed } from \"@src/Card\";\nimport { Counter } from \"@src/Counter\";\nexport default function Page() {\n  return <Boxed title=\"t\"><Counter /></Boxed>;\n}\n")]);
  let keys = |r: &Result<Rendered, String>| r.as_ref().map(|r| (r.html.clone(), r.islands.iter().map(|i| format!("{} {:?}", i.module, i.key)).collect::<Vec<_>>())).map_err(|e| e.clone());
  assert!(prop.is_ok(), "{prop:?}");
  assert_eq!(keys(&prop), keys(&inline));
}

#[test]
fn a_hoisted_read_inside_a_local_records_as_it_does_inline() {
  let money = ("src/money.ts", "export function money(n: number) {\n  return \"$\" + n.toFixed(2);\n}\n");
  let local = rendered("hoist_local", &[money, ("routes/page.tsx", "import { useState } from \"react\";\nimport { money } from \"@src/money\";\nexport default function Page({ price }) {\n  const [open, setOpen] = useState(false);\n  const label = <b>{money(price)}</b>;\n  return <button onClick={() => setOpen(!open)}>{label}</button>;\n}\n")]);
  let inline = rendered("hoist_inline", &[money, ("routes/page.tsx", "import { useState } from \"react\";\nimport { money } from \"@src/money\";\nexport default function Page({ price }) {\n  const [open, setOpen] = useState(false);\n  return <button onClick={() => setOpen(!open)}><b>{money(price)}</b></button>;\n}\n")]);
  let shape = |r: &Result<Rendered, String>| r.as_ref().map(|r| (r.html.clone(), r.hoisted.clone())).map_err(|e| e.clone());
  assert!(local.is_ok(), "{local:?}");
  assert_eq!(shape(&local), shape(&inline));
}

#[test]
fn a_read_no_placement_fills_is_left_plain() {
  let reads = |set: &ComponentSet, module: &str| format!("{:?}", set.components.iter().find(|(m, _)| m == module).unwrap().1.render).matches(SLOT_READ_PREFIX).count();
  let plain = lowered("plain", &[("src/Card.tsx", CARD), ("routes/page.tsx", "import { Card } from \"@src/Card\";\nexport default function Page() {\n  return <Card title=\"t\" body=\"b\" />;\n}\n")]).unwrap();
  assert_eq!(reads(&plain, "src/Card.tsx#Card"), 0, "nothing passes Card markup, so its reads are plain");
  let filled = lowered("filled", &[("src/Card.tsx", CARD), ("routes/page.tsx", "import { Card } from \"@src/Card\";\nexport default function Page() {\n  return <Card title=\"t\" body={<i>b</i>} />;\n}\n")]).unwrap();
  assert_eq!(reads(&filled, "src/Card.tsx#Card"), 1, "`body` is filled and `title` is not");
}
