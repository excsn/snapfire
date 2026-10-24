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
fn a_page_refused_when_the_session_opens_shows_why_and_opens_once_it_is_fixed() {
  let page = "import { Text } from \"ink\";\nimport { measureElement } from \"ink\";\n\nexport default function Page() {\n  const { width } = measureElement(null);\n  return <Text>{width}</Text>;\n}\n";
  let app = app(page);
  let mut shown = Vec::new();
  let session = snapfire_fsr_cli::terminal::open_watching(&app, &[], |why| {
    shown.push(why.to_owned());
    std::fs::write(app.join("routes/page.tsx"), "import { Text } from \"ink\";\n\nexport default function Page() {\n  return <Text>fixed</Text>;\n}\n")?;
    Ok(true)
  });
  let mut session = session.unwrap().expect("opened after the edit");
  assert!(shown.len() == 1 && shown[0].contains("measureElement"), "{shown:?}");
  assert_eq!(snapfire_fsr_terminal::render(&session.view().unwrap(), 10).lines()[0].trim_end(), "fixed");
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

#[test]
fn esc_clears_the_focus_an_autofocused_item_took_when_it_mounted() {
  let page = FOCUS.replace("useFocus({ id })", "useFocus({ id, autoFocus: label === \"two\" })");
  let mut session = Session::open(&app(&page)).unwrap();
  let lines = |node| snapfire_fsr_terminal::render(&node, 20).lines().iter().map(|line| line.trim_end().to_owned()).collect::<Vec<_>>();
  let press = |code| match key_event(KeyEvent::new(code, KeyModifiers::NONE)) {
    Some(Press::Input(press)) => press,
    _ => panic!("an input"),
  };
  assert_eq!(lines(session.view().unwrap()), ["one", "two *", "three"]);
  assert_eq!(lines(session.press(&press(KeyCode::Esc)).unwrap()), ["one", "two", "three"]);
  assert_eq!(lines(session.press(&press(KeyCode::Char('x'))).unwrap()), ["one", "two", "three"], "Ink autofocuses on mount only");
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
  let page = "import { system } from \"@snapfire/fsr-client/std\";\nimport { useEffect, useState } from \"react\";\nimport { Text } from \"ink\";\n\nexport default function Out() {\n  const [output, setOutput] = useState(\"\");\n  const [code, setCode] = useState(-1);\n  useEffect(() => {\n    const sub = system.spawn(\"echo\", [\"hi\"]);\n    sub.on(\"error\", (error) => {\n      setOutput(error.message);\n    });\n    sub.stdout.setEncoding(\"utf8\");\n    sub.stdout.on(\"data\", (chunk: string) => {\n      setOutput((o) => o + chunk);\n    });\n    sub.on(\"exit\", (exitCode) => setCode(exitCode));\n  }, [setOutput]);\n  return <Text>{output}|{code}</Text>;\n}\n";
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
  let page = "import { system } from \"@snapfire/fsr-client/std\";\nimport { useState } from \"react\";\nimport { Text, useApp, useInput } from \"ink\";\n\nconst runChild = async (command: string, args: string[]): Promise<void> =>\n  new Promise((resolve, reject) => {\n    const child = system.spawn(command, args, { stdio: \"inherit\" });\n    child.on(\"exit\", (code, signal) => {\n      if (code === 0) {\n        resolve();\n      } else {\n        reject(new Error(`exited with ${code}`));\n      }\n    });\n    child.on(\"error\", reject);\n  });\n\nexport default function Page() {\n  const { suspendTerminal } = useApp();\n  const [status, setStatus] = useState(\"ready\");\n  useInput((input) => {\n    void (async () => {\n      setStatus(\"suspended\");\n      try {\n        await suspendTerminal(async () => {\n          await runChild(\"sh\", [\"-c\", input]);\n        });\n        setStatus(\"resumed\");\n      } catch (error) {\n        setStatus(`failed: ${(error as Error).message}`);\n      }\n    })();\n  });\n  return <Text>{status}</Text>;\n}\n";
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
  let page = "import { system } from \"@snapfire/fsr-client/std\";\nimport { useState } from \"react\";\nimport { Text, useInput } from \"ink\";\n\nconst run = (command: string): Promise<void> =>\n  new Promise((resolve, reject) => {\n    const child = system.spawn(\"sh\", [\"-c\", command]);\n    child.on(\"exit\", (code) => {\n      if (code === 0) {\n        resolve();\n      } else {\n        reject(new Error(`exited with ${code}`));\n      }\n    });\n  });\n\nasync function attempt(command: string): Promise<string> {\n  let said = \"\";\n  try {\n    await run(command);\n    said = \"ok\";\n  } catch (error) {\n    said = `failed: ${(error as Error).message}`;\n  }\n  return `${command} ${said}`;\n}\n\nexport default function Page() {\n  const [status, setStatus] = useState(\"ready\");\n  useInput((input) => {\n    void (async () => {\n      const said = await attempt(input);\n      setStatus(said);\n    })();\n  });\n  return <Text>{status}</Text>;\n}\n";
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

#[test]
fn a_store_key_a_handler_writes_holds_its_value_for_the_next_step_and_every_component_reading_it() {
  let page = "import { Box, Text, useInput } from \"ink\";\nimport { useStore } from \"@snapfire/fsr-client/react\";\nimport { key, set } from \"@snapfire/fsr-client/store\";\n\nconst count = key<number>(\"demo/count\");\n\nfunction Shown() {\n  const [n] = useStore(count, 0);\n  return <Text>shown {n}</Text>;\n}\n\nexport default function Page() {\n  const [n, setN] = useStore(count, 0);\n  useInput((input) => {\n    if (input === \"+\") set(count, n + 1);\n    if (input === \"u\") setN((n: number) => n + 10);\n  });\n  return (\n    <Box flexDirection=\"column\">\n      <Text>page {n}</Text>\n      <Shown />\n    </Box>\n  );\n}\n";
  let mut session = Session::open(&app(page)).unwrap();
  let press = |c: char| {
    let Some(Press::Input(key)) = key_event(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)) else { panic!("{c} is input") };
    key
  };
  let lines = |node| snapfire_fsr_terminal::render(&node, 30).lines().iter().map(|l| l.trim_end().to_owned()).collect::<Vec<_>>();
  session.view().unwrap();
  session.press(&press('+')).unwrap();
  assert_eq!(lines(session.press(&press('+')).unwrap()), ["page 2", "shown 2"], "the second write reads what the first wrote");
  assert_eq!(lines(session.press(&press('u')).unwrap()), ["page 12", "shown 12"], "an updater starts from the value held");
  assert_eq!(lines(session.view().unwrap()), ["page 12", "shown 12"], "a render with no handler reads it too");
}

#[test]
fn a_handler_calling_an_imported_helper_that_sets_a_store_key_steps_the_page() {
  let page = "import { Text, useInput } from \"ink\";\nimport { useStore } from \"@snapfire/fsr-client/react\";\nimport { count, step } from \"../src/count\";\n\nexport default function Page() {\n  const [n] = useStore(count, 0);\n  useInput(() => step(n));\n  return <Text>{n}</Text>;\n}\n";
  let app = app(page);
  std::fs::create_dir_all(app.join("src")).unwrap();
  std::fs::write(app.join("src/count.ts"), "import { key, set } from \"@snapfire/fsr-client/store\";\n\nexport const count = key<number>(\"demo/count\");\n\nexport const step = (n: number) => set(count, n + 2);\n").unwrap();
  let mut session = Session::open(&app).unwrap();
  let Some(Press::Input(key)) = key_event(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE)) else { panic!("x is input") };
  let line = |node| snapfire_fsr_terminal::render(&node, 30).lines()[0].clone();
  session.view().unwrap();
  session.press(&key).unwrap();
  assert_eq!(line(session.press(&key).unwrap()), "4");
}

