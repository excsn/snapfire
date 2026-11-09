use super::*;
use crate::ast::{Entry, Expr, Tmpl};
use crate::bind::rendered_nodes;
use snapfire_fsr_core::Node;

#[test]
fn a_placement_in_a_loop_keys_its_region_under_its_iteration() {
  let mut library = Components::new();
  library.insert(
    "src/ui/Body.tsx#Body".to_owned(),
    Arc::new(Component { body: Vec::new(), render: Tmpl::Expr(Expr::var("$props").field("text")), state: Vec::new(), stores: Vec::new(), handlers: Vec::new(), owner: crate::ast::Owner::React, shadow: None, scope: None }),
  );
  let page = Component {
    body: Vec::new(),
    render: Tmpl::For {
      over: Expr::var("$props").field("blips"),
      params: vec!["blip".to_owned()],
      body: Box::new(Tmpl::Island { module: "src/ui/Body.tsx#Body".to_owned(), props: vec![Entry::Field("text".to_owned(), Expr::var("blip"))], children: Vec::new(), when: None, mode: None, id: 1, define: false }),
    },
    state: Vec::new(),
    stores: Vec::new(),
    handlers: Vec::new(),
    owner: crate::ast::Owner::React,
    shadow: None, scope: None,
  };
  let mut props = ValueMap::default();
  props.insert("blips".to_owned(), Value::seq(vec![Value::str("one"), Value::str("two"), Value::str("three")]));
  let rendered = Interpreter::default().render_module("routes/w/page.tsx#default", &page, &props, &library).unwrap();
  let keys: Vec<&str> = rendered.islands.iter().map(|i| i.key.as_str()).collect();
  assert_eq!(keys, ["routes/w/page.tsx#default|i1@0", "routes/w/page.tsx#default|i1@1", "routes/w/page.tsx#default|i1@2"], "one placement in a loop is one region per iteration");
  assert_eq!(rendered.islands[1].mount_props().get(KEY_PROP), Some(&Value::str("routes/w/page.tsx#default|i1@1")), "the key rides in the props the browser mounts with");
  let nodes = rendered_nodes(&rendered);
  let marked: Vec<&str> = nodes.iter().filter_map(|n| match n {
    Node::Raw(html) if html.0.starts_with("<sf-s data-sf-island") => Some(html.0.as_str()),
    _ => None,
  }).collect();
  assert_eq!(marked, ["<sf-s data-sf-island data-sf-region=\"routes/w/page.tsx#default|i1@0\">", "<sf-s data-sf-island data-sf-region=\"routes/w/page.tsx#default|i1@1\">", "<sf-s data-sf-island data-sf-region=\"routes/w/page.tsx#default|i1@2\">"]);
}

fn island_at(module: &str, id: u32) -> Tmpl {
  Tmpl::Island { module: module.to_owned(), props: Vec::new(), children: Vec::new(), when: None, mode: None, id, define: false }
}

fn keys_of(rendered: &Rendered) -> Vec<&str> {
  rendered.islands.iter().map(|i| i.key.as_str()).collect()
}

#[test]
fn two_placements_of_one_component_key_what_it_holds_apart() {
  let mut library = Components::new();
  library.insert("src/ui/Body.tsx#Body".to_owned(), Arc::new(Component::new(crate::ast::Owner::React, Vec::new(), Tmpl::Text("body".to_owned()))));
  library.insert("src/ui/Card.tsx#Card".to_owned(), Arc::new(Component::new(crate::ast::Owner::React, Vec::new(), island_at("src/ui/Body.tsx#Body", 0))));
  let card = |id: u32| Tmpl::Component { module: "src/ui/Card.tsx#Card".to_owned(), props: Vec::new(), children: Vec::new(), id, keyed: true };
  let page = Component::new(crate::ast::Owner::React, Vec::new(), Tmpl::Fragment(vec![card(1), card(2)]));
  let rendered = Interpreter::default().render_module("routes/w/page.tsx#default", &page, &ValueMap::default(), &library).unwrap();
  assert_eq!(keys_of(&rendered), ["src/ui/Card.tsx#Card|i0@c1", "src/ui/Card.tsx#Card|i0@c2"], "each keyed placement names itself on the path");
}

