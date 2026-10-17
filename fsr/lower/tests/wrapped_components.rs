//! A component wrapped in React's `memo` or `forwardRef` renders as the
//! function it wraps; a class component renders in the browser.

use std::path::PathBuf;
use std::sync::Arc;

use snapfire_fsr_core::ValueMap;
use snapfire_fsr_ir::render::{prepare, Components};
use snapfire_fsr_ir::Interpreter;
use snapfire_fsr_lower::component::ComponentSet;

fn app(tag: &str, files: &[(&str, &str)]) -> PathBuf {
  let dir = std::env::temp_dir().join(format!("fsr_wrapped_{}_{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  for (name, text) in files {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
  }
  dir
}

fn lowered(tag: &str, files: &[(&str, &str)]) -> Result<ComponentSet, String> {
  let dir = app(tag, files);
  let mut set = ComponentSet::new(&dir);
  let result = set.lower("routes/page.tsx#default").map_err(|e| e.to_string());
  std::fs::remove_dir_all(&dir).unwrap();
  result?;
  set.settle_fills();
  Ok(set)
}

fn html(tag: &str, files: &[(&str, &str)]) -> Result<String, String> {
  let set = lowered(tag, files)?;
  let library: Components = set.components.iter().map(|(module, component)| (module.clone(), Arc::new(prepare(component)))).collect();
  let module = "routes/page.tsx#default";
  Interpreter::default().with_consts(Some(Arc::new(set.consts.clone()))).render_module(module, &library[module], &ValueMap::default(), &library).map(|r| r.html).map_err(|e| format!("{e:?}"))
}

const PAGE: &str = "import Greeting from \"@src/Greeting\";\nexport default function Page() {\n  return <main><Greeting name=\"Ada\" /></main>;\n}\n";

#[test]
fn memo_renders_the_function_it_wraps() {
  let plain = html("plain", &[("src/Greeting.tsx", "export default function Greeting({ name }) {\n  return <h1>Hello, {name}</h1>;\n}\n"), ("routes/page.tsx", PAGE)]);
  assert_eq!(plain.as_deref(), Ok("<main><h1>Hello, Ada</h1></main>"));
  let spellings = [
    "import { memo } from \"react\";\nexport default memo(function Greeting({ name }) {\n  return <h1>Hello, {name}</h1>;\n});\n",
    "import { memo } from \"react\";\nconst Greeting = memo(({ name }) => <h1>Hello, {name}</h1>);\nexport default Greeting;\n",
    "import React from \"react\";\nexport default React.memo(({ name }) => <h1>Hello, {name}</h1>);\n",
    "import { memo as m } from \"react\";\nfunction Greeting({ name }) {\n  return <h1>Hello, {name}</h1>;\n}\nexport default m(Greeting);\n",
  ];
  for (i, source) in spellings.iter().enumerate() {
    assert_eq!(html(&format!("memo{i}"), &[("src/Greeting.tsx", source), ("routes/page.tsx", PAGE)]), plain, "{source}");
  }
}

#[test]
fn a_memo_placed_in_the_same_file_renders() {
  let page = "import { memo } from \"react\";\nconst Item = memo(function Item({ label }) {\n  return <li>{label}</li>;\n});\nexport default function Page() {\n  return <ul><Item label=\"a\" /><Item label=\"b\" /></ul>;\n}\n";
  assert_eq!(html("local", &[("routes/page.tsx", page)]).as_deref(), Ok("<ul><li>a</li><li>b</li></ul>"));
}

#[test]
fn forward_ref_binds_its_second_parameter_to_the_ref() {
  let input = "import { forwardRef } from \"react\";\nconst MyInput = forwardRef(function MyInput({ label, ...props }, ref) {\n  return <label>{label}<input {...props} ref={ref} /></label>;\n});\nexport default MyInput;\n";
  let page = "import MyInput from \"@src/MyInput\";\nexport default function Page() {\n  return <form><MyInput label=\"Name\" name=\"n\" /></form>;\n}\n";
  let out = html("fwd", &[("src/MyInput.tsx", input), ("routes/page.tsx", page)]);
  assert_eq!(out.as_deref(), Ok("<form><label>Name<input name=\"n\"></label></form>"));
}

#[test]
fn a_class_component_renders_in_the_browser_and_says_so() {
  let greeting = "import { Component } from \"react\";\nexport default class Greeting extends Component {\n  render() {\n    return <h1>Hello, {this.props.name}</h1>;\n  }\n}\n";
  let set = lowered("class", &[("src/Greeting.tsx", greeting), ("routes/page.tsx", PAGE)]).unwrap();
  let residue = set.browser_only.iter().find(|(m, _)| m == "src/Greeting.tsx#default").map(|(_, r)| r.message.clone());
  assert_eq!(residue.as_deref(), Some("a class component, which the build does not lower"));
  let pure = "import React from \"react\";\nclass Greeting extends React.PureComponent {\n  render() {\n    return <h1>{this.props.name}</h1>;\n  }\n}\nexport default Greeting;\n";
  let set = lowered("pure", &[("src/Greeting.tsx", pure), ("routes/page.tsx", PAGE)]).unwrap();
  assert!(set.browser_only.iter().any(|(m, _)| m == "src/Greeting.tsx#default"));
}

#[test]
fn a_class_that_is_not_a_react_component_is_still_no_component() {
  let greeting = "class Greeting {\n  render() {\n    return null;\n  }\n}\nexport default Greeting;\n";
  let out = lowered("notreact", &[("src/Greeting.tsx", greeting), ("routes/page.tsx", PAGE)]);
  assert!(out.as_ref().is_err_and(|e| e.contains("no exported `default`")), "{:?}", out.err());
}