#[test]
fn a_child_handler_calling_a_function_its_parent_passed_runs_the_parents_handler_in_the_same_step() {
  let page = "import { useState } from \"react\";\nimport { Box, Text, useInput } from \"ink\";\n\nfunction Picker({ onPick }: { onPick: (n: number) => void }) {\n  useInput((input) => {\n    if (input >= \"0\" && input <= \"9\") onPick(Number(input));\n  });\n  return <Text>pick a digit</Text>;\n}\n\nexport default function Page() {\n  const [picked, setPicked] = useState(-1);\n  return (\n    <Box flexDirection=\"column\">\n      <Picker onPick={(n) => setPicked(n * 2)} />\n      <Text>picked {picked}</Text>\n    </Box>\n  );\n}\n";
  let mut session = Session::open(&app(page)).unwrap();
  let Some(Press::Input(key)) = key_event(KeyEvent::new(KeyCode::Char('4'), KeyModifiers::NONE)) else { panic!("4 is input") };
  let lines = |node| snapfire_fsr_terminal::render(&node, 30).lines().iter().map(|l| l.trim_end().to_owned()).collect::<Vec<_>>();
  assert_eq!(lines(session.view().unwrap()), ["pick a digit", "picked -1"]);
  assert_eq!(lines(session.press(&key).unwrap()), ["pick a digit", "picked 8"]);
}

#[test]
fn a_custom_hooks_effect_starts_a_timer_whose_callback_sets_the_hooks_own_state() {
  let page = "import { useEffect, useState } from \"react\";\nimport { Text, useInput } from \"ink\";\n\nconst useDebounced = <T,>(value: T, delay: number): T => {\n  const [debounced, setDebounced] = useState(value);\n  useEffect(() => {\n    const timer = setTimeout(() => {\n      setDebounced(value);\n    }, delay);\n    return () => {\n      clearTimeout(timer);\n    };\n  }, [value, delay]);\n  return debounced;\n};\n\nexport default function Page() {\n  const [query, setQuery] = useState(\"\");\n  const shown = useDebounced(query, 200);\n  useInput((input) => setQuery(query + input));\n  return <Text>{query}|{shown}</Text>;\n}\n";
  let mut session = Session::open(&app(page)).unwrap();
  let press = |c: char| {
    let Some(Press::Input(key)) = key_event(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)) else { panic!("{c} is input") };
    key
  };
  let line = |node| snapfire_fsr_terminal::render(&node, 30).lines()[0].clone();
  session.view().unwrap();
  session.take_started();
  session.press(&press('a')).unwrap();
  let first = session.take_started();
  assert_eq!(line(session.press(&press('b')).unwrap()), "ab|");
  let second = session.take_started();
  assert_eq!(line(session.fire(&first[0]).unwrap()), "ab|", "the cleanup cleared the timer the first key started");
  assert_eq!(line(session.fire(&second[0]).unwrap()), "ab|ab");
}

#[test]
fn an_effect_returning_undefined_early_has_no_cleanup_and_the_rest_runs_when_it_does_not() {
  let page = "import { useEffect, useState } from \"react\";\nimport { Text, useInput } from \"ink\";\n\nexport default function Page() {\n  const [query, setQuery] = useState(\"\");\n  const [runs, setRuns] = useState(0);\n  useEffect(() => {\n    if (query.length <= 1) {\n      return undefined;\n    }\n    setRuns((n) => n + 1);\n  }, [query]);\n  useInput((input) => setQuery(query + input));\n  return <Text>{query} {runs}</Text>;\n}\n";
  let mut session = Session::open(&app(page)).unwrap();
  let press = |c: char| {
    let Some(Press::Input(key)) = key_event(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)) else { panic!("{c} is input") };
    key
  };
  let line = |node| snapfire_fsr_terminal::render(&node, 30).lines()[0].clone();
  assert_eq!(line(session.press(&press('a')).unwrap()), "a 0");
  assert_eq!(line(session.press(&press('b')).unwrap()), "ab 1");
}

#[test]
fn a_resize_runs_the_listeners_stdout_on_added_and_off_removes_the_one_an_effect_cleanup_names() {
  let page = "import { useEffect, useState } from \"react\";\nimport { Text, useStdout } from \"ink\";\nimport { useStdoutDimensions } from \"../src/use-stdout-dimensions\";\n\nexport default function Sized() {\n  const [columns, rows] = useStdoutDimensions();\n  const [heard, setHeard] = useState(0);\n  const { stdout } = useStdout();\n  useEffect(() => {\n    const count = () => setHeard((n) => n + 1);\n    stdout.on(\"resize\", count);\n    return () => stdout.off(\"resize\", count);\n  }, [stdout]);\n  return <Text>{columns}x{rows} heard {heard}</Text>;\n}\n";
  let app = app(page);
  std::fs::create_dir_all(app.join("src")).unwrap();
  std::fs::write(app.join("src/use-stdout-dimensions.ts"), "import { useEffect, useState } from \"react\";\nimport { useStdout } from \"ink\";\n\nexport function useStdoutDimensions(): [number, number] {\n  const { stdout } = useStdout();\n  const [dimensions, setDimensions] = useState<[number, number]>([\n    stdout.columns,\n    stdout.rows,\n  ]);\n\n  useEffect(() => {\n    const handler = () => setDimensions([stdout.columns, stdout.rows]);\n    stdout.on(\"resize\", handler);\n    return () => {\n      stdout.off(\"resize\", handler);\n    };\n  }, [stdout]);\n\n  return dimensions;\n}\n").unwrap();
  let mut session = Session::open(&app).unwrap();
  let line = |node| snapfire_fsr_terminal::render(&node, 30).lines()[0].clone();
  assert_eq!(line(session.view().unwrap()), "80x24 heard 0");
  session.resize(120, 40);
  assert_eq!(line(session.view().unwrap()), "120x40 heard 1");
  session.resize(90, 30);
  assert_eq!(line(session.view().unwrap()), "90x30 heard 2", "the listener the first run added is gone");
}

#[test]
fn ink_spinner_steps_through_the_frames_cli_spinners_names_at_its_interval() {
  use std::time::Duration;
  let page = "import { Text } from \"ink\";\nimport Spinner from \"ink-spinner\";\nimport spinners from \"cli-spinners\";\n\nexport default function Waiting() {\n  return (\n    <Text>\n      <Spinner type=\"line\" /> <Spinner /> {spinners.line.frames.length}\n    </Text>\n  );\n}\n";
  let mut session = Session::open(&app(page)).unwrap();
  let line = |node| snapfire_fsr_terminal::render(&node, 20).lines()[0].clone();
  assert_eq!(line(session.view().unwrap()), "- \u{280b} 4");
  let timers = session.take_started();
  assert_eq!(timers.iter().map(|t| t.every).collect::<Vec<_>>(), [Duration::from_millis(130), Duration::from_millis(80)]);
  session.fire(&timers[0]).unwrap();
  assert_eq!(line(session.fire(&timers[1]).unwrap()), "\\ \u{2819} 4");
}

