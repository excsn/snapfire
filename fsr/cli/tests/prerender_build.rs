//! A prerender directory is the build that wrote it: a host booting another
//! build renders live and says so, and a prerender from its own build puts
//! the files back in front.

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

async fn about(host: &Host) -> (bool, String) {
  let response = host.handle(Request::get("/about").body(Bytes::new()).unwrap()).await;
  let from_file = response.headers().get("x-sf-prerendered").is_some();
  (from_file, String::from_utf8_lossy(&response.into_body().collect().await.unwrap().to_bytes()).into_owned())
}

#[test]
fn a_prerender_written_by_another_build_is_ignored_until_this_build_writes_one() {
  let root = project();
  let dir = root.join("app/dist/prerender");
  let runtime = tokio::runtime::Runtime::new().unwrap();

  let host = boot(&root);
  runtime.block_on(host.prerender(&dir)).unwrap();
  assert!(dir.join(snapfire_fsr_host::BUILD_FILE).is_file(), "the build is written beside the documents");
  let (from_file, html) = runtime.block_on(about(&boot(&root)));
  assert!(from_file && html.contains("alpha-copy"), "the same build serves the file: {html}");

  std::fs::write(root.join("app/routes/about/page.tsx"), page("beta-copy")).unwrap();
  let host = boot(&root);
  assert!(host.report().to_string().contains("written by another build"), "{}", host.report());
  let (from_file, html) = runtime.block_on(about(&host));
  assert!(!from_file && html.contains("beta-copy") && !html.contains("alpha-copy"), "another build's file is not served: {html}");

  runtime.block_on(host.prerender(&dir)).unwrap();
  let (from_file, html) = runtime.block_on(about(&host));
  assert!(from_file && html.contains("beta-copy"), "a prerender from this build puts the files in front without a reload: {html}");
  std::fs::remove_dir_all(&root).unwrap();
}
