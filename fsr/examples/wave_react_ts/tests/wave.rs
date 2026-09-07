//! Waves on one controller: the rules, the view each window is built, and the
//! service the loaders and actions call.

use std::path::Path;
use std::sync::Arc;

use bytes::Bytes;
use http::{Request, StatusCode};
use plaza::state_logic::LogicOutput;
use plaza::{Agent, InProcessSession, LogicInput, SnapshotProvider, StateControllerBuilder, StateLogic};
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_host::{Config, Host, RenderMode};
use snapfire_fsr_runtime::SessionCell;
use snapfire_fsr_service::Transport;
use wave_react_ts::backend;
use wave_react_ts::field::{Conn, Field, Op, Rules, View, Views};

fn rules() -> Rules {
  Rules { clock: Box::new(|| "10:00".to_owned()) }
}

async fn apply(rules: &Rules, field: &mut Field, conn: Conn, ops: Vec<Op>) -> LogicOutput<Op, Conn> {
  rules.process_input(field, LogicInput::AgentOps { source: Agent::Human(conn), ops }).await.unwrap()
}

fn watch(wave: &str, name: &str) -> Op {
  Op::Watch { wave: wave.to_owned(), name: name.to_owned() }
}

fn typing(parent: &str, body: &str) -> Op {
  Op::Typing { parent: parent.to_owned(), body: body.to_owned() }
}

async fn view(field: &Field, conn: Conn) -> View {
  match Views.create_snapshot(field, Some(&Agent::Human(conn)), None).await.unwrap() {
    Some(Op::View(view)) => *view,
    other => panic!("no view for {conn}: {other:?}"),
  }
}

#[tokio::test]
async fn watching_a_wave_puts_you_on_it_and_leaving_takes_you_off() {
  let (rules, mut field) = (rules(), Field::new(backend::seed()));

  let out = apply(&rules, &mut field, 1, vec![watch("kickoff", "alice")]).await;
  assert_eq!(out.snapshots.len(), 1, "everyone on the wave is sent a view");
  assert_eq!(view(&field, 1).await.here, ["alice"], "the first arrival sees only herself");

  apply(&rules, &mut field, 2, vec![watch("kickoff", "bob")]).await;
  assert_eq!(view(&field, 1).await.here, ["alice", "bob"]);

  apply(&rules, &mut field, 3, vec![watch("kickoff", "alice")]).await;
  assert_eq!(view(&field, 1).await.here, ["alice", "bob"], "a second window of one person is one name");

  rules.process_input(&mut field, LogicInput::AgentLeft { agent_id: 2 }).await.unwrap();
  assert_eq!(view(&field, 1).await.here, ["alice"], "and a leave takes the name off");
}

#[tokio::test]
async fn a_draft_is_built_for_everyone_but_its_author() {
  let (rules, mut field) = (rules(), Field::new(backend::seed()));
  apply(&rules, &mut field, 1, vec![watch("kickoff", "alice")]).await;
  apply(&rules, &mut field, 2, vec![watch("kickoff", "bob")]).await;

  apply(&rules, &mut field, 1, vec![typing("2", "half a th")]).await;
  assert!(view(&field, 1).await.drafts.is_empty(), "a reader is never shown a ghost of their own");
  let seen = view(&field, 2).await.drafts;
  assert_eq!(seen.len(), 1, "and everyone else is: {seen:?}");
  assert_eq!(seen[0].who, "alice");
  assert_eq!(seen[0].parent, "2", "under the blip it answers");

  apply(&rules, &mut field, 1, vec![typing("2", "")]).await;
  assert!(view(&field, 2).await.drafts.is_empty(), "an empty draft is no draft");

  apply(&rules, &mut field, 2, vec![typing("", "bob's turn")]).await;
  rules.process_input(&mut field, LogicInput::AgentLeft { agent_id: 2 }).await.unwrap();
  assert!(view(&field, 1).await.drafts.is_empty(), "leaving takes the draft with it");
}

