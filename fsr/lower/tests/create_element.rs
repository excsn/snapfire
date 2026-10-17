//! React's `createElement(type, props, ...children)` lowers as the JSX it stands for.

use std::path::PathBuf;
use std::sync::Arc;

use snapfire_fsr_core::ValueMap;
use snapfire_fsr_ir::render::{prepare, Components};
use snapfire_fsr_ir::Interpreter;
use snapfire_fsr_lower::component::ComponentSet;

fn app(tag: &str, files: &[(&str, &str)]) -> PathBuf {
  let dir = std::env::temp_dir().join(format!("fsr_create_element_{}_{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  for (name, text) in files {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
  }
  dir
}

fn render(tag: &str, files: &[(&str, &str)]) -> Result<String, String> {
  let dir = app(tag, files);
  let mut set = ComponentSet::new(&dir);
  let result = set.lower("routes/page.tsx#default").map_err(|e| e.to_string());
  std::fs::remove_dir_all(&dir).unwrap();
  result?;
  set.settle_fills();
  let library: Components = set.components.iter().map(|(module, component)| (module.clone(), Arc::new(prepare(component)))).collect();
  let module = "routes/page.tsx#default";
  Interpreter::default().with_consts(Some(Arc::new(set.consts.clone()))).render_module(module, &library[module], &ValueMap::default(), &library).map(|r| r.html).map_err(|e| format!("{e:?}"))
}

#[test]
fn a_named_import_builds_elements_components_and_text_children() {
  let page = "import { createElement } from 'react';\n\nfunction Greeting({ name }) {\n  return createElement(\n    'h1',\n    { className: 'greeting' },\n    'Hello ',\n    createElement('i', null, name),\n    '. Welcome!'\n  );\n}\n\nexport default function App() {\n  return createElement(\n    Greeting,\n    { name: 'Taylor' }\n  );\n}\n";
  let html = render("named", &[("routes/page.tsx", page)]).unwrap();
  assert_eq!(html, "<h1 class=\"greeting\">Hello <i>Taylor</i>. Welcome!</h1>");
}

#[test]
fn the_default_import_builds_an_element_with_spread_and_shorthand_props() {
  let page = "import React from 'react';\n\nconst id = 'x';\nconst rest = { title: 't' };\n\nexport default function App() {\n  return React.createElement('p', { id, ...rest }, 'a', 'b');\n}\n";
  let html = render("default", &[("routes/page.tsx", page)]).unwrap();
  assert_eq!(html, "<p id=\"x\" title=\"t\">ab</p>");
}

#[test]
fn a_fragment_tag_and_a_member_tag_lower_as_jsx_does() {
  let page = "import { Fragment, createElement } from 'react';\nimport * as UI from '@src/ui';\n\nexport default function App() {\n  return createElement(Fragment, null, createElement('ul', null, createElement(UI.Item, { n: 1 }), createElement(UI.Item, { n: 2 })));\n}\n";
  let ui = "import { createElement } from 'react';\n\nexport function Item({ n }) {\n  return createElement('li', null, n);\n}\n";
  let html = render("fragment", &[("src/ui.js", ui), ("routes/page.tsx", page)]).unwrap();
  assert_eq!(html, "<ul><li>1</li><li>2</li></ul>");
}

#[test]
fn a_tag_held_in_a_variable_stays_a_call() {
  let page = "import { createElement } from 'react';\n\nconst tag = 'h1';\n\nexport default function App() {\n  return createElement(tag, null, 'x');\n}\n";
  let err = render("variable_tag", &[("routes/page.tsx", page)]).unwrap_err();
  assert!(err.contains("`createElement`"), "{err}");
}
