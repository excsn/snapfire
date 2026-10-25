//! `@snapfire/fsr/config`: the deployment's `[public]` values read by a module anywhere in the application, not only by a loader through `ctx.config`.

use std::path::{Path, PathBuf};

use bytes::Bytes;
use http::Request;
use http_body_util::BodyExt;
use snapfire_fsr_cli::new::{create, NewOptions};
use snapfire_fsr_cli::typecheck::{self, Typecheck};
use snapfire_fsr_cli::{build, write, Options};
use snapfire_fsr_host::config::Config;
use snapfire_fsr_host::Host;

fn project(tag: &str, files: &[(&str, &str)]) -> PathBuf {
  let root = std::env::temp_dir().join(format!("fsr-cli-public-config-{}-{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&root);
  create(&root, NewOptions::default().fetch(false)).unwrap();
  let toml = root.join("config/app.toml");
  let text = std::fs::read_to_string(&toml).unwrap();
  std::fs::write(&toml, format!("{text}\n[public]\nenvironment = \"production\"\nsite_name = \"Gift <Cards>\"\n")).unwrap();
  for (path, text) in files {
    let file = root.join("app").join(path);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
  }
  root
}

fn document(root: &Path, path: &str) -> String {
  let app = root.join("app");
  let built = build(&app, &Options::beside(&app)).unwrap_or_else(|e| panic!("builds: {e}"));
  write(&app, &built).unwrap();
  let host = Host::from_config(Config::load(root).unwrap()).unwrap().build().unwrap_or_else(|e| panic!("boots: {e}"));
  let runtime = tokio::runtime::Runtime::new().unwrap();
  runtime.block_on(async {
    let response = host.handle(Request::get(path).body(Bytes::new()).unwrap()).await;
    let body = response.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&body).into_owned()
  })
}

const IDS: &str = "import { environment } from \"@snapfire/fsr/config\";\n\nexport const appId = { staging: \"111\", production: \"222\" }[environment];\n";

const PAGE: &str = "import { config } from \"@snapfire/fsr/config\";\nimport { appId } from \"@src/ids\";\n\nexport default function Page() {\n  return <main><p>{config.site_name}</p><p>{appId}</p></main>;\n}\n";

#[test]
fn a_module_reads_public_values_through_the_config_module() {
  let root = project("read", &[("src/ids.ts", IDS), ("routes/page.tsx", PAGE)]);
  let html = document(&root, "/");
  assert!(html.contains("<p>Gift &lt;Cards&gt;</p><p>222</p>"), "a page and a module-level constant read the deployment's values: {html}");
  assert!(html.contains("<script type=\"application/json\" id=\"sf-config\">"), "the browser's module reads them from the document: {html}");
  assert!(!html.contains("Gift <Cards>"), "the values cannot close the script: {html}");
  let module = std::fs::read_to_string(root.join("app/generated/config.ts")).unwrap();
  assert!(module.contains("export const environment = config.environment;") && module.contains("export const site_name = config.site_name;"), "{module}");
  std::fs::remove_dir_all(&root).unwrap();
}

/// Needs `snapfiretc` on PATH, as `fsr build` does.
#[test]
fn an_app_with_public_values_typechecks() {
  let root = project("typed", &[("src/ids.ts", IDS), ("routes/page.tsx", PAGE)]);
  let app = root.join("app");
  let built = build(&app, &Options::beside(&app)).unwrap_or_else(|e| panic!("builds: {e}"));
  write(&app, &built).unwrap();
  let checked = typecheck::run(&app, &Typecheck::default().enabled(true)).unwrap().expect("snapfiretc is on PATH");
  let errors: Vec<String> = checked.diagnostics.iter().map(|d| d.to_string()).collect();
  assert_eq!(checked.errors(), 0, "{errors:#?}");
  std::fs::remove_dir_all(&root).unwrap();
}
