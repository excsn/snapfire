//! `<Suspense>` and `<Activity>` render their children inside the boundary
//! comments React's server writes, so a root hydrating over them finds them.

use std::path::PathBuf;
use std::sync::Arc;

use snapfire_fsr_core::ValueMap;
use snapfire_fsr_ir::render::{prepare, Components};
use snapfire_fsr_ir::Interpreter;
use snapfire_fsr_lower::component::ComponentSet;

fn app(tag: &str, files: &[(&str, &str)]) -> PathBuf {
  let dir = std::env::temp_dir().join(format!("fsr_boundaries_{}_{tag}", std::process::id()));
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

#[test]
fn suspense_renders_its_children_in_a_completed_boundary_and_never_the_fallback() {
  let page = "import { Suspense } from \"react\";\nfunction Loading() {\n  return <i>loading</i>;\n}\nexport default function Page() {\n  return <main><Suspense fallback={<Loading />}><h1>Albums</h1></Suspense></main>;\n}\n";
  assert_eq!(html("suspense", page).as_deref(), Ok("<main><!--$--><h1>Albums</h1><!--/$--></main>"));
  let namespaced = "import * as React from \"react\";\nexport default function Page() {\n  return <main><React.Suspense fallback={null}><h1>Albums</h1></React.Suspense></main>;\n}\n";
  assert_eq!(html("suspense_ns", namespaced).as_deref(), Ok("<main><!--$--><h1>Albums</h1><!--/$--></main>"));
}

#[test]
fn activity_renders_its_children_in_a_boundary_unless_hidden() {
  let page = |mode: &str| format!("import {{ Activity }} from \"react\";\nexport default function Page() {{\n  const mode = \"{mode}\";\n  return <main><Activity mode={{mode}}><p>tab</p></Activity><Activity><p>always</p></Activity></main>;\n}}\n");
  assert_eq!(html("visible", &page("visible")).as_deref(), Ok("<main><!--&--><p>tab</p><!--/&--><!--&--><p>always</p><!--/&--></main>"));
  assert_eq!(html("hidden", &page("hidden")).as_deref(), Ok("<main><!--&--><p>always</p><!--/&--></main>"));
}

#[test]
fn a_component_of_the_applications_own_named_suspense_is_that_component() {
  let page = "function Suspense({ children }) {\n  return <section>{children}</section>;\n}\nexport default function Page() {\n  return <Suspense><p>mine</p></Suspense>;\n}\n";
  assert_eq!(html("own", page).as_deref(), Ok("<section><p>mine</p></section>"));
}
