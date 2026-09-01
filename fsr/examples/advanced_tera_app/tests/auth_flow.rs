use std::time::Duration;

use advanced_tera_app::{build_app, respond_with, RenderMode};
use futures::executor::block_on;
use futures_util::StreamExt;
use snapfire_fsr_auth::AuthError;
use snapfire_fsr_core::{Value, ValueMap};

fn render_at(app: &advanced_tera_app::AppCore, opened: &snapfire_fsr_session::Opened, path: &str) -> String {
  let csrf = app.sessions().csrf_token(&opened.id);
  block_on(async {
    let chunks: Vec<String> = respond_with(app, path, RenderMode::Html, incoming(opened, csrf))
      .await
      .unwrap()
      .collect()
      .await;
    chunks.concat()
  })
}

fn creds(user: &str, password: &str) -> ValueMap {
  let mut params = ValueMap::new();
  params.insert("user".to_owned(), Value::Str(user.to_owned()));
  params.insert("password".to_owned(), Value::Str(password.to_owned()));
  params
}

fn incoming(opened: &snapfire_fsr_session::Opened, csrf: String) -> advanced_tera_app::Incoming {
  advanced_tera_app::Incoming::new(
    opened.cell.clone(),
    Some(csrf),
    std::sync::Arc::new(opened.tokens.clone()),
  )
}

#[test]
fn the_login_journey_end_to_end() {
  let app = build_app(Duration::ZERO);
  let opened = block_on(app.sessions().open(None));

  let html = render_at(&app, &opened, "/dash/servers");
  assert!(html.contains("login-link"), "anonymous nav offers login: {html}");

  let redirect = block_on(app.auth().login(&opened, "/dash/servers"));
  assert_eq!(redirect, "/login?return_to=%2Fdash%2Fservers");

  let login_page = render_at(&app, &opened, "/login");
  assert!(login_page.contains("action=\"/auth/callback\""), "the login page is an ordinary route: {login_page}");
  assert!(login_page.contains("alice"), "the login page names the dev accounts: {login_page}");

  let destination = block_on(app.auth().callback(&opened, creds("alice", "wonder"))).unwrap();
  assert_eq!(destination, "/dash/servers");

  let html = render_at(&app, &opened, "/dash/servers");
  assert!(html.contains("signed in as alice"), "{html}");
  assert!(html.contains("/auth/logout"));
  assert!(!html.contains("dev-token-alice"), "tokens never render");

  let cookie = block_on(app.sessions().persist(&opened)).expect("identified session persists");
  let header = cookie.split(';').next().unwrap().to_owned();
  let back = block_on(app.sessions().open(Some(&header)));
  assert_eq!(back.cell.identity().unwrap().subject, "alice");
  assert_eq!(back.tokens.get("access_token"), Some(Value::Str("dev-token-alice".into())));

  app.auth().logout(&back);
  let expire = block_on(app.sessions().destroy(&back));
  assert!(expire.contains("Max-Age=0"));
  let after = block_on(app.sessions().open(Some(&header)));
  assert!(after.cell.identity().is_none());
  let html = render_at(&app, &after, "/dash/servers");
  assert!(html.contains("login-link"), "anonymous again: {html}");
}

#[test]
fn wrong_password_leaves_the_session_anonymous() {
  let app = build_app(Duration::ZERO);
  let opened = block_on(app.sessions().open(None));
  block_on(app.auth().login(&opened, "/"));

  let err = block_on(app.auth().callback(&opened, creds("alice", "nope"))).unwrap_err();
  assert!(matches!(err, AuthError::Denied(_)));
  let html = render_at(&app, &opened, "/dash/servers");
  assert!(html.contains("login-link"));
  assert!(opened.cell.identity().is_none());
}
