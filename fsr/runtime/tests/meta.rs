use std::sync::Arc;

use futures::executor::block_on;
use futures_util::future::BoxFuture;
use futures_util::{stream, StreamExt};
use snapfire_fsr_core::{Data, DataSourceId, ModuleId, Node, NodeId, Params, PlanNode, SlotName, Value, ValueMap};
use snapfire_fsr_runtime::{
  assemble, html_stream, wire_stream, Chunk, DataSources, Evaluator, Evaluators, Head, HeadEl, LoadError, Meta, Metadata, NodeChunks, RequestCtx, Runtime,
};

struct Shell;

impl Evaluator for Shell {
  fn evaluate(&self, _module: &ModuleId, _props: &Data) -> NodeChunks {
    Box::pin(stream::iter([
      Ok(Chunk::Node(Node::raw("<head>"))),
      Ok(Chunk::Slot(SlotName("head".into()))),
      Ok(Chunk::Node(Node::raw("</head><body>"))),
      Ok(Chunk::Slot(SlotName("content".into()))),
      Ok(Chunk::Node(Node::raw("</body>"))),
    ]))
  }
}

struct Page;

impl Evaluator for Page {
  fn evaluate(&self, _module: &ModuleId, props: &Data) -> NodeChunks {
    let name = match props.get("name") {
      Some(Value::Str(s)) => s.clone(),
      _ => "?".to_owned(),
    };
    Box::pin(stream::iter([Ok(Chunk::Node(Node::raw(format!("<h1>{name}</h1>"))))]))
  }
}

/// Titles the document after the product the loader returned.
struct ProductMeta;

impl Metadata for ProductMeta {
  fn describe(&self, _ctx: &RequestCtx, data: &Data) -> BoxFuture<'static, Result<Meta, LoadError>> {
    let name = match data.get("name") {
      Some(Value::Str(s)) => s.clone(),
      _ => String::new(),
    };
    Box::pin(async move { Ok(Meta { title: Some(format!("{name} · Shop")), description: Some(format!("All about {name}")), head: Vec::new() }) })
  }
}

fn runtime() -> Arc<Runtime> {
  let mut sources = DataSources::new();
  sources.insert_fn("product", |_p| async move {
    let mut data = ValueMap::new();
    data.insert("name".to_owned(), Value::str("Nozzle <XL>"));
    Ok(data)
  });
  let mut evaluators = Evaluators::new();
  evaluators.register(|m: &ModuleId| m.path == "shell", Arc::new(Shell));
  evaluators.register(|m: &ModuleId| m.path == "page", Arc::new(Page));
  Runtime::builder().sources(sources).evaluators(evaluators).meta("product", Arc::new(ProductMeta)).build()
}

fn plan(deferred: bool) -> PlanNode {
  let mut page = PlanNode::new(NodeId(1), ModuleId::new("page", "default"));
  page.data_source = Some(DataSourceId("product".into()));
  page.deferred = deferred;
  let mut shell = PlanNode::new(NodeId(0), ModuleId::new("shell", "document"));
  shell.children.push((SlotName("content".into()), page));
  shell
}

fn head() -> Head {
  let mut head = Head::new("Shop", Node::raw("<meta charset=\"utf-8\">"));
  head.description = Some("A shop".to_owned());
  head
}

#[test]
fn a_described_segment_titles_the_document_over_the_defaults() {
  let assembly = block_on(assemble(&runtime(), &plan(false), &RequestCtx::anonymous(Params::new()), head())).unwrap();
  assert_eq!(assembly.meta, Meta { title: Some("Nozzle <XL> · Shop".into()), description: Some("All about Nozzle <XL>".into()), head: Vec::new() });
  let html: String = block_on(html_stream(assembly).collect::<Vec<_>>()).concat();
  assert!(html.contains("<meta charset=\"utf-8\"><title>Nozzle &lt;XL&gt; · Shop</title><meta name=\"description\" content=\"All about Nozzle &lt;XL&gt;\"></head>"), "{html}");
}

#[test]
fn without_a_described_segment_the_head_keeps_its_defaults() {
  let mut sources = DataSources::new();
  sources.insert_fn("product", |_p| async move { Ok(ValueMap::new()) });
  let mut evaluators = Evaluators::new();
  evaluators.register(|m: &ModuleId| m.path == "shell", Arc::new(Shell));
  evaluators.register(|m: &ModuleId| m.path == "page", Arc::new(Page));
  let rt = Runtime::builder().sources(sources).evaluators(evaluators).build();
  let assembly = block_on(assemble(&rt, &plan(false), &RequestCtx::anonymous(Params::new()), head())).unwrap();
  assert_eq!(assembly.meta, Meta { title: Some("Shop".into()), description: Some("A shop".into()), head: Vec::new() });
  let wire: String = block_on(wire_stream(assembly).collect::<Vec<_>>()).concat();
  assert!(wire.contains("\nH {\"title\":\"Shop\",\"description\":\"A shop\"}\n"), "{wire}");
}

