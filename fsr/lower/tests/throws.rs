//! `throw` on a render path renders the paths that do not throw and fails
//! the render on the one that does, as React's server renderer fails.

use std::path::PathBuf;
use std::sync::Arc;

use snapfire_fsr_core::ValueMap;
use snapfire_fsr_ir::render::{prepare, Components};
use snapfire_fsr_ir::Interpreter;
use snapfire_fsr_lower::component::ComponentSet;

fn app(tag: &str, files: &[(&str, &str)]) -> PathBuf {
  let dir = std::env::temp_dir().join(format!("fsr_throws_{}_{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  for (name, text) in files {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
  }
  dir
}

fn html(tag: &str, files: &[(&str, &str)]) -> Result<String, String> {
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
  Interpreter::default().with_consts(Some(Arc::new(set.consts.clone()))).render_module(module, &library[module], &ValueMap::default(), &library).map(|r| r.html).map_err(|e| format!("{e:?}"))
}

const HEADING: &str = "export default function Heading({ level, children }) {\n  switch (level) {\n    case 0:\n      throw Error('Heading must be inside a Section!');\n    case 1:\n      return <h1>{children}</h1>;\n    case 2:\n      return <h2>{children}</h2>;\n    default:\n      throw new Error('Unknown level: ' + level);\n  }\n}\n";

fn page(level: i32) -> String {
  format!("import Heading from \"@src/Heading\";\nexport default function Page() {{\n  return <section><Heading level={{{level}}}>Title</Heading></section>;\n}}\n")
}

#[test]
fn a_switch_whose_other_cases_throw_renders_the_case_taken() {
  assert_eq!(html("one", &[("src/Heading.tsx", HEADING), ("routes/page.tsx", &page(1))]).as_deref(), Ok("<section><h1>Title</h1></section>"));
  assert_eq!(html("two", &[("src/Heading.tsx", HEADING), ("routes/page.tsx", &page(2))]).as_deref(), Ok("<section><h2>Title</h2></section>"));
}

#[test]
fn a_throw_reached_fails_the_render_with_its_message() {
  let zero = html("zero", &[("src/Heading.tsx", HEADING), ("routes/page.tsx", &page(0))]);
  assert!(zero.as_ref().is_err_and(|e| e.contains("Heading must be inside a Section!")), "{zero:?}");
  let nine = html("nine", &[("src/Heading.tsx", HEADING), ("routes/page.tsx", &page(9))]);
  assert!(nine.as_ref().is_err_and(|e| e.contains("Unknown level: 9")), "{nine:?}");
}

#[test]
fn a_guard_that_throws_lets_the_rest_of_the_body_render() {
  let guard = "export default function Price({ amount }) {\n  if (amount < 0) {\n    throw new Error('negative');\n  }\n  return <b>{amount}</b>;\n}\n";
  let page = |amount: i32| format!("import Price from \"@src/Price\";\nexport default function Page() {{\n  return <p><Price amount={{{amount}}} /></p>;\n}}\n");
  assert_eq!(html("guard_ok", &[("src/Price.tsx", guard), ("routes/page.tsx", &page(3))]).as_deref(), Ok("<p><b>3</b></p>"));
  let thrown = html("guard_throw", &[("src/Price.tsx", guard), ("routes/page.tsx", &page(-1))]);
  assert!(thrown.as_ref().is_err_and(|e| e.contains("negative")), "{thrown:?}");
}

#[test]
fn a_helper_catches_what_its_try_throws() {
  let page = "const host = (s: string) => {\n  try {\n    return new URL(s).hostname;\n  } catch (e) {\n    return `bad ${e.name}`;\n  }\n};\nconst checked = (n: number) => {\n  try {\n    if (n > 1) throw new Error(\"big\");\n    return \"small\";\n  } catch (e) {\n    return e.message;\n  }\n};\nexport default function Page() {\n  return <p>{host(\"https://a.example/x\")} {host(\"nope\")} {checked(1)} {checked(2)}</p>;\n}\n";
  assert_eq!(html("try_catch", &[("routes/page.tsx", page)]).as_deref(), Ok("<p>a.example bad Error small big</p>"));
}
