use super::*;
use crate::ast::{Entry, Expr, Lit, Tmpl};

fn props(entries: &[(&str, Value)]) -> ValueMap {
  entries.iter().map(|(k, v)| ((*k).to_owned(), v.clone())).collect()
}

fn p(name: &str) -> Expr {
  Expr::var("$props").field(name)
}

const BY_REACT: crate::ast::Owner = crate::ast::Owner::React;

fn element(tag: &str, attrs: Vec<(&str, Expr)>, children: Vec<Tmpl>) -> Tmpl {
  Tmpl::Element { tag: tag.to_owned(), attrs: attrs.into_iter().map(|(name, expr)| Entry::Field(name.to_owned(), expr)).collect(), children }
}

fn render_under(react: Option<ReactMajor>, owner: crate::ast::Owner, render: Tmpl, props: &ValueMap, library: &Components) -> Result<String, Fail> {
  let component = Component { body: Vec::new(), render, state: Vec::new(), stores: Vec::new(), handlers: Vec::new(), owner, shadow: None, scope: None };
  Interpreter::default().with_frameworks(Frameworks { react, vue: None, svelte: None }).render(&component, props, library).map(|rendered| rendered.html)
}

fn render_svelte(render: Tmpl, props: &ValueMap) -> String {
  let component = Component { body: Vec::new(), render, state: Vec::new(), stores: Vec::new(), handlers: Vec::new(), owner: crate::ast::Owner::Svelte, shadow: None, scope: None };
  Interpreter::default().with_frameworks(Frameworks::default().svelte(Some(SvelteMajor::V5))).render(&component, props, &Components::new()).unwrap().html
}

/// Each expected string is what `svelte/server` 5.57.2 wrote for the same template.
#[test]
fn a_svelte_component_prints_text_and_attributes_as_svelte_server_does() {
  let values = props(&[("title", Value::str("Hi & bye")), ("t", Value::str("a\"b&c<d")), ("on", Value::Bool(true)), ("off", Value::Bool(false)), ("none", Value::Null)]);
  let heading = element("h1", vec![("class", Expr::lit_str("t"))], vec![Tmpl::Expr(p("title"))]);
  assert_eq!(render_svelte(heading, &values), "<!--[--><h1 class=\"t\">Hi &amp; bye</h1><!--]-->");
  let input = element("input", vec![("disabled", p("on")), ("hidden", p("off")), ("value", Expr::lit_str("v"))], Vec::new());
  assert_eq!(render_svelte(input, &values), "<!--[--><input disabled=\"\" value=\"v\"/><!--]-->");
  let escaped = element("p", vec![("title", p("t")), ("lang", p("none"))], vec![Tmpl::Expr(Expr::lit_str("<i>"))]);
  assert_eq!(render_svelte(escaped, &values), "<!--[--><p title=\"a&quot;b&amp;c&lt;d\">&lt;i></p><!--]-->");
  let shown = Tmpl::Fragment(vec![Tmpl::Expr(p("on")), Tmpl::Text(" ".to_owned()), Tmpl::Expr(p("none")), Tmpl::Expr(Expr::Array(vec![Entry::Item(Expr::Lit(Lit::Int(1i64.into()))), Entry::Item(Expr::Lit(Lit::Null)), Entry::Item(Expr::lit_str("a"))]))]);
  assert_eq!(render_svelte(shown, &values), "<!--[-->true 1,,a<!--]-->");
}

