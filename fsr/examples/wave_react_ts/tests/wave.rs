//! Waves on one controller: the rules, the view each window is built and the
//! service the loaders and actions call.

use std::collections::BTreeMap;
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
use wave_react_ts::field::{Change, Conn, Field, Op, Rules, View, Views};
use wave_react_ts::gadgets::State;

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
  Op::Typing { parent: parent.to_owned(), anchor: String::new(), writing: !body.is_empty(), body: body.to_owned() }
}

fn open(blip: &str, block: &str) -> Op {
  Op::Open { blip: blip.to_owned(), block: block.to_owned() }
}

fn amend(blip: &str, block: &str, who: &str, body: &str) -> Op {
  Op::Amend { wave: "kickoff".to_owned(), blip: blip.to_owned(), block: block.to_owned(), who: who.to_owned(), body: body.to_owned() }
}

/// The ids of the blocks of the kickoff wave's blip at `at`, in order.
fn ids(field: &Field, at: usize) -> Vec<String> {
  field.waves["kickoff"].blips[at].blocks.iter().map(|block| block.id.clone()).collect()
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
async fn a_draft_beside_a_block_carries_the_block() {
  let (rules, mut field) = (rules(), Field::new(backend::seed()));
  apply(&rules, &mut field, 1, vec![watch("kickoff", "alice")]).await;
  apply(&rules, &mut field, 2, vec![watch("kickoff", "bob")]).await;

  apply(&rules, &mut field, 1, vec![Op::Typing { parent: "4".to_owned(), anchor: "b3.1".to_owned(), writing: true, body: "and the host".to_owned() }]).await;
  let seen = view(&field, 2).await.drafts;
  assert_eq!((seen[0].parent.as_str(), seen[0].anchor.as_str()), ("4", "b3.1"), "bob sees it beside the list item alice answers");
}

#[tokio::test]
async fn a_draft_whose_words_are_kept_back_tells_the_others_only_who_is_typing() {
  let (rules, mut field) = (rules(), Field::new(backend::seed()));
  apply(&rules, &mut field, 1, vec![watch("kickoff", "alice")]).await;
  apply(&rules, &mut field, 2, vec![watch("kickoff", "bob")]).await;

  apply(&rules, &mut field, 1, vec![Op::Typing { parent: "2".to_owned(), anchor: String::new(), writing: true, body: String::new() }]).await;
  let seen = view(&field, 2).await.drafts;
  assert_eq!(seen.len(), 1, "bob is told alice is typing: {seen:?}");
  assert_eq!((seen[0].who.as_str(), seen[0].body.as_str()), ("alice", ""), "and nothing of what");

  apply(&rules, &mut field, 1, vec![Op::Typing { parent: "2".to_owned(), anchor: String::new(), writing: false, body: String::new() }]).await;
  assert!(view(&field, 2).await.drafts.is_empty(), "until she stops");
}

#[tokio::test]
async fn a_blip_is_held_by_one_window_and_the_others_watch_it_change() {
  let (rules, mut field) = (rules(), Field::new(backend::seed()));
  apply(&rules, &mut field, 1, vec![watch("kickoff", "alice")]).await;
  apply(&rules, &mut field, 2, vec![watch("kickoff", "bob")]).await;

  apply(&rules, &mut field, 1, vec![open("2", "")]).await;
  let held = view(&field, 2).await.edits;
  assert_eq!(held.len(), 1, "bob is shown the blip alice took");
  assert_eq!(held[0].who, "alice");
  assert_eq!(held[0].body, "Good. I will take the runtime half.", "and it starts from what the blip says");
  assert!(view(&field, 1).await.edits.is_empty(), "alice is never shown her own rewrite, the way she is never shown her own draft");

  apply(&rules, &mut field, 2, vec![open("2", "")]).await;
  apply(&rules, &mut field, 2, vec![Op::Rewriting { blip: "2".to_owned(), block: String::new(), body: "bob got in".to_owned() }]).await;
  assert_eq!(view(&field, 2).await.edits[0].who, "alice", "the second window to reach for it changes nothing");

  apply(&rules, &mut field, 1, vec![Op::Rewriting { blip: "2".to_owned(), block: String::new(), body: "I will take the runtime half, and the host.".to_owned() }]).await;
  assert_eq!(view(&field, 2).await.edits[0].body, "I will take the runtime half, and the host.", "bob watches the words as they land");

  apply(&rules, &mut field, 1, vec![Op::Close { blip: "2".to_owned(), block: String::new() }]).await;
  assert!(view(&field, 2).await.edits.is_empty(), "letting it go frees it");
  assert_eq!(field.waves["kickoff"].blips[1].body(), "Good. I will take the runtime half.", "and keeps nothing");
}

#[tokio::test]
async fn an_amend_keeps_the_rewrite_and_a_departure_lets_the_blip_go() {
  let (rules, mut field) = (rules(), Field::new(backend::seed()));
  apply(&rules, &mut field, 1, vec![watch("kickoff", "alice")]).await;
  apply(&rules, &mut field, 2, vec![watch("kickoff", "bob")]).await;

  apply(&rules, &mut field, 1, vec![open("2", "")]).await;
  apply(&rules, &mut field, 1, vec![Op::Amend { wave: "kickoff".to_owned(), blip: "2".to_owned(), block: String::new(), who: "alice".to_owned(), body: "the runtime and the host".to_owned() }])
    .await;
  assert_eq!(field.waves["kickoff"].blips[1].body(), "the runtime and the host");
  assert_eq!(field.waves["kickoff"].blips[1].edited, "10:00", "an amended blip says when it was amended");
  assert_eq!(field.waves["kickoff"].blips[1].editors, ["alice"], "and who amended it, though bob wrote it");

  apply(&rules, &mut field, 1, vec![Op::Amend { wave: "kickoff".to_owned(), blip: "2".to_owned(), block: String::new(), who: "alice".to_owned(), body: "the runtime, the host".to_owned() }])
    .await;
  apply(&rules, &mut field, 2, vec![Op::Amend { wave: "kickoff".to_owned(), blip: "2".to_owned(), block: String::new(), who: "bob".to_owned(), body: "the runtime, the host, the lot".to_owned() }])
    .await;
  assert_eq!(field.waves["kickoff"].blips[1].editors, ["alice", "bob"], "each of them once, in the order they first came to it");
  assert!(view(&field, 2).await.edits.is_empty(), "keeping it releases the hold");

  apply(&rules, &mut field, 3, vec![watch("kickoff", "carol")]).await;
  apply(&rules, &mut field, 3, vec![open("2", "")]).await;
  apply(&rules, &mut field, 3, vec![Op::Amend { wave: "kickoff".to_owned(), blip: "2".to_owned(), block: String::new(), who: "carol".to_owned(), body: "the lot".to_owned() }]).await;
  assert_eq!(field.waves["kickoff"].participants, ["alice", "bob", "carol"], "a rewrite puts its author on the wave, as keeping a blip does");

  apply(&rules, &mut field, 2, vec![open("1", "")]).await;
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

  apply(&rules, &mut field, 1, vec![open("2", "")]).await;
  assert!(view(&field, 2).await.edits.is_empty(), "nor holding a blip");
  apply(&rules, &mut field, 2, vec![open("2", "")]).await;
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
async fn a_board_in_a_blip_is_one_the_wave_shares_and_every_rule_is_the_field_s() {
  let (field, controller) =
    StateControllerBuilder::new(Arc::new(rules()), InProcessSession::<Op, Conn>::new(), Arc::new(Views), Field::new(backend::seed())).build();
  tokio::spawn(controller.run());
  let (service, mut told) = backend::service(field);

  let play = |who: &str, cell: i128| {
    ValueMap::from_iter([
      ("id".to_owned(), Value::str("kickoff")),
      ("blip".to_owned(), Value::str("7")),
      ("block".to_owned(), Value::str("b2")),
      ("who".to_owned(), Value::str(who)),
      ("cell".to_owned(), Value::Int(cell)),
    ])
  };
  let gadget = |blip: &Value| field_of(&nth(field_of(blip, "blocks"), 1), "gadget").clone();
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

  let board = gadget(&call(&service, "play", play("alice", 4)).await);
  assert_eq!(marks(&board), "x", "the first to move takes x");
  assert!(turn(&board).contains("o"));
  assert_eq!(told.try_recv().unwrap(), "wave/kickoff", "a move is a change to the wave, so the topic goes out");

  let board = gadget(&call(&service, "play", play("alice", 0)).await);
  assert_eq!(marks(&board), "x", "alice holds x, so she cannot answer herself");

  let board = gadget(&call(&service, "play", play("bob", 4)).await);
  assert_eq!(marks(&board), "x", "a cell that is taken stays taken");

  let board = gadget(&call(&service, "play", play("bob", 0)).await);
  assert_eq!(marks(&board), "ox", "bob takes o and the corner");

  call(&service, "play", play("alice", 1)).await;
  call(&service, "play", play("bob", 3)).await;
  let board = gadget(&call(&service, "play", play("alice", 7)).await);
  assert_eq!(marks(&board), "oxoxx", "the middle column is x's");
  assert!(format!("{board:?}").contains("\"won\""), "{board:?}");
  match &board {
    Value::Map(map) => assert_eq!(map.get("won"), Some(&Value::str("x")), "three in a column is the game"),
    other => panic!("{other:?}"),
  }

  let board = gadget(&call(&service, "play", play("bob", 2)).await);
  assert_eq!(marks(&board), "oxoxx", "and nothing moves after it is won");

  let fresh = ValueMap::from_iter([("id".to_owned(), Value::str("kickoff")), ("blip".to_owned(), Value::str("7")), ("block".to_owned(), Value::str("b2")), ("who".to_owned(), Value::str("bob"))]);
  let cleared = gadget(&call(&service, "play", fresh).await);
  assert_eq!(marks(&cleared), "", "a move with no cell is a new board");
  assert!(turn(&cleared).contains("x"), "which starts at x again");

  let answer = ValueMap::from_iter([
    ("id".to_owned(), Value::str("kickoff")),
    ("blip".to_owned(), Value::str("8")),
    ("block".to_owned(), Value::str("b1")),
    ("who".to_owned(), Value::str("carol")),
    ("answer".to_owned(), Value::str("Friday")),
  ]);
  let poll = field_of(&nth(field_of(&call(&service, "vote", answer).await, "blocks"), 0), "gadget").clone();
  assert_eq!(field_of(&poll, "kind"), &Value::str("poll"));
  let friday = nth(field_of(&poll, "choices"), 1);
  assert_eq!((field_of(&friday, "answer"), field_of(&friday, "count")), (&Value::str("Friday"), &Value::F64(1.0)), "carol's answer is counted under its choice: {poll:?}");
  assert_eq!(field_of(&friday, "who"), &Value::seq(vec![Value::str("carol")]), "by name");
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
  assert!(html.contains("<ol class=\"replies\">"), "the transcript is rendered on the server, nested: {html}");
  match session.get("waves") {
    Some(Value::Map(waves)) => assert!(waves.contains_key("kickoff"), "the loader recorded the wave: {waves:?}"),
    other => panic!("the session holds no waves: {other:?}"),
  }
}

#[tokio::test]
async fn a_blip_body_is_parts_the_service_parses_with_raw_html_as_text() {
  let (field, controller) =
    StateControllerBuilder::new(Arc::new(rules()), InProcessSession::<Op, Conn>::new(), Arc::new(Views), Field::new(backend::seed())).build();
  tokio::spawn(controller.run());
  let (service, _kept) = backend::service(field);
  let parts = |body: &'static str| {
    let service = service.clone();
    async move { flat(&block_parts(&kept(&service, "", "", body).await)) }
  };

  assert_eq!(parts("carol, **arriving** late").await, ["p@b1", "text=carol, ", "strong", "text=arriving", "text= late"]);
  let block = parts("<script>alert(1)</script>").await;
  assert!(block.iter().all(|part| part.starts_with("p") || part.starts_with("text=")) && block.iter().any(|part| part.contains("<script>")), "a raw HTML block is text: {block:?}");
  let inline = parts("a <b>bold</b> claim").await;
  assert!(inline.iter().all(|part| part.starts_with("p") || part.starts_with("text=")) && inline.contains(&"text=<b>".to_owned()), "raw inline HTML is text: {inline:?}");
  let links: Vec<String> = parts("[run](javascript:alert(1)) or [read](https://example.com/a)").await.into_iter().filter(|part| part.starts_with("a")).collect();
  assert_eq!(links, ["a>#", "a>https://example.com/a"], "a script link points nowhere and an https link is kept");
  assert_eq!(parts("- one\n- two\n\n  more").await[..3], ["ul", "li", "p@b1.0.0"], "a list item holding a paragraph is answered at the paragraph");
  let linked: Vec<String> = parts("see [the spec][cm]\n\nmore\n\n[cm]: https://commonmark.org").await.into_iter().filter(|part| part.starts_with("a")).collect();
  assert_eq!(linked, ["a>https://commonmark.org"], "a reference link resolves against a definition in another block");
}

/// Every part of a blip, its blocks' parts one after another.
fn block_parts(blip: &Value) -> Value {
  let Value::Seq(blocks) = field_of(blip, "blocks") else { panic!("no blocks in {blip:?}") };
  let mut out = Vec::new();
  for block in blocks.iter() {
    if let Value::Seq(parts) = field_of(block, "parts") {
      out.extend(parts.iter().cloned());
    }
  }
  Value::seq(out)
}

#[tokio::test]
async fn a_reply_to_a_block_sits_under_it_and_one_whose_block_is_gone_joins_the_others() {
  let (field, controller) =
    StateControllerBuilder::new(Arc::new(rules()), InProcessSession::<Op, Conn>::new(), Arc::new(Views), Field::new(backend::seed())).build();
  tokio::spawn(controller.run());
  let (service, _kept) = backend::service(field);

  kept(&service, "4", "b2", "beside the second paragraph").await;
  kept(&service, "4", "b9", "beside a block that is not there").await;
  let wave = call(&service, "getWave", ValueMap::from_iter([("id".to_owned(), Value::str("kickoff"))])).await;
  let four = nth(field_of(&wave, "blips"), 1);
  let second = nth(field_of(&nth(field_of(&four, "blocks"), 1), "parts"), 0);
  assert_eq!(field_of(&second, "at"), &Value::str("b2"));
  assert!(format!("{:?}", field_of(&second, "replies")).contains("beside the second paragraph"), "{second:?}");
  let item = nth(field_of(&nth(field_of(&nth(field_of(&four, "blocks"), 2), "parts"), 0), "children"), 1);
  assert_eq!(field_of(&item, "at"), &Value::str("b3.1"));
  assert!(format!("{:?}", field_of(&item, "replies")).contains("The client is mine too."), "the seed's reply to the list item: {item:?}");
  let replies = format!("{:?}", field_of(&four, "replies"));
  assert!(replies.contains("beside a block that is not there") && !replies.contains("beside the second paragraph"), "{replies}");
}

async fn kept(service: &Arc<dyn Transport>, parent: &str, anchor: &str, body: &str) -> Value {
  let args = ValueMap::from_iter([
    ("id".to_owned(), Value::str("kickoff")),
    ("parent".to_owned(), Value::str(parent)),
    ("anchor".to_owned(), Value::str(anchor)),
    ("who".to_owned(), Value::str("carol")),
    ("body".to_owned(), Value::str(body)),
  ]);
  call(service, "addBlip", args).await
}

fn field_of<'v>(value: &'v Value, key: &str) -> &'v Value {
  match value {
    Value::Map(map) => map.get(key).unwrap_or_else(|| panic!("no `{key}` in {value:?}")),
    other => panic!("not a map: {other:?}"),
  }
}

