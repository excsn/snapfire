use std::sync::Arc;

use futures::executor::block_on;
use futures_util::future::BoxFuture;
use futures_util::{StreamExt, stream};
use snapfire_fsr_core::{Data, DataSourceId, ModuleId, Node, NodeId, Params, PlanNode, SlotName, Value, ValueMap};
use snapfire_fsr_runtime::{
  Chunk, DataSources, Evaluator, Evaluators, Head, LoadError, NodeChunks, RequestCtx, Runtime, Seeds, assemble,
  html_stream, wire_stream,
};

struct Shell;

impl Evaluator for Shell {
  fn evaluate(&self, _module: &ModuleId, props: &Data) -> NodeChunks {
    Box::pin(stream::iter([
      Ok(Chunk::Node(Node::raw(format!("<body data-pending=\"{}\">", pending_of(props))))),
      Ok(Chunk::Slot(SlotName("content".into()))),
      Ok(Chunk::Node(Node::raw("</body>"))),
    ]))
  }
}

/// Renders whatever `$store` reached it, which is what a lowered `useStore` reads.
struct Page;

impl Evaluator for Page {
  fn evaluate(&self, _module: &ModuleId, props: &Data) -> NodeChunks {
    let seen = match props.get("$store") {
      Some(Value::Map(store)) => format!(
        "{:?}",
        store.iter().map(|(k, v)| (k.clone(), v.clone())).collect::<Vec<_>>()
      ),
      _ => "none".to_owned(),
    };
    Box::pin(stream::iter([Ok(Chunk::Node(Node::raw(format!("<p data-pending=\"{}\">{seen}</p>", pending_of(props)))))]))
  }
}

/// The pending keys a node was handed, comma-joined: what a held island's reads are checked against.
fn pending_of(props: &Data) -> String {
  match props.get(snapfire_fsr_runtime::PENDING_PROP) {
    Some(Value::Seq(keys)) => keys.iter().map(|k| match k {
      Value::Str(k) => k.to_string(),
      other => format!("{other:?}"),
    }).collect::<Vec<_>>().join(","),
    _ => String::new(),
  }
}

/// `FieldSeed` that says which key it seeds before its data is known, as a lowered `store` does.
struct KnownSeed(&'static str, &'static str);

impl Seeds for KnownSeed {
  fn seed(&self, ctx: &RequestCtx, data: &Data) -> BoxFuture<'static, Result<Data, LoadError>> {
    FieldSeed(self.0, self.1).seed(ctx, data)
  }

  fn keys(&self) -> Option<Vec<String>> {
    Some(vec![self.0.to_owned()])
  }
}

/// `Failing` that names its key, so a deferred segment using it promises one.
struct KnownFailing;

impl Seeds for KnownFailing {
  fn seed(&self, ctx: &RequestCtx, data: &Data) -> BoxFuture<'static, Result<Data, LoadError>> {
    Failing.seed(ctx, data)
  }

  fn keys(&self) -> Option<Vec<String>> {
    Some(vec!["owner".to_owned()])
  }
}

/// Seeds one key from a field of its segment's data.
struct FieldSeed(&'static str, &'static str);

impl Seeds for FieldSeed {
  fn seed(&self, _ctx: &RequestCtx, data: &Data) -> BoxFuture<'static, Result<Data, LoadError>> {
    let (key, field) = (self.0.to_owned(), self.1.to_owned());
    let value = data.get(&field).cloned().unwrap_or(Value::Null);
    Box::pin(async move {
      let mut out = Data::default();
      out.insert(key, value);
      Ok(out)
    })
  }
}

struct Failing;

impl Seeds for Failing {
  fn seed(&self, _ctx: &RequestCtx, _data: &Data) -> BoxFuture<'static, Result<Data, LoadError>> {
    Box::pin(async move {
      Err(LoadError {
        source_id: "page".to_owned(),
        message: "no".to_owned(),
        kind: snapfire_fsr_runtime::FailureKind::Internal,
      })
    })
  }
}

fn runtime(page_seeds: Option<Arc<dyn Seeds>>) -> Arc<Runtime> {
  runtime_with(page_seeds, false)
}

