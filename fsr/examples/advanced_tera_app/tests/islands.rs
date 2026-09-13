mod common;

use bytes::Bytes;
use common::{app, csrf_in, get, post_form, session_cookie, text};
use futures::executor::block_on;
use http::{header, Request};
use snapfire_fsr_host::{Body, Host};

fn step(host: &Host, body: &str) -> http::Response<Body> {
  let request = Request::post("/_sf/island/fleet.tera%23default")
    .header(header::CONTENT_TYPE, "application/json")
    .body(Bytes::from(body.to_owned()))
    .unwrap();
  block_on(host.handle(request))
}

#[test]
fn the_card_is_rendered_by_the_server_before_anything_mounts() {
  let host = app();
  let html = text(get(&host, "/dash/servers", None));

  assert!(html.contains(r#"data-sf-mode="server""#), "the placement asks for server mode: {html}");
  assert!(html.contains(r#"data-sf-module="fleet.tera#default""#), "the island names a template rather than a component");
  assert!(html.contains(r#"data-sf-on="click:filter""#), "the markup binds the handler by name, which is what `on` writes");
  assert!(html.contains("<h2>Fleet</h2>"), "and the card is in the first paint, with no JavaScript involved: {html}");
  assert!(html.contains("web-1") && html.contains("web-2"), "rendered from the state the placement gave it");
  assert!(html.contains(r#""$s":{"#), "whose state rides in the props for the browser to carry back");
}

#[test]
fn a_step_runs_the_named_handler_and_answers_the_markup_it_produced() {
  let host = app();
  let answer = text(step(&host, r#"{"props":{},"state":{"filter":"all"},"handler":"filter","event":{"target":{"value":"busy"}}}"#));

  assert!(answer.contains("web-1"), "web-1 is at 0.73, which is busy: {answer}");
  assert!(!answer.contains("web-2"), "web-2 is at 0.41, which is not");
  assert!(answer.contains(r#"class=\"on\""#), "the chosen filter is marked in the markup the server rendered");
  assert!(answer.contains(r#""filter":"busy""#), "and the state comes back for the browser to hold");
  assert!(answer.contains(r#""revalidate":false"#), "the handler wrote no session, so the page around it is untouched");
}

#[test]
fn the_handler_reads_the_fleet_as_it_is_now_rather_than_as_the_browser_saw_it() {
  let host = app();
  let page = get(&host, "/dash/servers", None);
  let cookie = session_cookie(&page);
  let token = csrf_in(&text(page));
  let added = post_form(
    &host,
    "/_sf/action/add_server",
    Some(&cookie),
    &format!("name=web-9&load=0.9&_csrf={token}"),
    Some("http://localhost/dash/servers"),
  );
  assert_eq!(added.status(), 303, "the add form posts as it always did");

  let answer = text(step(&host, r#"{"props":{},"state":{"filter":"all"},"handler":"filter","event":{"target":{"value":"busy"}}}"#));
  assert!(answer.contains("web-9"), "a step is a render on the server, so it sees what a page render would: {answer}");
}

#[test]
fn a_handler_the_module_does_not_have_is_not_found_and_a_bad_filter_is_refused() {
  let host = app();
  let missing = step(&host, r#"{"props":{},"state":{},"handler":"drain","event":null}"#);
  assert_eq!(missing.status(), 404);
  assert!(text(missing).contains("has no handler `drain`"));

  let refused = step(&host, r#"{"props":{},"state":{},"handler":"filter","event":{"target":{"value":"idle"}}}"#);
  assert_eq!(refused.status(), 400, "the event is input and the handler checks it");
  assert!(text(refused).contains("is not one of the fleet's filters"));

  let indexed = step(&host, r#"{"props":{},"state":{},"handler":0,"event":null}"#);
  assert_eq!(indexed.status(), 400, "a template island's handler is a name, never an index");
}

#[test]
fn a_step_with_no_handler_renders_the_module_from_the_state_it_was_given() {
  let host = app();
  let answer = text(step(&host, r#"{"props":{},"state":{"filter":"quiet","servers":[{"name":"web-7","load":{"$":"f","v":0.1}}]},"handler":null}"#));

  assert!(answer.contains("web-7"), "the state is what it renders from, whoever produced it: {answer}");
  assert!(!answer.contains("web-1"), "and nothing else leaks in");
}
