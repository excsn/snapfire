use std::sync::Arc;

use futures::executor::block_on;
use shopping_react_ts::routes::about_plan;
use snapfire_fsr_host::{Host, RenderMode};
use snapfire_fsr_runtime::SessionCell;
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_service::MockTransport;

fn product(id: i64, name: &str, price: i64, stock: i64) -> Value {
  let mut image = ValueMap::new();
  image.insert("color".to_owned(), Value::str("#2f3e46"));
  image.insert("emoji".to_owned(), Value::str("x"));
  let mut attribute = ValueMap::new();
  attribute.insert("name".to_owned(), Value::str("Diameter"));
  attribute.insert("value".to_owned(), Value::str("1.75 mm"));
  let mut map = ValueMap::new();
  map.insert("id".to_owned(), Value::int(id));
  map.insert("name".to_owned(), Value::str(name));
  map.insert("brand".to_owned(), Value::str("Polymaker"));
  map.insert("category".to_owned(), Value::str("printing"));
  map.insert("price_cents".to_owned(), Value::int(price));
  map.insert("stock".to_owned(), Value::int(stock));
  map.insert("rating".to_owned(), Value::F64(4.5));
  map.insert("reviews".to_owned(), Value::int(10));
  map.insert("description".to_owned(), Value::str("a spool"));
  map.insert("tags".to_owned(), Value::Seq(vec![Value::str("printing")]));
  map.insert("attributes".to_owned(), Value::Seq(vec![Value::Map(attribute)]));
  map.insert("image".to_owned(), Value::Map(image));
  Value::Map(map)
}

fn stock(id: i64) -> Value {
  let mut map = ValueMap::new();
  map.insert("product_id".to_owned(), Value::int(id));
  map.insert("on_hand".to_owned(), Value::int(12));
  map.insert("reserved".to_owned(), Value::int(0));
  map.insert("warehouse".to_owned(), Value::str("north"));
  map.insert("bins".to_owned(), Value::Seq(vec![Value::str("N-01")]));
  Value::Map(map)
}

fn cart_lines(session: &snapfire_fsr_runtime::SessionCell) -> ValueMap {
  match session.get("cart") {
    Some(Value::Map(map)) => map,
    _ => ValueMap::new(),
  }
}

fn hold(session: &snapfire_fsr_runtime::SessionCell, product_id: i64, quantity: i64) {
  let mut cart = cart_lines(session);
  cart.insert(product_id.to_string(), Value::int(quantity));
  session.insert("cart", Value::Map(cart));
}

fn app_over(transport: Arc<MockTransport>) -> Host {
  Host::from(env!("CARGO_MANIFEST_DIR"))
    .unwrap()
    .route("/about", about_plan())
    .services_over(transport)
    .build()
    .unwrap()
}

#[test]
fn the_published_document_is_the_only_client_description() {
  let imported = snapfire_fsr_service::import(shopping_react_ts::backend::shopping::DOCUMENT, "shopping").unwrap();
  let contract = &imported.contract;

  for method in ["listProducts", "getProduct", "placeOrder"] {
    assert!(contract.method("shopping", method).is_some(), "{method} was imported");
  }
  assert_eq!(imported.routes.len(), 3, "every operation carries its transport shape");
}

#[test]
fn a_route_ships_data_and_a_client_module_with_no_server_javascript() {
  let transport = Arc::new(
    MockTransport::new().returns("shopping.listProducts", Value::Seq(vec![product(1, "Filament", 2400, 12)])),
  );
  let app = app_over(transport);
  let html = block_on(app.render_to_string("/", RenderMode::Html, SessionCell::default())).unwrap();

  assert!(html.starts_with("<!--sf-g:shell#document--><!doctype html>"), "{}", &html[..80]);
  assert!(html.contains("data-sf-module=\"routes/index/page.tsx#default\""), "the component is named for the browser");
  assert!(html.contains("Filament"), "the props carry the data the backend returned");
  assert!(!html.contains("<li>"), "the server renders no component markup");
}