fn runtime_with(page_seeds: Option<Arc<dyn Seeds>>, page_fails: bool) -> Arc<Runtime> {
  let mut sources = DataSources::new();
  sources.insert_fn("layout", |_p| async move {
    let mut data = ValueMap::default();
    data.insert("count".to_owned(), Value::Int(2));
    data.insert("where".to_owned(), Value::str("layout"));
    Ok(data)
  });
  sources.insert_fn("page", move |_p| async move {
    if page_fails {
      return Err(LoadError { source_id: "page".to_owned(), message: "down".to_owned(), kind: snapfire_fsr_runtime::FailureKind::Internal });
    }
    let mut data = ValueMap::default();
    data.insert("where".to_owned(), Value::str("page"));
    Ok(data)
  });
  let mut evaluators = Evaluators::new();
  evaluators.register(|m: &ModuleId| m.path == "shell", Arc::new(Shell));
  evaluators.register(|m: &ModuleId| m.path == "page", Arc::new(Page));
  let mut runtime = Runtime::builder()
    .sources(sources)
    .evaluators(evaluators)
    .store("layout", Arc::new(LayoutSeed));
  if let Some(seeds) = page_seeds {
    runtime = runtime.store("page", seeds);
  }
  runtime.build()
}

/// Two keys, so an inner segment can win one and leave the other.
struct LayoutSeed;

impl Seeds for LayoutSeed {
  fn seed(&self, _ctx: &RequestCtx, data: &Data) -> BoxFuture<'static, Result<Data, LoadError>> {
    let count = data.get("count").cloned().unwrap_or(Value::Null);
    let owner = data.get("where").cloned().unwrap_or(Value::Null);
    Box::pin(async move {
      let mut out = Data::default();
      out.insert("cart/count".to_owned(), count);
      out.insert("owner".to_owned(), owner);
      Ok(out)
    })
  }
}

fn plan(deferred: bool) -> PlanNode {
  let mut page = PlanNode::new(NodeId(1), ModuleId::new("page", "default"));
  page.data_source = Some(DataSourceId("page".into()));
  page.deferred = deferred;
  let mut shell = PlanNode::new(NodeId(0), ModuleId::new("shell", "document"));
  shell.data_source = Some(DataSourceId("layout".into()));
  shell.children.push((SlotName("content".into()), page));
  shell
}

fn head() -> Head {
  Head::new("Shop", Node::raw(""))
}

fn assembled(page_seeds: Option<Arc<dyn Seeds>>, deferred: bool) -> snapfire_fsr_runtime::Assembly {
  block_on(assemble(
    &runtime(page_seeds),
    &plan(deferred),
    &RequestCtx::anonymous(Params::new()),
    head(),
  ))
  .unwrap()
}

#[test]
fn a_seeding_segment_reaches_the_wire_and_the_document() {
  let assembly = assembled(None, false);
  assert_eq!(assembly.store.get("cart/count"), Some(&Value::Int(2)));
  let wire: String = block_on(wire_stream(assembly).collect::<Vec<_>>()).concat();
  assert!(wire.contains(&format!("\nT {SHELL_SEED}\n")), "{wire}");

  let html: String = block_on(html_stream(assembled(None, false)).collect::<Vec<_>>()).concat();
  assert!(html.contains(&format!("<script type=\"application/json\" data-sf-store>{SHELL_SEED}</script>")), "{html}");
}

/// The shell's contribution as the streams write it: the segment's key, its slot path from the root and what it seeded.
const SHELL_SEED: &str = "[{\"k\":\"shell#document\",\"p\":[],\"v\":{\"cart/count\":2,\"owner\":\"layout\"}}]";
const PAGE_SEED: &str = "[{\"k\":\"page#default\",\"p\":[\"content\"],\"v\":{\"owner\":\"page\"}}]";

#[test]
fn an_inner_segment_wins_the_key_it_shares_with_an_outer_one() {
  let assembly = assembled(Some(Arc::new(FieldSeed("owner", "where"))), false);
  assert_eq!(assembly.store.get("owner"), Some(&Value::str("page")));
  assert_eq!(
    assembly.store.get("cart/count"),
    Some(&Value::Int(2)),
    "the key only the layout sets survives"
  );
}

#[test]
fn every_node_renders_with_the_seed_as_a_prop() {
  let html: String = block_on(html_stream(assembled(None, false)).collect::<Vec<_>>()).concat();
  assert!(html.contains("(\"cart/count\", Int(2))"), "{html}");
  assert!(html.contains("(\"owner\", Str(\"layout\"))"), "{html}");
}

