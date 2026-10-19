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
    ("@snapfire/fsr-client/svelte".to_owned(), "export function useStore(key, initial) { return { subscribe(run) { run(initial); return () => {}; } }; }\nexport function useLocale() { return { subscribe(run) { run('en'); return () => {}; } }; }\nexport function link(options) { return { href: options.href }; }\nexport function island() { return () => () => {}; }\n".to_owned() + PICTURE_PARTS),
    ("@snapfire/fsr-client".to_owned(), "export function get() { return 0; }\nexport async function optimistic() {}\nexport function key(name) { return name; }\n".to_owned()),
    ("@snapfire/fsr-client/store".to_owned(), "export function key(name) { return name; }\n".to_owned()),
  ])
}

/// The client's `pictureParts` for a string `src`, under the default policy.
const PICTURE_PARTS: &str = "export function picture(props) {\n  const { src, source, priority: high, widths, quality, sizes, loading, decoding, ...rest } = props;\n  const img = { ...rest, loading: loading ?? (high ? 'eager' : 'lazy'), decoding: decoding ?? 'async' };\n  if (high) img.fetchpriority = 'high';\n  img.src = src;\n  return { img, sources: null };\n}\n";

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
    for (file, source) in files.iter().filter(|(f, _)| f.ends_with(".js")) {
      modules.insert(format!("/{file}"), (*source).to_owned());
    }
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
  let siblings = "<script>let { h } = $props();</script><p>a {@html h} b</p>{@html h}<i>x</i>{#if h}{@html h}{/if}";
  for h in [s("<b>x</b>"), Value::Null, Value::int(3i64), s("")] {
    check("html-siblings", siblings, &props(&[("h", h)]));
  }
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
  let styled = "<script>let { c, w } = $props();</script><p style=\"margin: 0; color: blue; /* x */ background: url('a;b')\" style:color={c}>x</p><p style:color={c} style:width=\"{w}%\">y</p><p style:color={c}>z</p>";
  check("style", styled, &props(&[("c", s("red")), ("w", Value::int(40i64))]));
  check("style-null", styled, &props(&[("c", Value::Null), ("w", Value::int(0i64))]));
}

#[test]
fn class_directives_apply_to_any_class_value() {
  let source = "<script>let { on, c } = $props();</script><p class={{ k: on }} class:z={on}>a</p><p class=\"z y z\" class:z={on}>b</p><p class={c} class:z={on} class:y={!on}>c</p><p class=\"{c} w\" class:w={on}>d</p>";
  for (on, c) in [(true, "z"), (false, "z"), (false, "a z\tz y"), (true, ""), (false, "zz z🙂")] {
    check("class-directives", source, &props(&[("on", Value::Bool(on)), ("c", s(c))]));
  }
  check("class-directive-null", source, &props(&[("on", Value::Bool(false)), ("c", Value::Null)]));
}

#[test]
fn style_directives_apply_to_any_style_value() {
  let source = "<script>let { c, s } = $props();</script><p style=\"margin: {c}px; Color: blue\" style:color={c}>a</p><p style={s} style:color={c} style:--x=\"1\">b</p><p style=\"/* a\nb */ color: red; width: 1px /* c */\" style:width={c}>c</p>";
  let styles = ["", "color: red", "content: '🙂;'; COLOR: x; background: url(a;b)", "/* x */ color: red; /* y\n */ width: 2px", "a: b; c", "\u{feff} color: red \u{85}", "x: '/*'; color: y"];
  for s_value in styles {
    for c in [Value::Null, s("4"), s("")] {
      check("style-directives", source, &props(&[("c", c.clone()), ("s", s(s_value))]));
    }
  }
  check("style-directive-null", source, &props(&[("c", s("1")), ("s", Value::Null)]));
}

#[test]
fn class_and_style_text_collapses_whitespace_as_svelte_does() {
  let source = "<script>let { c } = $props();</script><p class=\"\n x\n\t y \" style=\" a:\n b \" title=\"t\n u\">a</p><p class=\"x\n {c}\n\" style=\"a:\n {c}\n\" style:width=\" {c}\n px\">b</p><p style=\"/* a\nb */ color: red\" style:width={c}>c</p>";
  check("collapse", source, &props(&[("c", s("1"))]));
}

#[test]
fn element_attributes_follow_svelte_server() {
  let o = |pairs: &[(&str, Value)]| Value::Map(props(pairs));
  let spreads = [
    o(&[]),
    o(&[("title", s("o")), ("Data-Up", s("k")), ("$$x", s("1")), ("a b", s("1")), ("hidden", Value::Bool(false)), ("disabled", s("")), ("data-x", Value::Null), ("style", s("")), ("class", s("")), ("onclick", s("x")), ("$y", s("<b>"))]),
    o(&[("class", Value::seq(vec![s("q"), s("z")])), ("style", s("color: blue; width: 1px")), ("value", s("v")), ("defaultValue", s("d")), ("translate", Value::Bool(false))]),
    o(&[("class", Value::Map(props(&[("m", Value::Bool(true))]))), ("viewBox", s("0 0 1 1")), ("checked", Value::Bool(true))]),
  ];
  let source = "<script>let { o, on, c } = $props(); const f = () => {};</script>\
    <p title=\"t\" {...o} class=\"c\">a</p>\
    <p class=\"a\" {...o} class:z={on} style:color={c} style:width|important=\"2px\">b</p>\
    <input {...o} defaultValue=\"d\" defaultChecked={on}>\
    <img {...o} alt=\"a\">\
    <img src=\"x.png\" alt=\"\" onload={f} onerror={f}>\
    <link rel=\"icon\" onload={f}>\
    <input disabled=\"false\" value=\"v\"><p style=\"\" class=\"\" key=\"k\" ref=\"r\" children=\"c\">c</p>\
    <textarea value=\"\nx\"></textarea><textarea defaultValue=\"x\"></textarea>\
    <p style:color|important={c} style=\"color: red\">d</p>\
    <my-el Foo=\"1\" {...o}></my-el><button is=\"my-b\" {...o}>e</button>\
    <svg><rect {...o}/></svg><p title=\"a&amp;b\" {...o}>f</p><p class=\"a&amp;b\" class:z={on} style=\"content: '&amp;'\" style:color={c}>g</p><p class={on}>h</p><p class={c}>i</p>";
  for spread in &spreads {
    for (on, c) in [(true, s("red")), (false, Value::Null)] {
      check("element-attributes", source, &props(&[("o", spread.clone()), ("on", Value::Bool(on)), ("c", c.clone())]));
    }
  }
}

