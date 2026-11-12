//! Every member of JavaScript's standard library, one row each in `standard_members.tsv`: a body that reads it, what a loader and a component do with it and node v26.8.1's answer with `TZ=UTC`. The guide's chapter 901 lists the rows; `reference.rs` checks the list.
//!
//! A `lowered` loader and a `server` component answer what node answers. Where the last column gives Chrome's answer they answer that instead. A `browser` component leaves the value to the browser. A `refused` row is refused at build. A `throws` row throws in both. An `awaits` component is not tried, since a component cannot await. None lowers and then fails at run time.

use std::sync::Arc;

use snapfire_fsr_core::{Params, Value, ValueMap};
use snapfire_fsr_ir::render::{prepare, Components};
use snapfire_fsr_ir::Interpreter;
use snapfire_fsr_lower::component::ComponentSet;
use snapfire_fsr_lower::lower_loader;
use snapfire_fsr_runtime::RequestCtx;

const ROWS: &str = include_str!("standard_members.tsv");

enum Answer {
  Refused(String),
  Browser(String),
  Value(String),
  Failed(String),
}

fn in_loader(body: &str) -> Answer {
  let source = format!("export async function load() {{\n  {body}\n  return {{ v: String(JSON.stringify(v)) }};\n}}\n");
  let lowered = match lower_loader("l.ts", &source) {
    Ok(lowered) => lowered,
    Err(e) => return Answer::Refused(e.to_string()),
  };
  match futures::executor::block_on(Interpreter::default().run(&lowered, &RequestCtx::anonymous(Params::new()), None)).map(|o| o.value) {
    Ok(Value::Map(fields)) => match fields.get("v") {
      Some(Value::Str(text)) => Answer::Value(text.to_string()),
      other => Answer::Failed(format!("{other:?}")),
    },
    other => Answer::Failed(format!("{other:?}")),
  }
}

fn in_component(body: &str, row: usize) -> Answer {
  let dir = std::env::temp_dir().join(format!("standard_members_{}_{row}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  std::fs::create_dir_all(dir.join("routes")).unwrap();
  std::fs::write(dir.join("routes/page.tsx"), format!("export default function Page() {{\n  {body}\n  return <p>{{String(JSON.stringify(v))}}</p>;\n}}\n")).unwrap();
  let mut set = ComponentSet::new(&dir);
  let lowered = set.lower("routes/page.tsx#default").map_err(|e| e.to_string());
  std::fs::remove_dir_all(&dir).unwrap();
  if let Err(e) = lowered {
    return Answer::Refused(e);
  }
  set.settle_fills();
  if let Some((_, residue)) = set.browser_only.first() {
    return Answer::Browser(residue.message.clone());
  }
  if let Some((_, text)) = set.browser_texts.first() {
    return Answer::Browser(text.clone());
  }
  let library: Components = set.components.iter().map(|(module, component)| (module.clone(), Arc::new(prepare(component)))).collect();
  let module = "routes/page.tsx#default";
  match Interpreter::default().with_consts(Some(Arc::new(set.consts.clone()))).render_module(module, &library[module], &ValueMap::default(), &library) {
    Ok(rendered) => match rendered.html.strip_prefix("<p>").and_then(|html| html.strip_suffix("</p>")) {
      Some(text) => Answer::Value(text.replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&")),
      None => Answer::Failed(rendered.html),
    },
    Err(e) => Answer::Failed(format!("{e:?}")),
  }
}

fn described(answer: Answer) -> String {
  match answer {
    Answer::Refused(e) => format!("refused: {}", e.lines().next().unwrap_or("")),
    Answer::Browser(e) => format!("left to the browser: {}", e.lines().next().unwrap_or("")),
    Answer::Value(text) => format!("answered {text}"),
    Answer::Failed(e) => format!("failed: {}", e.chars().take(160).collect::<String>()),
  }
}

#[test]
fn every_standard_member_answers_what_node_answers_or_is_refused_at_build() {
  // SAFETY: this target holds one test, so no other thread reads the environment.
  unsafe { std::env::set_var("TZ", "UTC") };
  let mut wrong = Vec::new();
  for (row, line) in ROWS.lines().enumerate().filter(|(_, line)| !line.starts_with('#')) {
    let [global, member, loader, component, body, node, chrome] = line.split('\t').collect::<Vec<_>>()[..] else { panic!("a row of seven fields: {line}") };
    let want = if chrome.is_empty() { node } else { chrome };
    let answers = |answer: &Answer, class: &str| match (class, answer) {
      ("lowered" | "server", Answer::Value(text)) => want.strip_prefix("ok:") == Some(text.as_str()),
      ("browser", Answer::Browser(_)) | ("refused", Answer::Refused(_)) => true,
      ("throws", Answer::Failed(_)) => node.starts_with("throws:"),
      _ => false,
    };
    let got = in_loader(body);
    if !answers(&got, loader) {
      wrong.push(format!("{global} {member} in a loader ({loader}): {body}\n    want {want}\n    fsr  {}", described(got)));
    }
    if component != "awaits" {
      let got = in_component(body, row);
      if !answers(&got, component) {
        wrong.push(format!("{global} {member} in a component ({component}): {body}\n    want {want}\n    fsr  {}", described(got)));
      }
    }
  }
  assert!(wrong.is_empty(), "{} rows wrong:\n{}", wrong.len(), wrong.join("\n"));
}
