//! `<Place>`: the element content placed at `#name` is written into, declared in a page or a layout.

use std::path::PathBuf;
use std::sync::Arc;

use snapfire_fsr_core::ValueMap;
use snapfire_fsr_ir::render::{prepare, Components, OUTLET_MARK};
use snapfire_fsr_ir::Interpreter;
use snapfire_fsr_lower::component::ComponentSet;
use snapfire_fsr_lower::LowerError;

fn app(tag: &str, files: &[(&str, &str)]) -> PathBuf {
  let dir = std::env::temp_dir().join(format!("fsr_places_{}_{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  for (name, text) in files {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
  }
  dir
}

fn lowered(tag: &str, page: &str) -> Result<ComponentSet, LowerError> {
  let dir = app(tag, &[("routes/page.tsx", page)]);
  let mut set = ComponentSet::new(&dir);
  let result = set.lower("routes/page.tsx#default");
  std::fs::remove_dir_all(&dir).unwrap();
  result.map(|_| set)
}

fn html(tag: &str, page: &str) -> String {
  let set = lowered(tag, page).unwrap_or_else(|e| panic!("{tag} lowers: {e}"));
  let library: Components = set.components.iter().map(|(module, component)| (module.clone(), Arc::new(prepare(component)))).collect();
  let module = "routes/page.tsx#default";
  Interpreter::default().render_module(module, &library[module], &ValueMap::default(), &library).expect("renders").html
}

fn refused(tag: &str, page: &str) -> String {
  match lowered(tag, page) {
    Err(LowerError::Residue(residue)) => residue.message,
    Err(other) => panic!("{tag} should be residue: {other}"),
    Ok(_) => panic!("{tag} should be residue"),
  }
}

#[test]
fn a_place_is_an_element_named_by_its_id_holding_its_outlet() {
  let page = "import { Place } from \"@snapfire/fsr-client/react\";\nexport default function Page() {\n  return <main><Place name=\"modals\" className=\"modal-root\" /><Place name=\"toasts\" as=\"aside\" /></main>;\n}\n";
  assert_eq!(html("place", page), format!("<main><div id=\"modals\" class=\"modal-root\">{OUTLET_MARK}modals\u{0}</div><aside id=\"toasts\">{OUTLET_MARK}toasts\u{0}</aside></main>"));
}

#[test]
fn a_place_is_written_out_by_name_and_holds_nothing_of_its_own() {
  let page = |body: &str| format!("import {{ Place }} from \"@snapfire/fsr-client/react\";\nexport default function Page({{ n }}: {{ n: string }}) {{\n  return <main>{body}</main>;\n}}\n");
  assert_eq!(refused("unnamed", &page("<Place />")), "a `<Place>` with no `name`");
  assert_eq!(refused("computed", &page("<Place name={n} />")), "a `<Place>`'s `name` is written out, in letters, digits, `-` and `_`");
  assert_eq!(refused("spaced", &page("<Place name=\"a b\" />")), "a `<Place>`'s `name` is written out, in letters, digits, `-` and `_`");
  assert_eq!(refused("id", &page("<Place name=\"m\" id=\"x\" />")), "an `id` on `<Place>`; its `name` is its id");
  assert_eq!(refused("children", &page("<Place name=\"m\"><p>x</p></Place>")), "a `<Place>` with children; it holds only what is placed at it");
}
