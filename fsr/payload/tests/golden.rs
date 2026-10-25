use snapfire_fsr_core::{ModuleId, Node, Place, SlotId, TypedArray, Value, ValueMap};
use snapfire_fsr_payload::{html_serialize, serialize_page};

fn walked_page() -> Node {
  let mut props = ValueMap::default();
  props.insert("series".to_owned(), Value::TypedArray(TypedArray::F64(vec![1.0, 2.5, 3.0])));
  Node::Seq(vec![
    Node::raw("<main><h1>Servers</h1>"),
    Node::Client {
      module: ModuleId::new("components/ServerChart.tsx", "default"),
      props,
      children: Vec::new(),
      rendered: false,
    },
    Node::raw("</main>"),
  ])
}

const SERIES_B64: &str = "AAAAAAAA8D8AAAAAAAAEQAAAAAAAAAhA";

#[test]
fn wire_rows_are_stable() {
  let expected = format!(
    "V {{\"fmt\":3,\"enc\":\"json\"}}\n\
     N [\"q\",[[\"r\",\"<main><h1>Servers</h1>\"],[\"c\",{{\"m\":\"components/ServerChart.tsx#default\",\"p\":{{\"series\":{{\"$\":\"ta\",\"k\":\"f64\",\"v\":\"{SERIES_B64}\"}}}},\"ch\":[],\"r\":false}}],[\"r\",\"</main>\"]]]\n"
  );
  assert_eq!(serialize_page(&walked_page()), expected);
}

#[test]
fn html_encoding_is_stable() {
  let expected = format!(
    "<main><h1>Servers</h1>\
     <sf-i id=\"sf-i0\" data-sf-module=\"components/ServerChart.tsx#default\"></sf-i>\
     <script type=\"application/json\" data-sf-props=\"sf-i0\">{{\"series\":{{\"$\":\"ta\",\"k\":\"f64\",\"v\":\"{SERIES_B64}\"}}}}</script>\
     </main>"
  );
  assert_eq!(html_serialize(&walked_page()), expected);
}

#[test]
fn text_nodes_escape_markup() {
  let node = Node::Seq(vec![Node::text("a < b & c > d")]);
  assert_eq!(html_serialize(&node), "a &lt; b &amp; c &gt; d");
}

#[test]
fn string_props_cannot_break_out_of_the_script_tag() {
  let mut props = ValueMap::default();
  props.insert("payload".to_owned(), Value::str("</script><script>alert(1)</script>"));
  let node = Node::Client {
    module: ModuleId::new("components/X.tsx", "default"),
    props,
    children: Vec::new(),
    rendered: false,
  };
  let html = html_serialize(&node);
  let after_open = html.split_once("data-sf-props=\"sf-i0\">").unwrap().1;
  let inner = after_open.rsplit_once("</script>").unwrap().0;
  assert!(!inner.contains("</script>"), "props JSON must not contain a literal close tag: {inner}");
}

#[test]
fn rendered_content_renders_inside_the_island_marker() {
  let node = Node::Client {
    module: ModuleId::new("components/X.tsx", "default"),
    props: ValueMap::default(),
    children: vec![Node::raw("<svg></svg>")],
    rendered: true,
  };
  assert!(html_serialize(&node).starts_with(
    "<sf-i id=\"sf-i0\" data-sf-module=\"components/X.tsx#default\" data-sf-ssr><svg></svg></sf-i>"
  ));
}

#[test]
fn pending_emits_fallback_in_place() {
  let node = Node::Pending { slot: SlotId(1), fallback: Box::new(Node::raw("<div class=skl></div>")) };
  assert_eq!(html_serialize(&node), "<div data-sf-slot=\"1\"><div class=skl></div></div>");
}

#[test]
fn island_ids_allocate_in_tree_order() {
  let island = |name: &str| Node::Client {
    module: ModuleId::new(format!("components/{name}.tsx"), "default"),
    props: ValueMap::default(),
    children: Vec::new(),
    rendered: false,
  };
  let html = html_serialize(&Node::Seq(vec![island("A"), island("B")]));
  let a = html.find("sf-i0").unwrap();
  let b = html.find("sf-i1").unwrap();
  assert!(a < b);
  assert!(html.contains("data-sf-module=\"components/B.tsx#default\""));
}

#[test]
fn placed_content_follows_the_tree_where_the_parser_puts_it_at_the_end_of_the_body() {
  let island = Node::Client { module: ModuleId::new("components/M.tsx", "default"), props: ValueMap::default(), children: vec![Node::raw("<b>m</b>")], rendered: true };
  let node = Node::Seq(vec![
    Node::raw("<main>"),
    Node::Placed { target: Place::Body, nodes: vec![Node::raw("<dialog>"), island, Node::raw("</dialog>")] },
    Node::raw("</main>"),
    Node::Placed { target: Place::Body, nodes: vec![Node::text("a < b")] },
  ]);
  assert_eq!(
    html_serialize(&node),
    "<main></main><template data-sf-l=\"\"></template><dialog><sf-i id=\"sf-i0\" data-sf-module=\"components/M.tsx#default\" data-sf-ssr><b>m</b></sf-i><script type=\"application/json\" data-sf-props=\"sf-i0\">{}</script></dialog>a &lt; b<template data-sf-l-end></template>"
  );
}

