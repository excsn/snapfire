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
  Interpreter::default().with_consts(Some(Arc::new(set.consts.clone()))).render_module(module, &library[module], &props, &library).map_err(|e| format!("{e:?}"))
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
fn markup_in_a_data_object_renders_where_it_is_read() {
  let out = html("data", &[("routes/page.tsx", "export default function Page() {\n  const rows = [{ id: 1, content: <p>one</p> }];\n  return <ul>{rows.map((r) => <li key={r.id}>{r.content}</li>)}</ul>;\n}\n")]);
  assert_eq!(out.as_deref(), Ok("<ul><li><p>one</p></li></ul>"));
}

#[test]
fn markup_passed_as_a_prop_to_a_browser_component_crosses_as_a_region_the_island_adopts() {
  let tip = "import { useState } from \"react\";\nexport function Tip({ content }) {\n  const [open, setOpen] = useState(false);\n  return <span onClick={() => setOpen(!open)}>{open ? content : null}</span>;\n}\n";
  let out = rendered("island_prop", &[("src/Tip.tsx", tip), ("routes/page.tsx", "import { Tip } from \"@src/Tip\";\nexport default function Page() {\n  return <Tip content={<b>hi</b>} />;\n}\n")]).unwrap();
  let island = &out.islands[0];
  assert_eq!(island.props.get("content").map(|v| format!("{v:?}")), Some(format!("{:?}", Value::Map([("$sfm".to_owned(), Value::Int(0))].into_iter().collect()))), "the prop crosses as the number of its region");
  assert!(island.body.html.ends_with("<template data-sf-m=\"0\"><b>hi</b></template>"), "the closed tip's content is held for the browser: {}", island.body.html);
}

