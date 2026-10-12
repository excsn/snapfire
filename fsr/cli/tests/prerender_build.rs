//! Production answers from this build's prerendered files and never another
//! build's; development renders live and, asked, renders what a prerender
//! would write.

use std::path::{Path, PathBuf};

use bytes::Bytes;
use http::Request;
use http_body_util::BodyExt;
use snapfire_fsr_cli::new::{create, NewOptions};
use snapfire_fsr_cli::{build, write, Options};
use snapfire_fsr_host::config::Config;
use snapfire_fsr_host::Host;

fn page(text: &str) -> String {
  format!("export default function Page() {{\n  return <main>{text}</main>;\n}}\n")
}

fn project() -> PathBuf {
  let root = std::env::temp_dir().join(format!("fsr-cli-prerender-build-{}", std::process::id()));
  let _ = std::fs::remove_dir_all(&root);
  create(&root, NewOptions { fetch: false, ..NewOptions::default() }).unwrap();
  std::fs::create_dir_all(root.join("app/routes/about")).unwrap();
  std::fs::write(root.join("app/routes/about/page.tsx"), page("alpha-copy")).unwrap();
  let config = root.join("config/app.toml");
  let text = std::fs::read_to_string(&config).unwrap().replacen("[server]\n", "[server]\nprerender = \"dist/prerender\"\n", 1);
  std::fs::write(&config, text).unwrap();
  root
}

fn boot(root: &Path) -> Host {
  let app = root.join("app");
  let built = build(&app, &Options::beside(&app)).unwrap_or_else(|e| panic!("builds: {e}"));
  write(&app, &built).unwrap();
  Host::from_config(Config::load(root).unwrap()).unwrap().build().unwrap_or_else(|e| panic!("boots: {e}"))
}

async fn get(host: &Host, path: &str) -> (u16, bool, String) {
  let response = host.handle(Request::get(path).body(Bytes::new()).unwrap()).await;
  let status = response.status().as_u16();
  let from_file = response.headers().get("x-sf-prerendered").is_some();
  (status, from_file, String::from_utf8_lossy(&response.into_body().collect().await.unwrap().to_bytes()).into_owned())
}

fn production(root: &Path) -> Host {
  let mut config = Config::load(root).unwrap();
  config.server.dev = Some(false);
  config.session.key = Some("a-test-key-of-sixty-four-characters-for-the-production-boot-0123".to_owned());
  Host::from_config(config).unwrap().build().unwrap_or_else(|e| panic!("boots: {e}"))
}

#[test]
fn development_renders_live_and_previews_what_a_prerender_would_write_when_asked() {
  let root = project();
  std::fs::create_dir_all(root.join("app/routes/cart")).unwrap();
  std::fs::write(root.join("app/routes/cart/page.tsx"), page("cart")).unwrap();
  std::fs::write(root.join("app/routes/cart/page.loader.ts"), "import type { Ctx } from \"@snapfire/fsr\";\nexport async function load({ session }: Ctx) {\n  return { who: session.id };\n}\n").unwrap();
  let dir = root.join("app/dist/prerender");
  let runtime = tokio::runtime::Runtime::new().unwrap();

  let host = boot(&root);
  assert!(!dir.exists(), "nothing has been prerendered");
  let (_, from_file, html) = runtime.block_on(get(&host, "/about"));
  assert!(!from_file && html.contains("alpha-copy"), "development renders live: {html}");
  let (status, preview, html) = runtime.block_on(get(&host, "/about?__prerendered"));
  assert!(status == 200 && preview && html.contains("alpha-copy") && !html.contains("EventSource"), "a preview with no run behind it is what a run would write, plain head and all: {html}");
  let (status, _, why) = runtime.block_on(get(&host, "/cart?__prerendered"));
  assert!(status == 404 && why.contains("`/cart` is not prerendered"), "a route that reads the request says why: {why}");

  std::fs::write(root.join("app/routes/about/page.tsx"), page("beta-copy")).unwrap();
  let host = boot(&root);
  let (_, _, html) = runtime.block_on(get(&host, "/about?__prerendered"));
  assert!(html.contains("beta-copy") && !html.contains("alpha-copy"), "an edit shows in the preview at once: {html}");

  runtime.block_on(host.prerender(&dir)).unwrap();
  assert!(dir.join(snapfire_fsr_host::BUILD_FILE).is_file(), "the build is written beside the documents");
  std::fs::write(root.join("app/routes/about/page.tsx"), page("gamma-copy")).unwrap();
  let host = boot(&root);
  assert!(host.report().to_string().contains("written by another build"), "{}", host.report());
  let (_, _, html) = runtime.block_on(get(&host, "/about?__prerendered"));
  assert!(html.contains("gamma-copy"), "the preview never answers from an older run's file: {html}");

  runtime.block_on(host.prerender(&dir)).unwrap();
  let host = production(&root);
  let (_, from_file, html) = runtime.block_on(get(&host, "/about"));
  assert!(from_file && html.contains("gamma-copy"), "production answers from this build's file unasked: {html}");
  std::fs::remove_dir_all(&root).unwrap();
}
