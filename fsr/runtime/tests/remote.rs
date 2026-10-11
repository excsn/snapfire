//! A plan node another host renders: its eager wave is spliced in place
//! under the remote's own segment key, its fills stream through under slot
//! ids this assembly allocated and what it described, seeded and needs
//! loaded reaches the assembly.

use std::sync::Arc;

use futures::executor::block_on;
use futures_util::future::BoxFuture;
use futures_util::stream::{self, StreamExt};
use snapfire_fsr_core::ext::FailureKind;
use snapfire_fsr_core::{Data, ModuleId, Node, NodeId, Params, PlanNode, SlotName, Value};
use snapfire_fsr_runtime::wire::RowReader;
use snapfire_fsr_runtime::{
  html_stream, wire_stream, Chunk, Evaluator, Evaluators, LoadError, NodeChunks, RemoteSegment, RemoteWave, RequestCtx, Runtime,
};

struct Shell;

impl Evaluator for Shell {
  fn evaluate(&self, _module: &ModuleId, _props: &Data) -> NodeChunks {
    Box::pin(stream::iter([
      Ok(Chunk::Node(Node::raw("<shell>"))),
      Ok(Chunk::Slot(SlotName("chart".into()))),
      Ok(Chunk::Slot(SlotName("content".into()))),
      Ok(Chunk::Node(Node::raw("</shell>"))),
    ]))
  }
}

