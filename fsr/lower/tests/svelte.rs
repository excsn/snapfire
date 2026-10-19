//! The Svelte front end against Svelte's own server renderer: each fixture is
//! lowered, rendered in Rust as an island and compared byte for byte with
//! what `svelte/server` writes for the same component, props and children,
//! hydration markers included.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use snapfire_compiler_wire::{Options, Outcome};
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_ir::ast::{Component, Entry, Expr, Tmpl};
use snapfire_fsr_ir::render::Components;
use snapfire_fsr_ir::{Frameworks, Interpreter, Owner, SvelteMajor};
use snapfire_fsr_lower::component::ComponentSet;
use snapfire_svelte::Compiler;

thread_local! {
  static COMPILER: Compiler = Compiler::new().expect("the compiler boots");
}

fn runtime() -> PathBuf {
  Path::new(env!("CARGO_MANIFEST_DIR")).join("../../compiler/svelte/tests/runtime")
}

/// What the components a fixture imports by bare specifier stand for in the oracle.
fn stubs() -> BTreeMap<String, String> {
  BTreeMap::from([
    ("@snapfire/fsr-client/svelte".to_owned(), "export function useStore(key, initial) { return { subscribe(run) { run(initial); return () => {}; } }; }\nexport function useLocale() { return { subscribe(run) { run('en'); return () => {}; } }; }\n".to_owned()),
    ("@snapfire/fsr-client".to_owned(), "export function get() { return 0; }\nexport async function optimistic() {}\n".to_owned()),
    ("@snapfire/fsr-client/store".to_owned(), "export function key(name) { return name; }\n".to_owned()),
  ])
}