#[test]
fn a_large_constant_another_module_exports_reads_in_the_terminal() {
  let page = "import { Text } from \"ink\";\nimport WORDS from \"../src/words\";\n\nexport default function Word() {\n  return <Text>{WORDS[3]} of {WORDS.length}</Text>;\n}\n";
  let app = app(page);
  std::fs::create_dir_all(app.join("src")).unwrap();
  std::fs::write(app.join("src/words.ts"), "const WORDS = [\"alpha\", \"bravo\", \"charlie\", \"delta\", \"echo\", \"foxtrot\", \"golf\", \"hotel\", \"india\", \"juliet\", \"kilo\", \"lima\", \"mike\", \"november\", \"oscar\", \"papa\", \"quebec\", \"romeo\", \"sierra\", \"tango\", \"uniform\", \"victor\", \"whiskey\", \"xray\", \"yankee\", \"zulu\", \"one\", \"two\", \"three\", \"four\", \"five\", \"six\", \"seven\", \"eight\", \"nine\", \"ten\", \"eleven\", \"twelve\", \"thirteen\", \"fourteen\"];\n\nexport default WORDS;\n").unwrap();
  let mut session = Session::open(&app).unwrap();
  assert_eq!(snapfire_fsr_terminal::render(&session.view().unwrap(), 20).lines()[0], "delta of 40");
}

#[test]
fn use_input_runs_only_while_its_is_active_option_holds() {
  let page = "import { useState } from \"react\";\nimport { Box, Text, useInput } from \"ink\";\n\nfunction Field({ name, active }: { name: string; active: boolean }) {\n  const [value, setValue] = useState(\"\");\n  useInput((input) => setValue(value + input), { isActive: active });\n  return <Text>{name}:{value}</Text>;\n}\n\nexport default function Fields() {\n  return (\n    <Box flexDirection=\"column\">\n      <Field name=\"a\" active={true} />\n      <Field name=\"b\" active={false} />\n    </Box>\n  );\n}\n";
  let mut session = Session::open(&app(page)).unwrap();
  let Some(Press::Input(x)) = key_event(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE)) else { panic!("x is input") };
  let lines = |node| snapfire_fsr_terminal::render(&node, 10).lines()[..2].to_vec();
  assert_eq!(lines(session.view().unwrap()), ["a:", "b:"]);
  assert_eq!(lines(session.press(&x).unwrap()), ["a:x", "b:"]);
}

#[test]
fn a_let_declared_without_a_value_is_undefined() {
  let page = "import { Text } from \"ink\";\n\nlet described: string | undefined;\n\nconst describe = (): string => {\n  if (described !== undefined) {\n    return described;\n  }\n  described = \"set\";\n  return \"unset\";\n};\n\nexport default function Page() {\n  let local: number | undefined;\n  return <Text>{describe()} {local === undefined ? \"undefined\" : \"null\"}</Text>;\n}\n";
  let mut session = Session::open(&app(page)).unwrap();
  assert_eq!(snapfire_fsr_terminal::render(&session.view().unwrap(), 20).lines()[0], "unset undefined");
}

#[test]
fn ink_text_input_edits_its_value_at_the_cursor_and_submits_it() {
  let page = "import { useState } from \"react\";\nimport { Box, Text } from \"ink\";\nimport TextInput, { UncontrolledTextInput } from \"ink-text-input\";\n\nexport default function Ask() {\n  const [query, setQuery] = useState(\"\");\n  const [sent, setSent] = useState(\"\");\n  return (\n    <Box flexDirection=\"column\">\n      <TextInput value={query} onChange={setQuery} onSubmit={setSent} placeholder=\"Search\" />\n      <Text>sent {sent}</Text>\n      <UncontrolledTextInput focus={false} initialValue=\"kept\" />\n    </Box>\n  );\n}\n";
  let mut session = Session::open(&app(page)).unwrap();
  let press = |code: KeyCode| {
    let Some(Press::Input(press)) = key_event(KeyEvent::new(code, KeyModifiers::NONE)) else { panic!("{code:?} is input") };
    press
  };
  let lines = |node| snapfire_fsr_terminal::render(&node, 12).lines()[..3].iter().map(|l| l.trim_end().to_owned()).collect::<Vec<_>>();
  assert_eq!(lines(session.view().unwrap()), ["Search", "sent", "kept"]);
  session.press(&press(KeyCode::Char('h'))).unwrap();
  assert_eq!(lines(session.press(&press(KeyCode::Char('i'))).unwrap()), ["hi", "sent", "kept"]);
  session.press(&press(KeyCode::Left)).unwrap();
  assert_eq!(lines(session.press(&press(KeyCode::Char('x'))).unwrap()), ["hxi", "sent", "kept"]);
  assert_eq!(lines(session.press(&press(KeyCode::Enter)).unwrap()), ["hxi", "sent hxi", "kept"]);
}

#[test]
fn a_handler_writes_its_own_let_in_a_branch_and_after_it() {
  let page = "import { useState } from \"react\";\nimport { Text, useInput } from \"ink\";\n\nexport default function Count() {\n  const [count, setCount] = useState(1);\n  useInput((input) => {\n    let next = count;\n    if (input === \"+\") {\n      next += 2;\n    } else {\n      next--;\n    }\n    next = next * 10;\n    setCount(next);\n  });\n  return <Text>{count}</Text>;\n}\n";
  let mut session = Session::open(&app(page)).unwrap();
  let press = |c: char| {
    let Some(Press::Input(press)) = key_event(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)) else { panic!("{c} is input") };
    press
  };
  let line = |node| snapfire_fsr_terminal::render(&node, 10).lines()[0].trim_end().to_owned();
  assert_eq!(line(session.view().unwrap()), "1");
  assert_eq!(line(session.press(&press('+')).unwrap()), "30");
  assert_eq!(line(session.press(&press('-')).unwrap()), "290");
}

#[test]
fn a_handler_reads_a_const_the_component_declares_below_its_use_input() {
  let page = "import { useState } from \"react\";\nimport { Text, useInput } from \"ink\";\n\nexport default function Count() {\n  const [count, setCount] = useState(1);\n  useInput(() => {\n    setCount(shown + 1);\n  });\n  const shown = count * 2;\n  return <Text>{shown}</Text>;\n}\n";
  let mut session = Session::open(&app(page)).unwrap();
  let Some(Press::Input(x)) = key_event(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE)) else { panic!("x is input") };
  let line = |node| snapfire_fsr_terminal::render(&node, 10).lines()[0].trim_end().to_owned();
  assert_eq!(line(session.view().unwrap()), "2");
  assert_eq!(line(session.press(&x).unwrap()), "6");
}

#[test]
fn a_state_setter_passed_as_a_prop_sets_the_parents_state_when_the_child_calls_it() {
  let page = "import { useState } from \"react\";\nimport { Box, Text, useInput } from \"ink\";\n\nfunction Typer({ value, onChange }: { value: string; onChange: (value: string) => void }) {\n  useInput((input) => onChange(value + input));\n  return <Text>typed {value}</Text>;\n}\n\nexport default function Page() {\n  const [query, setQuery] = useState(\"\");\n  return (\n    <Box flexDirection=\"column\">\n      <Typer value={query} onChange={setQuery} />\n      <Text>query {query}</Text>\n    </Box>\n  );\n}\n";
  let mut session = Session::open(&app(page)).unwrap();
  let Some(Press::Input(h)) = key_event(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE)) else { panic!("h is input") };
  let lines = |node| snapfire_fsr_terminal::render(&node, 12).lines()[..2].iter().map(|l| l.trim_end().to_owned()).collect::<Vec<_>>();
  assert_eq!(lines(session.view().unwrap()), ["typed", "query"]);
  assert_eq!(lines(session.press(&h).unwrap()), ["typed h", "query h"]);
}

#[test]
fn an_effect_calls_an_async_function_it_declares_and_sets_state_when_it_settles() {
  let page = "import { useEffect, useState } from \"react\";\nimport { Text } from \"ink\";\n\nconst lookup = async (query: string) => query.toUpperCase();\n\nexport default function Found() {\n  const [query] = useState(\"ab\");\n  const [found, setFound] = useState(\"none\");\n  useEffect(() => {\n    let isCanceled = false;\n    const run = async () => {\n      const value = await lookup(query);\n      if (!isCanceled) {\n        setFound(value);\n      }\n    };\n    run();\n    return () => {\n      isCanceled = true;\n    };\n  }, [query]);\n  return <Text>{found}</Text>;\n}\n";
  let mut session = Session::open(&app(page)).unwrap();
  let line = |node| snapfire_fsr_terminal::render(&node, 10).lines()[0].trim_end().to_owned();
  session.view().unwrap();
  assert_eq!(line(session.view().unwrap()), "AB");
}