#[test]
fn a_component_that_renders_itself_from_two_loops_keys_every_level_apart() {
  const THREAD: &str = "src/ui/Thread.tsx#Thread";
  let under = |field: &str, id: u32| Tmpl::For {
    over: Expr::var("$props").field(field),
    params: vec!["each".to_owned()],
    body: Box::new(Tmpl::Component { module: THREAD.to_owned(), props: vec![Entry::Spread(Expr::var("each"))], children: Vec::new(), id, keyed: true }),
  };
  let mut library = Components::new();
  library.insert("src/ui/Body.tsx#Body".to_owned(), Arc::new(Component::new(crate::ast::Owner::React, Vec::new(), Tmpl::Text("body".to_owned()))));
  library.insert(THREAD.to_owned(), Arc::new(Component::new(crate::ast::Owner::React, Vec::new(), Tmpl::Fragment(vec![island_at("src/ui/Body.tsx#Body", 0), under("replies", 1), under("asides", 2)]))));
  fn node(replies: Vec<Value>, asides: Vec<Value>) -> Value {
    let mut map = ValueMap::default();
    map.insert("replies".to_owned(), Value::seq(replies));
    map.insert("asides".to_owned(), Value::seq(asides));
    Value::Map(map)
  }
  let Value::Map(props) = node(vec![node(vec![node(vec![], vec![])], vec![])], vec![node(vec![node(vec![], vec![])], vec![])]) else { unreachable!() };
  let rendered = Interpreter::default().render_module(THREAD, &library[THREAD], &props, &library).unwrap();
  assert_eq!(
    keys_of(&rendered),
    [
      "src/ui/Thread.tsx#Thread|i0",
      "src/ui/Thread.tsx#Thread|i0@0.c1",
      "src/ui/Thread.tsx#Thread|i0@0.c1.0.c1",
      "src/ui/Thread.tsx#Thread|i0@0.c2",
      "src/ui/Thread.tsx#Thread|i0@0.c2.0.c1",
    ],
    "a reply and an aside at the same index are different placements"
  );
}

#[test]
fn a_callers_children_key_under_the_caller_rather_than_the_component_placing_them() {
  let mut library = Components::new();
  library.insert("src/ui/Body.tsx#Body".to_owned(), Arc::new(Component::new(crate::ast::Owner::React, Vec::new(), Tmpl::Text("body".to_owned()))));
  library.insert("src/ui/Wrap.tsx#Wrap".to_owned(), Arc::new(Component::new(crate::ast::Owner::React, Vec::new(), Tmpl::Fragment(vec![island_at("src/ui/Body.tsx#Body", 0), Tmpl::Slot("content".to_owned())]))));
  let page = Component::new(crate::ast::Owner::React, 
    Vec::new(),
    Tmpl::For {
      over: Expr::var("$props").field("rows"),
      params: vec!["row".to_owned()],
      body: Box::new(Tmpl::Component { module: "src/ui/Wrap.tsx#Wrap".to_owned(), props: Vec::new(), children: vec![island_at("src/ui/Body.tsx#Body", 2)], id: 1, keyed: true }),
    },
  );
  let mut props = ValueMap::default();
  props.insert("rows".to_owned(), Value::seq(vec![Value::str("one"), Value::str("two")]));
  let rendered = Interpreter::default().render_module("routes/w/page.tsx#default", &page, &props, &library).unwrap();
  assert_eq!(
    keys_of(&rendered),
    ["src/ui/Wrap.tsx#Wrap|i0@0.c1", "routes/w/page.tsx#default|i2@0", "src/ui/Wrap.tsx#Wrap|i0@1.c1", "routes/w/page.tsx#default|i2@1"],
    "the children were built by the page, which is where the browser computes their keys"
  );
}

#[test]
fn an_islands_children_render_in_a_region_where_the_component_places_them() {
  let mut library = Components::new();
  let card = Tmpl::Element { tag: "div".to_owned(), attrs: Vec::new(), children: vec![Tmpl::Text("card".to_owned()), Tmpl::Slot("content".to_owned())] };
  library.insert("src/ui/Card.tsx#Card".to_owned(), Arc::new(Component::new(crate::ast::Owner::React, Vec::new(), card)));
  let child = Tmpl::Element { tag: "p".to_owned(), attrs: Vec::new(), children: vec![Tmpl::Expr(Expr::var("$props").field("name"))] };
  let page = Component::new(crate::ast::Owner::React, Vec::new(), Tmpl::Island { module: "src/ui/Card.tsx#Card".to_owned(), props: Vec::new(), children: vec![child], when: None, mode: None, id: 0, define: false });
  let mut props = ValueMap::default();
  props.insert("name".to_owned(), Value::str("ada"));
  let rendered = Interpreter::default().render(&page, &props, &library).unwrap();
  assert_eq!(rendered.islands[0].body.html, "<div>card<sf-s data-sf-children><p>ada</p></sf-s></div>", "the children read the page's props");
}

