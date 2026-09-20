use futures::StreamExt;
use snapfire_fsr_core::{Data, ModuleId, Node, SlotName, Value};
use snapfire_fsr_runtime::{Chunk, EvalError, Evaluator};
use snapfire_fsr_tera::{register_markers, TeraEvaluator, MARKER};

fn evaluator(templates: &[(&str, &str)]) -> TeraEvaluator {
  let mut tera = tera::Tera::new();
  register_markers(&mut tera);
  tera.add_raw_templates(templates.to_vec()).expect("templates parse");
  TeraEvaluator::new(tera)
}

fn render(ev: &TeraEvaluator, path: &str, props: Data) -> Result<Vec<Chunk>, EvalError> {
  let module: ModuleId = format!("{path}#default").parse().expect("module id");
  futures::executor::block_on(async {
    ev.evaluate(&module, &props).collect::<Vec<_>>().await.into_iter().collect()
  })
}

fn one(ev: &TeraEvaluator, path: &str) -> Result<Vec<Chunk>, EvalError> {
  render(ev, path, Data::default())
}

fn raw(chunk: &Chunk) -> &str {
  match chunk {
    Chunk::Node(Node::Raw(html)) => &html.0,
    other => panic!("not a raw chunk: {other:?}"),
  }
}

fn island(chunk: &Chunk) -> (&ModuleId, &Data) {
  match chunk {
    Chunk::Node(Node::Client { module, props, .. }) => (module, props),
    other => panic!("not an island chunk: {other:?}"),
  }
}

#[test]
fn text_islands_and_slots_come_back_in_source_order() {
  let ev = evaluator(&[(
    "page.tera",
    r#"<main>{{ island(module="ui/Chart.tsx#default", props={"series": "cpu"}) }}<hr>{{ slot(name="content") }}</main>"#,
  )]);

  let chunks = one(&ev, "page.tera").expect("renders");
  assert_eq!(chunks.len(), 5, "{chunks:#?}");
  assert_eq!(raw(&chunks[0]), "<main>");
  let (module, props) = island(&chunks[1]);
  assert_eq!(module.to_string(), "ui/Chart.tsx#default");
  assert_eq!(props.get("series"), Some(&Value::str("cpu".to_owned())));
  assert_eq!(raw(&chunks[2]), "<hr>");
  assert_eq!(chunks[3], Chunk::Slot(SlotName("content".to_owned())));
  assert_eq!(raw(&chunks[4]), "</main>");
}

