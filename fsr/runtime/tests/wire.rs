//! The payload rows read back: every row `wire_stream` writes comes out of
//! `RowReader` as the value it was written from, across any chunking.

use futures_util::StreamExt;
use snapfire_fsr_core::{Data, Node, SlotId, Value};
use snapfire_fsr_runtime::assembler::{Assembly, PendingResolution, Resolved};
use snapfire_fsr_runtime::wire::{row_from_line, Row, RowReader};
use snapfire_fsr_runtime::{wire_stream, Contribution, Locale, Meta, SegmentInfo};

fn segment(key: &str, slot: Option<u32>) -> SegmentInfo {
  SegmentInfo { key: key.to_owned(), digest: if slot.is_some() { 0 } else { 7 }, name: "content".to_owned(), path: Vec::new(), slot, children: Vec::new(), keep: Vec::new() }
}

fn assembly() -> Assembly {
  let mut values = Data::default();
  values.insert("cart/count".to_owned(), Value::str("2"));
  let fill = Resolved {
    slot: SlotId(1),
    key: "reports:routes/page.tsx#default".to_owned(),
    node: Node::text("filled"),
    segments: vec![segment("reports:inner", None)],
    pending: Vec::new(),
    meta: Meta { title: Some("Reports".to_owned()), ..Meta::default() },
    contributions: vec![Contribution::seeded("reports:routes/page.tsx#default", vec!["content".to_owned()], values.clone())],
  };
  Assembly {
    tree: Node::Seq(vec![Node::text("shell"), Node::Pending { slot: SlotId(1), fallback: Box::new(Node::text("...")) }]),
    pending: vec![PendingResolution { slot: SlotId(1), key: "reports:routes/page.tsx#default".to_owned(), future: Box::pin(async move { fill }) }],
    segments: SegmentInfo { key: "shell#document".to_owned(), digest: 3, name: String::new(), path: Vec::new(), slot: None, children: vec![segment("reports:routes/page.tsx#default", Some(1))], keep: Vec::new() },
    meta: Meta { title: Some("Acme".to_owned()), description: Some("the portal".to_owned()), ..Meta::default() },
    contributions: vec![Contribution::promised("reports:routes/page.tsx#default", vec!["content".to_owned()], vec!["cart/count".to_owned()])],
    store: Data::default(),
    locale: Locale::new("en", true),
    entry: Some("/reports/static/js/app/src/main.js".to_owned()),
    styles: vec!["/reports/static/css/site.css".to_owned()],
    catalog: Some(r#"{"hello":"Hello"}"#.to_owned()),
    theme: Some(snapfire_fsr_runtime::DocumentTheme { name: Some("ocean".to_owned()), scheme: Some("dark".to_owned()), forced: false }),
    failed: None,
  }
}

#[test]
fn every_row_reads_back_as_what_it_was_written_from_whatever_the_chunking() {
  let text: String = futures::executor::block_on(wire_stream(assembly()).collect::<Vec<String>>()).concat();
  let mut reader = RowReader::new();
  let mut rows = Vec::new();
  for chunk in text.as_bytes().chunks(7) {
    rows.extend(reader.push(chunk).unwrap());
  }
  assert_eq!(reader.finish().unwrap(), None);
  let tags: Vec<&str> = rows.iter().map(|r| match r {
    Row::Version { .. } => "V", Row::Tree(_) => "N", Row::Head(_) => "H", Row::Store(_) => "T", Row::Locale(_) => "L",
    Row::Entry(_) => "E", Row::Styles(_) => "C", Row::Catalog(_) => "D", Row::Segments(_) => "G", Row::Fill { .. } => "S",
  }).collect();
  assert_eq!(tags, ["V", "N", "H", "T", "L", "E", "C", "D", "G", "T", "S", "H"], "{text}");
  let expected = assembly();
  assert_eq!(rows[0], Row::Version { fmt: 2, enc: "json".to_owned() });
  assert_eq!(rows[1], Row::Tree(expected.tree.clone()));
  assert_eq!(rows[2], Row::Head(Meta { title: Some("Acme".to_owned()), description: Some("the portal".to_owned()), ..Meta::default() }));
  assert_eq!(rows[3], Row::Store(expected.contributions.clone()));
  assert_eq!(rows[4], Row::Locale("en".to_owned()));
  assert_eq!(rows[5], Row::Entry("/reports/static/js/app/src/main.js".to_owned()));
  assert_eq!(rows[6], Row::Styles(vec!["/reports/static/css/site.css".to_owned()]));
  assert_eq!(rows[7], Row::Catalog(r#"{"hello":"Hello"}"#.to_owned()));
  assert_eq!(rows[8], Row::Segments(expected.segments.clone()));
  let Row::Store(seeded) = &rows[9] else { panic!("{:?}", rows[9]) };
  assert_eq!(seeded[0].segment, "reports:routes/page.tsx#default");
  assert_eq!(seeded[0].values.get("cart/count"), Some(&Value::str("2")));
  assert_eq!(rows[10], Row::Fill { slot: 1, node: Node::text("filled"), segments: vec![segment("reports:inner", None)] });
  assert_eq!(rows[11], Row::Head(Meta { title: Some("Reports".to_owned()), ..Meta::default() }));
}

#[test]
fn a_newer_format_an_unknown_tag_and_a_broken_line_are_refused_by_name() {
  let newer = row_from_line(r#"V {"fmt":3,"enc":"json"}"#).unwrap_err().to_string();
  assert!(newer.contains("payload format 3"), "{newer}");
  let unknown = row_from_line("Z []").unwrap_err().to_string();
  assert!(unknown.contains("unknown payload row `Z`"), "{unknown}");
  let broken = row_from_line("N").unwrap_err().to_string();
  assert!(broken.contains("a row is `<tag> <json>`"), "{broken}");
  let slot = row_from_line("S x {}").unwrap_err().to_string();
  assert!(slot.contains("slot `x` is not a number"), "{slot}");
}
