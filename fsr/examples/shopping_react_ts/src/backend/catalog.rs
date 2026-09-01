use std::sync::Arc;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Product {
  pub id: u64,
  pub name: String,
  pub price_cents: i64,
  pub stock: u32,
  pub tags: Vec<String>,
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

fn product(id: u64, name: &str, price_cents: i64, stock: u32, tags: &[&str]) -> Product {
  Product {
    id,
    name: name.to_owned(),
    price_cents,
    stock,
    tags: tags.iter().map(|t| (*t).to_owned()).collect(),
  }
}

impl Catalog {
  pub fn seed() -> Self {
    let products = vec![
      product(1, "Filament, PLA 1kg", 2400, 12, &["printing", "consumable"]),
      product(2, "Hotend, all metal", 5900, 4, &["printing", "part"]),
      product(3, "Build plate, textured", 3150, 0, &["printing", "part"]),
      product(4, "Calipers, digital", 2999, 7, &["tools"]),
      product(5, "Deburring tool", 1150, 23, &["tools", "consumable"]),
    ];
    Self(Arc::new(Mutex::new(State { products, next_order: 5001 })))
  }

  pub fn list(&self, tag: Option<&str>) -> Vec<Product> {
    let state = self.0.lock();
    state
      .products
      .iter()
      .filter(|p| tag.is_none_or(|t| p.tags.iter().any(|owned| owned == t)))
      .cloned()
      .collect()
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