#[test]
fn a_function_a_handler_calls_takes_the_arguments_of_the_call_and_its_defaults() {
  let page = "import { useCallback, useState } from \"react\";\nimport { Text, useInput } from \"ink\";\n\nexport default function Log() {\n  const [log, setLog] = useState(\"\");\n  const add = useCallback((word: string, twice = false) => {\n    setLog(log + (twice ? word + word : word));\n  }, [log]);\n  useInput((input) => {\n    if (input === \"a\") {\n      add(\"x\");\n    } else {\n      add(\"y\", true);\n    }\n  });\n  return <Text>log {log}</Text>;\n}\n";
  let mut session = Session::open(&app(page)).unwrap();
  let press = |c: char| {
    let Some(Press::Input(press)) = key_event(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)) else { panic!("{c} is input") };
    press
  };
  let line = |node| snapfire_fsr_terminal::render(&node, 12).lines()[0].trim_end().to_owned();
  session.view().unwrap();
  assert_eq!(line(session.press(&press('a')).unwrap()), "log x");
  assert_eq!(line(session.press(&press('b')).unwrap()), "log xyy");
}

#[test]
fn a_handler_prop_a_component_hands_on_to_the_one_it_places_reaches_the_parents_handler() {
  let page = "import { useCallback, useState } from \"react\";\nimport { Box, Text, useInput } from \"ink\";\n\nfunction Keys({ onKey }: { onKey: (key: string) => void }) {\n  useInput((input) => onKey(input));\n  return <Text>keys</Text>;\n}\n\nfunction Middle({ onKey }: { onKey: (key: string) => void }) {\n  return <Keys onKey={onKey} />;\n}\n\nexport default function Page() {\n  const [last, setLast] = useState(\"none\");\n  const heard = useCallback((key: string) => {\n    setLast(key);\n  }, []);\n  return (\n    <Box flexDirection=\"column\">\n      <Middle onKey={heard} />\n      <Text>last {last}</Text>\n    </Box>\n  );\n}\n";
  let mut session = Session::open(&app(page)).unwrap();
  let Some(Press::Input(q)) = key_event(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)) else { panic!("q is input") };
  let lines = |node| snapfire_fsr_terminal::render(&node, 12).lines()[..2].iter().map(|l| l.trim_end().to_owned()).collect::<Vec<_>>();
  assert_eq!(lines(session.view().unwrap()), ["keys", "last none"]);
  assert_eq!(lines(session.press(&q).unwrap()), ["keys", "last q"]);
}

#[test]
fn a_mount_timer_whose_delay_is_computed_starts_when_the_effect_runs() {
  let page = "import { useEffect, useState } from \"react\";\nimport { Text } from \"ink\";\n\nconst speed = { ms: 80 };\n\nexport default function Ticks() {\n  const [n, setN] = useState(0);\n  const [named, setNamed] = useState(0);\n  useEffect(() => {\n    const timer = setInterval(() => setN((n) => n + 1), speed.ms);\n    return () => clearInterval(timer);\n  }, []);\n  useEffect(() => {\n    const tick = () => setNamed((n) => n + 1);\n    const timer = setInterval(tick, speed.ms * 2);\n    return () => clearInterval(timer);\n  }, []);\n  return <Text>{n} {named}</Text>;\n}\n";
  let mut session = Session::open(&app(page)).unwrap();
  let line = |node| snapfire_fsr_terminal::render(&node, 10).lines()[0].trim_end().to_owned();
  assert_eq!(line(session.view().unwrap()), "0 0");
  let started = session.take_started();
  assert_eq!(started.iter().map(|t| (t.every, t.repeats)).collect::<Vec<_>>(), [(std::time::Duration::from_millis(80), true), (std::time::Duration::from_millis(160), true)], "{started:?}");
  assert_eq!(line(session.fire(&started[0]).unwrap()), "1 0");
  assert_eq!(line(session.fire(&started[1]).unwrap()), "1 1");
  assert_eq!(line(session.fire(&started[0]).unwrap()), "2 1");
}