fn nth(value: &Value, i: usize) -> Value {
  match value {
    Value::Seq(items) => items.iter().nth(i).cloned().unwrap_or_else(|| panic!("no item {i} in {value:?}")),
    other => panic!("not a sequence: {other:?}"),
  }
}

/// Every part, depth first, as its kind then `@at` for a block a reply can
/// answer, `=text` and `>href` where it has them.
fn flat(parts: &Value) -> Vec<String> {
  fn walk(parts: &Value, out: &mut Vec<String>) {
    let Value::Seq(parts) = parts else { return };
    for part in parts.iter() {
      let text = |key: &str| match field_of(part, key) {
        Value::Str(text) => text.to_string(),
        _ => String::new(),
      };
      let mut shown = text("kind");
      for (mark, key) in [('@', "at"), ('=', "text"), ('>', "href")] {
        if !text(key).is_empty() {
          shown.push(mark);
          shown.push_str(&text(key));
        }
      }
      out.push(shown);
      walk(field_of(part, "children"), out);
    }
  }
  let mut out = Vec::new();
  walk(parts, &mut out);
  out
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
  assert!(html.contains("<ol class=\"asides\">") && html.contains("The client is mine too."), "bob's reply to the list item sits under it: {html}");
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

#[tokio::test]
async fn two_windows_hold_two_blocks_of_one_blip_and_the_whole_blip_waits_for_both() {
  let (rules, mut field) = (rules(), Field::new(backend::seed()));
  for (conn, name) in [(1, "alice"), (2, "bob"), (3, "carol")] {
    apply(&rules, &mut field, conn, vec![watch("kickoff", name)]).await;
  }
  apply(&rules, &mut field, 1, vec![open("4", "b1")]).await;
  apply(&rules, &mut field, 2, vec![open("4", "b2")]).await;
  let held = view(&field, 3).await.edits;
  assert_eq!(held.iter().map(|edit| (edit.block.as_str(), edit.who.as_str())).collect::<Vec<_>>(), [("b1", "alice"), ("b2", "bob")], "a block each");
  assert_eq!(held[1].body, "A reply can answer one part of a blip:", "each starting from its own block's markdown");

  apply(&rules, &mut field, 3, vec![open("4", "")]).await;
  apply(&rules, &mut field, 3, vec![open("4", "b1")]).await;
  assert_eq!(view(&field, 1).await.edits.len(), 1, "carol can take neither the whole blip nor a block someone holds");

  apply(&rules, &mut field, 1, vec![Op::Close { blip: "4".to_owned(), block: "b1".to_owned() }]).await;
  apply(&rules, &mut field, 2, vec![Op::Close { blip: "4".to_owned(), block: "b2".to_owned() }]).await;
  apply(&rules, &mut field, 3, vec![open("4", "")]).await;
  assert_eq!(view(&field, 1).await.edits[0].who, "carol", "with both let go the whole blip is hers");
  apply(&rules, &mut field, 1, vec![open("4", "b3")]).await;
  assert_eq!(view(&field, 2).await.edits.len(), 1, "and while she holds it no block of it can be taken");
}

#[tokio::test]
async fn a_block_rewrite_changes_that_block_alone_and_its_id_stays_with_what_came_first() {
  let (rules, mut field) = (rules(), Field::new(backend::seed()));
  apply(&rules, &mut field, 1, vec![watch("kickoff", "alice")]).await;
  let four = 3;
  apply(&rules, &mut field, 1, vec![amend("4", "b2", "alice", "A reply can answer a part.\n\nOr a list item:")]).await;
  assert_eq!(ids(&field, four), ["b1", "b2", "b4", "b3"], "the block became two, the first keeping its id");
  assert_eq!(field.waves["kickoff"].blips[four].blocks[1].text, "A reply can answer a part.");
  assert!(field.waves["kickoff"].blips[four].body().starts_with("Anything that is **its own subject**"), "the blocks around it are untouched");
  assert_eq!(field.waves["kickoff"].blips[four].editors, ["alice"]);

  apply(&rules, &mut field, 1, vec![amend("4", "b4", "alice", "")]).await;
  assert_eq!(ids(&field, four), ["b1", "b2", "b3"], "a block rewritten to nothing is gone");
}

#[tokio::test]
async fn a_rewrite_of_the_whole_blip_keeps_the_ids_of_the_blocks_it_kept_or_edited() {
  let (rules, mut field) = (rules(), Field::new(backend::seed()));
  apply(&rules, &mut field, 1, vec![watch("kickoff", "alice")]).await;
  let whole = "Anything, reworded.\n\nA reply can answer one part of a blip:\n\nA new line.\n\n- the runtime\n- the client";
  apply(&rules, &mut field, 1, vec![amend("4", "", "alice", whole)]).await;
  assert_eq!(ids(&field, 3), ["b1", "b2", "b4", "b3"], "the edited block and the two it kept keep their ids and the inserted one takes a new id");
  assert_eq!(field.waves["kickoff"].blips[3].body(), whole);
}

#[tokio::test]
async fn the_log_holds_every_durable_change_and_replaying_it_rebuilds_the_wave() {
  let (rules, mut field) = (rules(), Field::new(backend::seed()));
  assert_eq!(field.waves["kickoff"].log.len(), 7, "the seed is written as its log, a kept blip each");
  let first = field.waves["kickoff"].blips[3].body();

  apply(&rules, &mut field, 1, vec![watch("kickoff", "alice")]).await;
  apply(&rules, &mut field, 1, vec![typing("", "never kept")]).await;
  apply(&rules, &mut field, 1, vec![open("4", "b2")]).await;
  apply(&rules, &mut field, 1, vec![amend("4", "b2", "alice", "A reply can answer a part.\n\nOr a list item:")]).await;
  let play = |who: &str, cell: usize| Op::Play { wave: "kickoff".to_owned(), blip: "7".to_owned(), block: "b2".to_owned(), who: who.to_owned(), cell: Some(cell) };
  let vote = |who: &str, answer: &str| Op::Vote { wave: "kickoff".to_owned(), blip: "8".to_owned(), block: "b1".to_owned(), who: who.to_owned(), answer: answer.to_owned() };
  apply(&rules, &mut field, 1, vec![play("alice", 4)]).await;
  apply(&rules, &mut field, 1, vec![play("bob", 4)]).await;
  apply(&rules, &mut field, 1, vec![amend("4", "b9", "alice", "no such block")]).await;
  apply(&rules, &mut field, 1, vec![vote("alice", "Friday"), vote("bob", "Someday")]).await;

  let wave = &field.waves["kickoff"];
  assert_eq!(wave.log.len(), 10, "the amend, the move and the vote are logged; the keystroke, the hold, the move onto a taken cell, the amend of a missing block and the answer the poll does not offer are not");
  let replayed = wave.replayed(wave.log.len());
  assert_eq!(replayed.blips, wave.blips, "the whole log replayed is the wave, block ids and gadgets included");
  assert_eq!(replayed.participants, wave.participants);

  let before = wave.replayed(7);
  assert_eq!(before.blips[3].body(), first, "seven steps in, blip 4 still says what it first said");
  assert!(before.blips.iter().all(|blip| blip.gadgets.is_empty()), "and nobody has played or voted");
  assert!(wave.replayed(0).blips.is_empty() && wave.replayed(0).participants.is_empty(), "before the first step there is nothing and nobody");
}

#[tokio::test]
async fn the_service_shows_a_wave_after_any_step_of_its_log() {
  let (field, controller) =
    StateControllerBuilder::new(Arc::new(rules()), InProcessSession::<Op, Conn>::new(), Arc::new(Views), Field::new(backend::seed())).build();
  tokio::spawn(controller.run());
  let (service, _kept) = backend::service(field);
  kept(&service, "4", "b2", "a later word").await;
  let at = |at: &'static str| {
    let service = service.clone();
    async move { call(&service, "getWave", ValueMap::from_iter([("id".to_owned(), Value::str("kickoff")), ("at".to_owned(), Value::str(at))])).await }
  };

  let live = at("").await;
  assert_eq!((field_of(&live, "step"), field_of(&live, "steps"), field_of(&live, "live")), (&Value::F64(8.0), &Value::F64(8.0), &Value::Bool(true)));
  assert!(!lit(&live), "nothing is lit on the wave as it stands");

  let three = at("3").await;
  assert_eq!((field_of(&three, "step"), field_of(&three, "live")), (&Value::F64(3.0), &Value::Bool(false)));
  let blips = field_of(&three, "blips");
  assert_eq!(match blips { Value::Seq(blips) => blips.len(), _ => 0 }, 1, "three steps in, alice's first blip is the only one at the top");
  let reply = nth(field_of(&nth(field_of(&nth(blips, 0), "replies"), 0), "replies"), 0);
  assert_eq!(field_of(&reply, "lit"), &Value::Bool(true), "the blip the third step kept is lit: {reply:?}");
  assert_eq!(field_of(field_of(&three, "change"), "kind"), &Value::str("kept"));
  assert_eq!(field_of(field_of(&three, "change"), "who"), &Value::str("alice"));
  assert_eq!(field_of(&three, "participants"), &Value::seq(vec![Value::str("alice"), Value::str("bob")]));

  let last = at("8").await;
  assert_eq!((field_of(&last, "step"), field_of(&last, "live")), (&Value::F64(8.0), &Value::Bool(false)), "the last step is still playback");
  assert!(lit(&last), "and lights what the last change touched");
  assert_eq!(field_of(&at("12").await, "step"), &Value::F64(8.0), "a step past the end is the last step");
  assert_eq!(field_of(&at("soon").await, "live"), &Value::Bool(true), "one that is not a number is the wave as it stands");
}

