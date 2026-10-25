//! An `async` component stops the build with the loader named, wherever it sits: as the page or placed by one.

use snapfire_fsr_cli::new::{create, NewOptions};
use snapfire_fsr_cli::{build, Options};
use snapfire_fsr_test_internal::Scratch;

const CHART: &str = "async function points(): Promise<number[]> {\n  return [1, 2, 3];\n}\n\nexport default async function Chart() {\n  const data = await points();\n  return <ol>{data.map((p) => <li key={p}>{p}</li>)}</ol>;\n}\n";

fn project(tag: &str, page: &str) -> Scratch {
  let root = Scratch::new(tag);
  create(root.path(), NewOptions::default().fetch(false).with(vec!["react".to_owned()])).unwrap();
  root.write("app/importmap.json", r#"{"imports":{"@snapfire/fsr/client/react":"/r","react":"/r","react-dom/client":"/d"}}"#);
  root.write("app/vendor/.fsr-vendor.json", r#"{"packages":{"react":{"version":"19.3.1"},"react-dom":{"version":"19.3.1"}}}"#);
  root.write("app/routes/page.tsx", page);
  root.write("app/src/Chart.tsx", CHART);
  root
}

fn refusal(root: &Scratch) -> String {
  let app = root.join("app");
  match build(&app, &Options::beside(&app)) {
    Err(e) => e.to_string(),
    Ok(built) => panic!("built, with Chart as {:?}", built.report.components.iter().find(|(module, _, _)| module.starts_with("src/Chart.tsx"))),
  }
}

/// DEFECTS 1.152.
#[test]
fn an_async_component_a_page_places_stops_the_build_with_the_loader_named() {
  let root = project("async-nested", "import Chart from \"../src/Chart\";\n\nexport default function Page() {\n  return <main><Chart /></main>;\n}\n");
  let error = refusal(&root);
  assert!(error.contains("src/Chart.tsx") && error.contains("an `async` component") && error.contains("page.loader.ts"), "{error}");
}

#[test]
fn an_async_page_stops_the_build_with_the_loader_named() {
  let root = project("async-page", "export default async function Page() {\n  const n = await Promise.resolve(1);\n  return <main>{n}</main>;\n}\n");
  let error = refusal(&root);
  assert!(error.contains("routes/page.tsx") && error.contains("an `async` component"), "{error}");
}