#[tokio::test]
async fn a_blip_is_held_by_one_window_and_the_others_watch_it_change() {
  let (rules, mut field) = (rules(), Field::new(backend::seed()));
  apply(&rules, &mut field, 1, vec![watch("kickoff", "alice")]).await;
  apply(&rules, &mut field, 2, vec![watch("kickoff", "bob")]).await;

  apply(&rules, &mut field, 1, vec![Op::Open { blip: "2".to_owned() }]).await;
  let held = view(&field, 2).await.edits;
  assert_eq!(held.len(), 1, "bob is shown the blip alice took");
  assert_eq!(held[0].who, "alice");
  assert_eq!(held[0].body, "Good. I will take the runtime half.", "and it starts from what the blip says");
  assert!(view(&field, 1).await.edits.is_empty(), "alice is never shown her own rewrite, the way she is never shown her own draft");

  apply(&rules, &mut field, 2, vec![Op::Open { blip: "2".to_owned() }]).await;
  apply(&rules, &mut field, 2, vec![Op::Rewriting { blip: "2".to_owned(), body: "bob got in".to_owned() }]).await;
  assert_eq!(view(&field, 2).await.edits[0].who, "alice", "the second window to reach for it changes nothing");

  apply(&rules, &mut field, 1, vec![Op::Rewriting { blip: "2".to_owned(), body: "I will take the runtime half, and the host.".to_owned() }]).await;
  assert_eq!(view(&field, 2).await.edits[0].body, "I will take the runtime half, and the host.", "bob watches the words as they land");

  apply(&rules, &mut field, 1, vec![Op::Close { blip: "2".to_owned() }]).await;
  assert!(view(&field, 2).await.edits.is_empty(), "letting it go frees it");
  assert_eq!(field.waves["kickoff"].blips[1].body, "Good. I will take the runtime half.", "and keeps nothing");
}

#[tokio::test]
async fn an_amend_keeps_the_rewrite_and_a_departure_lets_the_blip_go() {
  let (rules, mut field) = (rules(), Field::new(backend::seed()));
  apply(&rules, &mut field, 1, vec![watch("kickoff", "alice")]).await;
  apply(&rules, &mut field, 2, vec![watch("kickoff", "bob")]).await;

  apply(&rules, &mut field, 1, vec![Op::Open { blip: "2".to_owned() }]).await;
  apply(&rules, &mut field, 1, vec![Op::Amend { wave: "kickoff".to_owned(), blip: "2".to_owned(), who: "alice".to_owned(), body: "the runtime and the host".to_owned() }])
    .await;
  assert_eq!(field.waves["kickoff"].blips[1].body, "the runtime and the host");
  assert_eq!(field.waves["kickoff"].blips[1].edited, "10:00", "an amended blip says when it was amended");
  assert_eq!(field.waves["kickoff"].blips[1].editors, ["alice"], "and who amended it, though bob wrote it");

  apply(&rules, &mut field, 1, vec![Op::Amend { wave: "kickoff".to_owned(), blip: "2".to_owned(), who: "alice".to_owned(), body: "the runtime, the host".to_owned() }])
    .await;
  apply(&rules, &mut field, 2, vec![Op::Amend { wave: "kickoff".to_owned(), blip: "2".to_owned(), who: "bob".to_owned(), body: "the runtime, the host, the lot".to_owned() }])
    .await;
  assert_eq!(field.waves["kickoff"].blips[1].editors, ["alice", "bob"], "each of them once, in the order they first came to it");
  assert!(view(&field, 2).await.edits.is_empty(), "keeping it releases the hold");

  apply(&rules, &mut field, 2, vec![Op::Open { blip: "1".to_owned() }]).await;
  assert_eq!(view(&field, 1).await.edits.len(), 1);
  rules.process_input(&mut field, LogicInput::AgentLeft { agent_id: 2 }).await.unwrap();
  assert!(view(&field, 1).await.edits.is_empty(), "a window that goes away holds nothing");
}

