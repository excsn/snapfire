//! The wall and the about page rendered in-process: every image and font
//! path the example exists for, checked in the markup the host writes.

use std::path::Path;

use futures::executor::block_on;
use snapfire_fsr_host::{Config, Host, RenderMode};

fn gallery() -> Host {
  let config = Config::load(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
  Host::from_config(config).unwrap().build().unwrap()
}

fn render(host: &Host, path: &str) -> String {
  block_on(host.render_to_string(path, RenderMode::Html, snapfire_fsr_runtime::SessionCell::default())).unwrap()
}

#[test]
fn a_phone_photo_is_served_upright_in_every_size_from_the_asset_origin() {
  let host = gallery();
  let html = render(&host, "/");

  let lead = html.split("<picture>").find(|p| p.contains("class=\"lead\"")).expect("the lead photo is a picture");
  assert!(lead.contains("width=\"900\" height=\"1200\""), "stored 1200x900 with orientation 6, displayed portrait: {lead}");
  assert!(lead.contains("src=\"http://localhost:8200/static/js/app/src/img/harbour."), "the base is the asset origin: {lead}");
  assert!(lead.contains("480w") && lead.contains("900w"), "the policy widths below the photo's own, then its own: {lead}");
  assert!(lead.contains("fetchpriority=\"high\"") && lead.contains("loading=\"eager\""), "{lead}");
  assert!(html.contains("<link rel=\"preload\" as=\"image\""), "the lead photo is preloaded: {html}");

  assert_eq!(html.matches("<picture>").count(), 5, "the lead and four on the wall; the logo is not one: {html}");
  assert!(html.contains("<img src=\"http://localhost:8200/static/js/app/src/img/logo.") && html.contains(".svg\" width=\"120\" height=\"32\""), "an SVG is served as it is with its size: {html}");
}

#[test]
fn the_fonts_come_vendored_with_ranges_remote_from_a_provider_and_from_a_stylesheet() {
  let host = gallery();
  let html = render(&host, "/");

  assert!(html.contains("font-family:\"Fraunces\";font-style:normal;font-weight:100 900;"), "the provider served the variable face, one file per subset for every weight: {html}");
  assert!(html.contains("unicode-range:U+0000-00FF"), "a vendored subset carries the range its sidecar holds: {html}");
  assert!(html.contains("src:url(http://localhost:8200/static/js/app/fonts/Fraunces-latin."), "{html}");
  assert!(html.contains("font-family: \"Fraunces Fallback\"; src: local(\"Georgia\"); size-adjust:"), "{html}");
  assert!(html.contains("--font-display:\"Fraunces\", \"Fraunces Fallback\", serif;"), "{html}");
  assert!(html.contains("<link rel=\"stylesheet\" href=\"https://fonts.googleapis.com/css2?family=Inter"), "the linked family: {html}");
  assert!(html.contains("rel=\"preconnect\" href=\"https://fonts.gstatic.com\""), "{html}");
  assert!(html.contains("--font-sans:\"Inter\", sans-serif;"), "a linked family has no fallback metrics: {html}");
  assert!(html.contains("<link rel=\"stylesheet\" href=\"/static/js/app/src/ui/wall.css\">"), "the compiled stylesheet under src is linked: {html}");

  let wall = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("app/dist/src/ui/wall.css")).unwrap();
  assert!(wall.contains("url(\"http://localhost:8200/static/js/app/src/type/Inter-Medium.") && wall.contains(".woff2\")"), "a font the stylesheet names outside the fonts directory is served under its hash from the base: {wall}");
  assert!(wall.contains("url(\"http://localhost:8200/static/js/app/src/img/paper.") && wall.contains(".png\")"), "{wall}");
  assert!(Path::new(env!("CARGO_MANIFEST_DIR")).join("app/dist/src/img").read_dir().unwrap().any(|e| e.unwrap().file_name().to_string_lossy().starts_with("paper.")), "the texture was placed under dist by the build");
  assert!(Path::new(env!("CARGO_MANIFEST_DIR")).join("app/dist/src/type").read_dir().unwrap().any(|e| e.unwrap().file_name().to_string_lossy().starts_with("Inter-Medium.")), "so was the caption face");
  assert!(Path::new(env!("CARGO_MANIFEST_DIR")).join("app/dist/src/img").read_dir().unwrap().any(|e| e.unwrap().file_name().to_string_lossy().starts_with("close.")), "and the close icon only the browser's module imports");
}

#[test]
fn the_about_page_takes_an_avatar_from_a_remote_source_keeps_a_raw_img_and_rewrites_a_plain_one() {
  let host = gallery();
  let html = render(&host, "/about");

  assert!(html.contains("https://picsum.photos/id/1027/480/480 480w"), "the template filled per policy width: {html}");
  let avatar = html.split("<picture>").find(|p| p.contains("class=\"avatar\"")).expect("the avatar is a picture");
  assert!(avatar.contains("src=\"https://picsum.photos/id/1027/"), "the fallback src is the service too: {avatar}");
  assert!(avatar.contains("width=\"96\"") && avatar.contains("height=\"96\""), "the size is the author's, since there is no file to read: {avatar}");
  assert!(html.contains("<img src=\"http://localhost:8200/static/js/app/src/img/badge.") && html.contains("width=\"20\" height=\"20\" alt=\"\" class=\"badge\""), "data-sf-raw keeps the img plain and drops the attribute: {html}");
  assert!(!html.contains("data-sf-raw"), "{html}");
  let plain = html.split("<picture>").find(|p| p.contains("ridge.")).expect("a plain img of an imported photo is rewritten");
  assert!(plain.contains("class=\"plain\"") && plain.contains("width=\"900\" height=\"1200\""), "{plain}");
}
