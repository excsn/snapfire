//! `localeCompare` on the server, each answer the one Chrome 153 gave for the same call.

use std::path::PathBuf;
use std::sync::Arc;

use snapfire_fsr_core::ValueMap;
use snapfire_fsr_ir::render::{prepare, Components};
use snapfire_fsr_ir::Interpreter;
use snapfire_fsr_lower::component::ComponentSet;

fn app(tag: &str, files: &[(&str, &str)]) -> PathBuf {
  let dir = std::env::temp_dir().join(format!("fsr_collation_{}_{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  for (name, text) in files {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
  }
  dir
}

fn html(tag: &str, page: &str) -> Result<String, String> {
  let dir = app(tag, &[("routes/page.tsx", page)]);
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

fn sorted(tag: &str, words: &str, locale: &str) -> Result<String, String> {
  let page = format!("export default function Page() {{\n  const words = [{words}];\n  return <p>{{[...words].sort((a, b) => a.localeCompare(b{locale})).join(\" \")}}</p>;\n}}\n");
  html(tag, &page)
}

#[test]
fn locale_compare_sorts_by_the_locales_collation_as_the_browser_does() {
  let en = "\"résumé\", \"resume\", \"Resume\", \"apple\", \"Äpfel\", \"zebra\", \"Zürich\", \"10\", \"9\", \"a\", \"B\", \"ß\", \"ss\", \"éclair\", \"eclair\"";
  assert_eq!(sorted("en", en, ", \"en\"").as_deref(), Ok("<p>10 9 a Äpfel apple B eclair éclair resume Resume résumé ss ß zebra Zürich</p>"));
  assert_eq!(sorted("de", "\"Äpfel\", \"Apfel\", \"Zürich\", \"Zug\", \"ß\", \"ss\"", ", \"de\"").as_deref(), Ok("<p>Apfel Äpfel ss ß Zug Zürich</p>"));
  assert_eq!(sorted("sv", "\"ö\", \"o\", \"z\", \"å\", \"a\"", ", \"sv\"").as_deref(), Ok("<p>a o z å ö</p>"));
}

#[test]
fn locale_compare_without_a_locale_collates_in_the_requests_locale_and_answers_minus_one_zero_or_one() {
  let page = "export default function Page() {\n  return <p>{[\"a\".localeCompare(\"b\"), \"b\".localeCompare(\"a\"), \"a\".localeCompare(\"a\"), \"a\".localeCompare(\"A\"), \"résumé\".localeCompare(\"resume\")].join(\",\")}</p>;\n}\n";
  assert_eq!(html("pairs", page).as_deref(), Ok("<p>-1,1,0,-1,1</p>"));
}