#[test]
fn a_failing_seed_costs_its_keys_and_not_the_page() {
  let assembly = assembled(Some(Arc::new(Failing)), false);
  assert_eq!(assembly.store.get("cart/count"), Some(&Value::Int(2)));
  let html: String = block_on(html_stream(assembly).collect::<Vec<_>>()).concat();
  assert!(html.contains("<p data-pending=\"\">"), "{html}");
  assert!(html.contains("{\"k\":\"page#default\",\"p\":[\"content\"],\"v\":{}}"), "it seeds nothing, so the browser drops what the segment seeded before: {html}");
}

#[test]
fn a_deferred_segment_seeds_when_it_resolves() {
  let wire: Vec<String> = block_on(wire_stream(assembled(Some(Arc::new(FieldSeed("owner", "where"))), true)).collect());
  assert!(wire[0].contains(&format!("\nT {SHELL_SEED}\n")), "{}", wire[0]);
  assert!(wire[1].starts_with(&format!("T {PAGE_SEED}\nS 1 ")), "the seed goes ahead of the markup it rendered: {}", wire[1]);

  let html: Vec<String> = block_on(html_stream(assembled(Some(Arc::new(FieldSeed("owner", "where"))), true)).collect());
  assert!(html[1].ends_with(&format!("<script>__sfStore({PAGE_SEED});__sfFill(1)</script>")), "{}", html[1]);
}

#[test]
fn a_deferred_segment_renders_from_the_wave_around_it_plus_its_own_seed() {
  let html: Vec<String> = block_on(html_stream(assembled(Some(Arc::new(FieldSeed("owner", "where"))), true)).collect());
  assert!(html[1].contains("(\"cart/count\", Int(2))"), "the layout's key reaches the deferred page: {}", html[1]);
  assert!(html[1].contains("(\"owner\", Str(\"page\"))"), "its own seed wins the key it shares: {}", html[1]);
  assert!(!html[1].contains("Str(\"layout\")"), "{}", html[1]);
}

#[test]
fn contributions_merge_by_position_whatever_order_they_came_in() {
  use snapfire_fsr_runtime::{Contribution, merge_contributions};
  let at = |segment: &str, path: &[&str], key: &str, value: &str| {
    Contribution::seeded(segment, path.iter().map(|p| (*p).to_owned()).collect(), [(key.to_owned(), Value::str(value))].into_iter().collect())
  };
  let layout = at("layout", &[], "owner", "layout");
  let page = at("page", &["content"], "owner", "page");
  let modal = at("modal", &["modal"], "owner", "modal");
  let inner = at("inner", &["content", "content"], "owner", "inner");
  for order in [vec![&layout, &page, &modal, &inner], vec![&inner, &modal, &page, &layout], vec![&modal, &layout, &inner, &page]] {
    let merged = merge_contributions(&order.into_iter().cloned().collect::<Vec<_>>(), &[]);
    assert_eq!(merged.get("owner"), Some(&Value::str("inner")), "the deepest segment wins");
  }
  let siblings = merge_contributions(&[modal.clone(), page.clone()], &[]);
  assert_eq!(siblings.get("owner"), Some(&Value::str("modal")), "at one depth the later slot name wins, whichever resolved first");
  let siblings = merge_contributions(&[page.clone(), modal.clone()], &[]);
  assert_eq!(siblings.get("owner"), Some(&Value::str("modal")));

  let page_last = ["content".to_owned()];
  assert_eq!(merge_contributions(&[modal.clone(), page.clone()], &page_last).get("owner"), Some(&Value::str("page")), "a slot the order names beats one it leaves out");
  let both = ["modal".to_owned(), "content".to_owned()];
  assert_eq!(merge_contributions(&[page.clone(), modal.clone()], &both).get("owner"), Some(&Value::str("page")), "the later name in the order wins");
  let other = at("promo", &["promo"], "owner", "promo");
  assert_eq!(merge_contributions(&[other.clone(), modal.clone()], &["content".to_owned()]).get("owner"), Some(&Value::str("promo")), "slots the order leaves out stand in name order among themselves");
  assert_eq!(merge_contributions(&[other, inner], &both).get("owner"), Some(&Value::str("inner")), "depth still comes first");
}

