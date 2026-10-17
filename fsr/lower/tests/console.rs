//! `console` calls on a render path write to the server's log on every
//! render and leave the markup as it is, as a server renderer running the
//! component writes them.

use std::path::PathBuf;
use std::sync::Arc;

use std::sync::Mutex;
use snapfire_fsr_core::ValueMap;
use snapfire_fsr_ir::render::{prepare, Components};
use snapfire_fsr_ir::Interpreter;
use snapfire_fsr_lower::component::ComponentSet;

static LINES: Mutex<Vec<(tracing::Level, String)>> = Mutex::new(Vec::new());

struct Capture;

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Capture {
  fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
    struct Message(String);
    impl tracing::field::Visit for Message {
      fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
          self.0 = format!("{value:?}");
        }
      }
    }
    if event.metadata().target() == "fsr::console" {
      let mut message = Message(String::new());
      event.record(&mut message);
      LINES.lock().unwrap().push((*event.metadata().level(), message.0));
    }
  }
}

fn app(tag: &str, files: &[(&str, &str)]) -> PathBuf {
  let dir = std::env::temp_dir().join(format!("fsr_console_{}_{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  for (name, text) in files {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
  }
  dir
}

#[test]
fn a_console_call_logs_on_every_render_and_renders_nothing() {
  use tracing_subscriber::layer::SubscriberExt;
  tracing::subscriber::set_global_default(tracing_subscriber::registry().with(Capture)).unwrap();
  let page = "import { useState } from \"react\";\nexport default function Page() {\n  const [liked, setLiked] = useState(true);\n  console.log(\"rendering\", liked, { n: 1 }, [2, 3]);\n  if (liked) {\n    console.warn(\"liked\");\n  } else {\n    console.error(\"not liked\");\n  }\n  return <button onClick={() => setLiked(!liked)}>{liked ? \"yes\" : \"no\"}</button>;\n}\n";
  let dir = app("logs", &[("routes/page.tsx", page)]);
  let mut set = ComponentSet::new(&dir);
  let result = set.lower("routes/page.tsx#default");
  std::fs::remove_dir_all(&dir).unwrap();
  result.unwrap();
  set.settle_fills();
  assert!(set.browser_only.is_empty(), "{:?}", set.browser_only);
  let library: Components = set.components.iter().map(|(module, component)| (module.clone(), Arc::new(prepare(component)))).collect();
  let module = "routes/page.tsx#default";
  for _ in 0..2 {
    let out = Interpreter::default().render_module(module, &library[module], &ValueMap::default(), &library).unwrap();
    assert_eq!(out.html, "<button>yes</button>");
  }
  let lines = LINES.lock().unwrap().clone();
  let line = (tracing::Level::INFO, "rendering true {\"n\":1} [2,3]".to_owned());
  let warned = (tracing::Level::WARN, "liked".to_owned());
  assert_eq!(lines, vec![line.clone(), warned.clone(), line, warned], "one line per call per render, the branch not taken writing none");
}
