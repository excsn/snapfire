//! `fsr dev --terminal`: the root page lowered, drawn and stepped by keys read the way Ink reads them.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use snapfire_fsr_cli::terminal::{key_event, Press, Report, Session};
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
  assert_eq!(session.timers(), [Timer { id: None, path: String::new(), handler: 0, every: Duration::from_millis(100), repeats: true, settles: None, carry: Value::Null }, Timer { id: None, path: String::new(), handler: 1, every: Duration::from_millis(2000), repeats: false, settles: None, carry: Value::Null }]);
  let line = |node| snapfire_fsr_terminal::render(&node, 20).lines()[0].clone();
  assert_eq!(line(session.view().unwrap()), "ticks 0");
  let timers = session.timers();
  session.fire(&timers[0]).unwrap();
  assert_eq!(line(session.fire(&timers[0]).unwrap()), "ticks 2");
  assert_eq!(line(session.fire(&timers[1]).unwrap()), "ticks 100");
}

#[test]
fn a_component_that_would_render_in_a_browser_refuses_the_page_naming_it() {
  let page = "import { Box, Text } from \"ink\";\nimport { measureElement } from \"ink\";\n\nfunction Item() {\n  const { width } = measureElement(null);\n  return <Text>{width}</Text>;\n}\n\nexport default function Page() {\n  return <Box><Item /></Box>;\n}\n";
  let why = Session::open(&app(page)).err().expect("refused");
  assert!(why.contains("Item") && why.contains("measureElement"), "{why}");
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

#[test]
fn a_spawned_process_is_handed_to_the_terminal_and_its_output_and_exit_step_the_page() {
  let page = "import childProcess from \"node:child_process\";\nimport { useEffect, useState } from \"react\";\nimport { Text } from \"ink\";\n\nexport default function Out() {\n  const [output, setOutput] = useState(\"\");\n  const [code, setCode] = useState(-1);\n  useEffect(() => {\n    const sub = childProcess.spawn(\"echo\", [\"hi\"]);\n    sub.on(\"error\", (error) => {\n      setOutput(error.message);\n    });\n    sub.stdout.setEncoding(\"utf8\");\n    sub.stdout.on(\"data\", (chunk: string) => {\n      setOutput((o) => o + chunk);\n    });\n    sub.on(\"exit\", (exitCode) => setCode(exitCode));\n  }, [setOutput]);\n  return <Text>{output}|{code}</Text>;\n}\n";
  let mut session = Session::open(&app(page)).unwrap();
  let line = |node| snapfire_fsr_terminal::render(&node, 20).lines()[0].clone();
  assert_eq!(line(session.view().unwrap()), "|-1");
  let spawns = session.take_spawns();
  assert_eq!(spawns.len(), 1);
  assert_eq!((spawns[0].command.as_str(), spawns[0].args.clone()), ("echo", vec!["hi".to_owned()]));
  assert!(session.take_spawns().is_empty(), "a process starts once");
  assert_eq!(line(session.deliver(&spawns[0], Report::Stdout, &Value::Str("hi".into())).unwrap()), "hi|-1");
  let mut exit = snapfire_fsr_core::ValueMap::default();
  exit.insert("code".to_owned(), Value::Int(0));
  exit.insert("signal".to_owned(), Value::Null);
  assert_eq!(line(session.deliver(&spawns[0], Report::Exit, &Value::Map(exit)).unwrap()), "hi|0");
  let mut failed = snapfire_fsr_core::ValueMap::default();
  failed.insert("message".to_owned(), Value::Str("spawn echo ENOENT".into()));
  assert_eq!(line(session.deliver(&spawns[0], Report::Error, &Value::Map(failed)).unwrap()), "spawn echo ENOENT|0");
}

#[test]
fn an_effect_runs_again_after_a_render_that_changed_its_dependencies_and_not_otherwise() {
  let page = "import { useEffect, useState } from \"react\";\nimport { Text, useInput } from \"ink\";\n\nexport default function Doubled() {\n  const [n, setN] = useState(1);\n  const [other, setOther] = useState(0);\n  const [doubled, setDoubled] = useState(0);\n  const [runs, setRuns] = useState(0);\n  useInput((input) => {\n    if (input === \"+\") setN((v) => v + 1);\n    if (input === \"o\") setOther((v) => v + 1);\n  });\n  useEffect(() => {\n    setDoubled(n * 2);\n    setRuns((r) => r + 1);\n  }, [n]);\n  return <Text>{n} {doubled} {runs} {other}</Text>;\n}\n";
  let mut session = Session::open(&app(page)).unwrap();
  let line = |node| snapfire_fsr_terminal::render(&node, 20).lines()[0].clone();
  assert_eq!(line(session.view().unwrap()), "1 2 1 0", "it runs after the first render");
  let press = |c: char| {
    let Some(Press::Input(key)) = key_event(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)) else { panic!("{c} is input") };
    key
  };
  assert_eq!(line(session.press(&press('+')).unwrap()), "2 4 2 0");
  assert_eq!(line(session.press(&press('o')).unwrap()), "2 4 2 1", "a render that left n alone does not run it");
}

