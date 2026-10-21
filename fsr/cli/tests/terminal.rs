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
  let page = "import { Box, Text } from \"ink\";\nimport { useBoxMetrics } from \"ink\";\n\nfunction Item() {\n  const { width } = useBoxMetrics();\n  return <Text>{width}</Text>;\n}\n\nexport default function Page() {\n  return <Box><Item /></Box>;\n}\n";
  let why = Session::open(&app(page)).err().expect("refused");
  assert!(why.contains("Item") && why.contains("useBoxMetrics"), "{why}");
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

const FOCUS: &str = "import { Box, Text, useFocus, useFocusManager, useInput } from \"ink\";\n\nfunction Item({ label, id }: { label: string; id?: string }) {\n  const { isFocused } = useFocus({ id });\n  return <Text>{label}{isFocused ? \" *\" : \"\"}</Text>;\n}\n\nexport default function Page() {\n  const { focus } = useFocusManager();\n  useInput((input) => {\n    if (input === \"3\") focus(\"third\");\n  });\n  return (\n    <Box flexDirection=\"column\">\n      <Item label=\"one\" />\n      <Item label=\"two\" />\n      <Item label=\"three\" id=\"third\" />\n    </Box>\n  );\n}\n";

#[test]
fn tab_moves_the_focus_through_the_focusables_in_order_and_focus_names_one() {
  let mut session = Session::open(&app(FOCUS)).unwrap();
  let lines = |node| snapfire_fsr_terminal::render(&node, 20).lines().iter().map(|line| line.trim_end().to_owned()).collect::<Vec<_>>();
  let press = |code, modifiers| match key_event(KeyEvent::new(code, modifiers)) {
    Some(Press::Input(press)) => press,
    _ => panic!("an input"),
  };
  assert_eq!(lines(session.view().unwrap()), ["one", "two", "three"]);
  assert_eq!(lines(session.press(&press(KeyCode::Tab, KeyModifiers::NONE)).unwrap()), ["one *", "two", "three"]);
  assert_eq!(lines(session.press(&press(KeyCode::Tab, KeyModifiers::NONE)).unwrap()), ["one", "two *", "three"]);
  assert_eq!(lines(session.press(&press(KeyCode::BackTab, KeyModifiers::SHIFT)).unwrap()), ["one *", "two", "three"]);
  assert_eq!(lines(session.press(&press(KeyCode::BackTab, KeyModifiers::SHIFT)).unwrap()), ["one", "two", "three *"], "Shift+Tab from the first wraps to the last");
  assert_eq!(lines(session.press(&press(KeyCode::Esc, KeyModifiers::NONE)).unwrap()), ["one", "two", "three"]);
  assert_eq!(lines(session.press(&press(KeyCode::Char('3'), KeyModifiers::NONE)).unwrap()), ["one", "two", "three *"]);
}

const SEARCH: &str = "import { useState, useTransition } from \"react\";\nimport { Text, useInput } from \"ink\";\n\nconst segmenter = new Intl.Segmenter(undefined, { granularity: \"grapheme\" });\n\nfunction deleteLastGrapheme(text: string): string {\n  const last = segmenter.segment(text).containing(text.length - 1);\n  return text.slice(0, last?.index ?? 0);\n}\n\nexport default function Search() {\n  const [query, setQuery] = useState(\"\");\n  const [deferred, setDeferred] = useState(\"\");\n  const [isPending, startTransition] = useTransition();\n  useInput((input, key) => {\n    if (key.backspace) {\n      setQuery(deleteLastGrapheme);\n      startTransition(() => {\n        setDeferred(deleteLastGrapheme);\n      });\n    } else if (input !== \"\") {\n      setQuery((q) => q + input);\n      startTransition(() => setDeferred((q) => q + input));\n    }\n  });\n  return <Text>{query}|{deferred}|{isPending ? \"pending\" : \"done\"}</Text>;\n}\n";

#[test]
fn a_backspace_deletes_the_last_grapheme_and_a_transition_runs_in_place() {
  let mut session = Session::open(&app(SEARCH)).unwrap();
  let line = |node| snapfire_fsr_terminal::render(&node, 30).lines()[0].clone();
  let press = |code| match key_event(KeyEvent::new(code, KeyModifiers::NONE)) {
    Some(Press::Input(press)) => press,
    _ => panic!("an input"),
  };
  session.press(&press(KeyCode::Char('a'))).unwrap();
  assert_eq!(line(session.press(&press(KeyCode::Char('👍'))).unwrap()), "a👍|a👍|done");
  assert_eq!(line(session.press(&press(KeyCode::Backspace)).unwrap()), "a|a|done", "the emoji is one grapheme of two UTF-16 units");
}

#[test]
fn a_ref_written_while_rendering_keeps_the_value_for_the_next_render() {
  let page = "import { useRef, useState } from \"react\";\nimport { Text, useInput } from \"ink\";\n\nexport default function Even() {\n  const [n, setN] = useState(0);\n  const last = useRef(0);\n  if (n % 2 === 0) {\n    last.current = n;\n  }\n  useInput(() => setN(n + 1));\n  return <Text>{n} {last.current}</Text>;\n}\n";
  let mut session = Session::open(&app(page)).unwrap();
  let line = |node| snapfire_fsr_terminal::render(&node, 20).lines()[0].clone();
  let Some(Press::Input(key)) = key_event(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE)) else { panic!("x is input") };
  assert_eq!(line(session.view().unwrap()), "0 0");
  assert_eq!(line(session.press(&key).unwrap()), "1 0");
  assert_eq!(line(session.press(&key).unwrap()), "2 2");
  assert_eq!(line(session.press(&key).unwrap()), "3 2");
}