#[test]
fn a_template_that_is_only_a_slot_yields_only_that_slot() {
  let ev = evaluator(&[("bare.tera", r#"{{ slot(name="content") }}"#)]);
  assert_eq!(one(&ev, "bare.tera").expect("renders"), vec![Chunk::Slot(SlotName("content".to_owned()))]);
}

#[test]
fn head_is_the_head_slot() {
  let ev = evaluator(&[("doc.tera", "{{ head() }}")]);
  assert_eq!(one(&ev, "doc.tera").expect("renders"), vec![Chunk::Slot(SlotName("head".to_owned()))]);
}

#[test]
fn an_island_without_props_carries_an_empty_map() {
  let ev = evaluator(&[("page.tera", r#"{{ island(module="ui/Clock.tsx#default") }}"#)]);
  let chunks = one(&ev, "page.tera").expect("renders");
  let (module, props) = island(&chunks[0]);
  assert_eq!(module.to_string(), "ui/Clock.tsx#default");
  assert!(props.is_empty(), "{props:?}");
}

#[test]
fn props_reach_the_template_by_type() {
  let ev = evaluator(&[("page.tera", "{{ name }}|{{ count }}|{{ on }}|{{ tags[1] }}|{{ user.city }}")]);

  let mut props = Data::default();
  props.insert("name".to_owned(), Value::str("fleet".to_owned()));
  props.insert("count".to_owned(), Value::Int(42));
  props.insert("on".to_owned(), Value::Bool(true));
  props.insert("tags".to_owned(), Value::seq(vec![Value::str("a".to_owned()), Value::str("b".to_owned())]));
  let mut user = Data::default();
  user.insert("city".to_owned(), Value::str("Oslo".to_owned()));
  props.insert("user".to_owned(), Value::Map(user));

  let chunks = render(&ev, "page.tera", props).expect("renders");
  assert_eq!(raw(&chunks[0]), "fleet|42|true|b|Oslo");
}

#[test]
fn island_props_survive_escaping_intact() {
  let ev = evaluator(&[(
    "page.html",
    r#"{{ hostile }}{{ island(module="ui/Note.tsx#default", props={"body": "<b>a & b</b>", "quote": "\"q\""}) }}"#,
  )]);

  let mut props = Data::default();
  props.insert("hostile".to_owned(), Value::str("<b>a & b</b>".to_owned()));

  let chunks = render(&ev, "page.html", props).expect("renders");
  let (_, island_props) = island(&chunks[chunks.len() - 1]);
  assert_eq!(island_props.get("body"), Some(&Value::str("<b>a & b</b>".to_owned())), "base64 has no character escaping can touch");
  assert_eq!(island_props.get("quote"), Some(&Value::str("\"q\"".to_owned())));
  assert!(!raw(&chunks[0]).contains("<b>"), "the surrounding text is escaped, so the props above went through the same pass: {:?}", raw(&chunks[0]));
}

#[test]
fn a_marker_in_template_content_fails_rather_than_corrupting_the_split() {
  let ev = evaluator(&[("page.tera", &format!("before{MARKER}after"))]);
  let err = one(&ev, "page.tera").expect_err("one marker leaves an even number of parts");
  assert_eq!(err.module, "page.tera#default");
  assert!(err.message.contains("unbalanced marker delimiters"), "{}", err.message);
}

#[test]
fn an_unknown_marker_token_names_itself() {
  let ev = evaluator(&[("page.tera", &format!("{MARKER}sprocket:3{MARKER}"))]);
  let err = one(&ev, "page.tera").expect_err("sprocket is not a token this crate emits");
  assert!(err.message.contains("unknown marker token `sprocket:3`"), "{}", err.message);
}

#[test]
fn a_slot_name_that_could_not_be_an_attribute_is_refused() {
  let ev = evaluator(&[("page.tera", r#"{{ slot(name="a b") }}"#)]);
  let err = one(&ev, "page.tera").expect_err("a space is not allowed in a slot name");
  assert!(err.message.contains("invalid slot name `a b`"), "{}", err.message);

  let ev = evaluator(&[("page.tera", r#"{{ slot(name="") }}"#)]);
  let err = one(&ev, "page.tera").expect_err("an empty slot name names nothing");
  assert!(err.message.contains("invalid slot name"), "{}", err.message);
}

#[test]
fn island_props_that_are_not_a_map_are_refused() {
  let ev = evaluator(&[("page.tera", r#"{{ island(module="ui/List.tsx#default", props=[1, 2]) }}"#)]);
  let err = one(&ev, "page.tera").expect_err("props are the island's named inputs");
  assert!(err.message.contains("island props must be a map"), "{}", err.message);
}

#[test]
fn an_island_module_without_an_export_is_refused() {
  let ev = evaluator(&[("page.tera", r#"{{ island(module="ui/List.tsx") }}"#)]);
  let err = one(&ev, "page.tera").expect_err("a module id is `path#export`");
  assert!(err.message.contains("island module id"), "{}", err.message);
}

#[test]
fn a_module_no_template_answers_to_names_itself() {
  let ev = evaluator(&[("page.tera", "hello")]);
  let err = one(&ev, "absent.tera").expect_err("nothing was added under that name");
  assert_eq!(err.module, "absent.tera#default");
}

#[test]
fn a_timing_or_a_mode_wraps_the_placement_in_the_region_the_browser_reads() {
  let ev = evaluator(&[(
    "timed.tera",
    r#"{{ island(module="ui/Chart.tsx#default", props={}, when="visible") }}{{ island(module="ui/Help.tsx#default", props={}, mode="server") }}{{ island(module="ui/Now.tsx#default", props={}, when="idle", mode="browser") }}"#,
  )]);

  let chunks = one(&ev, "timed.tera").expect("renders");
  assert_eq!(chunks.len(), 9, "three placements, each wrapped: {chunks:#?}");
  assert_eq!(raw(&chunks[0]), "<sf-s data-sf-island data-sf-when=\"visible\">");
  assert_eq!(island(&chunks[1]).0.to_string(), "ui/Chart.tsx#default");
  assert_eq!(raw(&chunks[2]), "</sf-s>");
  assert_eq!(raw(&chunks[3]), "<sf-s data-sf-island data-sf-mode=\"server\">");
  assert_eq!(raw(&chunks[6]), "<sf-s data-sf-island data-sf-when=\"idle\">", "`browser` is the default, so it writes no mode");
}

#[test]
fn a_map_keeps_its_key_order_across_a_placement() {
  // Sixteen keys in an order that is neither sorted nor reversed, with eight
  // fields below each: a hash order matching this by chance is one in 16!,
  // so the test is a guard rather than a coin toss.
  const SYMBOLS: [&str; 16] = ["VLDT", "ARBR", "MRSH", "KLNS", "TQOP", "BHNE", "ZRAX", "GDLU", "PWIC", "NFKS", "YBRT", "EJHM", "SOVD", "CXQA", "LMPF", "RUGE"];
  const FIELDS: [&str; 8] = ["price", "change", "trail", "bid", "ask", "volume", "high", "low"];

  let ev = evaluator(&[("watch.tera", r#"{{ island(module="ui/Watch.tsx#default", props=watch) }}"#)]);
  let mut quotes = snapfire_fsr_core::ValueMap::default();
  for symbol in SYMBOLS {
    let mut quote = snapfire_fsr_core::ValueMap::default();
    for (i, field) in FIELDS.iter().enumerate() {
      quote.insert((*field).to_owned(), Value::Int(i as i128));
    }
    quotes.insert(symbol.to_owned(), Value::Map(quote));
  }
  let mut watch = snapfire_fsr_core::ValueMap::default();
  watch.insert("quotes".to_owned(), Value::Map(quotes));
  let mut props = Data::default();
  props.insert("watch".to_owned(), Value::Map(watch));

  let chunks = render(&ev, "watch.tera", props).expect("renders");
  let Some(Value::Map(quotes)) = island(&chunks[0]).1.get("quotes") else { panic!("the quotes map is there: {chunks:#?}") };
  assert_eq!(quotes.keys().cloned().collect::<Vec<_>>(), SYMBOLS, "the order the loader wrote, not the hash's");
  for symbol in SYMBOLS {
    let Some(Value::Map(quote)) = quotes.get(symbol) else { panic!("`{symbol}` is a map") };
    assert_eq!(quote.keys().cloned().collect::<Vec<_>>(), FIELDS, "and every map below it");
  }
}

#[test]
fn a_placement_with_neither_is_the_client_node_alone() {
  let ev = evaluator(&[("plain.tera", r#"{{ island(module="ui/Chart.tsx#default", props={}) }}"#)]);
  let chunks = one(&ev, "plain.tera").expect("renders");
  assert_eq!(chunks.len(), 1, "no region around it: {chunks:#?}");
}

#[test]
fn a_timing_the_runtime_does_not_know_is_refused_where_it_is_written() {
  let ev = evaluator(&[("bad.tera", r#"{{ island(module="ui/Chart.tsx#default", props={}, when="soon") }}"#)]);
  let message = one(&ev, "bad.tera").expect_err("refused").to_string();
  assert!(message.contains("soon"), "{message}");
}

#[test]
fn a_keyed_placement_is_a_region_the_browser_can_patch() {
  let ev = evaluator(&[("keyed.tera", r#"{{ island(module="ui/Chart.tsx#default", props={"n": 1}, key="chart") }}"#)]);
  let chunks = one(&ev, "keyed.tera").expect("renders");
  assert_eq!(chunks.len(), 3, "{chunks:#?}");
  assert_eq!(raw(&chunks[0]), "<sf-s data-sf-island data-sf-region=\"chart\">");
  let (_, props) = island(&chunks[1]);
  assert_eq!(props.get("$k"), Some(&Value::str("chart".to_owned())), "the key rides the props too, which is what a revalidation matches on");
  assert_eq!(props.get("n"), Some(&Value::Int(1)));
}

#[test]
fn a_number_from_a_loader_renders_as_a_number_whatever_it_came_as() {
  let ev = evaluator(&[("counts.tera", "<footer>passed={{ count }} whole={{ whole }} big={{ big }} half={{ half }} nested={{ nested.n }} list={{ list | length }} bytes={{ bytes }}</footer>")]);
  let mut props = Data::default();
  props.insert("count".to_owned(), Value::F64(2.0));
  props.insert("whole".to_owned(), Value::Int(7));
  props.insert("big".to_owned(), Value::Int(1 << 60));
  props.insert("half".to_owned(), Value::F64(2.5));
  props.insert("nested".to_owned(), Value::Map([("n".to_owned(), Value::F64(3.0))].into_iter().collect()));
  props.insert("list".to_owned(), Value::seq(vec![Value::Int(1), Value::F64(2.0)]));
  props.insert("bytes".to_owned(), Value::Bytes(vec![104, 105]));
  let chunks = render(&ev, "counts.tera", props).unwrap();
  let html: String = chunks.iter().map(raw).collect();
  assert_eq!(html, "<footer>passed=2 whole=7 big=1152921504606846976 half=2.5 nested=3 list=2 bytes=aGk=</footer>");
}