/// Whether anything in `value` is lit.
fn lit(value: &Value) -> bool {
  match value {
    Value::Map(map) => map.iter().any(|(key, value)| (key == "lit" && *value == Value::Bool(true)) || lit(value)),
    Value::Seq(items) => items.iter().any(lit),
    _ => false,
  }
}

#[tokio::test]
async fn a_gadget_is_a_block_and_keeps_its_state_while_the_text_around_it_changes() {
  let (rules, mut field) = (rules(), Field::new(backend::seed()));
  apply(&rules, &mut field, 1, vec![watch("kickoff", "alice")]).await;
  let vote = |who: &str, answer: &str| Op::Vote { wave: "kickoff".to_owned(), blip: "8".to_owned(), block: "b1".to_owned(), who: who.to_owned(), answer: answer.to_owned() };
  apply(&rules, &mut field, 1, vec![vote("alice", "Friday"), vote("bob", "Thursday"), vote("carol", "Someday")]).await;
  let poll = |field: &Field| field.waves["kickoff"].blips[6].gadgets.get("b1").cloned();
  let votes = |pairs: &[(&str, &str)]| Some(State::Votes(pairs.iter().map(|(who, answer)| ((*who).to_owned(), (*answer).to_owned())).collect::<BTreeMap<_, _>>()));
  assert_eq!(poll(&field), votes(&[("alice", "Friday"), ("bob", "Thursday")]), "one answer each; carol's is not one the poll offers");

  let above = "Pick one.\n\n```gadget poll\nWhen do we launch?\nThursday\nFriday\nNext week\n```";
  apply(&rules, &mut field, 1, vec![amend("8", "", "alice", above)]).await;
  assert_eq!(ids(&field, 6), ["b2", "b1"], "the paragraph written above the poll takes a new id and the poll keeps its own");
  assert_eq!(poll(&field), votes(&[("alice", "Friday"), ("bob", "Thursday")]), "so the answers stay with it");

  apply(&rules, &mut field, 1, vec![amend("8", "b1", "alice", "```gadget noughts\n```")]).await;
  assert_eq!(poll(&field), None, "a poll rewritten as a board keeps nothing of the poll");
  apply(&rules, &mut field, 1, vec![Op::Play { wave: "kickoff".to_owned(), blip: "8".to_owned(), block: "b1".to_owned(), who: "alice".to_owned(), cell: Some(0) }]).await;
  assert!(matches!(poll(&field), Some(State::Board(game)) if game.cells == "x........"), "and plays as a board from then on");
}