fn host_dir(name: &str) -> std::path::PathBuf {
  let dir = std::env::temp_dir().join(format!("fsr-terminal-host-{}-{name}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  std::fs::create_dir_all(&dir).unwrap();
  dir
}

#[test]
fn a_page_reads_a_file_and_the_paths_around_it() {
  let dir = host_dir("read");
  std::fs::write(dir.join("cards.txt"), "ace\nking\n").unwrap();
  unsafe { std::env::set_var("FSR_TEST_DIR", &dir); }
  let page = "import { Text, Box } from \"ink\";\nimport { file, path, system } from \"@snapfire/fsr-client/std\";\n\nexport default function Cards() {\n  const dir = system.env(\"FSR_TEST_DIR\");\n  const at = path.join(dir, \"cards.txt\");\n  const cards = file.read(at).trim().split(\"\\n\");\n  return (\n    <Box flexDirection=\"column\">\n      <Text>{cards.length} cards from {path.basename(at)} {path.extname(at)}</Text>\n      <Text>{file.exists(path.join(dir, \"missing.txt\")) ? \"has\" : \"no\"} missing, {file.list(dir).join(\",\")}</Text>\n      <Text>{system.home().length > 0 ? \"home\" : \"nowhere\"} {system.cwd().length > 0 ? \"cwd\" : \"nowhere\"} {system.platform()}</Text>\n    </Box>\n  );\n}\n";
  let mut session = Session::open(&app(page)).unwrap();
  let lines = |node| snapfire_fsr_terminal::render(&node, 60).lines()[..3].iter().map(|l| l.trim_end().to_owned()).collect::<Vec<_>>();
  assert_eq!(lines(session.view().unwrap()), ["2 cards from cards.txt .txt".to_owned(), "no missing, cards.txt".to_owned(), format!("home cwd {}", std::env::consts::OS)]);
}

#[test]
fn a_page_joins_lines_with_the_systems_line_end() {
  let page = "import { Text } from \"ink\";\nimport { system } from \"@snapfire/fsr-client/std\";\n\nexport default function Lines() {\n  const text = [\"a\", \"b\"].join(system.lineEnd()) + system.lineEnd();\n  return <Text>{JSON.stringify(text)}</Text>;\n}\n";
  let mut session = Session::open(&app(page)).unwrap();
  let line = |node| snapfire_fsr_terminal::render(&node, 20).lines()[0].trim_end().to_owned();
  let eol = if cfg!(windows) { "\\r\\n" } else { "\\n" };
  assert_eq!(line(session.view().unwrap()), format!("\"a{eol}b{eol}\""));
}

#[test]
fn a_page_that_imports_a_node_module_is_refused_naming_fsrs() {
  let page = "import { Text } from \"ink\";\nimport { readFileSync } from \"node:fs\";\n\nexport default function Read() {\n  return <Text>{readFileSync(\"cards.txt\", \"utf8\")}</Text>;\n}\n";
  let refused = Session::open(&app(page)).err().expect("refused");
  assert!(refused.contains("`readFileSync` from `node:fs`, which is not FSR's") && refused.contains("`file`, `path` and `system`"), "{refused}");
}

#[test]
fn a_page_that_reads_process_is_refused_naming_fsrs() {
  let page = "import { Text } from \"ink\";\n\nexport default function Home() {\n  return <Text>{process.env.HOME}</Text>;\n}\n";
  let refused = Session::open(&app(page)).err().expect("refused");
  assert!(refused.contains("`process`, which is not FSR's") && refused.contains("fsr migrate"), "{refused}");
}

#[test]
fn a_module_constant_that_reads_a_file_is_read_once_a_session() {
  let dir = host_dir("once");
  let conf = dir.join("conf.json");
  std::fs::write(&conf, "{ \"n\": 1 }").unwrap();
  unsafe { std::env::set_var("FSR_TEST_CONF", &conf); }
  let page = "import { useState } from \"react\";\nimport { Text, useInput } from \"ink\";\nimport { file, system } from \"@snapfire/fsr-client/std\";\n\nconst config = JSON.parse(file.read(system.env(\"FSR_TEST_CONF\")));\n\nexport default function Conf() {\n  const [presses, setPresses] = useState(0);\n  useInput(() => setPresses((n) => n + 1));\n  return <Text>n {config.n} after {presses}</Text>;\n}\n";
  let mut session = Session::open(&app(page)).unwrap();
  let line = |node| snapfire_fsr_terminal::render(&node, 20).lines()[0].trim_end().to_owned();
  assert_eq!(line(session.view().unwrap()), "n 1 after 0");
  std::fs::write(&conf, "{ \"n\": 2 }").unwrap();
  let Some(Press::Input(x)) = key_event(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE)) else { panic!("x is input") };
  assert_eq!(line(session.press(&x).unwrap()), "n 1 after 1", "the module ran once, when the session opened");
}

#[test]
fn an_effect_awaits_a_file_read() {
  let dir = host_dir("promises");
  std::fs::write(dir.join("hello.txt"), "hello").unwrap();
  unsafe { std::env::set_var("FSR_TEST_HELLO", dir.join("hello.txt")); }
  let page = "import { useEffect, useState } from \"react\";\nimport { Text } from \"ink\";\nimport { file, system } from \"@snapfire/fsr-client/std\";\n\nexport default function Hello() {\n  const [text, setText] = useState(\"loading\");\n  useEffect(() => {\n    const run = async () => {\n      const read = await file.read(system.env(\"FSR_TEST_HELLO\"));\n      setText(`loaded: ${read}`);\n    };\n    run();\n  }, []);\n  return <Text>{text}</Text>;\n}\n";
  let mut session = Session::open(&app(page)).unwrap();
  let line = |node| snapfire_fsr_terminal::render(&node, 20).lines()[0].trim_end().to_owned();
  session.view().unwrap();
  assert_eq!(line(session.view().unwrap()), "loaded: hello");
}

#[test]
fn a_page_stats_a_file_and_catches_enoent_for_a_missing_one() {
  let dir = host_dir("stat");
  std::fs::write(dir.join("five.txt"), "hello").unwrap();
  unsafe { std::env::set_var("FSR_TEST_STAT_DIR", &dir); }
  let page = "import { useEffect, useState } from \"react\";\nimport { Text } from \"ink\";\nimport { file, system } from \"@snapfire/fsr-client/std\";\n\nexport default function Stat() {\n  const [text, setText] = useState(\"checking\");\n  useEffect(() => {\n    const run = async () => {\n      const s = await file.stat(`${system.env(\"FSR_TEST_STAT_DIR\")}/five.txt`);\n      try {\n        await file.stat(`${system.env(\"FSR_TEST_STAT_DIR\")}/missing.txt`);\n        setText(\"found\");\n      } catch (error) {\n        setText(`${s.size} ${s.isFile ? \"file\" : \"other\"} ${error.code}`);\n      }\n    };\n    run();\n  }, []);\n  const d = file.stat(system.env(\"FSR_TEST_STAT_DIR\"));\n  return <Text>{d.isDirectory ? \"dir\" : \"not\"} {text}</Text>;\n}\n";
  let mut session = Session::open(&app(page)).unwrap();
  let line = |node| snapfire_fsr_terminal::render(&node, 30).lines()[0].trim_end().to_owned();
  session.view().unwrap();
  assert_eq!(line(session.view().unwrap()), "dir 5 file ENOENT");
}

#[test]
fn an_await_of_a_value_that_is_not_a_promise_continues_with_it() {
  let page = "import { useEffect, useState } from \"react\";\nimport { Text } from \"ink\";\n\nconst five = () => 5;\n\nexport default function Five() {\n  const [n, setN] = useState(0);\n  useEffect(() => {\n    const run = async () => {\n      const v = await five();\n      setN(v + 1);\n    };\n    run();\n  }, []);\n  return <Text>{n}</Text>;\n}\n";
  let mut session = Session::open(&app(page)).unwrap();
  let line = |node| snapfire_fsr_terminal::render(&node, 10).lines()[0].trim_end().to_owned();
  session.view().unwrap();
  assert_eq!(line(session.view().unwrap()), "6");
}

#[test]
fn exit_in_a_handler_ends_the_session_with_its_code() {
  let page = "import { Text, useApp, useInput } from \"ink\";\n\nexport default function Quits() {\n  const { exit } = useApp();\n  useInput((input) => {\n    if (input === \"q\") exit(3);\n  });\n  return <Text>running</Text>;\n}\n";
  let mut session = Session::open(&app(page)).unwrap();
  session.view().unwrap();
  let Some(Press::Input(x)) = key_event(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE)) else { panic!("x is input") };
  session.press(&x).unwrap();
  assert!(!session.exited());
  let Some(Press::Input(q)) = key_event(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)) else { panic!("q is input") };
  session.press(&q).unwrap();
  assert!(session.exited());
  assert_eq!(session.exit_code(), Some(3));
}

#[test]
fn stdout_columns_and_rows_are_the_terminals_size() {
  let page = "import { Text, useStdout } from \"ink\";\n\nexport default function Size() {\n  const { stdout } = useStdout();\n  return <Text>{stdout.columns}x{stdout.rows}</Text>;\n}\n";
  let mut session = Session::open(&app(page)).unwrap();
  let line = |node| snapfire_fsr_terminal::render(&node, 20).lines()[0].trim_end().to_owned();
  assert_eq!(line(session.view().unwrap()), "80x24");
  session.resize(100, 30);
  assert_eq!(line(session.view().unwrap()), "100x30");
}

#[test]
fn a_resize_runs_the_listener_stdout_on_added_with_a_function_the_effect_declares() {
  let page = "import { useEffect, useState } from \"react\";\nimport { Text, useStdout } from \"ink\";\n\nexport default function Sized() {\n  const { stdout } = useStdout();\n  const [size, setSize] = useState({ columns: stdout.columns || 80, rows: stdout.rows || 24 });\n  useEffect(() => {\n    function updateSize() {\n      setSize({ columns: stdout.columns || 80, rows: stdout.rows || 24 });\n    }\n    stdout.on(\"resize\", updateSize);\n    return () => {\n      stdout.off(\"resize\", updateSize);\n    };\n  }, []);\n  return <Text>{size.columns}x{size.rows}</Text>;\n}\n";
  let mut session = Session::open(&app(page)).unwrap();
  let line = |node| snapfire_fsr_terminal::render(&node, 30).lines()[0].clone();
  assert_eq!(line(session.view().unwrap()), "80x24");
  session.resize(120, 40);
  assert_eq!(line(session.view().unwrap()), "120x40");
}

