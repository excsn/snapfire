use std::sync::Arc;

use futures::executor::block_on;
use futures::StreamExt;
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

  for method in ["listProducts", "getProduct", "placeOrder", "getOrder"] {
    assert!(contract.method("shopping", method).is_some(), "{method} was imported");
  }
  assert_eq!(imported.routes.len(), 4, "every operation carries its transport shape");
}

#[test]
fn a_route_renders_its_page_on_the_server_with_no_javascript_engine() {
  let transport = Arc::new(
    MockTransport::new().returns("shopping.listProducts", Value::Seq(vec![product(1, "Filament", 2400, 12)])),
  );
  let app = app_over(transport);
  let html = block_on(app.render_to_string("/", RenderMode::Html, SessionCell::default())).unwrap();

  assert!(html.starts_with("<!--sf-g:shell#document--><!doctype html>"), "{}", &html[..80]);
  assert!(html.contains("data-sf-module=\"routes/index/page.tsx#default\""), "the component is named for the browser");
  assert!(html.contains("<h2 class=\"card-title\"><a href=\"/product/1\">Filament</a></h2>"), "the page's own markup is rendered from the lowered tree: {html}");
  assert!(html.contains("<span class=\"price\">$24.00</span>"), "a module helper ran in Rust: {html}");
  assert!(html.contains("<span class=\"stars-rating\">4.5</span>"), "a component the page imports rendered too");
  assert!(html.contains("<h1>Today&#39;s picks</h1>") || html.contains("<h1>Today's picks</h1>"), "text is the component's");
  assert!(html.contains("data-sf-props=\"sf-i0\""), "the props still ship, so the browser hydrates rather than mounts");
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
fn a_route_with_a_loading_module_ships_the_document_before_its_page() {
  let transport = Arc::new(
    MockTransport::new().returns("shopping.getProduct", product(1, "Filament", 2400, 12)).returns("inventory.getStock", stock(1)),
  );
  let app = app_over(transport);
  let parts: Vec<String> = block_on(async { app.render("/product/1", RenderMode::Html, SessionCell::default()).await.unwrap().collect().await });

  assert_eq!(parts.len(), 2, "the document, then one fill: {parts:?}");
  assert!(parts[0].contains("data-sf-module=\"routes/product/[id]/loading.tsx#default\""), "the loading module holds the slot: {}", parts[0]);
  assert!(parts[0].contains("<div class=\"skeleton skeleton-thumb\"></div>"), "rendered in Rust like any component");
  assert!(!parts[0].contains("data-sf-module=\"routes/product/[id]/page.tsx#default\""), "the page waits for its loader; the sidecar alone names it");
  assert!(parts[1].starts_with("<template data-sf-fill=\"1\">"), "{}", parts[1]);
  assert!(parts[1].contains("data-sf-module=\"routes/product/[id]/page.tsx#default\""));
  assert!(parts[1].contains("Filament"));
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
  assert!(transport.calls().iter().all(|(path, _, _)| path != "shopping.getProduct"), "the loader refused before the wire: {:?}", transport.calls());
}

#[test]
fn an_unmatched_path_renders_the_not_found_page() {
  let app = app_over(Arc::new(MockTransport::new()));
  assert!(block_on(app.render_to_string("/nope", RenderMode::Html, SessionCell::default())).is_err());
  let chunks = block_on(app.render_not_found("/nope?x=1", RenderMode::Html, SessionCell::default())).unwrap().expect("routes/not-found.tsx is the page");
  let html = block_on(chunks.collect::<Vec<String>>()).concat();
  assert!(html.contains("data-sf-module=\"routes/not-found.tsx#default\""), "{html}");
  assert!(html.contains("No page at <!-- -->/nope"), "the path reaches the page as params.path: {html}");
}

#[test]
fn routes_come_from_the_plan_file_and_from_rust() {
  let routes = snapfire_fsr::Routes::from_manifest(&shopping_react_ts::routes::plan())
    .unwrap()
    .add("/about", shopping_react_ts::routes::about_plan());

  assert_eq!(routes.patterns(), vec!["/", "/cart", "/order/{id}", "/product/{id}", "/about"]);
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
  assert_eq!(manifest.sources(), vec!["layout", "index", "layout.promo", "cart", "order", "product"], "the root layout's loader and its promo slot's are sources like any other");
  assert_eq!(manifest.action_ids(), vec!["cart.addToCart", "cart.removeFromCart", "cart.checkout"], "actions are declared, so an unanswered one is a boot error");
  assert!(manifest.modules().contains(&"routes/index/page.tsx#default".to_owned()));
  assert!(manifest.modules().contains(&"routes/error.tsx#default".to_owned()), "error modules count");
  assert!(manifest.modules().contains(&"routes/not-found.tsx#default".to_owned()), "the not-found tree counts");
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
fn the_order_page_reads_the_placed_order_back() {
  let mut line = ValueMap::new();
  line.insert("product_id".to_owned(), Value::int(1i64));
  line.insert("name".to_owned(), Value::str("Filament"));
  line.insert("quantity".to_owned(), Value::int(2i64));
  line.insert("line_cents".to_owned(), Value::int(4800i64));
  let mut order = ValueMap::new();
  order.insert("id".to_owned(), Value::int(5001i64));
  order.insert("total_cents".to_owned(), Value::int(4800i64));
  order.insert("lines".to_owned(), Value::Seq(vec![Value::Map(line)]));
  let transport = Arc::new(MockTransport::new().returns("shopping.getOrder", Value::Map(order)));
  let app = app_over(transport.clone());

  let html = block_on(app.render_to_string("/order/5001", RenderMode::Html, SessionCell::default())).unwrap();
  assert!(html.contains("data-sf-module=\"routes/order/[id]/page.tsx#default\""), "{html}");
  assert!(html.contains("Order #<!-- -->5001<!-- --> placed"), "the heading is rendered in Rust: {html}");
  assert!(html.contains("<a href=\"/product/1\">Filament</a>"), "each line links back to its product");
  assert!(html.contains("$48.00"));
  let calls = transport.calls();
  assert_eq!(calls.len(), 2, "the order's loader and the promo slot's");
  assert_eq!(calls[0].0, "shopping.getOrder");
}

#[test]
fn a_component_placed_as_an_island_renders_in_its_own_region_inside_the_page() {
  let mut order = ValueMap::new();
  order.insert("id".to_owned(), Value::int(5001i64));
  order.insert("total_cents".to_owned(), Value::int(4800i64));
  order.insert("lines".to_owned(), Value::Seq(Vec::new()));
  let transport = Arc::new(MockTransport::new().returns("shopping.getOrder", Value::Map(order)));
  let app = app_over(transport);
  let html = block_on(app.render_to_string("/order/5001", RenderMode::Html, SessionCell::default())).unwrap();
  let region = html.find("<sf-s data-sf-island data-sf-when=\"visible\"><sf-i id=\"sf-i3\" data-sf-module=\"src/ui/OrderHelp.tsx#OrderHelp\">").expect(&html);
  let page = html.find("data-sf-module=\"routes/order/[id]/page.tsx#default\"").unwrap();
  assert!(page < region, "the island sits inside the page's markup");
  assert!(html[region..].contains("<p>Quote order #<!-- -->5001<!-- --> when you write to us.</p>"), "rendered in Rust with the page's data: {html}");
  assert!(html[region..].contains("</sf-i><script type=\"application/json\" data-sf-props=\"sf-i3\">{\"orderId\":5001}</script></sf-s>"), "its own props script, inside the region: {html}");
  let payload = block_on(app.render_to_string("/order/5001", RenderMode::Payload, SessionCell::default())).unwrap();
  assert!(payload.contains("[\"c\",{\"m\":\"src/ui/OrderHelp.tsx#OrderHelp\""), "a nested client node on the wire: {payload}");
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
fn a_route_handler_answers_a_request_with_a_value() {
  use snapfire_fsr_runtime::FailureKind;

  let app = app_over(Arc::new(MockTransport::new()));
  let session = SessionCell::default();
  hold(&session, 1, 2);

  let got = block_on(app.call_handler("GET", "/api/cart?x=1", session.clone(), Value::Null)).unwrap();
  let Value::Map(map) = got else { panic!("a map") };
  assert_eq!(map.get("count"), Some(&Value::int(2i64)));

  let mut input = ValueMap::new();
  input.insert("product_id".to_owned(), Value::int(3i64));
  input.insert("quantity".to_owned(), Value::int(1i64));
  let Value::Map(map) = block_on(app.call_handler("post", "/api/cart", session.clone(), Value::Map(input))).unwrap() else { panic!("a map") };
  assert_eq!(map.get("count"), Some(&Value::int(3i64)));
  assert_eq!(cart_lines(&session).get("3"), Some(&Value::int(1i64)), "the handler wrote the session");

  let mut bad = ValueMap::new();
  bad.insert("product_id".to_owned(), Value::str("three"));
  let err = block_on(app.call_handler("POST", "/api/cart", session.clone(), Value::Map(bad))).unwrap_err();
  assert_eq!(err.kind, FailureKind::Invalid, "the input is checked against AddToCart before the body runs");

  let err = block_on(app.call_handler("GET", "/api/nothing", session.clone(), Value::Null)).unwrap_err();
  assert_eq!(err.kind, FailureKind::NotFound);
  let err = block_on(app.call_handler("DELETE", "/api/cart", session, Value::Null)).unwrap_err();
  assert_eq!(err.kind, FailureKind::NotFound, "a method the file does not export is not a handler");
  assert_eq!(app.report().app.handlers, vec![("GET /api/cart".to_owned(), snapfire_fsr::Owner::Lowered), ("POST /api/cart".to_owned(), snapfire_fsr::Owner::Lowered)]);
}

#[test]
fn the_middleware_redirects_rewrites_and_adds_a_header() {
  use snapfire_fsr_host::{Preflight, PreflightAction};

  let app = app_over(Arc::new(MockTransport::new()));
  let redirect = block_on(app.preflight("GET", "/basket", SessionCell::default())).unwrap();
  assert_eq!(redirect, Preflight { action: PreflightAction::Redirect { to: "/cart".into(), status: 307 }, headers: Vec::new() });
  let rewrite = block_on(app.preflight("get", "/shop?q=x", SessionCell::default())).unwrap();
  assert_eq!(rewrite.action, PreflightAction::Rewrite("/".into()));
  let plain = block_on(app.preflight("POST", "/_sf/action/cart.checkout", SessionCell::default())).unwrap();
  assert_eq!(plain, Preflight { action: PreflightAction::Continue, headers: vec![("x-storefront".into(), "fsr".into())] });
  assert_eq!(app.report().app.middleware, Some(snapfire_fsr::Owner::Lowered));
}

#[test]
fn middleware_written_in_rust_must_override_the_lowered_one() {
  let refused = Host::from(env!("CARGO_MANIFEST_DIR"))
    .unwrap()
    .services_over(Arc::new(MockTransport::new()))
    .middleware(|_ctx, _request| async { Ok(Value::Null) })
    .build();
  let Err(err) = refused else { panic!("the plan lowers middleware.ts") };
  assert!(matches!(err, snapfire_fsr_host::HostError::Bind(snapfire_fsr::BindError::MiddlewareClaimed)), "{err}");
  let app = Host::from(env!("CARGO_MANIFEST_DIR"))
    .unwrap()
    .services_over(Arc::new(MockTransport::new()))
    .middleware_override(|_ctx, _request| async { Ok(Value::Null) })
    .build()
    .unwrap();
  assert_eq!(block_on(app.preflight("GET", "/basket", SessionCell::default())).unwrap().action, snapfire_fsr_host::PreflightAction::Continue);
  assert_eq!(app.report().app.middleware, Some(snapfire_fsr::Owner::RustOverride));
}

#[test]
fn a_route_that_reads_nothing_of_the_request_is_prerendered_once() {
  let out = std::env::temp_dir().join(format!("fsr-prerender-{}", std::process::id()));
  let _ = std::fs::remove_dir_all(&out);
  let app = Host::from(env!("CARGO_MANIFEST_DIR"))
    .unwrap()
    .route("/about", about_plan())
    .services_over(Arc::new(MockTransport::new()))
    .prerendered(&out)
    .build()
    .unwrap();
  assert_eq!(app.prerenderable(), ["/about"], "every storefront page reads the session's cart; the Rust route reads nothing");
  assert_eq!(app.prerendered("/about", RenderMode::Html), None, "nothing written yet");

  let written = block_on(app.prerender(&out)).unwrap();
  assert_eq!(written.iter().map(|(p, f)| (p.as_str(), f.strip_prefix(&out).unwrap().to_string_lossy().into_owned())).collect::<Vec<_>>(), vec![("/about", "about/index.html".to_owned()), ("/about", "about/index.payload".to_owned())]);
  let html = app.prerendered("/about?anything=1", RenderMode::Html).unwrap();
  assert!(html.contains("data-sf-module=\"src/About.tsx#default\""), "the document, with the Rust route's island: {html}");
  assert!(!html.contains("EventSource"), "a prerendered document never carries the live-refresh script: {html}");
  let served = block_on(app.render_to_string("/about", RenderMode::Html, SessionCell::default())).unwrap();
  assert!(served.contains("EventSource"), "a served development document does: {served}");
  assert!(app.prerendered("/about", RenderMode::Payload).unwrap().contains("src/About.tsx#default"));
  assert_eq!(app.prerendered("/cart", RenderMode::Html), None);
  let _ = std::fs::remove_dir_all(&out);
}

#[test]
fn a_loader_meta_titles_the_document_and_a_streamed_page_retitles_it_on_resolution() {
  let transport = Arc::new(
    MockTransport::new()
      .returns("shopping.listProducts", Value::Seq(vec![product(1, "Filament", 2400, 12)]))
      .returns("shopping.getProduct", product(1, "Nozzle", 1200, 3))
      .returns("inventory.getStock", stock(1)),
  );
  let app = app_over(transport);
  let html = block_on(app.render_to_string("/", RenderMode::Html, SessionCell::default())).unwrap();
  assert!(html.contains("<title>Today&#39;s picks · Shopping</title></head>") || html.contains("<title>Today's picks · Shopping</title></head>"), "{html}");
  let payload = block_on(app.render_to_string("/?q=nozzle", RenderMode::Payload, SessionCell::default())).unwrap();
  assert!(payload.contains("\nH {\"title\":\"Results for nozzle · Shopping\"}\n"), "{payload}");

  let parts: Vec<String> = block_on(async { app.render("/product/1", RenderMode::Html, SessionCell::default()).await.unwrap().collect().await });
  assert!(parts[0].contains("<title>Shopping</title>"), "the document ships with the default: {}", parts[0]);
  assert!(parts[1].ends_with(";__sfHead({\"title\":\"Nozzle · Shopping\",\"description\":\"Nozzle for $12.00\"})</script>"), "{}", parts[1]);
  let payload = block_on(app.render_to_string("/product/1", RenderMode::Payload, SessionCell::default())).unwrap();
  assert!(payload.ends_with("\nH {\"title\":\"Nozzle · Shopping\",\"description\":\"Nozzle for $12.00\"}\n"), "{payload}");
}

#[test]
fn the_layouts_store_seeds_the_cart_count_and_follows_the_session() {
  use snapfire_fsr_runtime::SessionCell;

  let transport = Arc::new(
    MockTransport::new()
      .returns("shopping.listProducts", Value::Seq(vec![product(1, "Filament", 2400, 12)]))
      .returns("shopping.getProduct", product(1, "Nozzle", 1200, 3))
      .returns("inventory.getStock", stock(1)),
  );
  let app = app_over(transport);

  let empty = block_on(app.render_to_string("/", RenderMode::Html, SessionCell::default())).unwrap();
  assert!(empty.contains("<script type=\"application/json\" data-sf-store>{\"cart/count\":{\"$\":\"f\",\"v\":0.0}}</script>"), "{empty}");
  assert!(empty.contains("badge badge-empty\">0<"), "the header renders from the seed: {empty}");

  let session = SessionCell::default();
  hold(&session, 1, 3);
  let held = block_on(app.render_to_string("/", RenderMode::Html, session.clone())).unwrap();
  assert!(held.contains("data-sf-store>{\"cart/count\":{\"$\":\"f\",\"v\":3.0}}</script>"), "{held}");
  assert!(held.contains("badge\">3<"), "{held}");

  let payload = block_on(app.render_to_string("/cart", RenderMode::Payload, session)).unwrap();
  assert!(payload.contains("\nT {\"cart/count\":{\"$\":\"f\",\"v\":3.0}}\n"), "the navigation carries the seed: {payload}");
}

#[test]
fn a_page_and_its_layout_are_cached_by_module_once_per_distinct_params() {
  let transport = Arc::new(
    MockTransport::new()
      .returns("shopping.listProducts", Value::Seq(vec![product(1, "Filament", 2400, 12)]))
      .returns("shopping.getProduct", product(1, "Nozzle", 1200, 3))
      .returns("inventory.getStock", stock(1)),
  );
  let app = app_over(transport.clone());
  let render = |path: &str| block_on(app.render_to_string(path, RenderMode::Html, SessionCell::default())).unwrap();

  let first = render("/");
  assert_eq!(render("/"), first);
  assert_eq!(transport.calls().iter().filter(|(p, _, _)| p == "shopping.listProducts").count(), 4, "data resolves before render, so a hit still asks the service, for the page and for the promo slot");
  assert_eq!(block_on(app.invalidate("routes/index/page.tsx#default")), 1, "two renders with one answer share an entry");
  assert_eq!(block_on(app.invalidate("routes/slots/promo/page.tsx#default")), 1, "a parallel slot is an entry of its own");
  assert_eq!(block_on(app.invalidate("routes/layout.tsx#default")), 1, "the layout's subtree, page and slots included, is its own entry");
  assert_eq!(block_on(app.invalidate("shell#document")), 0, "the shell uses the head and is never written");
  assert_eq!(block_on(app.invalidate("routes/index/page.tsx#default")), 0);

  render("/product/1");
  render("/product/2");
  assert_eq!(block_on(app.invalidate("routes/product/[id]/page.tsx#default")), 2, "one entry per distinct params");
  assert_eq!(block_on(app.invalidate("routes/layout.tsx#default")), 0, "the product page streams behind loading.tsx, so its layout never caches");
}

#[test]
fn a_handler_written_in_rust_sits_beside_the_lowered_ones() {
  let app = Host::from(env!("CARGO_MANIFEST_DIR"))
    .unwrap()
    .services_over(Arc::new(MockTransport::new()))
    .handler("GET", "/api/health", |_ctx, _input| async { Ok(Value::str("ok")) })
    .build()
    .unwrap();
  assert_eq!(block_on(app.call_handler("GET", "/api/health", SessionCell::default(), Value::Null)).unwrap(), Value::str("ok"));
  let refused = Host::from(env!("CARGO_MANIFEST_DIR"))
    .unwrap()
    .services_over(Arc::new(MockTransport::new()))
    .handler("GET", "/api/cart", |_ctx, _input| async { Ok(Value::Null) })
    .build();
  let Err(err) = refused else { panic!("the plan lowers GET /api/cart") };
  assert!(matches!(err, snapfire_fsr_host::HostError::Bind(snapfire_fsr::BindError::HandlerClaimed(_))), "{err}");
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

  let catalog_calls = || transport.calls().into_iter().filter(|(_, args, _)| args.get("tag") != Some(&Value::str("snack"))).collect::<Vec<_>>();
  block_on(app.render_to_string("/?tag=printing&__payload", RenderMode::Html, SessionCell::default())).unwrap();
  let (_, args, _) = catalog_calls().into_iter().next().unwrap();
  assert_eq!(args.get("tag"), Some(&Value::str("printing")), "the query reached the loader");

  block_on(app.render_to_string("/", RenderMode::Html, SessionCell::default())).unwrap();
  let (_, args, _) = catalog_calls().into_iter().nth(1).unwrap();
  assert!(args.get("tag").is_none(), "no query, no argument");
}

#[test]
fn a_soft_navigation_from_a_page_under_the_layout_opens_the_product_in_its_modal_slot() {
  let transport = Arc::new(
    MockTransport::new()
      .returns("shopping.listProducts", Value::Seq(vec![product(1, "Filament", 2400, 12)]))
      .returns("shopping.getProduct", product(1, "Filament", 2400, 12))
      .returns("inventory.getStock", stock(1)),
  );
  let host = app_over(transport);

  let payload = block_on(host.render_navigation_to_string("/product/1", Some("/?q=pla"), None, SessionCell::default())).unwrap();
  let sidecar = payload.lines().find(|l| l.starts_with("G ")).unwrap();
  assert!(sidecar.contains("\"keep\":[\"content\",\"promo\"]"), "the layout keeps its page and its promo: {sidecar}");
  assert!(sidecar.contains("\"n\":\"modal\""), "{sidecar}");
  assert!(payload.contains("routes/product/[id]/page.modal.tsx#default"), "{payload}");
  assert!(!payload.contains("routes/product/[id]/page.tsx#default"), "the page itself is not rendered: {payload}");
  assert!(!payload.contains("routes/index/page.tsx#default") && !payload.contains("routes/slots/promo/page.tsx#default"), "nothing kept is rendered: {payload}");
  assert!(payload.contains("H {\"title\":\"Filament · Shopping\""), "the variant's loader describes the document: {payload}");

  let full = block_on(host.render_navigation_to_string("/product/1", None, None, SessionCell::default())).unwrap();
  assert!(full.contains("routes/product/[id]/page.tsx#default") && !full.contains("page.modal"), "no origin means the document's rendering: {full}");

  let named = block_on(host.render_navigation_to_string("/product/1", None, Some("modal"), SessionCell::default())).unwrap();
  assert!(named.contains("page.modal.tsx"), "`into` names the slot outright: {named}");
  let other = block_on(host.render_navigation_to_string("/product/1", None, Some("drawer"), SessionCell::default())).unwrap();
  assert!(!other.contains("page.modal.tsx"), "a slot the route has no variant for is the page: {other}");

  let document = block_on(host.render_to_string("/product/1", RenderMode::Html, SessionCell::default())).unwrap();
  assert!(document.contains("<sf-s data-sf-name=\"modal\"></sf-s>"), "a document load leaves the modal slot empty: {document}");
  assert!(document.contains("routes/product/[id]/page.tsx#default"), "{document}");
}

#[test]
fn the_promo_slot_renders_beside_the_page_from_its_own_loader() {
  let transport = Arc::new(MockTransport::new().returns("shopping.listProducts", Value::Seq(vec![{
    let mut snack = product(8, "Crackers", 395, 3);
    if let Value::Map(map) = &mut snack {
      map.insert("tags".to_owned(), Value::Seq(vec![Value::str("food"), Value::str("snack")]));
    }
    snack
  }])));
  let host = app_over(transport.clone());
  let html = block_on(host.render_to_string("/", RenderMode::Html, SessionCell::default())).unwrap();
  assert!(html.contains("<sf-s data-sf-name=\"promo\"><!--sf-g:routes/slots/promo/page.tsx#default-->"), "the slot's segment sits in its region: {html}");
  assert!(html.contains("Snacks at the counter"), "{html}");
  assert!(html.contains("\"n\":\"promo\""), "the sidecar names it: {html}");
  assert_eq!(transport.calls().iter().filter(|(name, _, _)| name.ends_with("listProducts")).count(), 2, "the catalog's loader and the promo's each ran once");
  assert!(host.report.to_string().contains("layout.promo"), "{}", host.report);
}
