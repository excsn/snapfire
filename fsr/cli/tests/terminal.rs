//! `fsr dev --terminal`: the root page lowered, drawn and stepped by keys read the way Ink reads them.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use snapfire_fsr_cli::terminal::{key_event, Press, Session};
use snapfire_fsr_core::Value;

const COUNTER: &str = "import { useState } from \"react\";\nimport { Box, Text, useInput } from \"ink\";\n\nexport default function Counter() {\n  const [count, setCount] = useState(0);\n  useInput((input, key) => {\n    if (input === \"+\" || key.upArrow) setCount(count + 1);\n  });\n  return (\n    <Box borderStyle=\"round\" paddingX={1}>\n      <Text>Count: {count}</Text>\n    </Box>\n  );\n}\n";

fn app(page: &str) -> std::path::PathBuf {
  let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
  let app = std::env::temp_dir().join(format!("fsr-terminal-{}-{nanos}", std::process::id())).join("app");
  std::fs::create_dir_all(app.join("routes")).unwrap();
  std::fs::write(app.join("routes/page.tsx"), page).unwrap();
  app
}

fn middle(session: &mut Session) -> String {
  let node = session.view().unwrap();
  snapfire_fsr_terminal::render(&node, 20).lines()[1].clone()
}

#[test]
fn a_session_draws_the_root_page_steps_it_on_a_key_and_keeps_its_state_across_a_reload() {
  let app = app(COUNTER);
  let mut session = Session::open(&app).unwrap();
  assert_eq!(middle(&mut session), "│ Count: 0         │");
  let Some(Press::Input(plus)) = key_event(KeyEvent::new(KeyCode::Char('+'), KeyModifiers::NONE)) else { panic!("+ is input") };
  session.press(&plus).unwrap();
  assert_eq!(middle(&mut session), "│ Count: 1         │");
  std::fs::write(app.join("routes/page.tsx"), COUNTER.replace("Count:", "Total:")).unwrap();
  session.reload().unwrap();
  assert_eq!(middle(&mut session), "│ Total: 1         │", "a reload keeps the state");
}

#[test]
fn a_page_that_is_not_ink_says_so() {
  let mut session = Session::open(&app("export default function Page() {\n  return <div>web</div>;\n}\n")).unwrap();
  assert!(session.view().unwrap_err().contains("div"));
}

#[test]
fn keys_read_as_ink_reads_them() {
  let field = |press: Option<Press>, path: &[&str]| -> Value {
    let Some(Press::Input(Value::Map(mut map))) = press else { panic!("an input") };
    for name in &path[..path.len() - 1] {
      let Some(Value::Map(inner)) = map.get(*name).cloned() else { panic!("{name}") };
      map = inner;
    }
    map[path[path.len() - 1]].clone()
  };
  let plain = |code| key_event(KeyEvent::new(code, KeyModifiers::NONE));
  assert_eq!(field(plain(KeyCode::Char('a')), &["input"]), Value::Str("a".into()));
  assert_eq!(field(plain(KeyCode::Char('a')), &["key", "ctrl"]), Value::Bool(false));
  assert_eq!(field(plain(KeyCode::Up), &["key", "upArrow"]), Value::Bool(true));
  assert_eq!(field(plain(KeyCode::Up), &["input"]), Value::Str("".into()));
  assert_eq!(field(plain(KeyCode::Enter), &["key", "return"]), Value::Bool(true));
  assert_eq!(field(key_event(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT)), &["key", "tab"]), Value::Bool(true));
  assert_eq!(field(key_event(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL)), &["key", "ctrl"]), Value::Bool(true));
  assert!(matches!(key_event(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)), Some(Press::Exit)), "Ctrl-C exits, as Ink's exitOnCtrlC does");
}

#[test]
fn a_session_lists_the_timers_its_page_started_and_a_fired_one_steps_the_state() {
  use snapfire_fsr_cli::terminal::Timer;
  use std::time::Duration;
  let clock = "import { useEffect, useState } from \"react\";\nimport { Text } from \"ink\";\n\nexport default function Clock() {\n  const [ticks, setTicks] = useState(0);\n  useEffect(() => {\n    const timer = setInterval(() => setTicks((t) => t + 1), 100);\n    return () => clearInterval(timer);\n  }, []);\n  useEffect(() => {\n    setTimeout(() => setTicks(100), 2000);\n  }, []);\n  return <Text>ticks {ticks}</Text>;\n}\n";
  let mut session = Session::open(&app(clock)).unwrap();
  assert_eq!(session.timers(), [Timer { handler: 0, every: Duration::from_millis(100), repeats: true }, Timer { handler: 1, every: Duration::from_millis(2000), repeats: false }]);
  let line = |node| snapfire_fsr_terminal::render(&node, 20).lines()[0].clone();
  assert_eq!(line(session.view().unwrap()), "ticks 0");
  session.fire(0).unwrap();
  assert_eq!(line(session.fire(0).unwrap()), "ticks 2");
  assert_eq!(line(session.fire(1).unwrap()), "ticks 100");
}
