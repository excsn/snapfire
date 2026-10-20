//! Ink's elements and `useInput`, imported from `ink` or `@snapfire/fsr/terminal`, lowered and stepped.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_ir::render::{prepare, Components, HandlerRef};
use snapfire_fsr_ir::Interpreter;
use snapfire_fsr_lower::component::ComponentSet;

static NEXT: AtomicU32 = AtomicU32::new(0);

const MODULE: &str = "routes/page.tsx#default";

fn counter(from: &str) -> String {
  format!(
    "import {{ useState }} from \"react\";\nimport {{ Box, Text, Newline, Spacer, useInput }} from \"{from}\";\n\nexport default function Counter() {{\n  const [count, setCount] = useState(0);\n  useInput((input, key) => {{\n    if (input === \"+\" || key.upArrow) setCount(count + 1);\n    if (input === \"-\" || key.downArrow) setCount(count - 1);\n  }});\n  return (\n    <Box flexDirection=\"column\" borderStyle=\"round\" paddingX={{1}}>\n      <Text bold color=\"green\">Counter</Text>\n      <Box><Text>Count:</Text><Spacer /><Text color=\"cyan\">{{count}}</Text></Box>\n      <Text dimColor>+ or up<Newline />- or down</Text>\n    </Box>\n  );\n}}\n"
  )
}