#[tokio::test]
async fn a_poll_s_author_closes_it_for_good_and_a_blip_of_theirs_announces_the_result() {
  let rules = rules();
  let mut field = Field::new(backend::seed());
  let vote = |who: &str, answer: &str| Op::Vote { wave: "kickoff".to_owned(), blip: "8".to_owned(), block: "b1".to_owned(), who: who.to_owned(), answer: answer.to_owned() };
  let close = |who: &str| Op::CloseVote { wave: "kickoff".to_owned(), blip: "8".to_owned(), block: "b1".to_owned(), who: who.to_owned() };
  let poll = |field: &Field| field.waves["kickoff"].blips.iter().find(|blip| blip.id == 8).cloned().unwrap();
  let fence = poll(&field).blocks[0].text.clone();
  let before = field.waves["kickoff"].blips.len();

  apply(&rules, &mut field, 1, vec![vote("alice", "Friday"), vote("bob", "Friday"), vote("carol", "Thursday"), close("bob")]).await;
  assert!(poll(&field).closed.is_empty(), "only its author closes a poll");
  apply(&rules, &mut field, 1, vec![close("alice")]).await;
  let announced = field.waves["kickoff"].blips.last().cloned().unwrap();
  assert_eq!(field.waves["kickoff"].blips.len(), before + 1, "closing posts one blip");
  assert_eq!((announced.who.as_str(), announced.parent.as_str(), announced.at.as_str()), ("alice", "", "10:00"), "at the top of the wave, from its author, as they close it");
  assert_eq!(announced.body(), "Poll closed: [When do we launch?](#blip-8)\n\nFriday won with 2 of 3 votes.");

  let held = poll(&field).gadgets.get("b1").cloned();
  apply(&rules, &mut field, 1, vec![close("alice"), vote("dave", "Thursday"), vote("alice", "Friday"), amend("8", "b1", "alice", "```gadget poll\nWhen?\nNever\n```")]).await;
  assert_eq!(field.waves["kickoff"].blips.len(), before + 1, "closing it again posts nothing, since there is no reopening");
  assert_eq!(poll(&field).gadgets.get("b1").cloned(), held, "a closed poll takes no answer and nobody takes theirs back");
  assert_eq!(poll(&field).blocks[0].text, fence, "and no rewrite");

  let wave = &field.waves["kickoff"];
  assert!(matches!(wave.log.last(), Some(Change::Closed { .. })), "the close is one change, the announcement with it");
  assert_eq!(wave.replayed(wave.log.len()).blips.last().map(|blip| blip.body()), Some(announced.body()), "which replaying rebuilds");
}