#[test]
fn a_resize_runs_the_listener_a_custom_hook_adds_on_use_stdout_with_a_template_event_name() {
  let page = "import { useEffect, useState } from \"react\";\nimport { Text, useStdout } from \"ink\";\n\nfunction useScreenSize() {\n  const { stdout } = useStdout() as any;\n  const [size, setSize] = useState(() => ({ width: stdout.columns, height: stdout.rows }));\n  useEffect(() => {\n    const onResize = () => setSize({ width: stdout.columns, height: stdout.rows });\n    stdout.on(`resize`, onResize);\n    return () => void stdout.off(`resize`, onResize);\n  }, [stdout]);\n  return size;\n}\n\nexport default function Sized() {\n  const { width, height } = useScreenSize();\n  return <Text>{width}x{height}</Text>;\n}\n";
  let mut session = Session::open(&app(page)).unwrap();
  let line = |node| snapfire_fsr_terminal::render(&node, 30).lines()[0].clone();
  assert_eq!(line(session.view().unwrap()), "80x24");
  session.resize(120, 40);
  assert_eq!(line(session.view().unwrap()), "120x40");
}

#[test]
fn a_handler_calls_an_async_module_function_as_a_statement() {
  let dir = host_dir("save");
  unsafe { std::env::set_var("FSR_TEST_SAVE", dir.join("saved.txt")); }
  let page = "import { useState } from \"react\";\nimport { Text, useInput } from \"ink\";\nimport { save } from \"./save\";\n\nexport default function Saves() {\n  const [last, setLast] = useState(\"none\");\n  useInput((input) => {\n    setLast(input);\n    save(input);\n  });\n  return <Text>{last}</Text>;\n}\n";
  let store = "import { file, system } from \"@snapfire/fsr-client/std\";\n\nlet saves = 0;\n\nexport const save = async (text: string) => {\n  saves += 1;\n  await file.write(system.env(\"FSR_TEST_SAVE\"), `${text} ${saves}`);\n};\n";
  let app = app(page);
  std::fs::write(app.join("routes/save.ts"), store).unwrap();
  let mut session = Session::open(&app).unwrap();
  session.view().unwrap();
  for c in ['x', 'y'] {
    let Some(Press::Input(press)) = key_event(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)) else { panic!("{c} is input") };
    session.press(&press).unwrap();
  }
  assert_eq!(std::fs::read_to_string(dir.join("saved.txt")).unwrap(), "y 2");
}

#[test]
fn a_handler_calls_an_async_module_function_whose_only_effect_is_an_act() {
  let dir = host_dir("act-only");
  unsafe { std::env::set_var("FSR_TEST_ACT_ONLY", dir.join("saved.txt")); }
  let page = "import { useState } from \"react\";\nimport { Text, useInput } from \"ink\";\nimport { update } from \"./save\";\n\nexport default function Saves() {\n  const [last, setLast] = useState(\"none\");\n  useInput((input) => {\n    setLast(input);\n    update((c) => `${c}${input}`);\n  });\n  return <Text>{last}</Text>;\n}\n";
  let store = "import { file, system } from \"@snapfire/fsr-client/std\";\n\nexport const update = async (fn: (c: string) => string) => {\n  const next = fn(\"a\");\n  await file.write(system.env(\"FSR_TEST_ACT_ONLY\"), next);\n};\n";
  let app = app(page);
  std::fs::write(app.join("routes/save.ts"), store).unwrap();
  let mut session = Session::open(&app).unwrap();
  session.view().unwrap();
  let Some(Press::Input(x)) = key_event(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE)) else { panic!("x is input") };
  session.press(&x).unwrap();
  assert_eq!(std::fs::read_to_string(dir.join("saved.txt")).unwrap(), "ax");
}

#[test]
fn a_callback_that_changes_the_object_it_is_passed_changes_the_callers_object() {
  let dir = host_dir("write-back");
  unsafe { std::env::set_var("FSR_TEST_WRITE_BACK", dir.join("config.json")); }
  let page = "import { useState } from \"react\";\nimport { Text, useInput } from \"ink\";\nimport { update } from \"./config\";\n\nexport default function Settings() {\n  const [last, setLast] = useState(\"none\");\n  useInput((input) => {\n    setLast(input);\n    update((cfg) => {\n      if (input === \"q\") return;\n      cfg.n = cfg.n + 1;\n      cfg.name = input;\n    });\n  });\n  return <Text>{last}</Text>;\n}\n";
  let config = "import { file, system } from \"@snapfire/fsr-client/std\";\n\nlet config = { n: 0, name: \"none\" };\n\nexport const update = async (fn: (c: { n: number; name: string }) => void) => {\n  fn(config);\n  await file.write(system.env(\"FSR_TEST_WRITE_BACK\"), JSON.stringify(config));\n};\n";
  let app = app(page);
  std::fs::write(app.join("routes/config.ts"), config).unwrap();
  let mut session = Session::open(&app).unwrap();
  session.view().unwrap();
  for c in ['x', 'q', 'y'] {
    let Some(Press::Input(press)) = key_event(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)) else { panic!("{c} is input") };
    session.press(&press).unwrap();
  }
  assert_eq!(std::fs::read_to_string(dir.join("config.json")).unwrap(), "{\"n\":2,\"name\":\"y\"}");
}

#[test]
fn a_function_that_replaces_a_parameter_it_also_changes_is_refused() {
  let page = "import { useState } from \"react\";\nimport { Text, useInput } from \"ink\";\nimport { update } from \"./config\";\n\nexport default function Settings() {\n  const [last, setLast] = useState(\"none\");\n  useInput((input) => {\n    setLast(input);\n    update((cfg) => {\n      cfg = { n: 9 };\n      cfg.n = 1;\n    });\n  });\n  return <Text>{last}</Text>;\n}\n";
  let config = "let config = { n: 0 };\n\nexport const update = (fn: (c: { n: number }) => void) => {\n  fn(config);\n};\n";
  let app = app(page);
  std::fs::write(app.join("routes/config.ts"), config).unwrap();
  let refused = Session::open(&app).err().expect("refused");
  assert!(refused.contains("`cfg`"), "{refused}");
}

fn app_with_loader(page: &str, loader: &str) -> std::path::PathBuf {
  let app = app(page);
  std::fs::write(app.join("routes/page.loader.ts"), loader).unwrap();
  app
}

#[test]
fn the_root_page_takes_what_its_loader_returns_as_its_props() {
  let page = "import { Text } from \"ink\";\n\nexport default function Hello({ name }: { name: string }) {\n  return <Text>hello {name}</Text>;\n}\n";
  let loader = "export async function load() {\n  return { name: \"world\" };\n}\n";
  let mut session = Session::open(&app_with_loader(page, loader)).unwrap();
  let node = session.view().unwrap();
  assert_eq!(snapfire_fsr_terminal::render(&node, 30).lines()[0], "hello world");
}

#[test]
fn the_command_line_after_the_dashes_is_the_loaders_params() {
  let args: Vec<String> = ["--name", "norm", "cards.txt", "--yes", "--limit=3", "extra"].iter().map(|s| s.to_string()).collect();
  let params = snapfire_fsr_cli::terminal::params(&args);
  assert_eq!(params.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect::<Vec<_>>(), [("name", "norm"), ("0", "cards.txt"), ("yes", "true"), ("limit", "3"), ("1", "extra")]);
  let page = "import { Text } from \"ink\";\n\nexport default function Hello({ name, file }: { name: string; file: string }) {\n  return <Text>hello {name} from {file}</Text>;\n}\n";
  let loader = "export async function load({ params }: { params: Record<string, string> }) {\n  return { name: params.name ?? \"world\", file: params[\"0\"] ?? \"nothing\" };\n}\n";
  let mut session = Session::open_with(&app_with_loader(page, loader), params).unwrap();
  let node = session.view().unwrap();
  assert_eq!(snapfire_fsr_terminal::render(&node, 40).lines()[0], "hello norm from cards.txt");
}

