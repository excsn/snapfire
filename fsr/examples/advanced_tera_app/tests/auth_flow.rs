mod common;

use common::{app, csrf_in, get, location, post_form, session_cookie, text};

#[test]
fn the_login_journey_end_to_end() {
  let host = app();

  let response = get(&host, "/dash/servers", None);
  let cookie = session_cookie(&response);
  let html = text(response);
  assert!(html.contains("login-link"), "anonymous nav offers login: {html}");

  let response = get(&host, "/auth/login?return_to=/dash/servers", Some(&cookie));
  assert_eq!(response.status(), 303);
  assert_eq!(location(&response), "/login?return_to=%2Fdash%2Fservers");

  let login_page = text(get(&host, "/login?return_to=%2Fdash%2Fservers", Some(&cookie)));
  assert!(login_page.contains("action=\"/auth/callback\""), "the login page is an ordinary route: {login_page}");
  assert!(login_page.contains("alice"), "the login page names the dev accounts: {login_page}");

  let response = post_form(&host, "/auth/callback", Some(&cookie), "user=alice&password=wonder", None);
  assert_eq!(response.status(), 303);
  assert_eq!(location(&response), "/dash/servers");

  let html = text(get(&host, "/dash/servers", Some(&cookie)));
  assert!(html.contains("signed in as alice"), "{html}");
  assert!(html.contains("/auth/logout"));
  assert!(!html.contains("dev-token-alice"), "tokens never render");
  let token = csrf_in(&html);

  let response = post_form(&host, "/auth/logout", Some(&cookie), "_csrf=wrong", None);
  assert_eq!(response.status(), 403);

  let response = post_form(&host, "/auth/logout", Some(&cookie), &format!("_csrf={token}"), None);
  assert_eq!(response.status(), 303);
  let expire = response.headers().get(http::header::SET_COOKIE).unwrap().to_str().unwrap();
  assert!(expire.contains("Max-Age=0"), "{expire}");

  let html = text(get(&host, "/dash/servers", Some(&cookie)));
  assert!(html.contains("login-link"), "anonymous again: {html}");
}

#[test]
fn wrong_password_leaves_the_session_anonymous() {
  let host = app();
  let response = get(&host, "/auth/login?return_to=/", None);
  let cookie = session_cookie(&response);

  let response = post_form(&host, "/auth/callback", Some(&cookie), "user=alice&password=nope", None);
  assert_eq!(response.status(), 303);
  assert_eq!(location(&response), "/login?error=denied&return_to=%2F");

  let html = text(get(&host, "/dash/servers", Some(&cookie)));
  assert!(html.contains("login-link"));
  assert!(!html.contains("signed in as"));
}