#[test]
fn an_island_calling_a_function_the_server_does_not_hold_mounts_in_the_browser() {
  let tip = "import { useState } from \"react\";\nexport function Tip({ format }) {\n  const [n, setN] = useState(1);\n  return <span onClick={() => setN(n + 1)}>{format(n)}</span>;\n}\n";
  let out = rendered("island_fn", &[("src/Tip.tsx", tip), ("routes/page.tsx", "import { Tip } from \"@src/Tip\";\nexport default function Page() {\n  return <Tip format={(n) => n * 2} />;\n}\n")]).unwrap();
  let island = &out.islands[0];
  assert_eq!(island.module, "src/Tip.tsx#Tip");
  assert_eq!(island.body.html, "", "the server cannot call the browser's `format`, so the island mounts fresh rather than failing the page");
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

#[test]
fn an_array_of_markup_renders_as_its_items_would_inline() {
  let page = |body: &str| format!("import {{ Card, Boxed }} from \"@src/Card\";\nexport default function Page({{ price }}) {{\n{body}\n}}\n");
  let same = |tag: &str, array: &str, inline: &str| {
    let a = html(&format!("{tag}_array"), &[("src/Card.tsx", CARD), ("routes/page.tsx", &page(array))]);
    let b = html(&format!("{tag}_inline"), &[("src/Card.tsx", CARD), ("routes/page.tsx", &page(inline))]);
    assert!(a.is_ok(), "{tag}: {a:?}");
    assert_eq!(a, b, "{tag}");
  };
  same("returned", "  return [<p key=\"a\">a</p>, <Boxed key=\"b\" title=\"t\" />];", "  return <><p>a</p><Boxed title=\"t\" /></>;");
  same("held", "  const items = [<li key=\"a\">a</li>, <li key=\"b\">b</li>];\n  return <ul>{items}</ul>;", "  return <ul><li>a</li><li>b</li></ul>;");
  same("in_place", "  return <ul>{[<li key=\"a\">a</li>, <li key=\"b\">b</li>]}</ul>;", "  return <ul><li>a</li><li>b</li></ul>;");
  same("values", "  return <p>{[<b key=\"a\">x</b>, \" and \", price]}</p>;", "  return <p><b>x</b>{\" and \"}{price}</p>;");
  same("of_locals", "  const a = <b>a</b>;\n  const b = <i>b</i>;\n  const both = [a, b];\n  return <div>{both}</div>;", "  return <div><b>a</b><i>b</i></div>;");
  same("spread", "  const head = [<li key=\"a\">a</li>];\n  const all = [...head, <li key=\"z\">z</li>];\n  return <ul>{all}</ul>;", "  return <ul><li>a</li><li>z</li></ul>;");
  same("branch", "  const on = price > 1;\n  return <div>{[on && <b key=\"a\">on</b>, <i key=\"b\">i</i>]}</div>;", "  const on = price > 1;\n  return <div>{on && <b>on</b>}<i>i</i></div>;");
  same("prop", "  return <Card title=\"t\" body={[<p key=\"a\">a</p>, <p key=\"b\">b</p>]} />;", "  return <Boxed title=\"t\"><p>a</p><p>b</p></Boxed>;");
  for (price, tag) in [("5", "on"), ("0", "off")] {
    let fixed = |body: &str| body.replace("price > 1", &format!("{price} > 1"));
    same(&format!("push_{tag}"), &fixed("  const out = [<i key=\"a\">a</i>];\n  if (price > 1) {\n    out.push(<b key=\"b\">b</b>);\n  }\n  return <div>{out}</div>;"), &fixed("  return <div><i>a</i>{price > 1 && <b>b</b>}</div>;"));
    same(&format!("filter_{tag}"), &fixed("  return <div>{[price > 1 && <b key=\"a\">a</b>, \"\", <i key=\"b\">b</i>].filter(Boolean)}</div>;"), &fixed("  return <div>{price > 1 && <b>a</b>}<i>b</i></div>;"));
  }
  same("concat", "  const a = [<b key=\"a\">a</b>];\n  return <div>{a.concat(<i key=\"b\">b</i>, [<u key=\"c\">c</u>])}</div>;", "  return <div><b>a</b><i>b</i><u>c</u></div>;");
}

#[test]
fn markup_inside_an_array_of_data_renders_where_each_item_reads_it() {
  let out = html("data_array", &[("routes/page.tsx", "export default function Page() {\n  const tabs = [{ label: \"a\", content: <p>a</p> }];\n  return <div>{tabs.map((t) => <section key={t.label}>{t.content}</section>)}</div>;\n}\n")]);
  assert_eq!(out.as_deref(), Ok("<div><section><p>a</p></section></div>"));
}

#[test]
fn an_optional_map_renders_each_item_and_nothing_for_a_missing_list() {
  let page = |rows: &str| format!("export default function Page() {{\n  const rows = {rows};\n  return <ul>{{rows?.map((r) => <li key={{r}}>{{r}}</li>)}}</ul>;\n}}\n");
  assert_eq!(html("opt_map", &[("routes/page.tsx", &page("[\"a\", \"b\"]"))]).as_deref(), Ok("<ul><li>a</li><li>b</li></ul>"));
  assert_eq!(html("opt_map_null", &[("routes/page.tsx", &page("null"))]).as_deref(), Ok("<ul></ul>"));
  let held = html("opt_map_held", &[("routes/page.tsx", "export default function Page() {\n  const data = { rows: [\"a\"] };\n  const items = data?.rows?.map((r) => <li key={r}>{r}</li>);\n  return <ul>{items}</ul>;\n}\n")]);
  assert_eq!(held.as_deref(), Ok("<ul><li>a</li></ul>"));
}

#[test]
fn a_for_of_that_pushes_markup_renders_as_the_map_it_stands_for() {
  let page = |body: &str| format!("export default function Page() {{\n  const rows = [{{ id: \"a\", on: true }}, {{ id: \"b\", on: false }}, {{ id: \"c\", on: true }}];\n{body}\n}}\n");
  let same = |tag: &str, looped: &str, mapped: &str| {
    let a = html(&format!("{tag}_loop"), &[("routes/page.tsx", &page(looped))]);
    let b = html(&format!("{tag}_map"), &[("routes/page.tsx", &page(mapped))]);
    assert!(a.is_ok(), "{tag}: {a:?}");
    assert_eq!(a, b, "{tag}");
  };
  same("push", "  const out = [];\n  for (const r of rows) {\n    out.push(<li key={r.id}>{r.id}</li>);\n  }\n  return <ul>{out}</ul>;", "  return <ul>{rows.map((r) => <li key={r.id}>{r.id}</li>)}</ul>;");
  same("after_head", "  const out = [<li key=\"h\">head</li>];\n  for (const { id } of rows) {\n    out.push(<li key={id}>{id}</li>);\n  }\n  return <ul>{out}</ul>;", "  return <ul><li>head</li>{rows.map((r) => <li key={r.id}>{r.id}</li>)}</ul>;");
  same("if_push", "  const out = [];\n  for (const r of rows) {\n    if (r.on) {\n      out.push(<li key={r.id}>{r.id}</li>);\n    }\n  }\n  return <ul>{out}</ul>;", "  return <ul>{rows.map((r) => r.on && <li key={r.id}>{r.id}</li>)}</ul>;");
  same("continue", "  const out = [];\n  for (const r of rows) {\n    if (!r.on) continue;\n    out.push(<li key={r.id}>{r.id}</li>);\n  }\n  return <ul>{out}</ul>;", "  return <ul>{rows.map((r) => r.on && <li key={r.id}>{r.id}</li>)}</ul>;");
  same("local_in_body", "  const out = [];\n  for (const r of rows) {\n    const label = r.id.toUpperCase();\n    out.push(<li key={r.id}>{label}</li>);\n  }\n  return <ul>{out}</ul>;", "  return <ul>{rows.map((r) => <li key={r.id}>{r.id.toUpperCase()}</li>)}</ul>;");
  same("two_lists", "  const on = [];\n  const off = [];\n  for (const r of rows) {\n    if (r.on) {\n      on.push(<b key={r.id}>{r.id}</b>);\n    } else {\n      off.push(<i key={r.id}>{r.id}</i>);\n    }\n  }\n  return <div>{on}{off}</div>;", "  return <div>{rows.map((r) => r.on && <b key={r.id}>{r.id}</b>)}{rows.map((r) => !r.on && <i key={r.id}>{r.id}</i>)}</div>;");
}

#[test]
fn a_for_of_that_writes_values_folds_them_in_order() {
  let page = "export default function Page() {\n  const rows = [{ n: 2, on: true }, { n: 3, on: false }, { n: 4, on: true }];\n  let total = 0;\n  let count = 0;\n  const names = [];\n  for (const r of rows) {\n    if (!r.on) continue;\n    total += r.n;\n    count++;\n    names.push(String(r.n));\n  }\n  return <p>{total} {count} {names.join(\",\")}</p>;\n}\n";
  let inline = "export default function Page() {\n  return <p>{6} {2} {\"2,4\"}</p>;\n}\n";
  assert_eq!(html("fold_values", &[("routes/page.tsx", page)]), html("fold_values_inline", &[("routes/page.tsx", inline)]));
  let lambda = "export default function Page() {\n  const groups = [[1, 2], [3]];\n  return <ul>{groups.map((g, i) => {\n    let sum = 0;\n    for (const n of g) {\n      sum += n;\n    }\n    return <li key={i}>{sum}</li>;\n  })}</ul>;\n}\n";
  assert_eq!(html("fold_lambda", &[("routes/page.tsx", lambda)]).as_deref(), Ok("<ul><li>3</li><li>3</li></ul>"));
}

#[test]
fn a_for_of_that_stops_early_is_refused_by_name() {
  for (tag, body, says) in [
    ("break", "if (r.on) break;", "`break` inside a loop"),
    ("return", "if (r.on) return null;", "`return` inside a loop"),
    ("late_continue", "total++;\n    if (r.on) { total++; continue; }", "`continue` inside a loop"),
  ] {
    let page = format!("export default function Page() {{\n  const rows = [{{ on: true }}];\n  let total = 0;\n  for (const r of rows) {{\n    {body}\n  }}\n  return <p>{{total}}</p>;\n}}\n");
    let out = html(tag, &[("routes/page.tsx", &page)]);
    assert!(out.as_ref().is_err_and(|e| e.contains(says)), "{tag}: {out:?}");
  }
}

#[test]
fn a_counted_for_loop_folds_over_its_range() {
  let markup = "export default function Page() {\n  const out = [];\n  for (let i = 1; i <= 3; i++) {\n    if (i === 2) continue;\n    out.push(<li key={i}>{i}</li>);\n  }\n  return <ul>{out}</ul>;\n}\n";
  assert_eq!(html("counted_markup", &[("routes/page.tsx", markup)]).as_deref(), Ok("<ul><li>1</li><li>3</li></ul>"));
  let values = "export default function Page() {\n  const n = 4;\n  let sum = 0;\n  for (let i = 0; i < n; i += 1) {\n    sum += i;\n  }\n  let none = 0;\n  for (let i = 5; i < 2; i++) {\n    none++;\n  }\n  return <p>{sum} {none}</p>;\n}\n";
  assert_eq!(html("counted_values", &[("routes/page.tsx", values)]), html("counted_values_inline", &[("routes/page.tsx", "export default function Page() {\n  return <p>{6} {0}</p>;\n}\n")]));
  for (tag, head, says) in [
    ("down", "let i = 3; i > 0; i--", "whose test is not `i < b` or `i <= b`"),
    ("step", "let i = 0; i < 3; i += 2", "that does not step by one"),
    ("writes_counter", "let i = 0; i < 3; i++) { i++; } for (let j = 0; j < 1; j++", "whose body writes `i`"),
    ("bound_moves", "let i = 0; i < total; i++", "whose bound reads `total`, which its body writes"),
  ] {
    let page = format!("export default function Page() {{\n  let total = 1;\n  for ({head}) {{\n    total++;\n  }}\n  return <p>{{total}}</p>;\n}}\n");
    let out = html(tag, &[("routes/page.tsx", &page)]);
    assert!(out.as_ref().is_err_and(|e| e.contains(says)), "{tag}: {out:?}");
  }
}

#[test]
fn a_module_level_helper_that_returns_markup_renders_where_its_call_goes() {
  let page = |body: &str| format!("const item = (label: string, at: number) => <li key={{at}}>{{label}}</li>;\nfunction list(labels: string[]) {{\n  if (labels.length === 0) {{\n    return null;\n  }}\n  return <ul>{{labels.map((l, i) => {{\n    const one = item(l, i);\n    return one;\n  }})}}</ul>;\n}}\nexport default function Page() {{\n  const on = true;\n{body}\n}}\n");
  let same = |tag: &str, called: &str, inline: &str| {
    let a = html(&format!("{tag}_called"), &[("routes/page.tsx", &page(called))]);
    let b = html(&format!("{tag}_inline"), &[("routes/page.tsx", &page(inline))]);
    assert!(a.is_ok(), "{tag}: {a:?}");
    assert_eq!(a, b, "{tag}");
  };
  same("helper_direct", "  return <div>{list([\"a\", \"b\"])}</div>;", "  return <div><ul><li>a</li><li>b</li></ul></div>;");
  same("helper_held", "  const shown = on ? list([\"a\"]) : null;\n  return <div>{shown}</div>;", "  return <div><ul><li>a</li></ul></div>;");
  same("helper_empty", "  return <div>{list([])}</div>;", "  return <div></div>;");
}

#[test]
fn clsx_and_classnames_join_class_names_as_the_package_does() {
  let page = |import: &str, call: &str| format!("{import}\nexport default function Page({{ price }}) {{\n  const on = price > 1;\n  return <p className={{{call}(\"a\", on && \"b\", !on && \"c\", {{ d: on, e: !on }}, [\"f\", undefined])}}>x</p>;\n}}\n");
  let want = Ok("<p class=\"a b d f\">x</p>".to_owned());
  assert_eq!(html("clsx_named", &[("routes/page.tsx", &page("import { clsx } from \"clsx\";", "clsx"))]), want);
  assert_eq!(html("clsx_default", &[("routes/page.tsx", &page("import cx from \"clsx\";", "cx"))]), want);
  assert_eq!(html("classnames", &[("routes/page.tsx", &page("import classNames from \"classnames\";", "classNames"))]), want);
}

#[test]
fn typeof_guards_render_as_they_do_in_the_browser() {
  let page = "const label = (v: unknown) => typeof v === \"string\" ? v : typeof v === \"number\" ? `#${v}` : typeof v === \"object\" && v !== null ? \"obj\" : \"none\";\nexport default function Page() {\n  const vs = [\"a\", 2, { k: 1 }, null, true];\n  return <ul>{vs.map((v, i) => <li key={i}>{label(v)}</li>)}</ul>;\n}\n";
  assert_eq!(html("typeof_guards", &[("routes/page.tsx", page)]).as_deref(), Ok("<ul><li>a</li><li>#2</li><li>obj</li><li>none</li><li>none</li></ul>"));
}

#[test]
fn a_tag_a_local_chooses_from_listed_literals_renders_that_element() {
  let table = "const toneClass = {\n  hero: { Heading: \"h1\", text: \"big\" },\n  section: { Heading: \"h2\", text: \"mid\" },\n} as const;\n";
  let page = |tone: &str| format!("{table}export default function Page() {{\n  const {{ Heading, text }} = toneClass[\"{tone}\" as \"hero\" | \"section\"];\n  return <Heading className={{text}}>hi</Heading>;\n}}\n");
  assert_eq!(html("tag_hero", &[("routes/page.tsx", &page("hero"))]).as_deref(), Ok("<h1 class=\"big\">hi</h1>"));
  assert_eq!(html("tag_section", &[("routes/page.tsx", &page("section"))]).as_deref(), Ok("<h2 class=\"mid\">hi</h2>"));
  let rows: String = (0..20).map(|i| format!("  k{i}: {{ Tag: \"h{}\", text: \"t{i} with enough text to be named once\" }},\n", 1 + i % 6)).collect();
  let big = format!("const levels = {{\n{rows}}} as const;\nexport default function Page() {{\n  const {{ Tag }} = levels[\"k3\" as keyof typeof levels];\n  return <Tag>x</Tag>;\n}}\n");
  assert_eq!(html("tag_const", &[("routes/page.tsx", &big)]).as_deref(), Ok("<h4>x</h4>"));
  let ternary = "export default function Page({ price }) {\n  const Tag = price > 1 ? \"strong\" : \"em\";\n  return <Tag>x</Tag>;\n}\n";
  assert_eq!(html("tag_ternary", &[("routes/page.tsx", ternary)]).as_deref(), Ok("<strong>x</strong>"));
  let from_prop = "export default function Page({ as }) {\n  const Tag = as ?? \"aside\";\n  return <Tag>x</Tag>;\n}\n";
  assert_eq!(html("tag_prop", &[("routes/page.tsx", from_prop)]).as_deref(), Ok("<aside>x</aside>"), "a tag the build cannot list is read at render");
  for (tag, page, says) in [
    ("tag_upper", "export default function Page({ price }) {\n  const Tag = price > 1 ? \"Strong\" : \"em\";\n  return <Tag>x</Tag>;\n}\n", "not a lowercase element name"),
  ] {
    let out = html(tag, &[("routes/page.tsx", page)]);
    assert!(out.as_ref().is_err_and(|e| e.contains(says)), "{tag}: {out:?}");
  }
}

#[test]
fn children_count_counts_what_the_placement_passed_as_react_does() {
  let grid = "import { Children } from \"react\";\nexport function Grid({ children }) {\n  const n = Children.toArray(children).length;\n  return <div data-n={n}>{children}</div>;\n}\nexport function Wrap({ children }) {\n  return <Grid>{children}</Grid>;\n}\n";
  let counted = |tag: &str, inner: &str| -> String {
    let page = format!("import {{ Grid, Wrap }} from \"@src/Grid\";\nexport default function Page({{ price }}) {{\n  const rows = [\"a\", \"b\"];\n  const on = price > 1;\n  return {inner};\n}}\n");
    let out = html(tag, &[("src/Grid.tsx", grid), ("routes/page.tsx", &page)]).unwrap_or_else(|e| panic!("{tag}: {e}"));
    let at = out.find("data-n=\"").unwrap_or_else(|| panic!("{tag}: {out}")) + 8;
    out[at..].split('"').next().unwrap().to_owned()
  };
  assert_eq!(counted("cc_elements", "<Grid><p>a</p><p>b</p><p>c</p></Grid>"), "3");
  assert_eq!(counted("cc_map", "<Grid>{rows.map((r) => <p key={r}>{r}</p>)}<p>z</p></Grid>"), "3");
  assert_eq!(counted("cc_falsy", "<Grid>{null}{false}{on && <p>on</p>}{!on && <p>off</p>}</Grid>"), "1");
  assert_eq!(counted("cc_fragment", "<Grid><><p>a</p><p>b</p></></Grid>"), "1");
  assert_eq!(counted("cc_array", "<Grid>{[<p key=\"a\">a</p>, <p key=\"b\">b</p>]}</Grid>"), "2");
  assert_eq!(counted("cc_through", "<Wrap><p>a</p>{rows.map((r) => <p key={r}>{r}</p>)}</Wrap>"), "3");
  let react = "import React from \"react\";\nexport default function Page() {\n  return <Count><i>a</i><i>b</i></Count>;\n}\nfunction Count({ children }) {\n  return <b>{React.Children.count(children)}</b>;\n}\n";
  assert_eq!(html("cc_react", &[("routes/page.tsx", react)]).as_deref(), Ok("<b>2</b>"));
  let empties = "import { Children } from \"react\";\nexport default function Page({ price }) {\n  const on = price > 1;\n  return <Count>{null}{false}{!on && <i>x</i>}<i>b</i></Count>;\n}\nfunction Count({ children }) {\n  return <b>{Children.count(children)} {Children.toArray(children).length}</b>;\n}\n";
  assert_eq!(html("cc_empties", &[("routes/page.tsx", empties)]), html("cc_empties_inline", &[("routes/page.tsx", "export default function Page() {\n  return <b>{4} {1}</b>;\n}\n")]), "count keeps empty nodes, toArray leaves them out");
}
