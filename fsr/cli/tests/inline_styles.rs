//! A `<style>` a component writes with literal content: its hash in `style-src`, so a policy that restricts styles still lets the page's own stylesheet apply.

use std::path::PathBuf;

use base64::Engine;
use sha2::Digest;
use snapfire_fsr_cli::new::{create, NewOptions};
use snapfire_fsr_cli::{build, write, Options};
use snapfire_fsr_host::config::Config;
use snapfire_fsr_host::{Host, RenderMode};
use snapfire_fsr_runtime::SessionCell;

const PAGE: &str = r#"export default function Page() {
  return (
    <main>
      <style>{"main > p { color: rebeccapurple }"}</style>
      <p>styled</p>
    </main>
  );
}
"#;

fn project(policy: &str) -> PathBuf {
  static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
  let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let root = std::env::temp_dir().join(format!("fsr-cli-inline-styles-{}-{n}", std::process::id()));
  let _ = std::fs::remove_dir_all(&root);
  create(&root, NewOptions::default().fetch(false)).unwrap();
  std::fs::write(root.join("app/routes/page.tsx"), PAGE).unwrap();
  let config = root.join("config/app.toml");
  let text = std::fs::read_to_string(&config).unwrap();
  std::fs::write(&config, format!("{text}\n[document.csp]\n{policy}\n")).unwrap();
  root
}

fn served(root: &PathBuf) -> (String, String) {
  let app = root.join("app");
  let built = build(&app, &Options::beside(&app)).unwrap_or_else(|e| panic!("builds: {e}"));
  write(&app, &built).unwrap();
  let host = Host::from_config(Config::load(root).unwrap()).unwrap().build().unwrap_or_else(|e| panic!("boots: {e}"));
  let html = tokio::runtime::Runtime::new().unwrap().block_on(host.render_to_string("/", RenderMode::Html, SessionCell::default())).unwrap();
  (html, host.report().csp.clone().expect("a policy"))
}

fn written_style(html: &str) -> String {
  let start = html.find("<style>").map(|i| i + "<style>".len()).unwrap_or_else(|| panic!("a style element: {html}"));
  let end = start + html[start..].find("</style>").expect("closed");
  html[start..end].to_owned()
}

#[test]
fn a_literal_style_element_is_hashed_into_style_src() {
  let root = project("default-src = [\"'self'\"]");
  let (html, policy) = served(&root);
  let css = written_style(&html);
  let hash = format!("'sha256-{}'", base64::engine::general_purpose::STANDARD.encode(sha2::Sha256::digest(css.as_bytes())));
  let style_src = policy.split(';').map(str::trim).find(|d| d.starts_with("style-src ")).unwrap_or_else(|| panic!("a style-src: {policy}"));
  assert!(style_src.contains(&hash), "the hash of what the server wrote, {css:?}, is in {style_src}");
  assert!(style_src.contains("'self'"), "the fallback carries over: {style_src}");
  std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn a_policy_that_already_allows_inline_styles_gains_no_hash() {
  let root = project("default-src = [\"'self'\"]\nstyle-src = [\"'self'\", \"'unsafe-inline'\"]");
  let (_, policy) = served(&root);
  assert!(!policy.contains("'sha256-") || !policy.split(';').any(|d| d.trim().starts_with("style-src ") && d.contains("'sha256-")), "a hash would turn 'unsafe-inline' off: {policy}");
  std::fs::remove_dir_all(&root).unwrap();
}
