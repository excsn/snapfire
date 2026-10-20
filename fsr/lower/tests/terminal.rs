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
  let mut set = ComponentSet::new(&dir);
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