fn streamed(page_seeds: Arc<dyn Seeds>, page_fails: bool) -> (Vec<String>, Vec<String>) {
  let ctx = RequestCtx::anonymous(Params::new());
  let wire = block_on(wire_stream(block_on(assemble(&runtime_with(Some(page_seeds.clone()), page_fails), &plan(true), &ctx, head())).unwrap()).collect());
  let html = block_on(html_stream(block_on(assemble(&runtime_with(Some(page_seeds), page_fails), &plan(true), &ctx, head())).unwrap()).collect());
  (wire, html)
}

/// The page's promise as the streams write it: no values and the key it will seed.
const PAGE_PROMISE: &str = "{\"k\":\"page#default\",\"p\":[\"content\"],\"v\":{},\"w\":[\"owner\"]}";

#[test]
fn a_deferred_segment_promises_its_keys_in_the_first_wave_and_keeps_the_promise_when_it_resolves() {
  let (wire, html) = streamed(Arc::new(KnownSeed("owner", "where")), false);
  assert!(wire[0].contains(PAGE_PROMISE), "the first wave says the page will seed `owner`: {}", wire[0]);
  assert!(html[0].contains(PAGE_PROMISE), "and so does the document's seed: {}", html[0]);
  assert!(wire[1].starts_with(&format!("T {PAGE_SEED}\nS 1 ")), "the seed that keeps it lands ahead of the markup: {}", wire[1]);
}

#[test]
fn a_node_rendered_before_the_seed_lands_is_told_the_key_is_pending_and_the_resolution_is_not() {
  let (_, html) = streamed(Arc::new(KnownSeed("owner", "where")), false);
  assert!(html[0].contains("<body data-pending=\"owner\">"), "the shell renders while `owner` is the layout's and about to be the page's: {}", html[0]);
  assert!(html[1].contains("<p data-pending=\"\">"), "the page renders with its own seed, so nothing is pending for it: {}", html[1]);
}

#[test]
fn a_segment_whose_keys_are_not_known_promises_nothing() {
  let (wire, html) = streamed(Arc::new(FieldSeed("owner", "where")), false);
  assert!(!wire[0].contains("\"w\""), "{}", wire[0]);
  assert!(html[0].contains("<body data-pending=\"\">"), "nothing is held: {}", html[0]);
}

#[test]
fn a_promise_is_kept_with_no_values_when_the_seed_fails_or_the_loader_does() {
  let empty = "T [{\"k\":\"page#default\",\"p\":[\"content\"],\"v\":{}}]\nS 1 ";
  let (wire, _) = streamed(Arc::new(KnownFailing), false);
  assert!(wire[1].starts_with(empty), "a failing seed still lands, empty, so the browser stops waiting: {}", wire[1]);
  let (wire, _) = streamed(Arc::new(KnownSeed("owner", "where")), true);
  assert!(wire[1].starts_with(empty), "a failing loader too: {}", wire[1]);
}

#[test]
fn a_promise_makes_a_key_pending_only_where_it_outranks_every_seed_of_it() {
  use snapfire_fsr_runtime::{Contribution, pending_keys};
  let seed = |segment: &str, path: &[&str]| Contribution::seeded(segment, path.iter().map(|p| (*p).to_owned()).collect(), [("k".to_owned(), Value::str(segment))].into_iter().collect());
  let promise = |segment: &str, path: &[&str]| Contribution::promised(segment, path.iter().map(|p| (*p).to_owned()).collect(), vec!["k".to_owned()]);
  let pending = |list: Vec<Contribution>, order: &[&str]| pending_keys(&list, &order.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>()).into_iter().collect::<Vec<_>>();

  assert_eq!(pending(vec![seed("layout", &[]), promise("page", &["content"])], &[]), ["k"], "a deeper promise outranks the layout");
  assert_eq!(pending(vec![promise("page", &["content"])], &[]), ["k"], "a key nothing has seeded yet");
  assert!(pending(vec![promise("layout", &[]), seed("page", &["content"])], &[]).is_empty(), "a deeper seed already wins it");
  assert!(pending(vec![promise("page", &["content"]), seed("page", &["content"])], &[]).is_empty(), "a seed of its own segment keeps the promise");
  assert!(pending(vec![promise("alpha", &["alpha"]), seed("beta", &["beta"])], &[]).is_empty(), "by name, beta wins the tie");
  assert_eq!(pending(vec![promise("alpha", &["alpha"]), seed("beta", &["beta"])], &["beta", "alpha"]), ["k"], "the slot order makes alpha win it");
}
