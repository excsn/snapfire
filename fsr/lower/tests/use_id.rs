//! `useId()` in a component body is a `let` of the id the hoist key spells,
//! `_S_<module>|<n>[@<path>]_` and the bundle's copy reads the same spelling
//! from the hoist reader, so the two halves agree with nothing on the wire.

use std::path::PathBuf;
use std::sync::Arc;

use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_ir::ast::{Expr, Stmt};
use snapfire_fsr_ir::render::{prepare, Components, Rendered, HOISTED_PROP};
use snapfire_fsr_ir::Interpreter;
use snapfire_fsr_lower::component::ComponentSet;

fn app(tag: &str, files: &[(&str, &str)]) -> PathBuf {
  let dir = std::env::temp_dir().join(format!("fsr_use_id_{}_{tag}", std::process::id()));
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
  rendered: Rendered,
}

fn lower(tag: &str, files: &[(&str, &str)]) -> Result<Lowered, String> {
  let dir = app(tag, files);
  let mut set = ComponentSet::new(&dir);
  let result = set.lower("routes/page.tsx#default").map_err(|e| e.to_string());
  std::fs::remove_dir_all(&dir).unwrap();
  result?;
  set.settle_fills();
  let library: Components = set.components.iter().map(|(module, component)| (module.clone(), Arc::new(prepare(component)))).collect();
  let module = "routes/page.tsx#default";
  let rendered = Interpreter::default().with_consts(Some(Arc::new(set.consts.clone()))).render_module(module, &library[module], &ValueMap::default(), &library).map_err(|e| format!("{e:?}"))?;
  Ok(Lowered { set, rendered })
}

const FORM: &str = "import { useId, useState } from 'react';\n\nexport default function Form() {\n  const first = useId();\n  const last = useId();\n  const [sent, setSent] = useState(false);\n  return (\n    <form onSubmit={() => setSent(true)}>\n      <label htmlFor={first}>First</label>\n      <input id={first} />\n      <label htmlFor={last}>Last</label>\n      <input id={last} />\n      <p>{sent ? 'sent' : 'open'}</p>\n    </form>\n  );\n}\n";

#[test]
fn each_call_is_a_let_of_the_id_its_key_spells_and_the_bundle_reads_the_same_spelling() {
  let page = "import Form from '@src/Form';\n\nexport default function Page() {\n  return <Form />;\n}\n";
  let out = lower("form", &[("src/Form.tsx", FORM), ("routes/page.tsx", page)]).unwrap();
  let (_, form) = out.set.components.iter().find(|(m, _)| m == "src/Form.tsx#default").expect("the form lowers");
  let ids: Vec<(&str, u32)> = form.body.iter().filter_map(|s| match s {
    Stmt::Let { name, expr: Expr::HoistId(id) } => Some((name.as_str(), *id)),
    _ => None,
  }).collect();
  assert_eq!(ids, [("first", 0), ("last", 1)], "{:?}", form.body);
  let island = &out.rendered.islands[0];
  assert!(island.body.html.contains("<label for=\"_S_src/Form.tsx#default|0_\">First</label><input id=\"_S_src/Form.tsx#default|0_\">"), "{}", island.body.html);
  assert!(island.body.html.contains("<label for=\"_S_src/Form.tsx#default|1_\">Last</label><input id=\"_S_src/Form.tsx#default|1_\">"), "{}", island.body.html);
  let table = match island.mount_props().get(HOISTED_PROP) {
    Some(Value::Map(table)) => table.clone(),
    _ => ValueMap::default(),
  };
  assert!(table.values().all(|v| !matches!(v, Value::Str(s) if s.starts_with("_S_"))), "an id is spelled on both halves, not carried: {table:?}");
  let (_, rewritten) = out.set.rewritten().into_iter().find(|(f, _)| f == "src/Form.tsx").expect("the form is rewritten");
  assert!(rewritten.contains("const first = __sfh.id(0);\n  const last = __sfh.id(1);"), "{rewritten}");
  assert!(!rewritten.contains("useId()"), "{rewritten}");
}

#[test]
fn two_placements_of_one_component_spell_their_ids_apart() {
  let field = "import { useId } from 'react';\n\nexport default function Field({ label }: { label: string }) {\n  const id = useId();\n  return <p><label htmlFor={id}>{label}</label><input id={id} /></p>;\n}\n";
  let form = "import { useState } from 'react';\nimport Field from './Field';\n\nexport default function Form() {\n  const [n, setN] = useState(0);\n  return <form onSubmit={() => setN(n + 1)}><Field label=\"a\" /><Field label=\"b\" /><b>{n}</b></form>;\n}\n";
  let page = "import Form from '@src/Form';\n\nexport default function Page() {\n  return <Form />;\n}\n";
  let out = lower("twice", &[("src/Field.tsx", field), ("src/Form.tsx", form), ("routes/page.tsx", page)]).unwrap();
  let html = &out.rendered.islands[0].body.html;
  assert!(html.contains("<label for=\"_S_src/Field.tsx#default|0@c0_\">a</label><input id=\"_S_src/Field.tsx#default|0@c0_\">"), "{html}");
  assert!(html.contains("<label for=\"_S_src/Field.tsx#default|0@c1_\">b</label><input id=\"_S_src/Field.tsx#default|0@c1_\">"), "{html}");
  let (_, rewritten) = out.set.rewritten().into_iter().find(|(f, _)| f == "src/Form.tsx").expect("the form is rewritten");
  assert!(rewritten.contains("__sfh.p(0, <Field label=\"a\" />)") && rewritten.contains("__sfh.p(1, <Field label=\"b\" />)"), "each placement is keyed, so the field's reader spells its id below it: {rewritten}");
}

#[test]
fn a_page_the_server_renders_whole_spells_its_ids_too() {
  let page = "import { useId } from 'react';\n\nexport default function Page() {\n  const id = useId();\n  return <main><label htmlFor={id}>Name</label><input id={id} /></main>;\n}\n";
  let out = lower("whole", &[("routes/page.tsx", page)]).unwrap();
  assert_eq!(out.rendered.html, "<main><label for=\"_S_routes/page.tsx#default|0_\">Name</label><input id=\"_S_routes/page.tsx#default|0_\"></main>");
  assert!(out.rendered.islands.is_empty());
}

#[test]
fn use_id_inside_a_custom_hook_is_residue() {
  let hook = "import { useId } from 'react';\n\nexport function useField() {\n  const id = useId();\n  return { id };\n}\n";
  let page = "import { useField } from '@src/useField';\n\nexport default function Page() {\n  const { id } = useField();\n  return <input id={id} />;\n}\n";
  let err = match lower("hook", &[("src/useField.ts", hook), ("routes/page.tsx", page)]) {
    Ok(_) => panic!("lowered"),
    Err(err) => err,
  };
  assert!(err.contains("`useId` inside a custom hook"), "{err}");
}
