use std::fs;
use std::path::Path;

mod common;
use common::{Fixture, get_snapfirec_cmd, run_snapfirec};

fn emitted(dist: &Path, dir: &str, stem: &str, ext: &str) -> String {
  let mut found: Vec<String> = fs::read_dir(dist.join(dir))
    .unwrap()
    .filter_map(|e| e.ok())
    .map(|e| e.file_name().to_string_lossy().into_owned())
    .filter(|n| n.starts_with(&format!("{stem}.")) && n.ends_with(&format!(".{ext}")) && n.len() > stem.len() + ext.len() + 2)
    .collect();
  assert_eq!(found.len(), 1, "one hashed {stem}.{ext} under {dir}, found {found:?}");
  found.pop().unwrap()
}

#[test]
fn an_imported_image_becomes_a_const_with_its_url_and_dimensions() {
  let fixture = Fixture::new("asset-urls");
  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()).args(["--public-path", "/static/js/app/"]));

  let dist = fixture.root().join("dist");
  let hero = emitted(&dist, "img", "hero", "png");
  let inter = emitted(&dist, "fonts", "inter", "woff2");
  let module = fs::read_to_string(dist.join("ui/card.js")).unwrap();

  assert!(!module.contains("hero.png\""), "the image import survived: {module}");
  assert!(module.contains(&format!("src: \"/static/js/app/img/{hero}\"")), "{module}");
  assert!(module.contains("width: 6"), "{module}");
  assert!(module.contains("height: 4"), "{module}");
  assert!(module.contains(&format!("const inter = \"/static/js/app/fonts/{inter}\"")), "{module}");
  assert!(module.contains("from \"./card.json\""), "a JSON import is still an import: {module}");
  assert!(!dist.join("img/hero.png").exists(), "the unhashed copy was written too");
  assert_eq!(fs::read(dist.join("img").join(&hero)).unwrap(), fs::read(fixture.root().join("input/img/hero.png")).unwrap());
}

#[test]
fn without_a_public_path_the_url_is_resolved_against_the_module() {
  let fixture = Fixture::new("asset-urls");
  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));

  let dist = fixture.root().join("dist");
  let hero = emitted(&dist, "img", "hero", "png");
  let module = fs::read_to_string(dist.join("ui/card.js")).unwrap();
  assert!(module.contains(&format!("new URL(\"../img/{hero}\", import.meta.url).href")), "{module}");
}

#[test]
fn a_stylesheets_relative_urls_point_at_the_hashed_files_and_the_rest_stay_as_written() {
  let fixture = Fixture::new("asset-urls");
  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()).arg("--minify"));

  let dist = fixture.root().join("dist");
  let hero = emitted(&dist, "img", "hero", "png");
  let inter = emitted(&dist, "fonts", "inter", "woff2");

  for name in ["theme.css", "theme.min.css"] {
    let css = fs::read_to_string(dist.join(name)).unwrap();
    assert!(css.contains(&format!("./fonts/{inter}#iefix")), "{name}: {css}");
    assert!(!css.contains("?v=2"), "{name} kept the query: {css}");
    assert!(css.contains(&format!("./img/{hero}")), "{name}: {css}");
    assert!(css.contains("https://cdn.example.com/x.png"), "{name}: {css}");
    assert!(css.contains("data:image/gif;base64,R0lGODlhAQABAAAAACw="), "{name}: {css}");
    assert!(css.contains("./img/absent.png"), "{name} lost a reference it could not emit: {css}");
    assert!(css.contains("./card.cur"), "{name} touched a reference of another kind: {css}");
    assert!(!css.contains("placeholder"), "{name}: {css}");
  }
}

#[test]
fn the_build_facts_list_every_asset_with_its_header() {
  let fixture = Fixture::new("asset-urls");
  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()).args(["--public-path", "/static/js/app"]));

  let dist = fixture.root().join("dist");
  let hero = emitted(&dist, "img", "hero", "png");
  let inter = emitted(&dist, "fonts", "inter", "woff2");
  let facts: serde_json::Value = serde_json::from_str(&fs::read_to_string(dist.join(".snapfire-build.json")).unwrap()).unwrap();

  assert_eq!(facts["version"], 2);
  let assets = facts["assets"].as_array().unwrap();
  assert_eq!(assets.len(), 2, "{assets:?}");
  let font = &assets[0];
  assert_eq!(font["source"], "fonts/inter.woff2");
  assert_eq!(font["path"], format!("fonts/{inter}"));
  assert_eq!(font["url"], format!("/static/js/app/fonts/{inter}"));
  assert_eq!(font["kind"], "font");
  assert!(font.get("width").is_none());
  let image = &assets[1];
  assert_eq!(image["source"], "img/hero.png");
  assert_eq!(image["path"], format!("img/{hero}"));
  assert_eq!(image["kind"], "image");
  assert_eq!(image["width"], 6);
  assert_eq!(image["height"], 4);
  assert_eq!(image["hash"].as_str().unwrap().len(), 8);
  assert!(hero.contains(image["hash"].as_str().unwrap()));

  let outputs: Vec<&str> = facts["outputs"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
  assert!(outputs.contains(&format!("img/{hero}").as_str()), "{outputs:?}");
  assert!(outputs.contains(&format!("fonts/{inter}").as_str()), "{outputs:?}");
}

#[test]
fn a_changed_image_gets_a_new_name_and_the_old_one_is_pruned() {
  let fixture = Fixture::new("asset-urls");
  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));
  let dist = fixture.root().join("dist");
  let before = emitted(&dist, "img", "hero", "png");

  let source = fixture.root().join("input/img/hero.png");
  let mut bytes = fs::read(&source).unwrap();
  bytes.push(0);
  fs::write(&source, bytes).unwrap();

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));
  let after = emitted(&dist, "img", "hero", "png");
  assert_ne!(before, after);
  assert!(!dist.join("img").join(&before).exists(), "the stale variant survived the rebuild");
  let module = fs::read_to_string(dist.join("ui/card.js")).unwrap();
  assert!(module.contains(&after), "{module}");
}
