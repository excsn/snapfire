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

/// A timer a `mount` handler of the root page starts: its delay, whether it repeats, the handler it runs and what it carries in.
struct Started {
  ms: i64,
  repeats: bool,
  handler: usize,
  carry: Value,
}

/// Runs every `mount` handler of the root page against `state` and lists the timers they start, in order.
fn mount_timers(library: &Components, props: &ValueMap, state: &ValueMap) -> Vec<Started> {
  let component = &library[MODULE];
  let mut started = Vec::new();
  for (index, handler) in component.handlers.iter().enumerate().filter(|(_, h)| h.event == "mount") {
    let stepped = Interpreter::default().island_step(MODULE, component, props, state, Some(HandlerRef::own(index)), &Value::Null, library).unwrap();
    for (action, input) in stepped.acts {
      let (true, Value::Map(timer)) = (action == snapfire_fsr_ir::host::Act::Timer.name(), input) else { continue };
      let Some(Value::Str(event)) = timer.get("event") else { panic!("a timer of mount handler {index} without an event: {timer:?}; {}", handler.event) };
      let at = component.handlers.iter().position(|h| h.event == **event).expect("the timer's handler");
      let ms = match timer.get("ms") {
        Some(Value::Int(ms)) => *ms as i64,
        Some(Value::F64(ms)) => *ms as i64,
        other => panic!("a timer delay of {other:?}"),
      };
      started.push(Started { ms, repeats: timer.get("repeats") == Some(&Value::Bool(true)), handler: at, carry: timer.get("carry").cloned().unwrap_or(Value::Null) });
    }
  }
  started
}

