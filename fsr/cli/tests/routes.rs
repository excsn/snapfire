use std::path::{Path, PathBuf};

use snapfire_fsr_cli::{build, BuildError, Options};

static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn app(files: &[(&str, &str)]) -> PathBuf {
  let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
  let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let dir = std::env::temp_dir().join(format!("fsr-cli-routes-{}-{n}-{nanos}", std::process::id()));
  std::fs::create_dir_all(dir.join("routes")).unwrap();
  std::fs::write(dir.join("importmap.json"), r#"{"imports":{}}"#).unwrap();
  for (name, source) in files {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, source).unwrap();
  }
  dir
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
    ("routes/index/page.tsx", "import { TipList } from \"../../src/Tips\";\nexport default function Page() {\n  return <div><TipList /></div>;\n}\n"),
    ("routes/index/page.loader.ts", "import type { Ctx } from \"@snapfire/fsr\";\nexport async function load(ctx: Ctx<\"/\">) {\n  return { items: await ctx.services.ledger.list({}) };\n}\n"),
    ("routes/index/actions.ts", "import { action } from \"@snapfire/fsr\";\nimport type { ActionCtx } from \"@snapfire/fsr\";\nimport type { Add } from \"../../schemas/inputs\";\nexport const add = action(async ({ input }: ActionCtx<Add>) => {\n  return input.n;\n});\n"),
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
    "\"id\": \"billing:index\"",
    "\"id\": \"billing:index.add\"",
    "\"id\": \"billing:api.ping.GET\"",
    "\"service\": \"billing:ledger\"",
    "\"module\": \"billing:src/Tips.tsx#TipList\"",
  ] {
    assert!(plan.contains(expected), "missing {expected} in {plan}\n{}", built.report);
  }
  assert!(built.contract.services.contains_key("billing:ledger") && built.contract.types.contains_key("billing:Invoice"), "{:?}", built.contract.services.keys().collect::<Vec<_>>());
  let file = |name: &str| built.files.iter().find(|(n, _)| n == name).map(|(_, t)| t.clone()).unwrap();
  let islands = file("generated/islands.ts");
  assert!(islands.contains("registerIsland(\"billing:routes/index/page.tsx#default\", { loader: () => import(\"../routes/index/page.js\")"), "{islands}");
  let client = file("generated/client.ts");
  assert!(client.contains("call(\"billing:index.add\")") && client.contains("  index: {"), "{client}");
  let declarations = file("generated/services.d.ts");
  assert!(declarations.contains("ledger") && !declarations.contains("billing:"), "the TypeScript surface keeps the unprefixed names: {declarations}");
  let fsr = file("generated/fsr.ts");
  assert!(fsr.contains("\"/\":") && !fsr.contains("/billing"), "route keys stay as written: {fsr}");
  let overlay = file(".fsr-bundle/src/Tips.tsx");
  assert!(overlay.contains("__sfUseHoisted(\"billing:src/Tips.tsx#TipList\")") && overlay.contains("__sfh.r(0, () => (money(cents)))"), "the reader keys under the prefixed module the host renders: {overlay}");
  assert_eq!(built.report.hoisted, vec![("billing:routes/index/page.tsx#default".to_owned(), 0, 1), ("billing:src/Tips.tsx#TipList".to_owned(), 1, 1)], "the page's div around the pure TipList is a subtree of its own: {}", built.report);
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
