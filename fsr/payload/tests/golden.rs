use snapfire_fsr_core::{ModuleId, Node, SlotId, TypedArray, Value, ValueMap};
use snapfire_fsr_payload::{html_serialize, serialize_page};

fn walked_page() -> Node {
  let mut props = ValueMap::new();
  props.insert("series".to_owned(), Value::TypedArray(TypedArray::F64(vec![1.0, 2.5, 3.0])));
  Node::Seq(vec![
    Node::raw("<main><h1>Servers</h1>"),
    Node::Client {
      module: ModuleId::new("components/ServerChart.tsx", "default"),
      props,
      children: Vec::new(),
      ssr: None,
    },
    Node::raw("</main>"),
  ])
}

const SERIES_B64: &str = "AAAAAAAA8D8AAAAAAAAEQAAAAAAAAAhA";

#[test]
fn wire_rows_are_stable() {
  let expected = format!(
    "V {{\"fmt\":1,\"enc\":\"json\"}}\n\
     N [\"q\",[[\"r\",\"<main><h1>Servers</h1>\"],[\"c\",{{\"m\":\"components/ServerChart.tsx#default\",\"p\":{{\"series\":{{\"$\":\"ta\",\"k\":\"f64\",\"v\":\"{SERIES_B64}\"}}}},\"ch\":[],\"s\":null}}],[\"r\",\"</main>\"]]]\n"
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
  let mut props = ValueMap::new();
  props.insert("payload".to_owned(), Value::str("</script><script>alert(1)</script>"));
  let node = Node::Client {
    module: ModuleId::new("components/X.tsx", "default"),
    props,
    children: Vec::new(),
    ssr: None,
  };
  let html = html_serialize(&node);
  let after_open = html.split_once("data-sf-props=\"sf-i0\">").unwrap().1;
  let inner = after_open.rsplit_once("</script>").unwrap().0;
  assert!(!inner.contains("</script>"), "props JSON must not contain a literal close tag: {inner}");
}

#[test]
fn ssr_content_renders_inside_the_island_marker() {
  let node = Node::Client {
    module: ModuleId::new("components/X.tsx", "default"),
    props: ValueMap::new(),
    children: Vec::new(),
    ssr: Some(Box::new(Node::raw("<svg></svg>"))),
  };
  assert!(html_serialize(&node).starts_with(
    "<sf-i id=\"sf-i0\" data-sf-module=\"components/X.tsx#default\"><svg></svg></sf-i>"
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
    props: ValueMap::new(),
    children: Vec::new(),
    ssr: None,
  };
  let html = html_serialize(&Node::Seq(vec![island("A"), island("B")]));
  let a = html.find("sf-i0").unwrap();
  let b = html.find("sf-i1").unwrap();
  assert!(a < b);
  assert!(html.contains("data-sf-module=\"components/B.tsx#default\""));
}