/// `timer` fired against `state`, as a terminal fires it.
fn fire(library: &Components, props: &ValueMap, state: &ValueMap, timer: &Started) -> snapfire_fsr_ir::render::Stepped {
  let mut event = ValueMap::default();
  event.insert("value".to_owned(), Value::Null);
  event.insert("ok".to_owned(), Value::Bool(true));
  event.insert("carry".to_owned(), timer.carry.clone());
  Interpreter::default().island_step(MODULE, &library[MODULE], props, state, Some(HandlerRef::own(timer.handler)), &Value::Map(event), library).unwrap()
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
fn a_timer_a_mount_effect_starts_is_an_act_its_mount_handler_runs() {
  let library = library(CLOCK);
  let none = ValueMap::default();
  let first = Interpreter::default().island_step(MODULE, &library[MODULE], &none, &none, None, &Value::Null, &library).unwrap();
  assert_eq!(first.rendered.html, "<ink-text>0 hidden</ink-text>");
  let timers = mount_timers(&library, &none, &first.state);
  assert_eq!(timers.iter().map(|t| (t.ms, t.repeats)).collect::<Vec<_>>(), [(250, true), (1000, false)], "an effect with dependencies, or one that starts no timer, starts none");
  let tick = fire(&library, &none, &first.state, &timers[0]);
  let tick = fire(&library, &none, &tick.state, &timers[0]);
  assert_eq!(tick.rendered.html, "<ink-text>2 hidden</ink-text>");
  let shown = fire(&library, &none, &tick.state, &timers[1]);
  assert_eq!(shown.rendered.html, "<ink-text>2 shown</ink-text>");
}

#[test]
fn use_app_exit_is_the_act_the_terminal_quits_on() {
  let quits = "import { Text, useApp, useInput } from \"ink\";\n\nexport default function Quits() {\n  const { exit } = useApp();\n  useInput((input) => {\n    if (input === \"q\") exit();\n  });\n  return <Text>press q</Text>;\n}\n";
  let library = library(quits);
  let component = &library[MODULE];
  let input = component.handlers.iter().position(|handler| handler.event == "input").unwrap();
  let interpreter = Interpreter::default();
  let exits = |stepped: &snapfire_fsr_ir::render::Stepped| stepped.acts.iter().any(|(action, _)| action == snapfire_fsr_ir::host::Act::Exit.name());
  let first = interpreter.island_step(MODULE, component, &ValueMap::default(), &ValueMap::default(), None, &Value::Null, &library).unwrap();
  assert!(!exits(&first));
  let other = interpreter.island_step(MODULE, component, &ValueMap::default(), &first.state, Some(HandlerRef::own(input)), &key("x", &[]), &library).unwrap();
  assert!(!exits(&other));
  let quit = interpreter.island_step(MODULE, component, &ValueMap::default(), &other.state, Some(HandlerRef::own(input)), &key("q", &[]), &library).unwrap();
  assert!(exits(&quit));
}

#[test]
fn the_terminal_size_reads_from_the_store_and_write_is_an_action_the_terminal_prints() {
  let sized = "import { useEffect } from \"react\";\nimport { Text, useStdout, useWindowSize, useIsScreenReaderEnabled } from \"ink\";\n\nexport default function Sized() {\n  const { write } = useStdout();\n  const { columns, rows } = useWindowSize();\n  const reader = useIsScreenReaderEnabled();\n  useEffect(() => {\n    const timer = setInterval(() => {\n      write(\"tick\\n\");\n    }, 1000);\n    return () => clearInterval(timer);\n  }, [write]);\n  return <Text>{columns}x{rows} {reader ? \"reader\" : \"screen\"}</Text>;\n}\n";
  let library = library(sized);
  let component = &library[MODULE];
  let mut store = ValueMap::default();
  store.insert("$terminal.columns".to_owned(), Value::Int(100));
  store.insert("$terminal.rows".to_owned(), Value::Int(30));
  let mut props = ValueMap::default();
  props.insert("$store".to_owned(), Value::Map(store));
  let interpreter = Interpreter::default();
  let first = interpreter.island_step(MODULE, component, &props, &ValueMap::default(), None, &Value::Null, &library).unwrap();
  assert_eq!(first.rendered.html, "<ink-text>100x30 screen</ink-text>");
  let timers = mount_timers(&library, &props, &first.state);
  assert_eq!(timers.iter().map(|t| (t.ms, t.repeats)).collect::<Vec<_>>(), [(1000, true)], "an effect depending only on write is a mount effect");
  let tick = fire(&library, &props, &first.state, &timers[0]);
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

#[test]
fn a_terminal_reads_local_time_in_this_machines_zone() {
  let page = "import { Text } from \"ink\";\n\nexport default function Clock() {\n  const at = new Date(1726011440123);\n  const now = new Date().toLocaleTimeString();\n  return <Text>{at.getHours()} {at.getMinutes()} {at.getTimezoneOffset()} {at.toLocaleTimeString(\"en-US\")} {now.length > 0 ? \"now\" : \"\"}</Text>;\n}\n";
  let library = library(page);
  let first = Interpreter::default().island_step(MODULE, &library[MODULE], &ValueMap::default(), &ValueMap::default(), None, &Value::Null, &library).unwrap();
  let text = first.rendered.html.trim_start_matches("<ink-text>").trim_end_matches("</ink-text>").to_owned();
  let parts: Vec<&str> = text.split(' ').collect();
  let offset: f64 = parts[2].parse().unwrap();
  let seconds = ((1_726_011_440_123.0 - offset * 60_000.0) / 1000.0).floor().rem_euclid(86_400.0) as u32;
  let (hours, minutes) = (seconds / 3600, seconds / 60 % 60);
  assert_eq!((parts[0], parts[1]), (hours.to_string().as_str(), minutes.to_string().as_str()), "{text}");
  let twelve = if hours % 12 == 0 { 12 } else { hours % 12 };
  assert_eq!(parts[3], format!("{twelve}:{minutes:02}:{:02}\u{202f}{}", seconds % 60, if hours < 12 { "AM" } else { "PM" }), "{text}");
  assert_eq!(parts[4], "now");
}

#[test]
fn a_terminal_updater_writes_a_copy_at_a_computed_index() {
  let page = "import { useState } from \"react\";\nimport { Text, useInput } from \"ink\";\n\nexport default function Lines() {\n  const [lines, setLines] = useState([\"a\", \"b\", \"c\"]);\n  useInput((input) => {\n    setLines((previous) => {\n      const next = [...previous];\n      next[Number(input)] = \"x\";\n      return next;\n    });\n  });\n  return <Text>{lines.join(\"\")}</Text>;\n}\n";
  let library = library(page);
  let component = &library[MODULE];
  let interpreter = Interpreter::default();
  let first = interpreter.island_step(MODULE, component, &ValueMap::default(), &ValueMap::default(), None, &Value::Null, &library).unwrap();
  let pressed = interpreter.island_step(MODULE, component, &ValueMap::default(), &first.state, Some(HandlerRef::own(0)), &key("1", &[]), &library).unwrap();
  assert_eq!(pressed.rendered.html, "<ink-text>axc</ink-text>");
}

#[test]
fn use_box_metrics_reads_what_the_terminal_measured_for_the_box_its_ref_is_on() {
  let page = "import { useRef } from \"react\";\nimport { Box, Text, useBoxMetrics } from \"ink\";\n\nexport default function Measured() {\n  const ref = useRef(null);\n  const { width, clientHeight, hasMeasured } = useBoxMetrics(ref);\n  const all = useBoxMetrics(ref);\n  return <Box ref={ref} borderStyle=\"round\"><Text>{hasMeasured ? \"m\" : \"-\"} {width} {clientHeight} {all.top}</Text></Box>;\n}\n";
  let library = library(page);
  let component = &library[MODULE];
  let interpreter = Interpreter::default();
  let first = interpreter.island_step(MODULE, component, &ValueMap::default(), &ValueMap::default(), None, &Value::Null, &library).unwrap();
  let html = &first.rendered.html;
  assert!(html.contains("<ink-text>- 0 0 0</ink-text>"), "zeros until measured: {html}");
  let id = html.split("ink-ref=\"").nth(1).and_then(|rest| rest.split('"').next()).unwrap_or_else(|| panic!("the box names its ref: {html}")).to_owned();
  let mut measured = ValueMap::default();
  for (key, n) in [("width", 12), ("height", 3), ("left", 0), ("top", 2), ("clientWidth", 10), ("clientHeight", 1)] {
    measured.insert(key.to_owned(), Value::Int(n));
  }
  let mut metrics = ValueMap::default();
  metrics.insert(id, Value::Map(measured));
  let mut store = ValueMap::default();
  store.insert("$terminal.metrics".to_owned(), Value::Map(metrics));
  let mut props = ValueMap::default();
  props.insert("$store".to_owned(), Value::Map(store));
  let second = interpreter.island_step(MODULE, component, &props, &first.state, None, &Value::Null, &library).unwrap();
  assert!(second.rendered.html.contains("<ink-text>m 12 1 2</ink-text>"), "{}", second.rendered.html);
}

#[test]
fn the_text_packages_ink_apps_import_run_natively_in_a_terminal() {
  let page = "import { ms } from \"@snapfire/fsr/terminal\";\nimport { stripAnsi } from \"@snapfire/fsr/terminal\";\nimport { stringWidth } from \"@snapfire/fsr/terminal\";\nimport { wrapAnsi } from \"@snapfire/fsr/terminal\";\nimport { Text } from \"ink\";\n\nexport default function Texts() {\n  const rows = wrapAnsi(\"ab cd\", 2, { hard: true }).split(\"\\n\");\n  return <Text>{ms(90000)} {ms(\"1h\")} {stripAnsi(\"\\u001b[1mx\\u001b[22m\")} {stringWidth(\"古a\")} {rows.join(\"|\")}</Text>;\n}\n";
  let library = library(page);
  let first = Interpreter::default().island_step(MODULE, &library[MODULE], &ValueMap::default(), &ValueMap::default(), None, &Value::Null, &library).unwrap();
  assert_eq!(first.rendered.html, "<ink-text>2m 3600000 x 3 ab|cd</ink-text>");
}

#[test]
fn an_effect_on_a_callback_that_never_changes_runs_and_one_on_changing_state_starts_a_timer_on_a_continuation() {
  let stable = "import { useCallback, useEffect, useReducer } from \"react\";\nimport { Text } from \"ink\";\n\nexport default function Ticks() {\n  const [n, dispatch] = useReducer((s) => s + 1, 0);\n  const tick = useCallback(() => dispatch(), []);\n  useEffect(() => {\n    const timer = setInterval(tick, 100);\n    return () => clearInterval(timer);\n  }, [tick]);\n  return <Text>{n}</Text>;\n}\n";
  let ticks = library(stable);
  let none = ValueMap::default();
  let first = Interpreter::default().island_step(MODULE, &ticks[MODULE], &none, &none, None, &Value::Null, &ticks).unwrap();
  assert_eq!(mount_timers(&ticks, &none, &first.state).iter().map(|t| (t.ms, t.repeats)).collect::<Vec<_>>(), [(100, true)]);
  let changing = "import { useEffect, useState } from \"react\";\nimport { Text } from \"ink\";\n\nexport default function Echo() {\n  const [n, setN] = useState(0);\n  useEffect(() => {\n    setTimeout(() => setN(n + 1), 100);\n  }, [n]);\n  return <Text>{n}</Text>;\n}\n";
  let echo = library(changing);
  let events: Vec<&str> = echo[MODULE].handlers.iter().map(|h| h.event.as_str()).collect();
  assert!(events[0].starts_with("resume:routes/page.tsx@") && events[1] == "effect:0", "{events:?}");
}

#[test]
fn a_timer_reads_its_delay_from_a_module_constant_and_runs_a_named_callback() {
  let page = "import { useCallback, useEffect, useReducer } from \"react\";\nimport { Text } from \"ink\";\n\nconst tickMs = 150;\n\nexport default function Ticks() {\n  const [n, dispatch] = useReducer((s: number) => s + 1, 0);\n  const tick = useCallback(() => {\n    dispatch();\n  }, []);\n  useEffect(() => {\n    const timer = setInterval(tick, tickMs);\n    return () => {\n      clearInterval(timer);\n    };\n  }, [tick]);\n  return <Text>{n}</Text>;\n}\n";
  let library = library(page);
  let none = ValueMap::default();
  let first = Interpreter::default().island_step(MODULE, &library[MODULE], &none, &none, None, &Value::Null, &library).unwrap();
  let timers = mount_timers(&library, &none, &first.state);
  assert_eq!(timers.iter().map(|t| (t.ms, t.repeats)).collect::<Vec<_>>(), [(150, true)]);
  let tick = fire(&library, &none, &first.state, &timers[0]);
  assert_eq!(tick.rendered.html, "<ink-text>1</ink-text>");
}

#[test]
fn set_cursor_position_while_rendering_hands_the_terminal_where_the_cursor_goes() {
  let page = "import { useState } from \"react\";\nimport { Text, useCursor, useInput } from \"ink\";\n\nexport default function Prompt() {\n  const [text, setText] = useState(\"\");\n  const { setCursorPosition } = useCursor();\n  useInput((input) => setText((t) => t + input));\n  setCursorPosition({ x: 2 + text.length, y: 0 });\n  return <Text>&gt; {text}</Text>;\n}\n";
  let library = library(page);
  let component = &library[MODULE];
  let interpreter = Interpreter::default();
  let first = interpreter.island_step(MODULE, component, &ValueMap::default(), &ValueMap::default(), None, &Value::Null, &library).unwrap();
  let at = |x: f64| {
    let mut map = ValueMap::default();
    map.insert("x".to_owned(), Value::F64(x));
    map.insert("y".to_owned(), Value::F64(0.0));
    Some(Value::Map(map))
  };
  let cursor = |stepped: &snapfire_fsr_ir::render::Stepped| stepped.emitted.iter().filter(|e| e.kind == snapfire_fsr_lower::terminal::CURSOR).map(|e| e.value.clone()).last();
  assert_eq!(cursor(&first), at(2.0));
  let typed = interpreter.island_step(MODULE, component, &ValueMap::default(), &first.state, Some(HandlerRef::own(0)), &key("ab", &[]), &library).unwrap();
  assert_eq!(cursor(&typed), at(4.0));
}

#[test]
fn a_module_let_a_function_writes_lasts_from_one_step_to_the_next_in_a_cell() {
  let page = "import { Text, useInput } from \"ink\";\n\nlet count = 0;\nlet rendered = 0;\n\nfunction bump() {\n  count = count + 1;\n}\n\nfunction seen() {\n  rendered = rendered + 1;\n  return rendered;\n}\n\nexport default function Counter() {\n  useInput(() => bump());\n  return <Text>{count} {seen()}</Text>;\n}\n";
  let library = library(page);
  let component = &library[MODULE];
  let interpreter = Interpreter::default();
  let first = interpreter.island_step(MODULE, component, &ValueMap::default(), &ValueMap::default(), None, &Value::Null, &library).unwrap();
  assert_eq!(first.rendered.html, "<ink-text>0 1</ink-text>");
  let mut props = ValueMap::default();
  props.insert(snapfire_fsr_ir::render::CELLS_PROP.to_owned(), Value::Map(first.cells.clone()));
  let pressed = interpreter.island_step(MODULE, component, &props, &first.state, Some(HandlerRef::own(0)), &key("x", &[]), &library).unwrap();
  assert_eq!(pressed.rendered.html, "<ink-text>1 2</ink-text>", "the handler's write and the render's own both last");
}

#[test]
fn ink_hooks_written_in_the_provided_module_are_inlined_like_an_apps_own() {
  let page = "import { Text, useWindowSize } from \"ink\";\n\nexport default function Size() {\n  const { columns } = useWindowSize();\n  return <Text>{columns}</Text>;\n}\n";
  let n = NEXT.fetch_add(1, Ordering::Relaxed);
  let dir = std::env::temp_dir().join(format!("fsr_terminal_{}_{n}", std::process::id()));
  std::fs::create_dir_all(dir.join("routes")).unwrap();
  std::fs::write(dir.join("routes/page.tsx"), page).unwrap();
  let mut set = ComponentSet::new(&dir).for_terminal();
  set.lower(MODULE).unwrap_or_else(|e| panic!("{e}"));
  let component = &set.components.iter().find(|(m, _)| m == MODULE).unwrap().1;
  assert!(component.stores.iter().any(|(_, key)| key == "$terminal.columns"), "{:?}", component.stores);
}

#[test]
fn a_file_read_on_a_web_page_is_refused_naming_what_it_needs() {
  let page = "import { file } from \"@snapfire/fsr-client/std\";\n\nexport default function Page() {\n  return <p>{file.read(\"/etc/hostname\")}</p>;\n}\n";
  let dir = std::env::temp_dir().join(format!("fsr_lower_terminal_web_fs_{}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  std::fs::create_dir_all(dir.join("routes")).unwrap();
  std::fs::write(dir.join("routes/page.tsx"), page).unwrap();
  let mut set = snapfire_fsr_lower::component::ComponentSet::new(&dir);
  let result = set.lower("routes/page.tsx#default").map_err(|e| e.to_string());
  std::fs::remove_dir_all(&dir).unwrap();
  let message = match result {
    Ok(()) => set.browser_only.first().map(|(_, r)| r.message.clone()).unwrap_or_default(),
    Err(e) => e,
  };
  assert!(message.contains("`file.read`, which needs a host with processes of its own"), "{message}");
}

#[test]
fn a_hook_called_under_a_type_cast_is_the_hook() {
  let sized = "import { useState } from \"react\";\nimport { Text, useStdout } from \"ink\";\n\nfunction useScreenSize() {\n  const { stdout } = useStdout() as any;\n  const [size] = useState(() => ({ width: stdout.columns, height: stdout.rows }));\n  return size;\n}\n\nexport default function Sized() {\n  const { width, height } = useScreenSize();\n  return <Text>{width}x{height}</Text>;\n}\n";
  let library = library(sized);
  let component = &library[MODULE];
  let mut store = ValueMap::default();
  store.insert("$terminal.columns".to_owned(), Value::Int(100));
  store.insert("$terminal.rows".to_owned(), Value::Int(30));
  let mut props = ValueMap::default();
  props.insert("$store".to_owned(), Value::Map(store));
  let interpreter = Interpreter::default();
  let first = interpreter.island_step(MODULE, component, &props, &ValueMap::default(), None, &Value::Null, &library).unwrap();
  assert_eq!(first.rendered.html, "<ink-text>100x30</ink-text>");
}

#[test]
fn a_handler_calls_a_function_the_component_declares_below_it() {
  let page = "import { useState } from \"react\";\nimport { Text, useInput } from \"ink\";\n\nexport default function Counter() {\n  const [count, setCount] = useState(0);\n  useInput((input, key) => {\n    if (input === \"+\") {\n      useStepUp(input, key);\n    }\n  });\n  function useStepUp(input: string, key: any) {\n    setCount(count + 1);\n  }\n  return <Text>{count}</Text>;\n}\n";
  let library = library(page);
  let component = &library[MODULE];
  let input = component.handlers.iter().position(|handler| handler.event == "input").unwrap();
  let interpreter = Interpreter::default();
  let first = interpreter.island_step(MODULE, component, &ValueMap::default(), &ValueMap::default(), None, &Value::Null, &library).unwrap();
  assert_eq!(first.rendered.html, "<ink-text>0</ink-text>");
  let plus = interpreter.island_step(MODULE, component, &ValueMap::default(), &first.state, Some(HandlerRef::own(input)), &key("+", &[]), &library).unwrap();
  assert_eq!(plus.rendered.html, "<ink-text>1</ink-text>");
}

const LOOPED: &str = "import { useEffect, useState } from \"react\";\nimport { Text } from \"ink\";\nimport { file } from \"@snapfire/fsr-client/std\";\n\nconst readAll = async (names: string[]) => {\n  const out: string[] = [];\n  for (const name of names) {\n    const text = await file.read(name);\n    out.push(text.trim());\n  }\n  return out.join(\",\");\n};\n\nexport default function Files() {\n  const [text, setText] = useState(\"loading\");\n  useEffect(() => {\n    const run = async () => {\n      const joined = await readAll([\"a\", \"b\"]);\n      setText(joined);\n    };\n    run();\n  }, []);\n  return <Text>{text}</Text>;\n}\n";

#[test]
fn a_loop_that_awaits_lowers_to_a_head_continuation_run_from_the_function_and_from_the_end_of_its_pass() {
  let library = library(LOOPED);
  let component = &library[MODULE];
  let events: Vec<&str> = component.handlers.iter().map(|h| h.event.as_str()).collect();
  let resumes = events.iter().filter(|e| e.starts_with("resume:")).count();
  assert!(resumes >= 3, "the head, the await inside the pass and the await of readAll: {events:?}");
  let runs = component.handlers.iter().map(|h| format!("{:?}", h.body).matches("action: \"$run\"").count()).sum::<usize>();
  assert!(runs >= 2, "the function runs the head and the end of the pass runs it again: {events:?}");
}
