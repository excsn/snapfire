use std::path::PathBuf;

use snapfire_fsr_cli::new::{create, NewOptions};
use snapfire_fsr_cli::{build, Options};

static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn root(tag: &str) -> PathBuf {
  let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let dir = std::env::temp_dir().join(format!("fsr-cli-new-{}-{n}-{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  dir
}

fn offline() -> NewOptions {
  NewOptions { fetch: false }
}

#[test]
fn a_scaffolded_project_builds_with_every_module_lowered() {
  let root = root("builds");
  let created = create(&root, offline()).unwrap();
  assert!(created.written.iter().any(|p| p.ends_with("config/app.toml")), "{:?}", created.written);
  assert!(created.written.iter().any(|p| p.ends_with("app/src/main.ts")), "{:?}", created.written);

  let built = build(&root.join("app"), &Options::beside(&root.join("app"))).unwrap();
  assert_eq!(built.report.routes.len(), 1, "{}", built.report);
  assert_eq!(built.report.sources, vec![("index".to_owned(), "routes/index/page.loader.ts".to_owned())], "{}", built.report);
  let residue: Vec<&(String, String, String)> = built.report.components.iter().filter(|(_, how, _)| how != "lowered").collect();
  assert!(residue.is_empty(), "every module lowers, but {residue:?}\n{}", built.report);
}

#[test]
fn the_scaffold_names_the_react_it_did_not_fetch() {
  let created = create(&root("offline"), offline()).unwrap();
  assert!(created.vendored.is_empty());
  assert!(created.next.iter().any(|s| s.contains("fsr add") && s.contains("react-dom@18.3.1/client")), "{:?}", created.next);
  assert!(created.next.last().unwrap().starts_with("fsr dev"), "{:?}", created.next);
}

#[test]
fn a_second_run_refuses_rather_than_overwriting() {
  let root = root("twice");
  create(&root, offline()).unwrap();
  let again = create(&root, offline()).unwrap_err().to_string();
  assert!(again.contains("already exists"), "{again}");
}

#[test]
fn a_body_calls_the_head_helpers_before_anything_is_generated() {
  let root = root("head");
  create(&root, offline()).unwrap();
  let app = root.join("app");
  std::fs::write(
    app.join("routes/index/page.loader.ts"),
    "import type { Ctx } from \"@snapfire/fsr\";\nimport { canonical, og } from \"@snapfire/fsr/head\";\n\nexport async function load(_ctx: Ctx<\"/\">) {\n  return { greeting: \"hi\" };\n}\n\nexport const meta = ({ data }: { data: { greeting: string } }) => ({\n  title: data.greeting,\n  head: [og(\"title\", data.greeting), canonical(\"/\")],\n});\n",
  )
  .unwrap();
  assert!(!app.join("generated").exists(), "nothing is generated yet");

  let built = build(&app, &Options::beside(&app)).unwrap();
  assert_eq!(built.report.sources, vec![("index".to_owned(), "routes/index/page.loader.ts".to_owned())], "{}", built.report);
}
