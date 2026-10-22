//! A `Map`, a `Set`, a `Date` or a bigint keeps its kind across a prop, so
//! `instanceof` and `typeof` lower to constants, its methods lower, and an
//! island mounts with the real type.

use std::path::PathBuf;
use std::sync::Arc;

use snapfire_fsr_core::value::typed;
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_ir::render::{prepare, Components, Rendered, TYPES_PROP};
use snapfire_fsr_ir::Interpreter;
use snapfire_fsr_lower::component::ComponentSet;

fn app(tag: &str, files: &[(&str, &str)]) -> PathBuf {
  let dir = std::env::temp_dir().join(format!("fsr_typed_values_{}_{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  for (name, text) in files {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
  }
  dir
}

fn render(tag: &str, files: &[(&str, &str)]) -> Result<Rendered, String> {
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
  Interpreter::default().with_consts(Some(Arc::new(set.consts.clone()))).render_module(module, &library[module], &ValueMap::default(), &library).map_err(|e| format!("{e:?}"))
}

fn html(tag: &str, files: &[(&str, &str)]) -> Result<String, String> {
  render(tag, files).map(|r| r.html)
}

const VIEWER_JS: &str = "export default function Viewer({ map, set, date, big }) {\n  return <div><ul>{[...map.entries()].map(([k, v]) => <li key={k}>{k}: {v}</li>)}</ul><p>{map instanceof Map ? \"map\" : \"no\"} {set instanceof Set ? \"set\" : \"no\"} {date instanceof Date ? \"date\" : \"no\"} {typeof big === \"bigint\" ? \"bigint\" : \"no\"}</p><p>{[...set].join(\"+\")} {date.toISOString()} {big.toString()}</p></div>;\n}\n";

const PAGE: &str = "import Viewer from \"@src/Viewer\";\nexport default function Page() {\n  const map = new Map([[\"alice\", 100], [\"bob\", 200]]);\n  const set = new Set([\"react\", \"next\"]);\n  const date = new Date(\"2025-06-15T12:00:00Z\");\n  const big = 9007199254740993n;\n  return <Viewer map={map} set={set} date={date} big={big} />;\n}\n";

const VIEWED: &str = "<div><ul><li>alice: 100</li><li>bob: 200</li></ul><p>map set date bigint</p><p>react+next 2025-06-15T12:00:00.000Z 9007199254740993</p></div>";

#[test]
fn instanceof_and_typeof_of_a_value_made_in_place_are_constants() {
  let out = html("in_place", &[("routes/page.tsx", "export default function Page() {\n  const m = new Map();\n  const s = new Set([1]);\n  const d = new Date(\"2025-06-15T12:00:00Z\");\n  const b = BigInt(7);\n  const a = [1];\n  return <p>{m instanceof Map ? 1 : 0}{s instanceof Set ? 1 : 0}{d instanceof Date ? 1 : 0}{m instanceof Set ? 1 : 0}{a instanceof Array ? 1 : 0}{typeof b}{typeof m}{typeof 3n}</p>;\n}\n")]);
  assert_eq!(out.as_deref(), Ok("<p>11101bigintobjectbigint</p>"));
}

#[test]
fn a_prop_takes_its_kind_from_the_placement() {
  let out = html("placement", &[("src/Viewer.js", VIEWER_JS), ("routes/page.tsx", PAGE)]);
  assert_eq!(out.as_deref(), Ok(VIEWED));
}

#[test]
fn a_prop_takes_its_kind_from_the_props_type() {
  let typed = "export default function Viewer({ map, set, date, big }: { map: Map<string, number>; set: Set<string>; date: Date; big: bigint }) {\n  return <p>{map instanceof Map ? \"map\" : \"no\"} {set.has(\"next\") ? \"has\" : \"no\"} {date instanceof Date ? \"date\" : \"no\"} {typeof big}</p>;\n}\n";
  let page = "import Viewer from \"@src/Viewer\";\nfunction made() {\n  return { map: new Map([[\"a\", 1]]), set: new Set([\"next\"]), date: new Date(\"2025-06-15T12:00:00Z\"), big: 5n };\n}\nexport default function Page() {\n  const v = made();\n  return <Viewer map={v.map} set={v.set} date={v.date} big={v.big} />;\n}\n";
  assert_eq!(html("typed_inline", &[("src/Viewer.tsx", typed), ("routes/page.tsx", page)]).as_deref(), Ok("<p>map has date bigint</p>"));
  let aliased = "type Props = { map: Map<string, number>; set: ReadonlySet<string>; date: Date; big: bigint };\nexport default function Viewer(props: Props) {\n  return <p>{props.map instanceof Map ? \"map\" : \"no\"} {props.set.has(\"next\") ? \"has\" : \"no\"} {props.map.size} {typeof props.big}</p>;\n}\n";
  assert_eq!(html("typed_alias", &[("src/Viewer.tsx", aliased), ("routes/page.tsx", page)]).as_deref(), Ok("<p>map has 1 bigint</p>"));
  let interface = "interface Props {\n  map: Map<string, number>;\n  set: Set<string>;\n  date: Date;\n  big: bigint;\n}\nexport default function Viewer({ map, date }: Props) {\n  return <p>{map instanceof Map ? \"map\" : \"no\"} {date.getUTCFullYear()}</p>;\n}\n";
  assert_eq!(html("typed_interface", &[("src/Viewer.tsx", interface), ("routes/page.tsx", page)]).as_deref(), Ok("<p>map 2025</p>"));
}

#[test]
fn an_island_mounts_with_its_typed_props_as_the_real_types() {
  let island = "import { useState } from \"react\";\nexport default function Viewer({ map, set, date, big }) {\n  const [n, setN] = useState(0);\n  return <button onClick={() => setN(n + 1)}>{map instanceof Map ? map.size : 0}{n}</button>;\n}\n";
  let rendered = render("island", &[("src/Viewer.js", island), ("routes/page.tsx", PAGE)]).unwrap();
  assert_eq!(rendered.islands[0].body.html, "<button>20</button>");
  let mount = rendered.islands[0].mount_props();
  assert!(!mount.contains_key(TYPES_PROP), "{mount:?}");
  let pairs = Value::seq(vec![Value::seq(vec![Value::str("alice"), Value::F64(100.0)]), Value::seq(vec![Value::str("bob"), Value::F64(200.0)])]);
  assert_eq!(mount.get("map"), Some(&Value::Variant { tag: typed::MAP.to_owned(), payload: Some(Box::new(pairs)) }));
  assert_eq!(mount.get("set"), Some(&Value::Variant { tag: typed::SET.to_owned(), payload: Some(Box::new(Value::seq(vec![Value::str("react"), Value::str("next")]))) }));
  assert_eq!(mount.get("date"), Some(&Value::Variant { tag: typed::DATE.to_owned(), payload: Some(Box::new(Value::F64(1_749_988_800_000.0))) }));
  assert_eq!(mount.get("big"), Some(&Value::Variant { tag: typed::BIGINT.to_owned(), payload: Some(Box::new(Value::Int(9_007_199_254_740_993))) }));
}

#[test]
fn a_typed_prop_the_placement_cannot_see_is_still_tagged_for_the_island() {
  let island = "import { useState } from \"react\";\nexport default function Viewer({ items }: { items: Set<string> }) {\n  const [n, setN] = useState(0);\n  return <button onClick={() => setN(n + 1)}>{items.size}</button>;\n}\n";
  let page = "import Viewer from \"@src/Viewer\";\nfunction made() {\n  return new Set([\"a\", \"b\"]);\n}\nexport default function Page() {\n  return <Viewer items={made()} />;\n}\n";
  let rendered = render("island_typed", &[("src/Viewer.tsx", island), ("routes/page.tsx", page)]).unwrap();
  assert_eq!(rendered.islands[0].body.html, "<button>2</button>");
  let mount = rendered.islands[0].mount_props();
  assert_eq!(mount.get("items"), Some(&Value::Variant { tag: typed::SET.to_owned(), payload: Some(Box::new(Value::seq(vec![Value::str("a"), Value::str("b")]))) }));
}

#[test]
fn instanceof_on_a_value_of_unknown_kind_is_refused_with_a_hint() {
  let out = html("unknown", &[("src/Viewer.js", "export default function Viewer({ v }) {\n  return <p>{v instanceof Map ? 1 : 0}</p>;\n}\n"), ("routes/page.tsx", "import Viewer from \"@src/Viewer\";\nexport default function Page() {\n  return <><Viewer v={new Map()} /><Viewer v={new Set()} /></>;\n}\n")]);
  assert!(out.as_ref().is_err_and(|e| e.contains("`instanceof Map` on a value whose type the build does not know")), "{out:?}");
}

#[test]
fn a_placement_passing_another_kind_for_a_prop_the_component_relies_on_is_reported_there() {
  let viewer = "export default function Viewer({ v }) {\n  return <p>{v instanceof Set ? \"set\" : \"other\"}</p>;\n}\n";
  let first = "import Viewer from \"@src/Viewer\";\nexport default function First() {\n  return <Viewer v={new Set([1])} />;\n}\n";
  let second = "import Viewer from \"@src/Viewer\";\nexport default function Second() {\n  return <Viewer v={new Map()} />;\n}\n";
  let page = "import First from \"@src/First\";\nimport Second from \"@src/Second\";\nexport default function Page() {\n  return <><First /><Second /></>;\n}\n";
  let out = html("conflict", &[("src/Viewer.js", viewer), ("src/First.tsx", first), ("src/Second.tsx", second), ("routes/page.tsx", page)]);
  assert!(out.as_ref().is_err_and(|e| e.starts_with("src/Second.tsx#default: ") && e.contains("`Viewer` reads its `v` prop as a `Set` and this placement passes a `Map`")), "{out:?}");
}

#[test]
fn a_url_is_built_read_and_written_as_url_does() {
  let page = "export default function Page() {\n  const url = new URL(\"/plus-referral?x=1#top\", \"https://www.example.com\");\n  url.searchParams.set(\"code\", \"a b\");\n  url.searchParams.append(\"next\", \"z\");\n  url.searchParams.delete(\"x\");\n  url.pathname = \"/p\";\n  return <a href={url.href}>{url.origin} {url.pathname}{url.search}{url.hash} {url.searchParams.get(\"code\")} {String(url)} {typeof url}</a>;\n}\n";
  assert_eq!(html("url", &[("routes/page.tsx", page)]).as_deref(), Ok("<a href=\"https://www.example.com/p?code=a+b&amp;next=z#top\">https://www.example.com /p?code=a+b&amp;next=z#top a b https://www.example.com/p?code=a+b&amp;next=z#top object</a>"));
}

#[test]
fn a_local_url_search_params_is_written_by_set_append_and_delete() {
  let page = "export default function Page() {\n  const q = new URLSearchParams(\"a=1&b=2&a=5\");\n  q.set(\"a\", \"3\");\n  q.delete(\"b\");\n  q.append(\"c\", \"4\");\n  return <p>{q.toString()} {q.getAll(\"a\").length}</p>;\n}\n";
  assert_eq!(html("query_writes", &[("routes/page.tsx", page)]).as_deref(), Ok("<p>a=3&amp;c=4 1</p>"));
}

#[test]
fn an_invalid_url_fails_the_render_as_new_url_throws() {
  let out = html("bad_url", &[("routes/page.tsx", "export default function Page() {\n  const url = new URL(\"not a url\");\n  return <p>{url.href}</p>;\n}\n")]);
  assert!(out.as_ref().is_err_and(|e| e.contains("Invalid URL")), "{out:?}");
}

#[test]
fn an_island_mounts_with_a_url_prop_as_a_url() {
  let island = "import { useState } from \"react\";\nexport default function Viewer({ url }: { url: URL }) {\n  const [n, setN] = useState(0);\n  return <button onClick={() => setN(n + 1)}>{url.hostname}{n}</button>;\n}\n";
  let page = "import Viewer from \"@src/Viewer\";\nexport default function Page() {\n  return <Viewer url={new URL(\"https://a.example/x?y=1\")} />;\n}\n";
  let rendered = render("island_url", &[("src/Viewer.tsx", island), ("routes/page.tsx", page)]).unwrap();
  assert_eq!(rendered.islands[0].body.html, "<button>a.example0</button>");
  assert_eq!(rendered.islands[0].mount_props().get("url"), Some(&Value::Variant { tag: typed::URL.to_owned(), payload: Some(Box::new(Value::str("https://a.example/x?y=1"))) }));
}

#[test]
fn a_loader_builds_a_url_from_the_query() {
  let loader = "export async function load({ query }: Ctx) {\n  const url = new URL(\"https://www.example.com/plus-referral\");\n  url.searchParams.set(\"code\", query.code);\n  url.hash = \"top\";\n  return { link: url.href, path: url.pathname };\n}\n";
  let body = snapfire_fsr_lower::lower_loader("page.loader.ts", loader).unwrap_or_else(|e| panic!("{e}"));
  let mut ctx = snapfire_fsr_runtime::RequestCtx::anonymous(Default::default());
  ctx.query.insert("code".to_owned(), "a&b".to_owned());
  let out = futures::executor::block_on(Interpreter::default().run(&body, &ctx, None)).map(|outcome| outcome.value).unwrap_or_else(|e| panic!("{}", e.message));
  let Value::Map(fields) = out else { panic!("{out:?}") };
  assert_eq!(fields.get("link"), Some(&Value::str("https://www.example.com/plus-referral?code=a%26b#top")));
  assert_eq!(fields.get("path"), Some(&Value::str("/plus-referral")));
}
