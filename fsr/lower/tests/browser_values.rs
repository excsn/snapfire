//! A value only the browser knows, the viewer's clock, as text under an
//! element: the server writes nothing there, the component hydrates and its
//! browser copy marks the element so React fills the text at hydration.

use std::path::PathBuf;
use std::sync::Arc;

use snapfire_fsr_core::ValueMap;
use snapfire_fsr_ir::render::{prepare, Components};
use snapfire_fsr_ir::Interpreter;
use snapfire_fsr_lower::component::ComponentSet;
use snapfire_fsr_lower::hoist;

fn app(tag: &str, files: &[(&str, &str)]) -> PathBuf {
  let dir = std::env::temp_dir().join(format!("fsr_browser_values_{}_{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  for (name, text) in files {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
  }
  dir
}

struct Lowered {
  set: ComponentSet,
  html: String,
}

fn lower(tag: &str, files: &[(&str, &str)]) -> Result<Lowered, String> {
  let dir = app(tag, files);
  let mut set = ComponentSet::new(&dir);
  let result = set.lower("routes/page.tsx#default").map_err(|e| e.to_string());
  std::fs::remove_dir_all(&dir).unwrap();
  result?;
  set.settle_fills();
  if let Some((module, residue)) = set.browser_only.first() {
    return Err(format!("{module}: {}", residue.message));
  }
  let library: Components = set.components.iter().map(|(module, component)| (module.clone(), Arc::new(prepare(component)))).collect();
  let module = "routes/page.tsx#default";
  let html = Interpreter::default().with_consts(Some(Arc::new(set.consts.clone()))).render_module(module, &library[module], &ValueMap::default(), &library).map(|r| r.html).map_err(|e| format!("{e:?}"))?;
  Ok(Lowered { set, html })
}

fn rewritten(lowered: &Lowered, file: &str, source: &str) -> String {
  let rewrites: Vec<&hoist::Rewrite> = lowered.set.rewrites.iter().filter(|r| r.file == file).collect();
  hoist::apply(source, &rewrites)
}

#[test]
fn the_viewers_clock_as_text_is_left_to_the_browser() {
  let page = "export default function Page() {\n  return (\n    <h1>\n      Current Date: {new Date().toLocaleDateString()}\n    </h1>\n  );\n}\n";
  let out = lower("clock", &[("routes/page.tsx", page)]).unwrap();
  assert_eq!(out.html, "<h1>Current Date: </h1>");
  assert_eq!(out.set.pure.get("routes/page.tsx#default"), Some(&false), "a component the browser fills hydrates");
  let owner = out.set.components.iter().find(|(m, _)| m == "routes/page.tsx#default").map(|(_, c)| c.owner);
  assert_eq!(owner, Some(snapfire_fsr_ir::Owner::React), "React mounts it, so the browser fills the text");
  let source = rewritten(&out, "routes/page.tsx", page);
  assert!(source.contains("const __sfb = __sfUseBrowser();") && source.contains("Current Date: {__sfb(() => new Date().toLocaleDateString())}"), "{source}");
  assert_eq!(out.set.browser_texts.len(), 1, "{:?}", out.set.browser_texts);
  assert!(out.set.browser_texts[0].1.starts_with("routes/page.tsx:4:22: the current time on a render path"), "{:?}", out.set.browser_texts);
}

#[test]
fn reacts_own_hydration_attribute_never_reaches_the_document() {
  let page = "export default function Page() {\n  return (\n    <h1 suppressHydrationWarning={true}>\n      Current Date: {new Date().toLocaleDateString()}\n    </h1>\n  );\n}\n";
  let out = lower("marked", &[("routes/page.tsx", page)]).unwrap();
  assert_eq!(out.html, "<h1>Current Date: </h1>");
  assert_eq!(out.set.pure.get("routes/page.tsx#default"), Some(&false));
}

#[test]
fn a_browser_value_passed_as_a_prop_is_filled_where_the_child_writes_it() {
  let clock = "import { useState } from 'react';\n\nexport default function Clock(props) {\n  const [color, setColor] = useState(props.color);\n  return (\n    <h1 style={{ color: color }}>\n      {props.time}\n    </h1>\n  );\n}\n";
  let page = "import { useState, useEffect } from 'react';\nimport Clock from '@src/Clock';\n\nfunction useTime() {\n  const [time, setTime] = useState(() => new Date());\n  useEffect(() => {\n    const id = setInterval(() => {\n      setTime(new Date());\n    }, 1000);\n    return () => clearInterval(id);\n  }, []);\n  return time;\n}\n\nexport default function App() {\n  const time = useTime();\n  const [color, setColor] = useState('lightcoral');\n  return (\n    <div>\n      <p>\n        Pick a color:{' '}\n        <select value={color} onChange={e => setColor(e.target.value)}>\n          <option value=\"lightcoral\">lightcoral</option>\n          <option value=\"midnightblue\">midnightblue</option>\n        </select>\n      </p>\n      <Clock color={color} time={time.toLocaleTimeString()} />\n    </div>\n  );\n}\n";
  let out = lower("prop", &[("src/Clock.js", clock), ("routes/page.tsx", page)]).unwrap();
  assert!(out.html.contains("<h1 style=\"color:lightcoral\"></h1>"), "{}", out.html);
  assert!(rewritten(&out, "src/Clock.js", clock).contains("{__sfb(() => props.time)}"), "{}", rewritten(&out, "src/Clock.js", clock));
  assert!(rewritten(&out, "routes/page.tsx", page).contains("time={__sfb(() => time.toLocaleTimeString())}"), "{}", rewritten(&out, "routes/page.tsx", page));
  let texts: Vec<&str> = out.set.browser_texts.iter().map(|(_, site)| site.as_str()).collect();
  assert_eq!(texts.len(), 2, "{texts:?}");
  assert!(texts.iter().any(|t| t.starts_with("routes/page.tsx:27:") && t.contains("reads the viewer's time zone")), "{texts:?}");
  assert!(texts.iter().any(|t| t.starts_with("src/Clock.js:7:") && t.contains("a value only the browser knows")), "{texts:?}");
}

#[test]
fn a_binding_and_a_helper_that_read_the_clock_are_left_to_the_browser() {
  let page = "function now() {\n  return Date.now();\n}\nexport default function Page() {\n  const label = new Date().toLocaleTimeString();\n  const at = now();\n  return <p><span>{label}</span><b>{at}</b></p>;\n}\n";
  let out = lower("binding", &[("routes/page.tsx", page)]).unwrap();
  assert_eq!(out.html, "<p><span></span><b></b></p>");
  let source = rewritten(&out, "routes/page.tsx", page);
  assert!(source.contains("const label = __sfb(() => new Date().toLocaleTimeString());") && source.contains("const at = __sfb(() => now());"), "{source}");
}

#[test]
fn a_browser_value_at_the_root_or_under_a_placement_is_left_to_the_browser_too() {
  for (tag, body, html) in [("root", "<>{new Date().toLocaleTimeString()}</>", ""), ("placed", "<Box>{new Date().toLocaleTimeString()}</Box>", "<div></div>")] {
    let page = format!("function Box({{ children }}) {{\n  return <div>{{children}}</div>;\n}}\nexport default function Page() {{\n  return {body};\n}}\n");
    let out = lower(tag, &[("routes/page.tsx", &page)]).unwrap();
    assert_eq!(out.html, html, "{tag}");
    assert_eq!(out.set.browser_texts.len(), 1, "{tag}: {:?}", out.set.browser_texts);
  }
}

#[test]
fn a_browser_value_in_an_attribute_or_choosing_markup_stays_a_residue() {
  for (tag, body, says) in [
    ("attribute", "<p title={new Date().toLocaleTimeString()}>x</p>", "the current time on a render path"),
    ("branch", "<p>{new Date().getHours() > 6 ? <b>day</b> : <i>night</i>}</p>", "the current time on a render path"),
    ("condition", "<p className={new Date().getHours() > 6 ? \"day\" : \"night\"}>x</p>", "the current time on a render path"),
  ] {
    let page = format!("function Box({{ children }}) {{\n  return <div>{{children}}</div>;\n}}\nexport default function Page() {{\n  return {body};\n}}\n");
    let out = lower(tag, &[("routes/page.tsx", &page)]);
    assert!(out.as_ref().is_err_and(|e| e.contains(says)), "{tag}: {:?}", out.as_ref().map(|o| o.html.clone()));
  }
}

