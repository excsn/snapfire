//! The manifest and the head rows `[pwa]` gives every page, rendered in-process.

use std::path::Path;

use bytes::Bytes;
use futures::executor::block_on;
use http_body_util::BodyExt;
use snapfire_fsr_host::{Config, Host, RenderMode};

fn trails() -> Host {
  let config = Config::load(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
  Host::from_config(config).unwrap().build().unwrap()
}

fn get(host: &Host, path: &str) -> (http::StatusCode, String) {
  block_on(async {
    let response = host.handle(http::Request::get(path).body(Bytes::new()).unwrap()).await;
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&body).into_owned())
  })
}

#[test]
fn every_page_links_the_manifest_written_from_the_pwa_section() {
  let host = trails();
  let html = block_on(host.render_to_string("/", RenderMode::Html, snapfire_fsr_runtime::SessionCell::default())).unwrap();
  assert!(html.contains("<link rel=\"manifest\" href=\"/manifest.webmanifest\">"), "{html}");
  assert!(html.contains("<meta name=\"theme-color\" content=\"#2f5d3a\">"), "{html}");

  let (status, manifest) = get(&host, "/manifest.webmanifest");
  assert_eq!(status, http::StatusCode::OK);
  let manifest: serde_json::Value = serde_json::from_str(&manifest).unwrap();
  assert_eq!(manifest["short_name"], "Trails");
  let sizes: Vec<&str> = manifest["icons"].as_array().unwrap().iter().filter_map(|icon| icon["sizes"].as_str()).collect();
  assert!(sizes.contains(&"192x192") && sizes.contains(&"512x512"), "{manifest}");
}

#[test]
fn a_development_host_registers_no_worker() {
  let host = trails();
  let html = block_on(host.render_to_string("/", RenderMode::Html, snapfire_fsr_runtime::SessionCell::default())).unwrap();
  assert!(!html.contains("sf:worker"), "{html}");
  assert_eq!(get(&host, "/sw.js").0, http::StatusCode::NOT_FOUND);
}
