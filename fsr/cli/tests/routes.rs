use std::path::{Path, PathBuf};

use snapfire_fsr_cli::{build, BuildError, Options};

fn app(files: &[(&str, &str)]) -> PathBuf {
  let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
  let dir = std::env::temp_dir().join(format!("fsr-cli-routes-{}-{nanos}", std::process::id()));
  std::fs::create_dir_all(dir.join("routes")).unwrap();
  std::fs::write(dir.join("importmap.json"), r#"{"imports":{}}"#).unwrap();
  for (name, source) in files {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, source).unwrap();
  }
  dir
}

const LAYOUT: &str = "import { Slot } from \"@snapfire/fsr-client/react\";\nexport default function Layout({ children, feed }: { children: unknown; feed: unknown }) {\n  return <div>{children}{feed}<Slot name=\"modal\" /></div>;\n}\n";
const PAGE: &str = "export default function Page() {\n  return <p>page</p>;\n}\n";

fn fails(dir: &Path) -> BuildError {
  match build(dir, &Options::default()) {
    Ok(_) => panic!("{} built", dir.display()),
    Err(e) => e,
  }
}

fn plan_json(dir: &Path) -> serde_json::Value {
  let built = build(dir, &Options::default()).unwrap();
  serde_json::from_str(&built.manifest.to_json()).unwrap()
}

#[test]
fn a_layouts_slots_directory_is_a_parallel_segment_beside_its_page() {
  let dir = app(&[
    ("routes/layout.tsx", LAYOUT),
    ("routes/index/page.tsx", PAGE),
    ("routes/slots/feed/page.tsx", "export default function Feed() {\n  return <ul>feed</ul>;\n}\n"),
    ("routes/slots/feed/page.loader.ts", "export async function load() {\n  return { items: [] };\n}\n"),
    ("routes/slots/feed/loading.tsx", "export default function Loading() {\n  return <p>soon</p>;\n}\n"),
  ]);
  let built = build(&dir, &Options::default()).unwrap();
  assert!(built.report.slots.contains(&("layout.feed".to_owned(), "routes/slots/feed/page.tsx#default".to_owned())), "{}", built.report);
  let plan = plan_json(&dir);
  let layout = &plan["routes"][0]["plan"]["children"][0]["node"];
  assert_eq!(layout["module"], "routes/layout.tsx#default");
  assert_eq!(layout["children"][0]["slot"], "content");
  assert_eq!(layout["children"][1]["slot"], "feed");
  let feed = &layout["children"][1]["node"];
  assert_eq!(feed["source"], "layout.feed");
  assert_eq!(feed["deferred"], true);
  assert_eq!(feed["fallback"], "routes/slots/feed/loading.tsx#default");
  let ids: Vec<u64> = [&plan["routes"][0]["plan"], layout, &layout["children"][0]["node"], feed].iter().map(|n| n["id"].as_u64().unwrap()).collect();
  assert_eq!(ids, vec![0, 1, 2, 3], "ids run in tree order");
  assert!(built.files.iter().any(|(name, text)| name == "generated/client.ts" && text.contains("export type LayoutFeedProps")), "the slot's props type is generated");
  assert!(built.report.to_string().contains("slots     layout.feed"), "{}", built.report);
  std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_page_slot_variant_is_an_intercept_under_the_layout_declaring_the_slot() {
  let dir = app(&[
    ("routes/layout.tsx", LAYOUT),
    ("routes/index/page.tsx", PAGE),
    ("routes/slots/feed/page.tsx", "export default function Feed() {\n  return <ul>feed</ul>;\n}\n"),
    ("routes/photo/[id]/page.tsx", PAGE),
    ("routes/photo/[id]/page.modal.tsx", "export default function Modal() {\n  return <dialog>photo</dialog>;\n}\n"),
    ("routes/photo/[id]/loading.tsx", "export default function Loading() {\n  return <p>page soon</p>;\n}\n"),
    ("routes/photo/[id]/loading.modal.tsx", "export default function Loading() {\n  return <p>modal soon</p>;\n}\n"),
  ]);
  let built = build(&dir, &Options::default()).unwrap();
  assert!(built.report.intercepts.contains(&("/photo/{id} into modal".to_owned(), "routes/photo/[id]/page.modal.tsx#default".to_owned())), "{}", built.report);
  let plan = plan_json(&dir);
  assert_eq!(plan["intercepts"].as_array().unwrap().len(), 1);
  let intercept = &plan["intercepts"][0];
  assert_eq!(intercept["pattern"], "/photo/{id}");
  let layout = &intercept["plan"]["children"][0]["node"];
  assert_eq!(layout["module"], "routes/layout.tsx#default");
  assert_eq!(layout["keep"], serde_json::json!(["content", "feed"]), "the page and the other slot stay as the browser has them");
  assert_eq!(layout["children"].as_array().unwrap().len(), 1);
  assert_eq!(layout["children"][0]["slot"], "modal");
  let variant = &layout["children"][0]["node"];
  assert_eq!(variant["module"], "routes/photo/[id]/page.modal.tsx#default");
  assert_eq!(variant["deferred"], true);
  assert_eq!(variant["fallback"], "routes/photo/[id]/loading.modal.tsx#default", "a variant streams behind its own loading module, not the page's");
  let route = plan["routes"].as_array().unwrap().iter().find(|r| r["pattern"] == "/photo/{id}").unwrap();
  assert_eq!(route["plan"]["children"][0]["node"]["children"][0]["node"]["fallback"], "routes/photo/[id]/loading.tsx#default");
  assert!(built.files.iter().any(|(name, text)| name == "generated/islands.ts" && text.contains("routes/photo/[id]/page.modal.tsx#default") && text.contains("loading.modal")), "the variant and its loading module mount in the browser");
  std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn slots_and_variants_out_of_place_are_refused() {
  let stray = app(&[("routes/index/page.tsx", PAGE), ("routes/slots/feed/page.tsx", PAGE)]);
  assert!(matches!(fails(&stray), BuildError::SlotsWithoutLayout(_)));

  let empty = app(&[("routes/layout.tsx", LAYOUT), ("routes/index/page.tsx", PAGE), ("routes/slots/feed/page.loader.ts", "export async function load() {\n  return {};\n}\n")]);
  assert!(matches!(fails(&empty), BuildError::SlotWithoutPage(_)));

  let nested = app(&[("routes/layout.tsx", LAYOUT), ("routes/index/page.tsx", PAGE), ("routes/slots/feed/page.tsx", PAGE), ("routes/slots/feed/more/page.tsx", PAGE)]);
  assert!(matches!(fails(&nested), BuildError::SlotRoute(_)));

  let undeclared = app(&[("routes/layout.tsx", LAYOUT), ("routes/index/page.tsx", PAGE), ("routes/photo/page.tsx", PAGE), ("routes/photo/page.drawer.tsx", PAGE)]);
  assert!(matches!(fails(&undeclared), BuildError::SlotUndeclared { slot, .. } if slot == "drawer"));

  let many = app(&[("routes/layout.tsx", LAYOUT), ("routes/index/page.tsx", PAGE), ("routes/photo/page.tsx", PAGE), ("routes/photo/page.modal.tsx", PAGE), ("routes/photo/page.feed.tsx", PAGE)]);
  assert!(matches!(fails(&many), BuildError::ManyVariants(_)));
  for dir in [stray, empty, nested, undeclared, many] {
    std::fs::remove_dir_all(&dir).unwrap();
  }
}
