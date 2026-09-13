use snapfire_fsr_core::{Value, ValueMap};

/// One line of the desk: what is held, what it is worth and where it moved.
/// `cost` is what the desk paid a share, which is the only reason it can say
/// what it has made.
pub struct Holding {
  pub symbol: &'static str,
  pub name: &'static str,
  pub shares: i64,
  pub price: f64,
  pub cost: f64,
  pub change: f64,
}

pub const HOLDINGS: &[Holding] = &[
  Holding { symbol: "ARBR", name: "Arbor Works", shares: 120, price: 41.20, cost: 36.80, change: 1.8 },
  Holding { symbol: "KLNS", name: "Kiln & Sons", shares: 64, price: 118.05, cost: 131.40, change: -0.9 },
  Holding { symbol: "MRSH", name: "Marsh Optics", shares: 310, price: 7.65, cost: 5.90, change: 4.2 },
  Holding { symbol: "VLDT", name: "Veldt Freight", shares: 45, price: 260.40, cost: 254.75, change: 0.3 },
];

/// What a visitor bought on top of the desk's own line: the shares and what
/// they cost, since a count alone cannot price a profit.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Position {
  pub shares: i64,
  pub spent: f64,
}

/// The desk's book at one tick: what it is worth, what it cost, the profit
/// between them and what the day has moved.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Figures {
  pub value: f64,
  pub cost: f64,
  pub profit: f64,
  pub profit_pct: f64,
  pub day: f64,
}

/// The book at `tick`, with `own` adding each visitor's own position to the
/// line. Pure: the same numbers for the same tick and the same positions.
pub fn figures(tick: usize, own: impl Fn(&Holding) -> Position) -> Figures {
  let mut value = 0.0;
  let mut cost = 0.0;
  let mut day = 0.0;
  for holding in HOLDINGS {
    let extra = own(holding);
    let shares = (holding.shares + extra.shares) as f64;
    let price = price_at(holding, tick);
    value += shares * price;
    cost += holding.shares as f64 * holding.cost + extra.spent;
    day += shares * price * holding.change / 100.0;
  }
  let profit = value - cost;
  let profit_pct = if cost == 0.0 { 0.0 } else { profit / cost * 100.0 };
  Figures { value, cost, profit, profit_pct, day }
}

impl Figures {
  /// The summary as a template reads it: every number already printed, since
  /// the markup shows them and nothing downstream does arithmetic on them.
  pub fn as_value(&self) -> Value {
    let mut map = ValueMap::default();
    map.insert("value".to_owned(), Value::str(format!("{:.2}", self.value)));
    map.insert("cost".to_owned(), Value::str(format!("{:.2}", self.cost)));
    map.insert("profit".to_owned(), Value::str(format!("{:+.2}", self.profit)));
    map.insert("profit_pct".to_owned(), Value::str(format!("{:+.1}%", self.profit_pct)));
    map.insert("day".to_owned(), Value::str(format!("{:+.2}", self.day)));
    map.insert("up".to_owned(), Value::Bool(self.profit >= 0.0));
    map.insert("day_up".to_owned(), Value::Bool(self.day >= 0.0));
    Value::Map(map)
  }
}

pub const HEADLINES: &[(&str, &str)] = &[
  ("ARBR", "Arbor Works signs the northern yard"),
  ("MRSH", "Marsh Optics lands a survey contract"),
  ("VLDT", "Veldt Freight adds a second night run"),
  ("KLNS", "Kiln & Sons pauses the third furnace"),
];

/// How far the tape has ticked, which is what moves a price. A desk with a
/// backend would read the backend; this one wants a number that changes while
/// the page is open, so a push has something to carry.
#[derive(Clone, Default)]
pub struct Ticks(std::sync::Arc<std::sync::atomic::AtomicUsize>);

impl Ticks {
  pub fn now(&self) -> usize {
    self.0.load(std::sync::atomic::Ordering::Relaxed)
  }

  pub fn advance(&self) -> usize {
    self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1
  }
}

/// A holding's price at `tick`: the same walk for everyone, so two browsers
/// watching the same symbol see the same number.
pub fn price_at(h: &Holding, tick: usize) -> f64 {
  let step = ((tick as f64) * 0.7 + h.symbol.len() as f64).sin();
  ((h.price + step * h.price * 0.004) * 100.0).round() / 100.0
}

pub fn holding(symbol: &str) -> Option<&'static Holding> {
  HOLDINGS.iter().find(|h| h.symbol == symbol)
}

/// A short price trail per holding, so the sparkline inside the masthead has
/// something to draw. Deterministic: a desk with a backend would read one.
pub fn trail(h: &Holding) -> Vec<f64> {
  let steps = [-0.9, 0.4, -0.2, 0.8, 0.1, 0.6];
  steps.iter().enumerate().map(|(i, step)| h.price + step * h.change * (i as f64 + 1.0) / 6.0).collect()
}

pub fn as_value(h: &Holding) -> Value {
  let mut map = ValueMap::default();
  map.insert("symbol".to_owned(), Value::str(h.symbol));
  map.insert("name".to_owned(), Value::str(h.name));
  map.insert("shares".to_owned(), Value::Int(h.shares as i128));
  map.insert("price".to_owned(), Value::F64(h.price));
  map.insert("cost".to_owned(), Value::F64(h.cost));
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

/// Where the headline rotation stands. A desk with a backend would read the
/// backend instead.
#[derive(Clone, Default)]
pub struct Tape(std::sync::Arc<std::sync::atomic::AtomicUsize>);

impl Tape {
  /// Where the tape stands. Reading it never moves it: the rotation belongs
  /// to the desk's clock, so a page loading twice sees the same tape and a
  /// revalidation from anywhere else leaves it alone.
  pub fn at(&self) -> usize {
    self.0.load(std::sync::atomic::Ordering::Relaxed)
  }

  pub fn advance(&self) {
    self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  }
}
