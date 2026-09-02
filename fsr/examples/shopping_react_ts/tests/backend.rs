use shopping_react_ts::backend::catalog::{Catalog, Filter, OrderError, OrderLine, OrderRequest, CATEGORIES};

fn order(lines: &[(u64, u32)]) -> OrderRequest {
  OrderRequest {
    lines: lines.iter().map(|(product_id, quantity)| OrderLine { product_id: *product_id, quantity: *quantity }).collect(),
  }
}

#[test]
fn the_catalog_lists_and_filters_by_tag_category_and_search() {
  let catalog = Catalog::seed();
  let all = catalog.list(&Filter::default());
  assert_eq!(all.len(), 14);
  assert!(all.iter().all(|p| CATEGORIES.contains(&p.category.as_str())), "every product sits in a known category");
  assert!(all.iter().all(|p| !p.description.is_empty() && !p.attributes.is_empty()), "every product is described");

  assert_eq!(catalog.list(&Filter { tag: Some("tools"), ..Filter::default() }).len(), 2);
  assert_eq!(catalog.list(&Filter { category: Some("books"), ..Filter::default() }).len(), 3);
  assert!(catalog.list(&Filter { tag: Some("nothing"), ..Filter::default() }).is_empty());

  let coffee = catalog.list(&Filter { q: Some("Espresso"), ..Filter::default() });
  assert_eq!(coffee.len(), 1, "search is case-insensitive over the name");
  assert_eq!(coffee[0].id, 6);
  let kleppmann = catalog.list(&Filter { q: Some("kleppmann"), ..Filter::default() });
  assert_eq!(kleppmann.len(), 1, "search reaches attribute values");
  let wireless_tech = catalog.list(&Filter { q: Some("wireless"), category: Some("tech"), ..Filter::default() });
  assert_eq!(wireless_tech.len(), 2);
  assert!(catalog.list(&Filter { q: Some("wireless"), category: Some("food"), ..Filter::default() }).is_empty());
  assert_eq!(catalog.list(&Filter { q: Some("  "), category: Some(""), ..Filter::default() }).len(), 14, "blank filters are no filter");

  assert_eq!(catalog.get(1).unwrap().name, "PLA filament, 1 kg spool");
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
    serde_json::from_str(shopping_react_ts::backend::shopping::DOCUMENT).unwrap();

  for path in ["/products", "/products/{id}", "/orders"] {
    assert!(doc["paths"].get(path).is_some(), "{path} is documented");
  }
  assert_eq!(doc["paths"]["/products"]["get"]["operationId"], "listProducts");
  assert_eq!(doc["paths"]["/orders"]["post"]["operationId"], "placeOrder");
  for schema in ["Attribute", "Image", "Product", "OrderLine", "OrderRequest", "PlacedLine", "Order"] {
    assert!(doc["components"]["schemas"].get(schema).is_some(), "{schema} is defined");
  }
}
