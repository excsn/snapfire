//! A file the browser loads never imports `@snapfire/fsr/host`: the build names the file, the line and where the value belongs.

use snapfire_fsr_cli::new::{create, NewOptions};
use snapfire_fsr_cli::{build, Options};
use snapfire_fsr_test_internal::Scratch;

fn project(tag: &str, page: &str) -> Scratch {
  let root = Scratch::new(tag);
  create(root.path(), NewOptions::default().fetch(false).with(vec!["react".to_owned()])).unwrap();
  root.write("app/importmap.json", r#"{"imports":{"@snapfire/fsr-client/react":"/r","react":"/r","react-dom/client":"/d"}}"#);
  root.write("app/vendor/.fsr-vendor.json", r#"{"packages":{"react":{"version":"19.3.1"},"react-dom":{"version":"19.3.1"}}}"#);
  root.write("app/routes/page.tsx", page);
  root
}

#[test]
fn a_handler_the_browser_runs_reading_the_host_side_stops_the_build() {
  let root = project(
    "host-in-handler",
    "import { useState } from \"react\";\nimport { time } from \"@snapfire/fsr/host\";\n\nexport default function Page() {\n  const [at, setAt] = useState(0);\n  return <button onClick={() => setAt(time.now())}>{at}</button>;\n}\n",
  );
  let app = root.join("app");
  let error = match build(&app, &Options::beside(&app)) {
    Err(e) => e.to_string(),
    Ok(built) => panic!("built: {}", built.report),
  };
  assert!(error.contains("@snapfire/fsr/host") && error.contains("routes/page") && error.contains("page.loader.ts"), "{error}");
}
