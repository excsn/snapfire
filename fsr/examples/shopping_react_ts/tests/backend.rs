use shopping_react_ts::backend::catalog::{Catalog, OrderError, OrderLine, OrderRequest};

fn order(lines: &[(u64, u32)]) -> OrderRequest {
  OrderRequest {
    lines: lines.iter().map(|(product_id, quantity)| OrderLine { product_id: *product_id, quantity: *quantity }).collect(),
  }
}

#[test]
fn the_catalog_lists_and_filters_by_tag() {
  let catalog = Catalog::seed();
  assert_eq!(catalog.list(None).len(), 5);
  assert_eq!(catalog.list(Some("tools")).len(), 2);
  assert!(catalog.list(Some("nothing")).is_empty());
  assert_eq!(catalog.get(1).unwrap().name, "Filament, PLA 1kg");
  assert!(catalog.get(99).is_none());
}

#[test]
fn placing_an_order_prices_it_and_moves_stock() {
  let catalog = Catalog::seed();
  let placed = catalog.place(&order(&[(1, 2), (4, 1)])).unwrap();

  assert_eq!(placed.total_cents, 2400 * 2 + 2999);
  assert_eq!(placed.lines.len(), 2);
  assert_eq!(catalog.get(1).unwrap().stock, 10);
  assert_eq!(catalog.get(4).unwrap().stock, 6);
}

#[test]
fn a_rejected_order_moves_no_stock() {
  let catalog = Catalog::seed();
  let err = catalog.place(&order(&[(1, 1), (3, 1)])).unwrap_err();

  assert_eq!(err, OrderError::OutOfStock { product_id: 3, wanted: 1, held: 0 });
  assert_eq!(err.status(), 409);
  assert_eq!(catalog.get(1).unwrap().stock, 12, "the line that would have succeeded did not move");
}

#[test]
fn order_failures_carry_the_status_a_ui_renders() {
  let catalog = Catalog::seed();
  assert_eq!(catalog.place(&order(&[])).unwrap_err().status(), 400);
  assert_eq!(catalog.place(&order(&[(99, 1)])).unwrap_err().status(), 404);
}

#[test]
fn order_ids_advance() {
  let catalog = Catalog::seed();
  let first = catalog.place(&order(&[(5, 1)])).unwrap().id;
  let second = catalog.place(&order(&[(5, 1)])).unwrap().id;
  assert_eq!(second, first + 1);
}

#[test]
fn the_published_document_describes_what_the_server_serves() {
  let doc: serde_json::Value =
    serde_json::from_str(shopping_react_ts::backend::openapi::DOCUMENT).unwrap();

  for path in ["/products", "/products/{id}", "/orders"] {
    assert!(doc["paths"].get(path).is_some(), "{path} is documented");
  }
  assert_eq!(doc["paths"]["/products"]["get"]["operationId"], "listProducts");
  assert_eq!(doc["paths"]["/orders"]["post"]["operationId"], "placeOrder");
  for schema in ["Product", "OrderLine", "OrderRequest", "PlacedLine", "Order"] {
    assert!(doc["components"]["schemas"].get(schema).is_some(), "{schema} is defined");
  }
}