#[test]
fn a_marker_is_written_as_given_and_the_svelte_root_is_wrapped_once() {
  let values = props(&[("on", Value::Bool(true))]);
  let branch = Tmpl::Fragment(vec![Tmpl::If { cond: p("on"), then: Box::new(Tmpl::Fragment(vec![Tmpl::Marker("<!--[0-->".to_owned()), element("b", Vec::new(), vec![Tmpl::Text("y".to_owned())])])), r#else: Some(Box::new(Tmpl::Marker("<!--[-1-->".to_owned()))) }, Tmpl::Marker("<!--]-->".to_owned())]);
  assert_eq!(render_svelte(branch, &values), "<!--[--><!--[0--><b>y</b><!--]--><!--]-->");
}

#[test]
fn a_custom_element_attribute_is_written_the_way_the_react_hydrating_it_writes_it() {
  let tree = element("x-el", vec![("rows", p("rows")), ("obj", p("obj")), ("on", p("on")), ("off", p("off")), ("className", Expr::lit_str("c"))], Vec::new());
  let values = props(&[("rows", Value::seq(vec![Value::int(1i64), Value::Null, Value::int(2i64)])), ("obj", Value::Map(props(&[("a", Value::int(1i64))]))), ("on", Value::Bool(true)), ("off", Value::Bool(false))]);
  let html = |react| render_under(react, BY_REACT, tree.clone(), &values, &Components::new()).unwrap();
  assert_eq!(html(Some(ReactMajor::V18)), "<x-el rows=\"1,,2\" obj=\"[object Object]\" on=\"true\" off=\"false\" class=\"c\"></x-el>", "React 18 coerces every prop the way '' + value does");
  assert_eq!(html(Some(ReactMajor::V19)), "<x-el on=\"\" class=\"c\"></x-el>", "React 19 leaves an object to the client and writes true as present");
}

#[test]
fn a_non_scalar_on_a_custom_element_nothing_hydrates_is_refused_by_name() {
  let tree = element("x-grid", vec![("rows", p("rows")), ("count", p("count"))], Vec::new());
  let values = props(&[("rows", Value::seq(vec![Value::int(1i64)])), ("count", Value::int(3i64))]);
  let fail = render_under(Some(ReactMajor::V19), crate::ast::Owner::Fsr, tree.clone(), &values, &Components::new()).unwrap_err();
  assert!(fail.message.contains("`rows` on `<x-grid>`") && fail.message.contains("`array`"), "{}", fail.message);
  let scalar = props(&[("rows", Value::str("1")), ("count", Value::int(3i64))]);
  assert_eq!(render_under(None, BY_REACT, tree, &scalar, &Components::new()).unwrap(), "<x-grid rows=\"1\" count=\"3\"></x-grid>", "a React component with no React vendored is plain markup, which takes a scalar");
}

#[test]
fn false_on_an_aria_data_or_booleanish_attribute_is_written_as_react_dom_writes_it() {
  let f = || Expr::Lit(Lit::Bool(false));
  let tree = element(
    "button",
    vec![("aria-expanded", f()), ("data-open", f()), ("spellCheck", f()), ("draggable", f()), ("contentEditable", f()), ("hidden", f()), ("title", f())],
    Vec::new(),
  );
  for major in [ReactMajor::V18, ReactMajor::V19] {
    assert_eq!(
      render_under(Some(major), BY_REACT, tree.clone(), &ValueMap::default(), &Components::new()).unwrap(),
      "<button aria-expanded=\"false\" data-open=\"false\" spellcheck=\"false\" draggable=\"false\" contentEditable=\"false\"></button>",
      "{major:?}"
    );
  }
}

#[test]
fn a_javascript_url_is_written_the_way_react_19_blocks_it() {
  let blocked = format!("\"{BLOCKED_JAVASCRIPT_URL}\"");
  let tree = Tmpl::Fragment(vec![
    element("a", vec![("href", p("a"))], Vec::new()),
    element("img", vec![("src", p("b"))], Vec::new()),
    element("form", vec![("action", p("c"))], Vec::new()),
    element("a", vec![("href", p("safe"))], Vec::new()),
    element("a", vec![("title", p("a"))], Vec::new()),
  ]);
  let values = props(&[
    ("a", Value::str("javascript:alert(1)")),
    ("b", Value::str(" \u{1}JaVa\tScRi\npt:alert(1)")),
    ("c", Value::str("JAVASCRIPT:x")),
    ("safe", Value::str("/javascript:ok")),
  ]);
  for react in [None, Some(ReactMajor::V18), Some(ReactMajor::V19)] {
    let html = render_under(react, BY_REACT, tree.clone(), &values, &Components::new()).unwrap();
    assert_eq!(html.matches(blocked.as_str()).count(), 3, "{react:?}: {html}");
    assert!(html.contains("href=\"/javascript:ok\""), "{react:?}: {html}");
    assert!(html.contains("title=\"javascript:alert(1)\""), "{react:?}: a non-URL attribute is left alone: {html}");
  }
}

#[test]
fn an_outlet_marks_where_its_place_is_and_fails_in_markup_a_framework_hydrates_unless_the_lowerer_put_it_there() {
  let place = |hydrated| element("div", vec![("id", Expr::lit_str("modals"))], vec![Tmpl::Outlet { name: "modals".to_owned(), hydrated }]);
  let html = render_under(None, crate::ast::Owner::Fsr, place(false), &ValueMap::default(), &Components::new()).unwrap();
  assert_eq!(html, format!("<div id=\"modals\">{OUTLET_MARK}modals\u{0}</div>"));
  let rendered = Rendered { html, islands: Vec::new(), places: Vec::new(), hoisted: ValueMap::default(), whole: true };
  assert_eq!(crate::bind::rendered_nodes(&rendered), vec![snapfire_fsr_core::Node::raw("<div id=\"modals\">"), snapfire_fsr_core::Node::Outlet("modals".to_owned()), snapfire_fsr_core::Node::raw("</div>")]);
  let err = render_under(Some(ReactMajor::V19), BY_REACT, place(false), &ValueMap::default(), &Components::new()).unwrap_err();
  assert!(format!("{err:?}").contains("the outlet for `#modals` is in markup the browser hydrates"), "{err:?}");
  assert!(render_under(Some(ReactMajor::V19), BY_REACT, place(true), &ValueMap::default(), &Components::new()).is_ok());
}

#[test]
fn inert_and_an_empty_url_follow_the_major() {
  let tree = Tmpl::Fragment(vec![
    element("div", vec![("inert", Expr::Lit(Lit::Bool(true)))], Vec::new()),
    element("img", vec![("src", Expr::lit_str(""))], Vec::new()),
    element("a", vec![("href", Expr::lit_str(""))], Vec::new()),
  ]);
  let html = |react| render_under(react, BY_REACT, tree.clone(), &ValueMap::default(), &Components::new()).unwrap();
  assert_eq!(html(Some(ReactMajor::V18)), "<div></div><img src=\"\"/><a href=\"\"></a>");
  assert_eq!(html(Some(ReactMajor::V19)), "<div inert=\"\"></div><img/><a href=\"\"></a>");
  assert_eq!(html(None), "<div inert=\"true\"></div><img src=\"\"><a href=\"\"></a>");
}

#[test]
fn react_19_refuses_a_head_tag_it_would_move_out_of_hydrated_markup() {
  let title = element("title", Vec::new(), vec![Tmpl::Text("Tools".to_owned())]);
  let fail = render_under(Some(ReactMajor::V19), BY_REACT, title.clone(), &ValueMap::default(), &Components::new()).unwrap_err();
  assert!(fail.message.contains("`<title>`") && fail.message.contains("`meta` export"), "{}", fail.message);
  assert_eq!(render_under(Some(ReactMajor::V18), BY_REACT, title.clone(), &ValueMap::default(), &Components::new()).unwrap(), "<title>Tools</title>");
  assert_eq!(render_under(Some(ReactMajor::V19), crate::ast::Owner::Fsr, title.clone(), &ValueMap::default(), &Components::new()).unwrap(), "<title>Tools</title>", "markup nothing hydrates keeps it in place");
  let in_svg = element("svg", Vec::new(), vec![title]);
  assert_eq!(render_under(Some(ReactMajor::V19), BY_REACT, in_svg, &ValueMap::default(), &Components::new()).unwrap(), "<svg><title>Tools</title></svg>", "an SVG title is the drawing's own");
}

#[test]
fn a_bake_keeps_only_an_open_tag_every_markup_prints_alike() {
  let component = Component::new(crate::ast::Owner::React, Vec::new(), Tmpl::Fragment(vec![
    element("p", vec![("class", Expr::lit_str("a"))], Vec::new()),
    element("x-el", vec![("flag", Expr::Lit(Lit::Bool(true)))], Vec::new()),
    element("title", Vec::new(), vec![Tmpl::Text("t".to_owned())]),
  ]));
  let Tmpl::Fragment(children) = prepare(&component).render else { panic!("a fragment stays a fragment") };
  assert!(matches!(&children[0], Tmpl::Baked { open, .. } if open == "<p class=\"a\">"), "{:?}", children[0]);
  assert!(matches!(children[1], Tmpl::Element { .. }), "React 18 writes flag=\"true\" and 19 flag=\"\": {:?}", children[1]);
  assert!(matches!(children[2], Tmpl::Element { .. }), "a head tag React 19 may move stays where a render sees it: {:?}", children[2]);
}

#[test]
fn an_element_template_renders_as_the_elements_declarative_shadow_root() {
  let mut library = Components::new();
  library.insert(
    "elements/x-grid.tsx#default".to_owned(),
    Arc::new(Component { body: Vec::new(), render: Tmpl::Fragment(vec![element("b", Vec::new(), vec![Tmpl::Expr(p("title"))]), Tmpl::Expr(Expr::Length(Box::new(p("rows"))))]), state: Vec::new(), stores: Vec::new(), handlers: Vec::new(), owner: crate::ast::Owner::Fsr, shadow: None, scope: None }),
  );
  let page = element("x-grid", vec![(SHADOW_ATTR, Expr::lit_str("elements/x-grid.tsx#default")), ("title", Expr::lit_str("t")), ("rows", p("rows"))], vec![Tmpl::Text("light".to_owned())]);
  let values = props(&[("rows", Value::seq(vec![Value::int(1i64), Value::int(2i64)]))]);
  for react in [None, Some(ReactMajor::V18), Some(ReactMajor::V19)] {
    assert_eq!(render_under(react, BY_REACT, page.clone(), &values, &library).unwrap(), "<x-grid title=\"t\"><template shadowrootmode=\"open\"><b>t</b>2</template>light</x-grid>", "under {react:?} the array reaches the template and never the host");
  }
}

#[test]
fn an_element_template_writes_the_shadow_root_it_declares() {
  let closed = ShadowRoot { mode: ShadowMode::Closed, delegates_focus: true, clonable: true, serializable: true };
  for (shadow, open) in [
    (closed, "<template shadowrootmode=\"closed\" shadowrootdelegatesfocus shadowrootclonable shadowrootserializable>"),
    (ShadowRoot::default(), "<template shadowrootmode=\"open\">"),
  ] {
    let mut library = Components::new();
    library.insert(
      "elements/x-box.tsx#default".to_owned(),
      Arc::new(Component { body: Vec::new(), render: Tmpl::Text("in".to_owned()), state: Vec::new(), stores: Vec::new(), handlers: Vec::new(), owner: crate::ast::Owner::Fsr, shadow: Some(shadow), scope: None }),
    );
    let page = element("x-box", vec![(SHADOW_ATTR, Expr::lit_str("elements/x-box.tsx#default"))], Vec::new());
    assert_eq!(render_under(None, BY_REACT, page, &ValueMap::default(), &library).unwrap(), format!("<x-box>{open}in</template></x-box>"));
  }
}

#[test]
fn an_island_reading_a_key_a_deferred_segment_has_yet_to_seed_is_held_with_its_children() {
  let badge = Component {
    body: vec![Stmt::Let { name: "n".to_owned(), expr: Expr::Coalesce(Box::new(Expr::Store("cart/count".to_owned())), Box::new(Expr::Lit(Lit::Int(0)))) }],
    render: Tmpl::Element { tag: "b".to_owned(), attrs: Vec::new(), children: vec![Tmpl::Expr(Expr::var("n"))] },
    state: vec!["n".to_owned()],
    stores: vec![("n".to_owned(), "cart/count".to_owned())],
    handlers: Vec::new(),
    owner: crate::ast::Owner::React,
    shadow: None, scope: None,
  };
  let mut library = Components::new();
  library.insert("src/Badge.tsx#Badge".to_owned(), Arc::new(badge));
  let children = vec![Tmpl::Element { tag: "i".to_owned(), attrs: Vec::new(), children: vec![Tmpl::Text("kept".to_owned())] }];
  let place = |mode: Option<&str>| Component::new(crate::ast::Owner::Fsr, Vec::new(), Tmpl::Island { module: "src/Badge.tsx#Badge".to_owned(), props: Vec::new(), children: children.clone(), when: None, mode: mode.map(str::to_owned), id: 0, define: false });
  let props = |pending: &[&str]| {
    let mut props = ValueMap::default();
    props.insert("$store".to_owned(), Value::Map([("cart/count".to_owned(), Value::Int(2))].into_iter().collect()));
    props.insert(snapfire_fsr_runtime::PENDING_PROP.to_owned(), Value::seq(pending.iter().map(|k| Value::str(*k)).collect::<Vec<_>>()));
    props
  };

  let held = Interpreter::default().render_module("routes/layout.tsx#default", &place(None), &props(&["cart/count", "other"]), &library).unwrap();
  let island = &held.islands[0];
  assert!(!island.body.html.contains("<b>"), "none of its own markup is written while the value is about to change: {}", island.body.html);
  assert!(island.body.html.contains("<i>kept</i>"), "its children still are, in their region: {}", island.body.html);
  let mounted = island.mount_props();
  assert_eq!(mounted.get(AWAITS_PROP), Some(&Value::seq(vec![Value::str("cart/count")])), "it waits for the pending key it reads and no other: {mounted:?}");
  assert!(!mounted.contains_key(RENDERED_STORE_PROP), "it hydrates nothing, so it carries no rendered values: {mounted:?}");

  let server = Interpreter::default().render_module("routes/layout.tsx#default", &place(Some(SERVER_MODE)), &props(&["cart/count"]), &library).unwrap();
  let mounted = server.islands[0].mount_props();
  assert!(mounted.contains_key(AWAITS_PROP) && mounted.contains_key(READS_PROP), "a server island is held too and still names its reads for its first step: {mounted:?}");

  let unrelated = Interpreter::default().render_module("routes/layout.tsx#default", &place(None), &props(&["other"]), &library).unwrap();
  assert!(unrelated.islands[0].body.html.starts_with("<b>2</b>"), "a pending key it does not read holds nothing: {}", unrelated.islands[0].body.html);
  assert!(!unrelated.islands[0].mount_props().contains_key(AWAITS_PROP));
}

#[test]
fn an_island_the_server_has_no_body_for_is_held_by_the_keys_the_plan_says_it_reads() {
  let library = Components::new();
  let place = Component::new(crate::ast::Owner::Fsr, Vec::new(), Tmpl::Island { module: "src/ui/Foreign.vue#default".to_owned(), props: Vec::new(), children: Vec::new(), when: None, mode: None, id: 0, define: false });
  let reads = [("src/ui/Foreign.vue#default".to_owned(), vec!["repro/owner".to_owned(), "cart/count".to_owned()])].into_iter().collect();
  let interpreter = Interpreter::default().with_island_reads(reads);
  let props = |pending: &[&str]| -> ValueMap { [(snapfire_fsr_runtime::PENDING_PROP.to_owned(), Value::seq(pending.iter().map(|k| Value::str(*k)).collect::<Vec<_>>()))].into_iter().collect() };
  let held = interpreter.render_module("routes/layout.tsx#default", &place, &props(&["repro/owner"]), &library).unwrap();
  assert_eq!(held.islands[0].mount_props().get(AWAITS_PROP), Some(&Value::seq(vec![Value::str("repro/owner")])), "the pending key it reads, not the settled one");
  let settled = interpreter.render_module("routes/layout.tsx#default", &place, &props(&["other"]), &library).unwrap();
  assert!(!settled.islands[0].mount_props().contains_key(AWAITS_PROP));
}

#[test]
fn an_island_carries_the_store_values_it_was_rendered_from() {
  let badge = Component {
    body: vec![
      Stmt::Let { name: "n".to_owned(), expr: Expr::Coalesce(Box::new(Expr::Store("cart/count".to_owned())), Box::new(Expr::Lit(Lit::Int(0)))) },
      Stmt::Let { name: "who".to_owned(), expr: Expr::Coalesce(Box::new(Expr::Store("user/name".to_owned())), Box::new(Expr::lit_str("guest"))) },
    ],
    render: Tmpl::Element { tag: "b".to_owned(), attrs: Vec::new(), children: vec![Tmpl::Expr(Expr::var("n")), Tmpl::Expr(Expr::var("who"))] },
    state: vec!["n".to_owned(), "who".to_owned()],
    stores: vec![("n".to_owned(), "cart/count".to_owned()), ("who".to_owned(), "user/name".to_owned())],
    handlers: Vec::new(),
    owner: crate::ast::Owner::React,
    shadow: None, scope: None,
  };
  let mut library = Components::new();
  library.insert("src/Badge.tsx#Badge".to_owned(), Arc::new(badge));
  let place = |mode: Option<&str>| Component::new(crate::ast::Owner::Fsr, Vec::new(), Tmpl::Island { module: "src/Badge.tsx#Badge".to_owned(), props: Vec::new(), children: Vec::new(), when: None, mode: mode.map(str::to_owned), id: 0, define: false });
  let mut seeded = ValueMap::default();
  seeded.insert("$store".to_owned(), Value::Map([("cart/count".to_owned(), Value::Int(2)), ("other".to_owned(), Value::Int(9))].into_iter().collect()));
  let placed = Interpreter::default().render_module("routes/page.tsx#default", &place(None), &seeded, &library).unwrap();
  let mounted = placed.islands[0].mount_props();
  assert_eq!(placed.islands[0].body.html, "<b>2guest</b>");
  assert_eq!(mounted.get(RENDERED_STORE_PROP), Some(&Value::Map([("cart/count".to_owned(), Value::Int(2))].into_iter().collect())), "the keys it reads that the store held, and only those: {mounted:?}");
  assert!(!mounted.contains_key(READS_PROP));

  let server = Interpreter::default().render_module("routes/page.tsx#default", &place(Some(SERVER_MODE)), &seeded, &library).unwrap();
  let mounted = server.islands[0].mount_props();
  assert_eq!(mounted.get(RENDERED_STORE_PROP), Some(&Value::Map([("cart/count".to_owned(), Value::Int(2))].into_iter().collect())), "a server island carries what it rendered from, to step when it mounts into a store that moved past it: {mounted:?}");
  assert!(mounted.contains_key(READS_PROP));

  let unseeded = Interpreter::default().render_module("routes/page.tsx#default", &place(None), &ValueMap::default(), &library).unwrap();
  assert!(!unseeded.islands[0].mount_props().contains_key(RENDERED_STORE_PROP), "nothing held, nothing carried: the browser falls back to `initial` as the server did");
}

#[test]
fn a_server_island_reads_and_writes_the_store_rather_than_holding_it() {
  use crate::ast::Handler;
  let counter = Component {
    body: vec![Stmt::Let { name: "n".to_owned(), expr: Expr::Coalesce(Box::new(Expr::Store("cart/count".to_owned())), Box::new(Expr::Lit(Lit::Int(0)))) }],
    render: Tmpl::Element { tag: "button".to_owned(), attrs: vec![Entry::Field(format!("{HANDLER_ATTR}click"), Expr::Lit(Lit::Int(0)))], children: vec![Tmpl::Expr(Expr::var("n"))] },
    state: vec!["n".to_owned()],
    stores: vec![("n".to_owned(), "cart/count".to_owned())],
    handlers: vec![Handler { event: "click".to_owned(), body: vec![Stmt::Return(Expr::Object(vec![Entry::Field("n".to_owned(), Expr::Arith(crate::ast::ArithOp::Add, Box::new(Expr::var("n")), Box::new(Expr::Lit(Lit::Int(1)))))]))] }],
    owner: crate::ast::Owner::React,
    shadow: None, scope: None,
  };
  let mut library = Components::new();
  library.insert("src/Counter.tsx#Counter".to_owned(), Arc::new(counter.clone()));
  let page = Component::new(crate::ast::Owner::Fsr, Vec::new(), Tmpl::Island { module: "src/Counter.tsx#Counter".to_owned(), props: Vec::new(), children: Vec::new(), when: None, mode: Some(SERVER_MODE.to_owned()), id: 0, define: false });
  let mut seeded = ValueMap::default();
  seeded.insert("$store".to_owned(), Value::Map([("cart/count".to_owned(), Value::Int(2))].into_iter().collect()));
  let placed = Interpreter::default().render_module("routes/page.tsx#default", &page, &seeded, &library).unwrap();
  let island = &placed.islands[0];
  assert_eq!(island.body.html, "<button data-sf-on=\"click:0\">2</button>", "the first paint reads the store the page was rendered with");
  let mounted = island.mount_props();
  assert_eq!(mounted.get(READS_PROP), Some(&Value::seq(vec![Value::str("cart/count")])), "the browser is told which keys to send");
  assert_eq!(mounted.get(STATE_PROP), Some(&Value::Map(ValueMap::default())), "a store binding is not state the island carries");

  let step = |count: i128, handler: Option<HandlerRef>| {
    let mut props = ValueMap::default();
    props.insert("$store".to_owned(), Value::Map([("cart/count".to_owned(), Value::Int(count))].into_iter().collect()));
    Interpreter::default().island_step("src/Counter.tsx#Counter", &counter, &props, &ValueMap::default(), handler, &Value::Null, &library).unwrap()
  };
  let clicked = step(4, Some(HandlerRef::own(0)));
  assert_eq!(clicked.rendered.html, "<button data-sf-on=\"click:0\">5</button>", "the handler read the browser's value and the render read its write");
  assert_eq!(clicked.store.get("cart/count"), Some(&Value::Int(5)), "the write goes back to the browser's store");
  assert!(clicked.state.is_empty(), "and not into the island's state: {:?}", clicked.state);
  let followed = step(9, None);
  assert_eq!(followed.rendered.html, "<button data-sf-on=\"click:0\">9</button>", "a step with no handler renders what the store holds now");
  assert!(followed.store.is_empty());
}

#[test]
fn a_server_island_step_renders_the_derive_its_write_changes() {
  use crate::ast::{ArithOp, Handler};
  use crate::interp::Derive;
  let store = |key: &str| Expr::Coalesce(Box::new(Expr::Store(key.to_owned())), Box::new(Expr::Lit(Lit::Int(0))));
  let order = Component {
    body: vec![Stmt::Let { name: "q".to_owned(), expr: store("order/qty") }, Stmt::Let { name: "t".to_owned(), expr: store("order/total") }],
    render: Tmpl::Element { tag: "button".to_owned(), attrs: vec![Entry::Field(format!("{HANDLER_ATTR}click"), Expr::Lit(Lit::Int(0)))], children: vec![Tmpl::Expr(Expr::var("t"))] },
    state: vec!["q".to_owned()],
    stores: vec![("q".to_owned(), "order/qty".to_owned())],
    handlers: vec![Handler { event: "click".to_owned(), body: vec![Stmt::Return(Expr::Object(vec![Entry::Field("q".to_owned(), Expr::Arith(ArithOp::Add, Box::new(Expr::var("q")), Box::new(Expr::Lit(Lit::Int(1)))))]))] }],
    owner: crate::ast::Owner::React,
    shadow: None, scope: None,
  };
  let derives = || vec![Derive { key: "order/total".to_owned(), sources: vec!["order/price".to_owned(), "order/qty".to_owned()], body: Some(Expr::Arith(ArithOp::Mul, Box::new(store("order/price")), Box::new(store("order/qty")))) }];
  let mut library = Components::new();
  library.insert("src/Order.tsx#Order".to_owned(), Arc::new(order.clone()));
  let page = Component::new(crate::ast::Owner::Fsr, Vec::new(), Tmpl::Island { module: "src/Order.tsx#Order".to_owned(), props: Vec::new(), children: Vec::new(), when: None, mode: Some(SERVER_MODE.to_owned()), id: 0, define: false });
  let seeded_store: ValueMap = [("order/price", 12), ("order/qty", 5), ("order/total", 60)].into_iter().map(|(key, value)| (key.to_owned(), Value::Int(value))).collect();
  let mut seeded = ValueMap::default();
  seeded.insert("$store".to_owned(), Value::Map(seeded_store.clone()));
  let placed = Interpreter::default().with_derives(derives()).render_module("routes/page.tsx#default", &page, &seeded, &library).unwrap();
  let reads = placed.islands[0].mount_props().get(READS_PROP).cloned();
  assert_eq!(reads, Some(Value::seq(vec![Value::str("order/price"), Value::str("order/qty"), Value::str("order/total")])), "the browser sends the sources a derived read needs");

  let mut props = ValueMap::default();
  props.insert("$store".to_owned(), Value::Map(seeded_store));
  let clicked = Interpreter::default().with_derives(derives()).island_step("src/Order.tsx#Order", &order, &props, &ValueMap::default(), Some(HandlerRef::own(0)), &Value::Null, &library).unwrap();
  assert_eq!(clicked.rendered.html, "<button data-sf-on=\"click:0\">72</button>", "the render reads the derive recomputed from the write");
  assert_eq!(clicked.store.get("order/qty"), Some(&Value::Int(6)));
  assert!(!clicked.store.contains_key("order/total"), "a derived key is not a write the browser takes: {:?}", clicked.store);
}

#[test]
fn a_handler_bound_inside_a_loop_is_addressed_by_its_component() {
  use crate::ast::Handler;
  let widget = Component {
    body: vec![Stmt::Let { name: "n".to_owned(), expr: Expr::Lit(Lit::Int(0)) }],
    render: Tmpl::Element {
      tag: "ul".to_owned(),
      attrs: Vec::new(),
      children: vec![Tmpl::For {
        over: Expr::Array(vec![Entry::Item(Expr::lit_str("a")), Entry::Item(Expr::lit_str("b"))]),
        params: vec!["c".to_owned()],
        body: Box::new(Tmpl::Element { tag: "button".to_owned(), attrs: vec![Entry::Field(format!("{HANDLER_ATTR}click"), Expr::Lit(Lit::Int(0)))], children: vec![Tmpl::Expr(Expr::var("c"))] }),
      }],
    },
    state: vec!["n".to_owned()],
    stores: Vec::new(),
    handlers: vec![Handler { event: "click".to_owned(), body: vec![Stmt::Return(Expr::Object(vec![Entry::Field("n".to_owned(), Expr::Lit(Lit::Int(7)))]))] }],
    owner: crate::ast::Owner::React,
    shadow: None, scope: None,
  };
  let library = Components::new();
  let props = ValueMap::default();
  let step = |state: &ValueMap, handler: Option<HandlerRef>| Interpreter::default().island_step("src/Widget.tsx#Widget", &widget, &props, state, handler, &Value::Null, &library);
  let first = step(&ValueMap::default(), None).unwrap();
  assert_eq!(first.rendered.html, "<ul><button data-sf-on=\"click:0\">a</button><button data-sf-on=\"click:0\">b</button></ul>", "the loop's iteration is not part of the handler's address");
  let second = step(&first.state, Some(HandlerRef::own(0))).unwrap();
  assert_eq!(second.state.get("n"), Some(&Value::Int(7)));
}

/// A component rendered inside the island keeps state of its own under its
/// address: `<path>/<name>` in the map, `<path>/<index>` on its markers,
/// one instance per keyed placement and per iteration.
#[test]
fn a_component_inside_the_island_keeps_state_under_its_address() {
  use crate::ast::Handler;
  let bump = |name: &str| Handler { event: "click".to_owned(), body: vec![Stmt::Return(Expr::Object(vec![Entry::Field(name.to_owned(), Expr::Arith(crate::ast::ArithOp::Add, Box::new(Expr::var(name)), Box::new(Expr::Lit(Lit::Int(1)))))]))] };
  let inner = Component {
    body: vec![Stmt::Let { name: "x".to_owned(), expr: Expr::var("$props").field("start") }],
    render: Tmpl::Element { tag: "i".to_owned(), attrs: vec![Entry::Field(format!("{HANDLER_ATTR}click"), Expr::Lit(Lit::Int(0)))], children: vec![Tmpl::Expr(Expr::var("x"))] },
    state: vec!["x".to_owned()],
    stores: Vec::new(),
    handlers: vec![bump("x")],
    owner: crate::ast::Owner::React,
    shadow: None, scope: None,
  };
  let place = |id: u32, start: Expr| Tmpl::Component { module: "src/Inner.tsx#Inner".to_owned(), props: vec![Entry::Field("start".to_owned(), start)], children: Vec::new(), id, keyed: true };
  let widget = Component {
    body: vec![Stmt::Let { name: "n".to_owned(), expr: Expr::Lit(Lit::Int(1)) }],
    render: Tmpl::Element {
      tag: "div".to_owned(),
      attrs: Vec::new(),
      children: vec![
        Tmpl::Element { tag: "button".to_owned(), attrs: vec![Entry::Field(format!("{HANDLER_ATTR}click"), Expr::Lit(Lit::Int(0)))], children: vec![Tmpl::Expr(Expr::var("n"))] },
        place(1, Expr::var("n")),
        Tmpl::For { over: Expr::Array(vec![Entry::Item(Expr::Lit(Lit::Int(10))), Entry::Item(Expr::Lit(Lit::Int(20)))]), params: vec!["s".to_owned()], body: Box::new(place(2, Expr::var("s"))) },
      ],
    },
    state: vec!["n".to_owned()],
    stores: Vec::new(),
    handlers: vec![bump("n")],
    owner: crate::ast::Owner::React,
    shadow: None, scope: None,
  };
  let mut library = Components::new();
  library.insert("src/Inner.tsx#Inner".to_owned(), Arc::new(inner));
  let props = ValueMap::default();
  let step = |state: &ValueMap, handler: Option<HandlerRef>| Interpreter::default().island_step("src/Widget.tsx#Widget", &widget, &props, state, handler, &Value::Null, &library);
  let spell = |state: &ValueMap| state.iter().map(|(k, v)| format!("{k}={}", stringify(v).unwrap())).collect::<Vec<_>>().join(" ");

  let first = step(&ValueMap::default(), None).unwrap();
  assert_eq!(first.rendered.html, "<div><button data-sf-on=\"click:0\">1</button><i data-sf-on=\"click:c1/0\">1</i><i data-sf-on=\"click:0.c2/0\">10</i><i data-sf-on=\"click:1.c2/0\">20</i></div>");
  assert_eq!(spell(&first.state), "n=1 c1/x=1 0.c2/x=10 1.c2/x=20", "every instance's state is in the map under its address");

  let second = step(&first.state, HandlerRef::parse("1.c2/0")).unwrap();
  assert_eq!(spell(&second.state), "n=1 c1/x=1 0.c2/x=10 1.c2/x=21", "the addressed instance stepped and nothing else moved");
  assert!(second.rendered.html.contains("<i data-sf-on=\"click:1.c2/0\">21</i>"), "{}", second.rendered.html);

  let third = step(&second.state, Some(HandlerRef::own(0))).unwrap();
  assert_eq!(spell(&third.state), "n=2 c1/x=1 0.c2/x=10 1.c2/x=21", "the island's own step keeps the nested state the browser carried, whatever the props now say");

  let missing = step(&third.state, HandlerRef::parse("c9/0")).unwrap_err();
  assert_eq!(missing.kind, FailureKind::NotFound, "{}", missing.message);
  let none = step(&third.state, HandlerRef::parse("c1/4")).unwrap_err();
  assert_eq!(none.kind, FailureKind::NotFound, "{}", none.message);
  assert!(HandlerRef::parse("c1/").is_none() && HandlerRef::parse("x").is_none() && HandlerRef::parse("2") == Some(HandlerRef::own(2)));
}
