use snapfire_fsr_core::{Value, ValueMap};

/// One line of the desk: what is held, what it is worth and where it moved.
pub struct Holding {
  pub symbol: &'static str,
  pub name: &'static str,
  pub shares: i64,
  pub price: f64,
  pub change: f64,
}

pub const HOLDINGS: &[Holding] = &[
  Holding { symbol: "ARBR", name: "Arbor Works", shares: 120, price: 41.20, change: 1.8 },
  Holding { symbol: "KLNS", name: "Kiln & Sons", shares: 64, price: 118.05, change: -0.9 },
  Holding { symbol: "MRSH", name: "Marsh Optics", shares: 310, price: 7.65, change: 4.2 },
  Holding { symbol: "VLDT", name: "Veldt Freight", shares: 45, price: 260.40, change: 0.3 },
];

pub const HEADLINES: &[(&str, &str)] = &[
  ("ARBR", "Arbor Works signs the northern yard"),
  ("MRSH", "Marsh Optics lands a survey contract"),
  ("VLDT", "Veldt Freight adds a second night run"),
  ("KLNS", "Kiln & Sons pauses the third furnace"),
];

pub fn holding(symbol: &str) -> Option<&'static Holding> {
  HOLDINGS.iter().find(|h| h.symbol == symbol)
}

pub fn as_value(h: &Holding) -> Value {
  let mut map = ValueMap::default();
  map.insert("symbol".to_owned(), Value::str(h.symbol));
  map.insert("name".to_owned(), Value::str(h.name));
  map.insert("shares".to_owned(), Value::Int(h.shares as i128));
  map.insert("price".to_owned(), Value::F64(h.price));
  map.insert("change".to_owned(), Value::F64(h.change));
  Value::Map(map)
}

pub fn holdings_value() -> Value {
  Value::seq(HOLDINGS.iter().map(as_value).collect::<Vec<_>>())
}

/// The headlines, newest first, rotated by `tick` so a polled fragment comes
/// back different without a backend behind it.
pub fn headlines_value(tick: usize) -> Value {
  let rows = (0..HEADLINES.len()).map(|i| {
    let (symbol, text) = HEADLINES[(i + tick) % HEADLINES.len()];
    let mut map = ValueMap::default();
    map.insert("symbol".to_owned(), Value::str(symbol));
    map.insert("text".to_owned(), Value::str(text));
    Value::Map(map)
  });
  Value::seq(rows.collect::<Vec<_>>())
}

/// How many times the tape has been asked for, so a polled fragment comes
/// back rotated. A desk with a backend would read the backend instead.
#[derive(Clone, Default)]
pub struct Tape(std::sync::Arc<std::sync::atomic::AtomicUsize>);

impl Tape {
  pub fn next(&self) -> usize {
    self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
  }
}
