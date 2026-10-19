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
    ("@snapfire/fsr-client/svelte".to_owned(), "export function useStore(key, initial) { return { subscribe(run) { run(initial); return () => {}; } }; }\nexport function useLocale() { return { subscribe(run) { run('en'); return () => {}; } }; }\nexport function link(options) { return { href: options.href }; }\nexport function island() { return () => () => {}; }\n".to_owned()),
    ("@snapfire/fsr-client".to_owned(), "export function get() { return 0; }\nexport async function optimistic() {}\nexport function key(name) { return name; }\n".to_owned()),
    ("@snapfire/fsr-client/store".to_owned(), "export function key(name) { return name; }\n".to_owned()),
    ("svelte".to_owned(), "export function onMount() {}\nexport function onDestroy() {}\nexport function tick() { return Promise.resolve(); }\n".to_owned()),
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
  let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples").join(app);
  let source = std::fs::read_to_string(dir.join(file)).unwrap();
  let mut set = ComponentSet::new(&dir);
  let mut modules = stubs();
  for (specifier, js) in stubbed {
    modules.insert((*specifier).to_owned(), (*js).to_owned());
  }
  COMPILER.with(|compiler| {
    match compiler.describe(file, &source, &Options::default()).expect("the driver answers") {
      Outcome::Described(described) => set.describe(file, described),
      other => panic!("{file}: not described: {other:?}"),
    }
    match compiler.ssr_module(file, &source).expect("the driver answers") {
      Outcome::Ok(compiled) => modules.insert(format!("/{file}"), compiled.js),
      other => panic!("{file}: no server module: {other:?}"),
    };
  });
  let module = format!("{file}#default");
  if let Err(e) = set.lower(&module) {
    panic!("{file}: does not lower: {e}");
  }
  let library: Components = set.components.iter().map(|(m, c)| (m.clone(), Arc::new(snapfire_fsr_ir::render::prepare(c)))).collect();
  let interpreter = Interpreter::default().with_frameworks(Frameworks::default().svelte(Some(SvelteMajor::V5)));
  let rendered = interpreter.render(&host(&module, Vec::new()), props, &library).expect("renders");
  let rust = rendered.islands[0].body.html.clone();
  let svelte = COMPILER.with(|compiler| compiler.render_module(&format!("/{file}"), &modules, &value_json(&Value::Map(props.clone())), None, &runtime())).expect("Svelte renders");
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
    match k % 16 {
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
      Node::Html => self.out.push_str("<div>{@html h}</div>"),
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

fn source(nodes: &[Node], legacy: bool) -> String {
  let scope = Scope { strs: vec!["a".to_owned()], nums: vec!["n".to_owned()], depth: 0 };
  let mut printer = Printer { out: String::new(), row: false };
  printer.nodes(nodes, &scope);
  let script = if legacy { "<script>export let a; export let n; export let on; export let xs; export let h;</script>\n" } else { "<script>let { a, n, on, xs, h } = $props();</script>\n" };
  let row = if printer.row { "{#snippet row(v)}<i>{v}</i>{/snippet}\n" } else { "" };
  format!("{script}{row}{}", printer.out)
}

mod generated {
  use proptest::prelude::*;

  use super::*;

  fn node() -> impl Strategy<Value = Node> {
    let leaf = prop_oneof![
      (0..TEXTS.len()).prop_map(Node::Text),
      (0..1000usize).prop_map(Node::Expr),
      Just(Node::Html),
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
    (word(), -3i64..12, any::<bool>(), prop::collection::vec(word(), 0..3), html).prop_map(|(a, n, on, xs, h)| props(&[("a", a), ("n", Value::int(n)), ("on", Value::Bool(on)), ("xs", Value::seq(xs)), ("h", h)]))
  }

  proptest! {
    #![proptest_config(ProptestConfig { cases: 512, ..ProptestConfig::default() })]

    #[test]
    fn generated_templates_render_as_svelte_server_writes_them(nodes in prop::collection::vec(node(), 1..4), given in given(), legacy in any::<bool>()) {
      let source = source(&nodes, legacy);
      let (rust, svelte) = both("generated", &[("src/C.svelte", &source)], &given, None);
      prop_assert_eq!(rust, svelte, "{}\n{:?}", source, given);
    }
  }
}