#[tokio::test]
async fn a_poll_takes_answers_until_its_deadline_and_none_after_it() {
  let at = |secs: u64| Rules { instant: Box::new(move || secs), ..rules() };
  let (before, after) = (at(951_868_800), at(951_868_860));
  let mut field = Field::new(backend::seed());
  let fence = "```gadget poll until=2000-03-01T00:01Z\nWhen?\nThursday\nFriday\n```";
  apply(&before, &mut field, 1, vec![Op::Keep { wave: "kickoff".to_owned(), parent: String::new(), anchor: String::new(), who: "alice".to_owned(), body: fence.to_owned() }]).await;
  let id = field.waves["kickoff"].blips.last().unwrap().id.to_string();
  let vote = |who: &str| Op::Vote { wave: "kickoff".to_owned(), blip: id.clone(), block: "b1".to_owned(), who: who.to_owned(), answer: "Friday".to_owned() };

  apply(&before, &mut field, 1, vec![vote("bob")]).await;
  apply(&after, &mut field, 1, vec![vote("carol"), amend(&id, "b1", "alice", "```gadget poll\nWhen?\nNever\n```")]).await;
  let polled = field.waves["kickoff"].blips.last().cloned().unwrap();
  assert_eq!(polled.gadgets.get("b1"), Some(&State::Votes(BTreeMap::from([("bob".to_owned(), "Friday".to_owned())]))), "bob answered in time; at the deadline carol's answer is refused");
  assert_eq!(polled.blocks[0].text, fence, "and so is a rewrite of the poll");

  apply(&after, &mut field, 1, vec![Op::CloseVote { wave: "kickoff".to_owned(), blip: id.clone(), block: "b1".to_owned(), who: "alice".to_owned() }]).await;
  assert_eq!(field.waves["kickoff"].blips.last().map(|blip| blip.body()), Some(format!("Poll closed: [When?](#blip-{id})\n\nFriday won with 1 of 1 vote.")), "its author can still announce it");
}

