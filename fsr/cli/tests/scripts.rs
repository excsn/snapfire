//! A script a route's `meta` declares: an inert marker in the document, the
//! same row in a navigation's payload, its origin in the policy and the
//! scripts row of the boot report.

use std::path::{Path, PathBuf};

use snapfire_fsr_cli::new::{create, NewOptions};
use snapfire_fsr_cli::{build, write, Options};
use snapfire_fsr_host::config::Config;
use snapfire_fsr_host::{Host, RenderMode};
use snapfire_fsr_runtime::SessionCell;

static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

const PAGE: &str = "export default function Page() {\n  return <main>map</main>;\n}\n";

fn project(tag: &str, loader: &str) -> PathBuf {
  let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let root = std::env::temp_dir().join(format!("fsr-cli-scripts-{}-{n}-{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&root);
  create(&root, NewOptions::default().fetch(false)).unwrap();
  let app = root.join("app");
  std::fs::write(app.join("routes/page.tsx"), PAGE).unwrap();
  std::fs::write(app.join("routes/page.loader.ts"), loader).unwrap();
  let config = root.join("config/app.toml");
  let text = std::fs::read_to_string(&config).unwrap();
  std::fs::write(&config, format!("{text}\n[document.csp]\nscript-src = [\"'self'\"]\n")).unwrap();
  root
}

fn host(root: &Path) -> Host {
  let app = root.join("app");
  let built = build(&app, &Options::beside(&app)).unwrap_or_else(|e| panic!("builds: {e}"));
  write(&app, &built).unwrap();
  Host::from_config(Config::load(root).unwrap()).unwrap().build().unwrap_or_else(|e| panic!("boots: {e}"))
}

const LOADER: &str = "import { scriptTag } from \"@snapfire/fsr/head\";\nexport async function load() {\n  return { n: 1 };\n}\nexport const meta = () => ({\n  head: [scriptTag(\"https://maps.example/widget.js\", \"interaction\"), { tag: \"script\", src: \"/static/local.js\", crossorigin: \"anonymous\" }],\n});\n";

#[test]
fn a_declared_script_is_an_inert_marker_whose_origin_the_policy_names() {
  let root = project("declared", LOADER);
  let host = host(&root);
  let runtime = tokio::runtime::Runtime::new().unwrap();
  let html = runtime.block_on(host.render_to_string("/", RenderMode::Html, SessionCell::default())).unwrap();
  assert!(html.contains("<script type=\"text/plain\" data-sf-script=\"interaction\" data-src=\"https://maps.example/widget.js\"></script>"), "the helper's row is a marker the client loads: {html}");
  assert!(html.contains("<script type=\"text/plain\" data-sf-script=\"load\" data-src=\"/static/local.js\" crossorigin=\"anonymous\"></script>"), "a row written out takes `load` and keeps its attributes: {html}");
  let report = host.report();
  let policy = report.csp.clone().expect("a policy");
  assert!(policy.contains("https://maps.example"), "the other origin is added to script-src: {policy}");
  assert!(!policy.contains("/static/local.js"), "a path on the document's origin adds nothing: {policy}");
  let scripts = report.app.to_string();
  assert!(scripts.contains("scripts") && scripts.contains("https://maps.example/widget.js") && scripts.contains("/static/local.js"), "{scripts}");
  let payload = runtime.block_on(host.render_to_string("/", RenderMode::Payload, SessionCell::default())).unwrap();
  let head = payload.lines().find(|line| line.starts_with("H ")).expect("a head row");
  assert!(head.contains("\"scripts\"") && head.contains("\"data-src\":\"https://maps.example/widget.js\"") && head.contains("\"data-sf-script\":\"interaction\""), "a navigation gets the rows it cannot read from markup: {head}");
  std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn a_script_row_with_inline_code_is_refused_by_the_build() {
  let root = project("inline", "export async function load() {\n  return { n: 1 };\n}\nexport const meta = () => ({ head: [{ tag: \"script\", children: \"alert(1)\" }] });\n");
  let app = root.join("app");
  let error = build(&app, &Options::beside(&app)).err().expect("refused").to_string();
  assert!(error.contains("routes/page.loader.ts") && error.contains("inline code") && error.contains("src/main.ts"), "{error}");
  std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn a_script_src_computed_at_request_time_is_reported_for_the_policy_to_name() {
  let root = project("computed", "export async function load() {\n  return { cdn: \"https://cdn.example\" };\n}\nexport const meta = ({ data }: { data: { cdn: string } }) => ({ head: [{ tag: \"script\", src: `${data.cdn}/a.js` }] });\n");
  let host = host(&root);
  let report = host.report().app.to_string();
  assert!(report.contains("computed at request time; name its origin in [document.csp] script-src"), "{report}");
  std::fs::remove_dir_all(&root).unwrap();
}
