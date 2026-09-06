//! Rooms over the seam that pushes: what a message does to the transcript,
//! and who is allowed to follow a room.

use std::path::Path;
use std::sync::Arc;

use bytes::Bytes;
use http::{Request, StatusCode};
use http_body_util::BodyExt;
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_host::{Config, Host, RenderMode};
use snapfire_fsr_runtime::SessionCell;

fn chat() -> (Arc<Host>, Arc<chat_react_ts::backend::Rooms>) {
  let (transport, rooms) = chat_react_ts::backend::rooms();
  let config = Config::load(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
  let host = Host::from_config(config)
    .unwrap()
    .services_over(transport)
    .topics(|topic, session, _| match topic.strip_prefix("room/") {
      Some(room) => matches!(session.get("rooms"), Some(Value::Map(open)) if open.contains_key(room)),
      None => false,
    })
    .build()
    .unwrap();
  (Arc::new(host), rooms)
}

fn named(name: &str) -> SessionCell {
  let session = SessionCell::default();
  session.insert("name", Value::str(name));
  session
}

fn said(input: &[(&str, Value)]) -> ValueMap {
  input.iter().map(|(key, value)| ((*key).to_owned(), value.clone())).collect()
}

#[tokio::test]
async fn a_room_renders_its_transcript_and_marks_what_the_reader_said() {
  let (host, _) = chat();
  let html = host.render_to_string("/room/lobby", RenderMode::Html, named("bob")).await.unwrap();
  assert!(html.contains("<h1>Lobby</h1>"), "{html}");
  assert!(html.contains("Morning. Coffee is on."), "the transcript is rendered on the server: {html}");
  assert!(html.contains("class=\"said mine\""), "bob's own message is marked: {html}");
  assert!(html.contains("you are bob"), "the layout reads the session: {html}");
}

#[tokio::test]
async fn opening_a_room_is_joining_it_and_the_topic_rule_reads_that() {
  let (host, _) = chat();

  let response = host.handle(Request::get("/_sf/live?topics=room/lobby").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.status(), StatusCode::FORBIDDEN, "a session that has opened no room follows none");

  let session = named("alice");
  host.render_to_string("/room/lobby", RenderMode::Html, session.clone()).await.unwrap();
  match session.get("rooms") {
    Some(Value::Map(rooms)) => assert!(rooms.contains_key("lobby"), "the loader recorded the room: {rooms:?}"),
    other => panic!("the session holds no rooms: {other:?}"),
  }
}

#[tokio::test]
async fn saying_something_keeps_it_and_names_the_room_as_a_topic() {
  let (host, rooms) = chat();
  let mut changes = rooms.changes();
  let session = named("alice");

  let out = host
    .call_action("room.$id.say", session.clone(), Value::Map(said(&[("room", Value::str("lobby")), ("body", Value::str("is anyone about"))])))
    .await
    .unwrap();
  assert!(format!("{out:?}").contains("is anyone about"), "the action answers with the message it kept: {out:?}");
  assert_eq!(changes.try_recv().unwrap(), "room/lobby", "the room's topic went out");

  let html = host.render_to_string("/room/lobby", RenderMode::Html, session).await.unwrap();
  assert!(html.contains("is anyone about"), "the next render of the room has it: {html}");

  let other = host.render_to_string("/room/deploys", RenderMode::Html, named("bob")).await.unwrap();
  assert!(!other.contains("is anyone about"), "and no other room does: {other}");
}

#[tokio::test]
async fn the_room_list_counts_what_is_in_each_room() {
  let (host, _) = chat();
  let html = host.render_to_string("/", RenderMode::Html, SessionCell::default()).await.unwrap();
  assert!(html.contains("Whose yoghurt"), "every room is listed: {html}");
  assert!(html.contains("class=\"room-count\">2<"), "the lobby holds the two it was seeded with: {html}");
  assert!(html.contains("class=\"room-count\">0<"), "and the kitchen holds none: {html}");
}
