use criterion::{black_box, criterion_group, criterion_main, Criterion};
use snapfire_fsr_core::{ModuleId, Node, TypedArray, Value, ValueMap};
use snapfire_fsr_payload::{html_serialize, serialize_page, value_to_json};

fn island(series_len: usize) -> Node {
  let mut props = ValueMap::new();
  props.insert(
    "series".to_owned(),
    Value::TypedArray(TypedArray::F64((0..series_len).map(|i| i as f64).collect())),
  );
  Node::Client {
    module: ModuleId::new("components/ServerChart.tsx", "default"),
    props,
    children: Vec::new(),
    ssr: None,
  }
}

fn page(sections: usize, series_len: usize) -> Node {
  let mut items = vec![Node::raw("<html><head></head><body><main>")];
  for i in 0..sections {
    items.push(Node::raw(format!("<section><h1>Section {i}</h1><table></table>")));
    items.push(island(series_len));
    items.push(Node::raw("</section>"));
  }
  items.push(Node::raw("</main></body></html>"));
  Node::Seq(items)
}

fn typical_props() -> Value {
  let mut server = ValueMap::new();
  server.insert("id".to_owned(), Value::int(90_071_992_547_409_920i128));
  server.insert("name".to_owned(), Value::str("web-1"));
  server.insert("healthy".to_owned(), Value::Bool(true));
  server.insert("load".to_owned(), Value::F64(0.73));
  let servers = Value::Seq((0..50).map(|_| Value::Map(server.clone())).collect());
  let mut props = ValueMap::new();
  props.insert("servers".to_owned(), servers);
  Value::Map(props)
}

fn bench_encode(c: &mut Criterion) {
  let small = page(1, 100);
  let large = page(20, 10_000);
  let props = typical_props();

  c.bench_function("json/value_typical_props", |b| {
    b.iter(|| value_to_json(black_box(&props)).to_string())
  });
  c.bench_function("wire/page_1_section_100_points", |b| {
    b.iter(|| serialize_page(black_box(&small)))
  });
  c.bench_function("wire/page_20_sections_10k_points", |b| {
    b.iter(|| serialize_page(black_box(&large)))
  });
  c.bench_function("html/page_1_section_100_points", |b| {
    b.iter(|| html_serialize(black_box(&small)))
  });
  c.bench_function("html/page_20_sections_10k_points", |b| {
    b.iter(|| html_serialize(black_box(&large)))
  });
}

criterion_group!(benches, bench_encode);
criterion_main!(benches);
