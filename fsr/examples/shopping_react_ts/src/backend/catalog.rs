use std::sync::Arc;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Attribute {
  pub name: String,
  pub value: String,
}

/// A placeholder in place of a photograph: a tile colour and a glyph.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Image {
  pub color: String,
  pub emoji: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Product {
  pub id: u64,
  pub name: String,
  pub brand: String,
  pub category: String,
  pub price_cents: i64,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub list_price_cents: Option<i64>,
  pub stock: u32,
  pub rating: f64,
  pub reviews: u32,
  pub description: String,
  pub tags: Vec<String>,
  pub attributes: Vec<Attribute>,
  pub image: Image,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OrderLine {
  pub product_id: u64,
  pub quantity: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OrderRequest {
  pub lines: Vec<OrderLine>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PlacedLine {
  pub product_id: u64,
  pub name: String,
  pub quantity: u32,
  pub line_cents: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Order {
  pub id: u64,
  pub total_cents: i64,
  pub lines: Vec<PlacedLine>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum OrderError {
  Empty,
  UnknownProduct(u64),
  OutOfStock { product_id: u64, wanted: u32, held: u32 },
}

impl OrderError {
  pub fn status(&self) -> u16 {
    match self {
      Self::Empty => 400,
      Self::UnknownProduct(_) => 404,
      Self::OutOfStock { .. } => 409,
    }
  }

  pub fn message(&self) -> String {
    match self {
      Self::Empty => "an order needs at least one line".to_owned(),
      Self::UnknownProduct(id) => format!("no product {id}"),
      Self::OutOfStock { product_id, wanted, held } => {
        format!("product {product_id}: wanted {wanted}, {held} in stock")
      }
    }
  }
}

struct State {
  products: Vec<Product>,
  next_order: u64,
}

/// The backend's own store. It is behind an HTTP boundary, so the FSR side
/// never touches it.
#[derive(Clone)]
pub struct Catalog(Arc<Mutex<State>>);

struct Seed {
  id: u64,
  name: &'static str,
  brand: &'static str,
  category: &'static str,
  price_cents: i64,
  list_price_cents: Option<i64>,
  stock: u32,
  rating: f64,
  reviews: u32,
  description: &'static str,
  tags: &'static [&'static str],
  attributes: &'static [(&'static str, &'static str)],
  color: &'static str,
  emoji: &'static str,
}

impl Seed {
  fn build(&self) -> Product {
    Product {
      id: self.id,
      name: self.name.to_owned(),
      brand: self.brand.to_owned(),
      category: self.category.to_owned(),
      price_cents: self.price_cents,
      list_price_cents: self.list_price_cents,
      stock: self.stock,
      rating: self.rating,
      reviews: self.reviews,
      description: self.description.to_owned(),
      tags: self.tags.iter().map(|t| (*t).to_owned()).collect(),
      attributes: self.attributes.iter().map(|(n, v)| Attribute { name: (*n).to_owned(), value: (*v).to_owned() }).collect(),
      image: Image { color: self.color.to_owned(), emoji: self.emoji.to_owned() },
    }
  }
}

const SEED: &[Seed] = &[
  Seed { id: 1, name: "PLA filament, 1 kg spool", brand: "Polymaker", category: "printing", price_cents: 2400, list_price_cents: Some(2900), stock: 12, rating: 4.7, reviews: 1834, description: "A dimensionally stable PLA wound on a cardboard spool. Prints cleanly at 200 to 215 degrees with no enclosure and little stringing.", tags: &["printing", "consumable", "filament"], attributes: &[("Diameter", "1.75 mm, +/- 0.02 mm"), ("Colour", "Matte black"), ("Print temperature", "200 to 215 C"), ("Net weight", "1 kg")], color: "#2f3e46", emoji: "\u{1f9f5}" },
  Seed { id: 2, name: "All-metal hotend, 24 V", brand: "Slice", category: "printing", price_cents: 5900, list_price_cents: None, stock: 4, rating: 4.5, reviews: 412, description: "A high-flow hotend with a titanium heat break and a 50 W ceramic heater, for printing engineering materials at up to 300 degrees.", tags: &["printing", "part", "hotend"], attributes: &[("Voltage", "24 V"), ("Heater", "50 W ceramic"), ("Maximum temperature", "300 C"), ("Nozzle thread", "M6")], color: "#8d5524", emoji: "\u{1f525}" },
  Seed { id: 3, name: "Textured PEI build plate, 235 mm", brand: "Energetic", category: "printing", price_cents: 3150, list_price_cents: None, stock: 0, rating: 4.6, reviews: 977, description: "A spring steel sheet with a double-sided powder-coated PEI texture. Parts release when the plate cools; no glue or tape.", tags: &["printing", "part", "bed"], attributes: &[("Size", "235 x 235 mm"), ("Thickness", "0.5 mm"), ("Surface", "Powder-coated PEI, both sides")], color: "#4a4e69", emoji: "\u{1f4d0}" },
  Seed { id: 4, name: "Digital calipers, 150 mm", brand: "Mitutoyo", category: "tools", price_cents: 2999, list_price_cents: Some(3499), stock: 7, rating: 4.8, reviews: 5210, description: "Stainless steel calipers with a large LCD readout, millimetre and inch modes and a thumb roller for fine adjustment.", tags: &["tools", "measuring"], attributes: &[("Range", "0 to 150 mm"), ("Resolution", "0.01 mm"), ("Accuracy", "+/- 0.02 mm"), ("Battery", "SR44, included")], color: "#1b4965", emoji: "\u{1f4cf}" },
  Seed { id: 5, name: "Deburring tool with 10 blades", brand: "Noga", category: "tools", price_cents: 1150, list_price_cents: None, stock: 23, rating: 4.6, reviews: 2288, description: "A swivel-head deburring handle with ten high-speed steel blades for cleaning printed and machined edges.", tags: &["tools", "consumable", "finishing"], attributes: &[("Blades", "10 x E100 HSS"), ("Handle", "Aluminium, knurled"), ("Blade change", "Tool-free")], color: "#6c757d", emoji: "\u{1f52a}" },
  Seed { id: 6, name: "Whole bean espresso, 1 kg", brand: "Square Mile", category: "food", price_cents: 2850, list_price_cents: None, stock: 30, rating: 4.7, reviews: 3409, description: "A seasonal espresso blend roasted to order. Chocolate and red fruit, with enough body to hold up in milk.", tags: &["food", "coffee", "beans"], attributes: &[("Ingredients", "100% arabica coffee beans"), ("Origin", "Brazil and Colombia"), ("Roast", "Medium"), ("Best before", "12 months from roast date")], color: "#5c3d2e", emoji: "\u{2615}" },
  Seed { id: 7, name: "Dark chocolate bar, 70%, pack of 5", brand: "Tony's", category: "food", price_cents: 1275, list_price_cents: Some(1495), stock: 41, rating: 4.9, reviews: 12045, description: "Five 180 g bars of slave-free dark chocolate with a snap that is worth the price of admission.", tags: &["food", "chocolate", "snack"], attributes: &[("Ingredients", "Cocoa mass, sugar, cocoa butter, emulsifier (soy lecithin)"), ("Allergens", "May contain milk, nuts"), ("Net weight", "5 x 180 g"), ("Vegan", "Yes")], color: "#7b2d26", emoji: "\u{1f36b}" },
  Seed { id: 8, name: "Sea salt and rosemary crackers", brand: "Peter's Yard", category: "food", price_cents: 395, list_price_cents: None, stock: 0, rating: 4.4, reviews: 688, description: "Thin sourdough crackers baked with rosemary and flaked sea salt. Good with soft cheese and better on their own.", tags: &["food", "snack", "bakery"], attributes: &[("Ingredients", "Wheat flour, rye flour, rosemary, sea salt, yeast, water"), ("Allergens", "Wheat, rye (gluten)"), ("Net weight", "90 g")], color: "#c9a66b", emoji: "\u{1f96f}" },
  Seed { id: 9, name: "USB-C hub, 7 in 1", brand: "Anker", category: "tech", price_cents: 3499, list_price_cents: Some(4499), stock: 18, rating: 4.5, reviews: 24310, description: "HDMI at 4K60, two USB-A ports, a USB-C data port, 100 W pass-through charging and SD plus microSD readers in an aluminium shell.", tags: &["tech", "usb-c", "accessory"], attributes: &[("Ports", "HDMI, 2 x USB-A 3.1, USB-C data, USB-C PD, SD, microSD"), ("Video", "4K at 60 Hz"), ("Charging", "100 W pass-through"), ("Cable", "Integrated, 15 cm")], color: "#1c1c1c", emoji: "\u{1f50c}" },
  Seed { id: 10, name: "Mechanical keyboard, 75%, wireless", brand: "Keychron", category: "tech", price_cents: 10900, list_price_cents: None, stock: 6, rating: 4.6, reviews: 8931, description: "A hot-swappable 75% board with a gasket mount, PBT keycaps and a 4000 mAh battery. Bluetooth 5.1 to three devices or USB-C wired.", tags: &["tech", "keyboard", "wireless"], attributes: &[("Layout", "75%, 84 keys, ANSI"), ("Switches", "Gateron Brown, hot-swappable"), ("Connectivity", "Bluetooth 5.1, USB-C"), ("Battery", "4000 mAh, about 100 hours")], color: "#3a506b", emoji: "\u{2328}\u{fe0f}" },
  Seed { id: 11, name: "Noise cancelling headphones", brand: "Sony", category: "tech", price_cents: 27900, list_price_cents: Some(34900), stock: 3, rating: 4.7, reviews: 41120, description: "Over-ear headphones with adaptive noise cancelling, thirty hours of battery and multipoint pairing. Folds flat into the included case.", tags: &["tech", "audio", "wireless"], attributes: &[("Driver", "30 mm"), ("Battery", "30 hours with ANC"), ("Codecs", "SBC, AAC, LDAC"), ("Weight", "250 g")], color: "#222831", emoji: "\u{1f3a7}" },
  Seed { id: 12, name: "The Rust Programming Language, 2nd edition", brand: "No Starch Press", category: "books", price_cents: 3995, list_price_cents: None, stock: 14, rating: 4.8, reviews: 2760, description: "The official book, updated for Rust 2021. Ownership, traits, lifetimes, closures, concurrency and a final project that builds a web server.", tags: &["books", "programming", "rust"], attributes: &[("Authors", "Steve Klabnik, Carol Nichols"), ("Pages", "560"), ("ISBN", "978-1718503106"), ("Format", "Paperback")], color: "#b7472a", emoji: "\u{1f4d5}" },
  Seed { id: 13, name: "Designing Data-Intensive Applications", brand: "O'Reilly", category: "books", price_cents: 4450, list_price_cents: Some(5399), stock: 9, rating: 4.8, reviews: 6120, description: "Replication, partitioning, transactions, batch and stream processing, and the trade-offs behind every storage engine you will ever pick.", tags: &["books", "programming", "databases"], attributes: &[("Author", "Martin Kleppmann"), ("Pages", "616"), ("ISBN", "978-1449373320"), ("Format", "Paperback")], color: "#2a6f97", emoji: "\u{1f4d8}" },
  Seed { id: 14, name: "Salt, Fat, Acid, Heat", brand: "Canongate", category: "books", price_cents: 2200, list_price_cents: None, stock: 21, rating: 4.7, reviews: 15980, description: "The four elements of good cooking, explained once and for all, with the illustrated charts that turn a recipe into a method.", tags: &["books", "cooking"], attributes: &[("Author", "Samin Nosrat"), ("Pages", "480"), ("ISBN", "978-1782112303"), ("Format", "Hardcover")], color: "#e07a5f", emoji: "\u{1f373}" },
];

/// The categories the storefront filters by, in display order.
pub const CATEGORIES: &[&str] = &["printing", "tools", "food", "tech", "books"];

#[derive(Debug, Clone, Default)]
pub struct Filter<'a> {
  pub q: Option<&'a str>,
  pub category: Option<&'a str>,
  pub tag: Option<&'a str>,
}

impl Product {
  fn matches(&self, filter: &Filter<'_>) -> bool {
    if let Some(category) = filter.category.filter(|c| !c.is_empty()) {
      if self.category != category {
        return false;
      }
    }
    if let Some(tag) = filter.tag.filter(|t| !t.is_empty()) {
      if !self.tags.iter().any(|owned| owned == tag) {
        return false;
      }
    }
    if let Some(q) = filter.q.map(str::trim).filter(|q| !q.is_empty()) {
      let q = q.to_lowercase();
      let haystack = [self.name.as_str(), self.brand.as_str(), self.description.as_str(), self.category.as_str()]
        .into_iter()
        .chain(self.tags.iter().map(String::as_str))
        .chain(self.attributes.iter().map(|a| a.value.as_str()));
      let mut words = q.split_whitespace();
      let fields: Vec<String> = haystack.map(str::to_lowercase).collect();
      if !words.all(|w| fields.iter().any(|f| f.contains(w))) {
        return false;
      }
    }
    true
  }
}

impl Catalog {
  pub fn seed() -> Self {
    let products = SEED.iter().map(Seed::build).collect();
    Self(Arc::new(Mutex::new(State { products, next_order: 5001 })))
  }

  pub fn list(&self, filter: &Filter<'_>) -> Vec<Product> {
    self.0.lock().products.iter().filter(|p| p.matches(filter)).cloned().collect()
  }

  pub fn get(&self, id: u64) -> Option<Product> {
    self.0.lock().products.iter().find(|p| p.id == id).cloned()
  }

  /// Every line is checked before any stock moves, so a rejected order leaves
  /// the catalog untouched.
  pub fn place(&self, request: &OrderRequest) -> Result<Order, OrderError> {
    if request.lines.is_empty() {
      return Err(OrderError::Empty);
    }
    let mut state = self.0.lock();

    for line in &request.lines {
      let Some(product) = state.products.iter().find(|p| p.id == line.product_id) else {
        return Err(OrderError::UnknownProduct(line.product_id));
      };
      if product.stock < line.quantity {
        return Err(OrderError::OutOfStock {
          product_id: product.id,
          wanted: line.quantity,
          held: product.stock,
        });
      }
    }

    let mut lines = Vec::with_capacity(request.lines.len());
    let mut total_cents = 0;
    for line in &request.lines {
      let product = state
        .products
        .iter_mut()
        .find(|p| p.id == line.product_id)
        .expect("every line was checked");
      product.stock -= line.quantity;
      let line_cents = product.price_cents * i64::from(line.quantity);
      total_cents += line_cents;
      lines.push(PlacedLine {
        product_id: product.id,
        name: product.name.clone(),
        quantity: line.quantity,
        line_cents,
      });
    }

    let id = state.next_order;
    state.next_order += 1;
    Ok(Order { id, total_cents, lines })
  }
}
