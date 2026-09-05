mod common;

use common::{app, csrf_in, get, render, session_cookie, text};

#[test]
fn visits_count_across_the_cookie_round_trip() {
  let host = app();

  let response = get(&host, "/dash/servers", None);
  let cookie = session_cookie(&response);
  let html = text(response);
  assert!(html.contains("visits 1"), "first visit: {html}");

  let html = text(get(&host, "/dash/servers", Some(&cookie)));
  assert!(html.contains("visits 2"), "the session remembered: {html}");

  let html = text(get(&host, "/dash/servers?__payload", Some(&cookie)));
  assert!(html.contains("visits 2"), "a navigation is the same page, not a visit: {html}");
}

#[test]
fn the_form_embeds_the_session_csrf_token() {
  let host = app();
  let response = get(&host, "/dash/servers", None);
  let cookie = session_cookie(&response);
  let token = csrf_in(&text(response));
  assert!(!token.is_empty());
  assert_eq!(csrf_in(&text(get(&host, "/dash/servers", Some(&cookie)))), token, "the token is the session's");
}

#[test]
fn anonymous_render_still_works_without_a_session_layer() {
  let host = app();
  let html = render(&host, "/dash/servers");
  assert!(html.contains("visits 0"), "a render outside the edge runs no middleware, so nothing counts: {html}");
  assert!(html.contains("name=\"_csrf\" value=\"\""), "no token without a session layer, form still renders");
}
