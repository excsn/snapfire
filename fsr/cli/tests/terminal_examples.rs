//! The terminal applications under `examples/`, each opened as `fsr dev --terminal` opens it and driven by keys, timers and the reports of the processes it starts, which are handed to it rather than run.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use snapfire_fsr_cli::terminal::{key_event, Press, Session};
use snapfire_fsr_core::Value;

fn open(example: &str) -> Session {
  let app = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples").join(example).join("app");
  Session::open(&app).unwrap_or_else(|why| panic!("{example}: {why}"))
}

fn lines(node: snapfire_fsr_terminal::Node, columns: u16) -> Vec<String> {
  snapfire_fsr_terminal::render(&node, columns).lines().iter().map(|line| line.trim_end().to_owned()).collect()
}

fn key(code: KeyCode) -> Value {
  let Some(Press::Input(press)) = key_event(KeyEvent::new(code, KeyModifiers::NONE)) else { panic!("{code:?} is input") };
  press
}

fn type_text(session: &mut Session, text: &str) {
  for ch in text.chars() {
    session.press(&key(KeyCode::Char(ch))).unwrap();
  }
}

#[test]
fn tasks_adds_what_is_typed_moves_the_focus_with_tab_and_logs_a_task_marked_done() {
  let mut session = open("tasks_terminal_ts");
  let first = lines(session.view().unwrap(), 80);
  assert_eq!(first[0], "Tasks, 0 open of 0");
  assert_eq!(first[2], "new task>");
  type_text(&mut session, "buy milk");
  assert_eq!(session.cursor(), Some((18, 2)), "the cursor sits after what was typed");
  session.press(&key(KeyCode::Enter)).unwrap();
  type_text(&mut session, "call home");
  let shown = lines(session.press(&key(KeyCode::Enter)).unwrap(), 80);
  assert_eq!(shown[0], "Tasks, 2 open of 2");
  assert!(shown.iter().any(|l| l.contains("buy milk") && l.contains("open")), "{shown:?}");
  session.press(&key(KeyCode::Tab)).unwrap();
  let focused = lines(session.view().unwrap(), 80);
  assert!(focused.iter().any(|l| l.starts_with(">") && l.contains("buy milk")), "Tab focuses the first task: {focused:?}");
  let done = lines(session.press(&key(KeyCode::Char(' '))).unwrap(), 80);
  assert!(done[0].starts_with("✔ buy milk at "), "the log above the page: {done:?}");
  assert!(done.iter().any(|l| l == "Tasks, 1 open of 2"), "{done:?}");
  assert!(done.iter().any(|l| l.contains("buy milk") && l.contains("done")), "{done:?}");
}

#[test]
fn log_streams_what_git_writes_and_scrolls_it_in_a_box_it_measures() {
  use snapfire_fsr_cli::terminal::Report;
  let mut session = open("log_terminal_ts");
  let first = lines(session.view().unwrap(), 80);
  assert!(first[0].contains("reading git log"), "{first:?}");
  let spawns = session.take_spawns();
  assert_eq!((spawns[0].command.as_str(), spawns[0].args[0].as_str()), ("git", "log"));
  let written: String = (1..=30).map(|n| format!("\u{1b}[33m{n:07x}\u{1b}[m commit number {n}\n")).collect();
  session.deliver(&spawns[0], Report::Stdout, &Value::Str(written.into())).unwrap();
  let mut exit = snapfire_fsr_core::ValueMap::default();
  exit.insert("code".to_owned(), Value::Int(0));
  exit.insert("signal".to_owned(), Value::Null);
  let done = lines(session.deliver(&spawns[0], Report::Exit, &Value::Map(exit)).unwrap(), 80);
  assert!(done[0].starts_with("30 commits"), "{done:?}");
  assert!(done[2].contains("0000001 commit number 1"), "escape codes stripped: {done:?}");
  assert!(done.last().unwrap().starts_with("1-10 of 30"), "{done:?}");
  let scrolled = lines(session.press(&key(KeyCode::Char('j'))).unwrap(), 80);
  assert!(scrolled[2].contains("commit number 2"), "{scrolled:?}");
  let bottom = lines(session.press(&key(KeyCode::Char('G'))).unwrap(), 80);
  assert!(bottom.last().unwrap().starts_with("21-30 of 30"), "{bottom:?}");
}