#[test]
fn a_mount_effect_runs_once_and_a_timer_its_function_restarts_counts_with_its_own_let() {
  let page = "import { useEffect, useState } from \"react\";\nimport { Text } from \"ink\";\n\nexport default function Tests() {\n  const [done, setDone] = useState(0);\n  useEffect(() => {\n    let completed = 0;\n    let timer: ReturnType<typeof setTimeout> | undefined;\n    const run = () => {\n      if (completed++ >= 3) {\n        return;\n      }\n      setDone((n) => n + 1);\n      timer = setTimeout(run, 100);\n    };\n    run();\n    return () => clearTimeout(timer);\n  }, []);\n  return <Text>done {done}</Text>;\n}\n";
  let mut session = Session::open(&app(page)).unwrap();
  let line = |node| snapfire_fsr_terminal::render(&node, 20).lines()[0].clone();
  assert_eq!(line(session.view().unwrap()), "done 1", "the effect ran its function once on mount");
  for done in 2..=3 {
    let started = session.take_started();
    assert_eq!(started.len(), 1, "the function started one timeout");
    assert_eq!(started[0].every, std::time::Duration::from_millis(100));
    assert_eq!(line(session.fire(&started[0]).unwrap()), format!("done {done}"));
  }
  let last = session.take_started();
  assert_eq!(line(session.fire(&last[0]).unwrap()), "done 3", "the fourth run returns early");
  assert!(session.take_started().is_empty(), "and starts no timer");
}

#[test]
fn static_draws_its_items_in_a_column_above_the_rest() {
  let page = "import { useState } from \"react\";\nimport { Box, Static, Text, useInput } from \"ink\";\n\nexport default function Log() {\n  const [done, setDone] = useState([\"a\"]);\n  useInput(() => setDone((d) => [...d, `t${d.length}`]));\n  return (\n    <>\n      <Static items={done} style={{ paddingLeft: 1 }}>\n        {(item, i) => <Text key={item}>✔ {i} {item}</Text>}\n      </Static>\n      <Text>count {done.length}</Text>\n    </>\n  );\n}\n";
  let mut session = Session::open(&app(page)).unwrap();
  let lines = |node| snapfire_fsr_terminal::render(&node, 20).lines().iter().map(|l| l.trim_end().to_owned()).collect::<Vec<_>>();
  assert_eq!(lines(session.view().unwrap())[..2], [" ✔ 0 a", "count 1"]);
  let Some(Press::Input(key)) = key_event(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE)) else { panic!("x is input") };
  assert_eq!(lines(session.press(&key).unwrap())[..3], [" ✔ 0 a", " ✔ 1 t1", "count 2"]);
}

#[test]
fn use_box_metrics_reads_the_layout_and_a_content_offset_scrolls_a_hidden_overflow() {
  let page = "import { useRef, useState } from \"react\";\nimport { Box, Text, useBoxMetrics, useInput } from \"ink\";\n\nexport default function Scroll() {\n  const ref = useRef(null);\n  const contentRef = useRef(null);\n  const { clientHeight } = useBoxMetrics(ref);\n  const content = useBoxMetrics(contentRef);\n  const [top, setTop] = useState(0);\n  const max = Math.max(0, content.height - clientHeight);\n  useInput(() => setTop((t) => Math.min(max, t + 1)));\n  return (\n    <Box flexDirection=\"column\">\n      <Box ref={ref} height={4} overflow=\"hidden\" contentOffsetY={top} borderStyle=\"round\" flexDirection=\"column\">\n        <Box ref={contentRef} flexDirection=\"column\" flexShrink={0}>\n          <Text>a</Text>\n          <Text>b</Text>\n          <Text>c</Text>\n        </Box>\n      </Box>\n      <Text>{top}/{max}</Text>\n    </Box>\n  );\n}\n";
  let mut session = Session::open(&app(page)).unwrap();
  let lines = |node| snapfire_fsr_terminal::render(&node, 6).lines().iter().map(|l| l.trim_end().to_owned()).collect::<Vec<_>>();
  assert_eq!(lines(session.view().unwrap()), ["╭────╮", "│a   │", "│b   │", "╰────╯", "0/1"]);
  let Some(Press::Input(key)) = key_event(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)) else { panic!("down is input") };
  assert_eq!(lines(session.press(&key).unwrap()), ["╭────╮", "│b   │", "│c   │", "╰────╯", "1/1"]);
  assert_eq!(lines(session.press(&key).unwrap())[4], "1/1", "clamped at the bottom");
}

#[test]
fn the_cursor_goes_where_the_page_set_it_and_is_hidden_when_it_sets_none() {
  let page = "import { useState } from \"react\";\nimport { Text, useCursor, useInput } from \"ink\";\n\nexport default function Prompt() {\n  const [text, setText] = useState(\"\");\n  const { setCursorPosition } = useCursor();\n  useInput((input) => setText((t) => t + input));\n  if (text !== \"done\") {\n    setCursorPosition({ x: 2 + text.length, y: 0 });\n  }\n  return <Text>&gt; {text}</Text>;\n}\n";
  let mut session = Session::open(&app(page)).unwrap();
  session.view().unwrap();
  assert_eq!(session.cursor(), Some((2, 0)));
  let Some(Press::Input(key)) = key_event(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)) else { panic!("a is input") };
  session.press(&key).unwrap();
  assert_eq!(session.cursor(), Some((3, 0)));
  let mut done = Session::open(&app(&page.replace("useState(\"\")", "useState(\"done\")"))).unwrap();
  done.view().unwrap();
  assert_eq!(done.cursor(), None);
}
