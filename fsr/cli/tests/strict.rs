//! `[build] strict`: a component that would render in the browser alone stops
//! the build, naming the cause and the component it leaves there.

use std::path::PathBuf;

use snapfire_fsr_cli::new::{create, NewOptions};
use snapfire_fsr_cli::{build, BuildError, Options};

static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

const PAGE: &str = "import { Island } from \"@snapfire/fsr-client/react\";\nimport { Clock } from \"../src/Clock\";\nexport default function Page() {\n  return <main><p>page</p><Island><Clock /></Island></main>;\n}\n";
const CLOCK: &str = "import { useTransition } from \"react\";\nexport function Clock() {\n  const transition = useTransition();\n  return <time>{transition[0] ? \"…\" : \"now\"}</time>;\n}\n";

/// A React scaffold whose page places a component that does not lower, with `toml` appended to its configuration.
fn project(tag: &str, toml: &str) -> PathBuf {
  let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let root = std::env::temp_dir().join(format!("fsr-cli-strict-{}-{n}-{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&root);
  create(&root, NewOptions { fetch: false, with: vec!["react".to_owned()], ..NewOptions::default() }).unwrap();
  let app = root.join("app");
  std::fs::write(app.join("importmap.json"), r#"{"imports":{"@snapfire/fsr-client/react":"/r","react":"/r","react-dom/client":"/d"}}"#).unwrap();
  std::fs::create_dir_all(app.join("vendor")).unwrap();
  std::fs::write(app.join("vendor/.fsr-vendor.json"), r#"{"packages":{"react":{"version":"19.1.0"},"react-dom":{"version":"19.1.0"}}}"#).unwrap();
  std::fs::write(app.join("routes/page.tsx"), PAGE).unwrap();
  std::fs::create_dir_all(app.join("src")).unwrap();
  std::fs::write(app.join("src/Clock.tsx"), CLOCK).unwrap();
  let config = root.join("config/app.toml");
  let mut text = std::fs::read_to_string(&config).unwrap();
  text.push_str(toml);
  std::fs::write(&config, text).unwrap();
  root
}

#[test]
fn a_component_that_renders_in_the_browser_alone_builds_unless_the_build_is_strict() {
  let root = project("option", "");
  let app = root.join("app");
  let options = Options::beside(&app);
  assert!(!options.strict, "strict is opt-in");
  let built = build(&app, &options).unwrap();
  assert!(built.report.components.iter().any(|(module, kind, _)| module == "src/Clock.tsx#Clock" && kind == "client"), "{}", built.report);

  let err = match build(&app, &Options { strict: true, ..options }) {
    Err(BuildError::Strict(refused)) => refused,
    other => panic!("not refused: {:?}", other.map(|built| built.report.to_string())),
  };
  assert!(err.contains("client   src/Clock.tsx:3:"), "{err}");
  assert!(err.contains("`useTransition`"), "{err}");
  assert!(err.contains("src/Clock.tsx#Clock"), "{err}");
  std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn the_configuration_turns_strict_on() {
  let root = project("config", "\n[build]\nstrict = true\n");
  let app = root.join("app");
  let options = Options::beside(&app);
  assert!(options.strict);
  let err = build(&app, &options).err().expect("refused").to_string();
  assert!(err.starts_with("strict: these components render in the browser alone"), "{err}");
  std::fs::remove_dir_all(&root).unwrap();
}