#[test]
fn an_islands_placed_content_is_one_run_naming_the_island_by_its_region_key_or_else_its_id() {
  let placed = |text: &str| Node::Placed { target: Place::Body, nodes: vec![Node::raw(format!("<!--teleport start anchor--><p>{text}</p><!--teleport anchor-->"))] };
  let island = |key: Option<&str>, nodes: Vec<Node>| {
    let mut props = ValueMap::default();
    if let Some(key) = key {
      props.insert("$k".to_owned(), Value::str(key));
    }
    Node::Client { module: ModuleId::new("components/V.vue", "default"), props, children: vec![Node::Seq(nodes)], rendered: true }
  };
  let node = Node::Seq(vec![island(None, vec![Node::raw("<button>a</button>"), placed("one"), Node::raw("<hr>"), placed("two")]), island(Some("routes/page.tsx#default/2"), vec![placed("three")])]);
  let html = html_serialize(&node);
  let (_, placed) = html.split_once("data-sf-props=\"sf-i1\">{\"$k\":\"routes/page.tsx#default/2\"}</script>").expect("both islands are written first");
  assert_eq!(
    placed,
    "<template data-sf-l=\"\" data-sf-island=\"sf-i0\"></template><!--teleport start anchor--><p>one</p><!--teleport anchor--><!--teleport start anchor--><p>two</p><!--teleport anchor--><template data-sf-l-end></template><template data-sf-l=\"\" data-sf-island=\"routes/page.tsx#default/2\"></template><!--teleport start anchor--><p>three</p><!--teleport anchor--><template data-sf-l-end></template>"
  );
}

fn named(text: &str) -> Node {
  Node::Placed { target: Place::Named("modals".to_owned()), nodes: vec![Node::raw(format!("<p>{text}</p>"))] }
}

fn outlet() -> Node {
  Node::Seq(vec![Node::raw("<div id=\"modals\">"), Node::Outlet("modals".to_owned()), Node::raw("</div>")])
}

#[test]
fn content_placed_at_a_named_place_is_written_inside_its_outlet_whether_the_outlet_comes_before_or_after_it() {
  let before = Node::Seq(vec![outlet(), Node::raw("<main>"), named("one"), Node::raw("</main>"), named("two")]);
  assert_eq!(html_serialize(&before), "<div id=\"modals\"><template data-sf-l=\"\"></template><p>one</p><p>two</p><template data-sf-l-end></template></div><main></main>");
  let after = Node::Seq(vec![Node::raw("<main>"), named("one"), Node::raw("</main>"), outlet()]);
  assert_eq!(html_serialize(&after), "<main></main><div id=\"modals\"><template data-sf-l=\"\"></template><p>one</p><template data-sf-l-end></template></div>");
}

#[test]
fn an_outlet_nothing_is_placed_at_writes_nothing_and_the_body_keeps_its_own_runs() {
  let node = Node::Seq(vec![outlet(), Node::Placed { target: Place::Body, nodes: vec![Node::raw("<dialog></dialog>")] }]);
  assert_eq!(html_serialize(&node), "<div id=\"modals\"></div><template data-sf-l=\"\"></template><dialog></dialog><template data-sf-l-end></template>");
}

#[test]
fn content_whose_place_the_tree_lacks_is_written_after_the_document_naming_its_place() {
  assert_eq!(html_serialize(&named("one")), "<template data-sf-l=\"\" data-sf-to=\"modals\"></template><p>one</p><template data-sf-l-end></template>");
}

#[test]
fn each_islands_runs_at_a_named_place_keep_their_island_and_their_order() {
  let island = |nodes: Vec<Node>| Node::Client { module: ModuleId::new("components/V.vue", "default"), props: ValueMap::default(), children: vec![Node::Seq(nodes)], rendered: true };
  let node = Node::Seq(vec![island(vec![named("a")]), outlet(), island(vec![named("b"), named("c")])]);
  let html = html_serialize(&node);
  let inside = html.split_once("<div id=\"modals\">").unwrap().1.split_once("</div>").unwrap().0;
  assert_eq!(inside, "<template data-sf-l=\"\" data-sf-island=\"sf-i0\"></template><p>a</p><template data-sf-l-end></template><template data-sf-l=\"\" data-sf-island=\"sf-i1\"></template><p>b</p><p>c</p><template data-sf-l-end></template>");
}

#[test]
fn a_session_splices_across_writes_into_one_string_and_hands_on_what_it_could_not_place() {
  use snapfire_fsr_payload::HtmlSession;
  let mut session = HtmlSession::new();
  let mut out = String::new();
  session.write_into(&outlet(), &mut out);
  session.write_into(&named("one"), &mut out);
  session.finish(&mut out);
  assert_eq!(out, "<div id=\"modals\"><template data-sf-l=\"\"></template><p>one</p><template data-sf-l-end></template></div>");
  assert_eq!(session.take_placed(), "");
  let mut late = String::new();
  session.write_into(&named("two"), &mut late);
  session.finish(&mut late);
  assert_eq!(late, "");
  assert_eq!(session.take_placed(), "<template data-sf-l=\"\" data-sf-to=\"modals\"></template><p>two</p><template data-sf-l-end></template>");
}
