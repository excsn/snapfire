//! The handbook has no runtime: every route is fixed, so the whole site is
//! files. These tests are the claim, checked.

use std::path::Path;

use futures::executor::block_on;
use snapfire_fsr_host::{Config, Host, RenderMode};

const ROUTES: [&str; 3] = ["/", "/faq", "/install"];

fn handbook() -> Host {
  let config = Config::load(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
  Host::from_config(config).unwrap().build().unwrap()
}

fn out() -> std::path::PathBuf {
  let dir = std::env::temp_dir().join(format!("fsr-handbook-{}-{:?}", std::process::id(), std::time::SystemTime::now()));
  std::fs::create_dir_all(&dir).unwrap();
  dir
}

#[test]
fn every_route_is_prerenderable_so_the_site_has_no_dynamic_part() {
  let host = handbook();
  let mut prerenderable = host.prerenderable();
  prerenderable.sort();
  assert_eq!(prerenderable, ROUTES, "{}", host.report());
  assert!(host.prerenderable_anonymous().is_empty(), "nothing here renders differently for a visitor with an identity");
  assert_eq!(host.report().app.routes.len(), ROUTES.len(), "no route is left out of the count: {}", host.report());
}

#[test]
fn the_whole_site_is_written_to_files_and_each_document_stands_alone() {
  let out = out();
  let host = handbook();
  let written = block_on(host.prerender(&out)).unwrap();

  for route in ROUTES {
    assert_eq!(written.iter().filter(|(pattern, _)| pattern == route).count(), 2, "a document and a payload for {route}: {written:?}");
  }
  for file in ["index.html", "faq/index.html", "install/index.html", "index.payload"] {
    assert!(out.join(file).is_file(), "{file} was written");
  }

  let home = std::fs::read_to_string(out.join("index.html")).unwrap();
  assert!(home.contains("<!doctype html>"), "{home}");
  assert!(home.contains("<title>The FSR handbook</title>"), "the route's own meta titled it: {home}");
  assert!(home.contains("<h1>A site with no server</h1>"), "the page rendered on the server: {home}");
  assert!(home.contains("Routes are files"), "the loader's data is in the markup: {home}");
  assert!(home.contains("<link rel=\"stylesheet\" href=\"/static/css/handbook.css\">"), "{home}");
  assert!(home.contains("<script type=\"module\" src=\"/static/js/app/src/main.js\"></script>"), "{home}");

  assert!(!home.contains("__fsr"), "no development endpoint is baked into a written document: {home}");
  assert!(!home.contains("src=\"http") && !home.contains("href=\"http"), "nothing is loaded from a host: {home}");
  for reference in ["href=\"/static", "src=\"/static", "href=\"/\"", "href=\"/faq\"", "href=\"/install\""] {
    assert!(home.contains(reference), "{reference} is in the document: {home}");
  }

  let faq = std::fs::read_to_string(out.join("faq/index.html")).unwrap();
  assert!(faq.contains("<title>FAQ · The FSR handbook</title>"), "{faq}");
  assert!(faq.contains("Does this site run a server?"), "{faq}");
  assert!(faq.contains("wordmark"), "the layout wraps every page: {faq}");

  std::fs::remove_dir_all(&out).ok();
}

#[test]
fn a_written_document_is_the_same_whoever_asks_for_it() {
  let host = handbook();
  let session = snapfire_fsr_runtime::SessionCell::default();
  session.insert("anything", snapfire_fsr_core::Value::str("at all"));
  let with = block_on(host.render_to_string("/", RenderMode::Html, session)).unwrap();
  let without = block_on(host.render_to_string("/", RenderMode::Html, snapfire_fsr_runtime::SessionCell::default())).unwrap();
  assert_eq!(with, without, "nothing on the plan reads the session, so one render serves everyone");
}