#[test]
fn node_crypto_members_run_in_a_terminal_loader() {
  let page = "import { Text } from \"ink\";\n\nexport default function Ids({ id, hex, digest, b64 }: { id: string; hex: string; digest: string; b64: string }) {\n  return <Text>{id.length} {hex.length} {digest} {b64}</Text>;\n}\n";
  let loader = "import { crypto } from \"@snapfire/fsr-client/std\";\n\nexport async function load() {\n  const digest = crypto.digest(\"sha256\", \"hex\", \"ab\", \"c\");\n  const b64 = crypto.digest(\"sha256\", \"base64\", \"abc\");\n  return { id: crypto.uuid(), hex: crypto.random(4), digest, b64 };\n}\n";
  let mut session = Session::open(&app_with_loader(page, loader)).unwrap();
  let node = session.view().unwrap();
  assert_eq!(snapfire_fsr_terminal::render(&node, 120).lines()[0], "36 8 ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad ungWv48Bz+pBQUDeXa4iI7ADYaOWF3qctBD/YfIAFa0=");
}

#[test]
fn performance_now_reads_a_clock_that_advances_within_a_loader() {
  let page = "import { Text } from \"ink\";\n\nexport default function Took({ ok }: { ok: string }) {\n  return <Text>{ok}</Text>;\n}\n";
  let loader = "export async function load() {\n  const started = performance.now();\n  const later = performance.now();\n  return { ok: started >= 0 && later >= started && later < 3_600_000 ? \"yes\" : \"no\" };\n}\n";
  let mut session = Session::open(&app_with_loader(page, loader)).unwrap();
  let node = session.view().unwrap();
  assert_eq!(snapfire_fsr_terminal::render(&node, 20).lines()[0], "yes");
}

#[test]
fn a_page_reads_its_resident_memory() {
  let page = "import { Text } from \"ink\";\nimport { system } from \"@snapfire/fsr-client/std\";\n\nexport default function Memory() {\n  return <Text>{system.memory() > 0 ? \"resident\" : \"none\"}</Text>;\n}\n";
  let mut session = Session::open(&app(page)).unwrap();
  let node = session.view().unwrap();
  assert_eq!(snapfire_fsr_terminal::render(&node, 30).lines()[0].trim_end(), "resident");
}

#[test]
fn a_mount_effect_branches_before_it_sets_state() {
  let page = "import { useEffect, useState } from \"react\";\nimport { Text } from \"ink\";\n\nconst startup = { wasUpdated: true, oldVersion: \"1.0\" };\n\nexport default function Note() {\n  const [note, setNote] = useState<string | null>(null);\n  const [applied, setApplied] = useState(true);\n  useEffect(() => {\n    if (startup.wasUpdated && startup.oldVersion) {\n      setNote(`updated from ${startup.oldVersion}`);\n      setApplied(false);\n    }\n  }, []);\n  return <Text>{note ?? \"fresh\"} {applied ? \"applied\" : \"pending\"}</Text>;\n}\n";
  let mut session = Session::open(&app(page)).unwrap();
  let node = session.view().unwrap();
  assert_eq!(snapfire_fsr_terminal::render(&node, 40).lines()[0].trim_end(), "updated from 1.0 pending");
}

#[test]
fn a_module_let_takes_an_awaited_value_by_assignment_in_an_async_function() {
  let page = "import { useEffect, useState } from \"react\";\nimport { Text } from \"ink\";\n\nlet cached: string | null = null;\nconst lookup = async (query: string) => query.toUpperCase();\nconst ensure = async (query: string) => {\n  if (!cached) {\n    cached = await lookup(query);\n  }\n  return cached;\n};\n\nexport default function Found() {\n  const [found, setFound] = useState(\"none\");\n  useEffect(() => {\n    const run = async () => {\n      const value = await ensure(\"ab\");\n      setFound(value);\n    };\n    run();\n  }, []);\n  return <Text>{found}</Text>;\n}\n";
  let mut session = Session::open(&app(page)).unwrap();
  let line = |node| snapfire_fsr_terminal::render(&node, 10).lines()[0].trim_end().to_owned();
  session.view().unwrap();
  assert_eq!(line(session.view().unwrap()), "AB");
}

#[test]
fn a_function_deletes_a_field_of_the_object_it_normalizes() {
  let page = "import { Text } from \"ink\";\n\ntype Verbs = { punctuation?: string; format?: string };\n\nconst normalize = (config: { verbs: Verbs }) => {\n  if (config.verbs.punctuation) {\n    config.verbs.format = \"{}\" + config.verbs.punctuation;\n    delete config.verbs.punctuation;\n  }\n  return config;\n};\n\nexport default function Shown() {\n  const config = normalize({ verbs: { punctuation: \"...\" } });\n  return <Text>{config.verbs.format} {\"punctuation\" in config.verbs ? \"kept\" : \"dropped\"}</Text>;\n}\n";
  let mut session = Session::open(&app(page)).unwrap();
  let node = session.view().unwrap();
  assert_eq!(snapfire_fsr_terminal::render(&node, 30).lines()[0].trim_end(), "{}... dropped");
}

#[test]
fn instanceof_error_tells_a_thrown_error_from_another_value() {
  let page = "import { Text } from \"ink\";\n\nconst explain = (e: unknown) => (e instanceof Error ? `${e.name}: ${e.message}` : \"not an error\");\n\nexport default function Explained() {\n  return <Text>{explain(new Error(\"boom\"))} | {explain(\"text\")} | {explain({ message: \"shape\" })}</Text>;\n}\n";
  let mut session = Session::open(&app(page)).unwrap();
  let node = session.view().unwrap();
  assert_eq!(snapfire_fsr_terminal::render(&node, 60).lines()[0].trim_end(), "Error: boom | not an error | not an error");
}

#[test]
fn a_write_through_a_field_alias_reaches_the_object_it_came_from() {
  let page = "import { Text } from \"ink\";\n\ntype Verbs = { punctuation?: string; format?: string };\ntype Config = { settings: { verbs: Verbs; theme: string } };\n\nconst normalize = (config: Config) => {\n  const verbs = config.settings.verbs;\n  if (verbs.punctuation) {\n    verbs.format = \"{}\" + verbs.punctuation;\n    delete verbs.punctuation;\n  }\n  return config;\n};\n\nexport default function Shown() {\n  const config = normalize({ settings: { verbs: { punctuation: \"...\" }, theme: \"dark\" } });\n  return <Text>{config.settings.verbs.format ?? \"none\"} {\"punctuation\" in config.settings.verbs ? \"kept\" : \"dropped\"} {config.settings.theme}</Text>;\n}\n";
  let mut session = Session::open(&app(page)).unwrap();
  let node = session.view().unwrap();
  assert_eq!(snapfire_fsr_terminal::render(&node, 40).lines()[0].trim_end(), "{}... dropped dark");
}

#[test]
fn an_async_function_awaits_inside_a_for_of_loop_and_a_counted_loop() {
  let dir = host_dir("loop");
  std::fs::write(dir.join("one.txt"), "one\n").unwrap();
  std::fs::write(dir.join("two.txt"), "two\n").unwrap();
  unsafe { std::env::set_var("FSR_TEST_LOOP_DIR", &dir); }
  let page = "import { useEffect, useState } from \"react\";\nimport { Text } from \"ink\";\nimport { file, path, system } from \"@snapfire/fsr-client/std\";\n\nconst readAll = async (dir: string, names: string[]) => {\n  const out: string[] = [];\n  for (const name of names) {\n    const text = await file.read(path.join(dir, name));\n    out.push(text.trim());\n  }\n  let total = 0;\n  for (let i = 0; i < out.length; i++) {\n    const again = await file.read(path.join(dir, names[i]));\n    total += again.length;\n  }\n  return `${out.join(\",\")} ${total}`;\n};\n\nexport default function Files() {\n  const [text, setText] = useState(\"loading\");\n  useEffect(() => {\n    const run = async () => {\n      const joined = await readAll(system.env(\"FSR_TEST_LOOP_DIR\"), [\"one.txt\", \"two.txt\"]);\n      setText(joined);\n    };\n    run();\n  }, []);\n  return <Text>{text}</Text>;\n}\n";
  let mut session = Session::open(&app(page)).unwrap();
  let line = |node| snapfire_fsr_terminal::render(&node, 30).lines()[0].trim_end().to_owned();
  session.view().unwrap();
  assert_eq!(line(session.view().unwrap()), "one,two 8");
}

