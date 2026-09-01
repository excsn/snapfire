use criterion::{black_box, criterion_group, criterion_main, Criterion};
use snapfire_fsr_core::{Fingerprint, ModuleId, Node, TypedArray, Value, ValueMap};

fn island(series_len: usize) -> Node {
  let mut props = ValueMap::new();
  props.insert(
    "series".to_owned(),
    Value::TypedArray(TypedArray::F64((0..series_len).map(|i| i as f64).collect())),
  );
  props.insert("title".to_owned(), Value::str("servers"));
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

fn deep_map(depth: usize, width: usize) -> Value {
  let mut current = Value::str("leaf");
  for _ in 0..depth {
    let mut map = ValueMap::new();
    for i in 0..width {
      map.insert(format!("key_{i}"), current.clone());
    }
    current = Value::Map(map);
  }
  current
}

fn bench_fingerprint(c: &mut Criterion) {
  let small = page(1, 100);
  let large = page(20, 10_000);
  let nested = deep_map(5, 8);

  c.bench_function("fingerprint/page_1_section_100_points", |b| {
    b.iter(|| black_box(&small).fingerprint())
  });
  c.bench_function("fingerprint/page_20_sections_10k_points", |b| {
    b.iter(|| black_box(&large).fingerprint())
  });
  c.bench_function("fingerprint/nested_map_depth5_width8", |b| {
    b.iter(|| black_box(&nested).fingerprint())
  });
}

criterion_group!(benches, bench_fingerprint);
criterion_main!(benches);
