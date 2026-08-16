use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::fs;

mod common;
use common::{Fixture, get_snapfirec_cmd, run_snapfirec};

fn field(map: &str, key: &str) -> String {
  let start = map.find(&format!("\"{key}\"")).unwrap_or_else(|| panic!("{key} missing"));
  let rest = &map[start..];
  let end = rest.find(',').unwrap_or(rest.len());
  rest[..end].to_string()
}

#[test]
fn test_no_maps_without_asking() {
  let fixture = Fixture::new("computed-root");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));

  assert!(!predicate::path::exists().eval(&fixture.root().join("dist/button.js.map")));

  let content = fs::read_to_string(fixture.root().join("dist/button.js")).unwrap();
  assert!(!content.contains("sourceMappingURL"));
}

#[test]
fn test_external_maps_land_beside_the_output() {
  let fixture = Fixture::new("computed-root");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()).arg("--source-map"));

  let content = fs::read_to_string(fixture.root().join("dist/button.js")).unwrap();
  assert!(content.trim_end().ends_with("//# sourceMappingURL=button.js.map"));

  let map = fs::read_to_string(fixture.root().join("dist/button.js.map")).unwrap();
  assert!(map.contains("\"version\":3"));
}

#[test]
fn test_map_sources_point_back_at_the_original() {
  let fixture = Fixture::new("computed-root");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()).arg("--source-map"));

  let dist = fixture.root().join("dist");
  let map = fs::read_to_string(dist.join("button.js.map")).unwrap();

  assert!(field(&map, "sources").contains("../src/ui/button.ts"));
  assert!(predicate::path::exists().eval(&dist.join("../src/ui/button.ts")));
}

#[test]
fn test_inline_maps_are_embedded_as_data_uris() {
  let fixture = Fixture::new("computed-root");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()).arg("--inline-source-map"));

  let content = fs::read_to_string(fixture.root().join("dist/button.js")).unwrap();
  assert!(content.contains("//# sourceMappingURL=data:application/json;base64,"));
  assert!(!predicate::path::exists().eval(&fixture.root().join("dist/button.js.map")));
}

#[test]
fn test_css_gets_a_map_with_css_comment_syntax() {
  let fixture = Fixture::new("source-maps");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));

  let content = fs::read_to_string(fixture.root().join("dist/style.css")).unwrap();
  assert!(content.contains("/*# sourceMappingURL=style.css.map */"));

  let map = fs::read_to_string(fixture.root().join("dist/style.css.map")).unwrap();
  assert!(map.contains("\"version\":3"));
}

#[test]
fn test_tsconfig_source_map_key_is_honoured() {
  let fixture = Fixture::new("source-maps");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));

  assert!(predicate::path::exists().eval(&fixture.root().join("dist/index.js.map")));
}

#[test]
fn test_inline_sources_embeds_the_original_text() {
  let fixture = Fixture::new("source-maps");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));

  let map = fs::read_to_string(fixture.root().join("dist/index.js.map")).unwrap();
  assert!(map.contains("name: string"), "the pre-strip source should be embedded");
}

#[test]
fn test_setting_both_map_modes_fails() {
  let fixture = Fixture::new("source-map-conflict");

  let mut cmd = get_snapfirec_cmd();
  cmd
    .arg("--root")
    .arg(fixture.root())
    .assert()
    .failure()
    .stderr(predicate::str::contains(
      "'sourceMap' and 'inlineSourceMap' cannot both be set",
    ));
}