#[test]
fn a_deferred_segment_describes_the_document_when_it_resolves() {
  let rt = runtime();
  let assembly = block_on(assemble(&rt, &plan(true), &RequestCtx::anonymous(Params::new()), head())).unwrap();
  assert_eq!(assembly.meta.title.as_deref(), Some("Shop"), "the eager wave only has the defaults");
  let wire: Vec<String> = block_on(wire_stream(assembly).collect());
  assert!(wire[0].contains("\nH {\"title\":\"Shop\",\"description\":\"A shop\"}\n"), "{}", wire[0]);
  assert!(wire[1].starts_with("S 1 "), "{}", wire[1]);
  assert!(wire[1].ends_with("\nH {\"title\":\"Nozzle <XL> · Shop\",\"description\":\"All about Nozzle <XL>\"}\n"), "{}", wire[1]);

  let assembly = block_on(assemble(&rt, &plan(true), &RequestCtx::anonymous(Params::new()), head())).unwrap();
  let html: Vec<String> = block_on(html_stream(assembly).collect());
  assert!(html[0].contains("<title>Shop</title>"), "{}", html[0]);
  assert!(html[1].ends_with("<script>__sfFill(1);__sfHead({\"title\":\"Nozzle \\u003cXL> · Shop\",\"description\":\"All about Nozzle \\u003cXL>\"})</script>"), "{}", html[1]);
}

fn el(tag: &str, attrs: &[(&str, &str)]) -> HeadEl {
  HeadEl { tag: tag.to_owned(), attrs: attrs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect(), children: None }
}

#[test]
fn an_inner_segment_replaces_a_head_element_in_place_and_appends_the_rest() {
  let mut outer = Meta {
    title: Some("Shop".into()),
    description: Some("A shop".into()),
    head: vec![el("meta", &[("property", "og:site_name"), ("content", "Shop")]), el("meta", &[("property", "og:type"), ("content", "website")])],
  };
  outer.merge(Meta {
    title: Some("Nozzle".into()),
    description: None,
    head: vec![el("meta", &[("property", "og:type"), ("content", "product")]), el("link", &[("rel", "canonical"), ("href", "/p/1")])],
  });

  assert_eq!(outer.title.as_deref(), Some("Nozzle"));
  assert_eq!(outer.description.as_deref(), Some("A shop"), "an absent inner field leaves the outer one");
  let rendered: Vec<String> = outer.head.iter().map(|e| { let mut s = String::new(); e.render(&mut s); s }).collect();
  assert_eq!(
    rendered,
    vec![
      "<meta property=\"og:site_name\" content=\"Shop\">".to_owned(),
      "<meta property=\"og:type\" content=\"product\">".to_owned(),
      "<link rel=\"canonical\" href=\"/p/1\">".to_owned(),
    ]
  );
}

#[test]
fn icons_of_different_sizes_are_different_elements() {
  let mut outer = Meta { title: None, description: None, head: vec![el("link", &[("rel", "icon"), ("sizes", "32x32"), ("href", "/a.png")])] };
  outer.merge(Meta {
    title: None,
    description: None,
    head: vec![el("link", &[("rel", "icon"), ("sizes", "16x16"), ("href", "/b.png")]), el("link", &[("rel", "icon"), ("sizes", "32x32"), ("href", "/c.png")])],
  });
  let hrefs: Vec<&str> = outer.head.iter().filter_map(|e| e.attrs.iter().find(|(k, _)| k == "href").map(|(_, v)| v.as_str())).collect();
  assert_eq!(hrefs, vec!["/c.png", "/b.png"], "32x32 was replaced in place, 16x16 appended");
}

#[test]
fn an_element_with_content_closes_its_tag() {
  let mut script = el("script", &[("type", "application/ld+json")]);
  script.children = Some("{\"@type\":\"Article\"}".to_owned());
  let mut out = String::new();
  script.render(&mut out);
  assert_eq!(out, "<script type=\"application/ld+json\">{\"@type\":\"Article\"}</script>");
}
