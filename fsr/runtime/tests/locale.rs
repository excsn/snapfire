use std::sync::Arc;

use futures::executor::block_on;
use futures_util::{stream, StreamExt};
use snapfire_fsr_core::{Data, ModuleId, Node, NodeId, Params, PlanNode, SlotName, Value};
use snapfire_fsr_runtime::{assemble, html_stream, wire_stream, Chunk, DataSources, Evaluator, Evaluators, Head, Locale, NodeChunks, RequestCtx, Runtime};

/// Renders the `locale` prop the assembler injects.
struct Page;

impl Evaluator for Page {
  fn evaluate(&self, _module: &ModuleId, props: &Data) -> NodeChunks {
    let seen = match props.get("locale") {
      Some(Value::Str(tag)) => tag.clone(),
      _ => "none".to_owned(),
    };
    Box::pin(stream::iter([Ok(Chunk::Node(Node::raw(format!("<p>{seen}</p>"))))]))
  }
}

struct Shell;

impl Evaluator for Shell {
  fn evaluate(&self, _module: &ModuleId, _props: &Data) -> NodeChunks {
    Box::pin(stream::iter([Ok(Chunk::Node(Node::raw("<body>"))), Ok(Chunk::Slot(SlotName("content".into()))), Ok(Chunk::Node(Node::raw("</body>")))]))
  }
}

fn runtime() -> Arc<Runtime> {
  let mut evaluators = Evaluators::new();
  evaluators.register(|m: &ModuleId| m.path == "shell", Arc::new(Shell));
  evaluators.register(|m: &ModuleId| m.path == "page", Arc::new(Page));
  Runtime::builder().sources(DataSources::new()).evaluators(evaluators).build()
}

fn plan() -> PlanNode {
  let mut shell = PlanNode::new(NodeId(0), ModuleId::new("shell", "document"));
  shell.children.push((SlotName("content".into()), PlanNode::new(NodeId(1), ModuleId::new("page", "default"))));
  shell
}

fn ctx(locale: Locale) -> RequestCtx {
  RequestCtx { locale, ..RequestCtx::anonymous(Params::new()) }
}

fn render(locale: Locale) -> (String, String) {
  let wire: String = block_on(wire_stream(block_on(assemble(&runtime(), &plan(), &ctx(locale.clone()), Head::new("t", Node::raw("")))).unwrap()).collect::<Vec<_>>()).concat();
  let html: String = block_on(html_stream(block_on(assemble(&runtime(), &plan(), &ctx(locale), Head::new("t", Node::raw("")))).unwrap()).collect::<Vec<_>>()).concat();
  (wire, html)
}

#[test]
fn a_locale_other_than_the_default_marks_every_segment_key_and_the_wire_names_it() {
  let (wire, html) = render(Locale::new("fr_FR", false));
  assert!(wire.contains("\nL \"fr_FR\"\n"), "{wire}");
  assert!(wire.contains("\"k\":\"shell#document@fr_FR\""), "{wire}");
  assert!(wire.contains("\"k\":\"page#default@fr_FR\""), "{wire}");
  assert!(html.contains("<!--sf-g:page#default@fr_FR--><p>fr_FR</p>"), "the page rendered with the locale as a prop: {html}");
}

#[test]
fn the_default_locale_leaves_the_keys_bare_and_still_reaches_the_page() {
  let (wire, html) = render(Locale::new("en_US", true));
  assert!(wire.contains("\nL \"en_US\"\n"), "{wire}");
  assert!(wire.contains("\"k\":\"shell#document\""), "{wire}");
  assert!(html.contains("<!--sf-g:page#default--><p>en_US</p>"), "{html}");
}

#[test]
fn a_context_without_a_locale_writes_no_row_and_no_prop() {
  let (wire, html) = render(Locale::default());
  assert!(!wire.contains("\nL "), "{wire}");
  assert!(html.contains("<p>none</p>"), "{html}");
}

#[test]
fn the_hyphenated_form_and_the_key_suffix() {
  assert_eq!(Locale::new("fr_FR", false).hyphenated(), "fr-FR");
  assert_eq!(Locale::new("fr", false).hyphenated(), "fr");
  assert_eq!(Locale::new("fr_FR", false).key_suffix(), "@fr_FR");
  assert_eq!(Locale::new("fr_FR", true).key_suffix(), "");
  assert_eq!(Locale::default().key_suffix(), "");
}
