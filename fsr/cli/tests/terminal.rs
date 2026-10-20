//! `fsr dev --terminal`: the root page lowered, drawn and stepped by keys read the way Ink reads them.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use snapfire_fsr_cli::terminal::{key_event, Press, Session};
use snapfire_fsr_core::Value;

const COUNTER: &str = "import { useState } from \"react\";\nimport { Box, Text, useInput } from \"ink\";\n\nexport default function Counter() {\n  const [count, setCount] = useState(0);\n  useInput((input, key) => {\n    if (input === \"+\" || key.upArrow) setCount(count + 1);\n  });\n  return (\n    <Box borderStyle=\"round\" paddingX={1}>\n      <Text>Count: {count}</Text>\n    </Box>\n  );\n}\n";

static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn app(page: &str) -> std::path::PathBuf {
  let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let app = std::env::temp_dir().join(format!("fsr-terminal-{}-{n}", std::process::id())).join("app");
  let _ = std::fs::remove_dir_all(&app);
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
  assert_eq!(session.timers(), [Timer { path: String::new(), handler: 0, every: Duration::from_millis(100), repeats: true }, Timer { path: String::new(), handler: 1, every: Duration::from_millis(2000), repeats: false }]);
  let line = |node| snapfire_fsr_terminal::render(&node, 20).lines()[0].clone();
  assert_eq!(line(session.view().unwrap()), "ticks 0");
  let timers = session.timers();
  session.fire(&timers[0]).unwrap();
  assert_eq!(line(session.fire(&timers[0]).unwrap()), "ticks 2");
  assert_eq!(line(session.fire(&timers[1]).unwrap()), "ticks 100");
}

#[test]
fn a_component_that_would_render_in_a_browser_refuses_the_page_naming_it() {
  let page = "import { Box, Text } from \"ink\";\nimport { useFocus } from \"ink\";\n\nfunction Item() {\n  const { isFocused } = useFocus();\n  return <Text>{isFocused ? \"on\" : \"off\"}</Text>;\n}\n\nexport default function Page() {\n  return <Box><Item /></Box>;\n}\n";
  let why = Session::open(&app(page)).err().expect("refused");
  assert!(why.contains("Item") && why.contains("useFocus"), "{why}");
}

#[test]
fn a_session_has_exited_once_the_page_calls_exit() {
  let quits = "import { Text, useApp, useInput } from \"ink\";\n\nexport default function Quits() {\n  const { exit: leave } = useApp();\n  useInput((input) => {\n    if (input === \"q\") leave();\n  });\n  return <Text>press q</Text>;\n}\n";
  let mut session = Session::open(&app(quits)).unwrap();
  session.view().unwrap();
  assert!(!session.exited());
  let Some(Press::Input(q)) = key_event(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)) else { panic!() };
  session.press(&q).unwrap();
  assert!(session.exited());
}

#[test]
fn a_session_draws_at_the_terminal_size_and_prints_what_the_page_writes_above_it() {
  let page = "import { useEffect } from \"react\";\nimport { Text, useStdout, useWindowSize } from \"ink\";\n\nexport default function Sized() {\n  const { write } = useStdout();\n  const { columns } = useWindowSize();\n  useEffect(() => {\n    const timer = setInterval(() => write(\"hello\\n\"), 500);\n    return () => clearInterval(timer);\n  }, [write]);\n  return <Text>width {columns}</Text>;\n}\n";
  let mut session = Session::open(&app(page)).unwrap();
  session.resize(120, 40);
  let lines = |node| snapfire_fsr_terminal::render(&node, 30).lines();
  assert_eq!(lines(session.view().unwrap()), ["width 120"]);
  let timers = session.timers();
  session.fire(&timers[0]).unwrap();
  assert_eq!(lines(session.fire(&timers[0]).unwrap()), ["hello", "hello", "width 120"]);
}

const NESTED: &str = "import { useEffect, useState } from \"react\";\nimport { Box, Text, useInput } from \"ink\";\n\nfunction Counter({ label }: { label: string }) {\n  const [count, setCount] = useState(0);\n  useInput((input) => {\n    if (input === \"+\") setCount(count + 1);\n  });\n  return <Text>{label}: {count}</Text>;\n}\n\nfunction Ticker() {\n  const [ticks, setTicks] = useState(0);\n  useEffect(() => {\n    const timer = setInterval(() => setTicks((n) => n + 1), 50);\n    return () => clearInterval(timer);\n  }, []);\n  return <Text>ticks: {ticks}</Text>;\n}\n\nexport default function Page() {\n  return (\n    <Box flexDirection=\"column\">\n      <Counter label=\"a\" />\n      <Counter label=\"b\" />\n      <Ticker />\n    </Box>\n  );\n}\n";

#[test]
fn a_stateful_component_a_page_places_draws_in_place_and_every_one_steps() {
  let mut session = Session::open(&app(NESTED)).unwrap();
  let lines = |session: &mut Session| snapfire_fsr_terminal::render(&session.view().unwrap(), 20).lines().iter().map(|line| line.trim_end().to_owned()).collect::<Vec<_>>();
  assert_eq!(lines(&mut session), ["a: 0", "b: 0", "ticks: 0"]);
  let Some(Press::Input(plus)) = key_event(KeyEvent::new(KeyCode::Char('+'), KeyModifiers::NONE)) else { panic!("+ is input") };
  session.press(&plus).unwrap();
  assert_eq!(lines(&mut session), ["a: 1", "b: 1", "ticks: 0"], "every useInput hears a key, as in Ink");
  let timers = session.timers();
  assert_eq!(timers.len(), 1);
  session.fire(&timers[0]).unwrap();
  assert_eq!(lines(&mut session), ["a: 1", "b: 1", "ticks: 1"]);
}