#[tokio::test]
async fn a_window_with_no_name_reads_and_writes_nothing() {
  let (rules, mut field) = (rules(), Field::new(backend::seed()));
  apply(&rules, &mut field, 1, vec![watch("kickoff", "")]).await;
  apply(&rules, &mut field, 2, vec![watch("kickoff", "alice")]).await;

  assert_eq!(view(&field, 2).await.here, ["alice"], "a nameless window is on the wave and not among the people on it");

  apply(&rules, &mut field, 1, vec![typing("", "anonymous")]).await;
  assert!(view(&field, 2).await.drafts.is_empty(), "and nobody sees it typing");

  apply(&rules, &mut field, 1, vec![Op::Open { blip: "2".to_owned() }]).await;
  assert!(view(&field, 2).await.edits.is_empty(), "nor holding a blip");
  apply(&rules, &mut field, 2, vec![Op::Open { blip: "2".to_owned() }]).await;
  assert_eq!(view(&field, 1).await.edits.len(), 1, "which leaves the blip free for someone who has named themselves");
}

#[tokio::test]
async fn an_unknown_wave_is_refused_and_an_unwatched_keystroke_is_dropped() {
  let (rules, mut field) = (rules(), Field::new(backend::seed()));
  let refused = rules
    .process_input(&mut field, LogicInput::AgentOps { source: Agent::Human(1), ops: vec![watch("nowhere", "alice")] })
    .await;
  assert!(refused.is_err(), "the rules refuse a wave that does not exist");

  let out = apply(&rules, &mut field, 9, vec![typing("", "into the void")]).await;
  assert!(out.snapshots.is_empty(), "a window watching nothing changes nothing");
}

#[tokio::test]
async fn the_service_reads_and_writes_through_the_controller() {
  let (field, controller) =
    StateControllerBuilder::new(Arc::new(rules()), InProcessSession::<Op, Conn>::new(), Arc::new(Views), Field::new(backend::seed())).build();
  tokio::spawn(controller.run());
  let (service, mut kept) = backend::service(field);

  let waves = call(&service, "listWaves", ValueMap::new()).await;
  assert!(format!("{waves:?}").contains("Snapfire kickoff"), "{waves:?}");

  let args = ValueMap::from_iter([
    ("id".to_owned(), Value::str("kickoff")),
    ("parent".to_owned(), Value::str("2")),
    ("who".to_owned(), Value::str("carol")),
    ("body".to_owned(), Value::str("carol, arriving late")),
  ]);
  let blip = call(&service, "addBlip", args).await;
  assert!(format!("{blip:?}").contains("10:00"), "the rules stamped it with their clock: {blip:?}");
  assert_eq!(kept.try_recv().unwrap(), "wave/kickoff", "and the wave's topic went out for the pages to revalidate");

  let wave = call(&service, "getWave", ValueMap::from_iter([("id".to_owned(), Value::str("kickoff"))])).await;
  let shown = format!("{wave:?}");
  assert!(shown.contains("carol, arriving late"), "the read after the write sees it: {shown}");
  assert!(shown.contains("\"carol\""), "carol is a participant now: {shown}");

  let args = ValueMap::from_iter([
    ("id".to_owned(), Value::str("kickoff")),
    ("blip".to_owned(), Value::str("1")),
    ("who".to_owned(), Value::str("carol")),
    ("body".to_owned(), Value::str("Starting a wave. Anyone may rewrite this.")),
  ]);
  let amended = call(&service, "editBlip", args).await;
  let shown = format!("{amended:?}");
  assert!(shown.contains("Anyone may rewrite this"), "the amend went through the controller: {shown}");
  assert!(shown.contains("10:00"), "and it is stamped: {shown}");
  assert!(shown.contains("\"carol\""), "with whoever rewrote it: {shown}");
  assert_eq!(kept.try_recv().unwrap(), "wave/kickoff", "the topic goes out again so every page revalidates");
}

