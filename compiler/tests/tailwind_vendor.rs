//! Classes a Tailwind stylesheet takes from outside the sources: the vendored packages the
//! sources import, and the class list a package build writes for them.

use std::fs;

mod common;
use common::{Fixture, get_snapfirec_cmd, run_snapfirec};

#[test]
fn test_a_vendored_packages_class_list_wins_and_a_package_without_one_is_read() {
  let fixture = Fixture::new("tailwind-vendor");
  let mut cmd = get_snapfirec_cmd();
  let assert = run_snapfirec(
    cmd
      .arg("--root")
      .arg(fixture.root())
      .args(["--import-map", "importmap.json", "--tailwind-vendor", "vendor", "--tailwind-vendor-base", "/vendor"]),
  );
  let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
  let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
  let css = fs::read_to_string(fixture.root().join("dist/styles/app.css")).unwrap();
  for listed in [".ring-inset", ".ring-1", ".bg-violet-100"] {
    assert!(css.contains(listed), "acme-ui's class list: {listed}: {css}");
  }
  assert!(!css.contains(".underline"), "a package with a list is not parsed: {css}");
  for read in [".tabular-nums", ".bg-sky-200", ".h-2", ".block"] {
    assert!(css.contains(read), "acme-charts read from its minified modules, its relative import followed: {read}: {css}");
  }
  assert!(!css.contains(".font-black"), "a class composed through a helper the bundle inlined is not seen: {css}");
  assert!(!stdout.contains("acme-chart-frame") && !stderr.contains("acme-chart-frame"), "a vendored string that is no utility is no warning: {stdout}{stderr}");
}

#[test]
fn test_a_package_build_writes_its_class_list() {
  let fixture = Fixture::new("tailwind-package");
  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()).arg("--emit-tailwind-classes"));
  let listed = fs::read_to_string(fixture.root().join("dist/tailwind.classes")).unwrap();
  for class in ["rounded-full", "px-2", "bg-violet-100", "text-violet-800", "bg-gray-100"] {
    assert!(listed.lines().any(|line| line == format!("{class}\tsrc/index.tsx")), "{class}: {listed}");
  }
  let facts = fs::read_to_string(fixture.root().join("dist/.snapfire-build.json")).unwrap();
  assert!(facts.contains("\"tailwind.classes\""), "the list is one of the build's outputs: {facts}");
}