#[test]
fn a_component_that_throws_a_pending_promise_shows_its_suspense_fallback_until_the_promise_settles() {
  let page = "import { Suspense } from \"react\";\nimport { Text } from \"ink\";\n\nlet promise: Promise<void> | undefined;\nlet state: string | undefined;\nlet value: string | undefined;\n\nconst read = () => {\n  if (!promise) {\n    promise = new Promise((resolve) => {\n      setTimeout(resolve, 500);\n    });\n    state = \"pending\";\n    (async () => {\n      await promise;\n      state = \"done\";\n      value = \"Hello World\";\n    })();\n  }\n  if (state === \"pending\") {\n    throw promise;\n  }\n  if (state === \"done\") {\n    return value;\n  }\n};\n\nfunction Example() {\n  const message = read();\n  return <Text>{message}</Text>;\n}\n\nfunction Fallback() {\n  return <Text>Loading...</Text>;\n}\n\nexport default function Root() {\n  return (\n    <Suspense fallback={<Fallback />}>\n      <Example />\n    </Suspense>\n  );\n}\n";
  let mut session = Session::open(&app(page)).unwrap();
  let line = |node| snapfire_fsr_terminal::render(&node, 20).lines()[0].clone();
  assert_eq!(line(session.view().unwrap()), "Loading...");
  let started = session.take_started();
  assert_eq!(started.len(), 1, "the promise's timer: {started:?}");
  assert_eq!(started[0].every, std::time::Duration::from_millis(500));
  assert_eq!(line(session.fire(&started[0]).unwrap()), "Hello World");
  assert!(session.take_started().is_empty(), "the promise is made once");
  assert_eq!(line(session.view().unwrap()), "Hello World");
}

#[test]
fn a_timer_callback_carries_what_it_reads_and_a_module_map_it_changes_lasts() {
  let page = "import { Suspense } from \"react\";\nimport { Text } from \"ink\";\n\nconst cache = new Map<string, { done: boolean; promise?: Promise<void> }>();\n\nfunction load(key: string, ms: number): string {\n  const held = cache.get(key);\n  if (held?.done) {\n    return `${key} after ${ms}`;\n  }\n  if (held) {\n    throw held.promise;\n  }\n  const promise = new Promise<void>((resolve) => {\n    setTimeout(() => {\n      cache.set(key, { done: true });\n      resolve();\n    }, ms);\n  });\n  cache.set(key, { done: false, promise });\n  throw promise;\n}\n\nfunction Item({ name, ms }: { name: string; ms: number }) {\n  return <Text>{load(name, ms)}</Text>;\n}\n\nexport default function Page() {\n  return (\n    <Suspense fallback={<Text>waiting</Text>}>\n      <Item name=\"a\" ms={30} />\n    </Suspense>\n  );\n}\n";
  let mut session = Session::open(&app(page)).unwrap();
  let line = |node| snapfire_fsr_terminal::render(&node, 20).lines()[0].clone();
  assert_eq!(line(session.view().unwrap()), "waiting");
  let started = session.take_started();
  assert_eq!(started.iter().map(|t| t.every).collect::<Vec<_>>(), [std::time::Duration::from_millis(30)]);
  assert_eq!(line(session.fire(&started[0]).unwrap()), "a after 30", "the callback read key and ms from what it carried");
}

