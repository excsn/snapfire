//! Which builds of the CLI link the JavaScript engine `fsr test` runs specs in.

use std::process::Command;

/// Whether `snapfire_fsr_engine` is a normal dependency of the CLI built with `features` and no defaults.
fn links_the_engine(features: &[&str]) -> bool {
  let mut tree = Command::new(env!("CARGO"));
  tree.args(["tree", "--offline", "-p", "snapfire_fsr_cli", "-e", "normal", "-i", "snapfire_fsr_engine", "--no-default-features"]);
  if !features.is_empty() {
    tree.args(["--features", &features.join(",")]);
  }
  let out = tree.current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).output().expect("cargo tree");
  let stdout = String::from_utf8_lossy(&out.stdout);
  let stderr = String::from_utf8_lossy(&out.stderr);
  match out.status.success() {
    true => stdout.contains("snapfire_fsr_engine"),
    false if stderr.contains("did not match any packages") => false,
    false => panic!("cargo tree {features:?}: {stderr}"),
  }
}

#[test]
fn a_serving_build_with_every_web_command_links_no_javascript_engine() {
  assert!(!links_the_engine(&["web", "tera", "mdx-highlight", "mdx-frontmatter"]));
  assert!(!links_the_engine(&["web", "desktop", "tls"]), "fsr serve --desktop needs no test engine either");
}

#[test]
fn the_default_build_and_the_test_feature_carry_fsr_test() {
  assert!(links_the_engine(&["test"]));
  let default = Command::new(env!("CARGO"))
    .args(["tree", "--offline", "-p", "snapfire_fsr_cli", "-e", "normal", "-i", "snapfire_fsr_engine"])
    .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
    .output()
    .expect("cargo tree");
  assert!(String::from_utf8_lossy(&default.stdout).contains("snapfire_fsr_engine"), "{}", String::from_utf8_lossy(&default.stderr));
}