#[test]
fn picture_answers_what_the_client_answers() {
  let source = "<script>import { picture } from '@snapfire/fsr-client/svelte'; let { p, n } = $props(); const image = picture({ src: '/pictures/probe.png', alt: 'probe', width: 4, height: 4 }); const hero = picture({ src: p, alt: n, priority: true, loading: 'auto', 'data-x': 1 });</script>\
    {#if image.sources}<picture>{#each image.sources as source, i (i)}<source {...source} />{/each}<img {...image.img} /></picture>{:else}<img {...image.img} />{/if}\
    <img {...hero.img}>{#if n}{@const low = picture({ src: n, alt: '' })}<img {...low.img}>{/if}";
  check("picture", source, &props(&[("p", s("/a.png")), ("n", s("b"))]));
}

#[test]
fn select_marks_the_options_svelte_marks() {
  let source = "<script>import C from './O.svelte'; let { v, vs, xs, o } = $props();</script>\
    <select value={v} class=\"s\"><option value=\"a\">A</option><option>b</option>{#each xs as x}<option value={x}>{x}</option>{/each}<optgroup label=\"g\"><option value=\"c\" selected>C</option></optgroup><option>{v}</option><option>t {v} &amp;</option><C /></select>\
    <select bind:value={vs} multiple><option value=\"a\">A</option><option value=\"b\" class=\"\">B</option></select>\
    <select class=\"p\"><option value=\"a\" class=\"\">A</option></select><option value=\"z\">z</option>\
    <select value={v}><option value={v}>{v} <b>r</b></option><hr></select><select {...o}><option {...o}>a</option><option value=\"b\">b</option></select>";
  let child = "<option value=\"b\">from a child</option>";
  for v in [s("a"), s("b"), s("c"), Value::Null] {
    for o in [Value::Map(props(&[("value", s("b")), ("Title", s("t"))])), Value::Map(props(&[("multiple", s("")), ("value", Value::seq(vec![s("a"), s("b")]))]))] {
      let given = props(&[("v", v.clone()), ("vs", Value::seq(vec![s("b")])), ("xs", Value::seq(vec![s("a"), s("b")])), ("o", o)]);
      let (rust, svelte) = both("select", &[("src/C.svelte", source), ("src/O.svelte", child)], &given, None);
      assert_eq!(rust, svelte, "{given:?}");
    }
  }
}

#[test]
fn textarea_content_and_raw_text_elements_write_as_svelte_does() {
  let source = "<script>let { x } = $props();</script><textarea>{x}</textarea><textarea>a {x} &lt;</textarea><textarea>\nfoo &amp;</textarea><textarea>\n{x}\n</textarea><div><style>p > a { color: red }</style><script>if (a < b && c) x();</script></div>";
  for x in [s("<i>&"), Value::Null, s("\nlead")] {
    check("textarea", source, &props(&[("x", x)]));
  }
}

#[test]
fn slots_and_passed_snippets_render_as_svelte_server_writes_them() {
  let legacy = "<script>export let t;</script><div><slot>fallback {t}</slot><slot name=\"head\" item={t}>no head</slot></div>{#if $$slots.head}<b>has</b>{/if}";
  let runes = "<script>let { row, children, n } = $props();</script>{@render row?.(n)}{#if children}{@render children()}{:else}none{/if}{#if row}<b>row</b>{/if}";
  let forward = "<script>import R from './R.svelte'; let { row } = $props();</script><R n={4} {row}/>";
  let parents = [
    ("legacy-slots", "<script>import L from './L.svelte';</script><L t=\"a\">kids<span slot=\"head\" let:item>{item}</span></L><L t=\"b\" />"),
    ("legacy-first", "<script>import L from './L.svelte';</script><L t=\"a\"><span slot=\"head\">h</span> kids</L>"),
    ("fragment-slot", "<script>import L from './L.svelte';</script><L t=\"c\"><svelte:fragment slot=\"head\" let:item>{item}!</svelte:fragment></L>"),
    ("runes-into-legacy", "<script>import L from './L.svelte';</script><L t=\"d\">{#snippet head(p)}<i>{p.item}</i>{/snippet}</L>"),
    ("snippets", "<script>import R from './R.svelte';</script><R n={1}>{#snippet row(v)}<i>{v}</i>{/snippet}kids</R><R n={3}/>"),
    ("attribute-snippet", "<script>import R from './R.svelte';</script><R n={2} row={r}/>{#snippet r(v)}{v}!{/snippet}"),
    ("forwarded", "<script>import F from './F.svelte';</script><F>{#snippet row(v)}<u>{v}</u>{/snippet}</F><F/>"),
  ];
  for (tag, parent) in parents {
    let (rust, svelte) = both(tag, &[("src/P.svelte", parent), ("src/L.svelte", legacy), ("src/R.svelte", runes), ("src/F.svelte", forward)], &props(&[]), None);
    assert_eq!(rust, svelte, "{tag}:\n{parent}");
  }
}

