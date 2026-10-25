//! `fsr test` runs an app's specs and then the Rust tests of the crate beside it.

use std::process::{Command, Output};

use snapfire_fsr_test_internal::Scratch;

const SPEC: &str = "import { expect, test } from \"@snapfire/fsr/testing\";\n\ntest(\"the spec runs\", () => {\n  expect(1 + 1).toEqual(2);\n});\n";

fn project(tag: &str, rust_test: Option<&str>) -> Scratch {
  let root = Scratch::new(tag);
  root.write("config/app.toml", "[server]\nlisten = \"127.0.0.1:0\"\n\n[session]\nkey = \"test-command-key\"\nttl = \"1h\"\n");
  root.write("app/importmap.json", r#"{"imports":{"@snapfire/fsr/client":"/static/js/fsr/index.js"}}"#);
  root.write("app/routes/page.tsx", "export default function Page() {\n  return <p>page</p>;\n}\n");
  root.write("app/tests/page.test.ts", SPEC);
  if let Some(body) = rust_test {
    root.write("Cargo.toml", "[package]\nname = \"beside\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n");
    root.write("src/lib.rs", "");
    root.write("tests/arith.rs", body);
  }
  root
}

fn fsr_test(root: &Scratch, args: &[&str]) -> (Output, String) {
  let output = Command::new(env!("CARGO_BIN_EXE_fsr"))
    .arg("test")
    .arg(root.join("app"))
    .arg("--no-typecheck")
    .args(args)
    .env("CARGO_TARGET_DIR", root.join("target"))
    .output()
    .unwrap();
  let printed = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
  (output, printed)
}

const PASSING: &str = "#[test]\nfn adds_up() {\n  assert_eq!(2 + 2, 4);\n}\n";
const FAILING: &str = "#[test]\nfn adds_up() {\n  assert_eq!(2 + 2, 4);\n}\n\n#[test]\nfn adds_wrong() {\n  assert_eq!(2 + 2, 5);\n}\n";

#[test]
fn the_crate_beside_the_app_has_its_tests_run_after_the_specs() {
  let root = project("test-command-pass", Some(PASSING));
  let (output, printed) = fsr_test(&root, &[]);
  assert!(output.status.success(), "{printed}");
  assert!(printed.contains("the spec runs ... ok"), "{printed}");
  assert!(printed.contains("adds_up"), "the Rust test ran: {printed}");
  let specs = printed.find("the spec runs").unwrap();
  assert!(printed.find("adds_up").unwrap() > specs, "the specs come first: {printed}");
}

#[test]
fn a_failing_rust_test_fails_the_run_and_a_filter_reaches_both_halves() {
  let root = project("test-command-fail", Some(FAILING));
  let (output, printed) = fsr_test(&root, &[]);
  assert_eq!(output.status.code(), Some(1), "{printed}");
  assert!(printed.contains("adds_wrong"), "{printed}");

  let (output, printed) = fsr_test(&root, &["adds_up"]);
  assert!(output.status.success(), "the filter leaves the failing test out: {printed}");
  assert!(!printed.contains("adds_wrong"), "{printed}");
}

#[test]
fn specs_only_and_rust_only_run_one_half() {
  let root = project("test-command-halves", Some(FAILING));
  let (output, printed) = fsr_test(&root, &["--specs-only"]);
  assert!(output.status.success(), "{printed}");
  assert!(printed.contains("the spec runs ... ok") && !printed.contains("adds_"), "{printed}");

  let (output, printed) = fsr_test(&root, &["--rust-only"]);
  assert_eq!(output.status.code(), Some(1), "{printed}");
  assert!(!printed.contains("the spec runs") && printed.contains("adds_wrong"), "{printed}");
}

#[test]
fn an_app_with_no_crate_beside_it_runs_its_specs_and_refuses_rust_only() {
  let root = project("test-command-none", None);
  let (output, printed) = fsr_test(&root, &[]);
  assert!(output.status.success(), "{printed}");
  assert!(printed.contains("the spec runs ... ok"), "{printed}");

  let (output, printed) = fsr_test(&root, &["--rust-only"]);
  assert_eq!(output.status.code(), Some(2), "{printed}");
  assert!(printed.contains("no Cargo.toml"), "{printed}");
}

#[test]
fn a_spec_loads_the_desktop_module_and_its_calls_reject_outside_a_window() {
  let root = project("test-command-desktop", None);
  root.write("app/tests/desktop.spec.tsx", "import { expect, test } from \"@snapfire/fsr/client/testing\";\nimport { openExternal } from \"@snapfire/fsr/desktop\";\n\ntest(\"a desktop call outside a window\", async () => {\n  let message = \"\";\n  try {\n    await openExternal(\"https://example.com\");\n  } catch (e) {\n    message = String(e);\n  }\n  expect(message).toContain(\"fsr --desktop window\");\n});\n");
  let (output, printed) = fsr_test(&root, &[]);
  assert!(output.status.success(), "{printed}");
  assert!(printed.contains("a desktop call outside a window ... ok"), "{printed}");
}