#[tokio::test]
async fn the_service_closes_a_vote_for_its_author_and_the_gadget_says_so() {
  let (field, controller) =
    StateControllerBuilder::new(Arc::new(rules()), InProcessSession::<Op, Conn>::new(), Arc::new(Views), Field::new(backend::seed())).build();
  tokio::spawn(controller.run());
  let (service, mut told) = backend::service(field);
  let close = ValueMap::from_iter([("id", "kickoff"), ("blip", "8"), ("block", "b1"), ("who", "alice")].map(|(key, value)| (key.to_owned(), Value::str(value))));
  let poll = field_of(&nth(field_of(&call(&service, "closeVote", close).await, "blocks"), 0), "gadget").clone();
  assert_eq!(field_of(&poll, "closed"), &Value::Bool(true));
  assert_eq!((field_of(&poll, "ended"), field_of(&poll, "ends")), (&Value::Bool(false), &Value::str("")), "a poll with no deadline has none to pass");
  assert_eq!(told.try_recv().unwrap(), "wave/kickoff", "and the wave's pages hear of it");
}

#[tokio::test]
async fn a_reply_stays_with_its_block_when_a_block_is_written_before_it() {
  let (field, controller) =
    StateControllerBuilder::new(Arc::new(rules()), InProcessSession::<Op, Conn>::new(), Arc::new(Views), Field::new(backend::seed())).build();
  tokio::spawn(controller.run());
  let (service, _kept) = backend::service(field);
  let body = "A new first line.\n\nAnything that is **its own subject** goes at the top level. A blip is [markdown](https://commonmark.org).\n\nA reply can answer one part of a blip:\n\n- the runtime\n- the client";
  let args = ValueMap::from_iter([
    ("id".to_owned(), Value::str("kickoff")),
    ("blip".to_owned(), Value::str("4")),
    ("who".to_owned(), Value::str("carol")),
    ("body".to_owned(), Value::str(body)),
  ]);
  call(&service, "editBlip", args).await;
  let wave = call(&service, "getWave", ValueMap::from_iter([("id".to_owned(), Value::str("kickoff"))])).await;
  let list = nth(field_of(&nth(field_of(&wave, "blips"), 1), "blocks"), 3);
  assert_eq!(field_of(&list, "id"), &Value::str("b3"), "the list moved down a place and kept its id");
  let item = nth(field_of(&nth(field_of(&list, "parts"), 0), "children"), 1);
  assert!(format!("{:?}", field_of(&item, "replies")).contains("The client is mine too."), "bob's reply to the second item is still under it: {item:?}");
}