struct Leaf(&'static str);

impl Evaluator for Leaf {
  fn evaluate(&self, _module: &ModuleId, _props: &Data) -> NodeChunks {
    Box::pin(stream::iter([Ok(Chunk::Node(Node::raw(self.0)))]))
  }
}

/// A remote answering with recorded rows, or refusing.
struct Recorded(Result<&'static str, FailureKind>);

impl RemoteSegment for Recorded {
  fn fetch(&self, ctx: &RequestCtx) -> BoxFuture<'static, Result<RemoteWave, LoadError>> {
    assert_eq!(ctx.path, "/reports/invoices");
    let answer = self.0;
    Box::pin(async move {
      let text = answer.map_err(|kind| LoadError::new("reports", "down").with_kind(kind))?;
      let rows = RowReader::new().push(text.as_bytes()).unwrap();
      RemoteWave::from_rows("reports", stream::iter(rows.into_iter().map(Ok)).boxed()).await
    })
  }
}

/// The rows a site host answers `/reports/invoices?__payload` with: its own
/// document around a layout around a page, the page deferred under the
/// remote's slot 1, seeding the store and titling the document on the way.
const ROWS: &str = concat!(
  "V {\"fmt\":2,\"enc\":\"json\"}\n",
  "N [\"q\",[[\"r\",\"<rdoc>\"],[\"q\",[[\"r\",\"<layout>\"],[\"p\",1,[\"r\",\"<loading>\"]]]]]]\n",
  "H {\"title\":\"Invoices\"}\n",
  "T [{\"k\":\"reports:routes/layout.tsx#default\",\"p\":[\"content\"],\"v\":{\"reports/open\":\"3\"}}]\n",
  "E \"/reports/static/js/app/src/main.js\"\n",
  "C [\"/reports/static/css/site.css\"]\n",
  "G {\"k\":\"reports:doc\",\"d\":\"0000000000000001\",\"c\":[{\"k\":\"reports:routes/layout.tsx#default\",\"d\":\"0000000000000002\",\"n\":\"content\",\"p\":[1],\"c\":[{\"k\":\"reports:routes/invoices/page.tsx#default\",\"n\":\"content\",\"s\":1,\"c\":[]}]}]}\n",
  "T [{\"k\":\"reports:routes/invoices/page.tsx#default\",\"p\":[\"content\",\"content\"],\"v\":{\"reports/count\":\"12\"}}]\n",
  "S 1 {\"n\":[\"r\",\"<page>\"],\"g\":[]}\n",
  "H {\"title\":\"Invoice 1\"}\n",
);

fn plan() -> PlanNode {
  let mut plan = PlanNode::new(NodeId(0), ModuleId::new("shell.tera", "default"));
  let mut chart = PlanNode::new(NodeId(1), ModuleId::new("chart.tera", "default"));
  chart.deferred = true;
  chart.fallback = Some(ModuleId::new("loading.tera", "default"));
  plan.children.push((SlotName("chart".into()), chart));
  plan.children.push((SlotName("content".into()), PlanNode::new(NodeId(2), ModuleId::new("reports:@remote", "site"))));
  plan
}

fn runtime(remote: Recorded) -> Arc<Runtime> {
  let mut evs = Evaluators::new();
  evs.register(|m: &ModuleId| m.path == "shell.tera", Arc::new(Shell));
  evs.register(|m: &ModuleId| m.path == "chart.tera", Arc::new(Leaf("<chart>")));
  evs.register(|m: &ModuleId| m.path == "loading.tera", Arc::new(Leaf("<wait>")));
  Runtime::builder().evaluators(evs).remote("reports:@remote#site", Arc::new(remote)).build()
}

fn ctx() -> RequestCtx {
  let mut ctx = RequestCtx::anonymous(Params::new());
  ctx.path = "/reports/invoices".to_owned();
  ctx
}

#[test]
fn the_remotes_wave_is_spliced_in_place_and_its_fill_streams_under_a_slot_this_assembly_allocated() {
  let assembly = block_on(snapfire_fsr_runtime::assemble(&runtime(Recorded(Ok(ROWS))), &plan(), &ctx(), &Node::raw(""))).unwrap();
  assert_eq!(assembly.meta.title.as_deref(), Some("Invoices"));
  assert_eq!(assembly.entry.as_deref(), Some("/reports/static/js/app/src/main.js"));
  assert_eq!(assembly.styles, ["/reports/static/css/site.css"]);
  let seeded = &assembly.contributions;
  assert_eq!(seeded.len(), 1, "{seeded:?}");
  assert_eq!(seeded[0].segment, "reports:routes/layout.tsx#default");
  assert_eq!(seeded[0].path, ["content"], "the remote's own content slot is stripped and the node's path put in front");
  assert_eq!(seeded[0].values.get("reports/open"), Some(&Value::str("3")));
  let content = &assembly.segments.children[1];
  assert_eq!(content.key, "reports:routes/layout.tsx#default", "the remote's key, not the plan node's");
  assert_eq!(content.digest, 2);
  assert_eq!(content.children[0].slot, Some(2), "the shell's own deferred chart took slot 1");
  assert_eq!(assembly.pending.len(), 2);

  let rows: String = block_on(wire_stream(assembly).collect::<Vec<_>>()).concat();
  assert!(rows.contains("[\"p\",2,[\"r\",\"<loading>\"]]"), "{rows}");
  assert!(rows.contains("S 2 {\"n\":[\"r\",\"<page>\"]"), "{rows}");
  assert!(rows.contains("T [{\"k\":\"reports:routes/invoices/page.tsx#default\",\"p\":[\"content\",\"content\"],\"v\":{\"reports/count\":\"12\"}}]\nS 2"), "{rows}");
  assert!(rows.contains("S 2 {\"n\":[\"r\",\"<page>\"]}\nH {\"title\":\"Invoice 1\"}"), "{rows}");
}

#[test]
fn the_document_carries_the_remotes_markup_inline_and_its_fill_as_a_template() {
  let assembly = block_on(snapfire_fsr_runtime::assemble(&runtime(Recorded(Ok(ROWS))), &plan(), &ctx(), &Node::raw(""))).unwrap();
  let html: String = block_on(html_stream(assembly).collect::<Vec<_>>()).concat();
  let first = html.split("<template").next().unwrap();
  assert!(first.contains("<layout>"), "{first}");
  assert!(!first.contains("<rdoc>"), "only the content subtree crosses: {first}");
  assert!(first.contains("data-sf-slot=\"2\""), "{first}");
  assert!(html.contains("<template data-sf-fill=\"2\"><!--sf-g:reports:routes/invoices/page.tsx#default--><page><!--/sf-g--></template>"), "{html}");
  assert!(html.contains("<template data-sf-fill=\"1\"><!--sf-g:chart.tera#default--><chart><!--/sf-g--></template>"), "{html}");
}

#[test]
fn a_remote_that_is_down_renders_the_nodes_error_and_fails_the_page_with_its_kind() {
  let assembly = block_on(snapfire_fsr_runtime::assemble(&runtime(Recorded(Err(FailureKind::Unavailable))), &plan(), &ctx(), &Node::raw(""))).unwrap();
  assert_eq!(assembly.failed, Some(FailureKind::Unavailable));
  assert_eq!(assembly.pending.len(), 1, "only the shell's own chart");
  let html: String = block_on(html_stream(assembly).collect::<Vec<_>>()).concat();
  assert!(html.contains("<div data-sf-error>"), "{html}");
}
