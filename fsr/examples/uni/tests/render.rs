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

  let markers = html.matches("<sf-i").count();
  assert_eq!(markers, 2, "two islands on the page, no more: {html}");
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
