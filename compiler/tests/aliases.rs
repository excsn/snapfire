use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::fs;

mod common;
use common::{Fixture, get_snapfirec_cmd};

#[test]
fn a_paths_alias_is_rewritten_to_the_relative_output_path() {
  let fixture = Fixture::new("aliases");
  let mut cmd = get_snapfirec_cmd();
  cmd.arg("--root").arg(fixture.root()).assert().success();

  let home = fs::read_to_string(fixture.root().join("dist/pages/home.js")).unwrap();
  assert!(home.contains("from \"../src/util/greet.js\""), "{home}");
  assert!(home.contains("from \"../src/lib/index.js\""), "an exact alias names a file: {home}");
  assert!(home.contains("from \"../src/lib/sibling.js\""), "{home}");
  assert!(home.contains("from \"date-fns\""), "a bare specifier no alias matches stays an external: {home}");
  assert!(!home.contains("@src") && !home.contains("@lib"), "{home}");

  let sibling = fs::read_to_string(fixture.root().join("dist/src/lib/sibling.js")).unwrap();
  assert!(sibling.contains("from \"./index.js\""), "a target beside the importer gets its `./`: {sibling}");
}

#[test]
fn an_alias_that_names_nothing_is_a_dangling_import() {
  let fixture = Fixture::new("aliases");
  fs::write(fixture.root().join("pages/broken.ts"), "import { x } from \"@src/nope\";\nexport const y = x;\n").unwrap();
  let mut cmd = get_snapfirec_cmd();
  cmd
    .arg("--root")
    .arg(fixture.root())
    .assert()
    .failure()
    .stderr(predicate::str::contains("'../src/nope.js', which resolves to nothing"));
}

#[test]
fn an_alias_is_not_an_external_the_import_map_must_cover() {
  let fixture = Fixture::new("aliases");
  fs::write(fixture.root().join("importmap.json"), "{ \"imports\": { \"date-fns\": \"/vendor/date-fns.js\" } }\n").unwrap();
  let mut cmd = get_snapfirec_cmd();
  cmd.arg("--root").arg(fixture.root()).arg("--import-map").arg("importmap.json").assert().success();
}