#[test]
fn a_component_the_server_cannot_render_is_placed_with_its_children() {
  let page = Component::new(crate::ast::Owner::React, 
    Vec::new(),
    Tmpl::Fragment(vec![
      Tmpl::Island { module: "src/ui/Tonight.vue#default".to_owned(), props: Vec::new(), children: vec![Tmpl::Element { tag: "p".to_owned(), attrs: Vec::new(), children: vec![Tmpl::Text("server".to_owned())] }], when: None, mode: None, id: 0, define: false },
      island_at("src/ui/Empty.vue#default", 1),
    ]),
  );
  let rendered = Interpreter::default().render(&page, &ValueMap::default(), &Components::new()).unwrap();
  assert_eq!(rendered.islands[0].body.html, "<sf-s data-sf-children><p>server</p></sf-s>");
  assert_eq!(rendered.islands[1].body.html, "");
}

#[test]
fn an_island_among_an_islands_children_keys_under_the_page_that_wrote_it() {
  let mut library = Components::new();
  library.insert("src/ui/Body.tsx#Body".to_owned(), Arc::new(Component::new(crate::ast::Owner::React, Vec::new(), Tmpl::Text("body".to_owned()))));
  library.insert("src/ui/Card.tsx#Card".to_owned(), Arc::new(Component::new(crate::ast::Owner::React, Vec::new(), Tmpl::Slot("content".to_owned()))));
  let page = Component::new(crate::ast::Owner::React, Vec::new(), Tmpl::Island { module: "src/ui/Card.tsx#Card".to_owned(), props: Vec::new(), children: vec![island_at("src/ui/Body.tsx#Body", 3)], when: None, mode: None, id: 0, define: false });
  let rendered = Interpreter::default().render_module("routes/w/page.tsx#default", &page, &ValueMap::default(), &library).unwrap();
  assert_eq!(keys_of(&rendered), ["routes/w/page.tsx#default|i0"]);
  assert_eq!(keys_of(&rendered.islands[0].body), ["routes/w/page.tsx#default|i3"]);
}

#[test]
fn an_island_renders_apart_and_binds_as_a_nested_client_node_in_a_region() {
  let mut library = Components::new();
  library.insert(
    "src/ui/Help.tsx#Help".to_owned(),
    Arc::new(Component { body: Vec::new(), render: Tmpl::Element { tag: "p".to_owned(), attrs: Vec::new(), children: vec![Tmpl::Text("help ".to_owned()), Tmpl::Expr(Expr::var("$props").field("id"))] }, state: Vec::new(), stores: Vec::new(), handlers: Vec::new(), owner: crate::ast::Owner::React, shadow: None, scope: None }),
  );
  let page = Component {
    body: Vec::new(),
    render: Tmpl::Element {
      tag: "main".to_owned(),
      attrs: Vec::new(),
      children: vec![
        Tmpl::Text("before".to_owned()),
        Tmpl::Island { module: "src/ui/Help.tsx#Help".to_owned(), props: vec![Entry::Field("id".to_owned(), Expr::var("$props").field("id"))], children: Vec::new(), when: Some("visible".to_owned()), mode: None, id: 9, define: false },
        Tmpl::Text("after".to_owned()),
      ],
    }, state: Vec::new(), stores: Vec::new(), handlers: Vec::new(), owner: crate::ast::Owner::React, shadow: None, scope: None
  };
  let mut props = ValueMap::default();
  props.insert("id".to_owned(), Value::int(7i64));
  let rendered = Interpreter::default().render(&page, &props, &library).unwrap();
  assert_eq!(rendered.html, format!("<main>before{ISLAND_MARK}0\u{0}after</main>"));
  assert_eq!(rendered.islands.len(), 1);
  assert_eq!(rendered.islands[0].body.html, "<p>help 7</p>");
  assert_eq!(rendered.islands[0].when.as_deref(), Some("visible"));

  let nodes = rendered_nodes(&rendered);
  assert_eq!(nodes.len(), 5, "{nodes:?}");
  assert_eq!(nodes[0], Node::raw("<main>before"));
  assert_eq!(nodes[1], Node::raw("<sf-s data-sf-island data-sf-region=\"|i9\" data-sf-when=\"visible\">"));
  let Node::Client { module, props: island_props, children, rendered: true } = &nodes[2] else { panic!("{:?}", nodes[2]) };
  assert_eq!(module.to_string(), "src/ui/Help.tsx#Help");
  assert_eq!(island_props.get("id"), Some(&Value::int(7i64)));
  assert_eq!(children, &vec![Node::raw("<p>help 7</p>")]);
  assert_eq!(nodes[3], Node::raw("</sf-s>"));
  assert_eq!(nodes[4], Node::raw("after</main>"));
}

