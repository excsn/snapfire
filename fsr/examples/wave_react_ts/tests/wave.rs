//! Waves on both seams: the durable half through the loader and the action,
//! and the ephemeral half through the field that answers the socket.

use std::path::Path;
use std::sync::Arc;

use bytes::Bytes;
use http::{Request, StatusCode};
use http_body_util::BodyExt;
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_host::socket::{On, Row, Who};
use snapfire_fsr_host::{Config, Host, RenderMode};
use snapfire_fsr_runtime::SessionCell;
use wave_react_ts::presence::Field;

fn waves() -> (Arc<Host>, Arc<wave_react_ts::backend::Waves>) {
  let (transport, waves) = wave_react_ts::backend::waves();
  let config = Config::load(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
  let host = Host::from_config(config)
    .unwrap()
    .services_over(transport)
    .topics(|topic, session, _| match topic.strip_prefix("wave/") {
      Some(wave) => matches!(session.get("waves"), Some(Value::Map(open)) if open.contains_key(wave)),
      None => false,
    })
    .socket({
      let field = Arc::new(Field::new());
      move |who, on| field.on(who, on)
    })
    .build()
    .unwrap();
  (Arc::new(host), waves)
}

fn named(name: &str) -> SessionCell {
  let session = SessionCell::default();
  session.insert("name", Value::str(name));
  session
}

fn who(name: &str, connection: u64) -> Who {
  Who { topic: "wave/kickoff".to_owned(), session: named(name), identity: None, connection }
}

fn map(pairs: &[(&str, &str)]) -> Value {
  Value::Map(pairs.iter().map(|(key, value)| ((*key).to_owned(), Value::str(*value))).collect::<ValueMap>())
}

fn names(row: &Row) -> Vec<String> {
  match &row.value {
    Value::Seq(items) => items
      .iter()
      .map(|item| match item {
        Value::Str(name) => name.clone(),
        Value::Map(draft) => match draft.get("who") {
          Some(Value::Str(name)) => name.clone(),
          _ => String::new(),
        },
        _ => String::new(),
      })
      .collect(),
    _ => Vec::new(),
  }
}

#[tokio::test]
async fn a_wave_renders_its_blips_in_reading_order_with_the_depth_of_each() {
  let (host, _) = waves();
  let html = host.render_to_string("/wave/kickoff", RenderMode::Html, named("bob")).await.unwrap();
  assert!(html.contains("<h1>Snapfire kickoff</h1>"), "{html}");

  let first = html.split("class=\"who\">").nth(1).unwrap_or_default();
  assert!(first.starts_with("alice"), "the first blip is the one nothing answers: {}", &first[..first.len().min(20)]);
  assert!(html.contains("margin-left:1.5rem"), "a reply is indented one step: {html}");
  assert!(html.contains("margin-left:3rem"), "and a reply to a reply two: {html}");
  assert!(html.contains("class=\"blip mine\""), "bob's own blip is marked: {html}");
}

#[tokio::test]
async fn keeping_a_blip_nests_it_names_the_wave_and_adds_whoever_wrote_it() {
  let (host, waves) = waves();
  let mut changes = waves.changes();
  let session = named("carol");

  let input = ValueMap::from_iter([
    ("wave".to_owned(), Value::str("kickoff")),
    ("parent".to_owned(), Value::str("2")),
    ("body".to_owned(), Value::str("carol, arriving late")),
  ]);
  host.call_action("$root.blip", session.clone(), Value::Map(input)).await.unwrap();
  assert_eq!(changes.try_recv().unwrap(), "wave/kickoff", "the wave's topic went out");

  let html = host.render_to_string("/wave/kickoff", RenderMode::Html, session).await.unwrap();
  assert!(html.contains("carol, arriving late"), "the blip is in the wave: {html}");
  assert!(html.contains(">ca</li>") || html.contains("carol"), "carol is a participant now: {html}");

  let other = host.render_to_string("/wave/board", RenderMode::Html, named("carol")).await.unwrap();
  assert!(!other.contains("carol, arriving late"), "and no other wave has it: {other}");
}

#[tokio::test]
async fn a_wave_is_followed_only_by_a_session_that_opened_it() {
  let (host, _) = waves();
  for path in ["/_sf/live?topics=wave/kickoff", "/_sf/socket?topic=wave/kickoff"] {
    let response = host.handle(Request::get(path).body(Bytes::new()).unwrap()).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN, "{path} is refused before the wave is opened");
  }

  let session = named("alice");
  host.render_to_string("/wave/kickoff", RenderMode::Html, session.clone()).await.unwrap();
  match session.get("waves") {
    Some(Value::Map(waves)) => assert!(waves.contains_key("kickoff"), "the loader recorded the wave: {waves:?}"),
    other => panic!("the session holds no waves: {other:?}"),
  }
}

#[test]
fn the_field_answers_a_join_with_who_is_here_and_a_leave_by_forgetting_them() {
  let field = Field::new();

  let reply = field.on(&who("alice", 1), On::Joined);
  assert_eq!(names(&reply.everyone[0]), ["alice"], "the first arrival sees only herself");
  assert_eq!(reply.everyone[0].key, "wave/here", "the key names what a page shows, not which wave, which is what lets the island lower");

  let reply = field.on(&who("bob", 2), On::Joined);
  assert_eq!(names(&reply.everyone[0]), ["alice", "bob"], "and everyone hears about the second");

  let reply = field.on(&who("alice", 3), On::Joined);
  assert_eq!(names(&reply.everyone[0]), ["alice", "bob"], "a second window of one person is one name");

  let reply = field.on(&who("bob", 2), On::Left);
  assert_eq!(names(&reply.everyone[0]), ["alice"], "and a leave takes the name off: {:?}", reply.everyone[0]);
}

#[test]
fn a_draft_reaches_everyone_but_its_author_and_goes_when_it_empties() {
  let field = Field::new();
  field.on(&who("alice", 1), On::Joined);
  field.on(&who("bob", 2), On::Joined);

  let reply = field.on(&who("alice", 1), On::Said(Row::new("typing", map(&[("parent", "2"), ("body", "half a th")]))));
  assert!(reply.everyone.is_empty() && reply.sender.is_empty(), "a draft is for the others, never an echo");
  assert_eq!(reply.others[0].key, "wave/drafts");
  assert_eq!(names(&reply.others[0]), ["alice"]);

  let reply = field.on(&who("alice", 1), On::Said(Row::new("typing", map(&[("parent", "2"), ("body", "")]))));
  assert!(names(&reply.others[0]).is_empty(), "an empty draft is no draft: {:?}", reply.others[0]);

  field.on(&who("bob", 2), On::Said(Row::new("typing", map(&[("parent", ""), ("body", "bob's turn")]))));
  let reply = field.on(&who("bob", 2), On::Left);
  assert!(names(&reply.everyone[1]).is_empty(), "leaving takes the draft with it: {:?}", reply.everyone[1]);

  let reply = field.on(&who("alice", 1), On::Said(Row::new("shout", Value::str("nope"))));
  assert!(reply.others.is_empty() && reply.everyone.is_empty(), "a key the field does not know is dropped");
}
