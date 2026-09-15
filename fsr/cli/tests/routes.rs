use std::path::{Path, PathBuf};

use snapfire_fsr_cli::{build, BuildError, Options};
use snapfire_fsr_ir::{ShadowMode, ShadowRoot};

static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn app(files: &[(&str, &str)]) -> PathBuf {
  let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
  let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let dir = std::env::temp_dir().join(format!("fsr-cli-routes-{}-{n}-{nanos}", std::process::id()));
  std::fs::create_dir_all(dir.join("routes")).unwrap();
  std::fs::write(dir.join("importmap.json"), r#"{"imports":{"@snapfire/fsr-client/react":"/r","react":"/r","react-dom/client":"/d"}}"#).unwrap();
  vendor_react(&dir, "18.3.1");
  for (name, source) in files {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, source).unwrap();
  }
  dir
}

/// What `fsr add` records for the React it vendored.
fn vendor_react(dir: &Path, version: &str) {
  std::fs::create_dir_all(dir.join("vendor")).unwrap();
  std::fs::write(dir.join("vendor/.fsr-vendor.json"), format!(r#"{{"packages":{{"react":{{"version":"{version}"}}}}}}"#)).unwrap();
}

const LAYOUT: &str = "import { Slot } from \"@snapfire/fsr-client/react\";\nexport default function Layout({ children, feed }: { children: unknown; feed: unknown }) {\n  return <div>{children}{feed}<Slot name=\"modal\"><p>closed</p></Slot><Slot name=\"drawer\" /></div>;\n}\n";
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

const HANDLED: &str = "export default function Page() {\n  async function go(): Promise<void> {}\n  return <button onClick={() => void go()}>go</button>;\n}\n";

fn placing(component: &str) -> String {
  format!("import {{ Island }} from \"@snapfire/fsr-client/react\";\nimport Chart from \"../src/ui/{component}\";\nexport default function Page() {{\n  return <Island><Chart /></Island>;\n}}\n")
}

#[test]
fn the_plan_records_the_vendored_react() {
  let dir = app(&[("routes/page.tsx", PAGE)]);
  assert_eq!(build(&dir, &Options::default()).unwrap().manifest.frameworks.get("react").map(String::as_str), Some("18.3.1"));
  vendor_react(&dir, "19.3.0");
  let built = build(&dir, &Options::default()).unwrap();
  assert_eq!(built.manifest.frameworks.get("react").map(String::as_str), Some("19.3.0"));
  assert!(built.manifest.to_sexpr().contains("(framework react"), "{}", built.manifest.to_sexpr());
  std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_map_serving_react_with_no_vendored_version_is_refused() {
  let dir = app(&[("routes/page.tsx", PAGE)]);
  std::fs::remove_file(dir.join("vendor/.fsr-vendor.json")).unwrap();
  match fails(&dir) {
    BuildError::ReactUnrecorded { manifest, version, .. } => {
      assert!(manifest.contains(".fsr-vendor.json"), "{manifest}");
      assert_eq!(version, "18.3.1");
    }
    other => panic!("{other}"),
  }
  std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_react_the_renderer_has_no_rules_for_is_refused() {
  let dir = app(&[("routes/page.tsx", PAGE)]);
  vendor_react(&dir, "17.0.2");
  match fails(&dir) {
    BuildError::ReactMajor { version, supported } => {
      assert_eq!(version, "17.0.2");
      assert_eq!(supported, "18 and 19");
    }
    other => panic!("{other}"),
  }
  std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn an_app_with_no_react_records_no_framework() {
  let dir = app(&[("routes/page.tsx", PAGE)]);
  std::fs::write(dir.join("importmap.json"), r#"{"imports":{}}"#).unwrap();
  std::fs::remove_file(dir.join("vendor/.fsr-vendor.json")).unwrap();
  assert!(build(&dir, &Options::default()).unwrap().manifest.frameworks.is_empty());
  std::fs::remove_dir_all(&dir).unwrap();
}

const GRID: &str = "export default function Grid({ rows, title }: { rows: number[]; title: string }) {\n  return <><b>{title}</b>{rows.length}</>;\n}\n";

#[test]
fn a_custom_element_with_a_template_under_elements_is_marked_where_it_is_placed() {
  let page = "export default function Page({ rows }: { rows: number[] }) {\n  return <x-grid title=\"t\" rows={rows}>light</x-grid>;\n}\n";
  let dir = app(&[("routes/page.tsx", page), ("elements/x-grid.tsx", GRID)]);
  let built = build(&dir, &Options::default()).unwrap();
  let json = built.manifest.to_json();
  assert!(json.contains("\"$shadow\"") && json.contains("elements/x-grid.tsx#default"), "{json}");
  let template = built.manifest.components.iter().find(|c| c.module == "elements/x-grid.tsx#default").expect("the template is lowered");
  assert!(template.body.hydrated_by.is_none(), "an element template is static");
  std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn an_element_template_with_a_handler_is_refused() {
  let dir = app(&[("routes/page.tsx", PAGE), ("elements/x-grid.tsx", HANDLED)]);
  match fails(&dir) {
    BuildError::ElementTemplate { module, .. } => assert_eq!(module, "elements/x-grid.tsx#default"),
    other => panic!("{other}"),
  }
  std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn an_element_template_not_named_for_a_tag_is_refused() {
  let dir = app(&[("routes/page.tsx", PAGE), ("elements/Grid.tsx", GRID)]);
  match fails(&dir) {
    BuildError::ElementName { file } => assert_eq!(file, "Grid.tsx"),
    other => panic!("{other}"),
  }
  std::fs::remove_dir_all(&dir).unwrap();
}

const PLACES_BOX: &str = "export default function Page() {\n  return <x-box>light</x-box>;\n}\n";

fn lowered_box(template: &str) -> (snapfire_fsr_ir::Component, String) {
  let dir = app(&[("routes/page.tsx", PLACES_BOX), ("elements/x-box.tsx", template)]);
  let built = build(&dir, &Options::default()).unwrap();
  std::fs::remove_dir_all(&dir).unwrap();
  let component = built.manifest.components.iter().find(|c| c.module == "elements/x-box.tsx#default").expect("the template is lowered").body.clone();
  (component, built.manifest.to_sexpr())
}

#[test]
fn an_element_template_rooted_in_a_shadow_template_declares_its_shadow_root() {
  let (component, plan) = lowered_box("export default function Box({ title }: { title: string }) {\n  return <template shadowrootmode=\"closed\" shadowrootdelegatesfocus><p>{title}</p></template>;\n}\n");
  assert_eq!(component.shadow, Some(ShadowRoot { mode: ShadowMode::Closed, delegates_focus: true, clonable: false, serializable: false }));
  assert!(!format!("{:?}", component.render).contains("\"template\""), "the root template is the record and not markup: {:?}", component.render);
  assert!(plan.contains("(shadow closed delegatesfocus)"), "{plan}");
}

#[test]
fn an_element_template_is_lowered_without_hoisting() {
  let page = "export default function Page({ rows }: { rows: number[] }) {\n  return <x-grid title=\"t\" rows={rows}>light</x-grid>;\n}\n";
  let dir = app(&[("routes/page.tsx", page), ("elements/x-grid.tsx", GRID)]);
  let built = build(&dir, &Options::default()).unwrap();
  std::fs::remove_dir_all(&dir).unwrap();
  let template = &built.manifest.components.iter().find(|c| c.module == "elements/x-grid.tsx#default").expect("the template is lowered").body;
  let lowered = format!("{:?}", template.render);
  assert!(!lowered.contains("$chunk") && !lowered.contains("Hoist"), "{lowered}");
  assert!(!built.report.hoisted.iter().any(|(module, ..)| module == "elements/x-grid.tsx#default"), "{:?}", built.report.hoisted);
}

#[test]
fn a_shadow_template_deeper_in_an_element_template_is_markup() {
  let (component, _) = lowered_box("export default function Box() {\n  return <div><inner-box><template shadowrootmode=\"open\"><p>in</p></template></inner-box></div>;\n}\n");
  assert_eq!(component.shadow, None);
  assert!(format!("{:?}", component.render).contains("\"template\""), "{:?}", component.render);
}

#[test]
fn an_element_template_whose_root_template_cannot_be_the_shadow_root_is_refused() {
  for (root, says) in [
    ("<template><p>in</p></template>", "has no `shadowrootmode`"),
    ("<template shadowrootmode={mode}><p>in</p></template>", "`shadowrootmode` on its root `<template>` is not the string"),
    ("<template shadowrootmode=\"closed\" id=\"box\"><p>in</p></template>", "`id` on its root `<template>` would be lost"),
    ("<><template shadowrootmode=\"open\"><p>in</p></template><p>beside</p></>", "shares the root with other nodes"),
    ("mode === \"closed\" ? <template shadowrootmode=\"closed\"><p>in</p></template> : <p>in</p>", "or sits in a branch"),
  ] {
    let template = format!("export default function Box({{ mode }}: {{ mode: \"open\" | \"closed\" }}) {{\n  return {root};\n}}\n");
    let dir = app(&[("routes/page.tsx", PLACES_BOX), ("elements/x-box.tsx", &template)]);
    match fails(&dir) {
      BuildError::ElementTemplate { module, reason } => {
        assert_eq!(module, "elements/x-box.tsx#default");
        assert!(reason.contains(says), "{root}: {reason}");
      }
      other => panic!("{root}: {other}"),
    }
    std::fs::remove_dir_all(&dir).unwrap();
  }
}

#[test]
fn each_element_template_types_its_tag_for_the_jsx_the_templates_are_read_as() {
  let dir = app(&[("routes/page.tsx", PLACES_BOX), ("elements/x-box.tsx", GRID)]);
  let declarations = |dir: &Path| build(dir, &Options::default()).unwrap().files.into_iter().find(|(name, _)| name == "generated/elements.d.ts").map(|(_, text)| text).expect("the build declares its element templates");
  let react = declarations(&dir);
  assert!(react.contains("import type Template0 from \"../elements/x-box\";"), "{react}");
  assert!(react.contains("declare module \"react\" {\n  namespace JSX {\n    interface IntrinsicElements {\n      [custom: `${string}-${string}`]: Host & Loose;\n      \"x-box\": Placed<typeof Template0>;\n"), "{react}");
  std::fs::write(dir.join("importmap.json"), r#"{"imports":{}}"#).unwrap();
  std::fs::remove_file(dir.join("vendor/.fsr-vendor.json")).unwrap();
  let dialect = declarations(&dir);
  assert!(dialect.contains("type Host = Attributes;"), "{dialect}");
  assert!(dialect.contains("declare module \"@snapfire/fsr-authoring/template\" {\n  interface ElementTemplates {\n    \"x-box\": Placed<typeof Template0>;\n"), "{dialect}");
  assert!(!dialect.contains("react"), "{dialect}");
  std::fs::remove_file(dir.join("elements/x-box.tsx")).unwrap();
  assert!(declarations(&dir).ends_with("\n\nexport {};\n"), "a dialect app with no template declares nothing");
  std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_vue_island_registers_through_the_vue_adapter_without_react_in_the_map() {
  let page = placing("Chart.vue");
  let dir = app(&[("routes/page.tsx", &page), ("src/ui/Chart.vue", "<template><p /></template>\n")]);
  std::fs::write(dir.join("importmap.json"), r#"{"imports":{"@snapfire/fsr-client/vue":"/v","vue":"/v"}}"#).unwrap();
  let built = build(&dir, &Options::default()).unwrap();
  let islands = built.files.iter().find(|(name, _)| name == "generated/islands.ts").map(|(_, text)| text.clone()).unwrap();
  assert!(islands.contains("import { vueMounter, vuePatcher, vueUnmounter } from \"@snapfire/fsr-client/vue\";"), "{islands}");
  assert!(islands.contains("registerIsland(\"src/ui/Chart.vue#default\""), "{islands}");
  assert!(!islands.contains("reactMounter"), "a page with no React island imports no React adapter: {islands}");
  std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn an_island_whose_framework_has_no_client_adapter_is_refused() {
  let page = placing("Chart.svelte");
  let dir = app(&[("routes/page.tsx", &page), ("src/ui/Chart.svelte", "<p>chart</p>\n")]);
  match fails(&dir) {
    BuildError::NoAdapter { module, ext } => {
      assert_eq!(module, "src/ui/Chart.svelte#default");
      assert_eq!(ext, "svelte");
    }
    other => panic!("{other}"),
  }
  std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn an_island_no_framework_claims_is_refused() {
  let page = placing("Chart.astro");
  let dir = app(&[("routes/page.tsx", &page), ("src/ui/Chart.astro", "<p>chart</p>\n")]);
  match fails(&dir) {
    BuildError::UnknownComponent { module } => assert_eq!(module, "src/ui/Chart.astro#default"),
    other => panic!("{other}"),
  }
  std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn an_island_whose_adapter_the_import_map_cannot_supply_is_refused_by_name() {
  let dir = app(&[("routes/page.tsx", HANDLED)]);
  std::fs::write(dir.join("importmap.json"), r#"{"imports":{"react":"/r"}}"#).unwrap();
  assert_eq!(fails(&dir).to_string(), "`routes/page.tsx#default` mounts through `@snapfire/fsr-client/react`, but the import map does not name `@snapfire/fsr-client/react` or `react-dom/client`");
  std::fs::write(dir.join("importmap.json"), r#"{"imports":{"@snapfire/fsr-client/":"/fsr/","react":"/r","react-dom/client":"/d"}}"#).unwrap();
  build(&dir, &Options::default()).expect("a trailing-slash key covers the adapter beneath it");
  std::fs::remove_file(dir.join("importmap.json")).unwrap();
  build(&dir, &Options::default()).expect("an app with no import map has nothing to check against");
  std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_site_is_checked_against_the_shells_import_map_as_well_as_its_own() {
  let dir = app(&[("routes/page.tsx", HANDLED)]);
  std::fs::write(dir.join("importmap.json"), r#"{"imports":{}}"#).unwrap();
  let shell = dir.join("shell.json");
  std::fs::write(&shell, r#"{"version":1,"imports":{"@snapfire/fsr-client/react":"/r","react":"/r","react-dom/client":"/d"}}"#).unwrap();
  let mut options = Options::default();
  options.site = Some(snapfire_fsr_cli::SiteOptions { name: "billing".to_owned(), at: "/billing".to_owned(), shell: Some(shell) });
  build(&dir, &options).expect("the shell serves React, so the site need not");
  std::fs::remove_dir_all(&dir).unwrap();
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
  assert_eq!(layout["keep"], serde_json::json!(["content", "feed", "drawer"]), "the page and the other slots stay as the browser has them");
  assert_eq!(layout["children"].as_array().unwrap().len(), 1);
  assert_eq!(layout["children"][0]["slot"], "modal");
  let variant = &layout["children"][0]["node"];
  assert_eq!(variant["module"], "routes/photo/[id]/page.modal.tsx#default");
  assert_eq!(variant["deferred"], true);
  assert_eq!(variant["fallback"], "routes/photo/[id]/loading.modal.tsx#default", "a variant streams behind its own loading module, not the page's");
  let route = plan["routes"].as_array().unwrap().iter().find(|r| r["pattern"] == "/photo/{id}").unwrap();
  assert_eq!(route["plan"]["children"][0]["node"]["children"][0]["node"]["fallback"], "routes/photo/[id]/loading.tsx#default");
  let islands = built.files.iter().find(|(name, _)| name == "generated/islands.ts").map(|(_, text)| text.clone()).unwrap();
  assert!(!islands.contains("page.modal") && !islands.contains("loading.modal"), "a variant with no state is static, so nothing registers it: {islands}");
  assert!(built.report.components.iter().any(|(module, owner, detail)| module == "routes/photo/[id]/page.modal.tsx#default" && owner == "lowered" && detail == "static"), "{}", built.report);
  std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_page_with_a_handler_hydrates_and_a_component_placed_as_an_island_is_registered_whatever_it_holds() {
  let dir = app(&[
    (
      "routes/index/page.tsx",
      "import { Island } from \"@snapfire/fsr-client/react\";\nimport { Tips } from \"../../src/ui/Tips\";\nexport default function Page() {\n  async function go(): Promise<void> {}\n  return <div><button onClick={() => void go()}>go</button><Island when=\"idle\"><Tips /></Island></div>;\n}\n",
    ),
    ("src/ui/Tips.tsx", "export function Tips() {\n  return <details><summary>tips</summary></details>;\n}\n"),
  ]);
  let built = build(&dir, &Options::default()).unwrap();
  let islands = built.files.iter().find(|(name, _)| name == "generated/islands.ts").map(|(_, text)| text.clone()).unwrap();
  assert!(islands.contains("registerIsland(\"routes/index/page.tsx#default\""), "a handler the lowerer cannot lower is still the browser's to run: {islands}");
  assert!(islands.contains("registerIsland(\"src/ui/Tips.tsx#Tips\""), "the page placed it as an island, so it mounts though it holds nothing: {islands}");
  assert!(!built.report.components.iter().any(|(module, _, detail)| (module == "routes/index/page.tsx#default" || module == "src/ui/Tips.tsx#Tips") && detail == "static"), "{}", built.report);
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

  let undeclared = app(&[("routes/layout.tsx", LAYOUT), ("routes/index/page.tsx", PAGE), ("routes/photo/page.tsx", PAGE), ("routes/photo/page.panel.tsx", PAGE)]);
  assert!(matches!(fails(&undeclared), BuildError::SlotUndeclared { slot, .. } if slot == "panel"));

  for dir in [stray, empty, nested, undeclared] {
    std::fs::remove_dir_all(&dir).unwrap();
  }
}

#[test]
fn a_route_may_carry_a_variant_per_slot() {
  let dir = app(&[("routes/layout.tsx", LAYOUT), ("routes/index/page.tsx", PAGE), ("routes/photo/page.tsx", PAGE), ("routes/photo/page.modal.tsx", PAGE), ("routes/photo/page.drawer.tsx", PAGE)]);
  let built = build(&dir, &Options::default()).unwrap();
  assert_eq!(built.report.intercepts.iter().map(|(a, _)| a.as_str()).collect::<Vec<_>>(), vec!["/photo into drawer", "/photo into modal"], "one entry per variant, in file order");
  let plan = plan_json(&dir);
  let slots: Vec<String> = plan["intercepts"].as_array().unwrap().iter().map(|e| e["plan"]["children"][0]["node"]["children"][0]["slot"].as_str().unwrap().to_owned()).collect();
  assert_eq!(slots, vec!["drawer", "modal"]);
  assert_eq!(plan["intercepts"][0]["plan"]["children"][0]["node"]["keep"], serde_json::json!(["content", "modal"]), "the other variant's slot is kept, and `feed` is no slot without a slots/ directory");
  std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_loaders_store_export_lowers_beside_its_meta() {
  let dir = app(&[
    ("routes/layout.tsx", LAYOUT),
    (
      "routes/layout.loader.ts",
      "export async function load() {\n  return { count: 2 };\n}\nexport const meta = ({ data }: { data: { count: number } }) => ({ title: `${data.count} in the cart` });\nexport const store = ({ data }: { data: { count: number } }) => ({ \"cart/count\": data.count });\n",
    ),
    ("routes/index/page.tsx", PAGE),
  ]);
  let plan = plan_json(&dir);
  let layout = plan["sources"].as_array().unwrap().iter().find(|s| s["id"] == "layout").expect("the layout is a source");
  assert!(layout["meta"].is_array(), "{layout}");
  let store = &layout["store"][0]["return"]["object"][0]["field"];
  assert_eq!(store[0], "cart/count", "{layout}");
}

#[test]
fn a_site_build_prefixes_every_id_and_puts_every_pattern_under_its_prefix() {
  let dir = app(&[
    ("routes/layout.tsx", LAYOUT),
    ("routes/page.tsx", "import { useState } from \"react\";\nimport { TipList } from \"../src/Tips\";\nexport default function Page() {\n  const [open, setOpen] = useState(false);\n  return <section><button onClick={() => setOpen(!open)}>{open ? \"hide\" : \"show\"}</button><div><TipList /></div></section>;\n}\n"),
    ("routes/page.loader.ts", "import type { Ctx } from \"@snapfire/fsr\";\nexport async function load(ctx: Ctx<\"/\">) {\n  return { items: await ctx.services.ledger.list({}) };\n}\n"),
    ("routes/actions.ts", "import { action } from \"@snapfire/fsr\";\nimport type { ActionCtx } from \"@snapfire/fsr\";\nimport type { Add } from \"../schemas/inputs\";\nexport const add = action(async ({ input }: ActionCtx<Add>) => {\n  return input.n;\n});\n"),
    ("schemas/inputs.ts", "export interface Add {\n  n: number;\n}\n"),
    ("routes/api/ping/route.ts", "import type { Ctx } from \"@snapfire/fsr\";\nexport async function GET(ctx: Ctx) {\n  return { ok: true, locale: ctx.locale };\n}\n"),
    ("src/Tips.tsx", "import { money } from \"./money\";\nexport function TipList({ cents = 5 }: { cents?: number }) {\n  return <ul>{money(cents)}</ul>;\n}\n"),
    ("src/money.ts", "export function money(cents: number): string {\n  return `$${cents}`;\n}\n"),
    ("clients/ledger.openapi.json", r##"{"openapi":"3.0.0","info":{"title":"ledger","version":"1"},"paths":{"/list":{"get":{"operationId":"list","responses":{"200":{"description":"ok","content":{"application/json":{"schema":{"type":"array","items":{"$ref":"#/components/schemas/Invoice"}}}}}}}}},"components":{"schemas":{"Invoice":{"type":"object","required":["id"],"properties":{"id":{"type":"integer","format":"int64"}}}}}}"##),
  ]);
  let mut options = Options::default();
  options.site = Some(snapfire_fsr_cli::SiteOptions { name: "billing".to_owned(), at: "/billing".to_owned(), shell: None });
  let built = build(&dir, &options).unwrap();
  let plan = built.manifest.to_json();
  for expected in [
    "\"pattern\": \"/billing\"",
    "\"pattern\": \"/billing/api/ping\"",
    "\"module\": \"shell#document\"",
    "\"module\": \"billing:routes/layout.tsx#default\"",
    "\"id\": \"billing:$root\"",
    "\"id\": \"billing:$root.add\"",
    "\"id\": \"billing:api.ping.GET\"",
    "\"service\": \"billing:ledger\"",
    "\"module\": \"billing:src/Tips.tsx#TipList\"",
  ] {
    assert!(plan.contains(expected), "missing {expected} in {plan}\n{}", built.report);
  }
  assert!(built.contract.services.contains_key("billing:ledger") && built.contract.types.contains_key("billing:Invoice"), "{:?}", built.contract.services.keys().collect::<Vec<_>>());
  let file = |name: &str| built.files.iter().find(|(n, _)| n == name).map(|(_, t)| t.clone()).unwrap();
  let islands = file("generated/islands.ts");
  assert!(islands.contains("registerIsland(\"billing:routes/page.tsx#default\", { loader: () => import(\"../routes/page.js\")"), "{islands}");
  let client = file("generated/client.ts");
  assert!(client.contains("call(\"billing:$root.add\")") && client.contains("  $root: {"), "{client}");
  let declarations = file("generated/services.d.ts");
  assert!(declarations.contains("ledger") && !declarations.contains("billing:"), "the TypeScript surface keeps the unprefixed names: {declarations}");
  let fsr = file("generated/fsr.ts");
  assert!(fsr.contains("\"/\":") && !fsr.contains("/billing"), "route keys stay as written: {fsr}");
  let overlay = file(".fsr-bundle/src/Tips.tsx");
  assert!(overlay.contains("__sfUseHoisted(\"billing:src/Tips.tsx#TipList\")") && overlay.contains("__sfh.r(0, () => (money(cents)))"), "the reader keys under the prefixed module the host renders: {overlay}");
  assert_eq!(built.report.hoisted, vec![("billing:routes/page.tsx#default".to_owned(), 0, 1), ("billing:src/Tips.tsx#TipList".to_owned(), 1, 1)], "the page's div around the pure TipList is a subtree of its own: {}", built.report);
  std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_shell_emits_its_contract_and_a_site_built_against_it_gets_the_declarations() {
  let shell = app(&[
    ("routes/layout.tsx", LAYOUT),
    ("routes/layout.loader.ts", "export async function load() {\n  return { count: 2, who: \"norm\" };\n}\nexport const store = ({ data }: { data: { count: number; who: string } }) => ({ \"cart/count\": data.count, \"session/who\": data.who });\n"),
    ("routes/index/page.tsx", PAGE),
    ("importmap.json", r#"{"imports":{"react":"/static/js/vendor/react/react.bundle.mjs","@snapfire/fsr-client":"/static/js/fsr/index.js"}}"#),
  ]);
  let built = build(&shell, &Options::default()).unwrap();
  let (_, contract) = built.files.iter().find(|(n, _)| n == "generated/shell.json").expect("a shell writes its contract");
  let json: serde_json::Value = serde_json::from_str(contract).unwrap();
  assert_eq!(json["version"], 1);
  assert_eq!(json["store"]["cart/count"], "number");
  assert_eq!(json["store"]["session/who"], "string");
  assert_eq!(json["imports"]["react"], "/static/js/vendor/react/react.bundle.mjs");
  std::fs::create_dir_all(shell.join("generated")).unwrap();
  std::fs::write(shell.join("generated/shell.json"), contract).unwrap();

  let site = app(&[
    ("routes/index/page.tsx", PAGE),
    ("importmap.json", r#"{"imports":{"react":"/elsewhere/react.mjs","chart":"/billing/static/js/vendor/chart.mjs"}}"#),
  ]);
  let mut options = Options::default();
  options.site = Some(snapfire_fsr_cli::SiteOptions { name: "billing".to_owned(), at: "/billing".to_owned(), shell: Some(shell.join("generated/shell.json")) });
  let built = build(&site, &options).unwrap();
  assert!(built.files.iter().all(|(n, _)| n != "generated/shell.json"), "a site emits no contract of its own");
  let (_, declarations) = built.files.iter().find(|(n, _)| n == "generated/shell.d.ts").expect("a site gets the shell's declarations");
  assert!(declarations.contains("export interface ShellStore {\n  \"cart/count\": number;\n  \"session/who\": string;\n}"), "{declarations}");
  assert!(declarations.contains("export type ShellImport = \"@snapfire/fsr-client\" | \"react\";"), "{declarations}");
  let report = built.report.to_string();
  assert!(report.contains("shell     ") && report.contains("2 store keys, 2 imports") && report.contains("react mapped differently here"), "{report}");
  std::fs::remove_dir_all(&shell).unwrap();
  std::fs::remove_dir_all(&site).unwrap();
}

#[test]
fn a_route_and_its_parameterised_child_get_ids_of_their_own() {
  let dir = app(&[
    ("routes/agents/page.tsx", PAGE),
    ("routes/agents/page.loader.ts", "export async function load() {\n  return { list: 1 };\n}\n"),
    ("routes/agents/[id]/page.tsx", PAGE),
    ("routes/agents/[id]/page.loader.ts", "export async function load() {\n  return { one: 2 };\n}\n"),
    ("routes/docs/[...rest]/page.tsx", PAGE),
    ("routes/docs/[...rest]/page.loader.ts", "export async function load() {\n  return { path: \"x\" };\n}\n"),
  ]);
  let built = build(&dir, &Options::default()).unwrap();

  let sources: Vec<&str> = built.report.sources.iter().map(|(id, _)| id.as_str()).collect();
  assert!(sources.contains(&"agents") && sources.contains(&"agents.$id"), "{sources:?}");
  assert!(sources.contains(&"docs.$rest"), "a catch-all is a parameter like any other: {sources:?}");

  let client = &built.files.iter().find(|(name, _)| name == "generated/client.ts").unwrap().1;
  assert!(client.contains("export type AgentsProps = { list: number };"), "{client}");
  assert!(client.contains("export type AgentsIdProps = { one: number };"), "the marker is not part of the name: {client}");
}

#[test]
fn a_directory_named_after_the_parameter_beside_it_is_refused_on_the_type_name() {
  let dir = app(&[("routes/agents/page.tsx", PAGE), ("routes/agents/x/page.tsx", PAGE), ("routes/agents/[x]/page.tsx", PAGE)]);
  match fails(&dir) {
    BuildError::ClaimedId { kind, id, first, second } => {
      assert_eq!(kind, "props type");
      assert_eq!(id, "AgentsXProps");
      assert!([first.as_str(), second.as_str()] == ["agents.x", "agents.$x"], "the ids differ; the name the marker is dropped from does not: {first} and {second}");
    }
    other => panic!("{other}"),
  }
}

#[test]
fn two_rows_claiming_one_id_are_refused_by_name() {
  let dir = app(&[
    ("routes/layout.tsx", LAYOUT),
    ("routes/index/page.tsx", PAGE),
    ("routes/slots/promo/page.tsx", PAGE),
    ("routes/slots/promo/page.loader.ts", "export async function load() {\n  return { a: 1 };\n}\n"),
    ("routes/layout/promo/page.tsx", PAGE),
    ("routes/layout/promo/page.loader.ts", "export async function load() {\n  return { b: 2 };\n}\n"),
  ]);
  match fails(&dir) {
    BuildError::ClaimedId { kind, id, first, second } => {
      assert_eq!(kind, "source");
      assert_eq!(id, "layout.promo", "a slot under the root layout and a route at `layout/promo` derive the same id");
      assert!(first.contains("promo") && second.contains("promo"), "{first} and {second}");
    }
    other => panic!("{other}"),
  }
}

#[test]
fn an_island_in_server_mode_is_refused_over_a_handler_that_did_not_lower_or_an_impure_component_inside() {
  let page = "import { Island } from \"@snapfire/fsr-client/react\";\nimport { Widget } from \"../../src/Widget\";\nexport default function Page() {\n  return <Island mode=\"server\"><Widget /></Island>;\n}\n";
  let shouting = app(&[
    ("routes/layout.tsx", LAYOUT),
    ("routes/index/page.tsx", page),
    ("src/Widget.tsx", "import { useState } from \"react\";\nexport function Widget() {\n  const [n, setN] = useState(0);\n  return <button onClick={() => alert(n)}>{n}</button>;\n}\n"),
  ]);
  let err = match build(&shouting, &Options::default()) {
    Err(e) => e.to_string(),
    Ok(_) => panic!("built"),
  };
  assert!(err.contains("`src/Widget.tsx#Widget` cannot be an island in server mode") && err.contains("a handler did not lower") && err.contains("a call to `alert`"), "{err}");
  std::fs::remove_dir_all(&shouting).unwrap();

  // A handler that closes over what the markup bound: the browser would keep
  // it in the closure, the host has no such name when the step runs.
  let captured = app(&[
    ("routes/layout.tsx", LAYOUT),
    ("routes/index/page.tsx", page),
    (
      "src/Widget.tsx",
      "import { useState } from \"react\";\nexport function Widget({ rows }: { rows: number[] }) {\n  const [n, setN] = useState(0);\n  return <ul>{rows.map((row) => <li key={row}><button onClick={() => setN(row)}>{n}</button></li>)}</ul>;\n}\n",
    ),
  ]);
  let err = match build(&captured, &Options::default()) {
    Err(e) => e.to_string(),
    Ok(_) => panic!("built"),
  };
  assert!(
    err.contains("`src/Widget.tsx#Widget` cannot be an island in server mode") && err.contains("handler 0 reads `row`") && err.contains("read it from `e.target`"),
    "{err}"
  );
  std::fs::remove_dir_all(&captured).unwrap();

  let nested = app(&[
    ("routes/layout.tsx", LAYOUT),
    ("routes/index/page.tsx", page),
    ("src/Widget.tsx", "import { useState } from \"react\";\nimport { Inner } from \"./Inner\";\nexport function Widget() {\n  const [n, setN] = useState(0);\n  return <div><button onClick={() => setN(n + 1)}>{n}</button><Inner /></div>;\n}\n"),
    ("src/Inner.tsx", "import { useState } from \"react\";\nexport function Inner() {\n  const [x, setX] = useState(0);\n  return <i onClick={() => setX(x + 1)}>{x}</i>;\n}\n"),
  ]);
  let err = match build(&nested, &Options::default()) {
    Err(e) => e.to_string(),
    Ok(_) => panic!("built"),
  };
  assert!(err.contains("`src/Inner.tsx#Inner` inside it has state or handlers of its own"), "{err}");
  std::fs::remove_dir_all(&nested).unwrap();

  let fine = app(&[
    ("routes/layout.tsx", LAYOUT),
    ("routes/index/page.tsx", page),
    ("src/Widget.tsx", "import { useState } from \"react\";\nimport { Inner } from \"./Inner\";\nexport function Widget() {\n  const [n, setN] = useState(0);\n  return <div><button onClick={() => setN(n + 1)}>{n}</button><Inner n={n} /></div>;\n}\n"),
    ("src/Inner.tsx", "export function Inner({ n }: { n: number }) {\n  return <i>{n * 2}</i>;\n}\n"),
  ]);
  let built = build(&fine, &Options::default()).unwrap();
  assert_eq!(built.report.islands, vec![("src/Widget.tsx#Widget".to_owned(), 1)], "{}", built.report);
  assert!(built.report.to_string().contains("islands   src/Widget.tsx#Widget              server      1 handler"), "{}", built.report);
  std::fs::remove_dir_all(&fine).unwrap();
}

/// A step renders the island with nothing on the slot stack, so a slot in its
/// markup comes back empty and the patch removes whatever filled it.
#[test]
fn an_island_in_server_mode_is_refused_over_a_slot_it_renders() {
  let page = "import { Island } from \"@snapfire/fsr-client/react\";\nimport { Widget } from \"../../src/Widget\";\nexport default function Page() {\n  return <Island mode=\"server\"><Widget><b>held</b></Widget></Island>;\n}\n";
  let slotted = app(&[
    ("routes/layout.tsx", LAYOUT),
    ("routes/index/page.tsx", page),
    ("src/Widget.tsx", "import { useState } from \"react\";\nexport function Widget({ children }: { children: unknown }) {\n  const [n, setN] = useState(0);\n  return <div><button onClick={() => setN(n + 1)}>{n}</button>{children}</div>;\n}\n"),
  ]);
  let err = match build(&slotted, &Options::default()) {
    Err(e) => e.to_string(),
    Ok(_) => panic!("built"),
  };
  assert!(err.contains("`src/Widget.tsx#Widget` cannot be an island in server mode") && err.contains("renders `children`, which a step would drop"), "{err}");
  std::fs::remove_dir_all(&slotted).unwrap();
}

#[test]
fn extensions_under_ext_are_reported_and_a_render_path_body_member_or_an_unlowerable_export_fails_the_build() {
  let page = "import { intl } from \"@snapfire/fsr-client/std\";\nimport { weight } from \"@ext/fmt\";\nimport { useState } from \"react\";\nexport default function Page({ grams }: { grams: number }) {\n  const [n, setN] = useState(1);\n  return <p onClick={() => setN(n + 1)} title={weight(grams)}>{intl.number(n * grams)}</p>;\n}\n";
  let fine = app(&[
    ("routes/layout.tsx", LAYOUT),
    ("routes/index/page.tsx", page),
    ("routes/index/page.loader.ts", "import { weight } from \"@ext/fmt\";\nimport { time } from \"@snapfire/fsr-client/std\";\nexport async function load() {\n  return { grams: 1500, label: weight(1500), at: time.now() };\n}\n"),
    ("ext/fmt.ts", "import { intl, native } from \"@snapfire/fsr-client/std\";\nexport function weight(grams: number): string {\n  return `${intl.number(grams / 1000)} kg`;\n}\nexport const pretty = native(\"fmt.pretty\", (n: number) => String(n));\nexport const stamp = native<() => string>(\"fmt.stamp\");\n"),
  ]);
  let built = build(&fine, &Options::default()).unwrap();
  assert_eq!(
    built.report.extensions,
    vec![("ext/fmt.ts#weight".to_owned(), "lowered".to_owned()), ("ext/fmt.ts#pretty".to_owned(), "native render".to_owned()), ("ext/fmt.ts#stamp".to_owned(), "native body".to_owned())]
  );
  assert_eq!(built.report.browser, vec![("routes/index/page.tsx#default".to_owned(), "routes/index/page.tsx:6:64".to_owned())], "{}", built.report);
  assert_eq!(built.report.hoisted, vec![("routes/index/page.tsx#default".to_owned(), 1, 0)], "{}", built.report);
  let text = built.report.to_string();
  assert!(text.contains("extensions ext/fmt.ts#weight      lowered") && text.contains("browser   routes/index/page.tsx#default      routes/index/page.tsx:6:64"), "{text}");
  let plan = built.manifest.to_json();
  assert!(plan.contains("\"ext\"") && plan.contains("\"intl\"") && plan.contains("\"time\""), "{plan}");
  std::fs::remove_dir_all(&fine).unwrap();

  let reach = app(&[("routes/layout.tsx", LAYOUT), ("routes/index/page.tsx", "import { id } from \"@snapfire/fsr-client/std\";\nexport default function Page() {\n  return <p>{id.new()}</p>;\n}\n")]);
  let err = fails(&reach).to_string();
  assert!(err.contains("routes/index/page.tsx:3:14: `id.new` on a render path") && err.contains("runs on the server only"), "{err}");
  std::fs::remove_dir_all(&reach).unwrap();

  let unlowerable = app(&[("routes/layout.tsx", LAYOUT), ("routes/index/page.tsx", PAGE), ("ext/bad.ts", "export function stamp(): string {\n  return new Date().toISOString();\n}\n")]);
  let err = fails(&unlowerable).to_string();
  assert!(err.contains("ext/bad.ts:2:") && err.contains("must lower"), "{err}");
  std::fs::remove_dir_all(&unlowerable).unwrap();
}
