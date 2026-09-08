//! Which `BuildHasher` suits a `ValueMap`: short string keys, maps of four to
//! fourteen entries, built and read many times per render.
//!
//! `insert` is the payload decoder's shape and `lookup` is the renderer's.
//! `adversarial` is the case that decides the choice rather than the speed:
//! the keys are attacker-supplied in `snapfire_fsr_payload::json_to_value`, so
//! a hasher whose collisions can be constructed turns one request into
//! quadratic work.

use std::hash::BuildHasher;

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use indexmap::IndexMap;

const PRODUCT: &[&str] = &[
  "id",
  "name",
  "brand",
  "category",
  "price_cents",
  "list_price_cents",
  "image",
  "rating",
  "reviews",
  "stock",
  "description",
  "tags",
  "attributes",
  "quantity",
];

const READ: &[&str] = &["name", "price_cents", "image", "rating", "stock", "tags", "$props", "products", "cartCount", "missing"];

fn build<S: BuildHasher + Default>(keys: &[&str]) -> IndexMap<String, u32, S> {
  let mut map = IndexMap::default();
  for (i, key) in keys.iter().enumerate() {
    map.insert((*key).to_owned(), i as u32);
  }
  map
}

fn insert_group<S: BuildHasher + Default>(c: &mut Criterion, name: &str) {
  c.bench_with_input(BenchmarkId::new("hash/insert", name), &PRODUCT, |b, keys| {
    b.iter(|| black_box(build::<S>(keys).len()));
  });
}

fn lookup_group<S: BuildHasher + Default>(c: &mut Criterion, name: &str) {
  let map = build::<S>(PRODUCT);
  c.bench_with_input(BenchmarkId::new("hash/lookup", name), &READ, |b, reads| {
    b.iter(|| {
      let mut found = 0u32;
      for key in reads.iter() {
        if let Some(v) = map.get(*key) {
          found = found.wrapping_add(*v);
        }
      }
      black_box(found)
    });
  });
}

/// 4096 keys of one shape, which is what a request body can carry.
fn adversarial_group<S: BuildHasher + Default>(c: &mut Criterion, name: &str) {
  let keys: Vec<String> = (0..4096u32).map(|i| format!("k{i:08x}")).collect();
  c.bench_with_input(BenchmarkId::new("hash/bulk_insert_4096", name), &keys, |b, keys| {
    b.iter(|| {
      let mut map: IndexMap<String, u32, S> = IndexMap::default();
      for (i, key) in keys.iter().enumerate() {
        map.insert(key.clone(), i as u32);
      }
      black_box(map.len())
    });
  });
}

macro_rules! hashers {
  ($c:expr, $($name:literal => $ty:ty),* $(,)?) => {
    $(
      insert_group::<$ty>($c, $name);
      lookup_group::<$ty>($c, $name);
      adversarial_group::<$ty>($c, $name);
    )*
  };
}

fn bench(c: &mut Criterion) {
  hashers!(
    c,
    "siphash" => std::collections::hash_map::RandomState,
    "foldhash_fast" => foldhash::fast::RandomState,
    "foldhash_quality" => foldhash::quality::RandomState,
    "ahash" => ahash::RandomState,
    "fxhash" => rustc_hash::FxBuildHasher,
    "seahash" => std::hash::BuildHasherDefault<seahash::SeaHasher>,
    "fnv" => fnv::FnvBuildHasher,
    "xxh3" => std::hash::BuildHasherDefault<twox_hash::xxhash3_64::Hasher>,
  );
}

criterion_group!(benches, bench);
criterion_main!(benches);