#[test]
fn the_payload_carries_the_widths_the_document_declared() {
  let transport = Arc::new(
    MockTransport::new().returns("shopping.getProduct", product(1, "Filament", 2400, 12)).returns("inventory.getStock", stock(1)),
  );
  let app = app_over(transport);
  let payload = block_on(app.render_to_string("/product/1", RenderMode::Payload, SessionCell::default())).unwrap();

  assert!(payload.starts_with("V {"), "the response opens with its format row");
  assert!(payload.contains("routes/product/[id]/page.tsx#default"));
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
  let html = block_on(app.render_to_string("/product/99", RenderMode::Html, SessionCell::default())).unwrap();

  assert!(html.contains("routes/error.tsx#default"), "the plan's error module renders instead");
  assert!(html.contains("no product 99"), "the failure reaches the component as a prop");
  assert!(html.contains("<!doctype html>"), "the document around it still renders");
}

#[test]
fn a_call_the_contract_rejects_never_reaches_the_backend() {
  let transport = Arc::new(MockTransport::new().returns("shopping.getProduct", Value::Null).returns("inventory.getStock", stock(0)));
  let app = app_over(transport.clone());

  let html = block_on(app.render_to_string("/product/notanumber", RenderMode::Html, SessionCell::default())).unwrap();
  assert!(html.contains("routes/error.tsx#default"));
  assert!(transport.calls().is_empty(), "the loader refused before the wire");
}

#[test]
fn an_unmatched_path_is_not_found() {
  let app = app_over(Arc::new(MockTransport::new()));
  assert!(block_on(app.render_to_string("/nope", RenderMode::Html, SessionCell::default())).is_err());
}

#[test]
fn routes_come_from_the_plan_file_and_from_rust() {
  let routes = snapfire_fsr::Routes::from_manifest(&shopping_react_ts::routes::plan())
    .unwrap()
    .add("/about", shopping_react_ts::routes::about_plan());

  assert_eq!(routes.patterns(), vec!["/", "/cart", "/product/{id}", "/about"]);
  assert!(routes.build().is_ok());
}

#[test]
fn a_pattern_claimed_twice_is_refused_unless_it_is_an_override() {
  use shopping_react_ts::routes::{about_plan, plan};
  use snapfire_fsr::Routes;

  let clash = Routes::from_manifest(&plan()).unwrap().add("/", about_plan()).build();
  let message = match clash {
    Ok(_) => panic!("the plan file already claims /"),
    Err(e) => e.to_string(),
  };
  assert!(message.contains("mark the Rust one as an override"), "{message}");

  let deliberate = Routes::from_manifest(&plan()).unwrap().replace("/", about_plan()).build();
  assert!(deliberate.is_ok(), "an override is allowed");
  drop(deliberate);
}

#[test]
fn the_plan_file_names_what_a_host_must_bind() {
  let manifest = snapfire_fsr_plan::Manifest::from_json(&shopping_react_ts::routes::plan()).unwrap();
  assert_eq!(manifest.sources(), vec!["index", "cart", "product"]);
  assert_eq!(manifest.action_ids(), vec!["cart.addToCart", "cart.removeFromCart", "cart.checkout"], "actions are declared, so an unanswered one is a boot error");
  assert!(manifest.modules().contains(&"routes/index/page.tsx#default".to_owned()));
  assert!(manifest.modules().contains(&"routes/error.tsx#default".to_owned()), "error modules count");
}

#[test]
fn a_route_added_in_rust_renders_like_any_other() {
  let app = app_over(Arc::new(MockTransport::new()));
  let html = block_on(app.render_to_string("/about", RenderMode::Html, SessionCell::default())).unwrap();
  assert!(html.contains("src/About.tsx#default"));
  assert!(html.contains("<!doctype html>"));
}

#[test]
fn adding_to_the_cart_holds_it_in_the_session() {
  use snapfire_fsr_runtime::SessionCell;

  let app = app_over(Arc::new(MockTransport::new()));
  let session = SessionCell::default();

  let mut input = ValueMap::new();
  input.insert("product_id".to_owned(), Value::int(1i64));
  input.insert("quantity".to_owned(), Value::int(2i64));
  block_on(app.call_action("cart.addToCart", session.clone(), Value::Map(input)))
    .unwrap();

  let held = cart_lines(&session);
  assert_eq!(held.get("1"), Some(&Value::Int(2)));
}