#[test]
fn an_awaiting_loop_skips_an_item_with_continue() {
  let dir = host_dir("skip");
  std::fs::write(dir.join("one.txt"), "one\n").unwrap();
  std::fs::write(dir.join("two.txt"), "two\n").unwrap();
  unsafe { std::env::set_var("FSR_TEST_SKIP_DIR", &dir); }
  let page = "import { useEffect, useState } from \"react\";\nimport { Text } from \"ink\";\nimport { file, path, system } from \"@snapfire/fsr-client/std\";\n\nconst readKept = async (dir: string, names: string[], skipped: Record<string, boolean | undefined>) => {\n  const out: string[] = [];\n  for (const name of names) {\n    if (skipped[name] === undefined) {\n      continue;\n    }\n    if (skipped[name]) continue;\n    const text = await file.read(path.join(dir, name));\n    out.push(text.trim());\n  }\n  return out.join(\",\");\n};\n\nexport default function Kept() {\n  const [text, setText] = useState(\"loading\");\n  useEffect(() => {\n    const run = async () => {\n      const joined = await readKept(system.env(\"FSR_TEST_SKIP_DIR\"), [\"zero.txt\", \"one.txt\", \"two.txt\"], { \"one.txt\": true, \"two.txt\": false });\n      setText(joined);\n    };\n    run();\n  }, []);\n  return <Text>{text}</Text>;\n}\n";
  let mut session = Session::open(&app(page)).unwrap();
  let line = |node| snapfire_fsr_terminal::render(&node, 30).lines()[0].trim_end().to_owned();
  session.view().unwrap();
  assert_eq!(line(session.view().unwrap()), "two");
}

#[test]
fn an_async_local_storage_scopes_a_value_to_a_run_and_restores_the_one_before() {
  let page = "import { Text } from \"ink\";\nimport { Scope } from \"@snapfire/fsr-client/std\";\n\nconst scope = new Scope<{ id: string }>();\nconst current = () => scope.get()?.id ?? \"none\";\nconst within = (id: string, fn: () => string) => scope.run({ id }, fn);\n\nexport default function Scoped() {\n  const seen = within(\"a\", () => `${current()} ${within(\"b\", () => current())} ${current()}`);\n  return <Text>{seen} {current()}</Text>;\n}\n";
  let mut session = Session::open(&app(page)).unwrap();
  let node = session.view().unwrap();
  assert_eq!(snapfire_fsr_terminal::render(&node, 30).lines()[0].trim_end(), "a b a none");
}

#[test]
fn an_awaiting_loop_returns_early_from_its_body() {
  let dir = host_dir("early");
  std::fs::write(dir.join("one.txt"), "one\n").unwrap();
  std::fs::write(dir.join("two.txt"), "two\n").unwrap();
  std::fs::write(dir.join("three.txt"), "three\n").unwrap();
  unsafe { std::env::set_var("FSR_TEST_EARLY_DIR", &dir); }
  let page = "import { useEffect, useState } from \"react\";\nimport { Text } from \"ink\";\nimport { file, path, system } from \"@snapfire/fsr-client/std\";\n\nconst firstMatching = async (dir: string, names: string[], wanted: string) => {\n  let looked = 0;\n  for (const name of names) {\n    looked++;\n    const text = await file.read(path.join(dir, name));\n    if (text.trim() === wanted) {\n      return `${name} after ${looked}`;\n    }\n  }\n  return `none after ${looked}`;\n};\n\nexport default function Found() {\n  const [text, setText] = useState(\"loading\");\n  useEffect(() => {\n    const run = async () => {\n      const hit = await firstMatching(system.env(\"FSR_TEST_EARLY_DIR\"), [\"one.txt\", \"two.txt\", \"three.txt\"], \"two\");\n      const miss = await firstMatching(system.env(\"FSR_TEST_EARLY_DIR\"), [\"one.txt\"], \"zero\");\n      setText(`${hit} | ${miss}`);\n    };\n    run();\n  }, []);\n  return <Text>{text}</Text>;\n}\n";
  let mut session = Session::open(&app(page)).unwrap();
  let line = |node| snapfire_fsr_terminal::render(&node, 60).lines()[0].trim_end().to_owned();
  session.view().unwrap();
  assert_eq!(line(session.view().unwrap()), "two.txt after 2 | none after 1");
}

#[test]
fn the_value_of_an_earlier_await_survives_a_later_one_in_a_handler_and_in_a_module_function() {
  let page = "import { useEffect, useState } from \"react\";\nimport { Text } from \"ink\";\n\nconst lookup = async (q: string) => q.toUpperCase();\nconst both = async () => {\n  const a = await lookup(\"x\");\n  const b = await lookup(\"y\");\n  return a + b;\n};\n\nexport default function Pairs() {\n  const [text, setText] = useState(\"loading\");\n  useEffect(() => {\n    const run = async () => {\n      const first = await lookup(\"p\");\n      const second = await lookup(\"q\");\n      const joined = await both();\n      setText(`${first}${second} ${joined}`);\n    };\n    run();\n  }, []);\n  return <Text>{text}</Text>;\n}\n";
  let mut session = Session::open(&app(page)).unwrap();
  let line = |node| snapfire_fsr_terminal::render(&node, 30).lines()[0].trim_end().to_owned();
  session.view().unwrap();
  assert_eq!(line(session.view().unwrap()), "PQ XY");
}

#[test]
fn a_terminal_page_reads_and_writes_front_matter() {
  let page = "import { Text } from \"ink\";\nimport { text } from \"@snapfire/fsr-client/std\";\n\nconst source = text.withFrontMatter(\"Body\", { name: \"prompt\", tags: [\"a\", \"b\"] }, { delimiters: [\"<!--\", \"-->\"] });\n\nexport default function Matter() {\n  const parsed = text.frontMatter(source, { delimiters: [\"<!--\", \"-->\"] });\n  return <Text>{parsed.data.name} {parsed.data.tags.join(\"+\")} {parsed.content.trim()}</Text>;\n}\n";
  let mut session = Session::open(&app(page)).unwrap();
  let node = session.view().unwrap();
  assert_eq!(snapfire_fsr_terminal::render(&node, 30).lines()[0].trim_end(), "prompt a+b Body");
}

#[test]
fn an_awaited_read_that_fails_inside_a_try_runs_its_catch() {
  let dir = host_dir("missing");
  unsafe { std::env::set_var("FSR_TEST_MISSING", &dir); }
  let page = "import { useEffect, useState } from \"react\";\nimport { Text } from \"ink\";\nimport { file, system } from \"@snapfire/fsr-client/std\";\n\nexport default function Stat() {\n  const [text, setText] = useState(\"checking\");\n  useEffect(() => {\n    const run = async () => {\n      try {\n        await file.read(`${system.env(\"FSR_TEST_MISSING\")}/missing.txt`);\n        setText(\"found\");\n      } catch (error) {\n        setText(`${error.code}`);\n      }\n    };\n    run();\n  }, []);\n  return <Text>{text}</Text>;\n}\n";
  let mut session = Session::open(&app(page)).unwrap();
  let line = |node| snapfire_fsr_terminal::render(&node, 30).lines()[0].trim_end().to_owned();
  session.view().unwrap();
  assert_eq!(line(session.view().unwrap()), "ENOENT");
}