#[tokio::test]
async fn a_view_names_which_waves_the_inbox_lists() {
  let (field, controller) =
    StateControllerBuilder::new(Arc::new(rules()), InProcessSession::<Op, Conn>::new(), Arc::new(Views), Field::new(backend::seed())).build();
  tokio::spawn(controller.run());
  let (service, _kept) = backend::service(field.clone());
  let listed = |view: &str, who: &str| {
    let args = ValueMap::from_iter([("view".to_owned(), Value::str(view)), ("who".to_owned(), Value::str(who))]);
    let service = service.clone();
    async move { titles(&call(&service, "listWaves", args).await) }
  };

  assert_eq!(listed("inbox", "alice").await.len(), 2, "the inbox is every wave");
  assert!(listed("active", "alice").await.is_empty(), "nobody is connected yet");
  assert_eq!(listed("mine", "bob").await, ["Snapfire kickoff"], "bob has written in one of them");
  assert!(listed("mine", "").await.is_empty(), "and a reader with no name has written in none");

  field
    .send(plaza::ControllerCommand::SubmitAgentOps { agent: Agent::Human(1), ops: vec![watch("board", "alice")] })
    .await
    .unwrap();
  assert_eq!(listed("active", "alice").await, ["Arrivals board review"], "a watched wave is the active one");

  let people = call(&service, "listPeople", ValueMap::new()).await;
  let shown = format!("{people:?}");
  assert!(shown.contains("\"alice\""), "everyone on any wave is a contact: {shown}");
  assert!(shown.contains("Bool(true)"), "and alice is here, on the wave she is watching: {shown}");
}

/// The titles a listing answered with, in order.
fn titles(listed: &Value) -> Vec<String> {
  match listed {
    Value::Seq(waves) => waves
      .iter()
      .filter_map(|wave| match wave {
        Value::Map(map) => match map.get("title") {
          Some(Value::Str(title)) => Some(title.clone()),
          _ => None,
        },
        _ => None,
      })
      .collect(),
    other => panic!("a listing is a sequence: {other:?}"),
  }
}

async fn call(service: &Arc<dyn Transport>, method: &str, args: ValueMap) -> Value {
  let call = snapfire_fsr_service::Call {
    service: "waves".to_owned(),
    method: method.to_owned(),
    args,
    identity: None,
    metadata: ValueMap::new(),
    credentials: Arc::new(snapfire_fsr_service::NoCredentials),
  };
  service.call(call).await.unwrap()
}

#[tokio::test]
async fn a_wave_is_followed_only_by_a_session_that_opened_it() {
  let config = Config::load(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
  let (field, controller) =
    StateControllerBuilder::new(Arc::new(rules()), InProcessSession::<Op, Conn>::new(), Arc::new(Views), Field::new(backend::seed())).build();
  tokio::spawn(controller.run());
  let (service, _kept) = backend::service(field);
  let host = Host::from_config(config)
    .unwrap()
    .services_over(service)
    .topics(|topic, session, _| match topic.strip_prefix("wave/") {
      Some(wave) => matches!(session.get("waves"), Some(Value::Map(open)) if open.contains_key(wave)),
      None => false,
    })
    .socket(|_, _| snapfire_fsr_host::socket::Reply::default())
    .build()
    .unwrap();

  for path in ["/_sf/live?topics=wave/kickoff", "/_sf/socket?topic=wave/kickoff"] {
    let response = host.handle(Request::get(path).body(Bytes::new()).unwrap()).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN, "{path} is refused before the wave is opened");
  }

  let session = SessionCell::default();
  session.insert("name", Value::str("alice"));
  let html = host.render_to_string("/wave/kickoff", RenderMode::Html, session.clone()).await.unwrap();
  assert!(html.contains("margin-left:1.5rem"), "the transcript is rendered on the server, nested: {html}");
  match session.get("waves") {
    Some(Value::Map(waves)) => assert!(waves.contains_key("kickoff"), "the loader recorded the wave: {waves:?}"),
    other => panic!("the session holds no waves: {other:?}"),
  }
}