#[test]
fn suspend_terminal_gives_up_the_screen_until_the_child_it_waits_on_exits() {
  let page = "import { spawn } from \"node:child_process\";\nimport { useState } from \"react\";\nimport { Text, useApp, useInput } from \"ink\";\n\nconst runChild = async (command: string, args: string[]): Promise<void> =>\n  new Promise((resolve, reject) => {\n    const child = spawn(command, args, { stdio: \"inherit\" });\n    child.on(\"exit\", (code, signal) => {\n      if (code === 0) {\n        resolve();\n      } else {\n        reject(new Error(`exited with ${code}`));\n      }\n    });\n    child.on(\"error\", reject);\n  });\n\nexport default function Page() {\n  const { suspendTerminal } = useApp();\n  const [status, setStatus] = useState(\"ready\");\n  useInput((input) => {\n    void (async () => {\n      setStatus(\"suspended\");\n      try {\n        await suspendTerminal(async () => {\n          await runChild(\"sh\", [\"-c\", input]);\n        });\n        setStatus(\"resumed\");\n      } catch (error) {\n        setStatus(`failed: ${(error as Error).message}`);\n      }\n    })();\n  });\n  return <Text>{status}</Text>;\n}\n";
  let mut session = Session::open(&app(page)).unwrap();
  let line = |node| snapfire_fsr_terminal::render(&node, 30).lines()[0].clone();
  assert_eq!(line(session.view().unwrap()), "ready");
  let press = |c: char| {
    let Some(Press::Input(key)) = key_event(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)) else { panic!("{c} is input") };
    key
  };
  assert_eq!(line(session.press(&press('r')).unwrap()), "suspended");
  assert!(!session.wants_screen(), "the child owns the terminal");
  let spawns = session.take_spawns();
  assert_eq!((spawns[0].command.as_str(), spawns[0].args.clone()), ("sh", vec!["-c".to_owned(), "r".to_owned()]));
  let exit = |code: i128| {
    let mut exit = snapfire_fsr_core::ValueMap::default();
    exit.insert("code".to_owned(), Value::Int(code));
    exit.insert("signal".to_owned(), Value::Null);
    Value::Map(exit)
  };
  assert_eq!(line(session.deliver(&spawns[0], Report::Exit, &exit(0)).unwrap()), "resumed");
  assert!(session.wants_screen(), "the screen is back once the child exits");
  session.press(&press('x')).unwrap();
  let failing = session.take_spawns();
  assert_eq!(line(session.deliver(&failing[0], Report::Exit, &exit(1)).unwrap()), "failed: exited with 1");
  assert!(session.wants_screen());
}

#[test]
fn an_async_helper_catches_a_rejected_await_inside_its_try_and_runs_what_follows_the_try_either_way() {
  let page = "import { spawn } from \"node:child_process\";\nimport { useState } from \"react\";\nimport { Text, useInput } from \"ink\";\n\nconst run = (command: string): Promise<void> =>\n  new Promise((resolve, reject) => {\n    const child = spawn(\"sh\", [\"-c\", command]);\n    child.on(\"exit\", (code) => {\n      if (code === 0) {\n        resolve();\n      } else {\n        reject(new Error(`exited with ${code}`));\n      }\n    });\n  });\n\nasync function attempt(command: string): Promise<string> {\n  let said = \"\";\n  try {\n    await run(command);\n    said = \"ok\";\n  } catch (error) {\n    said = `failed: ${(error as Error).message}`;\n  }\n  return `${command} ${said}`;\n}\n\nexport default function Page() {\n  const [status, setStatus] = useState(\"ready\");\n  useInput((input) => {\n    void (async () => {\n      const said = await attempt(input);\n      setStatus(said);\n    })();\n  });\n  return <Text>{status}</Text>;\n}\n";
  let mut session = Session::open(&app(page)).unwrap();
  let line = |node| snapfire_fsr_terminal::render(&node, 30).lines()[0].clone();
  let press = |c: char| {
    let Some(Press::Input(key)) = key_event(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)) else { panic!("{c} is input") };
    key
  };
  let exit = |code: i128| {
    let mut exit = snapfire_fsr_core::ValueMap::default();
    exit.insert("code".to_owned(), Value::Int(code));
    exit.insert("signal".to_owned(), Value::Null);
    Value::Map(exit)
  };
  session.press(&press('a')).unwrap();
  let spawns = session.take_spawns();
  assert_eq!(line(session.deliver(&spawns[0], Report::Exit, &exit(0)).unwrap()), "a ok");
  session.press(&press('b')).unwrap();
  let spawns = session.take_spawns();
  assert_eq!(line(session.deliver(&spawns[0], Report::Exit, &exit(2)).unwrap()), "b failed: exited with 2");
}

#[test]
fn an_interval_an_effect_starts_without_a_cleanup_keeps_ticking_after_the_effect_runs_again() {
  let page = "import { useEffect, useState } from \"react\";\nimport { Text, useInput } from \"ink\";\n\nexport default function Page() {\n  const [running, setRunning] = useState(false);\n  const [n, setN] = useState(0);\n  useEffect(() => {\n    if (!running) return;\n    setInterval(() => {\n      setN((n) => n + 1);\n    }, 1000);\n  }, [running]);\n  useInput(() => setRunning(!running));\n  return <Text>{n} {running ? \"running\" : \"paused\"}</Text>;\n}\n";
  let mut session = Session::open(&app(page)).unwrap();
  let line = |node| snapfire_fsr_terminal::render(&node, 30).lines()[0].clone();
  let space = || {
    let Some(Press::Input(key)) = key_event(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE)) else { panic!("space is input") };
    key
  };
  session.view().unwrap();
  session.take_started();
  session.press(&space()).unwrap();
  let first = session.take_started();
  session.press(&space()).unwrap();
  assert_eq!(line(session.fire(&first[0]).unwrap()), "1 paused", "nothing cleared the interval");
  session.press(&space()).unwrap();
  let second = session.take_started();
  session.fire(&first[0]).unwrap();
  assert_eq!(line(session.fire(&second[0]).unwrap()), "3 running", "a second interval runs beside the first");
}