#[test]
fn components_and_elements_chosen_at_run_time_render_as_svelte_writes_them() {
  let runes = "<script>import A from './A.svelte'; import B from './B.svelte'; let { on, tag, n } = $props(); const C = $derived(on ? A : B);</script><C x={1}>kid</C><svelte:component this={on ? A : B} x={2}/><svelte:element this={tag} class=\"e\" {n}>in {n}</svelte:element><svelte:element this={tag}/>{#if n > 0}<svelte:self n={n - 1} tag=\"i\" on={!on}/>{/if}";
  let legacy = "<script>import A from './A.svelte'; export let on; $: C = on ? A : null;</script><svelte:component this={C} x={3}/><svelte:component this={C}>k</svelte:component>";
  let a = "<script>let { x, children } = $props();</script><b>A{x}</b>{@render children?.()}";
  let b = "<script>let { x } = $props();</script><i>B{x}</i>";
  for (on, tag, n) in [(true, s("div"), 1i64), (false, s("br"), 2), (true, Value::Null, 0), (false, s("textarea"), 1), (true, s("my-el"), 0)] {
    let given = props(&[("on", Value::Bool(on)), ("tag", tag.clone()), ("n", Value::int(n))]);
    let (rust, svelte) = both("dynamic", &[("src/C.svelte", runes), ("src/A.svelte", a), ("src/B.svelte", b)], &given, None);
    assert_eq!(rust, svelte, "{given:?}");
    let (rust, svelte) = both("dynamic-legacy", &[("src/L.svelte", legacy), ("src/A.svelte", a)], &given, None);
    assert_eq!(rust, svelte, "legacy {given:?}");
  }
}

#[test]
fn key_await_boundary_and_window_elements_render_as_svelte_writes_them() {
  let source = "<script>let { k, p } = $props();</script><svelte:window on:resize={() => {}}/><svelte:body onclick={() => {}}/><svelte:document />\
    {#key k}<b>{k}</b>{/key}{#key k}text {k}{/key}{#await p}wait{:then v}<i>{v}</i>{:catch e}err{/await}{#await p then v}{v}{/await}{#await p}w{/await}{#await p catch e}{e}{/await}\
    <svelte:boundary><p>in</p>{#snippet failed(e)}x{/snippet}</svelte:boundary><svelte:boundary>{k}<p>in</p>{#snippet pending()}wait{/snippet}</svelte:boundary><svelte:boundary>text first</svelte:boundary>";
  for (k, p) in [(s("a"), s("v")), (Value::Null, Value::int(2i64))] {
    check("blocks-13", source, &props(&[("k", k), ("p", p)]));
  }
}

#[test]
fn stores_css_props_and_component_bindings_render_as_svelte_writes_them() {
  let a = "<script>let { x = 0, value = $bindable(5) } = $props();</script><b>{x} {value}</b>";
  let parent = "<script>import A from './A.svelte'; import { writable, readable, derived, get } from 'svelte/store'; import { count } from './stores.js'; const w = writable(1); const r = readable('r'); const d = derived(w, (x) => x * 2); const both = derived([w, r], ([x, y]) => `${x}${y}`); let v = $state(3); let { c, n } = $props();</script>\
    <p>{$w} {$r} {$d} {$both} {$count} {get(w)}</p><A --color={c} --size=\"2px\" x={1}/><svg><A --color=\"red\"/></svg><A bind:value={v}/><A bind:value={() => n, (x) => {}}/><A />";
  let stores = "import { writable } from 'svelte/store';\nexport const count = writable(7);\n";
  let (rust, svelte) = both("stores", &[("src/C.svelte", parent), ("src/A.svelte", a), ("src/stores.js", stores)], &props(&[("c", s("blue")), ("n", Value::int(4i64))]), None);
  assert_eq!(rust, svelte);
}

#[test]
fn scoped_styles_add_the_hash_svelte_adds() {
  let source = "<script>let { on, c, o } = $props();</script>\
    <p class=\"a\" class:b={on} title=\"t\">x</p><p id=\"i\">bare</p><span>y</span><div class={c} {...o}></div><div class={on ? 'c' : null}></div>\
    <select value={c} class=\"s\"><option value=\"a\">A</option></select><ul>{#each [1, 2] as n}<li>{n}</li>{/each}</ul><em class:b={on}>e</em>\
    <style>p { color: red } .b { color: blue } div { margin: 0 } option { color: red } li:first-child { x: y } em { z: 1 } :global(span) { q: 1 }</style>";
  for (on, c) in [(true, s("a")), (false, Value::Null)] {
    check("scoped", source, &props(&[("on", Value::Bool(on)), ("c", c.clone()), ("o", Value::Map(props(&[("class", s("k"))])))]));
  }
}

