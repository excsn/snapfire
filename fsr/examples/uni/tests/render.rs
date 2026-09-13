use bytes::Bytes;
use http::Request;
use http_body_util::BodyExt;
use snapfire_fsr_host::Host;

async fn body(response: http::Response<snapfire_fsr_host::Body>) -> String {
  let bytes = response.into_body().collect().await.unwrap().to_bytes();
  String::from_utf8(bytes.to_vec()).unwrap()
}

fn host() -> Host {
  uni::build().expect("the host builds")
}

#[tokio::test]
async fn one_page_carries_a_react_island_a_vue_island_and_a_region_that_is_neither() {
  let host = host();
  let html = body(host.handle(Request::get("/board").body(Bytes::new()).unwrap()).await).await;

  assert!(html.contains(r#"data-sf-module="js/src/ui/Watch.tsx#default""#), "the masthead island is React: {html}");
  assert!(html.contains(r#"data-sf-module="js/src/ui/Holdings.vue#default""#), "the page island is Vue: {html}");
  assert!(html.contains(r#"hx-get="?__fragment=tape""#), "the tape asks the host for itself");
  assert!(!html.contains("data-sf-module=\"tape"), "and mounts nothing");

  assert!(html.contains(r#"data-sf-module="js/src/ui/Lot.tsx#default""#), "the server-mode island is there too");
  assert!(html.contains(r#"data-sf-mode="server""#), "declared in the markup, which is where the browser reads it");
  // The marker is empty: a Tera template places the module, and nothing fills
  // a client node another evaluator would have to render. The browser asks
  // the host for the first render when it mounts, which is what the step
  // below answers.
  assert!(html.contains(r#"data-sf-module="js/src/ui/Lot.tsx#default"></sf-i>"#), "placed empty: {html}");

  let markers = html.matches("<sf-i").count();
  assert_eq!(markers, 3, "three placements, each a different mounting model: {html}");
}

#[tokio::test]
async fn a_step_of_the_server_mode_island_dispatches_the_action_its_handler_calls() {
  let host = host();
  let stepped = host
    .handle(
      http::Request::post("/_sf/island/js%2Fsrc%2Fui%2FLot.tsx%23default")
        .header(http::header::CONTENT_TYPE, "application/json")
        .body(Bytes::from(r#"{"props":{"size":10},"handler":1}"#))
        .unwrap(),
    )
    .await;
  let cookie = stepped
    .headers()
    .get(http::header::SET_COOKIE)
    .and_then(|v| v.to_str().ok())
    .and_then(|v| v.split(';').next())
    .map(str::to_owned)
    .expect("the step carries the session it wrote");
  let answer = body(stepped).await;
  assert!(answer.contains(r#""revalidate":true"#), "the handler called `desk.lot`, so the page's data is stale: {answer}");
  assert!(answer.contains("<output>10</output>"), "the island itself renders from the props it was given: {answer}");
  assert!(!answer.contains("\"kind\""), "{answer}");

  let html = body(host.handle(Request::get("/board").header(http::header::COOKIE, &cookie).body(Bytes::new()).unwrap()).await).await;
  assert!(html.contains(r#""lot":20"#), "the action wrote the session the masthead is rendered from: {html}");
  assert!(html.contains(r#""size":20"#), "and the stepper's own props follow: {html}");
}

#[tokio::test]
async fn the_lot_is_what_a_buy_takes() {
  let host = host();
  let step = |by: i64| {
    http::Request::post("/_sf/action/desk.lot")
      .header(http::header::CONTENT_TYPE, "application/json")
      .body(Bytes::from(format!(r#"{{"by":{by}}}"#)))
      .unwrap()
  };
  let first = host.handle(step(10)).await;
  let cookie = first
    .headers()
    .get(http::header::SET_COOKIE)
    .and_then(|v| v.to_str().ok())
    .and_then(|v| v.split(';').next())
    .map(str::to_owned)
    .expect("the action carries the session");
  assert_eq!(body(first).await, r#"{"lot":20}"#);
  let mut request = step(1000);
  request.headers_mut().insert(http::header::COOKIE, cookie.parse().unwrap());
  assert_eq!(body(host.handle(request).await).await, r#"{"lot":100}"#, "clamped: the value is input");

  let bought = host
    .handle(
      http::Request::post("/_sf/action/desk.buy")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(http::header::COOKIE, &cookie)
        .body(Bytes::from(r#"{"symbol":"KLNS"}"#))
        .unwrap(),
    )
    .await;
  assert_eq!(body(bought).await, r#"{"symbol":"KLNS","shares":100}"#);
}

#[tokio::test]
async fn watching_a_symbol_is_kept_by_the_session() {
  let host = host();
  let watched = host
    .handle(
      http::Request::post("/_sf/action/desk.watch")
        .header(http::header::CONTENT_TYPE, "application/json")
        .body(Bytes::from(r#"{"symbol":"MRSH"}"#))
        .unwrap(),
    )
    .await;
  let cookie = watched
    .headers()
    .get(http::header::SET_COOKIE)
    .and_then(|v| v.to_str().ok())
    .and_then(|v| v.split(';').next())
    .map(str::to_owned)
    .expect("the action carries the session");
  assert_eq!(body(watched).await, r#"{"symbol":"MRSH"}"#);
  let html = body(host.handle(Request::get("/board").header(http::header::COOKIE, &cookie).body(Bytes::new()).unwrap()).await).await;
  assert!(html.contains(r#""symbol":"MRSH""#), "the masthead is rendered from it: {html}");
  assert!(html.contains(r#""watched":"MRSH""#), "and so is the table: {html}");

  let refused = host
    .handle(
      http::Request::post("/_sf/action/desk.watch")
        .header(http::header::CONTENT_TYPE, "application/json")
        .body(Bytes::from(r#"{"symbol":"NOPE"}"#))
        .unwrap(),
    )
    .await;
  assert_eq!(refused.status(), 404);
}

#[tokio::test]
async fn both_islands_are_rendered_with_the_symbol_the_session_is_watching() {
  let host = host();
  let html = body(host.handle(Request::get("/board").body(Bytes::new()).unwrap()).await).await;
  assert!(html.contains("ARBR"), "the seeded symbol reaches the markup: {html}");
  assert!(html.contains("Arbor Works"), "and the table the Vue island will mount over");
}

#[tokio::test]
async fn the_router_swaps_a_vue_segment_for_a_react_one_under_the_same_layout() {
  let host = host();
  let board = body(host.handle(Request::get("/board?__payload").body(Bytes::new()).unwrap()).await).await;
  let news = body(host.handle(Request::get("/news?__payload").body(Bytes::new()).unwrap()).await).await;

  assert!(board.contains("Holdings.vue#default"), "the board payload names the Vue module: {board}");
  assert!(!board.contains("Feed.tsx#default"), "and not the React page one");
  assert!(news.contains("Feed.tsx#default"), "the news payload names the React module: {news}");
  assert!(!news.contains("Holdings.vue#default"), "and not the Vue one");
  assert!(news.contains("layout.tera"), "both carry the layout segment, which the navigator keeps");
}

#[tokio::test]
async fn the_tape_is_a_fragment_of_its_own_with_no_shell_around_it() {
  let host = host();
  let fragment = body(host.handle(Request::get("/board?__fragment=tape").body(Bytes::new()).unwrap()).await).await;

  assert!(fragment.contains("panel tape"), "the slot alone: {fragment}");
  assert!(!fragment.contains("<!doctype"), "no shell");
  assert!(!fragment.contains("masthead"), "no layout");
  assert!(!fragment.contains("<sf-i"), "and nothing to mount");
}

#[tokio::test]
async fn the_tape_moves_between_polls_so_a_swap_is_visible() {
  let host = host();
  let first = body(host.handle(Request::get("/board?__fragment=tape").body(Bytes::new()).unwrap()).await).await;
  let second = body(host.handle(Request::get("/board?__fragment=tape").body(Bytes::new()).unwrap()).await).await;
  assert_ne!(first, second, "the rotation is what makes a poll worth watching");
}
