use std::time::Duration;

use futures::executor::block_on;
use futures_util::StreamExt;
use tera_app::{build_app, respond_with, RenderMode};

fn render_with(app: &tera_app::AppCore, opened: &snapfire_fsr_session::Opened) -> String {
  let csrf = app.sessions().csrf_token(&opened.id);
  block_on(async {
    let chunks: Vec<String> = respond_with(app, "/dash/servers", RenderMode::Html, opened.cell.clone(), Some(csrf))
      .await
      .unwrap()
      .collect()
      .await;
    chunks.concat()
  })
}

#[test]
fn visits_count_across_the_cookie_round_trip() {
  let app = build_app(Duration::ZERO);

  let first = block_on(app.sessions().open(None));
  let html = render_with(&app, &first);
  assert!(html.contains("visits 1"), "first visit: {html}");
  let cookie = block_on(app.sessions().persist(&first)).expect("dirty fresh session sets a cookie");

  let header = cookie.split(';').next().unwrap().to_owned();
  let second = block_on(app.sessions().open(Some(&header)));
  let html = render_with(&app, &second);
  assert!(html.contains("visits 2"), "the session remembered: {html}");
}

#[test]
fn the_form_embeds_the_session_csrf_token() {
  let app = build_app(Duration::ZERO);
  let opened = block_on(app.sessions().open(None));
  let token = app.sessions().csrf_token(&opened.id);
  let html = render_with(&app, &opened);
  assert!(html.contains(&format!("name=\"_csrf\" value=\"{token}\"")), "hidden csrf input: {html}");
  assert!(app.sessions().verify_csrf(&opened.id, &token));
}

#[test]
fn anonymous_render_still_works_without_a_session_layer() {
  let app = build_app(Duration::ZERO);
  let html = block_on(tera_app::render(&app, "/dash/servers", RenderMode::Html)).unwrap();
  assert!(html.contains("visits 1"), "an anonymous cell still counts from one");
  assert!(html.contains("name=\"_csrf\" value=\"\""), "no token without a session layer, form still renders");
}
