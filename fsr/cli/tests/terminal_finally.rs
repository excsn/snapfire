//! `finally` in an async function a terminal page runs: it runs once the awaited promise settles, either way.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use snapfire_fsr_cli::terminal::{key_event, Press, Report, Session};
use snapfire_fsr_core::Value;

static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn app(page: &str) -> std::path::PathBuf {
  let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let app = std::env::temp_dir().join(format!("fsr-terminal-finally-{}-{n}", std::process::id())).join("app");
  let _ = std::fs::remove_dir_all(&app);
  std::fs::create_dir_all(app.join("routes")).unwrap();
  std::fs::write(app.join("routes/page.tsx"), page).unwrap();
  app
}

fn press(c: char) -> Value {
  let Some(Press::Input(key)) = key_event(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)) else { panic!("{c} is input") };
  key
}

fn exit(code: i128) -> Value {
  let mut exit = snapfire_fsr_core::ValueMap::default();
  exit.insert("code".to_owned(), Value::Int(code));
  exit.insert("signal".to_owned(), Value::Null);
  Value::Map(exit)
}

const RUN: &str = "import { system } from \"@snapfire/fsr-client/std\";\nimport { useState } from \"react\";\nimport { Text, useInput } from \"@snapfire/fsr/terminal/ink\";\n\nconst run = (command: string): Promise<void> =>\n  new Promise((resolve, reject) => {\n    const child = system.spawn(\"sh\", [\"-c\", command]);\n    child.on(\"exit\", (code) => {\n      if (code === 0) {\n        resolve();\n      } else {\n        reject(new Error(`exited with ${code}`));\n      }\n    });\n  });\n\n";

fn line(node: snapfire_fsr_terminal::Node) -> String {
  snapfire_fsr_terminal::render(&node, 40).lines()[0].clone()
}

#[test]
fn finally_runs_after_the_try_and_after_the_catch_around_an_await() {
  let page = format!("{RUN}async function attempt(command: string): Promise<string> {{\n  let said = \"\";\n  try {{\n    await run(command);\n    said = \"ok\";\n  }} catch (error) {{\n    said = `failed: ${{(error as Error).message}}`;\n  }} finally {{\n    said = said + \"!\";\n  }}\n  return `${{command}} ${{said}}`;\n}}\n\nexport default function Page() {{\n  const [status, setStatus] = useState(\"ready\");\n  useInput((input) => {{\n    void (async () => {{\n      const said = await attempt(input);\n      setStatus(said);\n    }})();\n  }});\n  return <Text>{{status}}</Text>;\n}}\n");
  let mut session = Session::open(&app(&page)).unwrap();
  session.press(&press('a')).unwrap();
  let spawns = session.take_spawns();
  assert_eq!(line(session.deliver(&spawns[0], Report::Exit, &exit(0)).unwrap()), "a ok!");
  session.press(&press('b')).unwrap();
  let spawns = session.take_spawns();
  assert_eq!(line(session.deliver(&spawns[0], Report::Exit, &exit(2)).unwrap()), "b failed: exited with 2!");
}

#[test]
fn a_try_with_only_a_finally_runs_it_and_rejects_with_what_the_await_rejected() {
  let page = format!("{RUN}let finished = 0;\n\nasync function attempt(command: string): Promise<string> {{\n  try {{\n    await run(command);\n  }} finally {{\n    finished = finished + 1;\n  }}\n  return `${{command}} ok`;\n}}\n\nexport default function Page() {{\n  const [status, setStatus] = useState(\"ready\");\n  useInput((input) => {{\n    void (async () => {{\n      try {{\n        const said = await attempt(input);\n        setStatus(`${{said}} after ${{finished}}`);\n      }} catch (error) {{\n        setStatus(`caught ${{(error as Error).message}} after ${{finished}}`);\n      }}\n    }})();\n  }});\n  return <Text>{{status}}</Text>;\n}}\n");
  let mut session = Session::open(&app(&page)).unwrap();
  session.press(&press('a')).unwrap();
  let spawns = session.take_spawns();
  assert_eq!(line(session.deliver(&spawns[0], Report::Exit, &exit(0)).unwrap()), "a ok after 1");
  session.press(&press('b')).unwrap();
  let spawns = session.take_spawns();
  assert_eq!(line(session.deliver(&spawns[0], Report::Exit, &exit(3)).unwrap()), "caught exited with 3 after 2");
}

#[test]
fn a_handler_async_arrow_runs_its_finally_after_the_await_settles() {
  let page = format!("{RUN}export default function Page() {{\n  const [status, setStatus] = useState(\"ready\");\n  const [runs, setRuns] = useState(0);\n  useInput((input) => {{\n    void (async () => {{\n      try {{\n        await run(input);\n        setStatus(`${{input}} ok`);\n      }} catch (error) {{\n        setStatus(`${{input}} failed`);\n      }} finally {{\n        setRuns((n) => n + 1);\n      }}\n    }})();\n  }});\n  return <Text>{{status}} {{runs}}</Text>;\n}}\n");
  let mut session = Session::open(&app(&page)).unwrap();
  session.press(&press('a')).unwrap();
  let spawns = session.take_spawns();
  assert_eq!(line(session.deliver(&spawns[0], Report::Exit, &exit(0)).unwrap()), "a ok 1");
  session.press(&press('b')).unwrap();
  let spawns = session.take_spawns();
  assert_eq!(line(session.deliver(&spawns[0], Report::Exit, &exit(1)).unwrap()), "b failed 2");
}
