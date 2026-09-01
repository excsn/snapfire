use std::sync::Arc;

use futures::executor::block_on;
use shopping_react_ts::server::{build_app_over, render, services, RenderMode};
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_service::{MockTransport, Services};

fn product(id: i64, name: &str, price: i64, stock: i64) -> Value {
  let mut map = ValueMap::new();
  map.insert("id".to_owned(), Value::int(id));
  map.insert("name".to_owned(), Value::str(name));
  map.insert("price_cents".to_owned(), Value::int(price));
  map.insert("stock".to_owned(), Value::int(stock));
  map.insert("tags".to_owned(), Value::Seq(vec![Value::str("printing")]));
  Value::Map(map)
}

fn app_over(transport: Arc<MockTransport>) -> shopping_react_ts::server::AppCore {
  let imported = services::import();
  let services = Services::builder()
    .contract(imported.contract)
    .default_transport(transport)
    .build();
  build_app_over(services)
}

#[test]
fn the_published_document_is_the_only_client_description() {
  let imported = services::import();
  let contract = &imported.contract;

  for method in ["listProducts", "getProduct", "placeOrder"] {
    assert!(contract.method(services::SHOPPING, method).is_some(), "{method} was imported");
  }
  assert_eq!(imported.routes.len(), 3, "every operation carries its transport shape");
}

#[test]
fn a_route_ships_data_and_a_client_module_with_no_server_javascript() {
  let transport = Arc::new(
    MockTransport::new().returns("shopping.listProducts", Value::Seq(vec![product(1, "Filament", 2400, 12)])),
  );
  let app = app_over(transport);
  let html = block_on(render(&app, "/", RenderMode::Html)).unwrap();

  assert!(html.starts_with("<!--sf-g:shell#document--><!doctype html>"), "{}", &html[..80]);
  assert!(html.contains("data-sf-module=\"app/main.tsx#Catalog\""), "the component is named for the browser");
  assert!(html.contains("Filament"), "the props carry the data the backend returned");
  assert!(!html.contains("<li>"), "the server renders no component markup");
}

#[test]
fn the_payload_carries_the_widths_the_document_declared() {
  let transport = Arc::new(
    MockTransport::new().returns("shopping.getProduct", product(1, "Filament", 2400, 12)),
  );
  let app = app_over(transport);
  let payload = block_on(render(&app, "/product/1", RenderMode::Payload)).unwrap();

  assert!(payload.starts_with("V {"), "the response opens with its format row");
  assert!(payload.contains("app/main.tsx#Product"));
  assert!(payload.contains("Filament"));
}

#[test]
fn a_failing_call_degrades_to_the_error_component() {
  let transport = Arc::new(MockTransport::new().fails(
    "shopping.getProduct",
    snapfire_fsr_runtime::FailureKind::NotFound,
    "no product 99",
  ));
  let app = app_over(transport);
  let html = block_on(render(&app, "/product/99", RenderMode::Html)).unwrap();

  assert!(html.contains("app/main.tsx#Failed"), "the plan's error module renders instead");
  assert!(html.contains("no product 99"), "the failure reaches the component as a prop");
  assert!(html.contains("<!doctype html>"), "the document around it still renders");
}

#[test]
fn a_call_the_contract_rejects_never_reaches_the_backend() {
  let transport = Arc::new(MockTransport::new().returns("shopping.getProduct", Value::Null));
  let app = app_over(transport.clone());

  let html = block_on(render(&app, "/product/notanumber", RenderMode::Html)).unwrap();
  assert!(html.contains("app/main.tsx#Failed"));
  assert!(transport.calls().is_empty(), "the loader refused before the wire");
}

#[test]
fn an_unmatched_path_is_not_found() {
  let app = app_over(Arc::new(MockTransport::new()));
  assert!(block_on(render(&app, "/nope", RenderMode::Html)).is_err());
}
