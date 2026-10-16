//! The payload rows read back: every row `wire_stream` writes comes out of
//! `RowReader` as the value it was written from, across any chunking.

use futures_util::StreamExt;
use snapfire_fsr_core::{Data, Node, SlotId, Value};
use snapfire_fsr_runtime::assembler::{Assembly, PendingResolution, Resolved};
use snapfire_fsr_runtime::wire::{row_from_line, Row, RowReader};
use snapfire_fsr_runtime::{wire_stream, Contribution, Locale, Meta, SegmentInfo};

fn segment(key: &str, slot: Option<u32>) -> SegmentInfo {
  SegmentInfo::new(key.to_owned(), if slot.is_some() { 0 } else { 7 }, "content".to_owned(), Vec::new(), slot, Vec::new(), Vec::new())
}

fn assembly() -> Assembly {
  let mut values = Data::default();
  values.insert("cart/count".to_owned(), Value::str("2"));
  let fill = Resolved::new(SlotId(1), "reports:routes/page.tsx#default".to_owned(), Node::text("filled"), vec![segment("reports:inner", None)], Vec::new(), Meta::default().title(Some("Reports".to_owned())), vec![Contribution::seeded("reports:routes/page.tsx#default", vec!["content".to_owned()], values.clone())]);
  Assembly::new(Node::Seq(vec![Node::text("shell"), Node::Pending { slot: SlotId(1), fallback: Box::new(Node::text("...")) }]), vec![PendingResolution::new(SlotId(1), "reports:routes/page.tsx#default".to_owned(), Box::pin(async move { fill }))], SegmentInfo::new("shell#document".to_owned(), 3, String::new(), Vec::new(), None, vec![segment("reports:routes/page.tsx#default", Some(1))], Vec::new()), Meta::default().title(Some("Acme".to_owned())).description(Some("the portal".to_owned())), vec![Contribution::promised("reports:routes/page.tsx#default", vec!["content".to_owned()], vec!["cart/count".to_owned()])], Data::default(), Locale::new("en", true), Some("/reports/static/js/app/src/main.js".to_owned()), vec!["/reports/static/css/site.css".to_owned()], Some(r#"{"hello":"Hello"}"#.to_owned()), Some(snapfire_fsr_runtime::DocumentTheme::default().name(Some("ocean".to_owned())).scheme(Some("dark".to_owned())).forced(false)), None)
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
  assert_eq!(rows[2], Row::Head(Meta::default().title(Some("Acme".to_owned())).description(Some("the portal".to_owned()))));
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
  assert_eq!(rows[11], Row::Head(Meta::default().title(Some("Reports".to_owned()))));
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