#[test]
fn an_island_inside_an_island_of_its_own_framework_reads_the_providers_above_it() {
  let reader = |owner| Arc::new(Component::new(owner, Vec::new(), Tmpl::Expr(Expr::Context("tone".to_owned()))));
  let mut library = Components::new();
  library.insert("src/ui/Reader.tsx#Reader".to_owned(), reader(crate::ast::Owner::React));
  library.insert("src/ui/Reader.vue#default".to_owned(), reader(crate::ast::Owner::Vue));
  let provide = |tone: &str, then: Tmpl| Tmpl::Let { name: format!("{CONTEXT_PREFIX}tone"), expr: Expr::Lit(crate::ast::Lit::Str(tone.to_owned())), then: Box::new(then) };
  let shell = Tmpl::Fragment(vec![Tmpl::Expr(Expr::Context("tone".to_owned())), provide("dusk", Tmpl::Fragment(vec![island_at("src/ui/Reader.tsx#Reader", 1), island_at("src/ui/Reader.vue#default", 2)]))]);
  library.insert("src/ui/Shell.tsx#Shell".to_owned(), Arc::new(Component::new(crate::ast::Owner::React, Vec::new(), shell)));
  let page = Component::new(crate::ast::Owner::React, Vec::new(), provide("page", island_at("src/ui/Shell.tsx#Shell", 0)));
  let rendered = Interpreter::default().render_module("routes/w/page.tsx#default", &page, &ValueMap::default(), &library).unwrap();
  let shell = &rendered.islands[0];
  assert!(!shell.body.html.contains("page"), "the page's provider stays outside the outermost island: {}", shell.body.html);
  assert_eq!(shell.body.islands[0].body.html, "dusk", "a React island in a React island");
  assert_eq!(shell.body.islands[1].body.html, "", "a Vue island in a React island");
}

#[test]
fn a_neutral_component_placed_as_an_island_takes_the_framework_of_its_file() {
  let mut library = Components::new();
  library.insert("src/ui/Label.tsx#default".to_owned(), Arc::new(Component::new(crate::ast::Owner::Fsr, Vec::new(), Tmpl::Expr(Expr::Context("tone".to_owned())))));
  let provide = Tmpl::Let { name: format!("{CONTEXT_PREFIX}tone"), expr: Expr::Lit(crate::ast::Lit::Str("dusk".to_owned())), then: Box::new(island_at("src/ui/Label.tsx#default", 1)) };
  library.insert("src/ui/Shell.tsx#Shell".to_owned(), Arc::new(Component::new(crate::ast::Owner::React, Vec::new(), provide)));
  let page = Component::new(crate::ast::Owner::React, Vec::new(), island_at("src/ui/Shell.tsx#Shell", 0));
  let rendered = Interpreter::default().render_module("routes/w/page.tsx#default", &page, &ValueMap::default(), &library).unwrap();
  assert_eq!(rendered.islands[0].body.islands[0].body.html, "dusk", "React mounts a .tsx island, so it renders in the React island's tree");
}