#[test]
fn scripts_and_patterns_lower_as_svelte_runs_them() {
  let legacy = "<script>export let a = 1; export let o = { x: 1 }; let b = 0; let c; $: if (a > 0) b = a * 2; $: { c = b + 1; } $: ({ x: d } = o); $: e = d + c;</script><p>{b} {c} {d} {e} {$$props.a} {JSON.stringify($$restProps)}</p>{#each [1] as n}{@const y = z + 1}{@const z = n}{y}{/each}";
  let runes = "<script>let { items, p } = $props(); let s = $state({ q: 1 }); const snap = $state.snapshot(s);</script><p>{snap.q} {$effect.tracking()}</p>{#each items as { k, v: [first] }, i (k)}{@const { a = 2, b } = { b: k }}<i>{k}{first}{a}{b}{i}</i>{/each}{#snippet row({ x }, y = 3)}{x}{y}{/snippet}{@render row({ x: 1 })}{#await p then { w }}{w}{/await}";
  for a in [Value::int(2i64), Value::int(-1i64)] {
    check("legacy-script", legacy, &props(&[("a", a.clone()), ("o", Value::Map(props(&[("x", Value::int(5i64))]))), ("extra", s("x"))]));
  }
  let declared = "<script>let { n, obj } = $props(); const { q, r: [t] = [9] } = obj;</script><p>{q} {t}</p>{#if n}{const y = n * 2}{let z = y + 1, w}<p>{y} {z} {w}</p>{/if}";
  check("declared", declared, &props(&[("n", Value::int(3i64)), ("obj", Value::Map(props(&[("q", s("q"))])))]));
  let child = "<slot item={{ a: 1, b: [2] }} />";
  let parent = "<script>import L from './L.svelte';</script><L let:item={{ a, b: [c] }}>{a}{c}</L>";
  let (rust, svelte) = both("let-pattern", &[("src/P.svelte", parent), ("src/L.svelte", child)], &props(&[]), None);
  assert_eq!(rust, svelte);
  let items = Value::seq(vec![Value::Map(props(&[("k", s("a")), ("v", Value::seq(vec![s("f")]))]))]);
  check("runes-script", runes, &props(&[("items", items), ("p", Value::Map(props(&[("w", s("w"))])))]));
}

#[test]
fn props_id_is_refused_until_the_render_counts_in_order() {
  let refusal = refused("props-id", "<script>const id = $props.id();</script><p id={id}>x</p>");
  assert!(refusal.contains("$props.id()"), "{refusal}");
}

#[test]
fn script_functions_handlers_and_promises_render_as_svelte_runs_them() {
  let parent = "<script>import C from './C.svelte'; import { writable } from 'svelte/store'; let { item } = $props(); let count = 0; const total = writable(4); let seen = 0; total.subscribe((value) => { seen = value; }); function label(n) { const c = item.count + n; return `${c} ${c === 1 ? 'comment' : 'comments'}`; } function bump() { count += 1; alert(count); } async function later() { return 1; } $inspect(count).with(console.trace); let pending = $state(later()); let held = 0; const stop = total.subscribe((value) => { held = value + 1; });</script><p>{label(0)} {label(1)} {seen} {held}</p>{#await pending}<i>p</i>{:then v}{v}{/await}<C onclick={bump} increment={() => count += 1} />{#await later()}<i>waiting</i>{:then v}<b>{v}</b>{/await}{#await item.count}w{:then n}<b>{n}</b>{/await}";
  let child = "<script>let { onclick, increment } = $props();</script><button {onclick}>go</button><button onclick={increment}>up</button>";
  let (rust, svelte) = both("functions", &[("src/P.svelte", parent), ("src/C.svelte", child)], &props(&[("item", Value::Map(props(&[("count", Value::int(0i64))])))]), None);
  assert_eq!(rust, svelte);
}

#[test]
fn svelte_library_values_hold_what_the_server_reads() {
  let source = "<script>import { spring, tweened, Spring, Tween } from 'svelte/motion'; import { SvelteSet, SvelteMap } from 'svelte/reactivity'; import { crossfade } from 'svelte/transition'; import { createEventDispatcher } from 'svelte'; const dispatch = createEventDispatcher(); const size = spring(10, { stiffness: 0.1 }); const progress = tweened(0.5, { duration: 400 }); const coords = new Spring({ x: 1, y: 2 }); const t = new Tween(3, { duration: 10 }); const picked = new SvelteSet(['a', 'b']); const names = new SvelteMap([['k', 'v']]); const [send, receive] = crossfade({ duration: 200 });</script><p>{$size} {$progress} {coords.current.x} {coords.target.y} {t.current} {picked.has('a')} {picked.size} {names.get('k')}</p>";
  check("library-values", source, &props(&[]));
}