#[test]
fn the_cart_page_names_and_prices_what_the_session_holds() {
  use snapfire_fsr_runtime::SessionCell;

  let transport = Arc::new(
    MockTransport::new().returns("shopping.listProducts", Value::Seq(vec![product(1, "Filament", 2400, 12)])),
  );
  let app = app_over(transport);
  let session = SessionCell::default();
  hold(&session, 1, 3);

  let chunks: Vec<String> = block_on(async {
    use futures_util::StreamExt;
    app.render("/cart", RenderMode::Html, session)
      .await
      .unwrap()
      .collect()
      .await
  });
  let html = chunks.concat();

  assert!(html.contains("routes/cart/page.tsx#default"));
  assert!(html.contains("Filament"), "the name came from the catalog, not the cart");
  assert!(html.contains("2400"), "so did the price");
}

#[test]
fn checkout_places_the_order_and_empties_the_cart() {
  use snapfire_fsr_runtime::SessionCell;

  let mut order = ValueMap::new();
  order.insert("id".to_owned(), Value::int(5001i64));
  order.insert("total_cents".to_owned(), Value::int(4800i64));
  order.insert("lines".to_owned(), Value::Seq(vec![]));

  let transport = Arc::new(MockTransport::new().returns("shopping.placeOrder", Value::Map(order)));
  let app = app_over(transport.clone());
  let session = SessionCell::default();
  hold(&session, 1, 2);

  let placed = block_on(app.call_action(
    "cart.checkout",
    session.clone(),
    Value::Map(ValueMap::new()),
  ))
  .unwrap();

  assert!(matches!(placed, Value::Map(_)));
  assert!(cart_lines(&session).is_empty(), "a placed order empties the cart");
  let (path, args, _) = transport.calls().into_iter().next().unwrap();
  assert_eq!(path, "shopping.placeOrder");
  assert!(matches!(args.get("lines"), Some(Value::Seq(lines)) if lines.len() == 1));
}

#[test]
fn checking_out_an_empty_cart_never_reaches_the_backend() {
  use snapfire_fsr_runtime::{FailureKind, SessionCell};

  let transport = Arc::new(MockTransport::new());
  let app = app_over(transport.clone());

  let err = block_on(app.call_action(
    "cart.checkout",
    SessionCell::default(),
    Value::Map(ValueMap::new()),
  ))
  .unwrap_err();

  assert_eq!(err.kind, FailureKind::Invalid);
  assert!(transport.calls().is_empty());
}

#[test]
fn a_declared_action_nothing_answers_refuses_to_start() {
  let manifest = r#"{"version":1,"routes":[{"pattern":"/","plan":{"id":0,"module":"shell#document"}}],
    "actions":["never_bound"]}"#;
  let err = snapfire_fsr::App::from_manifest(manifest)
    .unwrap()
    .evaluator(|_: &snapfire_fsr_core::ModuleId| true, Arc::new(snapfire_fsr_runtime::NullEvaluator))
    .build()
    .unwrap_err();

  assert_eq!(err, snapfire_fsr::BindError::UnboundAction { id: "never_bound".into() });
}

#[test]
fn an_action_input_the_schema_rejects_never_reaches_the_body() {
  use snapfire_fsr_runtime::{FailureKind, SessionCell};

  let app = app_over(Arc::new(MockTransport::new()));
  let session = SessionCell::default();
  let mut input = ValueMap::new();
  input.insert("product_id".to_owned(), Value::str("one"));
  input.insert("quantity".to_owned(), Value::int(1i64));

  let err = block_on(app.call_action("cart.addToCart", session.clone(), Value::Map(input))).unwrap_err();
  assert_eq!(err.kind, FailureKind::Invalid);
  assert!(err.message.contains("product_id"), "{}", err.message);
  assert!(cart_lines(&session).is_empty(), "the body never ran");
}

#[test]
fn the_catalog_filters_by_the_query_string() {
  let transport = Arc::new(MockTransport::new().returns("shopping.listProducts", Value::Seq(vec![])));
  let app = app_over(transport.clone());

  block_on(app.render_to_string("/?tag=printing&__payload", RenderMode::Html, SessionCell::default())).unwrap();
  let (_, args, _) = transport.calls().into_iter().next().unwrap();
  assert_eq!(args.get("tag"), Some(&Value::str("printing")), "the query reached the loader");

  block_on(app.render_to_string("/", RenderMode::Html, SessionCell::default())).unwrap();
  let (_, args, _) = transport.calls().into_iter().nth(1).unwrap();
  assert!(args.get("tag").is_none(), "no query, no argument");
}