fn library(source: &str) -> Components {
  let n = NEXT.fetch_add(1, Ordering::Relaxed);
  let dir = std::env::temp_dir().join(format!("fsr_terminal_{}_{n}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  std::fs::create_dir_all(dir.join("routes")).unwrap();
  std::fs::write(dir.join("routes/page.tsx"), source).unwrap();
  let mut set = ComponentSet::new(&dir).for_terminal();
  set.lower(MODULE).unwrap_or_else(|e| panic!("{e}"));
  set.components.iter().map(|(module, component)| (module.clone(), Arc::new(prepare(component)))).collect()
}

fn key(input: &str, pressed: &[&str]) -> Value {
  let mut key = ValueMap::default();
  for name in ["upArrow", "downArrow", "leftArrow", "rightArrow", "return", "escape", "ctrl", "shift", "tab", "backspace", "delete", "meta"] {
    key.insert(name.to_owned(), Value::Bool(pressed.contains(&name)));
  }
  let mut event = ValueMap::default();
  event.insert("input".to_owned(), Value::Str(input.into()));
  event.insert("key".to_owned(), Value::Map(key));
  Value::Map(event)
}

#[test]
fn ink_elements_lower_to_terminal_elements_and_use_input_steps_the_state() {
  for from in ["ink", "@snapfire/fsr/terminal"] {
    let library = library(&counter(from));
    let component = &library[MODULE];
    let input = component.handlers.iter().position(|handler| handler.event == "input").unwrap_or_else(|| panic!("{from}: no input handler in {:?}", component.handlers));
    let interpreter = Interpreter::default();
    let first = interpreter.island_step(MODULE, component, &ValueMap::default(), &ValueMap::default(), None, &Value::Null, &library).unwrap();
    let html = &first.rendered.html;
    assert!(html.starts_with("<ink-box flex-direction=\"column\" border-style=\"round\" padding-x=\"1\">"), "{from}: {html}");
    assert!(html.contains("<ink-text bold=\"true\" color=\"green\">Counter</ink-text>"), "{from}: {html}");
    assert!(html.contains("<ink-spacer></ink-spacer>") && html.contains("<ink-newline></ink-newline>"), "{from}: {html}");
    assert!(html.contains(">0</ink-text>"), "{from}: {html}");

    let plus = interpreter.island_step(MODULE, component, &ValueMap::default(), &first.state, Some(HandlerRef::own(input)), &key("+", &[]), &library).unwrap();
    assert!(plus.rendered.html.contains(">1</ink-text>"), "{from}: {}", plus.rendered.html);
    let up = interpreter.island_step(MODULE, component, &ValueMap::default(), &plus.state, Some(HandlerRef::own(input)), &key("", &["upArrow"]), &library).unwrap();
    assert!(up.rendered.html.contains(">2</ink-text>"), "{from}: {}", up.rendered.html);
    let other = interpreter.island_step(MODULE, component, &ValueMap::default(), &up.state, Some(HandlerRef::own(input)), &key("x", &[]), &library).unwrap();
    assert!(other.rendered.html.contains(">2</ink-text>"), "a key the handler ignores leaves the state: {}", other.rendered.html);
  }
}

const CLOCK: &str = "import { useEffect, useState } from \"react\";\nimport { Text } from \"ink\";\n\nexport default function Clock() {\n  const [ticks, setTicks] = useState(0);\n  const [shown, setShown] = useState(false);\n  useEffect(() => {\n    const timer = setInterval(() => setTicks((t) => t + 1), 250);\n    return () => clearInterval(timer);\n  }, []);\n  useEffect(() => {\n    setTimeout(() => setShown(true), 1000);\n  }, []);\n  useEffect(() => {\n    document.title = String(ticks);\n  }, [ticks]);\n  return <Text>{ticks} {shown ? \"shown\" : \"hidden\"}</Text>;\n}\n";

#[test]
fn a_timer_a_mount_effect_starts_lowers_to_a_handler_named_for_its_interval() {
  let library = library(CLOCK);
  let component = &library[MODULE];
  let events: Vec<&str> = component.handlers.iter().map(|handler| handler.event.as_str()).collect();
  assert_eq!(events, ["interval:250", "timeout:1000"], "an effect with dependencies, or one that starts no timer, is not one");
  let interpreter = Interpreter::default();
  let first = interpreter.island_step(MODULE, component, &ValueMap::default(), &ValueMap::default(), None, &Value::Null, &library).unwrap();
  assert_eq!(first.rendered.html, "<ink-text>0 hidden</ink-text>");
  let tick = interpreter.island_step(MODULE, component, &ValueMap::default(), &first.state, Some(HandlerRef::own(0)), &Value::Null, &library).unwrap();
  let tick = interpreter.island_step(MODULE, component, &ValueMap::default(), &tick.state, Some(HandlerRef::own(0)), &Value::Null, &library).unwrap();
  assert_eq!(tick.rendered.html, "<ink-text>2 hidden</ink-text>");
  let shown = interpreter.island_step(MODULE, component, &ValueMap::default(), &tick.state, Some(HandlerRef::own(1)), &Value::Null, &library).unwrap();
  assert_eq!(shown.rendered.html, "<ink-text>2 shown</ink-text>");
}

#[test]
fn use_app_exit_sets_the_state_the_terminal_reads_to_quit() {
  let quits = "import { Text, useApp, useInput } from \"ink\";\n\nexport default function Quits() {\n  const { exit } = useApp();\n  useInput((input) => {\n    if (input === \"q\") exit();\n  });\n  return <Text>press q</Text>;\n}\n";
  let library = library(quits);
  let component = &library[MODULE];
  let input = component.handlers.iter().position(|handler| handler.event == "input").unwrap();
  let interpreter = Interpreter::default();
  let first = interpreter.island_step(MODULE, component, &ValueMap::default(), &ValueMap::default(), None, &Value::Null, &library).unwrap();
  assert_eq!(first.state.get(snapfire_fsr_lower::EXIT_STATE), Some(&Value::Bool(false)));
  let other = interpreter.island_step(MODULE, component, &ValueMap::default(), &first.state, Some(HandlerRef::own(input)), &key("x", &[]), &library).unwrap();
  assert_eq!(other.state.get(snapfire_fsr_lower::EXIT_STATE), Some(&Value::Bool(false)));
  let quit = interpreter.island_step(MODULE, component, &ValueMap::default(), &other.state, Some(HandlerRef::own(input)), &key("q", &[]), &library).unwrap();
  assert_eq!(quit.state.get(snapfire_fsr_lower::EXIT_STATE), Some(&Value::Bool(true)));
}

#[test]
fn the_terminal_size_reads_from_the_store_and_write_is_an_action_the_terminal_prints() {
  let sized = "import { useEffect } from \"react\";\nimport { Text, useStdout, useWindowSize, useIsScreenReaderEnabled } from \"ink\";\n\nexport default function Sized() {\n  const { write } = useStdout();\n  const { columns, rows } = useWindowSize();\n  const reader = useIsScreenReaderEnabled();\n  useEffect(() => {\n    const timer = setInterval(() => {\n      write(\"tick\\n\");\n    }, 1000);\n    return () => clearInterval(timer);\n  }, [write]);\n  return <Text>{columns}x{rows} {reader ? \"reader\" : \"screen\"}</Text>;\n}\n";
  let library = library(sized);
  let component = &library[MODULE];
  assert_eq!(component.handlers.iter().map(|h| h.event.as_str()).collect::<Vec<_>>(), ["interval:1000"], "an effect depending only on write is a mount effect");
  let mut store = ValueMap::default();
  store.insert("$terminal.columns".to_owned(), Value::Int(100));
  store.insert("$terminal.rows".to_owned(), Value::Int(30));
  let mut props = ValueMap::default();
  props.insert("$store".to_owned(), Value::Map(store));
  let interpreter = Interpreter::default();
  let first = interpreter.island_step(MODULE, component, &props, &ValueMap::default(), None, &Value::Null, &library).unwrap();
  assert_eq!(first.rendered.html, "<ink-text>100x30 screen</ink-text>");
  let tick = interpreter.island_step(MODULE, component, &props, &first.state, Some(HandlerRef::own(0)), &Value::Null, &library).unwrap();
  assert_eq!(tick.acts, [("$stdout".to_owned(), Value::Str("tick\n".into()))]);
}

#[test]
fn use_animation_counts_frames_on_its_own_interval_while_active() {
  let animated = "import { useState } from \"react\";\nimport { Text, useAnimation, useInput } from \"ink\";\n\nexport default function Spin() {\n  const [paused, setPaused] = useState(false);\n  const { frame } = useAnimation({ interval: 80, isActive: !paused });\n  const { frame: slow, time } = useAnimation({ interval: 400 });\n  useInput((input) => {\n    if (input === \" \") setPaused(!paused);\n  });\n  return <Text>{frame} {slow} {time}</Text>;\n}\n";
  let library = library(animated);
  let component = &library[MODULE];
  let events: Vec<&str> = component.handlers.iter().map(|h| h.event.as_str()).collect();
  assert_eq!(events, ["interval:80", "interval:400", "input"]);
  let interpreter = Interpreter::default();
  let step = |state: &ValueMap, handler: Option<usize>, event: Value| interpreter.island_step(MODULE, component, &ValueMap::default(), state, handler.map(HandlerRef::own), &event, &library).unwrap();
  let first = step(&ValueMap::default(), None, Value::Null);
  assert_eq!(first.rendered.html, "<ink-text>0 0 0</ink-text>");
  let fast = step(&step(&first.state, Some(0), Value::Null).state, Some(0), Value::Null);
  let slow = step(&fast.state, Some(1), Value::Null);
  assert_eq!(slow.rendered.html, "<ink-text>2 1 400</ink-text>");
  let paused = step(&slow.state, Some(2), key(" ", &[]));
  assert_eq!(step(&paused.state, Some(0), Value::Null).rendered.html, "<ink-text>2 1 400</ink-text>", "an inactive animation holds its frame");
}

#[test]
fn an_empty_use_input_handler_is_no_handler() {
  let idle = "import { Text, useInput } from \"ink\";\n\nexport default function Idle() {\n  useInput(() => {\n    // waits for Ctrl-C\n  });\n  return <Text>idle</Text>;\n}\n";
  let library = library(idle);
  assert!(library[MODULE].handlers.is_empty());
}

#[test]
fn a_terminal_reads_the_clock_on_its_render_path() {
  let timed = "import { useState } from \"react\";\nimport { Text } from \"ink\";\n\nexport default function Timed() {\n  const [start] = useState(Date.now);\n  const now = Date.now();\n  return <Text>{now >= start && start > 1700000000000 ? \"ticking\" : \"stopped\"}</Text>;\n}\n";
  let library = library(timed);
  let first = Interpreter::default().island_step(MODULE, &library[MODULE], &ValueMap::default(), &ValueMap::default(), None, &Value::Null, &library).unwrap();
  assert_eq!(first.rendered.html, "<ink-text>ticking</ink-text>");

}

#[test]
fn number_checks_read_as_javascript_does() {
  let checks = "import { useState } from \"react\";\nimport { Text, useInput } from \"ink\";\n\nexport default function Pick() {\n  const [picked, setPicked] = useState(0);\n  useInput((input) => {\n    const number = Number(input);\n    if (Number.isSafeInteger(number) && number > 0) setPicked(number);\n  });\n  const flags = [Number.isInteger(2.5), Number.isInteger(4), Number.isFinite(1 / 0), Number.isNaN(0 / 0), Number.isSafeInteger(2 ** 60), Number.isInteger(\"4\")];\n  return <Text>{picked} {flags.join(\",\")}</Text>;\n}\n";
  let library = library(checks);
  let component = &library[MODULE];
  let interpreter = Interpreter::default();
  let first = interpreter.island_step(MODULE, component, &ValueMap::default(), &ValueMap::default(), None, &Value::Null, &library).unwrap();
  assert_eq!(first.rendered.html, "<ink-text>0 false,true,false,true,false,false</ink-text>");
  let three = interpreter.island_step(MODULE, component, &ValueMap::default(), &first.state, Some(HandlerRef::own(0)), &key("3", &[]), &library).unwrap();
  assert!(three.rendered.html.starts_with("<ink-text>3 "), "{}", three.rendered.html);
  let letter = interpreter.island_step(MODULE, component, &ValueMap::default(), &three.state, Some(HandlerRef::own(0)), &key("x", &[]), &library).unwrap();
  assert!(letter.rendered.html.starts_with("<ink-text>3 "), "{}", letter.rendered.html);
}

#[test]
fn the_screen_reader_props_are_left_out_of_a_terminal_element() {
  let aria = "import { Box, Text } from \"ink\";\n\nexport default function Aria() {\n  return <Box aria-role=\"checkbox\" aria-state={{ checked: true }} aria-label=\"done\"><Text aria-hidden>ok</Text></Box>;\n}\n";
  let library = library(aria);
  let first = Interpreter::default().island_step(MODULE, &library[MODULE], &ValueMap::default(), &ValueMap::default(), None, &Value::Null, &library).unwrap();
  assert_eq!(first.rendered.html, "<ink-box><ink-text>ok</ink-text></ink-box>");
}

#[test]
fn a_terminal_body_runs_a_while_loop_and_skips_a_busy_wait() {
  let looped = "import { Text } from \"ink\";\n\nexport default function Digits() {\n  let n = 12345;\n  let digits = 0;\n  while (n >= 1) {\n    n = Math.floor(n / 10);\n    digits += 1;\n  }\n  const start = Date.now();\n  while (Date.now() - start < 100) {\n  }\n  return <Text>{digits}</Text>;\n}\n";
  let library = library(looped);
  let first = Interpreter::default().island_step(MODULE, &library[MODULE], &ValueMap::default(), &ValueMap::default(), None, &Value::Null, &library).unwrap();
  assert_eq!(first.rendered.html, "<ink-text>5</ink-text>");
}

#[test]
fn a_terminal_function_returns_from_a_while_loop_drawing_random_numbers() {
  let picked = "import { Text } from \"ink\";\n\nfunction pick(taken: number[]): number {\n  while (true) {\n    const n = Math.floor(Math.random() * 4);\n    if (!taken.includes(n)) {\n      return n;\n    }\n  }\n}\n\nexport default function Free() {\n  const n = pick([0, 1, 3]);\n  return <Text>{n}</Text>;\n}\n";
  let library = library(picked);
  for _ in 0..20 {
    let step = Interpreter::default().island_step(MODULE, &library[MODULE], &ValueMap::default(), &ValueMap::default(), None, &Value::Null, &library).unwrap();
    assert_eq!(step.rendered.html, "<ink-text>2</ink-text>");
  }
}

#[test]
fn a_write_through_at_reaches_the_element_and_a_body_arrow_renders() {
  let grouped = "import { Text } from \"ink\";\n\nexport default function Runs() {\n  const runs: Array<{ text: string; color: string }> = [];\n  for (const cell of [\"a\", \"a\", \"b\", \"a\"]) {\n    const last = runs.at(-1);\n    if (last !== undefined && last.color === cell) {\n      last.text += cell;\n    } else {\n      runs.push({ text: cell, color: cell });\n    }\n  }\n  const shout = (s: string) => s.toUpperCase();\n  return <Text>{runs.map((run) => shout(run.text)).join(\",\")}</Text>;\n}\n";
  let library = library(grouped);
  let first = Interpreter::default().island_step(MODULE, &library[MODULE], &ValueMap::default(), &ValueMap::default(), None, &Value::Null, &library).unwrap();
  assert_eq!(first.rendered.html, "<ink-text>AA,B,A</ink-text>");
}
