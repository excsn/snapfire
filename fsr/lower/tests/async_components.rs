//! What a page may be async about: a component loaded lazily is placed and
//! rendered on the server like any other, and a component that awaits data
//! is refused with the loader named.

use std::path::PathBuf;

use snapfire_fsr_ir::Tmpl;
use snapfire_fsr_lower::component::ComponentSet;

fn app(tag: &str, files: &[(&str, &str)]) -> PathBuf {
  let dir = std::env::temp_dir().join(format!("fsr_async_{}_{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  for (name, source) in files {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, source).unwrap();
  }
  dir
}

/// The residue a module leaves, whether the set refused it or kept it for the browser.
fn residue_of(set: &mut ComponentSet, module: &str) -> Option<String> {
  match set.lower(module) {
    Err(e) => Some(e.to_string()),
    Ok(()) => set.browser_only.iter().find(|(m, _)| m == module).map(|(_, r)| r.message.clone()),
  }
}

fn placed_in(tmpl: &Tmpl) -> Vec<String> {
  match tmpl {
    Tmpl::Component { module, children, .. } => std::iter::once(module.clone()).chain(children.iter().flat_map(placed_in)).collect(),
    Tmpl::Island { module, children, .. } => std::iter::once(module.clone()).chain(children.iter().flat_map(placed_in)).collect(),
    Tmpl::Element { children, .. } | Tmpl::Fragment(children) => children.iter().flat_map(placed_in).collect(),
    _ => Vec::new(),
  }
}

const HEAVY: &str = "export default function Heavy({ n }: { n: number }) {\n  return <p className=\"heavy\">{n}</p>;\n}\n";

#[test]
fn a_lazily_imported_component_is_placed_and_rendered_on_the_server() {
  let page = r#"import { lazy } from "react";
const Heavy = lazy(() => import("@src/Heavy"));
export default function Page({ n }: { n: number }) {
  return <main><Heavy n={n} /></main>;
}
"#;
  let dir = app("lazy", &[("routes/page.tsx", page), ("src/Heavy.tsx", HEAVY)]);
  let mut set = ComponentSet::new(&dir);
  assert_eq!(residue_of(&mut set, "routes/page.tsx#default"), None);
  let (_, page) = set.components.iter().find(|(m, _)| m == "routes/page.tsx#default").unwrap();
  assert_eq!(placed_in(&page.render), vec!["src/Heavy.tsx#default".to_owned()], "{:?}", page.render);
  assert!(set.components.iter().any(|(m, _)| m == "src/Heavy.tsx#default"), "the lazy component is lowered like a static import");
}

#[test]
fn a_lazy_import_through_a_relative_path_resolves_beside_the_page() {
  let page = r#"import { lazy } from "react";
const Heavy = lazy(() => import("./Heavy"));
export default function Page() {
  return <main><Heavy n={1} /></main>;
}
"#;
  let dir = app("lazy_relative", &[("routes/page.tsx", page), ("routes/Heavy.tsx", HEAVY)]);
  let mut set = ComponentSet::new(&dir);
  assert_eq!(residue_of(&mut set, "routes/page.tsx#default"), None);
  let (_, page) = set.components.iter().find(|(m, _)| m == "routes/page.tsx#default").unwrap();
  assert_eq!(placed_in(&page.render), vec!["routes/Heavy.tsx#default".to_owned()]);
}

#[test]
fn an_async_page_is_refused_with_the_loader_named() {
  let page = r#"export default async function Page({ id }: { id: string }) {
  const data = await fetchThing(id);
  return <main>{data.name}</main>;
}
"#;
  let dir = app("async_page", &[("routes/page.tsx", page)]);
  let mut set = ComponentSet::new(&dir);
  let residue = residue_of(&mut set, "routes/page.tsx#default").expect("an async page is not lowered as a synchronous one");
  assert!(residue.contains("an `async` component") && residue.contains("page.loader.ts"), "{residue}");
}

#[test]
fn an_async_arrow_component_is_refused_the_same_way() {
  let page = r#"const Page = async ({ id }: { id: string }) => {
  const data = await fetchThing(id);
  return <main>{data.name}</main>;
};
export default Page;
"#;
  let dir = app("async_arrow", &[("routes/page.tsx", page)]);
  let mut set = ComponentSet::new(&dir);
  let residue = residue_of(&mut set, "routes/page.tsx#default").expect("an async arrow is not lowered as a synchronous one");
  assert!(residue.contains("an `async` component") && residue.contains("page.loader.ts"), "{residue}");
}