fn app(tag: &str, files: &[(&str, &str)]) -> PathBuf {
  let dir = std::env::temp_dir().join(format!("fsr_svelte_{}_{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  std::fs::create_dir_all(&dir).unwrap();
  for (name, source) in files {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, source).unwrap();
  }
  dir
}

fn value_json(v: &Value) -> String {
  match v {
    Value::Null => "null".to_owned(),
    Value::Bool(b) => b.to_string(),
    Value::Int(n) => n.to_string(),
    Value::F64(f) => f.to_string(),
    Value::Str(s) => serde_json::to_string(s.as_str()).unwrap(),
    Value::Seq(items) => format!("[{}]", items.iter().map(value_json).collect::<Vec<_>>().join(",")),
    Value::Map(map) => format!("{{{}}}", map.iter().map(|(k, v)| format!("{}:{}", serde_json::to_string(k.as_str()).unwrap(), value_json(v))).collect::<Vec<_>>().join(",")),
    other => panic!("{other:?}"),
  }
}

fn props(pairs: &[(&str, Value)]) -> ValueMap {
  pairs.iter().map(|(k, v)| ((*k).to_owned(), v.clone())).collect()
}

/// A page placing `module` as an island with `props` and `children`.
fn host(module: &str, children: Vec<Tmpl>) -> Component {
  Component { body: Vec::new(), render: Tmpl::Island { module: module.to_owned(), props: vec![Entry::Spread(Expr::var("$props"))], children, when: None, mode: None, id: 0, define: false }, state: Vec::new(), stores: Vec::new(), handlers: Vec::new(), owner: Owner::Fsr, shadow: None, scope: None }
}

/// The lowered render of the first of `files` and Svelte's own, for `props` and `children`.
fn both(tag: &str, files: &[(&str, &str)], props: &ValueMap, children: Option<&str>) -> (String, String) {
  let dir = app(tag, files);
  let mut set = ComponentSet::new(&dir);
  let mut modules = stubs();
  COMPILER.with(|compiler| {
    for (file, source) in files.iter().filter(|(f, _)| f.ends_with(".svelte")) {
      match compiler.describe(file, source, &Options::default()).expect("the driver answers") {
        Outcome::Described(described) => set.describe(*file, described),
        other => panic!("{file}: not described: {other:?}"),
      }
      match compiler.ssr_module(file, source).expect("the driver answers") {
        Outcome::Ok(compiled) => modules.insert(format!("/{file}"), compiled.js),
        other => panic!("{file}: no server module: {other:?}"),
      };
    }
  });
  let entry = files[0].0;
  let module = format!("{entry}#default");
  if let Err(e) = set.lower(&module) {
    panic!("{tag}: does not lower: {e}");
  }
  let library: Components = set.components.iter().map(|(m, c)| (m.clone(), Arc::new(snapfire_fsr_ir::render::prepare(c)))).collect();
  let interpreter = Interpreter::default().with_frameworks(Frameworks::default().svelte(Some(SvelteMajor::V5)));
  let child_tree = children.map(|c| vec![Tmpl::Text(c.to_owned())]).unwrap_or_default();
  let rendered = interpreter.render(&host(&module, child_tree), props, &library).expect("renders");
  assert_eq!(rendered.islands.len(), 1, "{rendered:?}");
  let rust = rendered.islands[0].body.html.clone();
  let svelte = COMPILER.with(|compiler| compiler.render_module(&format!("/{entry}"), &modules, &value_json(&Value::Map(props.clone())), children, &runtime())).expect("Svelte renders");
  let _ = std::fs::remove_dir_all(&dir);
  (rust, svelte)
}

fn check(tag: &str, source: &str, props: &ValueMap) {
  let (rust, svelte) = both(tag, &[("src/C.svelte", source)], props, None);
  assert_eq!(rust, svelte, "{tag}:\n{source}");
}

fn s(text: &str) -> Value {
  Value::str(text)
}

#[test]
fn text_and_expressions_render_as_svelte_server_writes_them() {
  check("text", "hello", &props(&[]));
  check("one", "<p>a</p>", &props(&[]));
  check("two", "<p>a</p><p>b</p>", &props(&[]));
  check("space", "\n<p>a</p>\n\n  <p>b</p>\n", &props(&[]));
  check("exprs", "<script>let { a, b } = $props();</script><p>x {a} y {b}</p>", &props(&[("a", Value::int(1i64)), ("b", s("<i>"))]));
  check("null", "<script>let { a } = $props();</script><p>[{a}]</p>", &props(&[("a", Value::Null)]));
  check("entities", "<p>a &amp; b &lt; {'&'}</p>", &props(&[]));
}

#[test]
fn blocks_carry_the_markers_svelte_server_writes() {
  let chain = "<script>let { n } = $props();</script>{#if n == 0}<b>0</b>{:else if n == 1}<b>1</b>{:else}<b>2</b>{/if}";
  for n in [0i64, 1, 5] {
    check("chain", chain, &props(&[("n", Value::int(n))]));
  }
  let open = "<script>let { n } = $props();</script>{#if n == 0}<b>0</b>{:else if n == 1}<b>1</b>{/if}";
  check("open", open, &props(&[("n", Value::int(5i64))]));
  let nested = "<script>let { a, b } = $props();</script>{#if a}<i>a</i>{:else}{#if b}<i>b</i>{/if}{/if}";
  check("nested", nested, &props(&[("a", Value::Bool(false)), ("b", Value::Bool(true))]));
  let each = "<script>let { xs } = $props();</script><ul>{#each xs as x, i}<li>{i}:{x}</li>{/each}</ul>";
  check("each", each, &props(&[("xs", Value::seq(vec![s("a"), s("b")]))]));
  check("each-empty", each, &props(&[("xs", Value::seq(Vec::new()))]));
  let fallback = "<script>let { xs } = $props();</script>{#each xs as x}<li>{x}</li>{:else}<p>none</p>{/each}";
  check("each-else-empty", fallback, &props(&[("xs", Value::seq(Vec::new()))]));
  check("each-else-full", fallback, &props(&[("xs", Value::seq(vec![s("a")]))]));
  check("text-if", "<script>let { on } = $props();</script><p>a {#if on}b{/if} c</p>", &props(&[("on", Value::Bool(true))]));
  check("const", "<script>let { xs } = $props();</script>{#each xs as x}{@const y = x + 1}<i>{y}</i>{/each}", &props(&[("xs", Value::seq(vec![Value::int(1i64), Value::int(2i64)]))]));
  check("snippet", "<script>let { xs } = $props();</script>{#snippet row(x)}<i>{x}</i>{/snippet}{#each xs as x}{@render row(x)}{/each}", &props(&[("xs", Value::seq(vec![Value::int(1i64)]))]));
  check("snippet-two", "{#snippet s(x)}<i>{x}</i><b>2</b>{/snippet}<p>{@render s(1)}</p>", &props(&[]));
  check("html", "<script>let { h } = $props();</script><div>{@html h}</div>", &props(&[("h", s("<b>x</b>"))]));
}

#[test]
fn attributes_print_as_svelte_server_prints_them() {
  check("bool", "<script>let { d, h } = $props();</script><input disabled={d} hidden={h} value=\"v\">", &props(&[("d", Value::Bool(true)), ("h", Value::Bool(false))]));
  check("null-attr", "<script>let { t } = $props();</script><a title={t} href=\"/x\">a</a>", &props(&[("t", Value::Null)]));
  check("escape", "<script>let { t } = $props();</script><p title={t}>x</p>", &props(&[("t", s("a\"b&c<d"))]));
  check("template", "<script>let { n } = $props();</script><a href=\"/x/{n}\" title=\"n: {n}\">x</a>", &props(&[("n", Value::int(3i64))]));
  check("class", "<script>let { on } = $props();</script><p class={{ a: on, b: !on }}>x</p><p class={['c', on && 'd']}>y</p><p class=\"e\" class:f={on}>z</p>", &props(&[("on", Value::Bool(true))]));
  check("void", "<br><img src=\"x.png\" alt=\"\">", &props(&[]));
  check("svg", "<svg viewBox=\"0 0 1 1\"><path d=\"M0\"/></svg>", &props(&[]));
  check("textarea", "<script>let { v } = $props();</script><textarea value={v}></textarea>", &props(&[("v", s("a<b"))]));
  check("handler", "<script>let n = $state(0);</script><button onclick={() => n++}>{n}</button>", &props(&[]));
}

#[test]
fn runes_bind_as_lets_the_template_reads() {
  let source = "<script>\n  let { title = 'none', rows = [] } = $props();\n  let n = $state(2);\n  const doubled = $derived(n * 2);\n  const total = $derived.by(() => rows.length + n);\n</script>\n\n<h1>{title}</h1><p>{n} {doubled} {total}</p>";
  check("runes", source, &props(&[("rows", Value::seq(vec![s("a")]))]));
  check("runes-given", source, &props(&[("title", s("t")), ("rows", Value::seq(Vec::new()))]));
}

#[test]
fn legacy_props_and_reactive_values_bind_as_lets() {
  let source = "<script>\n  export let a = 1;\n  export let label;\n  let n = 2;\n  $: d = n * a;\n</script>\n\n<p>{label}: {a} {d}</p>";
  check("legacy", source, &props(&[("a", Value::int(3i64)), ("label", s("x"))]));
  check("legacy-default", source, &props(&[("label", s("y"))]));
}

#[test]
fn a_child_component_renders_in_place_with_its_anchor() {
  let child = "<script>let { t, children } = $props();</script><i>{t}</i>{@render children?.()}";
  let one = "<script>let { t } = $props();</script><i>{t}</i>";
  let cases: &[(&str, &str, &str, ValueMap)] = &[
    ("in-div", "<script>import C from './C.svelte';</script><div><C t=\"a\"/></div>", one, props(&[])),
    ("two-roots", "<script>import C from './C.svelte';</script><div><C t=\"a\"/></div>", child, props(&[])),
    ("kids", "<script>import C from './C.svelte';</script><C t=\"a\"><b>k</b></C>", child, props(&[])),
    ("siblings", "<script>import C from './C.svelte';</script><C t=\"a\"/><C t=\"b\"/>", one, props(&[])),
    ("in-each", "<script>import C from './C.svelte'; let { xs } = $props();</script>{#each xs as x}<C t={x}/>{/each}", one, props(&[("xs", Value::seq(vec![s("a"), s("b")]))])),
    ("in-if", "<script>import C from './C.svelte'; let { on } = $props();</script>{#if on}<C t=\"a\"/>{/if}", one, props(&[("on", Value::Bool(true))])),
    ("text", "<script>import C from './C.svelte';</script><p><C t=\"a\"/>!</p>", "<script>let { t } = $props();</script>{t}", props(&[])),
  ];
  for (tag, parent, child_source, given) in cases {
    let (rust, svelte) = both(tag, &[("src/P.svelte", parent), ("src/C.svelte", child_source)], given, None);
    assert_eq!(rust, svelte, "{tag}:\n{parent}");
  }
}

#[test]
fn an_islands_children_render_where_the_component_renders_its_children() {
  let source = "<script>let { label, children } = $props();</script><section><h2>{label}</h2>{@render children?.()}</section>";
  let (rust, svelte) = both("children", &[("src/C.svelte", source)], &props(&[("label", s("l"))]), Some("kids"));
  assert_eq!(rust, svelte);
}
