//! Waves on one controller: the rules, the view each window is built and the
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
  Rules { clock: Box::new(|| "10:00".to_owned()), ..Rules::new() }
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

  apply(&rules, &mut field, 3, vec![watch("kickoff", "carol")]).await;
  apply(&rules, &mut field, 3, vec![Op::Open { blip: "2".to_owned() }]).await;
  apply(&rules, &mut field, 3, vec![Op::Amend { wave: "kickoff".to_owned(), blip: "2".to_owned(), who: "carol".to_owned(), body: "the lot".to_owned() }]).await;
  assert_eq!(field.waves["kickoff"].participants, ["alice", "bob", "carol"], "a rewrite puts its author on the wave, as keeping a blip does");

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

  let waves = call(&service, "listWaves", ValueMap::default()).await;
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
async fn the_gadget_is_one_board_the_wave_shares_and_every_rule_is_the_field_s() {
  let (field, controller) =
    StateControllerBuilder::new(Arc::new(rules()), InProcessSession::<Op, Conn>::new(), Arc::new(Views), Field::new(backend::seed())).build();
  tokio::spawn(controller.run());
  let (service, mut told) = backend::service(field);

  let play = |who: &str, cell: i128| {
    ValueMap::from_iter([
      ("id".to_owned(), Value::str("kickoff")),
      ("who".to_owned(), Value::str(who)),
      ("cell".to_owned(), Value::Int(cell)),
    ])
  };
  let marks = |board: &Value| match board {
    Value::Map(map) => match map.get("cells") {
      Some(Value::Seq(cells)) => cells
        .iter()
        .map(|cell| match cell {
          Value::Map(cell) => match cell.get("mark") {
            Some(Value::Str(mark)) => mark.to_string(),
            _ => String::new(),
          },
          _ => String::new(),
        })
        .collect::<Vec<_>>()
        .join(""),
      _ => String::new(),
    },
    _ => String::new(),
  };
  let turn = |board: &Value| match board {
    Value::Map(map) => format!("{:?}", map.get("turn")),
    _ => String::new(),
  };

  let board = call(&service, "play", play("alice", 4)).await;
  assert_eq!(marks(&board), "x", "the first to move takes x");
  assert!(turn(&board).contains("o"));
  assert_eq!(told.try_recv().unwrap(), "wave/kickoff", "a move is a change to the wave, so the topic goes out");

  let board = call(&service, "play", play("alice", 0)).await;
  assert_eq!(marks(&board), "x", "alice holds x, so she cannot answer herself");

  let board = call(&service, "play", play("bob", 4)).await;
  assert_eq!(marks(&board), "x", "a cell that is taken stays taken");

  let board = call(&service, "play", play("bob", 0)).await;
  assert_eq!(marks(&board), "ox", "bob takes o and the corner");

  call(&service, "play", play("alice", 1)).await;
  call(&service, "play", play("bob", 3)).await;
  let board = call(&service, "play", play("alice", 7)).await;
  assert_eq!(marks(&board), "oxoxx", "the middle column is x's");
  assert!(format!("{board:?}").contains("\"won\""), "{board:?}");
  match &board {
    Value::Map(map) => assert_eq!(map.get("won"), Some(&Value::str("x")), "three in a column is the game"),
    other => panic!("{other:?}"),
  }

  let board = call(&service, "play", play("bob", 2)).await;
  assert_eq!(marks(&board), "oxoxx", "and nothing moves after it is won");

  let cleared = call(&service, "play", ValueMap::from_iter([("id".to_owned(), Value::str("kickoff")), ("who".to_owned(), Value::str("bob"))])).await;
  assert_eq!(marks(&cleared), "", "a move with no cell is a new board");
  assert!(turn(&cleared).contains("x"), "which starts at x again");
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

  let people = call(&service, "listPeople", ValueMap::default()).await;
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
          Some(Value::Str(title)) => Some(title.to_string()),
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
    metadata: ValueMap::default(),
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

#[tokio::test]
async fn a_blip_body_is_markdown_the_service_renders_with_raw_html_as_text() {
  let (field, controller) =
    StateControllerBuilder::new(Arc::new(rules()), InProcessSession::<Op, Conn>::new(), Arc::new(Views), Field::new(backend::seed())).build();
  tokio::spawn(controller.run());
  let (service, _kept) = backend::service(field);

  assert_eq!(kept_html(&service, "carol, **arriving** late").await, "<p>carol, <strong>arriving</strong> late</p>\n");
  let block = kept_html(&service, "<script>alert(1)</script>").await;
  assert!(block.contains("&lt;script&gt;") && !block.contains("<script"), "a raw HTML block is text: {block}");
  let inline = kept_html(&service, "a <b>bold</b> claim").await;
  assert!(inline.contains("&lt;b&gt;") && !inline.contains("<b>"), "raw inline HTML is text: {inline}");
  let link = kept_html(&service, "[run](javascript:alert(1)) or [read](https://example.com/a)").await;
  assert!(link.contains("href=\"#\"") && !link.contains("javascript"), "a script link points nowhere: {link}");
  assert!(link.contains("href=\"https://example.com/a\""), "an https link is kept: {link}");
}

async fn kept_html(service: &Arc<dyn Transport>, body: &str) -> String {
  let args = ValueMap::from_iter([
    ("id".to_owned(), Value::str("kickoff")),
    ("parent".to_owned(), Value::str("")),
    ("who".to_owned(), Value::str("carol")),
    ("body".to_owned(), Value::str(body)),
  ]);
  match call(service, "addBlip", args).await {
    Value::Map(blip) => match blip.get("html") {
      Some(Value::Str(html)) => html.to_string(),
      other => panic!("the kept blip carries no html: {other:?}"),
    },
    other => panic!("addBlip answered something other than a blip: {other:?}"),
  }
}

#[tokio::test]
async fn the_wave_page_writes_a_blip_as_the_markup_the_service_made() {
  let config = Config::load(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
  let (field, controller) =
    StateControllerBuilder::new(Arc::new(rules()), InProcessSession::<Op, Conn>::new(), Arc::new(Views), Field::new(backend::seed())).build();
  tokio::spawn(controller.run());
  let (service, _kept) = backend::service(field);
  let host = Host::from_config(config).unwrap().services_over(service).build().unwrap();

  let session = SessionCell::default();
  session.insert("name", Value::str("alice"));
  let html = host.render_to_string("/wave/kickoff", RenderMode::Html, session).await.unwrap();
  assert!(html.contains("<strong>its own subject</strong>"), "the seed's markdown is markup in the page: {html}");
  assert!(html.contains("<a href=\"https://commonmark.org\">markdown</a>"), "and its link is kept: {html}");
}

#[tokio::test]
async fn on_a_tick_keystrokes_wait_for_it_and_the_wave_goes_out_once() {
  let rules = Rules { on_tick: true, ..rules() };
  let mut field = Field::new(backend::seed());
  apply(&rules, &mut field, 1, vec![watch("kickoff", "alice")]).await;
  apply(&rules, &mut field, 2, vec![watch("kickoff", "bob")]).await;

  let typed = apply(&rules, &mut field, 1, vec![typing("", "h")]).await;
  assert!(typed.snapshots.is_empty(), "a keystroke sends nothing on its own");
  apply(&rules, &mut field, 1, vec![typing("", "he")]).await;
  let tick = LogicInput::TimeStep { delta_time: std::time::Duration::from_millis(50) };
  let ticked = rules.process_input(&mut field, tick).await.unwrap();
  assert_eq!(ticked.snapshots.len(), 1, "the tick sends the wave once for both keystrokes");
  let quiet = rules.process_input(&mut field, LogicInput::TimeStep { delta_time: std::time::Duration::from_millis(50) }).await.unwrap();
  assert!(quiet.snapshots.is_empty(), "a tick with nothing new sends nothing");
}

#[tokio::test]
async fn a_uniform_view_is_one_request_and_carries_every_draft() {
  let rules = Rules { uniform: true, ..rules() };
  let mut field = Field::new(backend::seed());
  apply(&rules, &mut field, 1, vec![watch("kickoff", "alice")]).await;
  apply(&rules, &mut field, 2, vec![watch("kickoff", "bob")]).await;

  let typed = apply(&rules, &mut field, 1, vec![typing("", "hello")]).await;
  assert_eq!(typed.snapshots.len(), 1, "one request for the wave");
  assert!(typed.snapshots[0].uniform, "and it is a shared one");
  let context = Some(plaza::snapshot::SnapshotContext::ForPerspective("kickoff".to_owned()));
  match Views.create_snapshot(&field, None, context).await.unwrap() {
    Some(Op::View(view)) => assert_eq!(view.drafts.len(), 1, "alice's draft is in the one view everyone gets, alice included"),
    other => panic!("no shared view: {other:?}"),
  }
}
