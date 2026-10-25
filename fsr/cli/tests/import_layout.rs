//! A file the browser loads never imports `@snapfire/fsr/host`: the build names the file, the line and where the value belongs.

use snapfire_fsr_cli::new::{create, NewOptions};
use snapfire_fsr_cli::{build, Options};
use snapfire_fsr_test_internal::Scratch;

fn project(tag: &str, page: &str) -> Scratch {
  let root = Scratch::new(tag);
  create(root.path(), NewOptions::default().fetch(false).with(vec!["react".to_owned()])).unwrap();
  root.write("app/importmap.json", r#"{"imports":{"@snapfire/fsr/client/react":"/r","react":"/r","react-dom/client":"/d"}}"#);
  root.write("app/vendor/.fsr-vendor.json", r#"{"packages":{"react":{"version":"19.3.1"},"react-dom":{"version":"19.3.1"}}}"#);
  root.write("app/routes/page.tsx", page);
  root
}

#[test]
fn a_handler_the_browser_runs_reading_the_host_side_stops_the_build() {
  let root = project(
    "host-in-handler",
    "import { useState } from \"react\";\nimport { id } from \"@snapfire/fsr/host\";\n\nexport default function Page() {\n  const [at, setAt] = useState(\"\");\n  return <button onClick={() => setAt(id.new())}>{at}</button>;\n}\n",
  );
  let app = root.join("app");
  let error = match build(&app, &Options::beside(&app)) {
    Err(e) => e.to_string(),
    Ok(built) => panic!("built: {}", built.report),
  };
  assert!(error.contains("@snapfire/fsr/host") && error.contains("routes/page") && error.contains("page.loader.ts"), "{error}");
}

/// `fsr test` resolves `file` as the host does: against the application, under the configured roots.
#[test]
fn a_body_test_reads_a_file_under_the_configured_roots_as_the_host_does() {
  let root = Scratch::new("files-body-test");
  root.write("config/app.toml", "[server]\nlisten = \"127.0.0.1:0\"\n\n[session]\nkey = \"files-test-key\"\nttl = \"1h\"\n\n[files]\nroots = [\"content\"]\n");
  root.write("app/importmap.json", r#"{"imports":{"@snapfire/fsr/client":"/static/js/fsr/index.js"}}"#);
  root.write("app/content/a.md", "# A\n");
  root.write("app/routes/page.tsx", "export default function Page() {\n  return <p>page</p>;\n}\n");
  root.write("app/routes/page.loader.ts", "import { file } from \"@snapfire/fsr/host\";\n\nexport async function load() {\n  return { text: file.read(\"content/a.md\") };\n}\n");
  root.write("app/tests/page.test.ts", "import { load } from \"@routes/page.loader\";\nimport { ctx, expect, test } from \"@snapfire/fsr/testing\";\n\ntest(\"the loader reads the content root\", async () => {\n  const c = ctx({});\n  const data = await load(c);\n  expect(data.text).toEqual(\"# A\\n\");\n});\n");
  let app = root.join("app");
  let summary = snapfire_fsr_cli::test::run(&app, &Options::beside(&app), &snapfire_fsr_cli::typecheck::Typecheck::default(), None).unwrap();
  assert_eq!(summary.failed, 0, "{summary}");
  assert_eq!(summary.passed, 1, "{summary}");
}