#[test]
fn a_svelte_islands_children_carry_the_markers_of_the_tree_its_adapter_renders_them_in() {
  use crate::ast::Owner;
  let element = |tag: &str, attrs: Vec<Entry>, children: Vec<Tmpl>| Tmpl::Element { tag: tag.to_owned(), attrs, children };
  let mut library = Components::new();
  library.insert("src/ui/Outer.svelte#default".to_owned(), Arc::new(Component::new(Owner::Svelte, Vec::new(), element("div", Vec::new(), vec![Tmpl::Slot("children".to_owned())]))));
  library.insert("src/ui/Inner.svelte#default".to_owned(), Arc::new(Component::new(Owner::Svelte, Vec::new(), element("b", Vec::new(), vec![Tmpl::Text("in".to_owned())]))));
  library.insert("src/ui/Late.svelte#default".to_owned(), Arc::new(Component::new(Owner::Svelte, Vec::new(), element("i", Vec::new(), vec![Tmpl::Text("late".to_owned())]))));
  let late = Tmpl::Island { module: "src/ui/Late.svelte#default".to_owned(), props: Vec::new(), children: Vec::new(), when: Some("visible".to_owned()), mode: None, id: 2, define: false };
  let children = vec![
    Tmpl::Text("hi ".to_owned()),
    element("p", Vec::new(), vec![island_at("src/ui/Inner.svelte#default", 1)]),
    element("div", vec![Entry::Field("class".to_owned(), Expr::lit_str("q"))], vec![Tmpl::Text("q".to_owned())]),
    late,
    island_at("src/ui/Away.svelte#default", 3),
  ];
  let page = Component::new(Owner::Fsr, Vec::new(), Tmpl::Island { module: "src/ui/Outer.svelte#default".to_owned(), props: Vec::new(), children, when: None, mode: None, id: 0, define: false });
  let interpreter = Interpreter::default().with_frameworks(crate::Frameworks::default().svelte(Some(SvelteMajor::V5)));
  let rendered = interpreter.render_module("routes/w/page.tsx#default", &page, &ValueMap::default(), &library).unwrap();
  let outer = &rendered.islands[0].body;
  let expected = "<!--[--><div><sf-s data-sf-children><!--[-->\
      <!--[-1--><!---->hi <!----><!--]-->\
      <!--[1--><!----><p><!--[--><!--[0-->\u{0}sf-island:0\u{0}<!--]--><!--]--><!----></p><!----><!--]-->\
      <!--[2--><!----><div class=\"q\">q</div><!----><!--]-->\
      <!--[2--><!---->\u{0}sf-island:1\u{0}<!----><!--]-->\
      <!--[0-->\u{0}sf-island:2\u{0}<!--]-->\
      <!--]--><!----></sf-s></div><!--]-->";
  assert_eq!(outer.html, expected);
  assert_eq!(outer.islands[0].body.html, "<!--[0--><!--[--><b>in</b><!--]--><!--]-->", "an island hydrated in the tree");
  assert_eq!(outer.islands[1].body.html, "<!--[--><i>late</i><!--]-->", "an island hydrated later as a root of its own");
  assert_eq!(outer.islands[2].body.html, "<!--[-1--><!--]-->", "an island the browser renders in the tree");
}

#[test]
fn a_svelte_island_rendered_with_no_children_writes_the_empty_list_its_adapter_hydrates() {
  let component = Component::new(crate::ast::Owner::Svelte, Vec::new(), Tmpl::Element { tag: "div".to_owned(), attrs: Vec::new(), children: vec![Tmpl::Slot("children".to_owned())] });
  let interpreter = Interpreter::default().with_frameworks(crate::Frameworks::default().svelte(Some(SvelteMajor::V5)));
  let island = interpreter.render_island("src/ui/Box.svelte#default", &component, &ValueMap::default(), &Components::new()).unwrap();
  assert_eq!(island.html, format!("<!--[--><div>{CHILDREN_OPEN}<!--[--><!--]--><!----></sf-s></div><!--]-->"));
}

#[test]
fn an_island_the_server_cannot_render_binds_unrendered_with_its_children_as_nodes() {
  let mut library = Components::new();
  library.insert("src/ui/Body.tsx#Body".to_owned(), Arc::new(Component::new(crate::ast::Owner::React, Vec::new(), Tmpl::Text("body".to_owned()))));
  let page = Component::new(crate::ast::Owner::React, Vec::new(), Tmpl::Island { module: "src/ui/Shell.vue#default".to_owned(), props: Vec::new(), children: vec![island_at("src/ui/Body.tsx#Body", 3)], when: None, mode: None, id: 0, define: false });
  let rendered = Interpreter::default().render_module("routes/w/page.tsx#default", &page, &ValueMap::default(), &library).unwrap();
  assert!(!rendered.islands[0].rendered, "the server had no body for the module");
  assert!(rendered.islands[0].body.islands[0].rendered, "the island among its children it rendered");
  let nodes = rendered_nodes(&rendered);
  let Node::Client { module, rendered: false, children, .. } = &nodes[1] else { panic!("{:?}", nodes[1]) };
  assert_eq!(module.to_string(), "src/ui/Shell.vue#default");
  let nested = format!("{children:?}");
  assert!(nested.contains("path: \"src/ui/Body.tsx\", export: \"Body\"") && nested.contains("rendered: true"), "{nested}");
}