#[test]
fn number_and_string_builtins_render_what_javascript_computes() {
  let source = r#"<script>let { word, nums, hex, n, half } = $props();</script>
<p>{isNaN(n)} {isNaN(word)} {isNaN('12')} {isNaN(0 / 0)} {isNaN(null)} {isNaN(true)} {isNaN(' ')} {isNaN('0x1g')} {isNaN(half)}</p>
<p>{parseInt(hex.slice(1, 3), 16)} {parseInt('  -42px')} {parseInt('0x1f')} {parseInt('0x1f', 16)} {parseInt('0x1f', 10)} {parseInt('z', 36)} {parseInt('abc')} {parseInt('12.9e3')} {parseInt('101', 2)} {parseInt('9', 8)} {parseInt('', 10)} {parseInt('+7')} {parseInt('-0')} {parseInt('1e21')} {parseInt(half)} {parseInt('7', 1)} {parseInt('7', 37)} {parseInt('7', 0)} {parseInt('ffffffffffffffffffff', 16)} {parseInt(null)} {parseInt(n, 36)}</p>
<p>{word.lastIndexOf('l')} {word.lastIndexOf('l', 2)} {word.lastIndexOf('')} {word.lastIndexOf('', 2)} {word.lastIndexOf('z')} {word.lastIndexOf('lo', -5)} {nums.lastIndexOf(3)} {nums.lastIndexOf(3, -2)} {nums.lastIndexOf(9)} {nums.lastIndexOf(1, -99)}</p>
<p>{word.charCodeAt(0)} {word.charCodeAt(99)} {'😀'.charCodeAt(1)} {word.charCodeAt()} {word.charCodeAt(-1)} {word.charCodeAt(1.7)}</p>
<p>{(255).toString(16)} {(255).toString(2)} {(-255).toString(36)} {n.toString(16)} {'😀'.charCodeAt(0).toString(16)} {(0).toString(7)} {(1e21).toString(16)} {n.toString(10)}</p>
<p>{n * 1e20} {n * 1e19} {n / 1e8} {half * 1e-7} {half / 1e6} {-n * 1e30} {String(2 ** 70)} {1 / 3} {n * 1e300 * 1e10} {n * 0 - 121353887610955.125}</p>
<ul>{#each Array(3) as _, i}<li>{i}</li>{/each}</ul>
<ol>{#each Array(n % 4) as _, i}<li>{i}</li>{/each}</ol>"#;
  check(
    "builtins",
    source,
    &props(&[("word", s("hello")), ("nums", Value::Seq(vec![Value::int(1i64), Value::int(3i64), Value::int(5i64), Value::int(3i64)].into())), ("hex", s("#ff8000")), ("n", Value::int(42i64)), ("half", Value::F64(12.5))]),
  );
}

#[test]
fn patterns_built_at_run_time_and_code_points_render_what_javascript_computes() {
  let source = r#"<script>let { data, search, flags, word } = $props(); const re = new RegExp(search, flags); const plain = new RegExp(search);</script>
<p>{data.filter((d) => re.test(d)).join()} {data.filter((d) => plain.test(d)).join()} {new RegExp('a.c', 'i').test('ABC')} {new RegExp(search.length).test('x2')} {new Date(0).toISOString()} {[...new Set(data)].length}</p>
<p>{word.codePointAt(0)} {word.codePointAt(1)} {word.codePointAt(2)} [{word.codePointAt(9)}] [{word.codePointAt(-1)}] {word.codePointAt()} {word.codePointAt(1.5)}</p>"#;
  check("patterns", source, &props(&[("data", Value::seq(vec![s("Banana"), s("ANT"), s("cat")])), ("search", s("an")), ("flags", s("i")), ("word", s("a😀"))]));
}

/// What lowering `source` refuses, as `file:line:column: message`.
fn refused(tag: &str, source: &str) -> String {
  let dir = app(tag, &[("src/C.svelte", source)]);
  let mut set = ComponentSet::new(&dir);
  COMPILER.with(|compiler| match compiler.describe("src/C.svelte", source, &Options::default()).expect("the driver answers") {
    Outcome::Described(described) => set.describe("src/C.svelte", described),
    other => panic!("not described: {other:?}"),
  });
  let refusal = set.lower("src/C.svelte#default").expect_err("refused").to_string();
  let _ = std::fs::remove_dir_all(&dir);
  refusal
}

#[test]
fn a_store_that_sets_its_value_when_subscribed_is_refused() {
  let started = refused("started", "<script>import { readable } from 'svelte/store'; const now = readable(0, (set) => set(1));</script><p>{$now}</p>");
  assert!(started.contains("start function"), "{started}");
  let derived = refused("derived-set", "<script>import { writable, derived } from 'svelte/store'; const w = writable(1); const d = derived(w, (x, set) => set(x));</script><p>{$d}</p>");
  assert!(derived.contains("sets its value"), "{derived}");
}

#[test]
fn head_content_is_refused_toward_a_loaders_meta() {
  let refusal = refused("head", "<script>let { t } = $props();</script><svelte:head><title>{t}</title></svelte:head><p>body</p>");
  assert!(refusal.contains("<svelte:head>") && refusal.contains("`meta` export"), "{refusal}");
}

#[test]
fn bindings_write_what_svelte_server_writes() {
  let source = "<script>let { g, gs, h, t, v } = $props();</script>\
    <input type=\"radio\" value=\"a\" bind:group={g}><input type=\"radio\" value=\"b\" bind:group={g}>\
    <input type=\"checkbox\" value=\"a\" bind:group={gs}><input type=\"checkbox\" value=\"b\" bind:group={gs}>\
    <div contenteditable bind:innerHTML={h}></div><div contenteditable bind:textContent={t}></div>\
    <input type=\"file\" bind:value={v}><input bind:value={v}><textarea bind:value={v}></textarea>\
    <details bind:open={g}></details><div bind:clientWidth={v}></div>";
  check("bindings", source, &props(&[("g", s("a")), ("gs", Value::seq(vec![s("b")])), ("h", s("<b>h</b>")), ("t", s("<i>")), ("v", s("x"))]));
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

/// A component of an example application lowered where it lives, its imports
/// followed by the build, against Svelte's render with each import stubbed.
fn repo(app: &str, file: &str, stubbed: &[(&str, &str)], props: &ValueMap) -> (String, String) {
  repo_with(app, file, &[], stubbed, props, None)
}

/// `repo` for a component that renders `others`, the Svelte files it imports, with `children` for its island.
fn repo_with(app: &str, file: &str, others: &[&str], stubbed: &[(&str, &str)], props: &ValueMap, children: Option<&str>) -> (String, String) {
  let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples").join(app);
  let mut set = ComponentSet::new(&dir);
  let mut modules = stubs();
  for (specifier, js) in stubbed {
    modules.insert((*specifier).to_owned(), (*js).to_owned());
  }
  COMPILER.with(|compiler| {
    for each in std::iter::once(&file).chain(others) {
      let source = std::fs::read_to_string(dir.join(each)).unwrap();
      match compiler.describe(each, &source, &Options::default()).expect("the driver answers") {
        Outcome::Described(described) => set.describe(*each, described),
        other => panic!("{each}: not described: {other:?}"),
      }
      match compiler.ssr_module(each, &source).expect("the driver answers") {
        Outcome::Ok(compiled) => modules.insert(format!("/{each}"), compiled.js),
        other => panic!("{each}: no server module: {other:?}"),
      };
    }
  });
  let module = format!("{file}#default");
  if let Err(e) = set.lower(&module) {
    panic!("{file}: does not lower: {e}");
  }
  let library: Components = set.components.iter().map(|(m, c)| (m.clone(), Arc::new(snapfire_fsr_ir::render::prepare(c)))).collect();
  let interpreter = Interpreter::default().with_frameworks(Frameworks::default().svelte(Some(SvelteMajor::V5)));
  let child_tree = children.map(|c| vec![Tmpl::Text(c.to_owned())]).unwrap_or_default();
  let rendered = interpreter.render(&host(&module, child_tree), props, &library).expect("renders");
  let rust = rendered.islands[0].body.html.clone();
  let svelte = COMPILER.with(|compiler| compiler.render_module(&format!("/{file}"), &modules, &value_json(&Value::Map(props.clone())), children, &runtime())).expect("Svelte renders");
  (rust, svelte)
}

#[test]
fn the_examples_svelte_components_render_as_svelte_server_writes_them() {
  let store = ("@src/store", "export const doneCount = \"shelf/done\";\n");
  let visits = ("/src/ui/visits.svelte.js", "export const recent = { ids: [] };\nexport function visit() {}\n");
  let client = ("@generated/client", "export const actions = {};\n");
  let recent = ("/src/ui/recent.svelte.js", "export const recent = { ids: [] };\nexport function visit() {}\n");
  let tally = repo("shelf_svelte_ts/app", "src/ui/Tally.svelte", &[store], &props(&[("done", Value::int(2i64)), ("total", Value::int(6i64))]));
  assert_eq!(tally.0, tally.1, "Tally");
  for (read, done) in [(0i64, false), (120, false), (272, true)] {
    let given = props(&[("id", s("piranesi")), ("pages", Value::int(272i64)), ("read", Value::int(read)), ("done", Value::Bool(done)), ("current", Value::Bool(read > 0))]);
    let progress = repo("shelf_svelte_ts/app", "src/ui/Progress.svelte", &[store, client, recent, visits], &given);
    assert_eq!(progress.0, progress.1, "Progress at {read}");
  }
  let titles = Value::seq(vec![Value::Map(props(&[("id", s("stoner")), ("title", s("Stoner"))]))]);
  let shown = repo("shelf_svelte_ts/app", "src/ui/Recent.svelte", &[recent, visits], &props(&[("titles", titles)]));
  assert_eq!(shown.0, shown.1, "Recent");
  let holding = |symbol: &str, price: f64| Value::Map(props(&[("symbol", s(symbol)), ("name", s("Name & co")), ("shares", Value::int(3i64)), ("price", Value::F64(price)), ("change", Value::F64(0.5))]));
  let holdings = Value::seq(vec![holding("ACME", 12.5), holding("INIT", 3.0)]);
  let row = |id: &str, title: &str, pages: i64, read: i64, done: bool| Value::Map(props(&[("id", s(id)), ("title", s(title)), ("author", s("A & B")), ("pages", Value::int(pages)), ("read", Value::int(read)), ("done", Value::Bool(done))]));
  for rows in [Value::seq(vec![row("stoner", "Stoner", 278, 120, false), row("gilead", "Gilead", 247, 247, true)]), Value::seq(Vec::new())] {
    let others = ["src/ui/Table.svelte", "src/ui/Legend.svelte", "src/ui/Bars.svelte", "src/ui/Dots.svelte"];
    let stats = repo_with("shelf_svelte_ts/app", "src/ui/Stats.svelte", &others, &[], &props(&[("rows", rows.clone())]), Some("hint"));
    assert_eq!(stats.0, stats.1, "Stats");
  }
  for watched in ["ACME", "INIT", "NONE"] {
    let focus = repo("uni/js", "src/ui/Focus.svelte", &[], &props(&[("holdings", holdings.clone()), ("watched", s(watched))]));
    assert_eq!(focus.0, focus.1, "Focus watching {watched}");
  }
}

/// A generated template. Indexes pick a text, an expression, an attribute or a
/// condition, which are printed against the names in scope at that point.
#[derive(Debug, Clone)]
enum Node {
  Text(usize),
  Expr(usize),
  Html,
  Select(usize),
  Void(usize),
  Render(usize),
  Elem(usize, Vec<usize>, Vec<Node>),
  If(usize, Vec<Node>, Option<Vec<Node>>, bool),
  Each(Vec<Node>, Option<Vec<Node>>, bool),
}

const TEXTS: &[&str] = &["hi", " ", "\n  ", "a & b", "&amp;", "&lt;x&gt;", "x y", "é", "&nbsp;"];
const TAGS: &[&str] = &["div", "section", "span", "em", "b", "article"];

#[derive(Clone)]
struct Scope {
  strs: Vec<String>,
  nums: Vec<String>,
  depth: usize,
}

struct Printer {
  out: String,
  row: bool,
}

impl Scope {
  fn s(&self, k: usize) -> &str {
    &self.strs[k / 13 % self.strs.len()]
  }

  fn n(&self, k: usize) -> &str {
    &self.nums[k / 7 % self.nums.len()]
  }

  fn expr(&self, k: usize) -> String {
    let (s, n) = (self.s(k), self.n(k));
    match k % 11 {
      0 => s.to_owned(),
      1 => n.to_owned(),
      2 => format!("{n} * 2"),
      3 => format!("{n} / 4"),
      4 => "on".to_owned(),
      5 => format!("{s}.length"),
      6 => format!("{s} + {n}"),
      7 => format!("on ? {s} : 'no'"),
      8 => format!("`${{{s}}}-${{{n}}}`"),
      9 => format!("{n} % 3"),
      _ => "xs.length".to_owned(),
    }
  }

  fn cond(&self, k: usize) -> String {
    let (s, n) = (self.s(k), self.n(k));
    match k % 6 {
      0 => "on".to_owned(),
      1 => "!on".to_owned(),
      2 => format!("{n} > 2"),
      3 => format!("{s}.length > 1"),
      4 => "xs.length".to_owned(),
      _ => format!("{s} === 'x'"),
    }
  }

  fn attr(&self, k: usize) -> (&'static str, String) {
    let (s, n) = (self.s(k), self.n(k));
    match k % 17 {
      16 => ("{...sp}", "{...sp}".to_owned()),
      11 => ("class", format!("class=\"x\n {{{s}}} \"")),
      12 => ("class", format!("class={{{s}}}")),
      13 => ("style", format!("style={{{s}}}")),
      14 => ("style", format!("style=\"/* a\n b */ color: {{{s}}};\n width: 1px /* c */\"")),
      15 => ("style:width", format!("style:width=\" {{{n}}}\n px\"")),
      0 => ("title", format!("title={{{s}}}")),
      1 => ("title", format!("title=\"t {{{n}}}\"")),
      2 => ("class", "class=\"c\"".to_owned()),
      3 => ("class", "class={{ k: on }}".to_owned()),
      4 => ("class:z", "class:z={on}".to_owned()),
      5 => ("hidden", "hidden={on}".to_owned()),
      6 => ("data-x", format!("data-x={{{n}}}")),
      7 => ("style:color", format!("style:color={{{s}}}")),
      8 => ("aria-pressed", "aria-pressed={on}".to_owned()),
      9 => ("id", format!("id=\"i{{{n}}}\"")),
      _ => ("style", format!("style=\"margin: {{{n}}}px\"")),
    }
  }
}

impl Printer {
  fn nodes(&mut self, nodes: &[Node], scope: &Scope) {
    for node in nodes {
      self.node(node, scope);
    }
  }

  fn node(&mut self, node: &Node, scope: &Scope) {
    match node {
      Node::Text(i) => self.out.push_str(TEXTS[*i]),
      Node::Expr(k) => {
        let e = scope.expr(*k);
        self.out.push_str(&format!("{{{e}}}"));
      }
      Node::Html => self.out.push_str("{@html h}"),
      Node::Select(k) => {
        let s = scope.s(*k);
        let open = match k % 3 {
          0 => format!("<select value={{{s}}}>"),
          1 => "<select bind:value={xs} multiple>".to_owned(),
          _ => "<select class=\"c\">".to_owned(),
        };
        self.out.push_str(&format!("{open}<option value=\"x\">X</option>{{#each xs as o}}<option>{{o}}</option>{{/each}}<option value={{{s}}}>v</option><option>x y</option></select>"));
      }
      Node::Void(k) => {
        let s = scope.s(*k);
        self.out.push_str(&match k % 5 {
          0 => "<br>".to_owned(),
          1 => "<hr>".to_owned(),
          2 => format!("<input value={{{s}}}>"),
          3 => format!("<img alt={{{s}}} src=\"x.png\">"),
          _ => "<input type=\"checkbox\" checked={on}>".to_owned(),
        });
      }
      Node::Render(k) => {
        self.row = true;
        let s = scope.s(*k);
        self.out.push_str(&format!("{{@render row({s})}}"));
      }
      Node::Elem(t, attrs, kids) => {
        let tag = TAGS[*t];
        self.out.push('<');
        self.out.push_str(tag);
        let mut names = Vec::new();
        for k in attrs {
          let (name, text) = scope.attr(*k);
          if !names.contains(&name) {
            names.push(name);
            self.out.push(' ');
            self.out.push_str(&text);
          }
        }
        self.out.push('>');
        self.nodes(kids, scope);
        self.out.push_str(&format!("</{tag}>"));
      }
      Node::If(k, then, otherwise, chain) => {
        self.out.push_str(&format!("{{#if {}}}", scope.cond(*k)));
        self.nodes(then, scope);
        self.rest(otherwise.as_deref(), *chain, scope);
        self.out.push_str("{/if}");
      }
      Node::Each(body, otherwise, constant) => {
        let d = scope.depth;
        let mut inner = scope.clone();
        inner.depth += 1;
        inner.strs.push(format!("x{d}"));
        inner.nums.push(format!("i{d}"));
        self.out.push_str(&format!("{{#each xs as x{d}, i{d}}}"));
        if *constant {
          self.out.push_str(&format!("{{@const y{d} = x{d} + i{d}}}"));
          inner.strs.push(format!("y{d}"));
        }
        self.nodes(body, &inner);
        if let Some(otherwise) = otherwise {
          self.out.push_str("{:else}");
          self.nodes(otherwise, scope);
        }
        self.out.push_str("{/each}");
      }
    }
  }

  /// An else holding a lone `if` is printed as `{:else if}` when `chain` is set.
  fn rest(&mut self, otherwise: Option<&[Node]>, chain: bool, scope: &Scope) {
    match otherwise {
      None => {}
      Some([Node::If(k, then, more, next)]) if chain => {
        self.out.push_str(&format!("{{:else if {}}}", scope.cond(*k)));
        self.nodes(then, scope);
        self.rest(more.as_deref(), *next, scope);
      }
      Some(nodes) => {
        self.out.push_str("{:else}");
        self.nodes(nodes, scope);
      }
    }
  }
}

fn source(nodes: &[Node], legacy: bool, styled: bool) -> String {
  let scope = Scope { strs: vec!["a".to_owned()], nums: vec!["n".to_owned()], depth: 0 };
  let mut printer = Printer { out: String::new(), row: false };
  printer.nodes(nodes, &scope);
  let script = if legacy { "<script>export let a; export let n; export let on; export let xs; export let h; export let sp;</script>\n" } else { "<script>let { a, n, on, xs, h, sp } = $props();</script>\n" };
  let row = if printer.row { "{#snippet row(v)}<i>{v}</i>{/snippet}\n" } else { "" };
  let style = if styled { "\n<style>div { a: b } .c { a: b } span em { a: b } section > b { x: y } .z { q: r } input { i: j }</style>" } else { "" };
  format!("{script}{row}{}{style}", printer.out)
}

mod generated {
  use proptest::prelude::*;

  use super::*;

  fn node() -> impl Strategy<Value = Node> {
    let leaf = prop_oneof![
      (0..TEXTS.len()).prop_map(Node::Text),
      (0..1000usize).prop_map(Node::Expr),
      Just(Node::Html),
      (0..1000usize).prop_map(Node::Select),
      (0..1000usize).prop_map(Node::Void),
      (0..1000usize).prop_map(Node::Render),
    ];
    leaf.prop_recursive(4, 32, 4, |inner| {
      let kids = prop::collection::vec(inner, 0..4);
      prop_oneof![
        (0..TAGS.len(), prop::collection::vec(0..1000usize, 0..3), kids.clone()).prop_map(|(t, a, c)| Node::Elem(t, a, c)),
        (0..1000usize, kids.clone(), prop::option::of(kids.clone()), any::<bool>()).prop_map(|(k, t, e, c)| Node::If(k, t, e, c)),
        (kids.clone(), prop::option::of(kids), any::<bool>()).prop_map(|(b, e, c)| Node::Each(b, e, c)),
      ]
    })
  }

  fn word() -> impl Strategy<Value = Value> {
    prop::sample::select(vec!["", "x", "<b>", "a\"b", "&amp;", "Zoë", "x y", "z", "a: b; color: x", "🙂 z\tk", "'/*' ; x: (;)"]).prop_map(Value::str)
  }

  fn given() -> impl Strategy<Value = ValueMap> {
    let html = prop::sample::select(vec!["", "<b>x</b>", "a &amp; b"]).prop_map(Value::str);
    let key = prop::sample::select(vec!["title", "class", "style", "hidden", "Data-Up", "$$x", "a b", "onclick", "value", "defaultValue", "translate", "id", "disabled"]);
    let spread = prop::collection::vec((key, prop_oneof![word(), any::<bool>().prop_map(Value::Bool), Just(Value::Null), (-2i64..3).prop_map(Value::int)]), 0..4).prop_map(|pairs| Value::Map(pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect()));
    (word(), -3i64..12, any::<bool>(), prop::collection::vec(word(), 0..3), html, spread).prop_map(|(a, n, on, xs, h, sp)| props(&[("a", a), ("n", Value::int(n)), ("on", Value::Bool(on)), ("xs", Value::seq(xs)), ("h", h), ("sp", sp)]))
  }

  proptest! {
    #![proptest_config(ProptestConfig { cases: 512, ..ProptestConfig::default() })]

    #[test]
    fn generated_templates_render_as_svelte_server_writes_them(nodes in prop::collection::vec(node(), 1..4), given in given(), legacy in any::<bool>(), styled in any::<bool>()) {
      let source = source(&nodes, legacy, styled);
      let (rust, svelte) = both("generated", &[("src/C.svelte", &source)], &given, None);
      prop_assert_eq!(rust, svelte, "{}\n{:?}", source, given);
    }
  }

  const NUMBERS: &str = "<script>let { rows } = $props();</script>{#each rows as r}<p>{parseInt(r.s, r.radix)}|{parseInt(r.s)}|{isNaN(r.s)}|{r.n}|{String(r.n)}|{r.i.toString(r.base)}|{r.s.lastIndexOf(r.t, r.at)}|{r.s.lastIndexOf(r.t)}|{r.s.charCodeAt(r.at)}</p>{/each}";

  fn row() -> impl Strategy<Value = Value> {
    let text = "[ \t\u{a0}\u{85}\u{feff}\u{2028}+-]{0,2}(0[xX])?[0-9a-zA-Z._🙂]{0,10}".prop_map(Value::str);
    let radix = prop_oneof![(-2i64..40).prop_map(Value::int), Just(Value::Null)];
    let finite = any::<f64>().prop_filter("finite", |f| f.is_finite());
    let number = prop_oneof![finite.clone(), (-1_000_000i64..1_000_000).prop_map(|n| n as f64 / 1000.0), (-30i32..30).prop_map(|e| 10f64.powi(e))];
    let integer = prop_oneof![(-(1i64 << 53)..=(1i64 << 53)).prop_map(|n| n as f64), finite.prop_map(f64::trunc)];
    let search = "[ab🙂]{0,2}".prop_map(Value::str);
    (text, radix, number, integer, 2i64..=36, search, -3i64..14).prop_map(|(s, radix, n, i, base, t, at)| {
      let base = if i.abs() > (1i64 << 53) as f64 { 1 << (base % 5 + 1) } else { base };
      Value::Map(props(&[("s", s), ("radix", radix), ("n", Value::F64(n)), ("i", Value::F64(i)), ("base", Value::int(base)), ("t", t), ("at", Value::int(at))]))
    })
  }

  proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    #[test]
    fn number_parsing_and_printing_render_as_svelte_server_writes_them(rows in prop::collection::vec(row(), 1..12)) {
      let given = props(&[("rows", Value::seq(rows))]);
      let (rust, svelte) = both("numbers", &[("src/C.svelte", NUMBERS)], &given, None);
      prop_assert_eq!(rust, svelte, "{:?}", given);
    }
  }
}
